use serde_json::{json, Value};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::Path;
use std::process::Command;
use std::sync::{
    atomic::{AtomicBool, AtomicUsize, Ordering},
    Arc,
};
use std::time::{Duration, Instant};

/// Scripted fixture. `script(index)` returns a body, or None to hold the
/// connection open until `stop`.
fn fixture(
    script: impl Fn(usize) -> Option<String> + Send + 'static,
) -> (String, Arc<AtomicUsize>, Arc<AtomicBool>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let endpoint = format!(
        "http://{}/v1/chat/completions",
        listener.local_addr().unwrap()
    );
    let count = Arc::new(AtomicUsize::new(0));
    let stop = Arc::new(AtomicBool::new(false));
    let (seen, done) = (count.clone(), stop.clone());
    std::thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(30);
        while !done.load(Ordering::Relaxed) && Instant::now() < deadline {
            let (mut stream, _) = match listener.accept() {
                Ok(v) => v,
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(5));
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
            let index = seen.fetch_add(1, Ordering::Relaxed);
            match script(index) {
                None => {
                    while !done.load(Ordering::Relaxed) && Instant::now() < deadline {
                        std::thread::sleep(Duration::from_millis(10));
                    }
                }
                Some(body) => {
                    let _ = write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", body.len(), body);
                }
            }
        }
    });
    (endpoint, count, stop)
}

fn with_usage(message: Value, finish: &str, input: u64, output: u64) -> String {
    json!({"choices":[{"message":message,"finish_reason":finish}],"usage":{"prompt_tokens":input,"completion_tokens":output,"total_tokens":input+output}}).to_string()
}

fn job(dir: &Path, endpoint: &str, extra: Value) -> std::path::PathBuf {
    let source = dir.join("source");
    std::fs::create_dir(&source).unwrap();
    std::fs::write(source.join("input.txt"), "unchanged").unwrap();
    let mut job = json!({"task":"Inspect only","source":source,"state_dir":dir.join("state"),"endpoint":endpoint,"model":"fixture","model_retries":0});
    for (k, v) in extra.as_object().unwrap() {
        job[k] = v.clone();
    }
    let path = dir.join("job.json");
    std::fs::write(&path, job.to_string()).unwrap();
    path
}

fn run(path: &Path) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_zeus"))
        .args(["run", path.to_str().unwrap(), "--allow-network"])
        .output()
        .unwrap()
}

fn run_dir(dir: &Path) -> std::path::PathBuf {
    std::fs::read_dir(dir.join("state"))
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .find(|p| p.is_dir())
        .unwrap()
}

#[test]
fn job_deadline_ends_a_hung_request_and_is_labelled() {
    let temp = tempfile::tempdir().unwrap();
    let (endpoint, _count, stop) = fixture(|_| None);
    let path = job(
        temp.path(),
        &endpoint,
        json!({"max_job_seconds":2,"model_timeout":30}),
    );
    let started = Instant::now();
    let out = run(&path);
    stop.store(true, Ordering::Relaxed);
    assert!(
        started.elapsed() < Duration::from_secs(10),
        "deadline not enforced: {:?}",
        started.elapsed()
    );
    assert!(!out.status.success());
    assert_ne!(out.status.code(), Some(130), "a deadline is not a signal");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("deadline"), "{stderr}");
    assert!(!temp.path().join("state/run.lock").exists());
    let error: Value =
        serde_json::from_slice(&std::fs::read(run_dir(temp.path()).join("error.json")).unwrap())
            .unwrap();
    assert_eq!(error["status"], "deadline_exceeded");
    let list = Command::new(env!("CARGO_BIN_EXE_zeus"))
        .args([
            "sessions",
            "list",
            temp.path().join("state").to_str().unwrap(),
        ])
        .output()
        .unwrap();
    let list: Value = serde_json::from_slice(&list.stdout).unwrap();
    assert_eq!(list["sessions"][0]["status"], "deadline_exceeded");
}

#[test]
fn token_usage_is_accumulated_in_audit_and_report() {
    let temp = tempfile::tempdir().unwrap();
    let (endpoint, _count, _stop) = fixture(|i| {
        Some(match i {
            0 => with_usage(
                json!({"role":"assistant","content":null,"tool_calls":[{"id":"r1","type":"function","function":{"name":"read_file","arguments":"{\"path\":\"input.txt\"}"}}]}),
                "tool_calls",
                100,
                10,
            ),
            _ => with_usage(
                json!({"role":"assistant","content":"Fixture finished."}),
                "stop",
                200,
                20,
            ),
        })
    });
    let path = job(temp.path(), &endpoint, json!({}));
    let out = run(&path);
    let stderr = String::from_utf8_lossy(&out.stderr);
    let report: Value = serde_json::from_slice(&out.stdout).unwrap_or_else(|_| panic!("{stderr}"));
    assert_eq!(report["summary"], "Fixture finished.");
    assert_eq!(
        report["usage"],
        json!({"input_tokens":300,"output_tokens":30,"model_calls":2}),
        "{report}"
    );
    let events: Vec<Value> = std::fs::read_to_string(run_dir(temp.path()).join("events.jsonl"))
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    let turns: Vec<&Value> = events
        .iter()
        .filter(|e| e["kind"] == "model_turn")
        .collect();
    assert_eq!(turns.len(), 2);
    assert_eq!(
        turns[0]["data"]["usage"],
        json!({"input_tokens":100,"output_tokens":10})
    );
    assert!(
        turns[0]["data"]["message"].get("_usage").is_none(),
        "marker must not enter the transcript"
    );
}

#[test]
fn total_token_budget_stops_the_run() {
    let temp = tempfile::tempdir().unwrap();
    let (endpoint, count, _stop) = fixture(|i| {
        Some(match i {
            0 => with_usage(
                json!({"role":"assistant","content":null,"tool_calls":[{"id":"r1","type":"function","function":{"name":"read_file","arguments":"{\"path\":\"input.txt\"}"}}]}),
                "tool_calls",
                100,
                10,
            ),
            _ => with_usage(
                json!({"role":"assistant","content":"Fixture finished."}),
                "stop",
                200,
                20,
            ),
        })
    });
    let path = job(
        temp.path(),
        &endpoint,
        json!({"autonomy":{"max_total_tokens":150}}),
    );
    let out = run(&path);
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("token budget"), "{stderr}");
    assert_eq!(
        count.load(Ordering::Relaxed),
        2,
        "second turn is admitted, third is not"
    );
    let error: Value =
        serde_json::from_slice(&std::fs::read(run_dir(temp.path()).join("error.json")).unwrap())
            .unwrap();
    assert_eq!(error["status"], "infrastructure_error");
    assert_eq!(error["policy_updated"], false);
}

#[test]
fn job_limits_are_validated() {
    for (field, value) in [
        ("max_job_seconds", json!(0)),
        ("max_job_seconds", json!(90000)),
        ("autonomy", json!({"max_total_tokens":0})),
    ] {
        let temp = tempfile::tempdir().unwrap();
        let path = job(
            temp.path(),
            "http://127.0.0.1:9/v1/chat/completions",
            json!({field: value}),
        );
        let out = run(&path);
        assert!(!out.status.success());
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(
            stderr.contains("max_job_seconds") || stderr.contains("max_total_tokens"),
            "{stderr}"
        );
    }
}
