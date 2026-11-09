use serde_json::{json, Value};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::Path;
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Scripted HTTP responses in order; the last entry repeats. Each entry is
/// (status line, extra headers, body). Returns the request arrival instants.
fn fixture(
    script: Vec<(&'static str, &'static str, String)>,
) -> (String, Arc<Mutex<Vec<Instant>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let endpoint = format!(
        "http://{}/v1/chat/completions",
        listener.local_addr().unwrap()
    );
    let arrivals = Arc::new(Mutex::new(Vec::new()));
    let seen = arrivals.clone();
    std::thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(30);
        let mut index = 0;
        while Instant::now() < deadline {
            let (mut stream, _) = match listener.accept() {
                Ok(v) => v,
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(5));
                    continue;
                }
                Err(e) => panic!("{e}"),
            };
            stream.set_nonblocking(false).unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
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
            seen.lock().unwrap().push(Instant::now());
            let (status, headers, body) = &script[index.min(script.len() - 1)];
            index += 1;
            let _ = write!(
                stream,
                "HTTP/1.1 {status}\r\nContent-Type: application/json\r\n{headers}Content-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
        }
    });
    (endpoint, arrivals)
}

fn ok_body() -> String {
    json!({"choices":[{"message":{"role":"assistant","content":"Fixture finished."},"finish_reason":"stop"}]}).to_string()
}

fn run(dir: &Path, endpoint: &str, extra: Value) -> std::process::Output {
    let source = dir.join("source");
    std::fs::create_dir(&source).unwrap();
    std::fs::write(source.join("input.txt"), "unchanged").unwrap();
    let mut job = json!({"task":"Inspect only","source":source,"state_dir":dir.join("state"),"endpoint":endpoint,"model":"fixture","model_timeout":10});
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

#[test]
fn transient_503_is_retried_after_retry_after_and_run_completes() {
    let temp = tempfile::tempdir().unwrap();
    let (endpoint, arrivals) = fixture(vec![
        (
            "503 Service Unavailable",
            "Retry-After: 1\r\n",
            json!({"error":"overloaded"}).to_string(),
        ),
        ("200 OK", "", ok_body()),
    ]);
    let out = run(temp.path(), &endpoint, json!({}));
    let stderr = String::from_utf8_lossy(&out.stderr);
    let report: Value = serde_json::from_slice(&out.stdout).unwrap_or_else(|_| panic!("{stderr}"));
    assert_eq!(report["status"], "unverified", "{stderr}");
    assert_eq!(report["summary"], "Fixture finished.");
    let arrivals = arrivals.lock().unwrap();
    assert_eq!(arrivals.len(), 2, "expected exactly one retry");
    let gap = arrivals[1].duration_since(arrivals[0]);
    assert!(
        gap >= Duration::from_secs(1) && gap < Duration::from_secs(5),
        "{gap:?}"
    );
}

#[test]
fn client_error_body_is_surfaced_and_not_retried() {
    let temp = tempfile::tempdir().unwrap();
    let body = json!({"error":{"message":"This model's maximum context length is 8192 tokens","code":"context_length_exceeded"}}).to_string();
    let (endpoint, arrivals) = fixture(vec![("400 Bad Request", "", body)]);
    let out = run(temp.path(), &endpoint, json!({}));
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("HTTP 400"), "{stderr}");
    assert!(stderr.contains("context_length_exceeded"), "{stderr}");
    assert_eq!(arrivals.lock().unwrap().len(), 1, "4xx must not be retried");
    let run_dir = std::fs::read_dir(temp.path().join("state"))
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .find(|p| p.is_dir())
        .unwrap();
    let error: Value =
        serde_json::from_slice(&std::fs::read(run_dir.join("error.json")).unwrap()).unwrap();
    assert!(
        error["error"]
            .as_str()
            .unwrap()
            .contains("context_length_exceeded"),
        "{error}"
    );
}

#[test]
fn exhausted_retries_report_attempt_count() {
    let temp = tempfile::tempdir().unwrap();
    let (endpoint, arrivals) = fixture(vec![(
        "503 Service Unavailable",
        "Retry-After: 0\r\n",
        json!({"error":"still overloaded"}).to_string(),
    )]);
    let out = run(temp.path(), &endpoint, json!({}));
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("HTTP 503"), "{stderr}");
    assert!(stderr.contains("after 4 attempts"), "{stderr}");
    assert!(stderr.contains("still overloaded"), "{stderr}");
    assert_eq!(arrivals.lock().unwrap().len(), 4);
}

#[test]
fn retries_consume_the_shared_model_call_budget() {
    let temp = tempfile::tempdir().unwrap();
    let (endpoint, arrivals) = fixture(vec![(
        "429 Too Many Requests",
        "Retry-After: 0\r\n",
        json!({"error":"rate limited"}).to_string(),
    )]);
    let out = run(
        temp.path(),
        &endpoint,
        json!({"autonomy":{"max_model_calls":2}}),
    );
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("budget exhausted"), "{stderr}");
    assert_eq!(
        arrivals.lock().unwrap().len(),
        2,
        "budget bounds attempts, not calls"
    );
}

#[test]
fn connection_refused_is_retried_then_named() {
    let temp = tempfile::tempdir().unwrap();
    // Bind then drop so the port is closed; retries hit a refused connection.
    let endpoint = {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        format!(
            "http://{}/v1/chat/completions",
            listener.local_addr().unwrap()
        )
    };
    let started = Instant::now();
    let out = run(temp.path(), &endpoint, json!({"model_retries":1}));
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("after 2 attempts"), "{stderr}");
    assert!(stderr.to_lowercase().contains("connect"), "{stderr}");
    assert!(
        started.elapsed() >= Duration::from_secs(1),
        "no backoff between attempts"
    );
}

#[test]
fn timeouts_are_named_and_not_retried() {
    let temp = tempfile::tempdir().unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!(
        "http://{}/v1/chat/completions",
        listener.local_addr().unwrap()
    );
    // Accept but never answer.
    let holder = std::thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(15);
        let mut kept = Vec::new();
        listener.set_nonblocking(true).unwrap();
        while Instant::now() < deadline {
            if let Ok((s, _)) = listener.accept() {
                kept.push(s);
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        kept.len()
    });
    let out = run(temp.path(), &endpoint, json!({"model_timeout":1}));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("timed out after 1s"), "{stderr}");
    assert!(!stderr.contains("attempts"), "{stderr}");
    drop(out);
    assert_eq!(holder.join().unwrap(), 1, "timeout must not be retried");
}

#[test]
fn model_retries_is_validated() {
    let temp = tempfile::tempdir().unwrap();
    let out = run(
        temp.path(),
        "http://127.0.0.1:9/v1/chat/completions",
        json!({"model_retries":9}),
    );
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("model_retries"));
}
