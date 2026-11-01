use crate::model::Protocol;
use serde::Deserialize;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthMode {
    #[default]
    Environment,
    Chatgpt,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ServerConfig {
    pub name: String,
    pub command: Vec<String>,
    pub allowed_tools: Vec<String>,
}
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Job {
    pub task: String,
    pub source: PathBuf,
    pub state_dir: PathBuf,
    pub endpoint: String,
    pub model: String,
    pub api_key_env: Option<String>,
    #[serde(default)]
    pub protocol: Protocol,
    #[serde(default)]
    pub auth: AuthMode,
    #[serde(default = "context")]
    pub context: String,
    #[serde(default = "turns")]
    pub max_turns: usize,
    #[serde(default = "model_timeout")]
    pub model_timeout: u64,
    #[serde(default)]
    pub verifier: Vec<String>,
    #[serde(default = "timeout")]
    pub verifier_timeout: u64,
    #[serde(default)]
    pub mcp: Vec<ServerConfig>,
}
fn context() -> String {
    "reconstruction".into()
}
fn turns() -> usize {
    12
}
fn timeout() -> u64 {
    30
}
fn model_timeout() -> u64 {
    crate::model::DEFAULT_MODEL_TIMEOUT
}
#[derive(Clone, Copy, Default)]
pub struct Approvals {
    pub network: bool,
    pub exec: bool,
    pub mcp: bool,
}
impl Job {
    pub(crate) fn validate_model(&self) -> Result<(), String> {
        crate::model::validate_model_timeout(self.model_timeout)?;
        if self.auth == AuthMode::Chatgpt
            && (self.protocol != Protocol::Responses
                || self.endpoint != "https://api.openai.com/v1/responses"
                || self.api_key_env.is_some())
        {
            return Err("ChatGPT subscription requires protocol responses, endpoint https://api.openai.com/v1/responses, and no api_key_env; no API-billing fallback is allowed".into());
        }
        Ok(())
    }

    pub fn load(path: &Path, approval: Approvals) -> Result<Self, String> {
        if !approval.network {
            return Err("model access requires --allow-network (sends selected source/tool evidence to configured endpoint)".into());
        }
        let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
        if bytes.len() > 128 * 1024 {
            return Err("job exceeds 128KiB".into());
        }
        let mut job: Self =
            serde_json::from_slice(&bytes).map_err(|e| format!("invalid job: {e}"))?;
        job.validate_model()?;
        if job.task.trim().is_empty() || job.task.len() > 32768 {
            return Err("task must be 1..32768 bytes".into());
        }
        if !(1..=32).contains(&job.max_turns) {
            return Err("max_turns must be 1..32".into());
        }
        if !(1..=120).contains(&job.verifier_timeout) {
            return Err("verifier_timeout must be 1..120".into());
        }
        if job.context.trim().is_empty() || job.context.len() > 128 {
            return Err("context must be 1..128 bytes".into());
        }
        if !job.verifier.is_empty() && !approval.exec {
            return Err("verifier execution requires --allow-exec".into());
        }
        if job.verifier.len() > 128 {
            return Err("too many verifier arguments".into());
        }
        if let Some(program) = job.verifier.first() {
            if !Path::new(program).is_absolute() {
                return Err("verifier executable must be absolute".into());
            }
        }
        if !job.mcp.is_empty() && !approval.mcp {
            return Err("MCP servers execute host programs: --allow-mcp is required; only approve trusted servers and tools".into());
        }
        if job.mcp.len() > 8 {
            return Err("maximum 8 MCP servers".into());
        }
        let mut names = std::collections::HashSet::new();
        for server in &job.mcp {
            if server.name.is_empty()
                || server.name.len() > 16
                || !server
                    .name
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'_')
                || !names.insert(&server.name)
            {
                return Err("invalid or duplicate MCP server name".into());
            }
            if server.command.is_empty() || !Path::new(&server.command[0]).is_absolute() {
                return Err("MCP executable must be an explicit absolute path".into());
            }
            if server.allowed_tools.is_empty() || server.allowed_tools.len() > 64 {
                return Err("MCP tools require a nonempty bounded allowlist".into());
            }
        }
        if let Some(name) = &job.api_key_env {
            if name.is_empty() || !name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') {
                return Err("invalid api_key_env name".into());
            }
        }
        let base = std::fs::canonicalize(path)
            .map_err(|e| e.to_string())?
            .parent()
            .ok_or("job parent missing")?
            .to_owned();
        for value in [&mut job.source, &mut job.state_dir] {
            if value.is_relative() {
                *value = base.join(&*value);
            }
        }
        job.source = std::fs::canonicalize(&job.source).map_err(|e| format!("source: {e}"))?;
        if job
            .state_dir
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
        {
            return Err("state_dir must not contain parent traversal".into());
        }
        if job.state_dir.starts_with(&job.source) {
            return Err("state_dir must be outside source".into());
        }
        if job.state_dir.exists() {
            let actual = std::fs::canonicalize(&job.state_dir).map_err(|e| e.to_string())?;
            if actual.starts_with(&job.source) {
                return Err("state_dir must be outside source".into());
            }
        }
        // Do not infer code-vs-data from arbitrary interpreter argv. Only an
        // external executable, or an interpreter + external script, establishes
        // an explicit entry point. Complex commands belong in a trusted wrapper.
        let files: Vec<&String> = match job.verifier.as_slice() {
            [] => vec![],
            [program] => vec![program],
            [program, script] => vec![program, script],
            [program, option, script] if option == "-I" => vec![program, script],
            _ => return Err("verifier must be an external executable or an interpreter with an absolute external script (optional -I); use an external wrapper for other arguments".into()),
        };
        let state_parent =
            std::fs::canonicalize(job.state_dir.parent().ok_or("state_dir has no parent")?)
                .map_err(|e| format!("state_dir parent must exist: {e}"))?;
        let state = if job.state_dir.exists() {
            std::fs::canonicalize(&job.state_dir).map_err(|e| e.to_string())?
        } else {
            state_parent.join(job.state_dir.file_name().ok_or("invalid state_dir")?)
        };
        for arg in files {
            let path = Path::new(arg);
            if !path.is_absolute() || !path.is_file() {
                return Err("verifier entry points must be absolute existing regular files".into());
            }
            let path = std::fs::canonicalize(path).map_err(|e| e.to_string())?;
            if path.starts_with(&job.source) || path.starts_with(&state) {
                return Err("verifier files must be external to source and state/candidate".into());
            }
        }
        #[cfg(unix)]
        if let Some(program) = job.verifier.first() {
            let program = std::ffi::CString::new(program.as_str()).map_err(|e| e.to_string())?;
            // Check this process's effective credentials/ACLs, not just whether
            // any class has an execute bit. The launch handshake still handles
            // races, invalid formats, and missing shebang interpreters.
            if unsafe {
                libc::faccessat(
                    libc::AT_FDCWD,
                    program.as_ptr(),
                    libc::X_OK,
                    libc::AT_EACCESS,
                )
            } != 0
            {
                return Err(format!(
                    "verifier program must be executable: {}",
                    std::io::Error::last_os_error()
                ));
            }
        }
        Ok(job)
    }
}
