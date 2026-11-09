use serde_json::{json, Value};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::process::Command;

// Scripted fixture: first turn asks to read a file, then the provider fails.
// Proves the audit keeps what the model said before the failure.
fn fixture() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let endpoint = format!(
        "http://{}/v1/chat/completions",
        listener.local_addr().unwrap()
    );
    std::thread::spawn(move || {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
        let mut count = 0;
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
            let (status, body) = if count == 0 {
                ("200 OK", json!({"choices":[{"message":{"role":"assistant","content":"Let me look.","tool_calls":[{"id":"r1","type":"function","function":{"name":"read_file","arguments":"{\"path\":\"input.txt\"}"}}]},"finish_reason":"tool_calls"}]}).to_string())
            } else {
                (
                    "500 Internal Server Error",
                    json!({"error":{"type":"server_error","message":"fixture outage"}}).to_string(),
                )
            };
            count += 1;
            let _ = write!(stream, "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", body.len(), body);
        }
    });
    endpoint
}

#[test]
fn audit_keeps_model_turns_and_timestamps_through_a_failed_run() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    std::fs::create_dir(&source).unwrap();
    std::fs::write(source.join("input.txt"), "unchanged").unwrap();
    let state = temp.path().join("state");
    let job = temp.path().join("job.json");
    std::fs::write(
        &job,
        json!({"task":"Inspect only","source":source,"state_dir":state,"endpoint":fixture(),"model":"fixture","model_retries":0}).to_string(),
    )
    .unwrap();
    let before = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;
    let out = Command::new(env!("CARGO_BIN_EXE_zeus"))
        .args(["run", job.to_str().unwrap(), "--allow-network"])
        .output()
        .unwrap();
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("HTTP 500"), "{stderr}");
    let run_dir = std::fs::read_dir(&state)
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .find(|p| p.is_dir())
        .unwrap();
    let events: Vec<Value> = std::fs::read_to_string(run_dir.join("events.jsonl"))
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    let kinds: Vec<&str> = events.iter().map(|e| e["kind"].as_str().unwrap()).collect();
    // Every event is timestamped in wall-clock millis, in order.
    let mut last = before;
    for event in &events {
        let ts = event["ts_ms"]
            .as_u64()
            .unwrap_or_else(|| panic!("missing ts_ms: {event}"));
        assert!(ts >= last, "timestamps out of order: {kinds:?}");
        last = ts;
    }
    // The model's first turn survived the later provider failure.
    let turn = events
        .iter()
        .find(|e| e["kind"] == "model_turn")
        .unwrap_or_else(|| panic!("no model_turn event: {kinds:?}"));
    assert_eq!(turn["data"]["role"], "implementer");
    assert_eq!(turn["data"]["turn"], 1);
    assert_eq!(
        turn["data"]["message"]["tool_calls"][0]["function"]["name"],
        "read_file"
    );
    assert_eq!(turn["data"]["message"]["content"], "Let me look.");
    // Tool evidence follows the turn that requested it, and the failure closes the log.
    let turn_index = kinds.iter().position(|k| *k == "model_turn").unwrap();
    let request_index = kinds.iter().position(|k| *k == "tool_request").unwrap();
    assert!(turn_index < request_index, "{kinds:?}");
    assert_eq!(*kinds.last().unwrap(), "run_error", "{kinds:?}");
    assert!(events.last().unwrap()["data"]["error"]
        .as_str()
        .unwrap()
        .contains("server_error"));
}
