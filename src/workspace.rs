use serde::Serialize;
use sha2::{Digest, Sha256};
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

const MAX_FILE: u64 = 16 * 1024 * 1024;
const MAX_TOTAL: u64 = 64 * 1024 * 1024;
const MAX_TEXT: usize = 128 * 1024;
const MAX_FILES: usize = 2000;
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
    ) || name == ".env"
        || name.starts_with(".env.")
        || name.ends_with(".pem")
        || name.ends_with(".key")
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
    } else if std::str::from_utf8(bytes).is_ok() && !bytes.contains(&0) {
        "text"
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
    walk(source, base, &mut inventory, &mut total)?;
    Ok(inventory)
}
fn walk(path: &Path, base: &Path, inv: &mut Inventory, total: &mut u64) -> Result<(), String> {
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
            walk(&child, base, inv, total)?;
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
        Ok(Self { root, inventory })
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
        Ok(ReadResult { content, sha256 })
    }
    pub fn write(&self, path: &str, content: &str, expected: Option<&str>) -> Result<(), String> {
        if content.len() > MAX_TEXT {
            return Err("text size limit exceeded".into());
        }
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
        // Recheck after directory creation; only the coordinator writes during agent execution.
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
    pub fn list(&self) -> Result<Inventory, String> {
        inspect(&self.root)
    }
}
