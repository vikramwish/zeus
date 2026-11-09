use serde_json::{json, Value};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::Path;
use std::process::Command;
use std::sync::{Arc, Mutex};

/// Scripted fixture: replies come from `script(index, request)`; records requests.
fn fixture(
    script: impl Fn(usize, &Value) -> String + Send + 'static,
) -> (String, Arc<Mutex<Vec<Value>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let endpoint = format!("http://{}/v1/x", listener.local_addr().unwrap());
    let requests = Arc::new(Mutex::new(Vec::new()));
    let seen = requests.clone();
    std::thread::spawn(move || {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
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
            let body = script(index, &request);
            let _ = write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", body.len(), body);
        }
    });
    (endpoint, requests)
}

fn run(dir: &Path, endpoint: &str, extra: Value) -> std::process::Output {
    let source = dir.join("source");
    std::fs::create_dir(&source).unwrap();
    std::fs::write(source.join("input.txt"), "unchanged").unwrap();
    let mut job = json!({"task":"Inspect only","source":source,"state_dir":dir.join("state"),"endpoint":endpoint,"model":"fixture","model_retries":0});
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

fn chat(message: Value, finish: &str) -> String {
    json!({"choices":[{"message":message,"finish_reason":finish}]}).to_string()
}

#[test]
fn chat_length_truncation_is_recovered_without_running_partial_tool_calls() {
    let temp = tempfile::tempdir().unwrap();
    let (endpoint, requests) = fixture(|i, _| match i {
        // A cut-off response: partial text plus a half-written tool call.
        0 => chat(
            json!({"role":"assistant","content":"Writing the file now: {\"path\":\"input.t","tool_calls":[{"id":"w1","type":"function","function":{"name":"write_file","arguments":"{\"path\":\"input.txt\",\"content\":\"aaaa"}}]}),
            "length",
        ),
        _ => chat(
            json!({"role":"assistant","content":"Done in smaller steps."}),
            "stop",
        ),
    });
    let out = run(temp.path(), &endpoint, json!({}));
    let stderr = String::from_utf8_lossy(&out.stderr);
    let report: Value = serde_json::from_slice(&out.stdout).unwrap_or_else(|_| panic!("{stderr}"));
    assert_eq!(report["status"], "unverified", "{stderr}");
    assert_eq!(report["summary"], "Done in smaller steps.");
    let requests = requests.lock().unwrap();
    assert_eq!(requests.len(), 2);
    let messages = requests[1]["messages"].as_array().unwrap();
    let assistant = &messages[messages.len() - 2];
    assert_eq!(assistant["role"], "assistant");
    assert!(assistant["content"]
        .as_str()
        .unwrap()
        .starts_with("Writing the file now"));
    assert!(
        assistant.get("tool_calls").is_none_or(Value::is_null),
        "partial tool call replayed: {assistant}"
    );
    assert!(
        assistant.get("_truncated").is_none(),
        "internal marker leaked to provider"
    );
    let nudge = messages.last().unwrap();
    assert_eq!(nudge["role"], "user");
    assert!(
        nudge["content"].as_str().unwrap().contains("cut off"),
        "{nudge}"
    );
    // The source copy was never touched by the half-written call.
    assert_eq!(
        std::fs::read_to_string(temp.path().join("source/input.txt")).unwrap(),
        "unchanged"
    );
}

#[test]
fn anthropic_max_tokens_is_configurable_and_truncation_recovers() {
    for configured in [None, Some(2048)] {
        let temp = tempfile::tempdir().unwrap();
        let (endpoint, requests) = fixture(|i, _| {
            match i {
            0 => json!({"type":"message","role":"assistant","content":[{"type":"text","text":"partial"},{"type":"tool_use","id":"t1","name":"write_file","input":{"path":"input.txt","content":"x"}}],"stop_reason":"max_tokens"}).to_string(),
            _ => json!({"type":"message","role":"assistant","content":[{"type":"text","text":"Finished after the cut."}],"stop_reason":"end_turn"}).to_string(),
        }
        });
        let mut extra = json!({"protocol":"anthropic"});
        if let Some(n) = configured {
            extra["max_output_tokens"] = json!(n);
        }
        let out = run(temp.path(), &endpoint, extra);
        let stderr = String::from_utf8_lossy(&out.stderr);
        let report: Value =
            serde_json::from_slice(&out.stdout).unwrap_or_else(|_| panic!("{stderr}"));
        assert_eq!(report["summary"], "Finished after the cut.", "{stderr}");
        let requests = requests.lock().unwrap();
        assert_eq!(requests.len(), 2);
        assert_eq!(requests[0]["max_tokens"], json!(configured.unwrap_or(8192)));
        let last = requests[1]["messages"].as_array().unwrap().last().unwrap();
        assert_eq!(last["role"], "user");
        let text = last["content"].to_string();
        assert!(text.contains("cut off"), "{text}");
        assert_eq!(
            std::fs::read_to_string(temp.path().join("source/input.txt")).unwrap(),
            "unchanged"
        );
    }
}

#[test]
fn repeated_truncation_is_a_named_error_not_a_loop() {
    let temp = tempfile::tempdir().unwrap();
    let (endpoint, requests) = fixture(|_, _| {
        chat(
            json!({"role":"assistant","content":"still too long"}),
            "length",
        )
    });
    let out = run(temp.path(), &endpoint, json!({}));
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("truncated"), "{stderr}");
    assert!(stderr.contains("output token limit"), "{stderr}");
    assert_eq!(
        requests.lock().unwrap().len(),
        2,
        "must stop after two consecutive truncations"
    );
}

#[test]
fn max_output_tokens_is_validated() {
    let temp = tempfile::tempdir().unwrap();
    let out = run(
        temp.path(),
        "http://127.0.0.1:9/v1/x",
        json!({"max_output_tokens":10}),
    );
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("max_output_tokens"));
}
