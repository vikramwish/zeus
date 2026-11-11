//! Explicitly approved host bundles; manifest and skill text are untrusted data.
//!
//! Version-1 manifests declare `name`, optional `description`, `command`, `tools`,
//! `skills: [{name, path}]`, and `hooks: {event: tool}`. Hook targets must also be
//! declared in `tools`; approving a hook does not expose its target to the model.
//! `allowed_hooks` contains event names, not tool names. Only run_start/run_end/
//! run_error are supported. `${PLUGIN_DIR}` is literal argv substitution, not a shell.
//!
//! Limits: 16 bundles, 64 declared tools and 64 skills in aggregate, 128 KiB per
//! manifest, 16 KiB per UTF-8 skill, 32 KiB serialized catalog, 4096 description
//! bytes, and 1..600 seconds per MCP request (default 30). Skills are cached at
//! load, labeled untrusted, and excluded from catalog content. Host execution
//! approval belongs to the caller; loading never starts a process.
use serde::Deserialize;
use serde_json::{json, Value};
use std::fs::{File, OpenOptions};
use std::io::Read;
use std::os::fd::{AsRawFd, FromRawFd};
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::path::{Component, Path, PathBuf};

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PluginSpec {
    pub manifest: PathBuf,
    #[serde(default)]
    pub allowed_tools: Vec<String>,
    #[serde(default)]
    pub allowed_hooks: Vec<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    version: u32,
    name: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    command: Vec<String>,
    #[serde(default = "default_timeout")]
    timeout_seconds: u64,
    #[serde(default)]
    tools: Vec<String>,
    #[serde(default, deserialize_with = "unique_hooks")]
    hooks: std::collections::BTreeMap<String, String>,
    #[serde(default)]
    skills: Vec<SkillSpec>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SkillSpec {
    name: String,
    path: PathBuf,
}
struct Skill {
    metadata: Value,
    content: String,
}
struct LoadedPlugin {
    config: crate::mcp::McpConfig,
    exposed: Vec<String>,
    hooks: std::collections::BTreeMap<String, String>,
}
struct ConnectedPlugin {
    loaded: LoadedPlugin,
    client: crate::mcp::McpClient,
}
pub struct PluginSet {
    skills: Vec<Skill>,
    plugins: Vec<LoadedPlugin>,
}
pub struct PluginRuntime {
    skills: Vec<Skill>,
    plugins: Vec<ConnectedPlugin>,
}
// Resolve existing ancestors, including symlinks, without requiring state to exist.
fn resolved_path(path: &Path) -> Result<PathBuf, String> {
    let absolute = if path.is_absolute() {
        path.to_owned()
    } else {
        std::env::current_dir()
            .map_err(|e| e.to_string())?
            .join(path)
    };
    let mut result = PathBuf::new();
    for part in absolute.components() {
        match part {
            Component::ParentDir => {
                result.pop();
            }
            Component::CurDir => {}
            _ => {
                result.push(part.as_os_str());
                match std::fs::symlink_metadata(&result) {
                    Ok(_) => {
                        result = result
                            .canonicalize()
                            .map_err(|e| format!("plugin path: {e}"))?
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                    Err(e) => return Err(format!("plugin path: {e}")),
                }
            }
        }
    }
    Ok(result)
}
fn bounded_file(mut file: File, limit: usize) -> Result<Vec<u8>, String> {
    let meta = file.metadata().map_err(|e| e.to_string())?;
    if !meta.is_file() || meta.nlink() != 1 {
        return Err("plugin file must be regular, without hardlinks".into());
    }
    if meta.len() > limit as u64 {
        return Err("plugin file size limit exceeded".into());
    }
    let mut bytes = Vec::new();
    file.by_ref()
        .take((limit + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > limit {
        return Err("plugin file size limit exceeded".into());
    }
    Ok(bytes)
}
fn read_regular(path: &Path, limit: usize) -> Result<Vec<u8>, String> {
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)
        .map_err(|e| format!("plugin file: {e}"))?;
    bounded_file(file, limit)
}
fn read_skill_file(directory: &Path, relative: &Path) -> Result<Vec<u8>, String> {
    let parts: Vec<_> = relative.components().collect();
    if parts.is_empty() || parts.iter().any(|p| !matches!(p, Component::Normal(_))) {
        return Err("plugin skill path must be relative without traversal".into());
    }
    let mut file = File::open(directory).map_err(|e| e.to_string())?;
    for (i, part) in parts.iter().enumerate() {
        use std::os::unix::ffi::OsStrExt;
        let name = std::ffi::CString::new(part.as_os_str().as_bytes())
            .map_err(|_| "invalid plugin skill path")?;
        let flags = libc::O_RDONLY
            | libc::O_NOFOLLOW
            | libc::O_NONBLOCK
            | libc::O_CLOEXEC
            | if i + 1 < parts.len() {
                libc::O_DIRECTORY
            } else {
                0
            };
        // Each component is opened relative to an owned descriptor, never following links.
        let fd = unsafe { libc::openat(file.as_raw_fd(), name.as_ptr(), flags) };
        if fd < 0 {
            return Err(format!(
                "plugin skill file: {}",
                std::io::Error::last_os_error()
            ));
        }
        file = unsafe { File::from_raw_fd(fd) };
    }
    bounded_file(file, 16 * 1024)
}
fn unique_hooks<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<std::collections::BTreeMap<String, String>, D::Error> {
    struct HooksVisitor;
    impl<'de> serde::de::Visitor<'de> for HooksVisitor {
        type Value = std::collections::BTreeMap<String, String>;
        fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            f.write_str("unique hook events")
        }
        fn visit_map<M: serde::de::MapAccess<'de>>(
            self,
            mut map: M,
        ) -> Result<Self::Value, M::Error> {
            let mut hooks = std::collections::BTreeMap::new();
            while let Some((event, tool)) = map.next_entry::<String, String>()? {
                if hooks.insert(event, tool).is_some() {
                    return Err(serde::de::Error::custom("duplicate plugin hook event"));
                }
            }
            Ok(hooks)
        }
    }
    deserializer.deserialize_map(HooksVisitor)
}
fn default_timeout() -> u64 {
    30
}
fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}
fn valid_event(event: &str) -> bool {
    matches!(event, "run_start" | "run_end" | "run_error")
}
fn validate_manifest(m: &Manifest, spec: &PluginSpec) -> Result<(), String> {
    if m.version != 1 {
        return Err("unsupported plugin manifest version".into());
    }
    if !valid_name(&m.name) || m.name.len() > 16 || m.name.contains("__") {
        return Err("invalid plugin name".into());
    }
    if m.description.len() > 4096 {
        return Err("plugin description limit exceeded".into());
    }
    crate::mcp::validate_timeout(m.timeout_seconds)?;
    if let Some(command) = m.command.first() {
        if !Path::new(command).is_absolute() && !command.starts_with("${PLUGIN_DIR}/") {
            return Err("plugin executable must be absolute".into());
        }
    } else if !m.tools.is_empty() || !m.hooks.is_empty() {
        return Err("plugin tools require a command".into());
    }
    let mut tools = std::collections::HashSet::new();
    for tool in &m.tools {
        if !valid_name(tool) || format!("plugin__{}__{}", m.name, tool).len() > 64 {
            return Err("invalid plugin tool name".into());
        }
        if !tools.insert(tool) {
            return Err("duplicate plugin tool".into());
        }
    }
    for (event, tool) in &m.hooks {
        if !valid_event(event) || !tools.contains(tool) {
            return Err("invalid plugin hook declaration".into());
        }
    }
    let mut allowed = std::collections::HashSet::new();
    for tool in &spec.allowed_tools {
        if !tools.contains(tool) || !allowed.insert(tool) {
            return Err("invalid or duplicate plugin allowed_tools".into());
        }
    }
    let mut hooks = std::collections::HashSet::new();
    for event in &spec.allowed_hooks {
        if !m.hooks.contains_key(event) || !hooks.insert(event) {
            return Err("invalid or duplicate plugin allowed_hooks".into());
        }
    }
    Ok(())
}
impl PluginSet {
    pub fn load(specs: &[PluginSpec], source: &Path, state: &Path) -> Result<Self, String> {
        use sha2::{Digest, Sha256};
        let mut skills = Vec::new();
        let mut plugins = Vec::new();
        if specs.len() > 16 {
            return Err("plugin limit exceeded (16)".into());
        }
        let mut ids = std::collections::HashSet::new();
        let mut qualified_tools = std::collections::HashSet::new();
        let mut tool_count = 0;
        if specs.is_empty() {
            return Ok(Self { skills, plugins });
        }
        let source = resolved_path(source)?;
        let state = resolved_path(state)?;
        for spec in specs {
            let path = resolved_path(&spec.manifest)?;
            if path.starts_with(&source) || path.starts_with(&state) {
                return Err("plugin manifest must be outside source and state directories".into());
            }
            let bytes = read_regular(&spec.manifest, 128 * 1024)?;
            let manifest: Manifest = serde_json::from_slice(&bytes)
                .map_err(|e| format!("invalid plugin manifest: {e}"))?;
            let directory = path.parent().ok_or("plugin manifest has no directory")?;
            validate_manifest(&manifest, spec)?;
            if !ids.insert(manifest.name.clone()) {
                return Err("duplicate plugin ID".into());
            }
            for tool in &spec.allowed_tools {
                let qualified = format!("plugin__{}__{tool}", manifest.name);
                if !qualified_tools.insert(qualified.clone()) {
                    return Err(format!("duplicate plugin qualified tool name: {qualified}"));
                }
            }
            tool_count += manifest.tools.len();
            if tool_count > 64 || skills.len() + manifest.skills.len() > 64 {
                return Err("plugin tool or skill count limit exceeded (64)".into());
            }
            let mut approved = spec.allowed_tools.clone();
            let mut hooks = std::collections::BTreeMap::new();
            for event in &spec.allowed_hooks {
                let tool = manifest.hooks[event].clone();
                if !approved.contains(&tool) {
                    approved.push(tool.clone());
                }
                hooks.insert(event.clone(), tool);
            }
            let directory_text = directory.to_str().ok_or("plugin directory must be UTF-8")?;
            let command = manifest
                .command
                .iter()
                .map(|arg| arg.replace("${PLUGIN_DIR}", directory_text))
                .collect();
            plugins.push(LoadedPlugin {
                config: crate::mcp::McpConfig {
                    name: manifest.name.clone(),
                    command,
                    allowed_tools: approved,
                    timeout_seconds: manifest.timeout_seconds,
                },
                exposed: spec.allowed_tools.clone(),
                hooks,
            });
            let mut names = std::collections::HashSet::new();
            for skill in manifest.skills {
                if !valid_name(&skill.name)
                    || skill.name.len() > 64
                    || !names.insert(skill.name.clone())
                {
                    return Err("invalid or duplicate plugin skill name".into());
                }
                let content = String::from_utf8(read_skill_file(directory, &skill.path)?)
                    .map_err(|_| "plugin skill is not UTF-8")?;
                let name = format!("{}/{}", manifest.name, skill.name);
                let source_id = format!("plugin:{name}#{:x}", Sha256::digest(content.as_bytes()));
                skills.push(Skill { metadata: json!({"name":name,"plugin":manifest.name,"source_id":source_id,"authority":"untrusted_plugin_data"}), content });
            }
        }
        let catalog = json!({"skills":skills.iter().map(|s| &s.metadata).collect::<Vec<_>>(),"skipped":[],"truncated":false});
        if serde_json::to_vec(&catalog)
            .map_err(|e| e.to_string())?
            .len()
            > 32 * 1024
        {
            return Err("plugin skill catalog size limit exceeded".into());
        }
        Ok(Self { skills, plugins })
    }
    /// Starts only selected capabilities, after every manifest has been validated by load.
    /// The caller must obtain explicit host-execution approval before calling this.
    pub fn connect(self) -> Result<PluginRuntime, String> {
        let mut plugins = Vec::new();
        for loaded in self.plugins {
            if loaded.config.allowed_tools.is_empty() {
                continue;
            }
            crate::cancel::check()?;
            let client = crate::mcp::McpClient::connect(&loaded.config)
                .map_err(|e| format!("plugin {} connect: {e}", loaded.config.name))?;
            plugins.push(ConnectedPlugin { loaded, client });
        }
        Ok(PluginRuntime {
            skills: self.skills,
            plugins,
        })
    }
}
impl PluginRuntime {
    pub fn empty() -> Self {
        Self {
            skills: vec![],
            plugins: vec![],
        }
    }
    pub fn schemas(&self) -> Vec<Value> {
        let mut schemas = Vec::new();
        for plugin in &self.plugins {
            for mut schema in plugin.client.schemas() {
                let name = schema["function"]["name"].as_str().unwrap_or("").to_owned();
                if plugin.loaded.exposed.contains(&name) {
                    schema["function"]["name"] =
                        json!(format!("plugin__{}__{name}", plugin.loaded.config.name));
                    schemas.push(schema);
                }
            }
        }
        schemas
    }
    pub fn call(&mut self, name: &str, args: Value) -> Result<Value, String> {
        for plugin in &mut self.plugins {
            for tool in &plugin.loaded.exposed {
                if name == format!("plugin__{}__{tool}", plugin.loaded.config.name) {
                    crate::cancel::check()?;
                    return plugin.client.call(tool, args).map_err(|e| {
                        format!("plugin {} tool {tool}: {e}", plugin.loaded.config.name)
                    });
                }
            }
        }
        Err("plugin tool not allowed".into())
    }
    pub fn hook(&mut self, event: &str, payload: Value) -> Result<Vec<Value>, String> {
        self.hook_observed(event, payload, |_, _| Ok(()))
    }
    /// Observe each handler before dispatch and immediately after completion.
    /// A failed observer stops dispatch, so hosts can require durable audit writes.
    pub fn hook_observed(
        &mut self,
        event: &str,
        payload: Value,
        mut observe: impl FnMut(&str, Value) -> Result<(), String>,
    ) -> Result<Vec<Value>, String> {
        if !valid_event(event) {
            return Err("unknown plugin hook event".into());
        }
        let mut records = Vec::new();
        for plugin in &mut self.plugins {
            if let Some(tool) = plugin.loaded.hooks.get(event) {
                crate::cancel::check()?;
                observe(
                    "plugin_hook_request",
                    json!({"plugin":plugin.loaded.config.name,"event":event,"tool":tool,"payload":payload}),
                )?;
                let result = plugin.client.call(tool, payload.clone()).map_err(|e| {
                    format!(
                        "plugin {} hook {event} ({tool}): {e}",
                        plugin.loaded.config.name
                    )
                });
                observe(
                    "plugin_hook_result",
                    json!({"plugin":plugin.loaded.config.name,"event":event,"tool":tool,"result":result}),
                )?;
                let result = result?;
                records.push(
                    json!({"plugin":plugin.loaded.config.name,"event":event,"result":result}),
                );
            }
        }
        Ok(records)
    }
    pub fn skill_catalog(&self) -> Value {
        json!({"skills":self.skills.iter().map(|s| &s.metadata).collect::<Vec<_>>(),"skipped":[],"truncated":false})
    }
    pub fn read_skill(&self, name: &str) -> Result<Value, String> {
        let skill = self
            .skills
            .iter()
            .find(|s| s.metadata["name"] == name)
            .ok_or("unknown plugin skill")?;
        let mut result = skill.metadata.clone();
        result["content"] = json!(skill.content);
        Ok(result)
    }
}
