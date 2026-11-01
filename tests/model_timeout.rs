use serde_json::{json, Value};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::process::Command;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::{Duration, Instant};

// Actual CLI and loopback HTTP fixture; no live model or credentials.
fn delayed_response(timeout: u64, expect_response: bool, trickle: bool) {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    std::fs::create_dir(&source).unwrap();
    std::fs::write(source.join("input.txt"), "unchanged").unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let endpoint = format!(
        "http://{}/v1/chat/completions",
        listener.local_addr().unwrap()
    );
    let stop = Arc::new(AtomicBool::new(false));
    let done = stop.clone();
    let server = std::thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(10);
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
            stream
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            stream
                .set_write_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            let mut header = Vec::new();
            let mut byte = [0];
            while !header.ends_with(b"\r\n\r\n") {
                stream.read_exact(&mut byte).unwrap();
                header.push(byte[0]);
                assert!(header.len() < 16384);
            }
            let length: usize = String::from_utf8(header)
                .unwrap()
                .lines()
                .find_map(|line| {
                    line.to_ascii_lowercase()
                        .strip_prefix("content-length:")
                        .map(|v| v.trim().parse().unwrap())
                })
                .unwrap();
            assert!(length < 1024 * 1024);
            let mut bytes = vec![0; length];
            stream.read_exact(&mut bytes).unwrap();
            let received_at = Instant::now();
            let _: Value = serde_json::from_slice(&bytes).unwrap();
            let body = json!({"choices":[{"message":{"role":"assistant","content":"Fixture finished."},"finish_reason":"stop"}]}).to_string();
            if trickle {
                write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len()).unwrap();
                for byte in body.bytes() {
                    if done.load(Ordering::Relaxed) || stream.write_all(&[byte]).is_err() {
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(100));
                }
            } else {
                // Stay well beyond the 1s budget and 2s assertion, but stop promptly.
                while received_at.elapsed() < Duration::from_secs(3)
                    && !done.load(Ordering::Relaxed)
                {
                    std::thread::sleep(Duration::from_millis(10));
                }
                if !done.load(Ordering::Relaxed) {
                    let _ = write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", body.len(), body);
                }
            }
            return Some(received_at);
        }
        None
    });
    let state = temp.path().join("state");
    let job = temp.path().join("job.json");
    std::fs::write(&job, json!({"task":"Inspect only", "source":source,"state_dir":state,"endpoint":endpoint,"model":"fixture","model_timeout":timeout}).to_string()).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_zeus"))
        .arg("run")
        .arg(&job)
        .arg("--allow-network")
        .output();
    // Exclude CLI/workspace startup and fixture cleanup from the request budget.
    let completed_at = Instant::now();
    stop.store(true, Ordering::Relaxed);
    let received = server.join().unwrap();
    let output = output.unwrap();
    assert!(
        received.is_some(),
        "model request never started: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let elapsed = completed_at.duration_since(received.unwrap());
    assert!(!output.status.success()); // No verifier means no success claim.
    assert!(!state.join("policy.json").exists());
    assert!(!state.join("run.lock").exists());
    if expect_response {
        let report: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(report["status"], "unverified");
        assert_eq!(report["summary"], "Fixture finished.");
        assert!(elapsed >= Duration::from_secs(3));
    } else {
        let error = String::from_utf8_lossy(&output.stderr);
        assert!(
            error.contains("model request failed")
                || error.contains("failed to read model response"),
            "{error}"
        );
        assert!(
            elapsed < Duration::from_secs(2),
            "deadline was not applied: {elapsed:?}"
        );
        let run = std::fs::read_dir(&state)
            .unwrap()
            .map(|e| e.unwrap().path())
            .find(|p| p.is_dir())
            .unwrap();
        let error: Value =
            serde_json::from_slice(&std::fs::read(run.join("error.json")).unwrap()).unwrap();
        assert_eq!(error["status"], "infrastructure_error");
        assert_eq!(error["policy_updated"], false);
    }
}

#[test]
fn short_model_deadline_stops_cli_request_without_reward() {
    delayed_response(1, false, false);
}

#[test]
fn longer_model_deadline_allows_same_delayed_response() {
    delayed_response(5, true, false);
}

#[test]
fn model_deadline_bounds_total_read_despite_continued_bytes() {
    delayed_response(1, false, true);
}

#[test]
fn timeout_bounds_are_validated_before_state_or_credentials() {
    use zeus::config::{Approvals, Job};
    use zeus::model::{ChatClient, Protocol};
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    std::fs::create_dir(&source).unwrap();
    let state = temp.path().join("state");
    let path = temp.path().join("job.json");
    let base = json!({"task":"Inspect", "source":source,"state_dir":state,"endpoint":"http://127.0.0.1:9/v1/chat/completions","model":"fixture"});
    let approvals = Approvals {
        network: true,
        exec: false,
        mcp: false,
    };
    std::fs::write(&path, base.to_string()).unwrap();
    assert_eq!(Job::load(&path, approvals).unwrap().model_timeout, 180);
    for timeout in [0, 901, u64::MAX] {
        let mut value = base.clone();
        value["model_timeout"] = json!(timeout);
        std::fs::write(&path, value.to_string()).unwrap();
        assert!(Job::load(&path, approvals)
            .unwrap_err()
            .contains("model_timeout"));
        // Public runtime entry must reject an invalid directly deserialized Job too.
        value["api_key_env"] = json!("ZEUS_TEST_MISSING_CREDENTIAL_MUST_NOT_BE_READ");
        let job: Job = serde_json::from_value(value).unwrap();
        assert!(zeus::runtime::run(job)
            .unwrap_err()
            .contains("model_timeout"));
        assert!(ChatClient::with_protocol_timeout(
            "http://127.0.0.1:9",
            "fixture",
            None,
            Protocol::ChatCompletions,
            timeout
        )
        .is_err());
        assert!(!state.exists());
    }
    for timeout in [1, 900] {
        let mut value = base.clone();
        value["model_timeout"] = json!(timeout);
        std::fs::write(&path, value.to_string()).unwrap();
        assert_eq!(Job::load(&path, approvals).unwrap().model_timeout, timeout);
        assert!(ChatClient::with_protocol_timeout(
            "http://127.0.0.1:9",
            "fixture",
            None,
            Protocol::ChatCompletions,
            timeout
        )
        .is_ok());
    }
}
