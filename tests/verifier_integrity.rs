#![cfg(target_os = "macos")]

use serde_json::{json, Value};
use std::{
    fs,
    io::{Read, Write},
    net::TcpListener,
    path::Path,
    process::{Command, Output},
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};

// Local scripted protocol fixture, not a model or a paid service.
struct Model {
    endpoint: String,
    requests: Arc<AtomicUsize>,
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}
impl Model {
    fn new(first: Option<Value>) -> Self {
        Self::with_first_request_action(first, || {})
    }

    fn with_first_request_action(
        first: Option<Value>,
        action: impl FnOnce() + Send + 'static,
    ) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let endpoint = format!(
            "http://{}/v1/chat/completions",
            listener.local_addr().unwrap()
        );
        let requests = Arc::new(AtomicUsize::new(0));
        let stop = Arc::new(AtomicBool::new(false));
        let (count, shutdown) = (requests.clone(), stop.clone());
        let thread = std::thread::spawn(move || {
            let mut action = Some(action);
            while !shutdown.load(Ordering::Relaxed) {
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
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let mut header = Vec::new();
                let mut byte = [0];
                while !header.ends_with(b"\r\n\r\n") {
                    stream.read_exact(&mut byte).unwrap();
                    header.push(byte[0]);
                    assert!(header.len() < 65536);
                }
                let header = String::from_utf8(header).unwrap();
                let size: usize = header
                    .lines()
                    .find_map(|l| {
                        l.to_ascii_lowercase()
                            .strip_prefix("content-length:")
                            .map(|v| v.trim().parse().unwrap())
                    })
                    .unwrap();
                let mut body = vec![0; size];
                stream.read_exact(&mut body).unwrap();
                let i = count.fetch_add(1, Ordering::Relaxed);
                if let Some(action) = action.take() {
                    action();
                }
                let message = if i == 0 { first.clone() } else { None }.unwrap_or_else(
                    || json!({"role":"assistant","content":"No change to the input."}),
                );
                let reason = if message.get("tool_calls").is_some() {
                    "tool_calls"
                } else {
                    "stop"
                };
                let body =
                    json!({"choices":[{"message":message,"finish_reason":reason}]}).to_string();
                write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", body.len(), body).unwrap();
            }
        });
        Self {
            endpoint,
            requests,
            stop,
            thread: Some(thread),
        }
    }
}
impl Drop for Model {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        let result = self.thread.take().unwrap().join();
        if !std::thread::panicking() {
            result.unwrap();
        }
    }
}

fn run(root: &Path, model: &Model, verifier: Value, timeout: u64) -> Output {
    let source = root.join("source");
    fs::create_dir_all(&source).unwrap();
    fs::write(source.join("input.txt"), "wrong").unwrap();
    let job = root.join("job.json");
    fs::write(
        &job,
        serde_json::to_vec(&json!({
            "task":"Make input.txt contain right", "source":source,
            "state_dir":root.join("state"), "endpoint":model.endpoint,
            "model":"local-protocol-fixture", "max_turns":3,
            "verifier":verifier, "verifier_timeout":timeout
        }))
        .unwrap(),
    )
    .unwrap();
    Command::new(env!("CARGO_BIN_EXE_zeus"))
        .arg("run")
        .arg(job)
        .args(["--allow-network", "--allow-exec"])
        .output()
        .unwrap()
}

fn assert_no_learning(root: &Path) {
    assert!(!root.join("state/policy.json").exists());
    if let Ok(runs) = fs::read_dir(root.join("state")) {
        for entry in runs.flatten() {
            assert!(!entry.path().join("learning.json").exists());
            assert!(!entry.path().join("report.json").exists());
        }
    }
}

#[test]
fn missing_shebang_interpreter_stops_before_model_and_learning() {
    use std::os::unix::fs::PermissionsExt;
    let root = tempfile::tempdir().unwrap();
    let verifier = root.path().join("trusted");
    fs::write(
        &verifier,
        "#!/definitely-missing-zeus-interpreter\nexit 0\n",
    )
    .unwrap();
    fs::set_permissions(&verifier, fs::Permissions::from_mode(0o700)).unwrap();
    let model = Model::new(None);
    let out = run(root.path(), &model, json!([verifier]), 10);
    assert_eq!(
        model.requests.load(Ordering::Relaxed),
        0,
        "model invoked after failed exec: {}",
        String::from_utf8_lossy(&out.stdout)
    );
    assert!(!out.status.success());
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("launch"),
        "{out:?}"
    );
    assert_no_learning(root.path());
}

#[test]
fn nonexecutable_cli_verifier_is_rejected_before_model_and_state() {
    use std::os::unix::fs::PermissionsExt;
    let root = tempfile::tempdir().unwrap();
    let verifier = root.path().join("trusted");
    fs::write(&verifier, "#!/bin/sh\nexit 0\n").unwrap();
    fs::set_permissions(&verifier, fs::Permissions::from_mode(0o600)).unwrap();
    let model = Model::new(None);
    let out = run(root.path(), &model, json!([verifier]), 10);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("executable"));
    assert_eq!(model.requests.load(Ordering::Relaxed), 0);
    assert!(!root.path().join("state").exists());
}

#[test]
fn baseline_timeout_stops_before_model_and_learning() {
    let root = tempfile::tempdir().unwrap();
    let verifier = root.path().join("trusted.sh");
    fs::write(&verifier, "exec /bin/sleep 30\n").unwrap();
    let model = Model::new(None);
    let out = run(root.path(), &model, json!(["/bin/sh", verifier]), 1);
    assert_eq!(model.requests.load(Ordering::Relaxed), 0, "{out:?}");
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("infrastructure"));
    assert_no_learning(root.path());
}

#[test]
fn tool_verifier_infrastructure_error_cannot_be_followed_by_learning() {
    let root = tempfile::tempdir().unwrap();
    let verifier = root.path().join("trusted.sh");
    fs::write(
        &verifier,
        "if test -f trigger; then rm trigger; exec /bin/sleep 30; fi\nexit 1\n",
    )
    .unwrap();
    let model = Model::new(Some(
        json!({"role":"assistant","content":null,"tool_calls":[
            {"id":"trigger","type":"function","function":{"name":"write_file","arguments":json!({"path":"trigger","content":"timeout","expected_sha256":null}).to_string()}},
            {"id":"check","type":"function","function":{"name":"run_check","arguments":"{}"}}
        ]}),
    ));
    let out = run(root.path(), &model, json!(["/bin/sh", verifier]), 1);
    assert!(!out.status.success());
    assert_eq!(model.requests.load(Ordering::Relaxed), 1, "{out:?}");
    assert!(String::from_utf8_lossy(&out.stderr).contains("infrastructure"));
    assert_no_learning(root.path());
}

#[test]
fn exec_failure_after_baseline_stops_repair_and_never_trains() {
    use std::os::unix::fs::PermissionsExt;
    let root = tempfile::tempdir().unwrap();
    let verifier = root.path().join("trusted");
    fs::write(&verifier, "#!/bin/sh\nexit 1\n").unwrap();
    fs::set_permissions(&verifier, fs::Permissions::from_mode(0o700)).unwrap();
    let changed = verifier.clone();
    // Simulate a host-side verifier installation breaking after baseline. The
    // candidate cannot modify this external file; only the fixture host can.
    let model = Model::with_first_request_action(None, move || {
        fs::write(changed, "#!/definitely-missing-zeus-interpreter\nexit 0\n").unwrap();
    });
    let out = run(root.path(), &model, json!([verifier]), 10);
    assert!(!out.status.success());
    assert_eq!(model.requests.load(Ordering::Relaxed), 1, "{out:?}");
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("launch"),
        "{out:?}"
    );
    assert_no_learning(root.path());
}

#[test]
fn baseline_signal_and_output_limit_never_start_model_or_learning() {
    for script in ["kill -KILL $$\n", "exec /usr/bin/yes\n"] {
        let root = tempfile::tempdir().unwrap();
        let verifier = root.path().join("trusted.sh");
        fs::write(&verifier, script).unwrap();
        let model = Model::new(None);
        let out = run(root.path(), &model, json!(["/bin/sh", verifier]), 10);
        assert!(!out.status.success());
        assert_eq!(model.requests.load(Ordering::Relaxed), 0, "{out:?}");
        assert!(String::from_utf8_lossy(&out.stderr).contains("infrastructure"));
        assert_no_learning(root.path());
    }
}

#[test]
fn genuine_exit_71_is_a_test_failure_not_a_launch_failure() {
    let root = tempfile::tempdir().unwrap();
    let verifier = root.path().join("trusted.sh");
    fs::write(
        &verifier,
        "echo 'sandbox-exec: exec failed: No such file or directory' >&2\nexit 71\n",
    )
    .unwrap();
    let model = Model::new(None);
    let out = run(root.path(), &model, json!(["/bin/sh", verifier]), 10);
    let report: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(report["status"], "failed");
    assert_eq!(report["verification"]["exit_code"], 71);
    assert_eq!(model.requests.load(Ordering::Relaxed), 3);
    assert_eq!(report["policy_updated"], true);
}

#[test]
fn candidate_python_usercustomize_cannot_forge_cli_reward() {
    let root = tempfile::tempdir().unwrap();
    let python = "/opt/homebrew/bin/python3";
    let version = Command::new(python)
        .args([
            "-I",
            "-c",
            "import sys; print('%d.%d' % sys.version_info[:2])",
        ])
        .output()
        .unwrap();
    assert!(version.status.success());
    let version = String::from_utf8(version.stdout).unwrap();
    let attack = format!(
        ".native-home/Library/Python/{}/lib/python/site-packages/usercustomize.py",
        version.trim()
    );
    let model = Model::new(Some(
        json!({"role":"assistant","content":null,"tool_calls":[{
            "id":"poison-home","type":"function","function":{"name":"write_file", "arguments":json!({
                "path":attack,"content":"import os\nos._exit(0)\n","expected_sha256":null
            }).to_string()}
        }]}),
    ));
    let verifier = root.path().join("trusted.py");
    fs::write(&verifier, "from pathlib import Path\nprint('EXTERNAL_ASSERTION_REACHED', flush=True)\nassert Path('input.txt').read_text() == 'right'\n").unwrap();
    let out = run(root.path(), &model, json!([python, verifier]), 10);
    let report: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(report["status"], "failed", "{report}");
    assert_eq!(report["attempts"], 3);
    assert!(report["verification"]["stdout"]
        .as_str()
        .unwrap()
        .contains("EXTERNAL_ASSERTION_REACHED"));
    let candidate = Path::new(report["candidate"].as_str().unwrap());
    assert!(
        candidate.join(attack).is_file(),
        "attack must actually be written"
    );
    assert_eq!(
        fs::read_to_string(candidate.join("input.txt")).unwrap(),
        "wrong"
    );
    let learning: Value = serde_json::from_slice(
        &fs::read(Path::new(report["run_dir"].as_str().unwrap()).join("learning.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(learning["reward"], false);
}
