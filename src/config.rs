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
    /// Per-request deadline for this server (default 30, allowed 1..600).
    #[serde(default = "mcp_timeout")]
    pub timeout_seconds: u64,
}
fn mcp_timeout() -> u64 {
    crate::mcp::DEFAULT_TIMEOUT_SECONDS
}
/// User-owned limits. Repository text and model plans cannot enlarge them.
#[derive(Clone, Debug, serde::Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Autonomy {
    /// None preserves whole-candidate editing; Some([]) denies model file writes.
    pub write_paths: Option<Vec<String>>,
    pub max_repairs: usize,
    /// One shared request budget for planner, workers, integration and repairs.
    pub max_model_calls: Option<usize>,
    /// Shared input+output token budget, from provider usage reports.
    pub max_total_tokens: Option<u64>,
}
impl Default for Autonomy {
    fn default() -> Self {
        Self {
            write_paths: None,
            max_repairs: 2,
            max_model_calls: None,
            max_total_tokens: None,
        }
    }
}
impl Autonomy {
    fn validate(&self) -> Result<(), String> {
        if self.max_repairs > 8 {
            return Err("autonomy.max_repairs must be 0..8".into());
        }
        if self
            .max_model_calls
            .is_some_and(|n| !(1..=1024).contains(&n))
        {
            return Err("autonomy.max_model_calls must be 1..1024".into());
        }
        if self.max_total_tokens.is_some_and(|n| n == 0) {
            return Err("autonomy.max_total_tokens must be at least 1".into());
        }
        if let Some(paths) = &self.write_paths {
            if paths.len() > 128 {
                return Err("autonomy.write_paths permits at most 128 exact files".into());
            }
            for (index, path) in paths.iter().enumerate() {
                if path.is_empty()
                    || path.len() > 4096
                    || path.contains('\\')
                    || path.chars().any(char::is_control)
                    || path
                        .split('/')
                        .any(|p| p.is_empty() || p == "." || p == "..")
                    || !Path::new(path)
                        .components()
                        .all(|p| matches!(p, std::path::Component::Normal(_)))
                {
                    return Err(
                        "autonomy.write_paths requires canonical relative exact file paths".into(),
                    );
                }
                if paths[..index].iter().any(|other| {
                    Path::new(path).starts_with(other) || Path::new(other).starts_with(path)
                }) {
                    return Err(
                        "autonomy.write_paths contains duplicate or overlapping paths".into(),
                    );
                }
            }
        }
        Ok(())
    }
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
    /// Explicit user routing; omission retains learned direct/parallel selection.
    #[serde(default)]
    pub strategy: Option<crate::policy::Strategy>,
    /// Local experimental predictions only; never overrides strategy selection.
    #[serde(default)]
    pub shadow_judge: Option<crate::judge::Config>,
    #[serde(default)]
    pub autonomy: Autonomy,
    #[serde(default = "turns")]
    pub max_turns: usize,
    #[serde(default = "model_timeout")]
    pub model_timeout: u64,
    /// Retries for transient model failures (408/429/5xx, connection errors).
    #[serde(default = "model_retries")]
    pub model_retries: usize,
    /// Output token ceiling sent where the protocol requires one (Anthropic).
    #[serde(default = "max_output_tokens")]
    pub max_output_tokens: u32,
    /// Transcript bytes past which the oldest tool results are elided.
    #[serde(default = "compaction_threshold_bytes")]
    pub compaction_threshold_bytes: usize,
    /// Whole-job wall-clock deadline, enforced at every cancellation checkpoint.
    #[serde(default = "max_job_seconds")]
    pub max_job_seconds: u64,
    #[serde(default)]
    pub verifier: Vec<String>,
    #[serde(default = "timeout")]
    pub verifier_timeout: u64,
    #[serde(default)]
    pub mcp: Vec<ServerConfig>,
    #[serde(default)]
    pub plugins: Vec<crate::plugins::PluginSpec>,
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
fn model_retries() -> usize {
    crate::model::DEFAULT_RETRIES
}
fn max_output_tokens() -> u32 {
    crate::model::DEFAULT_MAX_OUTPUT_TOKENS
}
fn compaction_threshold_bytes() -> usize {
    crate::agent::DEFAULT_COMPACTION_THRESHOLD
}
fn max_job_seconds() -> u64 {
    3600
}
#[derive(Clone, Copy, Default)]
pub struct Approvals {
    pub network: bool,
    pub exec: bool,
    pub mcp: bool,
    pub plugins: bool,
}
impl Job {
    pub(crate) fn validate_model(&self) -> Result<(), String> {
        self.autonomy.validate()?;
        if self.autonomy.write_paths.is_some()
            && self
                .plugins
                .iter()
                .any(|p| !p.allowed_tools.is_empty() || !p.allowed_hooks.is_empty())
        {
            return Err("scoped autonomy cannot expose unsandboxed plugin tools or hooks".into());
        }
        if self.autonomy.write_paths.is_some() && !self.mcp.is_empty() {
            return Err("scoped autonomy cannot expose unsandboxed MCP tools; remove MCP or explicitly use unrestricted candidate file scope".into());
        }
        crate::model::validate_model_timeout(self.model_timeout)?;
        crate::model::validate_retries(self.model_retries)?;
        crate::model::validate_max_output_tokens(self.max_output_tokens)?;
        crate::agent::validate_compaction_threshold(self.compaction_threshold_bytes)?;
        if !(1..=86400).contains(&self.max_job_seconds) {
            return Err("max_job_seconds must be 1..86400".into());
        }
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
        if !job.plugins.is_empty() && !approval.plugins {
            return Err("plugins require --allow-plugins; approve only trusted bundles (tools and hooks execute host programs)".into());
        }
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
            crate::mcp::validate_timeout(server.timeout_seconds)?;
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
        for plugin in &mut job.plugins {
            if plugin.manifest.is_relative() {
                plugin.manifest = base.join(&plugin.manifest);
            }
        }
        if let Some(path) = job
            .shadow_judge
            .as_mut()
            .and_then(|c| c.model_path.as_mut())
        {
            if path.is_relative() {
                *path = base.join(&*path);
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
