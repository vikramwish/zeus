//! Local scripted HTTP + real Zeus CLI. No live inference or provider billing.
mod common;
use serde_json::{json, Value};
use std::{
    fs,
    io::{Read, Write},
    net::TcpListener,
    process::Command,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};

fn tool(id: &str, name: &str, args: Value) -> Value {
    json!({"id":id,"type":"function","function":{"name":name,"arguments":args.to_string()}})
}
fn reply(calls: Vec<Value>) -> Value {
    json!({"role":"assistant","content":null,"tool_calls":calls})
}
fn result(request: &Value, id: &str) -> Value {
    let message = request["messages"]
        .as_array()
        .unwrap()
        .iter()
        .rev()
        .find(|m| m["role"] == "tool" && m["tool_call_id"] == id)
        .unwrap();
    serde_json::from_str(message["content"].as_str().unwrap()).unwrap()
}
fn exercise(
    autonomy: Value,
    parallel: bool,
    verify: bool,
    respond: impl Fn(usize, &Value) -> Value + Send + 'static,
) -> (tempfile::TempDir, Value, usize) {
    let tmp = tempfile::tempdir().unwrap();
    let source = tmp.path().join("source");
    fs::create_dir(&source).unwrap();
    for name in ["allowed.txt", "protected.txt"] {
        fs::write(source.join(name), "before").unwrap();
    }
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let endpoint = format!("http://{}/", listener.local_addr().unwrap());
    let stop = Arc::new(AtomicBool::new(false));
    let done = stop.clone();
    let server = std::thread::spawn(move || {
        let mut count = 0;
        while !done.load(Ordering::SeqCst) {
            let mut stream = match listener.accept() {
                Ok((stream, _)) => stream,
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(2));
                    continue;
                }
                Err(e) => panic!("{e}"),
            };
            stream.set_nonblocking(false).unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut header = Vec::new();
            while !header.ends_with(b"\r\n\r\n") {
                let mut b = [0];
                stream.read_exact(&mut b).unwrap();
                header.push(b[0]);
                assert!(header.len() < 65536);
            }
            let length = String::from_utf8(header)
                .unwrap()
                .lines()
                .find_map(|l| {
                    l.to_ascii_lowercase()
                        .strip_prefix("content-length:")
                        .map(|v| v.trim().parse::<usize>().unwrap())
                })
                .unwrap();
            let mut body = vec![0; length];
            stream.read_exact(&mut body).unwrap();
            let request = serde_json::from_slice(&body).unwrap();
            let response = json!({"choices":[{"message":respond(count,&request)}]}).to_string();
            count += 1;
            write!(stream,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{response}",response.len()).unwrap();
        }
        count
    });
    let verifier = tmp.path().join("verify.py");
    fs::write(&verifier, "raise SystemExit(1)\n").unwrap();
    let job = tmp.path().join("job.json");
    fs::write(&job,json!({"task":"Update allowed.txt without changing protected.txt", "source":source,"state_dir":tmp.path().join("state"),"endpoint":endpoint,"model":"scripted-fixture","max_turns":4,"model_timeout":2,"strategy":if parallel {"parallel"} else {"direct"},"autonomy":autonomy,"verifier":if verify {json!([common::python3(),"-I",verifier])} else {json!([])}}).to_string()).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_zeus"))
        .arg("run")
        .arg(job)
        .args(["--allow-network", "--allow-exec"])
        .output()
        .unwrap();
    stop.store(true, Ordering::SeqCst);
    let count = server.join().unwrap();
    let state = tmp.path().join("state");
    let run = fs::read_dir(&state)
        .unwrap()
        .filter_map(Result::ok)
        .map(|e| e.path())
        .find(|p| p.is_dir())
        .unwrap();
    let report = if run.join("report.json").exists() {
        run.join("report.json")
    } else {
        run.join("error.json")
    };
    let mut report: Value = serde_json::from_slice(&fs::read(report).unwrap()).unwrap();
    report["run_dir"] = json!(run);
    assert!(!output.status.success()); // All fixtures intentionally unverified or failed.
    assert_eq!(
        fs::read_to_string(source.join("allowed.txt")).unwrap(),
        "before"
    );
    assert_eq!(
        fs::read_to_string(source.join("protected.txt")).unwrap(),
        "before"
    );
    (tmp, report, count)
}

#[test]
fn user_scope_limits_integrator_not_only_delegated_workers() {
    let (tmp, report, count) = exercise(
        json!({"write_paths":["allowed.txt"],"max_repairs":0}),
        false,
        false,
        |stage, request| {
            if stage == 0 {
                reply(vec![
                    tool(
                        "bad",
                        "write_file",
                        json!({"path":"protected.txt","content":"bad","expected_sha256":zeus::workspace::digest(b"before")}),
                    ),
                    tool(
                        "good",
                        "edit_file",
                        json!({"path":"allowed.txt","old_string":"before","new_string":"after","expected_sha256":zeus::workspace::digest(b"before")}),
                    ),
                ])
            } else {
                assert!(result(request, "bad")["error"].is_string());
                assert_eq!(result(request, "good")["written"], "allowed.txt");
                json!({"role":"assistant","content":"done"})
            }
        },
    );
    assert_eq!(count, 2);
    assert_eq!(report["status"], "unverified");
    let run = std::path::Path::new(report["run_dir"].as_str().unwrap());
    assert_eq!(
        fs::read_to_string(run.join("candidate/allowed.txt")).unwrap(),
        "after"
    );
    assert_eq!(
        fs::read_to_string(run.join("candidate/protected.txt")).unwrap(),
        "before"
    );
    assert!(!tmp.path().join("state/policy.json").exists());
}

#[test]
fn empty_user_scope_removes_file_write_tools() {
    let (_, report, count) = exercise(json!({"write_paths":[]}), false, false, |_, request| {
        assert!(!request["tools"]
            .as_array()
            .unwrap()
            .iter()
            .any(|t| matches!(
                t["function"]["name"].as_str(),
                Some("write_file" | "edit_file")
            )));
        json!({"role":"assistant","content":"read-only report"})
    });
    assert_eq!(count, 1);
    assert_eq!(report["status"], "unverified");
}

#[test]
fn configured_repair_budget_controls_autonomous_retry_count() {
    for max_repairs in [0, 1] {
        let (_, report, count) = exercise(
            json!({"max_repairs":max_repairs}),
            false,
            true,
            |_, _| json!({"role":"assistant","content":"unable to repair fixture"}),
        );
        assert_eq!(report["status"], "failed");
        assert_eq!(count, max_repairs + 1);
        assert_eq!(report["attempts"], max_repairs + 1);
    }
}

#[test]
fn run_model_budget_exhaustion_retains_candidate_without_training() {
    let (tmp, report, count) = exercise(json!({"max_model_calls":1}), false, false, |_, _| {
        reply(vec![tool(
            "read",
            "read_file",
            json!({"path":"allowed.txt"}),
        )])
    });
    assert_eq!(count, 1);
    assert_eq!(report["status"], "infrastructure_error");
    assert!(report["error"]
        .as_str()
        .unwrap()
        .contains("budget exhausted"));
    assert!(!tmp.path().join("state/policy.json").exists());
}

#[test]
fn planner_cannot_expand_user_write_scope() {
    let (tmp, report, count) = exercise(
        json!({"write_paths":["allowed.txt"]}),
        true,
        false,
        |_, _| json!({"role":"assistant","content":json!({"tasks":[{"task":"allowed","write_paths":["allowed.txt"]},{"task":"escape","write_paths":["protected.txt"]}]}).to_string()}),
    );
    assert_eq!(count, 1);
    assert_eq!(report["status"], "infrastructure_error");
    assert!(!tmp.path().join("state/policy.json").exists());
}
