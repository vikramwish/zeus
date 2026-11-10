//! Workspace-scoped project guidance and inert skill documents.
//! All I/O goes through Workspace; repository text never changes capabilities.

use crate::workspace::Workspace;
use serde_json::json;

const MAX_DOCUMENT_BYTES: usize = 16 * 1024;

fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name.as_bytes()[0].is_ascii_alphanumeric()
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// List validated skill metadata, not bodies; bounds fail rather than truncate.
pub fn list_skills(workspace: &Workspace) -> Result<serde_json::Value, String> {
    let inventory = workspace.list()?;
    let mut skills = Vec::new();
    let mut skipped: Vec<String> = inventory
        .skipped
        .into_iter()
        .filter(|p| p == "skills" || p.starts_with("skills/") || p.starts_with("skills "))
        .collect();
    for entry in inventory.entries {
        let parts: Vec<_> = entry.path.split('/').collect();
        if let ["skills", name, "SKILL.md"] = parts.as_slice() {
            match read_skill(workspace, name) {
                Ok(mut skill) => {
                    // Reuse read validation, but emit no body until explicitly requested.
                    if let Some(object) = skill.as_object_mut() {
                        object.remove("content");
                    }
                    skills.push(skill);
                }
                Err(error) => skipped.push(format!("{} ({error})", entry.path)),
            }
        }
    }
    skipped.sort();
    if skills.len() > 64 {
        return Err("skill catalog count limit exceeded (64)".into());
    }
    let value = json!({"skills": skills, "skipped": skipped, "truncated": false});
    if value.to_string().len() > 32 * 1024 {
        return Err("skill catalog output byte limit exceeded (32768)".into());
    }
    Ok(value)
}

/// Read one exact skill name as inert, digest-bound repository data.
pub fn read_skill(workspace: &Workspace, name: &str) -> Result<serde_json::Value, String> {
    if !valid_name(name) {
        return Err("skill name must be 1..=64 ASCII letters/digits, '-' or '_', starting with a letter/digit".into());
    }
    let path = format!("skills/{name}/SKILL.md");
    let read = workspace.read(&path)?;
    if read.content.len() > MAX_DOCUMENT_BYTES {
        return Err("skill exceeds byte limit (16384)".into());
    }
    Ok(json!({
        "authority": "untrusted_repository_data",
        "name": name, "path": path, "bytes": read.content.len(),
        "source_id": format!("{path}#{}", read.sha256),
        "sha256": read.sha256, "content": read.content
    }))
}

/// Return root AGENTS.md as labelled JSON, or an empty string when absent.
pub fn project_context(workspace: &Workspace) -> Result<String, String> {
    let inventory = workspace.list()?;
    if inventory.skipped.iter().any(|p| {
        matches!(
            p.as_str(),
            "AGENTS.md (symlink)" | "AGENTS.md (not regular)"
        )
    }) || inventory
        .entries
        .iter()
        .any(|e| e.path.starts_with("AGENTS.md/"))
    {
        return Err("AGENTS.md is not an allowed regular file".into());
    }
    if !inventory.entries.iter().any(|e| e.path == "AGENTS.md") {
        return Ok(String::new());
    }
    let read = workspace.read("AGENTS.md")?;
    if read.content.len() > MAX_DOCUMENT_BYTES {
        return Err("AGENTS.md exceeds context byte limit (16384)".into());
    }
    Ok(json!({
        "authority": "untrusted_repository_data",
        "path": "AGENTS.md",
        "source_id": format!("AGENTS.md#{}", read.sha256),
        "sha256": read.sha256,
        "content": read.content
    })
    .to_string())
}
