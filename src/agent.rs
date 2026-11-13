use crate::model::ChatClient;
use serde::Serialize;
use serde_json::Value;

pub trait ToolHost: Send + Sync {
    fn schemas(&self) -> Vec<Value>;
    fn call(&self, name: &str, args: Value) -> Result<Value, String>;
    /// Durable trace of loop progress (each model turn), recorded before any
    /// tool runs so a later failure cannot erase what the model said.
    fn observe(&self, _kind: &str, _data: Value) -> Result<(), String> {
        Ok(())
    }
    /// Transcript size past which the oldest tool results are elided.
    fn compaction_threshold(&self) -> usize {
        DEFAULT_COMPACTION_THRESHOLD
    }
}
#[derive(Clone, Serialize, Debug)]
pub struct AgentResult {
    pub text: String,
    pub messages: Vec<Value>,
    pub tool_calls: usize,
}
const MAX_TURNS: usize = 64;
const MAX_TOOL_CALLS: usize = 64;
const MAX_CONTEXT_BYTES: usize = 4 * 1024 * 1024;
const MAX_TOOL_RESULT_BYTES: usize = 256 * 1024;
const MAX_CONSECUTIVE_TRUNCATIONS: usize = 1;
pub const DEFAULT_COMPACTION_THRESHOLD: usize = 1024 * 1024;
pub const MIN_COMPACTION_THRESHOLD: usize = 64 * 1024;
/// Recent tool results that compaction never touches (Hermes protect_last_n).
const PROTECT_LAST_TOOL_RESULTS: usize = 4;

pub fn validate_compaction_threshold(bytes: usize) -> Result<(), String> {
    if !(MIN_COMPACTION_THRESHOLD..=MAX_CONTEXT_BYTES).contains(&bytes) {
        return Err("compaction_threshold_bytes must be 65536..4194304".into());
    }
    Ok(())
}

fn is_elided(message: &Value) -> bool {
    message["content"]
        .as_str()
        .is_some_and(|c| c.starts_with("{\"elided\":true"))
}

/// Replace the oldest unprotected tool results with a short placeholder so the
/// transcript fits. Tool call/result pairs stay intact; the model can re-run a
/// tool if it needs the data again. Returns how many results were elided.
fn compact(messages: &mut [Value], bytes: &mut usize, keep_last: usize) -> Result<usize, String> {
    let candidates: Vec<usize> = messages
        .iter()
        .enumerate()
        .filter(|(_, m)| m["role"] == "tool" && !is_elided(m))
        .map(|(i, _)| i)
        .collect();
    if candidates.len() <= keep_last {
        return Ok(0);
    }
    let elide = &candidates[..candidates.len() - keep_last];
    for &index in elide {
        let omitted = messages[index]["content"].as_str().map_or(0, str::len);
        let placeholder = serde_json::json!({"elided":true,"omitted_bytes":omitted,"note":"earlier tool result removed to fit the context window; call the tool again if its content is needed"});
        messages[index] = serde_json::json!({"role":"tool","tool_call_id":messages[index]["tool_call_id"],"content":placeholder.to_string()});
    }
    *bytes = 2;
    for message in messages.iter() {
        let len = crate::output::json_len_within(message, usize::MAX)
            .map_err(|_| "invalid context JSON")?
            .unwrap_or(0);
        *bytes += len + 1;
    }
    Ok(elide.len())
}

/// Provider rejections that mean the transcript itself is too large.
fn is_context_overflow(error: &str) -> bool {
    let e = error.to_ascii_lowercase();
    e.contains("context_length_exceeded")
        || e.contains("context window exceeded")
        || e.contains("maximum context length")
        || e.contains("prompt is too long")
}

fn push_message(
    messages: &mut Vec<Value>,
    bytes: &mut usize,
    message: Value,
) -> Result<(), String> {
    if *bytes >= MAX_CONTEXT_BYTES {
        return Err("agent context exceeds 4 MiB limit".into());
    }
    let remaining = MAX_CONTEXT_BYTES.saturating_sub(bytes.saturating_add(1));
    let Some(size) =
        crate::output::json_len_within(&message, remaining).map_err(|_| "invalid context JSON")?
    else {
        return Err("agent context exceeds 4 MiB limit".into());
    };
    *bytes = bytes.saturating_add(size).saturating_add(1);
    messages.push(message);
    Ok(())
}

/// Runs independent contexts with the default limit of three scoped workers.
/// See [`run_parallel_with_limit`] for ordering and failure semantics.
pub fn run_parallel(
    client: &ChatClient,
    host: &dyn ToolHost,
    system: &str,
    tasks: &[String],
    max_turns: usize,
) -> Result<Vec<AgentResult>, String> {
    run_parallel_with_limit(client, host, system, tasks, max_turns, 3)
}

/// Runs independent contexts on a bounded, work-conserving queue (1..=3 workers).
/// Results and errors use input task order, not completion order. A completed
/// failure closes admission; already admitted siblings finish and are joined.
/// Capability restrictions belong to the supplied host, not this scheduler.
pub fn run_parallel_with_limit(
    client: &ChatClient,
    host: &dyn ToolHost,
    system: &str,
    tasks: &[String],
    max_turns: usize,
    max_workers: usize,
) -> Result<Vec<AgentResult>, String> {
    run_tasks(tasks.len(), max_workers, |index| {
        run(client, host, system, &tasks[index], max_turns)
    })
}

/// Shared scheduling boundary for uniform and per-task capability hosts.
/// Claims and failure publication use one short critical section. No model or
/// tool work runs under that lock; idle workers immediately claim the next task.
pub(crate) fn run_tasks<T: Send>(
    task_count: usize,
    max_workers: usize,
    task: impl Fn(usize) -> Result<T, String> + Sync,
) -> Result<Vec<T>, String> {
    use std::panic::{catch_unwind, AssertUnwindSafe};
    use std::sync::Mutex;
    if !(1..=3).contains(&max_workers) {
        return Err("worker limit must be between 1 and 3".into());
    }
    struct Queue<T> {
        next: usize,
        stopped: bool,
        results: Vec<Option<Result<T, String>>>,
    }
    let queue = Mutex::new(Queue {
        next: 0,
        stopped: false,
        results: (0..task_count).map(|_| None).collect(),
    });
    std::thread::scope(|scope| -> Result<(), String> {
        let mut handles = Vec::new();
        let mut launch_error = None;
        for _ in 0..max_workers.min(task_count) {
            let (queue, task) = (&queue, &task);
            match std::thread::Builder::new().spawn_scoped(scope, move || loop {
                let index = {
                    let mut state = queue.lock().map_err(|_| "scheduler lock poisoned")?;
                    if state.stopped || state.next == task_count {
                        return Ok::<(), String>(());
                    }
                    let index = state.next;
                    state.next += 1;
                    index
                };
                let result = catch_unwind(AssertUnwindSafe(|| task(index)))
                    .unwrap_or_else(|_| Err("agent worker panicked".into()));
                let mut state = queue.lock().map_err(|_| "scheduler lock poisoned")?;
                state.stopped |= result.is_err();
                state.results[index] = Some(result);
            }) {
                Ok(handle) => handles.push(handle),
                Err(error) => {
                    // Thread-creation failures are infrastructure errors; do not
                    // detach successfully started workers or admit more tasks.
                    if let Ok(mut state) = queue.lock() {
                        state.stopped = true;
                    }
                    launch_error = Some(format!("cannot start agent worker: {error}"));
                    break;
                }
            }
        }
        let joined: Vec<_> = handles
            .into_iter()
            .map(|handle| {
                handle
                    .join()
                    .unwrap_or_else(|_| Err("agent worker panicked".into()))
            })
            .collect();
        if let Some(error) = launch_error {
            return Err(error);
        }
        for result in joined {
            result?;
        }
        Ok(())
    })?;
    let state = queue.into_inner().map_err(|_| "scheduler lock poisoned")?;
    let mut results = Vec::with_capacity(task_count);
    for result in state.results.into_iter().flatten() {
        results.push(result?);
    }
    // No task can be skipped without an error having closed admission.
    if results.len() != task_count {
        return Err("scheduler ended without completing every task".into());
    }
    Ok(results)
}

fn identifier(value: &str, max: usize) -> bool {
    !value.is_empty()
        && value.len() <= max
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

pub fn run(
    client: &ChatClient,
    host: &dyn ToolHost,
    system: &str,
    task: &str,
    max_turns: usize,
) -> Result<AgentResult, String> {
    use serde_json::json;
    if !(1..=MAX_TURNS).contains(&max_turns) {
        return Err("turn limit must be between 1 and 64".into());
    }
    if system.len().saturating_add(task.len()) > MAX_CONTEXT_BYTES {
        return Err("agent context exceeds 4 MiB limit".into());
    }
    let tools = host.schemas();
    let mut allowed = std::collections::HashSet::new();
    for schema in &tools {
        let name = schema["function"]["name"]
            .as_str()
            .filter(|s| identifier(s, 64))
            .ok_or("invalid tool schema name")?;
        if schema["type"] != "function" || !allowed.insert(name) {
            return Err("invalid or duplicate tool schema".into());
        }
    }
    let mut ids = std::collections::HashSet::new();
    let mut messages = Vec::new();
    let mut context_bytes = 2;
    push_message(
        &mut messages,
        &mut context_bytes,
        json!({"role":"system","content":system}),
    )?;
    push_message(
        &mut messages,
        &mut context_bytes,
        json!({"role":"user","content":task}),
    )?;
    let mut tool_calls = 0;
    let mut consecutive_truncations = 0;
    for turn in 1..=max_turns {
        crate::cancel::check()?;
        if context_bytes > host.compaction_threshold() {
            let elided = compact(&mut messages, &mut context_bytes, PROTECT_LAST_TOOL_RESULTS)?;
            if elided > 0 {
                host.observe("compaction", json!({"turn":turn,"reason":"threshold","elided":elided,"bytes_after":context_bytes}))?;
            }
        }
        // A signal during a blocking request surfaces as a transport error;
        // report the cancellation, not the interrupted read.
        let message = match client.complete(&messages, &tools) {
            Ok(message) => message,
            Err(error) => {
                crate::cancel::check()?;
                // One compact-and-retry when the provider says the transcript
                // is too large; keep only the most recent tool result.
                if !is_context_overflow(&error) {
                    return Err(error);
                }
                let elided = compact(&mut messages, &mut context_bytes, 1)?;
                if elided == 0 {
                    return Err(error);
                }
                host.observe("compaction", json!({"turn":turn,"reason":"provider_overflow","elided":elided,"bytes_after":context_bytes,"error":error}))?;
                client
                    .complete(&messages, &tools)
                    .map_err(|e| crate::cancel::check().err().unwrap_or(e))?
            }
        };
        // Usage is audit data, never transcript content.
        let mut message = message;
        let usage = message
            .as_object_mut()
            .and_then(|m| m.remove(crate::model::USAGE));
        host.observe(
            "model_turn",
            json!({"turn":turn,"message":message,"usage":usage}),
        )?;
        if message.get(crate::model::TRUNCATED) == Some(&Value::Bool(true)) {
            consecutive_truncations += 1;
            if consecutive_truncations > MAX_CONSECUTIVE_TRUNCATIONS {
                return Err("model output truncated twice in a row at the output token limit; raise max_output_tokens or split the work".into());
            }
            // Keep the partial text as context, never its partial tool calls.
            push_message(
                &mut messages,
                &mut context_bytes,
                json!({"role":"assistant","content":message["content"].as_str().unwrap_or("")}),
            )?;
            push_message(
                &mut messages,
                &mut context_bytes,
                json!({"role":"user","content":"Your previous response was cut off at the output token limit and its tool calls were discarded. Do not repeat it. Continue in smaller steps: at most one tool call per turn, and split large file contents across several edit_file or write_file calls."}),
            )?;
            continue;
        }
        consecutive_truncations = 0;
        let call_count = message
            .get("tool_calls")
            .and_then(Value::as_array)
            .map_or(0, Vec::len);
        if call_count > MAX_TOOL_CALLS - tool_calls {
            return Err("agent tool call limit exceeded".into());
        }
        let final_text = if call_count == 0 {
            Some(
                message["content"]
                    .as_str()
                    .ok_or("missing final text")?
                    .to_owned(),
            )
        } else {
            None
        };
        push_message(&mut messages, &mut context_bytes, message)?;
        if let Some(text) = final_text {
            return Ok(AgentResult {
                text,
                messages,
                tool_calls,
            });
        }
        // Validate the complete batch before invoking any capability.
        let mut validated = Vec::with_capacity(call_count);
        {
            let calls = messages
                .last()
                .and_then(|m| m.get("tool_calls"))
                .and_then(Value::as_array)
                .ok_or("missing tool_calls")?;
            for call in calls {
                if call["type"] != "function" {
                    return Err("unsupported tool call type".into());
                }
                let id = call["id"]
                    .as_str()
                    .filter(|s| identifier(s, 128))
                    .ok_or("invalid tool call ID")?;
                if !ids.insert(id.to_string()) {
                    return Err("duplicate tool call ID".into());
                }
                let name = call["function"]["name"]
                    .as_str()
                    .filter(|s| identifier(s, 64))
                    .ok_or("invalid tool name")?;
                let args: Value = serde_json::from_str(
                    call["function"]["arguments"]
                        .as_str()
                        .ok_or("missing tool arguments")?,
                )
                .map_err(|_| "invalid tool arguments")?;
                if !args.is_object() {
                    return Err("tool arguments must be a JSON object".into());
                }
                validated.push((id.to_owned(), name.to_owned(), args));
            }
        }
        // A well-formed capability denial is recoverable, but never dispatch a
        // partial batch. Denied and skipped calls consume IDs and the call budget;
        // only a subsequent model turn can request fresh calls.
        let denied = validated
            .iter()
            .any(|(_, name, _)| !allowed.contains(name.as_str()));
        tool_calls += validated.len();
        for (id, name, args) in validated {
            let result = if denied {
                if allowed.contains(name.as_str()) {
                    json!({"error":"batch skipped: another call requested an unknown or unauthorized tool; no calls were executed"})
                } else {
                    json!({"error":"unknown or unauthorized tool: not in the advertised allowlist; no calls were executed"})
                }
            } else {
                host.call(&name, args)?
            };
            let content = result.to_string();
            if content.len() > MAX_TOOL_RESULT_BYTES {
                return Err("tool result exceeds 256 KiB limit".into());
            }
            push_message(
                &mut messages,
                &mut context_bytes,
                json!({"role":"tool","tool_call_id":id,"content":content}),
            )?;
        }
    }
    Err("agent exhausted its turn limit".into())
}
