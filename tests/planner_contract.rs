use serde_json::{json, Value};
use std::fs;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::process::{Command, Stdio};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::{Duration, Instant};

// Deterministic loopback HTTP fixture exercising the real CLI and saved Parallel
// policy. No credentials, paid inference, verifier, or model-quality claims.
fn exercise(plan: &str, workers: usize, deny_worker_write: bool) {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    let state = temp.path().join("state");
    fs::create_dir(&source).unwrap();
    fs::create_dir(&state).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&state, fs::Permissions::from_mode(0o700)).unwrap();
    }
    fs::write(source.join("input.txt"), "unchanged").unwrap();
    let policy = json!({"contexts":{"planner-contract":[
        {"trials":1,"successes":1},{"trials":0,"successes":0}
    ]}})
    .to_string();
    fs::write(state.join("policy.json"), &policy).unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let endpoint = format!(
        "http://{}/v1/chat/completions",
        listener.local_addr().unwrap()
    );
    let stop = Arc::new(AtomicBool::new(false));
    let done = stop.clone();
    let planner_text = plan.to_owned();
    let server = std::thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut requests = Vec::new();
        while !done.load(Ordering::Relaxed) && Instant::now() < deadline {
            let (mut stream, _) = match listener.accept() {
                Ok(pair) => pair,
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(2));
                    continue;
                }
                Err(e) => panic!("{e}"),
            };
            stream.set_nonblocking(false).unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            stream
                .set_write_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let mut header = Vec::new();
            let mut byte = [0];
            while !header.ends_with(b"\r\n\r\n") {
                stream.read_exact(&mut byte).unwrap();
                header.push(byte[0]);
                assert!(header.len() <= 16384);
            }
            let header = String::from_utf8(header).unwrap();
            let length: usize = header
                .lines()
                .find_map(|line| {
                    line.to_ascii_lowercase()
                        .strip_prefix("content-length:")
                        .map(|value| value.trim().parse().unwrap())
                })
                .unwrap();
            assert!(length <= 1024 * 1024);
            let mut bytes = vec![0; length];
            stream.read_exact(&mut bytes).unwrap();
            let request: Value = serde_json::from_slice(&bytes).unwrap();
            assert!(requests.len() < 8, "unexpected retry/fallback");
            let writable = request["tools"]
                .as_array()
                .unwrap()
                .iter()
                .any(|tool| tool["function"]["name"] == "write_file");
            let message = if requests.is_empty() {
                assert!(!writable);
                assert!(request["messages"][1]["content"]
                    .as_str()
                    .unwrap()
                    .contains("Decompose the investigation"));
                json!({"role":"assistant","content":planner_text})
            } else if !writable && deny_worker_write {
                json!({"role":"assistant","content":null,"tool_calls":[{
                    "id":"denied-write","type":"function","function":{
                        "name":"write_file","arguments":json!({
                            "path":"forbidden.txt","content":"bad","expected_sha256":null
                        }).to_string()
                    }
                }]})
            } else {
                // Non-planner output must remain untouched, including wrapper tags.
                json!({"role":"assistant","content":"<think>synthetic fixture</think>unchanged report"})
            };
            requests.push(request);
            let finish = if message.get("tool_calls").is_some() {
                "tool_calls"
            } else {
                "stop"
            };
            let body = json!({"choices":[{"message":message,"finish_reason":finish}]}).to_string();
            write!(stream,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",body.len(),body).unwrap();
        }
        requests
    });
    let job = temp.path().join("job.json");
    fs::write(
        &job,
        json!({
            "task":"Investigate the fixture without changing files", "source":source,
            "state_dir":state,"context":"planner-contract","endpoint":endpoint,
            "model":"deterministic-local-fixture","max_turns":2
        })
        .to_string(),
    )
    .unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_zeus"))
        .arg("run")
        .arg(&job)
        .arg("--allow-network")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut timed_out = false;
    while child.try_wait().unwrap().is_none() {
        if Instant::now() >= deadline {
            child.kill().unwrap();
            timed_out = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    let output = child.wait_with_output().unwrap();
    stop.store(true, Ordering::Relaxed);
    let requests = server.join().unwrap();
    assert!(!timed_out, "CLI exceeded fixture deadline");
    assert!(
        !output.status.success(),
        "unverified/error CLI must be nonzero"
    );
    let runs: Vec<_> = fs::read_dir(&state)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.is_dir())
        .collect();
    assert_eq!(runs.len(), 1);
    let run = &runs[0];
    let events: Vec<Value> = fs::read_to_string(run.join("events.jsonl"))
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(events[0]["data"]["strategy"], "Parallel");
    let planner: Vec<_> = events.iter().filter(|e| e["kind"] == "planner").collect();
    assert_eq!(planner.len(), 1);
    assert_eq!(
        planner[0]["data"]["text"], plan,
        "raw audit must be preserved"
    );
    assert_eq!(
        fs::read(state.join("policy.json")).unwrap(),
        policy.as_bytes()
    );
    assert!(!state.join("run.lock").exists());
    assert!(!run.join("learning.json").exists());
    assert_eq!(
        fs::read_to_string(source.join("input.txt")).unwrap(),
        "unchanged"
    );
    assert_eq!(
        fs::read_to_string(run.join("candidate/input.txt")).unwrap(),
        "unchanged"
    );
    assert!(!run.join("candidate/forbidden.txt").exists());
    if workers == 0 || deny_worker_write {
        assert!(!run.join("report.json").exists());
        let error: Value =
            serde_json::from_slice(&fs::read(run.join("error.json")).unwrap()).unwrap();
        assert_eq!(error["status"], "infrastructure_error");
        assert_eq!(error["policy_updated"], false);
        assert_eq!(requests.len(), 1 + workers);
        assert!(events
            .iter()
            .all(|e| e["kind"] != "worker" && e["kind"] != "implementer"));
        if deny_worker_write {
            assert!(error["error"]
                .as_str()
                .unwrap()
                .contains("unknown tool requested"));
        } else {
            let diagnostic = error["error"].as_str().unwrap();
            assert!(
                diagnostic.starts_with("planner ") || diagnostic == "invalid plan task",
                "{diagnostic}"
            );
        }
        assert!(events.iter().all(|e| e["kind"] != "tool_request"));
    } else {
        assert!(
            !run.join("error.json").exists(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let report: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(report["status"], "unverified");
        assert_eq!(report["policy_updated"], false);
        assert_eq!(
            report["summary"],
            "<think>synthetic fixture</think>unchanged report"
        );
        assert_eq!(requests.len(), workers + 2);
        assert_eq!(
            events.iter().filter(|e| e["kind"] == "worker").count(),
            workers
        );
        assert_eq!(
            events.iter().filter(|e| e["kind"] == "implementer").count(),
            1
        );
        for request in &requests[1..=workers] {
            let tools = request["tools"].as_array().unwrap();
            assert!(tools.iter().all(|tool| {
                let name = tool["function"]["name"].as_str().unwrap();
                name != "write_file" && name != "run_check" && !name.starts_with("mcp__")
            }));
            assert!(request["messages"][1]["content"]
                .as_str()
                .unwrap()
                .contains("Your read-only investigation question:"));
        }
        assert!(requests.last().unwrap()["tools"]
            .as_array()
            .unwrap()
            .iter()
            .any(|tool| tool["function"]["name"] == "write_file"));
    }
}

#[test]
fn ambiguous_schema_fails_closed() {
    for plan in [
        r#"{"tasks":["a","b"],"tasks":["c","d"]}"#,
        r#"{"tasks":["a","b"],"extra":true}"#,
        r#"{"tasks":["a","b"],"ta\u0073ks":["c","d"]}"#,
        r#"[["a","b"]]"#,
    ] {
        exercise(plan, 0, false);
        exercise(&format!("<think>fixture</think>{plan}"), 0, false);
    }
}

#[test]
fn malformed_framing_and_invalid_tasks_fail_before_delegation() {
    let valid = r#"{"tasks":["a","b"]}"#;
    let cases = vec![
        format!("preface {valid}"),
        format!("{valid} trailing prose"),
        format!("{valid}{valid}"),
        format!("```json\n{valid}\n```"),
        format!("<think>unfinished {valid}"),
        format!("<think><think>nested</think>{valid}"),
        format!("<think>one</think><think>two</think>{valid}"),
        format!("<think>one</think></think>{valid}"),
        format!("<think>one</think>{valid}{valid}"),
        format!("<think>one</think>```json\n{valid}\n```"),
        format!("<think>one</think>prefix {valid}"),
        format!("<think>one</think>{valid} suffix"),
        "<think>one</think>".to_string(),
        "<THINK>one</THINK>".to_string() + valid,
        format!("{}{}", " ".repeat(64 * 1024), valid),
        format!("<think>{}</think>{valid}", "x".repeat(64 * 1024)),
        "null".to_string(),
        "[]".to_string(),
        "{}".to_string(),
        r#"{"tasks":null}"#.to_string(),
        r#"{"tasks":"a"}"#.to_string(),
        r#"{"tasks":[]}"#.to_string(),
        r#"{"tasks":["a"]}"#.to_string(),
        r#"{"tasks":["a","b","c","d"]}"#.to_string(),
        r#"{"tasks":["a",null]}"#.to_string(),
        r#"{"tasks":["a",1]}"#.to_string(),
        r#"{"tasks":["a",{}]}"#.to_string(),
        r#"{"tasks":["a",""]}"#.to_string(),
        json!({"tasks":["a"," \n\t"]}).to_string(),
        json!({"tasks":["a","x".repeat(4097)]}).to_string(),
        json!({"tasks":["a","é".repeat(2049)]}).to_string(),
        format!("<think>fixture</think>{}", json!({"tasks":["a",""]})),
        r#"{"tasks":["a","b"],}"#.to_string(),
    ];
    for (index, plan) in cases.iter().enumerate() {
        eprintln!("invalid planner case {index}");
        exercise(plan, 0, false);
    }
}

#[test]
fn plain_json_and_exact_bounds_remain_supported() {
    exercise(r#"{"tasks":["a","b"]}"#, 2, false);
    exercise(
        &json!({"tasks":["x".repeat(4096), "é".repeat(2048)]}).to_string(),
        2,
        false,
    );
    // Delimiter-looking text inside a JSON string is data, not framing.
    exercise(
        r#"{"tasks":["literal <think> and </think>","b"]}"#,
        2,
        false,
    );
    let json = r#"{"tasks":["a","b"]}"#;
    let overhead = "<think></think>".len() + json.len();
    let at_limit = format!("<think>{}</think>{json}", "x".repeat(64 * 1024 - overhead));
    assert_eq!(at_limit.len(), 64 * 1024);
    exercise(&at_limit, 2, false);
}

#[test]
fn analysts_cannot_gain_write_capabilities_from_valid_plan() {
    exercise("<think>fixture</think>{\"tasks\":[\"a\",\"b\"]}", 2, true);
}

#[test]
fn complete_leading_think_envelope_reaches_parallel_workers() {
    exercise(" \n<think>synthetic fixture, not private model reasoning</think>\n{\"tasks\":[\"behavior\",\"implementation\",\"risks\"]}\n", 3, false);
}
