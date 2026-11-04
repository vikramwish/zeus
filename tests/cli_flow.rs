use std::io::{Read, Write};
use std::process::Command;

// Scripted protocol fixture, NOT an LLM intelligence benchmark.
#[test]
fn full_run_edits_private_candidate_verifies_and_updates_policy() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    std::fs::create_dir(&source).unwrap();
    std::fs::write(source.join("main.txt"), "wrong").unwrap();
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let endpoint = format!(
        "http://{}/v1/chat/completions",
        listener.local_addr().unwrap()
    );
    let server = std::thread::spawn(move || {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
        let mut requests = vec![];
        while requests.len() < 4 && std::time::Instant::now() < deadline {
            let (mut stream, _) = match listener.accept() {
                Ok(v) => v,
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(std::time::Duration::from_millis(10));
                    continue;
                }
                Err(e) => panic!("{e}"),
            };
            // Accepted sockets inherit nonblocking mode on macOS.
            stream.set_nonblocking(false).unwrap();
            stream
                .set_read_timeout(Some(std::time::Duration::from_secs(3)))
                .unwrap();
            let mut header = Vec::new();
            let mut byte = [0];
            while !header.ends_with(b"\r\n\r\n") {
                stream.read_exact(&mut byte).unwrap();
                header.push(byte[0]);
                assert!(header.len() < 65536);
            }
            let header = String::from_utf8(header).unwrap();
            let length: usize = header
                .lines()
                .find_map(|l| {
                    l.to_ascii_lowercase()
                        .strip_prefix("content-length:")
                        .map(|v| v.trim().parse().unwrap())
                })
                .unwrap();
            let mut bytes = vec![0; length];
            stream.read_exact(&mut bytes).unwrap();
            let request: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            let i = requests.len();
            let message = if i == 0 {
                serde_json::json!({"role":"assistant","content":null,"tool_calls":[{"id":"denied1","type":"function","function":{"name":"read_file","arguments":"{\"path\":\"/outside\"}"}}]})
            } else if i == 1 {
                let result: serde_json::Value = serde_json::from_str(
                    request["messages"].as_array().unwrap().last().unwrap()["content"]
                        .as_str()
                        .unwrap(),
                )
                .unwrap();
                assert!(result["error"].is_string());
                serde_json::json!({"role":"assistant","content":null,"tool_calls":[{"id":"read1","type":"function","function":{"name":"read_file","arguments":"{\"path\":\"main.txt\"}"}}]})
            } else if i == 2 {
                let tool = request["messages"].as_array().unwrap().last().unwrap();
                let result: serde_json::Value =
                    serde_json::from_str(tool["content"].as_str().unwrap()).unwrap();
                let hash = result["sha256"].as_str().unwrap();
                serde_json::json!({"role":"assistant","content":null,"tool_calls":[{"id":"write1","type":"function","function":{"name":"write_file","arguments":serde_json::json!({"path":"main.txt","content":"42","expected_sha256":hash}).to_string()}}]})
            } else {
                serde_json::json!({"role":"assistant","content":"Implemented the requested candidate change."})
            };
            requests.push(request);
            let body=serde_json::json!({"choices":[{"message":message,"finish_reason":if i<3 {"tool_calls"} else {"stop"}}]}).to_string();
            write!(stream,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",body.len(),body).unwrap();
        }
        requests
    });
    let state = temp.path().join("state");
    let job = temp.path().join("job.json");
    let verifier = temp.path().join("verify.py");
    std::fs::write(
        &verifier,
        "from pathlib import Path\nassert Path('main.txt').read_text() == '42'\n",
    )
    .unwrap();
    std::fs::write(&job,serde_json::to_vec(&serde_json::json!({"task":"Make main.txt contain exactly 42","source":source,"state_dir":state,"endpoint":endpoint,"model":"scripted-protocol-fixture","max_turns":4,"verifier":["/opt/homebrew/bin/python3","-I",verifier],"verifier_timeout":5})).unwrap()).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_zeus"))
        .args([
            "run",
            job.to_str().unwrap(),
            "--allow-network",
            "--allow-exec",
        ])
        .output()
        .unwrap();
    if !out.status.success() {
        for entry in std::fs::read_dir(&state).unwrap().flatten() {
            let path = entry.path().join("events.jsonl");
            if path.is_file() {
                eprintln!("{}", std::fs::read_to_string(path).unwrap());
            }
        }
    }
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(report["status"], "verified");
    assert_eq!(
        std::fs::read_to_string(source.join("main.txt")).unwrap(),
        "wrong"
    );
    assert_eq!(
        std::fs::read_to_string(
            std::path::Path::new(report["candidate"].as_str().unwrap()).join("main.txt")
        )
        .unwrap(),
        "42"
    );
    assert!(state.join("policy.json").is_file());
    let requests = server.join().unwrap();
    assert_eq!(requests.len(), 4);
    assert_eq!(requests[1]["messages"][2]["role"], "assistant");
    assert_eq!(requests[1]["messages"][3]["role"], "tool");
}

#[test]
fn run_denies_network_without_explicit_approval_before_creating_state() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    std::fs::create_dir(&source).unwrap();
    std::fs::write(source.join("main.txt"), "initial").unwrap();
    let job = temp.path().join("job.json");
    std::fs::write(
        &job,
        serde_json::to_vec(&serde_json::json!({
            "task":"fix it","source":source,"state_dir":temp.path().join("state"),
            "endpoint":"http://127.0.0.1:9/v1/chat/completions","model":"test"
        }))
        .unwrap(),
    )
    .unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_zeus"))
        .arg("run")
        .arg(job)
        .output()
        .unwrap();
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("--allow-network"));
    assert!(!temp.path().join("state").exists());
}
#[test]
fn inspect_cli_reports_artifact_digest_without_mutation() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("sample.wasm");
    std::fs::write(&path, b"\0asm\x01\0\0\0").unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_zeus"))
        .arg("inspect")
        .arg(&path)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(report["entries"][0]["kind"], "wasm");
    assert_eq!(report["entries"][0]["sha256"].as_str().unwrap().len(), 64);
    assert_eq!(std::fs::read(path).unwrap(), b"\0asm\x01\0\0\0");
}
