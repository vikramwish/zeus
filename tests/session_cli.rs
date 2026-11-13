use serde_json::{json, Value};
use std::process::{Command, Output};

fn cli(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_zeus"))
        .args(args)
        .output()
        .unwrap()
}
fn json_output(output: Output) -> Value {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

fn console(job: &std::path::Path, flags: &[&str], input: &str) -> Output {
    use std::io::Write;
    use std::process::Stdio;
    let mut child = Command::new(env!("CARGO_BIN_EXE_zeus"))
        .arg("console")
        .arg(job)
        .args(flags)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    if let Err(err) = child.stdin.take().unwrap().write_all(input.as_bytes()) {
        assert_eq!(err.kind(), std::io::ErrorKind::BrokenPipe, "{err}");
    }
    child.wait_with_output().unwrap()
}

fn job_file(temp: &tempfile::TempDir, endpoint: &str) -> std::path::PathBuf {
    let source = temp.path().join("source");
    std::fs::create_dir(&source).unwrap();
    std::fs::write(source.join("input.txt"), "original").unwrap();
    let job = temp.path().join("job.json");
    std::fs::write(&job, json!({"task":"configured task", "source":source,"state_dir":temp.path().join("state"),"endpoint":endpoint,"model":"scripted-fixture","max_turns":1}).to_string()).unwrap();
    job
}

#[test]
fn console_runs_independent_tasks_and_retains_browsable_proof() {
    use std::io::{Read, Write};
    let temp = tempfile::tempdir().unwrap();
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let job = job_file(
        &temp,
        &format!(
            "http://{}/v1/chat/completions",
            listener.local_addr().unwrap()
        ),
    );
    let server = std::thread::spawn(move || {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(8);
        let mut requests = Vec::new();
        while requests.len() < 2 && std::time::Instant::now() < deadline {
            let (mut stream, _) = match listener.accept() {
                Ok(v) => v,
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(std::time::Duration::from_millis(10));
                    continue;
                }
                Err(e) => panic!("{e}"),
            };
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
            let length: usize = String::from_utf8(header)
                .unwrap()
                .lines()
                .find_map(|line| {
                    line.to_ascii_lowercase()
                        .strip_prefix("content-length:")
                        .map(|v| v.trim().parse().unwrap())
                })
                .unwrap();
            let mut body = vec![0; length];
            stream.read_exact(&mut body).unwrap();
            requests.push(serde_json::from_slice::<Value>(&body).unwrap());
            let body = json!({"choices":[{"message":{"role":"assistant","content":"fixture response, not model quality proof"},"finish_reason":"stop"}]}).to_string();
            write!(stream,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",body.len(),body).unwrap();
        }
        requests
    });
    let out = console(
        &job,
        &["--allow-network"],
        "/help\n/run\nsecond independent task\n/list\n/quit\n",
    );
    let requests = server.join().unwrap();
    assert_eq!(
        requests.len(),
        2,
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        !out.status.success(),
        "unverified tasks must not become success"
    );
    assert!(String::from_utf8_lossy(&out.stderr).contains("not transcript resume"));
    assert!(requests[0]["messages"][1]["content"]
        .as_str()
        .unwrap()
        .contains("configured task"));
    assert!(requests[1]["messages"][1]["content"]
        .as_str()
        .unwrap()
        .contains("second independent task"));
    assert!(!requests[1].to_string().contains("fixture response"));
    let list = json_output(cli(&[
        "sessions",
        "list",
        temp.path().join("state").to_str().unwrap(),
    ]));
    assert_eq!(list["sessions"].as_array().unwrap().len(), 2);
    for run in list["sessions"].as_array().unwrap() {
        assert_eq!(run["status"], "unverified");
    }
    assert_eq!(
        std::fs::read_to_string(temp.path().join("source/input.txt")).unwrap(),
        "original"
    );
    assert!(!temp.path().join("state/policy.json").exists());
}

#[test]
fn console_requires_approvals_and_does_not_run_on_entry() {
    let temp = tempfile::tempdir().unwrap();
    let job = job_file(&temp, "http://127.0.0.1:9/v1/chat/completions");
    let denied = console(&job, &[], "/run\n");
    assert!(!denied.status.success());
    assert!(String::from_utf8_lossy(&denied.stderr).contains("--allow-network"));
    let idle = console(&job, &["--allow-network"], "/help\n/quit\n");
    assert!(
        idle.status.success(),
        "{}",
        String::from_utf8_lossy(&idle.stderr)
    );
    assert!(!temp.path().join("state").exists());
    for (field, value, permission) in [
        ("verifier", json!(["/usr/bin/true"]), "--allow-exec"),
        (
            "mcp",
            json!([{"name":"fixture", "command":["/usr/bin/false"],"allowed_tools":["read"]}]),
            "--allow-mcp",
        ),
    ] {
        let mut data: Value = serde_json::from_slice(&std::fs::read(&job).unwrap()).unwrap();
        data.as_object_mut().unwrap().remove("verifier");
        data[field] = value;
        std::fs::write(&job, data.to_string()).unwrap();
        let out = console(&job, &["--allow-network"], "/run\n");
        assert!(!out.status.success());
        assert!(String::from_utf8_lossy(&out.stderr).contains(permission));
        assert!(!temp.path().join("state").exists());
    }
}

#[test]
fn browse_and_search_existing_run_artifacts_without_approvals() {
    let temp = tempfile::tempdir().unwrap();
    let state = temp.path().to_str().unwrap();
    let run = temp.path().join("run-123-456");
    std::fs::create_dir(&run).unwrap();
    std::fs::write(run.join("events.jsonl"), format!("{}\n", json!({"kind":"start","data":{"task":"Investigate parser","source":"/fixture/source","model":"fixture"}}))).unwrap();
    std::fs::write(
        run.join("report.json"),
        json!({"status":"unverified","summary":"Parser inspected","policy_updated":false})
            .to_string(),
    )
    .unwrap();
    let list = json_output(cli(&["sessions", "list", state]));
    assert_eq!(list["sessions"][0]["id"], "run-123-456");
    assert_eq!(list["sessions"][0]["task"], "Investigate parser");
    assert_eq!(list["sessions"][0]["status"], "unverified");
    let found = json_output(cli(&["sessions", "search", state, "PARSER"]));
    assert_eq!(found["sessions"].as_array().unwrap().len(), 1);
    let absent = json_output(cli(&["sessions", "search", state, "absent"]));
    assert!(absent["sessions"].as_array().unwrap().is_empty());
    let shown = json_output(cli(&["sessions", "show", state, "run-123-456"]));
    assert_eq!(shown["report"]["summary"], "Parser inspected");
    assert!(!temp.path().join("run.lock").exists());
    assert!(!temp.path().join("policy.json").exists());
}

#[test]
fn browsing_rejects_traversal_symlinks_corrupt_and_oversized_artifacts() {
    let temp = tempfile::tempdir().unwrap();
    let state = temp.path().to_str().unwrap();
    let run = temp.path().join("run-1-1");
    std::fs::create_dir(&run).unwrap();
    assert_eq!(
        json_output(cli(&["sessions", "show", state, "run-1-1"]))["status"],
        "incomplete"
    );
    for id in [
        "../run-1-1",
        "/run-1-1",
        "run-1-1/../run-1-1",
        "run-1",
        "run-1-x",
    ] {
        assert!(!cli(&["sessions", "show", state, id]).status.success());
    }
    std::fs::write(run.join("report.json"), "{").unwrap();
    assert!(!cli(&["sessions", "show", state, "run-1-1"])
        .status
        .success());
    std::fs::write(run.join("report.json"), vec![b' '; 1024 * 1024 + 1]).unwrap();
    let oversized = cli(&["sessions", "show", state, "run-1-1"]);
    assert!(!oversized.status.success());
    assert!(String::from_utf8_lossy(&oversized.stderr).contains("read limit"));
    std::fs::remove_file(run.join("report.json")).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;
        let outside = tempfile::tempdir().unwrap();
        std::fs::write(outside.path().join("secret.json"), "{}").unwrap();
        symlink(outside.path().join("secret.json"), run.join("report.json")).unwrap();
        assert!(!cli(&["sessions", "show", state, "run-1-1"])
            .status
            .success());
        symlink(outside.path(), temp.path().join("run-2-2")).unwrap();
        assert!(!cli(&["sessions", "list", state]).status.success());
    }
}

#[test]
fn console_rejects_oversized_input_unknown_commands_and_flags_without_a_run() {
    let temp = tempfile::tempdir().unwrap();
    let job = job_file(&temp, "http://127.0.0.1:9/v1/chat/completions");
    for input in [
        format!("{}\n", "x".repeat(32769)),
        "/resume\n/quit\n".into(),
        "/allow-network\n/quit\n".into(),
    ] {
        let out = console(&job, &["--allow-network"], &input);
        assert!(!out.status.success());
        assert!(!temp.path().join("state").exists());
    }
    let out = console(&job, &["--allow-network", "--unknown"], "");
    assert!(!out.status.success());
    assert!(!temp.path().join("state").exists());
}

#[test]
fn console_failure_is_retained_and_does_not_block_browsing_or_train_policy() {
    let temp = tempfile::tempdir().unwrap();
    let job = job_file(&temp, "http://127.0.0.1:9/v1/chat/completions");
    let out = console(&job, &["--allow-network"], "/run\n/list\n/quit\n");
    assert!(!out.status.success());
    let list: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(list["sessions"][0]["status"], "infrastructure_error");
    let id = list["sessions"][0]["id"].as_str().unwrap();
    let shown = json_output(cli(&[
        "sessions",
        "show",
        temp.path().join("state").to_str().unwrap(),
        id,
    ]));
    assert_eq!(shown["error"]["policy_updated"], false);
    assert!(!temp.path().join("state/policy.json").exists());
    assert!(!temp.path().join("state/run.lock").exists());
}

#[cfg(target_os = "macos")]
#[test]
fn console_preserves_successful_external_verifier_outcome() {
    let temp = tempfile::tempdir().unwrap();
    let job = job_file(&temp, "http://127.0.0.1:9/v1/chat/completions");
    let mut data: Value = serde_json::from_slice(&std::fs::read(&job).unwrap()).unwrap();
    data["verifier"] = json!(["/usr/bin/true"]);
    std::fs::write(&job, data.to_string()).unwrap();
    let out = console(&job, &["--allow-network", "--allow-exec"], "/run\n/quit\n");
    let report = json_output(out);
    assert_eq!(report["status"], "already_verified");
    // This deliberately trivial fixture proves status plumbing, not useful task verification.
    assert_eq!(report["policy_updated"], false);
}
