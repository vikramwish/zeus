//! Read-only views over the runtime's existing durable artifacts; not transcript resume.
use serde_json::{json, Value};
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};

const FILE_LIMIT: usize = 1024 * 1024;
const SCAN_LIMIT: usize = 64 * 1024 * 1024;

fn directory(path: &Path) -> Result<(), String> {
    let meta = fs::symlink_metadata(path).map_err(|e| format!("{}: {e}", path.display()))?;
    if !meta.is_dir() || meta.file_type().is_symlink() {
        return Err(format!(
            "{} must be a real directory, not a symlink",
            path.display()
        ));
    }
    Ok(())
}

fn valid_id(id: &str) -> bool {
    let Some(rest) = id.strip_prefix("run-") else {
        return false;
    };
    let Some((stamp, pid)) = rest.split_once('-') else {
        return false;
    };
    !stamp.is_empty()
        && !pid.is_empty()
        && stamp.bytes().all(|b| b.is_ascii_digit())
        && pid.bytes().all(|b| b.is_ascii_digit())
}

fn run_path(state: &Path, id: &str) -> Result<PathBuf, String> {
    if !valid_id(id) {
        return Err(
            "invalid run ID; use the exact run-<timestamp>-<pid> from sessions list".into(),
        );
    }
    directory(state)?;
    let path = state.join(id);
    directory(&path)?;
    Ok(path)
}

fn open_artifact(path: &Path) -> Result<Option<File>, String> {
    let meta = match fs::symlink_metadata(path) {
        Ok(meta) => meta,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.to_string()),
    };
    if !meta.is_file() || meta.file_type().is_symlink() {
        return Err(format!(
            "{} must be a regular file, not a symlink",
            path.display()
        ));
    }
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    let file = options.open(path).map_err(|e| e.to_string())?;
    if !file.metadata().map_err(|e| e.to_string())?.is_file() {
        return Err("artifact is not a regular file".into());
    }
    Ok(Some(file))
}

fn artifact(path: &Path, first_line: bool, budget: &mut usize) -> Result<Option<Value>, String> {
    let Some(file) = open_artifact(path)? else {
        return Ok(None);
    };
    let limit = if first_line { 128 * 1024 } else { FILE_LIMIT };
    let mut bytes = Vec::new();
    let mut reader = BufReader::new(file.take((limit + 1) as u64));
    if first_line {
        reader
            .read_until(b'\n', &mut bytes)
            .map_err(|e| e.to_string())?;
    } else {
        reader.read_to_end(&mut bytes).map_err(|e| e.to_string())?;
    }
    *budget = budget
        .checked_sub(bytes.len())
        .ok_or("session scan exceeds 64 MiB")?;
    if bytes.len() > limit {
        return Err(format!("{} exceeds artifact read limit", path.display()));
    }
    // A run may exist before its first audit record is written.
    if first_line && bytes.is_empty() {
        return Ok(None);
    }
    let value: Value = serde_json::from_slice(&bytes)
        .map_err(|e| format!("{}: invalid JSON: {e}", path.display()))?;
    if !value.is_object() {
        return Err(format!("{}: expected JSON object", path.display()));
    }
    Ok(Some(value))
}

fn read_run(state: &Path, id: &str, budget: &mut usize) -> Result<Value, String> {
    let path = run_path(state, id)?;
    let start = artifact(&path.join("events.jsonl"), true, budget)?;
    if start.as_ref().is_some_and(|v| v["kind"] != "start") {
        return Err("first audit event is not start".into());
    }
    let report = artifact(&path.join("report.json"), false, budget)?;
    let error = artifact(&path.join("error.json"), false, budget)?;
    let learning = artifact(&path.join("learning.json"), false, budget)?;
    let status = error
        .as_ref()
        .map(|_| "infrastructure_error")
        .or_else(|| report.as_ref().and_then(|r| r["status"].as_str()))
        .unwrap_or("incomplete");
    Ok(json!({"id":id,"run_dir":path,"status":status,
        "task":start.as_ref().map(|s| &s["data"]["task"]),
        "source":start.as_ref().map(|s| &s["data"]["source"]),
        "summary":report.as_ref().map(|r| &r["summary"]),
        "start":start,"report":report,"error":error,"learning":learning}))
}

/// Show one exact run ID without replaying tools or modifying state.
pub fn show(state: &Path, id: &str) -> Result<Value, String> {
    let mut budget = SCAN_LIMIT;
    read_run(state, id, &mut budget)
}

/// List or case-insensitively search task, summary, status and ID. No global discovery.
pub fn list(state: &Path, query: Option<&str>) -> Result<Value, String> {
    directory(state)?;
    if query.is_some_and(|q| q.trim().is_empty() || q.len() > 4096) {
        return Err("search query must be 1..4096 bytes".into());
    }
    let mut ids = Vec::new();
    for (index, entry) in fs::read_dir(state).map_err(|e| e.to_string())?.enumerate() {
        if index >= 10000 {
            return Err("state directory exceeds 10000 entries".into());
        }
        let entry = entry.map_err(|e| e.to_string())?;
        if let Some(id) = entry.file_name().to_str().filter(|s| valid_id(s)) {
            ids.push(id.to_owned());
            if ids.len() > 1000 {
                return Err(
                    "session catalog exceeds 1000 runs; select a smaller state directory".into(),
                );
            }
        }
    }
    ids.sort();
    ids.reverse();
    let mut budget = SCAN_LIMIT;
    let mut sessions = Vec::new();
    let query = query.map(str::to_lowercase);
    for id in ids {
        let mut value = read_run(state, &id, &mut budget)?;
        for key in ["start", "report", "error", "learning"] {
            value.as_object_mut().unwrap().remove(key);
        }
        if query.as_ref().is_none_or(|q| {
            ["id", "task", "summary", "status"].iter().any(|key| {
                value[key]
                    .as_str()
                    .is_some_and(|s| s.to_lowercase().contains(q))
            })
        }) {
            sessions.push(value);
        }
    }
    Ok(
        json!({"sessions":sessions,"scope":"run metadata and final summaries; not a resumable transcript"}),
    )
}
