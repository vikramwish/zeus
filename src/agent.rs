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

fn push_message(
    messages: &mut Vec<Value>,
    bytes: &mut usize,
    message: Value,
) -> Result<(), String> {
    let size = serde_json::to_vec(&message)
        .map_err(|_| "invalid context JSON")?
        .len();
    let next = bytes.saturating_add(size).saturating_add(1);
    if next > MAX_CONTEXT_BYTES {
        return Err("agent context exceeds 4 MiB limit".into());
    }
    *bytes = next;
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
    for turn in 1..=max_turns {
        crate::cancel::check()?;
        // A signal during a blocking request surfaces as a transport error;
        // report the cancellation, not the interrupted read.
        let message = client
            .complete(&messages, &tools)
            .map_err(|e| crate::cancel::check().err().unwrap_or(e))?;
        host.observe("model_turn", json!({"turn":turn,"message":message}))?;
        let calls = message
            .get("tool_calls")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        if calls.len() > MAX_TOOL_CALLS - tool_calls {
            return Err("agent tool call limit exceeded".into());
        }
        push_message(&mut messages, &mut context_bytes, message.clone())?;
        if calls.is_empty() {
            return Ok(AgentResult {
                text: message["content"]
                    .as_str()
                    .ok_or("missing final text")?
                    .into(),
                messages,
                tool_calls,
            });
        }
        // Validate the complete batch before invoking any capability.
        let mut validated = Vec::new();
        for call in &calls {
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
            validated.push((id, name, args));
        }
        // A well-formed capability denial is recoverable, but never dispatch a
        // partial batch. Denied and skipped calls consume IDs and the call budget;
        // only a subsequent model turn can request fresh calls.
        let denied = validated.iter().any(|(_, name, _)| !allowed.contains(name));
        tool_calls += validated.len();
        for (id, name, args) in validated {
            let result = if denied {
                if allowed.contains(name) {
                    json!({"error":"batch skipped: another call requested an unknown or unauthorized tool; no calls were executed"})
                } else {
                    json!({"error":"unknown or unauthorized tool: not in the advertised allowlist; no calls were executed"})
                }
            } else {
                host.call(name, args)?
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
