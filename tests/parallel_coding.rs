//! Actual CLI + deterministic local protocol; no model quality claims.
use serde_json::{json, Value};
use std::{
    fs,
    io::{BufRead, BufReader, Read, Write},
    net::{TcpListener, TcpStream},
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};
const WAIT: Duration = Duration::from_secs(8);
struct Request {
    stream: TcpStream,
    body: Value,
}
impl Request {
    fn reply(mut self, message: Value) {
        let body = json!({"choices":[{"message":message}]}).to_string();
        write!(self.stream,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).unwrap();
    }
    fn text(self, text: &str) {
        self.reply(json!({"role":"assistant","content":text}));
    }
    fn calls(self, calls: Vec<Value>) {
        self.reply(json!({"role":"assistant","tool_calls":calls}));
    }
    fn result(&self) -> Value {
        serde_json::from_str(
            self.body["messages"].as_array().unwrap().last().unwrap()["content"]
                .as_str()
                .unwrap(),
        )
        .unwrap()
    }
    fn index(&self) -> usize {
        if self.body["messages"][1]["content"]
            .as_str()
            .unwrap()
            .contains("TASK_ALPHA")
        {
            0
        } else {
            1
        }
    }
}
fn call(id: &str, name: &str, args: Value) -> Value {
    json!({"id":id,"type":"function","function":{"name":name,"arguments":args.to_string()}})
}
struct Run {
    temp: tempfile::TempDir,
    listener: TcpListener,
    child: Child,
}
impl Run {
    fn new(verifier: bool) -> Self {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        fs::create_dir(root.join("source")).unwrap();
        fs::create_dir(root.join("state")).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(root.join("state"), fs::Permissions::from_mode(0o700)).unwrap();
        }
        for file in ["a.txt", "b.txt"] {
            fs::write(root.join("source").join(file), "old").unwrap();
        }

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        fs::write(root.join("verify.py"),"from pathlib import Path\nassert all(Path(p).read_text() == 'new' for p in ['a.txt', 'b.txt'])\n").unwrap();
        let mut job = json!({"task":"Make both files new", "source":root.join("source"),"state_dir":root.join("state"),"context":"coding","endpoint":format!("http://{}/v1/chat/completions",listener.local_addr().unwrap()),"model":"fixture","model_timeout":8,"max_turns":8});
        job["strategy"] = json!("parallel");
        if verifier {
            job["verifier"] = json!(["/opt/homebrew/bin/python3", "-I", root.join("verify.py")]);
            job["verifier_timeout"] = json!(5);
        }
        fs::write(root.join("job.json"), job.to_string()).unwrap();
        let child = Command::new(env!("CARGO_BIN_EXE_zeus"))
            .arg("run")
            .arg(root.join("job.json"))
            .args(["--allow-network", "--allow-exec"])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        Self {
            temp,
            listener,
            child,
        }
    }
    fn next(&mut self) -> Request {
        let deadline = Instant::now() + WAIT;
        let stream = loop {
            match self.listener.accept() {
                Ok((s, _)) => break s,
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    assert!(
                        self.child.try_wait().unwrap().is_none(),
                        "CLI exited before expected request"
                    );
                    assert!(Instant::now() < deadline, "concurrent request missing");
                    std::thread::sleep(Duration::from_millis(2));
                }
                Err(e) => panic!("{e}"),
            }
        };
        stream.set_nonblocking(false).unwrap();
        stream.set_read_timeout(Some(WAIT)).unwrap();
        stream.set_write_timeout(Some(WAIT)).unwrap();
        let mut reader = BufReader::new(stream);
        let mut length = None;
        loop {
            let mut line = String::new();
            assert_ne!(reader.read_line(&mut line).unwrap(), 0);
            if line == "\r\n" {
                break;
            }
            if let Some((k, v)) = line.split_once(':') {
                if k.eq_ignore_ascii_case("content-length") {
                    length = Some(v.trim().parse::<usize>().unwrap());
                }
            }
        }
        let mut bytes = vec![0; length.unwrap()];
        reader.read_exact(&mut bytes).unwrap();
        Request {
            stream: reader.into_inner(),
            body: serde_json::from_slice(&bytes).unwrap(),
        }
    }
    fn finish(&mut self) -> (Value, Vec<Value>, std::path::PathBuf) {
        let deadline = Instant::now() + WAIT;
        while self.child.try_wait().unwrap().is_none() {
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(2));
        }
        let run = fs::read_dir(self.temp.path().join("state"))
            .unwrap()
            .map(|e| e.unwrap().path())
            .find(|p| p.is_dir())
            .unwrap();
        let report = if run.join("error.json").exists() {
            "error.json"
        } else {
            "report.json"
        };
        let report: Value = serde_json::from_slice(&fs::read(run.join(report)).unwrap()).unwrap();
        assert_eq!(
            self.child.try_wait().unwrap().unwrap().success(),
            report["status"] == "verified"
        );
        let events = fs::read_to_string(run.join("events.jsonl"))
            .unwrap()
            .lines()
            .map(|s| serde_json::from_str(s).unwrap())
            .collect();
        for file in ["a.txt", "b.txt"] {
            assert_eq!(
                fs::read_to_string(self.temp.path().join("source").join(file)).unwrap(),
                "old"
            );
        }
        (report, events, run)
    }
}
impl Drop for Run {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
fn plan() -> Value {
    json!({"tasks":[{"task":"TASK_ALPHA","write_paths":["a.txt"]},{"task":"TASK_BETA","write_paths":["b.txt"]}]})
}
#[test]
fn invalid_scopes_fail_before_any_worker_or_write() {
    for paths in [
        json!(["a.txt", "a.txt"]),
        json!(["a.txt", "A.txt"]),
        json!(["New.txt", "new.txt"]),
        json!(["Dir", "dir/file"]),
        json!(["a.txt", "a.txt/child"]),
        json!(["dir", "dir/file"]),
        json!(["./a.txt", "b.txt"]),
        json!(["x/../a.txt", "b.txt"]),
        json!(["/a.txt", "b.txt"]),
        json!(["x//a", "b.txt"]),
        json!(["x/", "b.txt"]),
        json!(["", "b.txt"]),
        json!(["x\\a", "b.txt"]),
        json!(["C:/a", "b.txt"]),
        json!(["a.txt/child", "b.txt"]),
    ] {
        let mut run = Run::new(false);
        let p = json!({"tasks":[{"task":"a","write_paths":[paths[0]]},{"task":"b","write_paths":[paths[1]]}]});
        run.next().text(&p.to_string());
        let (report, events, path) = run.finish();
        assert_eq!(report["status"], "infrastructure_error");
        assert!(
            report["error"]
                .as_str()
                .unwrap()
                .contains("planner write_paths"),
            "{report}"
        );
        assert!(events
            .iter()
            .all(|e| e["kind"] != "tool_request" && e["kind"] != "worker"));
        assert!(!path.join("learning.json").exists());
        assert!(run.listener.accept().is_err());
    }
}
#[test]
fn malformed_structured_tasks_never_delegate() {
    for plan in [
        r#"{"tasks":[["a",["a.txt"]],{"task":"b","write_paths":["b.txt"]}]}"#,
        r#"{"tasks":[{"task":"a","write_paths":[]},{"task":"b","write_paths":["b.txt"]}]}"#,
        r#"{"tasks":[{"task":"a","write_paths":["a.txt","a.txt"]},{"task":"b","write_paths":["b.txt"]}]}"#,
        r#"{"tasks":[{"task":"a","task":"x","write_paths":["a.txt"]},"b"]}"#,
        r#"{"tasks":[{"task":"a","write_paths":["a.txt"],"write_paths":["b.txt"]},"b"]}"#,
        r#"{"tasks":[{"task":"a","write_paths":["a.txt"],"extra":true},"b"]}"#,
        r#"{"tasks":[{"task":"a"},"b"]}"#,
        r#"{"tasks":[{"task":"a","write_paths":null},"b"]}"#,
    ] {
        let mut run = Run::new(false);
        run.next().text(plan);
        let (report, events, path) = run.finish();
        assert_eq!(report["status"], "infrastructure_error");
        assert!(report["error"].as_str().unwrap().starts_with("planner"));
        assert!(events.iter().all(|e| e["kind"] != "worker_scopes"));
        assert!(!path.join("learning.json").exists());
        assert!(!run.temp.path().join("state/policy.json").exists());
        assert!(run.listener.accept().is_err());
    }
}
#[test]
fn failed_worker_joins_writing_sibling_and_retains_artifacts_without_reward() {
    let mut run = Run::new(false);
    run.next().text(&plan().to_string());
    let a = run.next();
    let b = run.next();
    let mut workers = vec![a, b];
    workers.sort_by_key(Request::index);
    let b = workers.pop().unwrap();
    let a = workers.pop().unwrap();
    a.reply(json!({"role":"assistant","tool_calls":[{"id":"bad","type":"invalid"}]}));
    // The sibling is still blocked on its first response: no integrator/error may finish yet.
    assert!(run.child.try_wait().unwrap().is_none());
    b.calls(vec![call("read", "read_file", json!({"path":"b.txt"}))]);
    let b = run.next();
    let hash = b.result()["sha256"].clone();
    b.calls(vec![call(
        "write",
        "write_file",
        json!({"path":"b.txt","content":"new","expected_sha256":hash}),
    )]);
    let b = run.next();
    assert_eq!(b.result()["written"], "b.txt");
    b.text("sibling completed");
    let (report, events, path) = run.finish();
    assert_eq!(report["status"], "infrastructure_error");
    assert_eq!(report["policy_updated"], false);
    assert_eq!(
        fs::read_to_string(path.join("candidate/b.txt")).unwrap(),
        "new"
    );
    assert_eq!(
        events
            .iter()
            .filter(|e| e["kind"] == "worker_outcome")
            .count(),
        2
    );
    assert!(events
        .iter()
        .all(|e| e["kind"] != "implementer" && e["kind"] != "verification"));
    assert!(!path.join("learning.json").exists());
    assert!(!run.temp.path().join("state/policy.json").exists());
    assert!(run.listener.accept().is_err());
}
#[test]
fn coding_plan_uses_three_slots_and_admits_next_task_without_batch_barrier() {
    let mut run = Run::new(false);
    let tasks: Vec<_> = (0..4)
        .map(|i| json!({"task":format!("P{i}"),"write_paths":[format!("new-{i}.txt")]}))
        .collect();
    run.next().text(&json!({"tasks":tasks}).to_string());
    let mut pending: Vec<_> = (0..3).map(|_| run.next()).collect();
    pending.sort_by_key(|r| {
        r.body["messages"][1]["content"]
            .as_str()
            .unwrap()
            .to_owned()
    });
    for (i, r) in pending.iter().enumerate() {
        assert!(r.body["messages"][1]["content"]
            .as_str()
            .unwrap()
            .contains(&format!("coding task: P{i}")));
    }
    assert!(
        run.listener.accept().is_err(),
        "fourth task exceeded three slots"
    );
    pending.pop().unwrap().text("DONE_2");
    let next = run.next();
    assert!(next.body["messages"][1]["content"]
        .as_str()
        .unwrap()
        .contains("coding task: P3"));
    next.text("DONE_3");
    pending.pop().unwrap().text("DONE_1");
    pending.pop().unwrap().text("DONE_0");
    let integrator = run.next();
    let context = integrator.body["messages"][1]["content"].as_str().unwrap();
    let positions: Vec<_> = (0..4)
        .map(|i| context.find(&format!("DONE_{i}")).unwrap())
        .collect();
    assert!(positions.windows(2).all(|p| p[0] < p[1]));
    integrator.text("integrated");
    let (report, events, _) = run.finish();
    assert_eq!(report["status"], "unverified");
    assert_eq!(events.iter().filter(|e| e["kind"] == "worker").count(), 4);
}
#[test]
fn scoped_coders_overlap_edit_and_integrate_after_join() {
    let mut run = Run::new(true);
    run.next().text(&plan().to_string());
    // Receive BOTH requests before responding: serial execution cannot pass.
    let first = run.next();
    let second = run.next();
    assert_ne!(first.index(), second.index());
    for request in [first, second] {
        let names: Vec<_> = request.body["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["function"]["name"].as_str().unwrap())
            .collect();
        assert!(names.contains(&"edit_file") && names.contains(&"write_file"));
        assert!(!names.contains(&"run_check"));
        assert!(names
            .iter()
            .all(|n| !n.starts_with("mcp__") && *n != "shell"));
        request.calls(vec![call(
            "deny",
            "write_file",
            json!({"path":"forbidden.txt","content":"bad","expected_sha256":null}),
        )]);
    }
    for _ in 0..2 {
        let r = run.next();
        assert!(r.result()["error"].is_string());
        let file = if r.index() == 0 { "a.txt" } else { "b.txt" };
        r.calls(vec![call("read", "read_file", json!({"path":file}))]);
    }
    for _ in 0..2 {
        let r = run.next();
        let hash = r.result()["sha256"].clone();
        let file = if r.index() == 0 { "a.txt" } else { "b.txt" };
        r.calls(vec![call(
            "edit",
            "edit_file",
            json!({"path":file,"old_string":"old","new_string":"new","expected_sha256":hash}),
        )]);
    }
    let first = run.next();
    let second = run.next();
    assert!(first.result()["written"].is_string());
    assert!(second.result()["written"].is_string());
    // Finish in reverse task order; integration context still uses task order.
    let mut workers = vec![first, second];
    workers.sort_by_key(Request::index);
    workers.pop().unwrap().text("BETA_DONE");
    workers.pop().unwrap().text("ALPHA_DONE");
    let integrator = run.next();
    let context = integrator.body["messages"][1]["content"].as_str().unwrap();
    assert!(context.find("ALPHA_DONE").unwrap() < context.find("BETA_DONE").unwrap());
    integrator.calls(vec![
        call("a", "read_file", json!({"path":"a.txt"})),
        call("b", "read_file", json!({"path":"b.txt"})),
    ]);
    let integrator = run.next();
    let results: Vec<Value> = integrator.body["messages"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|m| m["role"] == "tool")
        .map(|m| serde_json::from_str(m["content"].as_str().unwrap()).unwrap())
        .collect();
    assert!(results.iter().all(|v| v["content"] == "new"));
    integrator.text("integrated");
    let (report, events, path) = run.finish();
    assert_eq!(report["status"], "verified");
    assert!(!path.join("candidate/forbidden.txt").exists());
    let edits: Vec<_> = events
        .iter()
        .filter(|e| e["kind"] == "tool_request" && e["data"]["name"] == "edit_file")
        .collect();
    assert_eq!(edits.len(), 2);
    for e in edits {
        assert!(e["data"]["worker"].is_u64());
        assert_eq!(e["data"]["write_paths"].as_array().unwrap().len(), 1);
    }
}
