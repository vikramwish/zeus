//! Synthetic local HTTP fixtures: these tests do not contact an inference provider.
use zeus::{agent, model};

#[derive(Default)]
struct Host {
    calls: Mutex<Vec<(String, Value)>>,
}
impl agent::ToolHost for Host {
    fn schemas(&self) -> Vec<Value> {
        vec![
            json!({"type":"function","function":{"name":"inspect","parameters":{"type":"object"}}}),
        ]
    }
    fn call(&self, name: &str, args: Value) -> Result<Value, String> {
        self.calls.lock().unwrap().push((name.into(), args.clone()));
        Ok(json!({"observed":args}))
    }
}
fn tool(id: &str, name: &str, args: &str) -> Value {
    json!({"id":id,"type":"function","function":{"name":name,"arguments":args}})
}

#[test]
fn agent_validates_entire_tool_batch_before_any_dispatch() {
    let bad_calls = vec![
        tool("bad", "unknown", "{}"),
        tool("", "inspect", "{}"),
        tool("call_1", "inspect", "{}"),
        tool("bad id", "inspect", "{}"),
        tool("bad", "inspect", "not JSON"),
        tool("bad", "inspect", "[]"),
        json!({"id":"bad","type":"other","function":{"name":"inspect","arguments":"{}"}}),
        json!({"type":"function","function":{"name":"inspect","arguments":"{}"}}),
        json!({"id":"bad","type":"function","function":{"name":"inspect","arguments":{}}}),
    ];
    for bad in bad_calls {
        let server = Fixture::new(vec![
            json!({"role":"assistant","tool_calls":[tool("call_1","inspect","{}"), bad]}),
        ]);
        let host = Host::default();
        assert!(agent::run(&server.client(), &host, "system", "task", 1).is_err());
        assert!(
            host.calls.lock().unwrap().is_empty(),
            "partially executed invalid batch"
        );
    }
}

#[test]
fn agent_rejects_reused_call_ids_across_turns() {
    let message = json!({"role":"assistant","tool_calls":[tool("same","inspect","{}")]});
    let server = Fixture::new(vec![message.clone(), message]);
    let host = Host::default();
    assert!(agent::run(&server.client(), &host, "system", "task", 2).is_err());
    assert_eq!(host.calls.lock().unwrap().len(), 1);
}

#[test]
fn agent_enforces_turn_call_and_context_budgets() {
    let unused = Fixture::new(vec![]);
    for limit in [0, 65, usize::MAX] {
        let error = agent::run(&unused.client(), &Host::default(), "s", "t", limit).unwrap_err();
        assert!(error.contains("turn"), "{error}");
    }
    let error = agent::run(
        &unused.client(),
        &Host::default(),
        "s",
        &"x".repeat(4 * 1024 * 1024),
        1,
    )
    .unwrap_err();
    assert!(error.contains("context"), "{error}");
    let calls: Vec<Value> = (0..65)
        .map(|i| tool(&format!("c{i}"), "inspect", "{}"))
        .collect();
    let server = Fixture::new(vec![json!({"role":"assistant","tool_calls":calls})]);
    let host = Host::default();
    let error = agent::run(&server.client(), &host, "s", "t", 1).unwrap_err();
    assert!(error.contains("tool call"));
    assert!(host.calls.lock().unwrap().is_empty());
}

#[test]
fn agent_total_call_budget_spans_turns_and_exhaustion_is_error() {
    let calls: Vec<Value> = (0..64)
        .map(|i| tool(&format!("c{i}"), "inspect", "{}"))
        .collect();
    let server = Fixture::new(vec![
        json!({"role":"assistant","tool_calls":calls}),
        json!({"role":"assistant","tool_calls":[tool("extra","inspect","{}")]}),
    ]);
    let host = Host::default();
    let error = agent::run(&server.client(), &host, "s", "t", 2).unwrap_err();
    assert!(error.contains("tool call"), "{error}");
    assert_eq!(host.calls.lock().unwrap().len(), 64);
    let server = Fixture::new(vec![
        json!({"role":"assistant","tool_calls":[tool("one","inspect","{}")]}),
    ]);
    let error = agent::run(&server.client(), &Host::default(), "s", "t", 1).unwrap_err();
    assert!(error.contains("exhausted"));
    assert_eq!(server.requests.lock().unwrap().len(), 1);
}

struct LargeHost(usize);
impl agent::ToolHost for LargeHost {
    fn schemas(&self) -> Vec<Value> {
        Host::default().schemas()
    }
    fn call(&self, _: &str, _: Value) -> Result<Value, String> {
        Ok(json!("x".repeat(self.0)))
    }
}
#[test]
fn agent_bounds_tool_results_and_accumulated_transcript() {
    let server = Fixture::new(vec![
        json!({"role":"assistant","tool_calls":[tool("one","inspect","{}")]}),
    ]);
    let error = agent::run(&server.client(), &LargeHost(256 * 1024), "s", "t", 1).unwrap_err();
    assert!(error.contains("tool result"), "{error}");
    let calls: Vec<Value> = (0..20)
        .map(|i| tool(&format!("c{i}"), "inspect", "{}"))
        .collect();
    let server = Fixture::new(vec![json!({"role":"assistant","tool_calls":calls})]);
    let error = agent::run(&server.client(), &LargeHost(250 * 1024), "s", "t", 1).unwrap_err();
    assert!(error.contains("context"), "{error}");
}

fn parallel_server(
    count: usize,
) -> (
    ChatClient,
    Arc<std::sync::atomic::AtomicUsize>,
    Arc<std::sync::atomic::AtomicUsize>,
    thread::JoinHandle<()>,
) {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let client = ChatClient::new(
        &format!(
            "http://{}/v1/chat/completions",
            listener.local_addr().unwrap()
        ),
        "synthetic",
        None,
    )
    .unwrap();
    let active = Arc::new(AtomicUsize::new(0));
    let peak = Arc::new(AtomicUsize::new(0));
    let completed = Arc::new(AtomicUsize::new(0));
    let max_active = peak.clone();
    let done = completed.clone();
    let worker = thread::spawn(move || {
        thread::scope(|scope| {
            let deadline = Instant::now() + Duration::from_secs(5);
            for _ in 0..count {
                let mut stream = loop {
                    match listener.accept() {
                        Ok((stream, _)) => break stream,
                        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                            if Instant::now() >= deadline {
                                return;
                            }
                            thread::sleep(Duration::from_millis(2));
                        }
                        Err(e) => panic!("{e}"),
                    }
                };
                let active = active.clone();
                let max_active = max_active.clone();
                let done = done.clone();
                scope.spawn(move || {
                    let request = read_request(&mut stream);
                    let task = request["messages"][1]["content"].as_str().unwrap();
                    max_active
                        .fetch_max(active.fetch_add(1, Ordering::SeqCst) + 1, Ordering::SeqCst);
                    if task != "bad" {
                        thread::sleep(Duration::from_millis(100));
                    }
                    active.fetch_sub(1, Ordering::SeqCst);
                    done.fetch_add(1, Ordering::SeqCst);
                    let role = if task == "bad" { "user" } else { "assistant" };
                    reply(
                        &mut stream,
                        "200 OK",
                        &json!({"choices":[{"message":{"role":role,"content":task}}]}).to_string(),
                        "",
                    );
                });
            }
        });
    });
    (client, peak, completed, worker)
}

#[test]
fn parallel_workers_overlap_stay_bounded_and_preserve_input_order() {
    let (client, peak, completed, server) = parallel_server(7);
    let tasks: Vec<String> = (0..7).map(|i| format!("task_{i}")).collect();
    let result = agent::run_parallel(&client, &Host::default(), "s", &tasks, 1);
    server.join().unwrap();
    let results = result.unwrap();
    assert_eq!(
        results.iter().map(|r| r.text.clone()).collect::<Vec<_>>(),
        tasks
    );
    assert_eq!(completed.load(std::sync::atomic::Ordering::SeqCst), 7);
    assert!(
        (2..=3).contains(&peak.load(std::sync::atomic::Ordering::SeqCst)),
        "workers did not overlap or exceeded three"
    );
}

#[test]
fn parallel_error_waits_for_every_started_worker() {
    let (client, _, completed, server) = parallel_server(3);
    let tasks = vec!["bad".into(), "slow_a".into(), "slow_b".into()];
    let result = agent::run_parallel(&client, &Host::default(), "s", &tasks, 1);
    let completed_on_return = completed.load(std::sync::atomic::Ordering::SeqCst);
    server.join().unwrap();
    assert!(result.is_err());
    assert_eq!(
        completed_on_return, 3,
        "returned before joining slow workers"
    );
}

#[test]
fn agent_rejects_malformed_assistant_protocol_without_dispatch() {
    let messages = vec![
        json!({"role":"assistant","content":"ok","function_call":{"name":"inspect","arguments":"{}"}}),
        json!({"role":"assistant","content":"ok","tool_calls":{}}),
        json!({"role":"assistant","content":123}),
        json!({"role":"assistant","content":null,"tool_calls":[]}),
        json!({"role":"user","content":"ok"}),
    ];
    for message in messages {
        let server = Fixture::new(vec![message]);
        let host = Host::default();
        assert!(agent::run(&server.client(), &host, "s", "t", 1).is_err());
        assert!(host.calls.lock().unwrap().is_empty());
    }
    for (reason, message) in [
        (
            json!("tool_calls"),
            json!({"role":"assistant","content":"ok"}),
        ),
        (
            json!("stop"),
            json!({"role":"assistant","tool_calls":[tool("one","inspect","{}")]}),
        ),
        (Value::Null, json!({"role":"assistant","content":"ok"})),
    ] {
        let server = Fixture::raw(vec![(
            "200 OK".into(),
            json!({"choices":[{"message":message,"finish_reason":reason}]}).to_string(),
            String::new(),
        )]);
        assert!(server.client().complete(&[], &[]).is_err());
    }
}

struct SchemaHost(Vec<Value>);
impl agent::ToolHost for SchemaHost {
    fn schemas(&self) -> Vec<Value> {
        self.0.clone()
    }
    fn call(&self, _: &str, _: Value) -> Result<Value, String> {
        panic!("invalid schema must not dispatch")
    }
}
#[test]
fn agent_rejects_invalid_tool_schemas_before_connecting() {
    let server = Fixture::new(vec![]);
    let valid = json!({"type":"function","function":{"name":"inspect"}});
    for schemas in [
        vec![valid.clone(), valid],
        vec![json!({"type":"function","function":{"name":"bad name"}})],
        vec![json!({"type":"other","function":{"name":"inspect"}})],
        vec![json!({"type":"function","function":{}})],
    ] {
        let error = agent::run(&server.client(), &SchemaHost(schemas), "s", "t", 1).unwrap_err();
        assert!(error.contains("schema"), "{error}");
    }
}

#[test]
fn agent_preserves_multi_tool_transcript_and_final_answer() {
    let calls = json!({"role":"assistant","content":null,"tool_calls":[tool("call_1","inspect",r#"{"path":"a"}"#),tool("call_2","inspect",r#"{"path":"b"}"#)]});
    let final_message = json!({"role":"assistant","content":"finished"});
    let server = Fixture::new(vec![calls.clone(), final_message.clone()]);
    let host = Host::default();
    let result = agent::run(&server.client(), &host, "system", "task", 2).unwrap();
    assert_eq!(result.text, "finished");
    assert_eq!(result.tool_calls, 2);
    let expected = vec![
        json!({"role":"system","content":"system"}),
        json!({"role":"user","content":"task"}),
        calls,
        json!({"role":"tool","tool_call_id":"call_1","content":json!({"observed":{"path":"a"}}).to_string()}),
        json!({"role":"tool","tool_call_id":"call_2","content":json!({"observed":{"path":"b"}}).to_string()}),
    ];
    assert_eq!(
        server.requests.lock().unwrap()[1]["messages"],
        json!(expected)
    );
    assert_eq!(&result.messages[..5], expected.as_slice());
    assert_eq!(result.messages[5], final_message);
    assert_eq!(host.calls.lock().unwrap().len(), 2);
    assert_eq!(
        serde_json::to_value(result.clone()).unwrap()["tool_calls"],
        2
    );
}

use model::ChatClient;
use serde_json::{json, Value};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

fn read_request(stream: &mut TcpStream) -> Value {
    stream.set_nonblocking(false).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let mut bytes = Vec::new();
    let mut byte = [0];
    while !bytes.ends_with(b"\r\n\r\n") {
        stream.read_exact(&mut byte).unwrap();
        bytes.push(byte[0]);
    }
    let headers = String::from_utf8(bytes).unwrap();
    let len: usize = headers
        .lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case("content-length")
                .then(|| value.trim().parse().unwrap())
        })
        .unwrap();
    let mut body = vec![0; len];
    stream.read_exact(&mut body).unwrap();
    serde_json::from_slice(&body).unwrap()
}

fn reply(stream: &mut TcpStream, status: &str, body: &str, headers: &str) {
    write!(stream, "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n{headers}\r\n{body}", body.len()).unwrap();
}

struct Fixture {
    endpoint: String,
    requests: Arc<Mutex<Vec<Value>>>,
    worker: Option<thread::JoinHandle<()>>,
}
impl Fixture {
    fn new(messages: Vec<Value>) -> Self {
        let responses = messages
            .into_iter()
            .map(|message| {
                (
                    "200 OK".to_string(),
                    json!({"choices":[{"message":message}]}).to_string(),
                    String::new(),
                )
            })
            .collect();
        Self::raw(responses)
    }
    fn raw(responses: Vec<(String, String, String)>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let endpoint = format!(
            "http://{}/v1/chat/completions",
            listener.local_addr().unwrap()
        );
        let requests = Arc::new(Mutex::new(Vec::new()));
        let captured = requests.clone();
        let worker = thread::spawn(move || {
            for (status, body, headers) in responses {
                let deadline = Instant::now() + Duration::from_secs(5);
                let mut stream = loop {
                    match listener.accept() {
                        Ok((stream, _)) => break stream,
                        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                            if Instant::now() >= deadline {
                                return;
                            }
                            thread::sleep(Duration::from_millis(2));
                        }
                        Err(e) => panic!("fixture accept: {e}"),
                    }
                };
                captured.lock().unwrap().push(read_request(&mut stream));
                reply(&mut stream, &status, &body, &headers);
            }
        });
        Self {
            endpoint,
            requests,
            worker: Some(worker),
        }
    }
    fn client(&self) -> ChatClient {
        ChatClient::new(&self.endpoint, "synthetic", Some("fixture-secret".into())).unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.worker.take().unwrap().join().unwrap();
    }
}

#[test]
fn endpoint_validation_allows_only_https_or_loopback_http() {
    for endpoint in [
        "https://example.com/v1/chat/completions",
        "http://127.0.0.1:8000/v1/chat/completions",
        "http://[::1]:8000/v1/chat/completions",
        "http://localhost:8000/v1/chat/completions",
    ] {
        assert!(
            ChatClient::new(endpoint, "synthetic", None).is_ok(),
            "{endpoint}"
        );
    }
    for endpoint in [
        "http://example.com/v1/chat/completions",
        "http://127.0.0.1.evil.test/",
        "ftp://127.0.0.1/",
        "not a url",
        "https://user:secret@example.com/",
        "https://example.com/#fragment",
        "http://0.0.0.0/",
        "http://[::]/",
        "https://example.com/\n",
    ] {
        assert!(
            ChatClient::new(endpoint, "synthetic", None).is_err(),
            "{endpoint}"
        );
    }
    assert!(ChatClient::new("https://example.com/", " ", None).is_err());
    assert!(ChatClient::new("https://example.com/", "m", Some("secret\r\nx: y".into())).is_err());
}

#[test]
fn transport_rejects_redirects_without_contacting_target() {
    let target = TcpListener::bind("127.0.0.1:0").unwrap();
    target.set_nonblocking(true).unwrap();
    let source = Fixture::raw(vec![(
        "302 Found".into(),
        "{}".into(),
        format!("Location: http://{}/leak\r\n", target.local_addr().unwrap()),
    )]);
    // If the client follows, let it complete so the test cannot hang.
    let followed = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let seen = followed.clone();
    let watcher = thread::spawn(move || {
        let until = Instant::now() + Duration::from_millis(500);
        while Instant::now() < until {
            if let Ok((mut s, _)) = target.accept() {
                seen.store(true, std::sync::atomic::Ordering::SeqCst);
                let mut buf = [0; 4096];
                let _ = s.read(&mut buf);
                reply(
                    &mut s,
                    "200 OK",
                    &json!({"choices":[{"message":{"role":"assistant","content":"redirected"}}]})
                        .to_string(),
                    "",
                );
                break;
            }
            thread::sleep(Duration::from_millis(2));
        }
    });
    let result = source.client().complete(&[], &[]);
    watcher.join().unwrap();
    assert!(
        !followed.load(std::sync::atomic::Ordering::SeqCst),
        "redirect target was contacted"
    );
    assert!(result.is_err());
}

#[test]
fn transport_bounds_response_and_rejects_bad_envelopes_without_secrets() {
    let oversized =
        json!({"choices":[{"message":{"role":"assistant","content":"x".repeat(2 * 1024 * 1024)}}]})
            .to_string();
    let cases = vec![
        ("200 OK", oversized),
        ("200 OK", "fixture-secret is not JSON".into()),
        ("200 OK", json!({"choices":[{"message":{"role":"user","content":"wrong role"}}]}).to_string()),
        ("200 OK", json!({"choices":[{"message":{"role":"assistant","content":"   "}}]}).to_string()),
        ("200 OK", json!({"choices":[]}).to_string()),
        ("200 OK", json!({"choices":[{"message":{"role":"assistant","content":"a"}},{"message":{"role":"assistant","content":"b"}}]}).to_string()),
        ("200 OK", json!({"choices":[{"message":{"role":"assistant","content":"partial"},"finish_reason":"length"}]}).to_string()),
        ("500 Internal Server Error", "fixture-secret".into()),
    ];
    for (index, (status, body)) in cases.into_iter().enumerate() {
        let server = Fixture::raw(vec![(status.into(), body, String::new())]);
        let error = server
            .client()
            .complete(&[], &[])
            .expect_err(&format!("invalid case {index}"));
        assert!(!error.contains("fixture-secret"));
    }
}

#[test]
fn transport_bounds_outbound_context_before_connecting() {
    let server = Fixture::new(vec![]);
    let error = server
        .client()
        .complete(
            &[json!({"role":"user","content":"x".repeat(4 * 1024 * 1024)})],
            &[],
        )
        .unwrap_err();
    assert!(error.contains("limit"), "{error}");
    assert!(server.requests.lock().unwrap().is_empty());
}

// Slow regression: exercise the real transport across its former 60-second deadline.
#[test]
#[ignore = "takes 61 seconds; run explicitly when changing transport deadlines"]
fn transport_allows_local_generation_longer_than_one_minute() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let client = ChatClient::new(
        &format!(
            "http://{}/v1/chat/completions",
            listener.local_addr().unwrap()
        ),
        "synthetic",
        None,
    )
    .unwrap();
    let worker = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        read_request(&mut stream);
        thread::sleep(Duration::from_secs(61));
        let body = json!({"choices":[{"message":{"role":"assistant","content":"finished"},"finish_reason":"stop"}]}).to_string();
        // The red run disconnects before this response. Do not mask its client error.
        let _ = write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
    });
    let result = client.complete(&[json!({"role":"user","content":"task"})], &[]);
    worker.join().unwrap();
    assert_eq!(result.unwrap()["content"], "finished");
}

#[test]
fn transport_posts_messages_and_returns_assistant() {
    let expected = json!({"role":"assistant","content":"fixture answer"});
    let server = Fixture::new(vec![expected.clone()]);
    let messages = vec![json!({"role":"user","content":"hello"})];
    let tools = vec![json!({"type":"function","function":{"name":"inspect"}})];
    assert_eq!(
        server.client().complete(&messages, &tools).unwrap(),
        expected
    );
    let requests = server.requests.lock().unwrap();
    assert_eq!(requests[0]["model"], "synthetic");
    assert_eq!(requests[0]["messages"], json!(messages));
    assert_eq!(requests[0]["tools"], json!(tools));
    assert_eq!(requests[0]["stream"], false);
}
