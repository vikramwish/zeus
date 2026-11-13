use serde::Serialize;
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
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let hash = ring::digest::digest(&ring::digest::SHA256, bytes);
    let mut out = [0u8; 64];
    for (i, &byte) in hash.as_ref().iter().enumerate() {
        out[i * 2] = HEX[(byte >> 4) as usize];
        out[i * 2 + 1] = HEX[(byte & 0x0f) as usize];
    }
    std::str::from_utf8(&out).unwrap_or_default().to_owned()
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
const fn secret_luts() -> ([u8; 256], [u8; 256], [u8; 256], [u8; 256]) {
    let mut b0 = [0u8; 256];
    let mut b1 = [0u8; 256];
    let mut b2 = [0u8; 256];
    let mut b3 = [0u8; 256];

    b0[b's' as usize] |= 1;
    b1[b'k' as usize] |= 1;
    b2[b'-' as usize] |= 1;
    b3[b'a' as usize] |= 1;
    b3[b'p' as usize] |= 1;
    b3[b'o' as usize] |= 1;
    b3[b'l' as usize] |= 1;

    b0[b'A' as usize] |= 2;
    b1[b'K' as usize] |= 2;
    b1[b'S' as usize] |= 2;
    b2[b'I' as usize] |= 2;
    b3[b'A' as usize] |= 2;

    b0[b'g' as usize] |= 4;
    b1[b'h' as usize] |= 4;
    b2[b'p' as usize] |= 4;
    b2[b'o' as usize] |= 4;
    b2[b'u' as usize] |= 4;
    b2[b's' as usize] |= 4;
    b2[b'r' as usize] |= 4;
    b3[b'_' as usize] |= 4;

    b0[b'x' as usize] |= 8;
    b1[b'o' as usize] |= 8;
    b2[b'x' as usize] |= 8;
    b3[b'b' as usize] |= 8;
    b3[b'p' as usize] |= 8;
    b3[b'a' as usize] |= 8;
    b3[b'r' as usize] |= 8;
    b3[b's' as usize] |= 8;

    b0[b'A' as usize] |= 16;
    b1[b'I' as usize] |= 16;
    b2[b'z' as usize] |= 16;
    b3[b'a' as usize] |= 16;

    b0[b'P' as usize] |= 32;
    b1[b'R' as usize] |= 32;
    b2[b'I' as usize] |= 32;
    b3[b'V' as usize] |= 32;

    (b0, b1, b2, b3)
}

const SECRET_LUTS: ([u8; 256], [u8; 256], [u8; 256], [u8; 256]) = secret_luts();

fn secret_at(text: &str, bytes: &[u8], index: usize) -> Option<&'static str> {
    #[inline]
    fn run_len(bytes: &[u8], allowed: fn(u8) -> bool) -> usize {
        bytes.iter().take_while(|b| allowed(**b)).count()
    }
    #[inline]
    fn token(b: u8) -> bool {
        b.is_ascii_alphanumeric() || b == b'_' || b == b'-'
    }

    let window = &bytes[index..index + 4];
    let rest = &bytes[index + 4..];
    if window == b"PRIV" && rest.starts_with(b"ATE KEY-----") {
        return Some("private key block");
    }
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
    }?;
    // Preserve precedence of private key blocks when both appear in one file.
    if text[index + 4..].contains("PRIVATE KEY-----") {
        return Some("private key block");
    }
    Some(kind)
}

pub fn secret_like(text: &str) -> Option<&'static str> {
    let bytes = text.as_bytes();
    // Shortest pattern ("PRIVATE KEY-----") is 16 bytes.
    if bytes.len() < 16 {
        return None;
    }
    let (b0, b1, b2, b3) = &SECRET_LUTS;
    let limit = bytes.len() - 15;
    let mut i = 0;
    while i + 8 <= limit {
        let c0 = b0[bytes[i] as usize]
            & b1[bytes[i + 1] as usize]
            & b2[bytes[i + 2] as usize]
            & b3[bytes[i + 3] as usize];
        let c1 = b0[bytes[i + 1] as usize]
            & b1[bytes[i + 2] as usize]
            & b2[bytes[i + 3] as usize]
            & b3[bytes[i + 4] as usize];
        let c2 = b0[bytes[i + 2] as usize]
            & b1[bytes[i + 3] as usize]
            & b2[bytes[i + 4] as usize]
            & b3[bytes[i + 5] as usize];
        let c3 = b0[bytes[i + 3] as usize]
            & b1[bytes[i + 4] as usize]
            & b2[bytes[i + 5] as usize]
            & b3[bytes[i + 6] as usize];
        let c4 = b0[bytes[i + 4] as usize]
            & b1[bytes[i + 5] as usize]
            & b2[bytes[i + 6] as usize]
            & b3[bytes[i + 7] as usize];
        let c5 = b0[bytes[i + 5] as usize]
            & b1[bytes[i + 6] as usize]
            & b2[bytes[i + 7] as usize]
            & b3[bytes[i + 8] as usize];
        let c6 = b0[bytes[i + 6] as usize]
            & b1[bytes[i + 7] as usize]
            & b2[bytes[i + 8] as usize]
            & b3[bytes[i + 9] as usize];
        let c7 = b0[bytes[i + 7] as usize]
            & b1[bytes[i + 8] as usize]
            & b2[bytes[i + 9] as usize]
            & b3[bytes[i + 10] as usize];
        let mask = c0 | c1 | c2 | c3 | c4 | c5 | c6 | c7;
        if mask != 0 {
            for (k, c) in [c0, c1, c2, c3, c4, c5, c6, c7].into_iter().enumerate() {
                if c != 0 {
                    if let Some(kind) = secret_at(text, bytes, i + k) {
                        return Some(kind);
                    }
                }
            }
        }
        i += 8;
    }
    while i < limit {
        if (b0[bytes[i] as usize]
            & b1[bytes[i + 1] as usize]
            & b2[bytes[i + 2] as usize]
            & b3[bytes[i + 3] as usize])
            != 0
        {
            if let Some(kind) = secret_at(text, bytes, i) {
                return Some(kind);
            }
        }
        i += 1;
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
fn open_regular_nofollow(path: &Path) -> std::io::Result<fs::File> {
    let mut opts = OpenOptions::new();
    opts.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.custom_flags(libc::O_NOFOLLOW);
    }
    opts.open(path)
}

fn read_open_bounded_into(
    mut file: fs::File,
    limit: u64,
    meta: &fs::Metadata,
    out: &mut Vec<u8>,
) -> Result<(), String> {
    if !meta.is_file() || meta.file_type().is_symlink() {
        return Err("regular files only".into());
    }
    if meta.len() > limit {
        return Err(format!("file exceeds {limit} byte limit"));
    }
    out.clear();
    let needed = (meta.len() as usize).saturating_add(1);
    if out.capacity() < needed {
        out.reserve(needed - out.capacity());
    }
    (&mut file)
        .take(limit + 1)
        .read_to_end(out)
        .map_err(|e| e.to_string())?;
    if out.len() as u64 > limit {
        return Err("file grew beyond limit".into());
    }
    Ok(())
}

fn read_bounded_into(
    path: &Path,
    limit: u64,
    meta: &fs::Metadata,
    out: &mut Vec<u8>,
) -> Result<(), String> {
    if !meta.is_file() || meta.file_type().is_symlink() {
        return Err("regular files only".into());
    }
    if meta.len() > limit {
        return Err(format!("file exceeds {limit} byte limit"));
    }
    let file = open_regular_nofollow(path).map_err(|e| e.to_string())?;
    read_open_bounded_into(file, limit, meta, out)
}

fn bounded_read(path: &Path, limit: u64) -> Result<Vec<u8>, String> {
    let meta = fs::symlink_metadata(path).map_err(|e| e.to_string())?;
    let mut data = Vec::new();
    read_bounded_into(path, limit, &meta, &mut data)?;
    Ok(data)
}

#[cfg(unix)]
fn sanitize_permissions(meta: &fs::Metadata) -> fs::Permissions {
    use std::os::unix::fs::PermissionsExt;
    // Preserve rwx, never copy setuid, setgid, or sticky bits into the candidate.
    fs::Permissions::from_mode(meta.permissions().mode() & 0o777)
}

struct WalkCtx<'a> {
    inv: Inventory,
    total: u64,
    scratch: Vec<u8>,
    copy_root: Option<&'a Path>,
}

pub fn inspect(source: &Path) -> Result<Inventory, String> {
    walk_root(source, None)
}

fn walk_root(source: &Path, copy_root: Option<&Path>) -> Result<Inventory, String> {
    let meta = fs::symlink_metadata(source).map_err(|e| e.to_string())?;
    if meta.file_type().is_symlink() {
        return Err("target cannot be a symlink".into());
    }
    let mut ctx = WalkCtx {
        inv: Inventory {
            entries: vec![],
            skipped: vec![],
        },
        total: 0,
        scratch: Vec::with_capacity(64 * 1024),
        copy_root,
    };
    if meta.is_dir() {
        walk_dir(source, "", 0, &mut ctx)?;
    } else if meta.is_file() {
        let name = source
            .file_name()
            .and_then(|s| s.to_str())
            .ok_or("non-UTF8 filename")?;
        if excluded(name) {
            ctx.inv.skipped.push(name.to_owned());
        } else {
            let mut root_created = true;
            visit_regular_file(source, name, &meta, &mut root_created, &mut ctx)?;
        }
    } else {
        let name = source
            .file_name()
            .and_then(|s| s.to_str())
            .ok_or("non-UTF8 filename")?;
        ctx.inv.skipped.push(format!("{name} (not regular)"));
    }
    Ok(ctx.inv)
}

fn visit_open_regular_file(
    path: &Path,
    relative: &str,
    file: fs::File,
    meta: &fs::Metadata,
    dir_created: &mut bool,
    ctx: &mut WalkCtx<'_>,
) -> Result<(), String> {
    if ctx.inv.entries.len() >= MAX_FILES {
        return Err("file-count limit exceeded".into());
    }
    if meta.len() > MAX_FILE || ctx.total + meta.len() > MAX_TOTAL {
        return Err("snapshot byte limit exceeded; select a smaller target".into());
    }
    read_open_bounded_into(file, MAX_FILE, meta, &mut ctx.scratch)?;
    if ctx.copy_root.is_some() && ctx.scratch.len() as u64 != meta.len() {
        return Err("source changed during snapshot".into());
    }
    ctx.total += ctx.scratch.len() as u64;
    if ctx.total > MAX_TOTAL {
        return Err("snapshot byte limit exceeded".into());
    }
    let sha256 = digest(&ctx.scratch);
    let file_kind = kind(&ctx.scratch, path);
    if let Some(copy_root) = ctx.copy_root {
        let target = copy_root.join(relative);
        if !*dir_created {
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            *dir_created = true;
        }
        let mut out_file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&target)
            .map_err(|e| e.to_string())?;
        out_file
            .write_all(&ctx.scratch)
            .map_err(|e| e.to_string())?;
        #[cfg(unix)]
        out_file
            .set_permissions(sanitize_permissions(meta))
            .map_err(|e| e.to_string())?;
    }
    ctx.inv.entries.push(Entry {
        path: relative.to_owned(),
        bytes: ctx.scratch.len() as u64,
        sha256,
        kind: file_kind,
    });
    Ok(())
}

fn visit_regular_file(
    path: &Path,
    relative: &str,
    meta: &fs::Metadata,
    dir_created: &mut bool,
    ctx: &mut WalkCtx<'_>,
) -> Result<(), String> {
    let file = open_regular_nofollow(path).map_err(|e| e.to_string())?;
    visit_open_regular_file(path, relative, file, meta, dir_created, ctx)
}

fn walk_dir(
    dir: &Path,
    rel_prefix: &str,
    depth: usize,
    ctx: &mut WalkCtx<'_>,
) -> Result<(), String> {
    if depth > MAX_DEPTH {
        return Err(format!(
            "directory depth limit exceeded ({MAX_DEPTH}); select a smaller target"
        ));
    }
    let mut entries = Vec::new();
    for entry in fs::read_dir(dir).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        entries.push((entry.file_name(), entry.file_type().ok()));
        if entries.len() > 10_000 {
            return Err("directory entry limit exceeded".into());
        }
    }
    entries.sort_by(|a, b| a.0.cmp(&b.0));
    let mut dir_created = rel_prefix.is_empty();
    for (os_name, ft) in entries {
        let name = os_name.to_str().ok_or("non-UTF8 filename")?;
        let relative = if rel_prefix.is_empty() {
            name.to_owned()
        } else {
            format!("{rel_prefix}/{name}")
        };
        if excluded(name) {
            ctx.inv.skipped.push(relative);
        } else if ft.is_some_and(|t| t.is_symlink()) {
            ctx.inv.skipped.push(format!("{relative} (symlink)"));
        } else {
            let child = dir.join(name);
            if ft.is_some_and(|t| t.is_file()) {
                match open_regular_nofollow(&child) {
                    Ok(file) => {
                        let meta = file.metadata().map_err(|e| e.to_string())?;
                        if meta.is_file() {
                            visit_open_regular_file(
                                &child,
                                &relative,
                                file,
                                &meta,
                                &mut dir_created,
                                ctx,
                            )?;
                        } else {
                            ctx.inv.skipped.push(format!("{relative} (not regular)"));
                        }
                    }
                    #[cfg(unix)]
                    Err(e) if e.raw_os_error() == Some(libc::ELOOP) => {
                        ctx.inv.skipped.push(format!("{relative} (symlink)"));
                    }
                    Err(e) => return Err(e.to_string()),
                }
            } else {
                let meta = fs::symlink_metadata(&child).map_err(|e| e.to_string())?;
                if meta.file_type().is_symlink() {
                    ctx.inv.skipped.push(format!("{relative} (symlink)"));
                } else if meta.is_dir() {
                    walk_dir(&child, &relative, depth + 1, ctx)?;
                } else if meta.is_file() {
                    visit_regular_file(&child, &relative, &meta, &mut dir_created, ctx)?;
                } else {
                    ctx.inv.skipped.push(format!("{relative} (not regular)"));
                }
            }
        }
        if ctx.inv.skipped.len() > MAX_FILES {
            return Err("too many skipped entries".into());
        }
    }
    Ok(())
}

fn check_clean_text(text: &str) -> Result<(), String> {
    if text.as_bytes().contains(&0) {
        return Err("binary file: use artifact analysis tools".into());
    }
    if let Some(kind) = secret_like(text) {
        return Err(format!(
            "file contains secret-like content ({kind}); it stays in the candidate but is not shown to the model"
        ));
    }
    Ok(())
}

impl Workspace {
    pub fn snapshot(source: &Path, destination: &Path) -> Result<Self, String> {
        let src_meta = fs::symlink_metadata(source).map_err(|e| e.to_string())?;
        if src_meta.file_type().is_symlink() {
            return Err("target cannot be a symlink".into());
        }
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
        let root = match fs::canonicalize(destination) {
            Ok(root) => root,
            Err(e) => {
                let _ = fs::remove_dir_all(destination);
                return Err(e.to_string());
            }
        };
        let inventory = match walk_root(&canonical_source, Some(&root)) {
            Ok(inv) => inv,
            Err(e) => {
                let _ = fs::remove_dir_all(&root);
                return Err(e);
            }
        };
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
        if bytes.contains(&0) {
            return Err("binary file: use artifact analysis tools".into());
        }
        let content = String::from_utf8(bytes)
            .map_err(|_| "binary file: use artifact analysis tools".to_string())?;
        check_clean_text(&content)?;
        let sha256 = digest(content.as_bytes());
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
        #[cfg(unix)]
        let mut permissions = None;
        let existed = match fs::symlink_metadata(&target) {
            Ok(meta) => {
                #[cfg(unix)]
                {
                    permissions = Some(sanitize_permissions(&meta));
                }
                let mut current = Vec::new();
                read_bounded_into(&target, MAX_TEXT as u64, &meta, &mut current)?;
                if Some(digest(&current).as_str()) != expected {
                    return Err("stale or missing expected_sha256".into());
                }
                true
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                if expected.is_some() {
                    return Err("expected existing file is missing".into());
                }
                false
            }
            Err(e) => return Err(e.to_string()),
        };
        let parent = target.parent().ok_or("missing parent")?;
        if !existed {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            // Recheck after directory creation; all agent writes share the mutation lock.
            self.resolve(path)?;
        }
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
            // Drop the writable FD before rename so concurrent exec never hits ETXTBSY.
            drop(file);
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
        // Tuple `(path, kind_hint)` keeps exact `path` ordering while avoiding
        // redundant `symlink_metadata` lookups for `d_type`-confirmed files/symlinks.
        const HINT_UNKNOWN: u8 = 0;
        const HINT_FILE: u8 = 1;
        const HINT_SYMLINK: u8 = 2;
        let mut pending = std::collections::BTreeSet::from([(String::new(), HINT_UNKNOWN)]);
        let mut entries = 0;
        let mut files = 0;
        let mut bytes = 0u64;
        let mut scratch = Vec::with_capacity(64 * 1024);
        'paths: while let Some((path, hint)) = pending.pop_first() {
            if skipped.len() == MAX_RESULTS {
                truncation_reason = Some("diagnostic limit");
                break;
            }
            let target = if path.is_empty() {
                self.root.clone()
            } else {
                let leaf = path.rsplit('/').next().unwrap_or(&path);
                if excluded(leaf) {
                    skipped.push(serde_json::json!({
                        "path": path,
                        "reason": "excluded or sensitive path"
                    }));
                    continue;
                }
                if hint == HINT_SYMLINK {
                    skipped.push(serde_json::json!({"path": path, "reason": "symlinks forbidden"}));
                    continue;
                }
                self.root.join(&path)
            };
            if hint == HINT_FILE && (files == MAX_FILES || bytes + MAX_TEXT as u64 > MAX_TOTAL) {
                truncation_reason = Some("file or byte limit");
                break;
            }
            let (file, meta) = if hint == HINT_FILE {
                match open_regular_nofollow(&target) {
                    Ok(file) => {
                        let meta = file.metadata().map_err(|e| e.to_string())?;
                        (Some(file), meta)
                    }
                    #[cfg(unix)]
                    Err(e) if e.raw_os_error() == Some(libc::ELOOP) => {
                        skipped.push(
                            serde_json::json!({"path": path, "reason": "symlinks forbidden"}),
                        );
                        continue;
                    }
                    Err(e) => return Err(e.to_string()),
                }
            } else {
                (
                    None,
                    fs::symlink_metadata(&target).map_err(|e| e.to_string())?,
                )
            };
            if meta.file_type().is_symlink() {
                skipped.push(serde_json::json!({"path": path, "reason": "symlinks forbidden"}));
                continue;
            }
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
                    let child_hint = match child.file_type() {
                        Ok(ft) if ft.is_file() => HINT_FILE,
                        Ok(ft) if ft.is_symlink() => HINT_SYMLINK,
                        _ => HINT_UNKNOWN,
                    };
                    let name = child
                        .file_name()
                        .into_string()
                        .map_err(|_| "non-UTF8 path")?;
                    let next = if path.is_empty() {
                        name
                    } else {
                        format!("{path}/{name}")
                    };
                    pending.insert((next, child_hint));
                }
                continue;
            }
            if !meta.is_file() {
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
            let file = match file {
                Some(f) => f,
                None => match open_regular_nofollow(&target) {
                    Ok(f) => f,
                    Err(e) => {
                        bytes += MAX_TEXT as u64;
                        skipped.push(serde_json::json!({"path": path, "reason": e.to_string()}));
                        continue;
                    }
                },
            };
            if let Err(reason) = read_open_bounded_into(file, MAX_TEXT as u64, &meta, &mut scratch)
            {
                bytes += MAX_TEXT as u64;
                skipped.push(serde_json::json!({"path": path, "reason": reason}));
                continue;
            }
            if scratch.contains(&0) {
                bytes += MAX_TEXT as u64;
                skipped.push(serde_json::json!({
                    "path": path,
                    "reason": "binary file: use artifact analysis tools"
                }));
                continue;
            }
            let Ok(text) = std::str::from_utf8(&scratch) else {
                bytes += MAX_TEXT as u64;
                skipped.push(serde_json::json!({
                    "path": path,
                    "reason": "binary file: use artifact analysis tools"
                }));
                continue;
            };
            if let Err(reason) = check_clean_text(text) {
                bytes += MAX_TEXT as u64;
                skipped.push(serde_json::json!({"path": path, "reason": reason}));
                continue;
            }
            bytes += text.len() as u64;
            let mut iter = text.match_indices(query).peekable();
            if iter.peek().is_none() {
                continue;
            }
            let sha256 = digest(text.as_bytes());
            let source_id = format!("{path}#{sha256}");
            let text_bytes = text.as_bytes();
            let mut line = 1usize;
            let mut scanned = 0usize;
            for (offset, _) in iter {
                if matches.len() == max_results {
                    truncation_reason = Some("result limit");
                    break 'paths;
                }
                line += text_bytes[scanned..offset]
                    .iter()
                    .filter(|&&b| b == b'\n')
                    .count();
                scanned = offset;
                let line_start = text[..offset].rfind('\n').map_or(0, |i| i + 1);
                let line_end = text[offset..].find('\n').map_or(text.len(), |i| offset + i);
                // Center long-line excerpts near the match rather than silently hiding it.
                let mut start = line_start.max(offset.saturating_sub(MAX_EXCERPT / 4));
                while !text.is_char_boundary(start) {
                    start += 1;
                }
                let mut end = line_end.min(start + MAX_EXCERPT);
                while !text.is_char_boundary(end) {
                    end -= 1;
                }
                matches.push(serde_json::json!({
                    "path": path, "line": line, "byte_offset": offset,
                    "content": &text[start..end],
                    "content_truncated": start != line_start || end != line_end || offset + query.len() > end,
                    "sha256": &sha256,
                    "source_id": &source_id,
                }));
            }
        }
        // Account for JSON escaping and long paths, not just excerpt bytes. Reserve
        // space for the envelope; preserve a prefix of each bounded result array.
        let mut output_bytes = 1024;
        for items in [&mut matches, &mut skipped] {
            let mut keep = 0;
            for item in items.iter() {
                let Some(budget) = MAX_TEXT.checked_sub(output_bytes + 1) else {
                    truncation_reason = Some("output byte limit");
                    break;
                };
                let Some(size) = crate::output::json_len_within(item, budget)? else {
                    truncation_reason = Some("output byte limit");
                    break;
                };
                output_bytes += size + 1;
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
    pub(crate) fn inspect_subdir(&self, subdir: &str) -> Result<Inventory, String> {
        let sub = self.root.join(subdir);
        let meta = match std::fs::symlink_metadata(&sub) {
            Ok(meta) => meta,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Inventory {
                    entries: vec![],
                    skipped: vec![],
                });
            }
            Err(e) => return Err(e.to_string()),
        };
        let mut ctx = WalkCtx {
            inv: Inventory {
                entries: vec![],
                skipped: vec![],
            },
            total: 0,
            scratch: Vec::with_capacity(64 * 1024),
            copy_root: None,
        };
        if meta.file_type().is_symlink() {
            ctx.inv.skipped.push(format!("{subdir} (symlink)"));
        } else if excluded(subdir) {
            ctx.inv.skipped.push(subdir.to_owned());
        } else if meta.is_dir() {
            walk_dir(&sub, subdir, 1, &mut ctx)?;
        } else if meta.is_file() {
            let mut created = true;
            visit_regular_file(&sub, subdir, &meta, &mut created, &mut ctx)?;
        } else {
            ctx.inv.skipped.push(format!("{subdir} (not regular)"));
        }
        Ok(ctx.inv)
    }
}
