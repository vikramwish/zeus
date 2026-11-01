use serde_json::json;
use std::{
    io::{Read, Write},
    process::Command,
};

#[test]
fn cli_uses_selected_responses_protocol_without_api_billing_fallback() {
    let tmp = tempfile::tempdir().unwrap();
    let source = tmp.path().join("source");
    std::fs::create_dir(&source).unwrap();
    std::fs::write(source.join("file.txt"), "read-only fixture").unwrap();
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let endpoint = format!("http://{}/v1/responses", listener.local_addr().unwrap());
    let server = std::thread::spawn(move || {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        let mut stream = loop {
            match listener.accept() {
                Ok((stream, _)) => break stream,
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    assert!(std::time::Instant::now() < deadline, "CLI did not connect");
                    std::thread::sleep(std::time::Duration::from_millis(10));
                }
                Err(e) => panic!("{e}"),
            }
        };
        stream
            .set_read_timeout(Some(std::time::Duration::from_secs(5)))
            .unwrap();
        let mut header = vec![];
        while !header.ends_with(b"\r\n\r\n") {
            let mut b = [0];
            stream.read_exact(&mut b).unwrap();
            header.push(b[0]);
            assert!(header.len() < 65536);
        }
        let header = String::from_utf8(header).unwrap();
        let len: usize = header
            .lines()
            .find_map(|line| {
                line.to_ascii_lowercase()
                    .strip_prefix("content-length:")
                    .map(|v| v.trim().parse().unwrap())
            })
            .unwrap();
        assert!(len < 4 * 1024 * 1024);
        let mut body = vec![0; len];
        stream.read_exact(&mut body).unwrap();
        let request: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let event = json!({"type":"response.completed", "response":{"status":"completed", "output":[{"type":"message", "role":"assistant", "status":"completed", "content":[{"type":"output_text", "text":"Inspected fixture; no verifier supplied.", "annotations":[]}]}]}});
        let body = format!("event: response.completed\ndata: {event}\n\n");
        write!(stream,"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",body.len(),body).unwrap();
        request
    });
    let job = tmp.path().join("job.json");
    std::fs::write(&job, serde_json::to_vec(&json!({"task":"Inspect only", "source":source, "state_dir":tmp.path().join("state"), "endpoint":endpoint, "model":"arbitrary-custom-model", "protocol":"responses", "max_turns":1})).unwrap()).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_zeus"))
        .arg("run")
        .arg(job)
        .arg("--allow-network")
        .output()
        .unwrap();
    let request = server.join().unwrap();
    assert_eq!(request["stream"], true, "runtime ignored selected protocol");
    assert_eq!(request["store"], false);
    assert_eq!(request["model"], "arbitrary-custom-model");
    let report: serde_json::Value = serde_json::from_slice(&out.stdout)
        .unwrap_or_else(|_| panic!("{}", String::from_utf8_lossy(&out.stderr)));
    assert_eq!(report["status"], "unverified");
    assert!(
        !out.status.success(),
        "no verifier must not be presented as verified"
    );
    assert!(!tmp.path().join("state/policy.json").exists());
}
