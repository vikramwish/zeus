use serde::Serialize;
use sha2::{Digest, Sha256};
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

const MAX_FILE: u64 = 16 * 1024 * 1024;
const MAX_TOTAL: u64 = 64 * 1024 * 1024;
const MAX_TEXT: usize = 128 * 1024;
const MAX_FILES: usize = 2000;
/// Directory nesting bound for the snapshot walker, which recurses per level.
const MAX_DEPTH: usize = 128;
static TEMP_ID: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, Serialize)]
pub struct Entry {
    pub path: String,
    pub bytes: u64,
    pub sha256: String,
    pub kind: String,
}
#[derive(Debug, Clone, Serialize)]
pub struct Inventory {
    pub entries: Vec<Entry>,
    pub skipped: Vec<String>,
}
#[derive(Debug, Serialize)]
pub struct ReadResult {
    pub content: String,
    pub sha256: String,
}
#[derive(Debug)]
pub struct Workspace {
    pub root: PathBuf,
    pub inventory: Inventory,
    // Serialize version validation through rename, not model calls or reads.
    // Atomic rename alone permits two writers to accept the same old digest.
    mutation: Mutex<()>,
}

pub fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn excluded(name: &str) -> bool {
    matches!(
        name,
        ".git"
            | "node_modules"
            | "target"
            | ".venv"
            | "venv"
            | "__pycache__"
            | ".ssh"
            | ".aws"
            | ".hermes"
            | ".native-agent"
            | ".DS_Store"
            | "auth.json"
            | "credentials.json"
            | "id_rsa"
            | "id_ed25519"
            | "id_ecdsa"
            | "id_dsa"
            | ".netrc"
            | ".npmrc"
            | ".pypirc"
            | ".git-credentials"
            | ".docker"
            | ".kube"
            | ".gnupg"
            | ".terraform"
    ) || name == ".env"
        || name.starts_with(".env.")
        || name.ends_with(".pem")
        || name.ends_with(".key")
        || name.ends_with(".p12")
        || name.ends_with(".pfx")
        || name.ends_with(".jks")
        || name.ends_with(".keystore")
        || name.ends_with(".tfstate")
        || name.ends_with(".tfstate.backup")
}

/// Conservative content check for credentials in ordinary text files. Matches
/// well-known key shapes only; it is a guard against the common leak, not a
/// secret scanner. Returns the matched kind, never the matched value.
pub fn secret_like(text: &str) -> Option<&'static str> {
    fn run_len(bytes: &[u8], allowed: fn(u8) -> bool) -> usize {
        bytes.iter().take_while(|b| allowed(**b)).count()
    }
    fn token(b: u8) -> bool {
        b.is_ascii_alphanumeric() || b == b'_' || b == b'-'
    }
    if text.contains("PRIVATE KEY-----") {
        return Some("private key block");
    }
    let bytes = text.as_bytes();
    for (index, window) in bytes.windows(4).enumerate() {
        let rest = &bytes[index + 4..];
        let kind = match window {
            b"sk-a" | b"sk-p" | b"sk-o" | b"sk-l" => {
                (run_len(rest, token) >= 20).then_some("sk- API key")
            }
            b"AKIA" | b"ASIA" => (run_len(rest, |b| b.is_ascii_uppercase() || b.is_ascii_digit())
                == 16
                && rest.get(16).is_none_or(|b| !token(*b)))
            .then_some("AWS access key id"),
            b"ghp_" | b"gho_" | b"ghu_" | b"ghs_" | b"ghr_" => {
                (run_len(rest, |b| b.is_ascii_alphanumeric()) >= 36).then_some("GitHub token")
            }
            b"xoxb" | b"xoxp" | b"xoxa" | b"xoxr" | b"xoxs" => (rest.first() == Some(&b'-')
                && run_len(&rest[1..], token) >= 20)
                .then_some("Slack token"),
            b"AIza" => (run_len(rest, token) >= 35).then_some("Google API key"),
            _ => None,
        };
        if let Some(kind) = kind {
            return Some(kind);
        }
    }
    None
}
fn kind(bytes: &[u8], path: &Path) -> String {
    if bytes.starts_with(b"\x7fELF") {
        "elf"
    } else if bytes.starts_with(b"MZ") {
        "pe"
    } else if bytes.starts_with(b"\0asm") {
        "wasm"
    } else if bytes.starts_with(b"PK\x03\x04") {
        "zip-package"
    } else if [
        b"\xcf\xfa\xed\xfe".as_slice(),
        b"\xfe\xed\xfa\xcf".as_slice(),
        b"\xce\xfa\xed\xfe".as_slice(),
        b"\xfe\xed\xfa\xce".as_slice(),
        b"\xca\xfe\xba\xbe".as_slice(),
    ]
    .iter()
    .any(|magic| bytes.starts_with(magic))
    {
        "mach-o-or-fat"
    } else if bytes.starts_with(b"SQLite format 3\0") {
        "sqlite"
    } else if bytes.starts_with(b"\x89PNG") || bytes.starts_with(b"\xff\xd8\xff") {
        "image"
    } else if bytes.starts_with(b"\xd4\xc3\xb2\xa1") || bytes.starts_with(b"\x0a\x0d\x0d\x0a") {
        "packet-capture"
    } else if path.extension().is_some_and(|x| x == "asar") {
        "asar-candidate"
    } else if let Some(text) = std::str::from_utf8(bytes)
        .ok()
        .filter(|_| !bytes.contains(&0))
    {
        if secret_like(text).is_some() {
            "secret-like-text"
        } else {
            "text"
        }
    } else {
        "binary-unknown"
    }
    .into()
}
fn bounded_read(path: &Path, limit: u64) -> Result<Vec<u8>, String> {
    let meta = fs::symlink_metadata(path).map_err(|e| e.to_string())?;
    if !meta.is_file() || meta.file_type().is_symlink() {
        return Err("regular files only".into());
    }
    if meta.len() > limit {
        return Err(format!("file exceeds {limit} byte limit"));
    }
    let mut data = Vec::new();
    fs::File::open(path)
        .map_err(|e| e.to_string())?
        .take(limit + 1)
        .read_to_end(&mut data)
        .map_err(|e| e.to_string())?;
    if data.len() as u64 > limit {
        return Err("file grew beyond limit".into());
    }
    Ok(data)
}

#[cfg(unix)]
fn normal_permissions(path: &Path) -> std::io::Result<fs::Permissions> {
    use std::os::unix::fs::PermissionsExt;
    // Preserve rwx, never copy setuid, setgid, or sticky bits into the candidate.
    let mode = fs::symlink_metadata(path)?.permissions().mode() & 0o777;
    Ok(fs::Permissions::from_mode(mode))
}

pub fn inspect(source: &Path) -> Result<Inventory, String> {
    let meta = fs::symlink_metadata(source).map_err(|e| e.to_string())?;
    if meta.file_type().is_symlink() {
        return Err("target cannot be a symlink".into());
    }
    let mut inventory = Inventory {
        entries: vec![],
        skipped: vec![],
    };
    let mut total = 0;
    let base = if meta.is_file() {
        source.parent().ok_or("target has no parent")?
    } else {
        source
    };
    walk(source, base, &mut inventory, &mut total, 0)?;
    Ok(inventory)
}
fn walk(
    path: &Path,
    base: &Path,
    inv: &mut Inventory,
    total: &mut u64,
    depth: usize,
) -> Result<(), String> {
    if depth > MAX_DEPTH {
        return Err(format!(
            "directory depth limit exceeded ({MAX_DEPTH}); select a smaller target"
        ));
    }
    let name = path
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or("non-UTF8 filename")?;
    let relative = path
        .strip_prefix(base)
        .map_err(|e| e.to_string())?
        .to_str()
        .ok_or("non-UTF8 path")?
        .to_owned();
    if path != base && excluded(name) {
        inv.skipped.push(relative);
        return Ok(());
    }
    let meta = fs::symlink_metadata(path).map_err(|e| e.to_string())?;
    if meta.file_type().is_symlink() {
        inv.skipped.push(format!("{relative} (symlink)"));
        return Ok(());
    }
    if meta.is_dir() {
        let mut children = fs::read_dir(path)
            .map_err(|e| e.to_string())?
            .map(|e| e.map(|x| x.path()))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        if children.len() > 10000 {
            return Err("directory entry limit exceeded".into());
        }
        children.sort();
        for child in children {
            walk(&child, base, inv, total, depth + 1)?;
        }
    } else if meta.is_file() {
        if inv.entries.len() >= MAX_FILES {
            return Err("file-count limit exceeded".into());
        }
        if meta.len() > MAX_FILE || *total + meta.len() > MAX_TOTAL {
            return Err("snapshot byte limit exceeded; select a smaller target".into());
        }
        let data = bounded_read(path, MAX_FILE)?;
        *total += data.len() as u64;
        if *total > MAX_TOTAL {
            return Err("snapshot byte limit exceeded".into());
        }
        inv.entries.push(Entry {
            path: relative,
            bytes: data.len() as u64,
            sha256: digest(&data),
            kind: kind(&data, path),
        });
    } else {
        inv.skipped.push(format!("{relative} (not regular)"));
    }
    if inv.skipped.len() > MAX_FILES {
        return Err("too many skipped entries".into());
    }
    Ok(())
}

impl Workspace {
    pub fn snapshot(source: &Path, destination: &Path) -> Result<Self, String> {
        let inventory = inspect(source)?;
        let canonical_source = fs::canonicalize(source).map_err(|e| e.to_string())?;
        let parent = fs::canonicalize(destination.parent().ok_or("destination has no parent")?)
            .map_err(|e| e.to_string())?;
        if parent.starts_with(&canonical_source) {
            return Err("candidate must be outside source".into());
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            fs::DirBuilder::new()
                .mode(0o700)
                .create(destination)
                .map_err(|e| e.to_string())?;
        }
        #[cfg(not(unix))]
        fs::create_dir(destination).map_err(|e| e.to_string())?;
        let root = fs::canonicalize(destination).map_err(|e| e.to_string())?;
        let base = if canonical_source.is_file() {
            canonical_source.parent().ok_or("source parent missing")?
        } else {
            &canonical_source
        };
        for entry in &inventory.entries {
            let bytes = bounded_read(&base.join(&entry.path), MAX_FILE)?;
            if digest(&bytes) != entry.sha256 {
                return Err("source changed during snapshot".into());
            }
            let target = root.join(&entry.path);
            fs::create_dir_all(target.parent().ok_or("target parent missing")?)
                .map_err(|e| e.to_string())?;
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&target)
                .map_err(|e| e.to_string())?;
            file.write_all(&bytes).map_err(|e| e.to_string())?;
            #[cfg(unix)]
            file.set_permissions(
                normal_permissions(&base.join(&entry.path)).map_err(|e| e.to_string())?,
            )
            .map_err(|e| e.to_string())?;
        }
        Ok(Self {
            root,
            inventory,
            mutation: Mutex::new(()),
        })
    }
    fn resolve(&self, path: &str) -> Result<PathBuf, String> {
        if path.is_empty() || path.len() > 4096 {
            return Err("invalid path length".into());
        }
        let relative = Path::new(path);
        let mut target = self.root.clone();
        for part in relative.components() {
            let Component::Normal(name) = part else {
                return Err("only relative normal path components allowed".into());
            };
            let name_str = name.to_str().ok_or("non-UTF8 path")?;
            if excluded(name_str) {
                return Err("excluded or sensitive path".into());
            }
            target.push(name);
            match fs::symlink_metadata(&target) {
                Ok(m) if m.file_type().is_symlink() => return Err("symlinks forbidden".into()),
                Ok(_) => (),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
                Err(e) => return Err(e.to_string()),
            }
        }
        Ok(target)
    }
    pub fn read(&self, path: &str) -> Result<ReadResult, String> {
        let bytes = bounded_read(&self.resolve(path)?, MAX_TEXT as u64)?;
        let sha256 = digest(&bytes);
        let content = String::from_utf8(bytes)
            .map_err(|_| "binary file: use artifact analysis tools".to_string())?;
        if content.contains('\0') {
            return Err("binary file: use artifact analysis tools".into());
        }
        if let Some(kind) = secret_like(&content) {
            return Err(format!(
                "file contains secret-like content ({kind}); it stays in the candidate but is not shown to the model"
            ));
        }
        Ok(ReadResult { content, sha256 })
    }
    pub fn write(&self, path: &str, content: &str, expected: Option<&str>) -> Result<(), String> {
        if content.len() > MAX_TEXT {
            return Err("text size limit exceeded".into());
        }
        let _guard = self
            .mutation
            .lock()
            .map_err(|_| "workspace mutation lock poisoned")?;
        let target = self.resolve(path)?;
        if target.exists() {
            let current = bounded_read(&target, MAX_TEXT as u64)?;
            if Some(digest(&current).as_str()) != expected {
                return Err("stale or missing expected_sha256".into());
            }
        } else if expected.is_some() {
            return Err("expected existing file is missing".into());
        }
        #[cfg(unix)]
        let permissions = if target.exists() {
            Some(normal_permissions(&target).map_err(|e| e.to_string())?)
        } else {
            None
        };
        let parent = target.parent().ok_or("missing parent")?;
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        // Recheck after directory creation; all agent writes share the mutation lock.
        self.resolve(path)?;
        let temp = parent.join(format!(
            ".native-write-{}-{}",
            std::process::id(),
            TEMP_ID.fetch_add(1, Ordering::Relaxed)
        ));
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temp)
            .map_err(|e| e.to_string())?;
        let result = (|| {
            file.write_all(content.as_bytes())?;
            #[cfg(unix)]
            if let Some(permissions) = permissions {
                file.set_permissions(permissions)?;
            }
            file.sync_all()?;
            fs::rename(&temp, &target)
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temp);
        }
        result.map_err(|e| e.to_string())
    }
    /// Literal, non-overlapping occurrences in current candidate text (not the snapshot
    /// inventory). `truncated` means a search/result budget stopped traversal; skipped
    /// files and shortened UTF-8 excerpts are reported separately. Source IDs use the
    /// same path#sha256 convention as read_file. No filesystem mutation is performed.
    pub fn search(&self, query: &str, max_results: usize) -> Result<serde_json::Value, String> {
        const MAX_RESULTS: usize = 100;
        const MAX_ENTRIES: usize = 10_000;
        const MAX_EXCERPT: usize = 512;
        if query.is_empty() || query.len() > 4096 {
            return Err("query must contain 1..=4096 UTF-8 bytes".into());
        }
        if !(1..=MAX_RESULTS).contains(&max_results) {
            return Err("max_results must be in 1..=100".into());
        }
        let mut matches = Vec::new();
        let mut skipped = Vec::new();
        let mut truncation_reason = None;
        let mut pending = std::collections::BTreeSet::from([String::new()]);
        let mut entries = 0;
        let mut files = 0;
        let mut bytes = 0u64;
        'paths: while let Some(path) = pending.pop_first() {
            if skipped.len() == MAX_RESULTS {
                truncation_reason = Some("diagnostic limit");
                break;
            }
            let target = if path.is_empty() {
                self.root.clone()
            } else {
                match self.resolve(&path) {
                    Ok(target) => target,
                    Err(reason) => {
                        skipped.push(serde_json::json!({"path": path, "reason": reason}));
                        continue;
                    }
                }
            };
            let meta = fs::symlink_metadata(&target).map_err(|e| e.to_string())?;
            if meta.is_dir() {
                // Bound enumeration before collecting/sorting; empty directories also
                // consume budget. An oversized directory is not partially searched.
                let children = fs::read_dir(&target)
                    .map_err(|e| e.to_string())?
                    .take(MAX_ENTRIES - entries + 1)
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(|e| e.to_string())?;
                entries += children.len();
                if entries > MAX_ENTRIES {
                    truncation_reason = Some("directory entry limit");
                    break;
                }
                for child in children {
                    let name = child
                        .file_name()
                        .into_string()
                        .map_err(|_| "non-UTF8 path")?;
                    pending.insert(if path.is_empty() {
                        name
                    } else {
                        format!("{path}/{name}")
                    });
                }
                continue;
            }
            if !meta.is_file() || meta.file_type().is_symlink() {
                skipped.push(serde_json::json!({"path": path, "reason": "regular files only"}));
                continue;
            }
            if files == MAX_FILES || bytes + MAX_TEXT as u64 > MAX_TOTAL {
                truncation_reason = Some("file or byte limit");
                break;
            }
            files += 1;
            if meta.len() > MAX_TEXT as u64 {
                skipped.push(serde_json::json!({"path": path, "reason": "file exceeds 131072 byte text limit"}));
                continue;
            }
            let text = match self.read(&path) {
                Ok(text) => text,
                Err(reason) => {
                    // A failed bounded read may still have consumed its full allowance.
                    bytes += MAX_TEXT as u64;
                    skipped.push(serde_json::json!({"path": path, "reason": reason}));
                    continue;
                }
            };
            bytes += text.content.len() as u64;
            for (offset, _) in text.content.match_indices(query) {
                if matches.len() == max_results {
                    truncation_reason = Some("result limit");
                    break 'paths;
                }
                let line = text.content[..offset]
                    .bytes()
                    .filter(|b| *b == b'\n')
                    .count()
                    + 1;
                let line_start = text.content[..offset].rfind('\n').map_or(0, |i| i + 1);
                let line_end = text.content[offset..]
                    .find('\n')
                    .map_or(text.content.len(), |i| offset + i);
                // Center long-line excerpts near the match rather than silently hiding it.
                let mut start = line_start.max(offset.saturating_sub(MAX_EXCERPT / 4));
                while !text.content.is_char_boundary(start) {
                    start += 1;
                }
                let mut end = line_end.min(start + MAX_EXCERPT);
                while !text.content.is_char_boundary(end) {
                    end -= 1;
                }
                matches.push(serde_json::json!({
                    "path": path, "line": line, "byte_offset": offset,
                    "content": &text.content[start..end],
                    "content_truncated": start != line_start || end != line_end || offset + query.len() > end,
                    "sha256": text.sha256,
                    "source_id": format!("{}#{}", path, text.sha256),
                }));
            }
        }
        // Account for JSON escaping and long paths, not just excerpt bytes. Reserve
        // space for the envelope; preserve a prefix of each bounded result array.
        let mut output_bytes = 1024;
        for items in [&mut matches, &mut skipped] {
            let mut keep = 0;
            for item in items.iter() {
                let size = item.to_string().len() + 1;
                if output_bytes + size > MAX_TEXT {
                    truncation_reason = Some("output byte limit");
                    break;
                }
                output_bytes += size;
                keep += 1;
            }
            items.truncate(keep);
        }
        Ok(serde_json::json!({
            "matches": matches, "skipped": skipped,
            "truncated": truncation_reason.is_some(), "truncation_reason": truncation_reason,
        }))
    }

    /// Replace one exact literal in an existing text file, bound to its current digest.
    pub fn edit(
        &self,
        path: &str,
        old_string: &str,
        new_string: &str,
        expected_sha256: &str,
    ) -> Result<(), String> {
        if old_string.is_empty() {
            return Err("old_string must not be empty".into());
        }
        if new_string.contains('\0') {
            return Err("replacement must be text without NUL bytes".into());
        }
        let current = self.read(path)?;
        if current.sha256 != expected_sha256 {
            return Err("stale or missing expected_sha256".into());
        }
        let start = current
            .content
            .find(old_string)
            .ok_or("old_string not found")?;
        // Advance one character, not the match length: overlapping occurrences
        // (for example "aa" in "aaa") are ambiguous too.
        let Some(first) = current.content[start..].chars().next() else {
            return Err("old_string not found".into());
        };
        let next = start + first.len_utf8();
        if current.content[next..].contains(old_string) {
            return Err("old_string is ambiguous".into());
        }
        // Enforce write's bound before allocating the replacement buffer.
        if new_string.len() > MAX_TEXT - (current.content.len() - old_string.len()) {
            return Err("text size limit exceeded".into());
        }
        let content = current.content.replacen(old_string, new_string, 1);
        // write rechecks this digest under the mutation lock, so an intervening
        // edit cannot be silently overwritten by this optimistic read.
        self.write(path, &content, Some(expected_sha256))
    }

    pub fn list(&self) -> Result<Inventory, String> {
        inspect(&self.root)
    }
}
