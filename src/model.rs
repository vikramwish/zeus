use serde_json::{json, Value};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Protocol {
    #[default]
    ChatCompletions,
    Responses,
    Anthropic,
}

const RESPONSE_LIMIT: usize = 2 * 1024 * 1024;
pub(crate) const DEFAULT_MODEL_TIMEOUT: u64 = 180;

pub(crate) fn validate_model_timeout(seconds: u64) -> Result<(), String> {
    if !(1..=900).contains(&seconds) {
        return Err("model_timeout must be 1..900 seconds".into());
    }
    Ok(())
}

// Serialize before translating/cloning a transcript, and cap the wire request too.
struct RequestBuffer(Vec<u8>);
impl std::io::Write for RequestBuffer {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if self.0.len().saturating_add(bytes.len()) > 4 * 1024 * 1024 {
            return Err(std::io::Error::other("request limit"));
        }
        self.0.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
fn bounded_json(value: &impl serde::Serialize) -> Result<Vec<u8>, String> {
    let mut body = RequestBuffer(Vec::new());
    serde_json::to_writer(&mut body, value)
        .map_err(|_| "model request exceeds 4 MiB limit".to_string())?;
    Ok(body.0)
}

#[derive(Clone)]
pub struct ChatClient {
    agent: ureq::Agent,
    endpoint: String,
    model: String,
    api_key: Option<String>,
    protocol: Protocol,
    request_budget: Option<std::sync::Arc<std::sync::atomic::AtomicUsize>>,
    timeout_seconds: u64,
    retries: usize,
}

pub(crate) const DEFAULT_RETRIES: usize = 3;
const MAX_RETRIES: usize = 8;
const RETRY_AFTER_CAP: std::time::Duration = std::time::Duration::from_secs(30);
const ERROR_BODY_LIMIT: usize = 2048;

pub(crate) fn validate_retries(retries: usize) -> Result<(), String> {
    if retries > MAX_RETRIES {
        return Err("model_retries must be 0..8".into());
    }
    Ok(())
}

/// One failed attempt. `retry_after` is Some only for transient failures
/// (408/429/5xx, DNS, connection failures, resets); deadlines and 4xx are final.
struct Failure {
    message: String,
    retry_after: Option<std::time::Duration>,
}
impl From<String> for Failure {
    fn from(message: String) -> Self {
        Self {
            message,
            retry_after: None,
        }
    }
}
impl From<&str> for Failure {
    fn from(message: &str) -> Self {
        Self::from(message.to_string())
    }
}

fn backoff(attempt: usize) -> std::time::Duration {
    std::time::Duration::from_secs(1u64 << attempt.saturating_sub(1).min(3))
}

/// Structured provider error detail for the audit: only `error.code`,
/// `error.type` and `error.message` (or a plain `error` string) from a JSON
/// body, with URLs and token-like strings scrubbed. Raw bodies are never
/// surfaced, since proxies and providers may echo credentials or signed URLs.
fn error_detail(response: ureq::Response) -> Option<String> {
    use std::io::Read;
    let mut bytes = Vec::new();
    response
        .into_reader()
        .take(ERROR_BODY_LIMIT as u64)
        .read_to_end(&mut bytes)
        .ok()?;
    let body: Value = serde_json::from_slice(&bytes).ok()?;
    let error = body.get("error")?;
    let fields: Vec<String> = match error {
        Value::String(s) => vec![scrub(s)],
        Value::Object(map) => ["code", "type", "message"]
            .iter()
            .filter_map(|k| {
                map.get(*k)
                    .and_then(Value::as_str)
                    .map(|v| format!("{k}={}", scrub(v)))
            })
            .collect(),
        _ => return None,
    };
    let detail = fields.join(" ");
    (!detail.is_empty()).then(|| detail.chars().take(512).collect())
}

fn scrub(text: &str) -> String {
    text.split_whitespace()
        .map(|word| {
            if word.contains("://") {
                "[url]"
            } else if word.len() >= 20
                && word.bytes().any(|b| b.is_ascii_digit())
                && word.bytes().any(|b| b.is_ascii_alphabetic())
                && word
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.'))
            {
                "[redacted]"
            } else {
                word
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}
impl ChatClient {
    pub fn new(endpoint: &str, model: &str, api_key: Option<String>) -> Result<Self, String> {
        Self::with_protocol(endpoint, model, api_key, Protocol::ChatCompletions)
    }
    pub fn with_protocol(
        endpoint: &str,
        model: &str,
        api_key: Option<String>,
        protocol: Protocol,
    ) -> Result<Self, String> {
        Self::with_protocol_timeout(endpoint, model, api_key, protocol, DEFAULT_MODEL_TIMEOUT)
    }
    /// Whole-request deadline, including response reads; never an automatic retry.
    pub fn with_protocol_timeout(
        endpoint: &str,
        model: &str,
        api_key: Option<String>,
        protocol: Protocol,
        timeout_seconds: u64,
    ) -> Result<Self, String> {
        validate_model_timeout(timeout_seconds)?;
        if model.trim().is_empty() || model.len() > 256 {
            return Err("invalid model name".into());
        }
        if endpoint
            .chars()
            .any(|c| c.is_whitespace() || c.is_control() || c == '\\')
        {
            return Err("invalid model endpoint".into());
        }
        if api_key
            .as_ref()
            .is_some_and(|key| key.is_empty() || !key.bytes().all(|b| (33..=126).contains(&b)))
        {
            return Err("invalid API key format".into());
        }
        let parsed = ureq::post(endpoint)
            .request_url()
            .map_err(|_| "invalid model endpoint".to_string())?;
        let mut url = parsed.as_url().clone();
        if !url.username().is_empty() || url.password().is_some() || url.fragment().is_some() {
            return Err("endpoint credentials and fragments are forbidden".into());
        }
        let host = url.host_str().ok_or("endpoint has no host")?;
        if url.scheme() == "http" {
            if host == "localhost" {
                // Pin localhost to a literal loopback address, rather than trusting DNS.
                url.set_host(Some("127.0.0.1"))
                    .map_err(|_| "invalid endpoint host")?;
            } else if !host
                .trim_matches(['[', ']'])
                .parse::<std::net::IpAddr>()
                .is_ok_and(|ip| ip.is_loopback())
            {
                return Err("HTTP requires a literal loopback address or localhost".into());
            }
        } else if url.scheme() != "https" {
            return Err("endpoint must use HTTPS or loopback HTTP".into());
        }
        Ok(Self {
            agent: ureq::AgentBuilder::new()
                .redirects(0)
                .try_proxy_from_env(false)
                // Non-streaming local inference can take over a minute to generate
                // a complete tool call. Keep a bounded whole-response deadline,
                // including reads. OS DNS lookup is not interruptible by ureq.
                .timeout(std::time::Duration::from_secs(timeout_seconds))
                .build(),
            endpoint: url.to_string(),
            model: model.into(),
            api_key,
            protocol,
            request_budget: None,
            timeout_seconds,
            // The raw client never retries; the job opts in via with_retries.
            retries: 0,
        })
    }
    /// Bounded retries for transient failures only; each attempt is one
    /// request against the shared call budget. Zero disables retrying.
    pub fn with_retries(mut self, retries: usize) -> Result<Self, String> {
        validate_retries(retries)?;
        self.retries = retries;
        Ok(self)
    }
    /// Install a run-wide attempted-request limit before cloning the client.
    /// Clones share the remaining counter; failed requests are not refunded.
    pub fn with_request_limit(mut self, limit: usize) -> Result<Self, String> {
        if !(1..=1024).contains(&limit) {
            return Err("autonomy.max_model_calls must be 1..1024".into());
        }
        self.request_budget = Some(std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(
            limit,
        )));
        Ok(self)
    }
    pub fn complete(&self, messages: &[Value], tools: &[Value]) -> Result<Value, String> {
        // Bound the original transcript before creating provider-specific copies.
        bounded_json(&(messages, tools))?;
        let mut request = self.agent.post(&self.endpoint);
        if let Some(key) = &self.api_key {
            request = match self.protocol {
                Protocol::Anthropic => request.set("x-api-key", key),
                _ => request.set("Authorization", &format!("Bearer {key}")),
            };
        }
        let mut payload = match self.protocol {
            Protocol::ChatCompletions => {
                json!({"model":self.model,"messages":messages,"stream":false})
            }
            Protocol::Responses => {
                json!({"model":self.model,"input":responses_input(messages)?,"stream":true,"store":false})
            }
            Protocol::Anthropic => {
                request = request.set("anthropic-version", "2023-06-01");
                anthropic_input(&self.model, messages)?
            }
        };
        if !tools.is_empty() {
            payload["tools"] = match self.protocol {
                Protocol::ChatCompletions => json!(tools),
                _ => json!(translate_tools(tools, self.protocol)?),
            };
        }
        let body = bounded_json(&payload)?;
        let protocol = self.protocol;
        let budget = self.request_budget.clone();
        let (retries, timeout_seconds) = (self.retries, self.timeout_seconds);
        interruptible(move || {
            let mut attempt = 0;
            loop {
                attempt += 1;
                crate::cancel::check()?;
                if let Some(budget) = &budget {
                    use std::sync::atomic::Ordering;
                    budget
                        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |left| {
                            left.checked_sub(1)
                        })
                        .map_err(|_| "autonomy model-call budget exhausted")?;
                }
                let failure = match exchange(request.clone(), &body, protocol, timeout_seconds) {
                    Ok(message) => return Ok(message),
                    Err(failure) => failure,
                };
                match failure.retry_after {
                    Some(delay) if attempt <= retries => {
                        let until = std::time::Instant::now() + delay;
                        while std::time::Instant::now() < until {
                            crate::cancel::check()?;
                            std::thread::sleep(std::time::Duration::from_millis(50));
                        }
                    }
                    _ if attempt > 1 => {
                        return Err(format!("{} (after {attempt} attempts)", failure.message))
                    }
                    _ => return Err(failure.message),
                }
            }
        })
    }
}

/// Send and parse one request on a helper thread so a cancellation signal is
/// honored while the socket read blocks (a blocking read restarts after EINTR).
/// An abandoned request stays bounded by the agent's whole-request deadline.
fn interruptible(
    perform: impl FnOnce() -> Result<Value, String> + Send + 'static,
) -> Result<Value, String> {
    let (tx, rx) = std::sync::mpsc::sync_channel(1);
    let spawned = std::thread::Builder::new()
        .name("model-request".into())
        .spawn(move || {
            let _ = tx.send(perform());
        });
    if spawned.is_err() {
        return Err("cannot start model request thread".into());
    }
    loop {
        crate::cancel::check()?;
        match rx.recv_timeout(std::time::Duration::from_millis(50)) {
            Ok(result) => return result,
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                return Err("model request thread ended without a result".into())
            }
        }
    }
}

fn exchange(
    request: ureq::Request,
    body: &[u8],
    protocol: Protocol,
    timeout_seconds: u64,
) -> Result<Value, Failure> {
    let response = request
        .set("Content-Type", "application/json")
        .send_bytes(body)
        .map_err(|error| match error {
            ureq::Error::Status(status, response) => {
                let retry_after = matches!(status, 408 | 429 | 500 | 502 | 503 | 504).then(|| {
                    response
                        .header("Retry-After")
                        .and_then(|v| v.trim().parse::<u64>().ok())
                        .map(std::time::Duration::from_secs)
                        .map(|d| d.min(RETRY_AFTER_CAP))
                        .unwrap_or_else(|| backoff(1))
                });
                let message = match error_detail(response) {
                    Some(detail) => format!("model request failed: HTTP {status}: {detail}"),
                    None => format!("model request failed: HTTP {status}"),
                };
                Failure {
                    message,
                    retry_after,
                }
            }
            ureq::Error::Transport(error) => {
                use std::error::Error;
                let io_kind = error
                    .source()
                    .and_then(|s| s.downcast_ref::<std::io::Error>())
                    .map(std::io::Error::kind);
                let detail = error.message().unwrap_or("").to_ascii_lowercase();
                let timed_out = matches!(
                    io_kind,
                    Some(std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock)
                ) || detail.contains("timed out")
                    || detail.contains("timeout");
                if timed_out {
                    return Failure::from(format!(
                        "model request timed out after {timeout_seconds}s"
                    ));
                }
                let transient = matches!(
                    error.kind(),
                    ureq::ErrorKind::Dns | ureq::ErrorKind::ConnectionFailed | ureq::ErrorKind::Io
                );
                Failure {
                    message: format!(
                        "model request failed: {}{}",
                        error.kind(),
                        error
                            .message()
                            .map(|m| format!(": {m}"))
                            .unwrap_or_default()
                    ),
                    retry_after: transient.then(|| backoff(1)),
                }
            }
        })?;
    if response.status() != 200 {
        return Err("model returned a non-success status".into());
    }
    use std::io::Read;
    if protocol == Protocol::Responses {
        if response.header("Content-Type").is_none_or(|v| {
            !v.split(';')
                .next()
                .unwrap_or("")
                .trim()
                .eq_ignore_ascii_case("text/event-stream")
        }) {
            return Err("expected Responses event stream".into());
        }
        return responses_stream(response.into_reader()).map_err(Failure::from);
    }
    let mut bytes = Vec::new();
    response
        .into_reader()
        .take((RESPONSE_LIMIT + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|_| Failure::from("failed to read model response"))?;
    if bytes.len() > RESPONSE_LIMIT {
        return Err("model response exceeds 2 MiB limit".into());
    }
    let body: Value =
        serde_json::from_slice(&bytes).map_err(|_| Failure::from("invalid model JSON"))?;
    match protocol {
        Protocol::Anthropic => anthropic_output(&body),
        _ => chat_message(&body),
    }
    .map_err(Failure::from)
}

fn chat_message(body: &Value) -> Result<Value, String> {
    let choices = body
        .get("choices")
        .and_then(Value::as_array)
        .filter(|c| c.len() == 1)
        .ok_or("expected exactly one model choice")?;
    let choice = &choices[0];
    let message = choice.get("message").ok_or("missing assistant message")?;
    if message.get("role").and_then(Value::as_str) != Some("assistant") {
        return Err("expected assistant role".into());
    }
    if message.get("function_call").is_some_and(|c| !c.is_null()) {
        return Err("legacy function_call is unsupported; use tool_calls".into());
    }
    let content = message.get("content");
    if content.is_some_and(|c| !c.is_null() && !c.is_string()) {
        return Err("assistant content must be text or null".into());
    }
    let calls = match message.get("tool_calls") {
        None | Some(Value::Null) => None,
        Some(Value::Array(calls)) => Some(calls),
        _ => return Err("tool_calls must be an array".into()),
    };
    if calls.is_none_or(|c| c.is_empty())
        && content
            .and_then(Value::as_str)
            .is_none_or(|s| s.trim().is_empty())
    {
        return Err("assistant response has neither tools nor nonempty text".into());
    }
    if let Some(reason) = choice.get("finish_reason") {
        let expected = if calls.is_some_and(|c| !c.is_empty()) {
            "tool_calls"
        } else {
            "stop"
        };
        if reason != expected {
            return Err("finish reason contradicts assistant message".into());
        }
    }
    Ok(message.clone())
}

fn anthropic_input(model: &str, messages: &[Value]) -> Result<Value, String> {
    let mut system = Vec::new();
    let mut turns: Vec<Value> = Vec::new();
    for message in messages {
        let role = text(message, "role")?;
        if role == "system" {
            if !turns.is_empty() {
                return Err("Anthropic system messages must precede conversation".into());
            }
            system.push(json!({"type":"text","text":text(message,"content")?}));
            continue;
        }
        let mut blocks = Vec::new();
        let role = match role {
            "tool" => {
                blocks.push(json!({"type":"tool_result","tool_use_id":text(message,"tool_call_id")?,"content":text(message,"content")?}));
                "user"
            }
            "user" => {
                blocks.push(json!({"type":"text","text":text(message,"content")?}));
                "user"
            }
            "assistant" => {
                if let Some(content) = message.get("content").filter(|v| !v.is_null()) {
                    let content = content.as_str().ok_or("unsupported assistant content")?;
                    if !content.is_empty() {
                        blocks.push(json!({"type":"text","text":content}));
                    }
                }
                if let Some(calls) = message.get("tool_calls").filter(|v| !v.is_null()) {
                    for call in calls.as_array().ok_or("invalid tool calls")? {
                        if call["type"] != "function" {
                            return Err("unsupported tool call type".into());
                        }
                        let args: Value =
                            serde_json::from_str(text(&call["function"], "arguments")?)
                                .map_err(|_| "invalid tool arguments")?;
                        if !args.is_object() {
                            return Err("tool arguments must be an object".into());
                        }
                        blocks.push(json!({"type":"tool_use","id":text(call,"id")?,"name":text(&call["function"],"name")?,"input":args}));
                    }
                }
                if let Some(original) = message.get("_anthropic_content") {
                    let stop = if message["tool_calls"]
                        .as_array()
                        .is_some_and(|c| !c.is_empty())
                    {
                        "tool_use"
                    } else {
                        "end_turn"
                    };
                    let canonical = anthropic_output(
                        &json!({"type":"message","role":"assistant","content":original,"stop_reason":stop}),
                    )?;
                    if canonical["content"] != message["content"]
                        || canonical["tool_calls"] != message["tool_calls"]
                    {
                        return Err("Anthropic replay does not match assistant message".into());
                    }
                    blocks = array(&canonical, "_anthropic_content")?.clone();
                }
                "assistant"
            }
            _ => return Err("unsupported Anthropic message role".into()),
        };
        if turns.last().is_some_and(|m| m["role"] == role) {
            turns.last_mut().unwrap()["content"]
                .as_array_mut()
                .unwrap()
                .extend(blocks);
        } else {
            turns.push(json!({"role":role,"content":blocks}));
        }
    }
    let mut payload = json!({"model":model,"messages":turns,"max_tokens":4096,"stream":false});
    if !system.is_empty() {
        payload["system"] = json!(system);
    }
    Ok(payload)
}

fn anthropic_output(body: &Value) -> Result<Value, String> {
    if body["type"] != "message" || body["role"] != "assistant" {
        return Err("invalid Anthropic message".into());
    }
    let mut content = String::new();
    let mut calls = Vec::new();
    let mut replay = Vec::new();
    for block in array(body, "content")? {
        match text(block, "type")? {
            "text" => {
                let value = text(block, "text")?;
                content.push_str(value);
                replay.push(json!({"type":"text","text":value}));
            }
            "tool_use" => {
                if !block["input"].is_object() {
                    return Err("tool input must be an object".into());
                }
                calls.push(json!({"type":"function","id":text(block,"id")?,"function":{"name":text(block,"name")?,"arguments":block["input"].to_string()}}));
                replay.push(json!({"type":"tool_use","id":block["id"],"name":block["name"],"input":block["input"]}));
            }
            // Extended thinking / server-side tools need additional replay semantics.
            _ => return Err("unsupported Anthropic content capability".into()),
        }
    }
    let expected = if calls.is_empty() {
        "end_turn"
    } else {
        "tool_use"
    };
    if body["stop_reason"] != expected {
        return Err("incomplete or unsupported Anthropic stop reason".into());
    }
    chat_message(
        &json!({"choices":[{"message":{"role":"assistant","content":content,"tool_calls":calls,"_anthropic_content":replay}}]}),
    )
}

fn text<'a>(v: &'a Value, key: &str) -> Result<&'a str, String> {
    v.get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| "unsupported or malformed protocol text".into())
}
fn array<'a>(v: &'a Value, key: &str) -> Result<&'a Vec<Value>, String> {
    v.get(key)
        .and_then(Value::as_array)
        .ok_or_else(|| "malformed protocol array".into())
}
fn translate_tools(tools: &[Value], protocol: Protocol) -> Result<Vec<Value>, String> {
    tools.iter().map(|tool| {
        if tool["type"] != "function" { return Err("only function tools are supported".into()); }
        let function = &tool["function"];
        let name = text(function, "name")?;
        if !function["parameters"].is_object() { return Err("function parameters must be an object".into()); }
        let mut translated = if protocol == Protocol::Responses {
            // Preserve the host's non-strict schemas instead of letting a provider
            // normalize optional arguments into required properties.
            json!({"type":"function","name":name,"parameters":function["parameters"],"strict":false})
        } else {
            json!({"name":name,"input_schema":function["parameters"]})
        };
        for key in ["description", "strict"] {
            if let Some(value) = function.get(key) { translated[key] = value.clone(); }
        }
        Ok(translated)
    }).collect()
}

fn responses_input(messages: &[Value]) -> Result<Vec<Value>, String> {
    let mut input = Vec::new();
    for message in messages {
        match text(message, "role")? {
            "system" | "developer" | "user" => input.push(json!({"role":message["role"],"content":text(message,"content")?})),
            "tool" => input.push(json!({"type":"function_call_output","call_id":text(message,"tool_call_id")?,"output":text(message,"content")?})),
            "assistant" => {
                if let Some(output) = message.get("_responses_output") {
                    // Metadata is only replayable if it describes exactly the exposed
                    // canonical message. Never hide tool calls from agent validation.
                    let canonical = responses_output(output)?;
                    if canonical["content"] != message["content"] || canonical["tool_calls"] != message["tool_calls"] {
                        return Err("Responses replay does not match assistant message".into());
                    }
                    input.extend(array(&canonical,"_responses_output")?.iter().cloned());
                } else {
                    if let Some(content) = message.get("content").filter(|v| !v.is_null()) {
                        let content = content.as_str().ok_or("unsupported assistant content")?;
                        input.push(json!({"role":"assistant","content":content}));
                    }
                    if let Some(calls) = message.get("tool_calls").filter(|v| !v.is_null()) {
                        for call in calls.as_array().ok_or("invalid tool calls")? {
                            if call["type"] != "function" { return Err("unsupported tool call type".into()); }
                            input.push(json!({"type":"function_call","call_id":text(call,"id")?,"name":text(&call["function"],"name")?,"arguments":text(&call["function"],"arguments")?}));
                        }
                    }
                }
            }
            _ => return Err("unsupported message role".into()),
        }
    }
    Ok(input)
}

fn responses_output(output: &Value) -> Result<Value, String> {
    let items = output.as_array().ok_or("missing Responses output")?;
    let mut content = String::new();
    let mut calls = Vec::new();
    let mut replay = Vec::new();
    for item in items {
        if item.get("status").is_some_and(|v| v != "completed") {
            return Err("incomplete Responses output item".into());
        }
        let mut clean = match text(item, "type")? {
            "function_call" => {
                let id = text(item, "call_id")?;
                let name = text(item, "name")?;
                let arguments = text(item, "arguments")?;
                calls.push(json!({"id":id,"type":"function","function":{"name":name,"arguments":arguments}}));
                json!({"type":"function_call","call_id":id,"name":name,"arguments":arguments})
            }
            "message" => {
                if item["role"] != "assistant" {
                    return Err("expected assistant role".into());
                }
                let mut parts = Vec::new();
                for part in array(item, "content")? {
                    match text(part, "type")? {
                        "output_text" => {
                            let value = text(part, "text")?;
                            content.push_str(value);
                            parts.push(json!({"type":"output_text","text":value,"annotations":[]}));
                        }
                        "refusal" => {
                            let value = text(part, "refusal")?;
                            content.push_str(value);
                            parts.push(json!({"type":"refusal","refusal":value}));
                        }
                        _ => return Err("unsupported Responses content capability".into()),
                    }
                }
                let mut message = json!({"type":"message","role":"assistant","content":parts});
                if let Some(phase) = item.get("phase") {
                    if !phase.is_null()
                        && !matches!(phase.as_str(), Some("commentary" | "final_answer"))
                    {
                        return Err("invalid Responses message phase".into());
                    }
                    message["phase"] = phase.clone();
                }
                message
            }
            "reasoning" => {
                let mut reasoning = json!({"type":"reasoning","summary":array(item,"summary")?});
                for part in array(item, "summary")? {
                    if part["type"] != "summary_text" {
                        return Err("unsupported reasoning summary".into());
                    }
                    text(part, "text")?;
                }
                if let Some(encrypted) = item.get("encrypted_content") {
                    if !encrypted.is_null() && !encrypted.is_string() {
                        return Err("invalid encrypted reasoning".into());
                    }
                    reasoning["encrypted_content"] = encrypted.clone();
                }
                if let Some(parts) = item.get("content") {
                    for part in parts.as_array().ok_or("invalid reasoning content")? {
                        if part["type"] != "reasoning_text" {
                            return Err("unsupported reasoning content".into());
                        }
                        text(part, "text")?;
                    }
                    reasoning["content"] = parts.clone();
                }
                reasoning
            }
            _ => return Err("unsupported Responses output capability".into()),
        };
        if let Some(id) = item.get("id") {
            if !id.is_string() {
                return Err("invalid Responses item ID".into());
            }
            clean["id"] = id.clone();
        }
        if item.get("status").is_some() {
            clean["status"] = json!("completed");
        }
        replay.push(clean);
    }
    let message =
        json!({"role":"assistant","content":content,"tool_calls":calls,"_responses_output":replay});
    chat_message(&json!({"choices":[{"message":message}]}))
}

// Incrementally frame SSE, but expose no text or tools until a completed response.
// The take limit bounds even a malicious unterminated line; ureq bounds wall time.
fn responses_stream(reader: impl std::io::Read) -> Result<Value, String> {
    use std::io::{BufRead, BufReader};
    let mut reader = BufReader::new(reader.take((RESPONSE_LIMIT + 1) as u64));
    let mut total = 0;
    let mut data = String::new();
    let mut event = String::new();
    loop {
        let mut line = String::new();
        let count = reader
            .read_line(&mut line)
            .map_err(|_| "failed to read Responses stream")?;
        total += count;
        if total > RESPONSE_LIMIT {
            return Err("model response exceeds 2 MiB limit".into());
        }
        if count == 0 {
            return Err("Responses stream ended without response.completed".into());
        }
        let line = line.strip_suffix('\n').unwrap_or(&line);
        let line = line.strip_suffix('\r').unwrap_or(line);
        if line.is_empty() {
            if data.is_empty() {
                event.clear();
                continue;
            }
            let value: Value =
                serde_json::from_str(&data).map_err(|_| "invalid Responses event JSON")?;
            let kind = text(&value, "type")?;
            if !event.is_empty() && event != kind {
                return Err("inconsistent Responses event type".into());
            }
            match kind {
                "response.completed" => {
                    let response = &value["response"];
                    if response["status"] != "completed"
                        || response.get("error").is_some_and(|e| !e.is_null())
                    {
                        return Err("Responses completion is not successful".into());
                    }
                    return responses_output(&response["output"]);
                }
                "response.failed" | "response.incomplete" | "error" => {
                    return Err("Responses inference failed or incomplete".into())
                }
                kind if kind.starts_with("response.") => {}
                _ => return Err("unsupported Responses event".into()),
            }
            data.clear();
            event.clear();
        } else if let Some(value) = line.strip_prefix("data:") {
            data.push_str(value.strip_prefix(' ').unwrap_or(value));
            data.push('\n');
        } else if let Some(value) = line.strip_prefix("event:") {
            if !event.is_empty() {
                return Err("duplicate Responses event type".into());
            }
            event = value.strip_prefix(' ').unwrap_or(value).into();
        } else if !line.starts_with(':') && !line.starts_with("id:") && !line.starts_with("retry:")
        {
            return Err("invalid Responses stream framing".into());
        }
    }
}
