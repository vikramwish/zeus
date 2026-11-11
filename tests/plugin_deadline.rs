//! A dedicated test process isolates the process-global cancellation deadline.
mod common;
use serde_json::json;
use std::time::{Duration, Instant};
use zeus::mcp::{McpClient, McpConfig};

#[test]
fn plugin_transport_observes_job_deadline_while_stdin_is_blocked() {
    let script = r#"
import json, sys, time
for line in sys.stdin:
    r = json.loads(line)
    if r['method'] == 'notifications/initialized': continue
    if r['method'] == 'initialize':
        result = {'protocolVersion':'2024-11-05','capabilities':{'tools':{}}}
    else:
        result = {'tools':[{'name':'echo','inputSchema':{'type':'object'}}]}
    print(json.dumps({'jsonrpc':'2.0','id':r['id'],'result':result}), flush=True)
    if r['method'] == 'tools/list': time.sleep(30)
"#;
    let mut client = McpClient::connect(&McpConfig {
        name: "blocked".into(),
        command: vec![common::python3(), "-I".into(), "-c".into(), script.into()],
        allowed_tools: vec!["echo".into()],
        timeout_seconds: 5,
    })
    .unwrap();
    zeus::cancel::set_deadline(1);
    let start = Instant::now();
    let error = client
        .call("echo", json!({"data":"x".repeat(1_000_000)}))
        .unwrap_err();
    let elapsed = start.elapsed();
    zeus::cancel::reset();
    assert!(error.contains("job deadline exceeded"), "{error}");
    assert!(
        elapsed < Duration::from_secs(4),
        "blocked send: {elapsed:?}"
    );
    assert!(client.call("echo", json!({})).is_err());
}
