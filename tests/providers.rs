//! Real loopback HTTP fixtures only; never external inference or credentials.
use serde_json::{json, Value};
use std::{
    io::{Read, Write},
    net::TcpListener,
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant},
};
use zeus::{
    agent::{self, ToolHost},
    model::{ChatClient, Protocol},
};

struct Fixture {
    endpoint: String,
    requests: Arc<Mutex<Vec<(String, Value)>>>,
    worker: Option<thread::JoinHandle<()>>,
}
impl Fixture {
    fn new(replies: Vec<(u16, String)>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let endpoint = format!("http://{}/inference", listener.local_addr().unwrap());
        let requests = Arc::new(Mutex::new(Vec::new()));
        let captured = requests.clone();
        let worker = thread::spawn(move || {
            for (status, body) in replies {
                let deadline = Instant::now() + Duration::from_secs(3);
                let mut stream = loop {
                    if let Ok((s, _)) = listener.accept() {
                        break s;
                    }
                    if Instant::now() >= deadline {
                        return;
                    }
                    thread::sleep(Duration::from_millis(2));
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
                let header = String::from_utf8(header).unwrap();
                let len: usize = header
                    .lines()
                    .find_map(|l| {
                        let (k, v) = l.split_once(':')?;
                        k.eq_ignore_ascii_case("content-length")
                            .then(|| v.trim().parse().unwrap())
                    })
                    .unwrap();
                let mut bytes = vec![0; len];
                stream.read_exact(&mut bytes).unwrap();
                captured
                    .lock()
                    .unwrap()
                    .push((header, serde_json::from_slice(&bytes).unwrap()));
                let content_type = if body.starts_with("event:")
                    || body.starts_with("data:")
                    || body.starts_with(':')
                {
                    "text/event-stream"
                } else {
                    "application/json"
                };
                let _ = write!(stream,"HTTP/1.1 {status} Fixture\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len());
            }
        });
        Self {
            endpoint,
            requests,
            worker: Some(worker),
        }
    }
    fn client(&self, protocol: Protocol) -> ChatClient {
        ChatClient::with_protocol(
            &self.endpoint,
            "vendor/model:any-version",
            Some("fixture-secret".into()),
            protocol,
        )
        .unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.worker.take().unwrap().join().unwrap();
    }
}
#[derive(Default)]
struct Host(Mutex<Vec<Value>>);
impl ToolHost for Host {
    fn schemas(&self) -> Vec<Value> {
        vec![
            json!({"type":"function","function":{"name":"inspect","description":"Inspect","parameters":{"type":"object","properties":{"path":{"type":"string"}}}}}),
        ]
    }
    fn call(&self, _: &str, args: Value) -> Result<Value, String> {
        self.0.lock().unwrap().push(args.clone());
        Ok(json!({"observed":args}))
    }
}
fn completed(output: Value) -> String {
    format!(
        "event: response.completed\ndata: {}\n\n",
        json!({"type":"response.completed","response":{"status":"completed","output":output}})
    )
}
fn anthropic(content: Value, stop: &str) -> String {
    json!({"type":"message","role":"assistant","content":content,"stop_reason":stop}).to_string()
}
#[test]
fn anthropic_full_tool_cycle_separates_auth_and_groups_results() {
    let output = json!([
        {"type":"tool_use","id":"c1","name":"inspect","input":{"path":"a"}},
        {"type":"text","text":"Checking"},
        {"type":"tool_use","id":"c2","name":"inspect","input":{"path":"b"}}
    ]);
    let server = Fixture::new(vec![
        (200, anthropic(output.clone(), "tool_use")),
        (
            200,
            anthropic(json!([{"type":"text","text":"finished"}]), "end_turn"),
        ),
    ]);
    let host = Host::default();
    let result = agent::run(
        &server.client(Protocol::Anthropic),
        &host,
        "system",
        "task",
        2,
    )
    .unwrap();
    assert_eq!(result.text, "finished");
    assert_eq!(result.tool_calls, 2);
    let requests = server.requests.lock().unwrap();
    for (headers, body) in requests.iter() {
        assert!(headers.to_lowercase().contains("x-api-key: fixture-secret"));
        assert!(headers
            .to_lowercase()
            .contains("anthropic-version: 2023-06-01"));
        assert!(!headers.to_lowercase().contains("authorization:"));
        assert_eq!(body["system"], json!([{"type":"text","text":"system"}]));
        assert_eq!(body["stream"], false);
        assert!(body["max_tokens"]
            .as_u64()
            .is_some_and(|n| n > 0 && n <= 8192));
        assert_eq!(
            body["tools"][0]["input_schema"],
            host.schemas()[0]["function"]["parameters"]
        );
    }
    let messages = requests[1].1["messages"].as_array().unwrap();
    assert_eq!(messages.len(), 3);
    assert_eq!(messages[1]["content"], output);
    assert_eq!(messages[2]["role"], "user");
    assert_eq!(
        messages[2]["content"],
        json!([
            {"type":"tool_result","tool_use_id":"c1","content":"{\"observed\":{\"path\":\"a\"}}"},
            {"type":"tool_result","tool_use_id":"c2","content":"{\"observed\":{\"path\":\"b\"}}"}
        ])
    );
    assert_eq!(
        serde_json::from_value::<Protocol>(json!("anthropic")).unwrap(),
        Protocol::Anthropic
    );
}

#[test]
fn responses_rejects_truncated_failed_and_malformed_streams_without_dispatch() {
    let partial = "event: response.output_item.done\ndata: {\"type\":\"response.output_item.done\",\"item\":{\"type\":\"function_call\",\"call_id\":\"c1\",\"name\":\"inspect\",\"arguments\":\"{}\"}}\n\n";
    let mut cases = vec![
        partial.to_string(),
        "data: fixture-secret\n\n".into(),
        "data: [DONE]\n\n".into(),
        completed(text_output()).trim_end().to_string(),
        completed(text_output()).replace("event: response.completed", "event: response.failed"),
        completed(text_output()).replace("\"status\":\"completed\"", "\"status\":\"incomplete\""),
        completed(json!([])),
        completed(json!([{"type":"web_search_call","status":"completed"}])),
        format!(":{}", "x".repeat(2 * 1024 * 1024)),
    ];
    for kind in ["response.failed", "response.incomplete", "error"] {
        cases.push(format!("{partial}event: {kind}\ndata: {{\"type\":\"{kind}\",\"error\":\"fixture-secret\"}}\n\n"));
    }
    for body in cases {
        let server = Fixture::new(vec![(200, body)]);
        let host = Host::default();
        let error =
            agent::run(&server.client(Protocol::Responses), &host, "s", "t", 2).unwrap_err();
        assert!(!error.contains("fixture-secret"));
        assert!(host.0.lock().unwrap().is_empty());
        assert_eq!(server.requests.lock().unwrap().len(), 1);
    }
}

#[test]
fn anthropic_rejects_incomplete_and_unsupported_capabilities() {
    let tool = json!({"type":"tool_use","id":"c1","name":"inspect","input":{}});
    let mut cases = Vec::new();
    for stop in [
        "max_tokens",
        "pause_turn",
        "stop_sequence",
        "model_context_window_exceeded",
        "end_turn",
    ] {
        cases.push(anthropic(json!([tool.clone()]), stop));
    }
    for bad in [
        json!({"type":"thinking","thinking":"fixture-secret","signature":"opaque"}),
        json!({"type":"server_tool_use","name":"bash"}),
        json!({"type":"tool_use","id":"bad","name":"inspect","input":[]}),
        json!({"type":"text","text":2}),
    ] {
        cases.push(anthropic(json!([tool.clone(), bad]), "tool_use"));
    }
    cases.push(anthropic(json!([]), "end_turn"));
    cases.push(anthropic(
        json!([{"type":"text","text":"x".repeat(2*1024*1024)}]),
        "end_turn",
    ));
    for body in cases {
        let server = Fixture::new(vec![(200, body)]);
        let host = Host::default();
        let error =
            agent::run(&server.client(Protocol::Anthropic), &host, "s", "t", 2).unwrap_err();
        assert!(!error.contains("fixture-secret"));
        assert!(host.0.lock().unwrap().is_empty());
    }
}

#[test]
fn provider_tools_still_require_whole_batch_validation() {
    for protocol in [Protocol::Responses, Protocol::Anthropic] {
        for (name, args, id) in [
            ("unknown", "{}", "bad"),
            ("inspect", "[]", "bad"),
            ("inspect", "{}", "c1"),
            ("inspect", "{}", "bad id"),
        ] {
            let body = if protocol == Protocol::Responses {
                completed(json!([
                    {"type":"function_call","call_id":"c1","name":"inspect","arguments":"{}"},
                    {"type":"function_call","call_id":id,"name":name,"arguments":args}
                ]))
            } else {
                anthropic(
                    json!([
                        {"type":"tool_use","id":"c1","name":"inspect","input":{}},
                        {"type":"tool_use","id":id,"name":name,"input":serde_json::from_str::<Value>(args).unwrap()}
                    ]),
                    "tool_use",
                )
            };
            let server = Fixture::new(vec![(200, body)]);
            let host = Host::default();
            assert!(agent::run(&server.client(protocol), &host, "s", "t", 2).is_err());
            assert!(host.0.lock().unwrap().is_empty());
        }
    }
}

#[test]
fn protocol_failures_never_retry_fallback_or_echo_provider_secrets() {
    for protocol in [
        Protocol::ChatCompletions,
        Protocol::Responses,
        Protocol::Anthropic,
    ] {
        for status in [400, 401, 403, 429, 500, 302] {
            let server = Fixture::new(vec![(
                status,
                "fixture-secret https://secret.invalid?token=fixture-secret".into(),
            )]);
            let error = server.client(protocol).complete(&[], &[]).unwrap_err();
            assert!(!error.contains("fixture-secret"));
            assert!(!error.contains("secret.invalid"));
            assert_eq!(server.requests.lock().unwrap().len(), 1);
        }
        for endpoint in [
            "http://example.com/",
            "https://u:fixture-secret@example.com/",
            "https://example.com/#x",
        ] {
            assert!(ChatClient::with_protocol(endpoint, "any", None, protocol).is_err());
        }
    }
}

#[test]
fn providers_bound_transcripts_and_reject_multimodal_inputs_before_connecting() {
    for protocol in [Protocol::Responses, Protocol::Anthropic] {
        let server = Fixture::new(vec![]);
        for input in [
            json!({"role":"user","content":"x".repeat(4*1024*1024)}),
            json!({"role":"user","content":[{"type":"image_url","image_url":{"url":"https://example.com"}}]}),
        ] {
            assert!(server.client(protocol).complete(&[input], &[]).is_err());
        }
        assert!(server
            .client(protocol)
            .complete(&[], &[json!({"type":"web_search"})])
            .is_err());
        assert!(server.requests.lock().unwrap().is_empty());
    }
}

#[test]
fn replay_metadata_cannot_hide_a_tool_call() {
    for protocol in [Protocol::Responses, Protocol::Anthropic] {
        let server = Fixture::new(vec![]);
        let mut message = json!({"role":"assistant","content":"fine","tool_calls":[]});
        if protocol == Protocol::Responses {
            message["_responses_output"] = json!([{"type":"function_call","call_id":"hidden","name":"inspect","arguments":"{}"}]);
        } else {
            message["_anthropic_content"] =
                json!([{"type":"tool_use","id":"hidden","name":"inspect","input":{}}]);
        }
        assert!(server.client(protocol).complete(&[message], &[]).is_err());
        assert!(server.requests.lock().unwrap().is_empty());
    }
}

#[test]
fn responses_reads_chunked_terminal_event_without_waiting_for_eof() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!("http://{}/v1/responses", listener.local_addr().unwrap());
    let (done, wait) = std::sync::mpsc::channel();
    let worker = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        let mut header = Vec::new();
        let mut byte = [0];
        while !header.ends_with(b"\r\n\r\n") {
            stream.read_exact(&mut byte).unwrap();
            header.push(byte[0]);
        }
        let header = String::from_utf8(header).unwrap();
        assert!(!header.to_lowercase().contains("authorization:"));
        assert!(!header.to_lowercase().contains("x-api-key:"));
        let len: usize = header
            .lines()
            .find_map(|line| {
                let (name, value) = line.split_once(':')?;
                name.eq_ignore_ascii_case("content-length")
                    .then(|| value.trim().parse().unwrap())
            })
            .unwrap();
        stream.read_exact(&mut vec![0; len]).unwrap();
        write!(stream,"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream; charset=utf-8\r\nTransfer-Encoding: chunked\r\n\r\n").unwrap();
        for chunk in completed(text_output()).as_bytes().chunks(7) {
            write!(stream, "{:x}\r\n", chunk.len()).unwrap();
            stream.write_all(chunk).unwrap();
            stream.write_all(b"\r\n").unwrap();
        }
        // No EOF or terminating HTTP chunk until the client has consumed completed.
        wait.recv_timeout(Duration::from_secs(3))
            .expect("client waited for EOF");
    });
    let result = ChatClient::with_protocol(&endpoint, "local/arbitrary", None, Protocol::Responses)
        .unwrap()
        .complete(&[], &[]);
    done.send(()).unwrap();
    worker.join().unwrap();
    assert_eq!(result.unwrap()["content"], "finished");
}

#[test]
fn responses_uses_only_completed_output_and_supports_sse_comments_crlf_multiline() {
    let pending="event: response.output_item.done\ndata: {\"type\":\"response.output_item.done\",\"item\":{\"type\":\"function_call\",\"name\":\"inspect\",\"arguments\":\"{}\"}}\n\n";
    let terminal = completed(text_output()).replace("data: {", "data: {\ndata: ");
    let body = format!(": heartbeat\n\n{pending}{terminal}").replace('\n', "\r\n");
    let server = Fixture::new(vec![(200, body)]);
    let host = Host::default();
    let result = agent::run(&server.client(Protocol::Responses), &host, "s", "t", 1).unwrap();
    assert_eq!(result.text, "finished");
    assert!(host.0.lock().unwrap().is_empty());
}

fn text_output() -> Value {
    json!([{"type":"message","id":"m1","status":"completed","role":"assistant","content":[{"type":"output_text","text":"finished","annotations":[]}]}])
}
#[test]
fn responses_tool_cycle_preserves_optional_message_phase() {
    for phase in [None, Some(Value::Null), Some(json!("commentary"))] {
        let mut update = json!({"type":"message","id":"update","status":"completed","role":"assistant","content":[{"type":"output_text","text":"Checking","annotations":[]}]});
        if let Some(phase) = phase {
            update["phase"] = phase;
        }
        let call = json!({"type":"function_call","call_id":"c1","name":"inspect","arguments":"{\"path\":\"a\"}"});
        let mut final_output = text_output();
        final_output[0]["phase"] = json!("final_answer");
        let server = Fixture::new(vec![
            (200, completed(json!([update.clone(), call.clone()]))),
            (200, completed(final_output.clone())),
            (200, completed(text_output())),
        ]);
        let client = server.client(Protocol::Responses);
        let host = Host::default();
        let result = agent::run(&client, &host, "system", "task", 2).unwrap();
        assert_eq!(result.text, "finished");
        assert_eq!(result.tool_calls, 1);
        assert_eq!(*host.0.lock().unwrap(), vec![json!({"path":"a"})]);
        // Replay the final answer too, as on a subsequent user turn.
        let mut messages = result.messages;
        messages.push(json!({"role":"user","content":"continue"}));
        client.complete(&messages, &host.schemas()).unwrap();
        let requests = server.requests.lock().unwrap();
        assert_eq!(requests.len(), 3);
        let input = requests[1].1["input"].as_array().unwrap();
        assert_eq!(input[2], update);
        assert_eq!(input[3], call);
        assert_eq!(
            input[4],
            json!({"type":"function_call_output","call_id":"c1","output":"{\"observed\":{\"path\":\"a\"}}"})
        );
        let continued = requests[2].1["input"].as_array().unwrap();
        assert_eq!(&continued[..5], input);
        assert_eq!(continued[5], final_output[0]);
        assert_eq!(continued[6], json!({"role":"user","content":"continue"}));
    }
}

#[test]
fn responses_rejects_invalid_message_phase_before_dispatch_or_replay() {
    for phase in [
        json!("fixture-secret"),
        json!(""),
        json!(false),
        json!(1),
        json!([]),
        json!({"phase":"commentary"}),
    ] {
        let mut output = text_output();
        output[0]["phase"] = phase;
        // Even an otherwise valid call preceding the malformed message must not run.
        output.as_array_mut().unwrap().insert(
            0,
            json!({"type":"function_call","call_id":"c1","name":"inspect","arguments":"{}"}),
        );
        let server = Fixture::new(vec![(200, completed(output.clone()))]);
        let host = Host::default();
        let error =
            agent::run(&server.client(Protocol::Responses), &host, "s", "t", 1).unwrap_err();
        assert_eq!(error, "invalid Responses message phase");
        assert!(host.0.lock().unwrap().is_empty());
        assert_eq!(server.requests.lock().unwrap().len(), 1);

        let replay_server = Fixture::new(vec![]);
        let message = json!({"role":"assistant","content":"finished","tool_calls":[{"type":"function","id":"c1","function":{"name":"inspect","arguments":"{}"}}],"_responses_output":output});
        let error = replay_server
            .client(Protocol::Responses)
            .complete(&[message], &host.schemas())
            .unwrap_err();
        assert_eq!(error, "invalid Responses message phase");
        assert!(replay_server.requests.lock().unwrap().is_empty());
    }
}

#[test]
fn responses_full_tool_cycle_preserves_reasoning_and_auth() {
    let output = json!([
        {"type":"reasoning","id":"r1","summary":[],"encrypted_content":"opaque"},
        {"type":"function_call","id":"f1","status":"completed","call_id":"c1","name":"inspect","arguments":"{\"path\":\"a\"}"},
        {"type":"function_call","id":"f2","status":"completed","call_id":"c2","name":"inspect","arguments":"{\"path\":\"b\"}"}
    ]);
    let server = Fixture::new(vec![
        (200, completed(output.clone())),
        (200, completed(text_output())),
    ]);
    let host = Host::default();
    let result = agent::run(
        &server.client(Protocol::Responses),
        &host,
        "system",
        "task",
        2,
    )
    .unwrap();
    assert_eq!(result.text, "finished");
    assert_eq!(result.tool_calls, 2);
    let requests = server.requests.lock().unwrap();
    for (headers, body) in requests.iter() {
        assert!(headers
            .to_lowercase()
            .contains("authorization: bearer fixture-secret"));
        assert!(!headers.to_lowercase().contains("x-api-key"));
        assert_eq!(body["stream"], true);
        assert_eq!(body["store"], false);
        assert_eq!(body["model"], "vendor/model:any-version");
        assert_eq!(body["tools"][0]["name"], "inspect");
        assert_eq!(body["tools"][0]["strict"], false);
        assert_eq!(
            body["tools"][0]["parameters"],
            host.schemas()[0]["function"]["parameters"]
        );
    }
    let input = requests[1].1["input"].as_array().unwrap();
    assert_eq!(&input[2..5], output.as_array().unwrap());
    assert_eq!(
        input[5],
        json!({"type":"function_call_output","call_id":"c1","output":"{\"observed\":{\"path\":\"a\"}}"})
    );
    assert_eq!(input[6]["call_id"], "c2");
    assert_eq!(
        serde_json::to_value(Protocol::default()).unwrap(),
        "chat_completions"
    );
    assert_eq!(
        serde_json::from_value::<Protocol>(json!("responses")).unwrap(),
        Protocol::Responses
    );
}
