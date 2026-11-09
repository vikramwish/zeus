use serde_json::{json, Value};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::Path;
use std::process::Command;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::{Duration, Instant};

// Loopback chat-completions fixture: answers one request with a final text
// message, or holds it open until `stop` is set. No live model is involved.
fn fixture(hang: bool) -> (String, Arc<AtomicBool>, Arc<AtomicBool>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let endpoint = format!(
        "http://{}/v1/chat/completions",
        listener.local_addr().unwrap()
    );
    let received = Arc::new(AtomicBool::new(false));
    let stop = Arc::new(AtomicBool::new(false));
    let (seen, done) = (received.clone(), stop.clone());
    std::thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(20);
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
            seen.store(true, Ordering::Relaxed);
            if hang {
                while !done.load(Ordering::Relaxed) && Instant::now() < deadline {
                    std::thread::sleep(Duration::from_millis(10));
                }
                continue;
            }
            let body = json!({"choices":[{"message":{"role":"assistant","content":"Fixture finished."},"finish_reason":"stop"}]}).to_string();
            let _ = write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", body.len(), body);
        }
    });
    (endpoint, received, stop)
}

fn write_job(
    dir: &Path,
    endpoint: &str,
    model_timeout: u64,
) -> (std::path::PathBuf, std::path::PathBuf) {
    let source = dir.join("source");
    std::fs::create_dir(&source).unwrap();
    std::fs::write(source.join("input.txt"), "unchanged").unwrap();
    let state = dir.join("state");
    let job = dir.join("job.json");
    std::fs::write(
        &job,
        json!({"task":"Inspect only","source":source,"state_dir":state,"endpoint":endpoint,"model":"fixture","model_timeout":model_timeout}).to_string(),
    )
    .unwrap();
    (job, state)
}

fn private_state(state: &Path) {
    use std::os::unix::fs::DirBuilderExt;
    std::fs::DirBuilder::new()
        .mode(0o700)
        .create(state)
        .unwrap();
}

fn exited_pid() -> u32 {
    let child = Command::new("/usr/bin/true").spawn().unwrap();
    let pid = child.id();
    let mut child = child;
    child.wait().unwrap();
    pid
}

#[test]
fn stale_lock_from_exited_process_is_reclaimed() {
    let temp = tempfile::tempdir().unwrap();
    let (endpoint, _received, stop) = fixture(false);
    let (job, state) = write_job(temp.path(), &endpoint, 10);
    private_state(&state);
    std::fs::write(state.join("run.lock"), format!("{}\n", exited_pid())).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_zeus"))
        .args(["run", job.to_str().unwrap(), "--allow-network"])
        .output()
        .unwrap();
    stop.store(true, Ordering::Relaxed);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(!stderr.contains("state is locked"), "{stderr}");
    let report: Value = serde_json::from_slice(&out.stdout).unwrap_or_else(|_| panic!("{stderr}"));
    assert_eq!(report["status"], "unverified");
    assert!(!state.join("run.lock").exists());
}

#[test]
fn lock_held_by_live_process_is_refused() {
    let temp = tempfile::tempdir().unwrap();
    let (endpoint, received, stop) = fixture(false);
    let (job, state) = write_job(temp.path(), &endpoint, 10);
    private_state(&state);
    let lock = state.join("run.lock");
    std::fs::write(&lock, format!("{}\n", std::process::id())).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_zeus"))
        .args(["run", job.to_str().unwrap(), "--allow-network"])
        .output()
        .unwrap();
    stop.store(true, Ordering::Relaxed);
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("state is locked"), "{stderr}");
    assert!(stderr.contains(&std::process::id().to_string()), "{stderr}");
    assert!(lock.exists());
    assert!(!received.load(Ordering::Relaxed));
}

#[test]
fn unparsable_lock_is_refused_not_reclaimed() {
    let temp = tempfile::tempdir().unwrap();
    let (endpoint, _received, stop) = fixture(false);
    let (job, state) = write_job(temp.path(), &endpoint, 10);
    private_state(&state);
    let lock = state.join("run.lock");
    std::fs::write(&lock, "not a pid\n").unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_zeus"))
        .args(["run", job.to_str().unwrap(), "--allow-network"])
        .output()
        .unwrap();
    stop.store(true, Ordering::Relaxed);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("state is locked"));
    assert!(lock.exists());
}

#[test]
fn sigterm_during_model_call_records_cancellation_and_releases_lock() {
    let temp = tempfile::tempdir().unwrap();
    let (endpoint, received, stop) = fixture(true);
    let (job, state) = write_job(temp.path(), &endpoint, 30);
    let child = Command::new(env!("CARGO_BIN_EXE_zeus"))
        .args(["run", job.to_str().unwrap(), "--allow-network"])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let started = Instant::now();
    while !received.load(Ordering::Relaxed) {
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "fixture never received a request"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(state.join("run.lock").exists());
    unsafe {
        assert_eq!(libc::kill(child.id() as i32, libc::SIGTERM), 0);
    }
    let signalled = Instant::now();
    let out = child.wait_with_output().unwrap();
    stop.store(true, Ordering::Relaxed);
    // The 30s model deadline must not be what ends the run.
    assert!(
        signalled.elapsed() < Duration::from_secs(10),
        "cancellation took {:?}",
        signalled.elapsed()
    );
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("cancelled"), "{stderr}");
    assert!(!state.join("run.lock").exists(), "stale lock left behind");
    let run_dir = std::fs::read_dir(&state)
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .find(|p| p.is_dir())
        .expect("run directory retained");
    let error: Value =
        serde_json::from_slice(&std::fs::read(run_dir.join("error.json")).unwrap()).unwrap();
    assert_eq!(error["status"], "cancelled");
    let events = std::fs::read_to_string(run_dir.join("events.jsonl")).unwrap();
    let last: Value = serde_json::from_str(events.lines().last().unwrap()).unwrap();
    assert_eq!(last["kind"], "run_error", "{events}");
    assert_eq!(last["data"]["status"], "cancelled");
    // A cancelled run is browsable and labelled as such, not as an infrastructure error.
    let list = Command::new(env!("CARGO_BIN_EXE_zeus"))
        .args(["sessions", "list", state.to_str().unwrap()])
        .output()
        .unwrap();
    let list: Value = serde_json::from_slice(&list.stdout).unwrap();
    assert_eq!(list["sessions"][0]["status"], "cancelled", "{list}");
}
