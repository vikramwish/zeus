use serde_json::json;
use std::{
    io::{Read, Write},
    net::TcpListener,
    sync::{Arc, Barrier},
    time::{Duration, Instant},
};
use zeus::model::ChatClient;

#[test]
fn cloned_parallel_clients_share_one_atomic_request_budget() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let endpoint = format!("http://{}/", listener.local_addr().unwrap());
    let client = ChatClient::new(&endpoint, "fixture", None)
        .unwrap()
        .with_request_limit(4)
        .unwrap();
    let server = std::thread::spawn(move || {
        for _ in 0..4 {
            let deadline = Instant::now() + Duration::from_secs(5);
            let mut stream = loop {
                match listener.accept() {
                    Ok((s, _)) => break s,
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        assert!(Instant::now() < deadline, "missing admitted request");
                        std::thread::sleep(Duration::from_millis(2));
                    }
                    Err(e) => panic!("{e}"),
                }
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
            }
            let length = String::from_utf8(header)
                .unwrap()
                .lines()
                .find_map(|line| {
                    line.to_ascii_lowercase()
                        .strip_prefix("content-length:")
                        .map(|v| v.trim().parse::<usize>().unwrap())
                })
                .unwrap();
            stream.read_exact(&mut vec![0; length]).unwrap();
            let body =
                json!({"choices":[{"message":{"role":"assistant","content":"done"}}]}).to_string();
            write!(stream,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).unwrap();
        }
        assert!(listener.accept().is_err(), "extra request escaped budget");
    });
    let gate = Arc::new(Barrier::new(16));
    let results = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..16)
            .map(|_| {
                let client = client.clone();
                let gate = gate.clone();
                scope.spawn(move || {
                    gate.wait();
                    client.complete(&[json!({"role":"user","content":"test"})], &[])
                })
            })
            .collect();
        handles
            .into_iter()
            .map(|h| h.join().unwrap())
            .collect::<Vec<_>>()
    });
    server.join().unwrap();
    assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 4);
    for result in results.into_iter().filter(|r| r.is_err()) {
        assert_eq!(result.unwrap_err(), "autonomy model-call budget exhausted");
    }
    assert!(ChatClient::new(&endpoint, "fixture", None)
        .unwrap()
        .with_request_limit(0)
        .is_err());
}
