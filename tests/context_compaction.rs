use serde_json::{json, Value};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::Path;
use std::process::Command;
use std::sync::{Arc, Mutex};

/// Scripted fixture: `script(index, request)` returns (status line, body).
fn fixture(
    script: impl Fn(usize, &Value) -> (&'static str, String) + Send + 'static,
) -> (String, Arc<Mutex<Vec<Value>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let endpoint = format!(
        "http://{}/v1/chat/completions",
        listener.local_addr().unwrap()
    );
    let requests = Arc::new(Mutex::new(Vec::new()));
    let seen = requests.clone();
    std::thread::spawn(move || {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
        while std::time::Instant::now() < deadline {
            let (mut stream, _) = match listener.accept() {
                Ok(v) => v,
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(std::time::Duration::from_millis(5));
                    continue;
                }
                Err(e) => panic!("{e}"),
            };
            stream.set_nonblocking(false).unwrap();
            let mut header = Vec::new();
            let mut byte = [0];
            while !header.ends_with(b"\r\n\r\n") {
                stream.read_exact(&mut byte).unwrap();
                header.push(byte[0]);
            }
            let length: usize = String::from_utf8(header)
                .unwrap()
                .lines()
                .find_map(|l| {
                    l.to_ascii_lowercase()
                        .strip_prefix("content-length:")
                        .map(|v| v.trim().parse().unwrap())
                })
                .unwrap();
            let mut bytes = vec![0; length];
            stream.read_exact(&mut bytes).unwrap();
            let request: Value = serde_json::from_slice(&bytes).unwrap();
            let index = {
                let mut seen = seen.lock().unwrap();
                seen.push(request.clone());
                seen.len() - 1
            };
            let (status, body) = script(index, &request);
            let _ = write!(stream, "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", body.len(), body);
        }
    });
    (endpoint, requests)
}

fn read_call(id: &str) -> (&'static str, String) {
    ("200 OK", json!({"choices":[{"message":{"role":"assistant","content":null,"tool_calls":[{"id":id,"type":"function","function":{"name":"read_file","arguments":"{\"path\":\"big.txt\"}"}}]},"finish_reason":"tool_calls"}]}).to_string())
}
fn final_text() -> (&'static str, String) {
    ("200 OK", json!({"choices":[{"message":{"role":"assistant","content":"Fixture finished."},"finish_reason":"stop"}]}).to_string())
}

fn run(dir: &Path, endpoint: &str, extra: Value) -> std::process::Output {
    let source = dir.join("source");
    std::fs::create_dir(&source).unwrap();
    std::fs::write(source.join("big.txt"), "y".repeat(30 * 1024)).unwrap();
    let mut job = json!({"task":"Inspect only","source":source,"state_dir":dir.join("state"),"endpoint":endpoint,"model":"fixture","model_retries":0,"max_turns":10});
    for (k, v) in extra.as_object().unwrap() {
        job[k] = v.clone();
    }
    let path = dir.join("job.json");
    std::fs::write(&path, job.to_string()).unwrap();
    Command::new(env!("CARGO_BIN_EXE_zeus"))
        .args(["run", path.to_str().unwrap(), "--allow-network"])
        .output()
        .unwrap()
}

fn tool_messages(request: &Value) -> Vec<(String, String)> {
    request["messages"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|m| m["role"] == "tool")
        .map(|m| {
            (
                m["tool_call_id"].as_str().unwrap().to_string(),
                m["content"].as_str().unwrap().to_string(),
            )
        })
        .collect()
}

fn events(dir: &Path) -> Vec<Value> {
    let run_dir = std::fs::read_dir(dir.join("state"))
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .find(|p| p.is_dir())
        .unwrap();
    std::fs::read_to_string(run_dir.join("events.jsonl"))
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}

#[test]
fn oldest_tool_results_are_elided_once_the_threshold_is_crossed() {
    let temp = tempfile::tempdir().unwrap();
    // Five 30 KiB reads exceed a 64 KiB threshold; the last four results are protected.
    let (endpoint, requests) = fixture(|i, _| match i {
        0..=4 => read_call(["r1", "r2", "r3", "r4", "r5"][i]),
        _ => final_text(),
    });
    let out = run(
        temp.path(),
        &endpoint,
        json!({"compaction_threshold_bytes":65536}),
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    let report: Value = serde_json::from_slice(&out.stdout).unwrap_or_else(|_| panic!("{stderr}"));
    assert_eq!(report["summary"], "Fixture finished.", "{stderr}");
    let requests = requests.lock().unwrap();
    assert_eq!(requests.len(), 6);
    let tools = tool_messages(&requests[5]);
    assert_eq!(
        tools.len(),
        5,
        "tool messages must stay paired with their calls"
    );
    assert_eq!(tools[0].0, "r1");
    let elided: Value = serde_json::from_str(&tools[0].1).unwrap();
    assert_eq!(
        elided["elided"], true,
        "oldest result should be elided: {}",
        tools[0].1
    );
    assert!(elided["omitted_bytes"].as_u64().unwrap() > 30 * 1024);
    for (id, content) in &tools[1..] {
        assert!(
            content.contains(&"y".repeat(1024)),
            "{id} was elided but is protected"
        );
    }
    assert!(tools[0].1.len() < 512);
    let all = events(temp.path());
    let compactions: Vec<&Value> = all.iter().filter(|e| e["kind"] == "compaction").collect();
    assert!(!compactions.is_empty(), "compaction must be audited");
    assert_eq!(compactions[0]["data"]["elided"], 1);
    // Earlier requests were untouched: compaction happens only past the threshold.
    assert!(tool_messages(&requests[3])
        .iter()
        .all(|(_, c)| !c.contains("elided")));
}

#[test]
fn provider_context_overflow_compacts_and_retries_once() {
    let temp = tempfile::tempdir().unwrap();
    let (endpoint, requests) = fixture(|i, _| {
        match i {
        0 => read_call("r1"),
        1 => read_call("r2"),
        2 => (
            "400 Bad Request",
            json!({"error":{"message":"This model's maximum context length is 8192 tokens","code":"context_length_exceeded"}}).to_string(),
        ),
        _ => final_text(),
    }
    });
    let out = run(temp.path(), &endpoint, json!({}));
    let stderr = String::from_utf8_lossy(&out.stderr);
    let report: Value = serde_json::from_slice(&out.stdout).unwrap_or_else(|_| panic!("{stderr}"));
    assert_eq!(report["summary"], "Fixture finished.", "{stderr}");
    let requests = requests.lock().unwrap();
    assert_eq!(requests.len(), 4, "exactly one retry after compaction");
    let tools = tool_messages(&requests[3]);
    assert_eq!(tools.len(), 2);
    assert!(tools[0].1.contains("\"elided\":true"), "{}", tools[0].1);
    assert!(
        tools[1].1.contains(&"y".repeat(1024)),
        "most recent result must survive"
    );
    let events = events(temp.path());
    let compaction = events
        .iter()
        .find(|e| e["kind"] == "compaction")
        .expect("audited");
    assert_eq!(compaction["data"]["reason"], "provider_overflow");
}

#[test]
fn overflow_with_nothing_left_to_elide_is_a_named_error() {
    let temp = tempfile::tempdir().unwrap();
    let (endpoint, requests) = fixture(|_, _| {
        (
            "400 Bad Request",
            json!({"error":{"message":"prompt is too long","type":"invalid_request_error"}})
                .to_string(),
        )
    });
    let out = run(temp.path(), &endpoint, json!({}));
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("HTTP 400"), "{stderr}");
    assert_eq!(
        requests.lock().unwrap().len(),
        1,
        "no retry without compaction"
    );
}

#[test]
fn compaction_threshold_is_validated() {
    let temp = tempfile::tempdir().unwrap();
    let out = run(
        temp.path(),
        "http://127.0.0.1:9/v1/chat/completions",
        json!({"compaction_threshold_bytes":1024}),
    );
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("compaction_threshold_bytes"));
}
