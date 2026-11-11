//! Approved host integrations; approval is enforced by the caller, not MCP data.
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::{io::AsRawFd, process::CommandExt};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

const MAX_BYTES: usize = 2 * 1024 * 1024;
pub const DEFAULT_TIMEOUT_SECONDS: u64 = 30;

/// Per-request deadline bound for one server: handshake, each discovery page
/// set and each tool call. A slow analyzer gets a larger value in its own
/// entry; the transport limits stay fixed.
pub fn validate_timeout(seconds: u64) -> Result<(), String> {
    if !(1..=600).contains(&seconds) {
        return Err("MCP timeout_seconds must be 1..600".into());
    }
    Ok(())
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct McpConfig {
    pub name: String,
    pub command: Vec<String>,
    pub allowed_tools: Vec<String>,
    pub timeout_seconds: u64,
}

pub struct McpClient {
    child: Child,
    stdin: ChildStdin,
    stdout: Option<Receiver<Result<Vec<u8>, String>>>,
    stopped: bool,
    next_id: u64,
    schemas: Vec<Value>,
    timeout: Duration,
}
impl McpClient {
    pub fn connect(config: &McpConfig) -> Result<Self, String> {
        validate_timeout(config.timeout_seconds)?;
        let timeout = Duration::from_secs(config.timeout_seconds);
        let deadline = Instant::now() + timeout;
        let executable = config.command.first().ok_or("empty MCP command")?;
        let mut command = Command::new(executable);
        command
            .args(&config.command[1..])
            .env_clear()
            .process_group(0)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            // /dev/null drains stderr without retaining or exposing server secrets.
            .stderr(Stdio::null());
        for key in ["PATH", "HOME", "TMPDIR"] {
            if let Some(value) = std::env::var_os(key) {
                command.env(key, value);
            }
        }
        let mut child = command.spawn().map_err(|_| "MCP spawn failed")?;
        let (stdin, stdout) = match (child.stdin.take(), child.stdout.take()) {
            (Some(stdin), Some(stdout)) => (stdin, stdout),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return Err("MCP pipe setup failed".into());
            }
        };
        let (tx, rx) = mpsc::sync_channel(1);
        let mut client = Self {
            child,
            stdin,
            stdout: Some(rx),
            stopped: false,
            next_id: 1,
            schemas: vec![],
            timeout,
        };
        let fd = client.stdin.as_raw_fd();
        // Nonblocking writes ensure a server that stops reading cannot defeat the deadline.
        unsafe {
            let flags = libc::fcntl(fd, libc::F_GETFL);
            if flags < 0 || libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) < 0 {
                return Err("MCP pipe setup failed".into());
            }
        }
        std::thread::Builder::new()
            .name("mcp-stdout".into())
            .spawn(move || {
                let mut reader = BufReader::new(stdout);
                loop {
                    let mut line = Vec::new();
                    let read = reader
                        .by_ref()
                        .take((MAX_BYTES + 1) as u64)
                        .read_until(b'\n', &mut line);
                    let result = match read {
                        Ok(0) => Err("MCP stdout closed".into()),
                        Ok(_) if line.len() > MAX_BYTES => Err("MCP line limit exceeded".into()),
                        Ok(_) if line.last() != Some(&b'\n') => {
                            Err("unterminated MCP message".into())
                        }
                        Ok(_) => Ok(line),
                        Err(_) => Err("MCP read failed".into()),
                    };
                    let failed = result.is_err();
                    if tx.send(result).is_err() || failed {
                        break;
                    }
                }
            })
            .map_err(|_| "MCP reader setup failed")?;
        let init = client.request_inner("initialize", json!({"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"zeus","version":"0.1.0"}}), deadline)?;
        if init["protocolVersion"] != "2024-11-05" || !init["capabilities"]["tools"].is_object() {
            return Err("unsupported MCP initialization".into());
        }
        client.send_until(
            json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
            deadline,
        )?;
        let mut cursors = std::collections::HashSet::new();
        let mut names = std::collections::HashSet::new();
        let mut params = json!({});
        let mut discovery_bytes = 0;
        for _ in 0..64 {
            let result = client.request_inner("tools/list", params, deadline)?;
            discovery_bytes += serde_json::to_vec(&result)
                .map_err(|_| "invalid MCP tool list")?
                .len();
            if discovery_bytes > MAX_BYTES {
                return Err("MCP discovery limit exceeded".into());
            }
            for tool in result["tools"].as_array().ok_or("invalid tool list")? {
                let name = tool["name"]
                    .as_str()
                    .filter(|s| !s.is_empty())
                    .ok_or("invalid tool name")?;
                if !names.insert(name.to_owned()) {
                    return Err("duplicate MCP tool".into());
                }
                if config.allowed_tools.iter().any(|n| n == name) {
                    if !tool["inputSchema"].is_object() || tool["inputSchema"]["type"] != "object" {
                        return Err("invalid tool schema".into());
                    }
                    client.schemas.push(json!({"type":"function","function":{"name":name,"description":tool["description"].as_str().unwrap_or(""),"parameters":tool["inputSchema"]}}));
                }
            }
            match result.get("nextCursor") {
                None => {
                    if config.allowed_tools.iter().any(|n| !names.contains(n)) {
                        return Err("allowed MCP tool missing".into());
                    }
                    return Ok(client);
                }
                Some(cursor) => {
                    let cursor = cursor
                        .as_str()
                        .filter(|s| !s.is_empty())
                        .ok_or("invalid MCP cursor")?;
                    if !cursors.insert(cursor.to_owned()) {
                        return Err("MCP pagination loop".into());
                    }
                    params = json!({"cursor":cursor});
                }
            }
        }
        Err("MCP page limit exceeded".into())
    }
    pub fn schemas(&self) -> Vec<Value> {
        self.schemas.clone()
    }
    pub fn call(&mut self, name: &str, args: Value) -> Result<Value, String> {
        if !self.schemas.iter().any(|s| s["function"]["name"] == name) {
            return Err("MCP tool not allowed".into());
        }
        if !args.is_object() {
            return Err("MCP arguments must be an object".into());
        }
        let result = self.request("tools/call", json!({"name":name,"arguments":args}))?;
        if !result["content"].is_array() || result.get("isError").is_some_and(|v| !v.is_boolean()) {
            self.stop();
            return Err("invalid MCP tool result".into());
        }
        if result["isError"] == true {
            return Err("MCP tool reported an error".into());
        }
        Ok(result)
    }
    fn send_until(&mut self, message: Value, deadline: Instant) -> Result<(), String> {
        if self.stopped {
            return Err("MCP connection closed".into());
        }
        let mut bytes = serde_json::to_vec(&message).map_err(|_| "MCP serialization failed")?;
        bytes.push(b'\n');
        if bytes.len() > MAX_BYTES {
            return Err("MCP request limit exceeded".into());
        }
        let mut remaining = bytes.as_slice();
        while !remaining.is_empty() {
            crate::cancel::check()?;
            if Instant::now() >= deadline {
                return Err("MCP request timed out".into());
            }
            match self.stdin.write(remaining) {
                Ok(0) => return Err("MCP stdin closed".into()),
                Ok(n) => remaining = &remaining[n..],
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(2))
                }
                Err(_) => return Err("MCP write failed".into()),
            }
        }
        Ok(())
    }
    fn request(&mut self, method: &str, params: Value) -> Result<Value, String> {
        let result = self.request_inner(method, params, Instant::now() + self.timeout);
        if result.is_err() {
            self.stop();
        }
        result
    }
    fn request_inner(
        &mut self,
        method: &str,
        params: Value,
        deadline: Instant,
    ) -> Result<Value, String> {
        let id = self.next_id;
        self.next_id += 1;
        self.send_until(
            json!({"jsonrpc":"2.0","id":id,"method":method,"params":params}),
            deadline,
        )?;
        let mut received = 0;
        loop {
            crate::cancel::check()?;
            let remaining = deadline
                .checked_duration_since(Instant::now())
                .ok_or("MCP request timed out")?;
            // Wake periodically so a cancellation signal is honored mid-wait.
            let line = match self
                .stdout
                .as_ref()
                .ok_or("MCP connection closed")?
                .recv_timeout(remaining.min(Duration::from_millis(100)))
            {
                Ok(line) => line?,
                Err(mpsc::RecvTimeoutError::Timeout) => continue,
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    return Err("MCP response timed out or closed".into())
                }
            };
            received += line.len();
            if received > MAX_BYTES {
                return Err("MCP response limit exceeded".into());
            }
            let response: Value = serde_json::from_slice(&line).map_err(|_| "invalid MCP JSON")?;
            if response["jsonrpc"] != "2.0" {
                return Err("invalid MCP envelope".into());
            }
            if response.get("id").is_none() && response["method"].is_string() {
                continue;
            }
            if response["id"] != id || response.get("method").is_some() {
                return Err("unexpected MCP response id".into());
            }
            if response.get("error").is_some() {
                return Err("MCP request failed".into());
            }
            return response
                .get("result")
                .filter(|v| v.is_object())
                .cloned()
                .ok_or_else(|| "invalid MCP result".into());
        }
    }
    fn stop(&mut self) {
        if self.stopped {
            return;
        }
        self.stopped = true;
        // Kill the group before reaping its leader, preventing PID reuse here.
        unsafe {
            libc::kill(-(self.child.id() as i32), libc::SIGKILL);
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
        self.stdout.take();
    }
}
impl Drop for McpClient {
    fn drop(&mut self) {
        self.stop();
    }
}
