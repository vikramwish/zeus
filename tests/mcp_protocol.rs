use mcp::{McpClient, McpConfig};
use serde_json::json;
use zeus::mcp;

// Labelled deterministic Python stdlib MCP fixture, not a live MCP backend.
const FIXTURE: &str = r#"
import json, sys, os, time, subprocess
mode = sys.argv[1]
if len(sys.argv) > 2:
    descendant = subprocess.Popen(['/usr/bin/python3','-c','import time; time.sleep(90)'], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    with open(sys.argv[2], 'w') as f: json.dump([os.getpid(), descendant.pid], f)
if mode == 'oversize':
    print(' ' * (2 * 1024 * 1024) + '{}', flush=True)
if mode == 'malformed': print('not json secret', flush=True)
if mode == 'stall': time.sleep(31)
def send(v):
    print(json.dumps(v), flush=True)
initialized = False
for line in sys.stdin:
    r = json.loads(line)
    m = r['method']
    if m == 'notifications/initialized':
        initialized = True
        continue
    if m == 'initialize':
        assert r['params']['protocolVersion'] == '2024-11-05'
        result = {'protocolVersion':'2024-11-05','capabilities':{'tools':{}},'serverInfo':{'name':'fixture','version':'1'}}
    elif m == 'tools/list':
        assert initialized
        result = {'tools':[{'name':n,'description':n,'inputSchema':{'type':'object'}} for n in ['echo','forbidden']]}
    elif m == 'tools/call':
        if mode == 'slowcall': time.sleep(2)
        result = {'content':[{'type':'text','text':json.dumps(r['params']['arguments'])}]}
        if mode == 'env': result['env'] = dict(os.environ)
        if mode == 'stderr': sys.stderr.write('secret' * 1000000); sys.stderr.flush()
        if mode == 'badcall': print('broken secret', flush=True); continue
    if m == 'initialize' and mode == 'version': result['protocolVersion'] = 'unsupported'
    if m == 'tools/list':
        if mode == 'missing': result = {'tools':[]}
        if mode == 'schema': result['tools'][0]['inputSchema'] = []
        if mode == 'bigpages':
            result['padding'] = 'x' * 1100000
            if not r['params'].get('cursor'):
                result['tools'] = []
                result['nextCursor'] = 'next'
        if mode == 'slowpages': time.sleep(16)
        if mode in ['pages', 'loop', 'slowpages']:
            if not r['params'].get('cursor'):
                result = {'tools':[], 'nextCursor':'next'}
            elif mode == 'loop': result = {'tools':[], 'nextCursor':'next'}
    if m == 'tools/call':
        if mode == 'badresult': result = {'isError':'yes'}
        if mode == 'toolerror': result['isError'] = True
        if mode == 'rpcerror':
            send({'jsonrpc':'2.0','id':r['id'],'error':{'code':-32000,'message':'fixture secret error'}})
            continue
    if mode == 'notifications': send({'jsonrpc':'2.0','method':'notifications/message','params':{}})
    if mode == 'flood':
        for _ in range(3): send({'jsonrpc':'2.0','method':'notifications/message','params':{'data':'x'*900000}})
    send({'jsonrpc':'2.0','id':r['id'] + (1 if mode == 'wrongid' else 0),'result':result})
    if mode == 'blockedwrite' and m == 'tools/list': time.sleep(31)
"#;
fn config(mode: &str) -> McpConfig {
    McpConfig {
        name: "fixture".into(),
        command: vec![
            "/usr/bin/python3".into(),
            "-u".into(),
            "-c".into(),
            FIXTURE.into(),
            mode.into(),
        ],
        allowed_tools: vec!["echo".into()],
        timeout_seconds: 30,
    }
}
#[test]
fn per_server_timeout_bounds_slow_tool_calls() {
    // A one-second budget fails a two-second call quickly and closes the
    // connection; a generous budget for the same server succeeds.
    let mut strict = config("slowcall");
    strict.timeout_seconds = 1;
    let mut c = McpClient::connect(&strict).unwrap();
    let start = std::time::Instant::now();
    let error = c.call("echo", json!({})).unwrap_err();
    assert!(error.contains("timed out"), "{error}");
    assert!(start.elapsed() < std::time::Duration::from_millis(1800));
    assert!(c.call("echo", json!({})).is_err());
    let mut lenient = config("slowcall");
    lenient.timeout_seconds = 5;
    let mut c = McpClient::connect(&lenient).unwrap();
    assert!(c.call("echo", json!({})).is_ok());
    for bad in [0, 601] {
        let mut cfg = config("normal");
        cfg.timeout_seconds = bad;
        assert!(McpClient::connect(&cfg).is_err(), "accepted {bad}");
    }
}
#[test]
fn discovery_validates_protocol_and_pages() {
    for mode in ["version", "missing", "schema", "loop", "wrongid"] {
        assert!(
            McpClient::connect(&config(mode)).is_err(),
            "accepted {mode}"
        );
    }
    for mode in ["pages", "notifications"] {
        assert_eq!(
            McpClient::connect(&config(mode)).unwrap().schemas().len(),
            1
        );
    }
}
#[test]
fn tool_errors_are_not_successes() {
    for mode in ["toolerror", "rpcerror"] {
        let mut c = McpClient::connect(&config(mode)).unwrap();
        assert!(c.call("echo", json!({})).is_err(), "accepted {mode}");
    }
}
#[test]
fn transport_limits_and_environment() {
    for mode in ["oversize", "malformed"] {
        assert!(
            McpClient::connect(&config(mode)).is_err(),
            "accepted {mode}"
        );
    }
    let mut c = McpClient::connect(&config("env")).unwrap();
    let result = c.call("echo", json!({})).unwrap();
    for key in result["env"].as_object().unwrap().keys() {
        // Apple's /usr/bin/python3 launcher adds SDK/search-path metadata even
        // under env -i; these keys are not inherited by Command.
        assert!(
            [
                "PATH",
                "HOME",
                "TMPDIR",
                "LC_CTYPE",
                "CPATH",
                "LIBRARY_PATH",
                "MANPATH",
                "SDKROOT",
                "__CF_USER_TEXT_ENCODING"
            ]
            .contains(&key.as_str()),
            "inherited {key}"
        );
    }
    assert_eq!(
        result["env"]["PATH"].as_str(),
        std::env::var("PATH").ok().as_deref()
    );
    assert!(c
        .call("echo", json!({"large":"x".repeat(2*1024*1024)}))
        .is_err());
    assert!(McpClient::connect(&config("stderr"))
        .unwrap()
        .call("echo", json!({}))
        .is_ok());
}
#[test]
fn stalled_server_times_out() {
    let start = std::time::Instant::now();
    assert!(McpClient::connect(&config("stall")).is_err());
    assert!(start.elapsed().as_secs() >= 29 && start.elapsed().as_secs() < 33);
}
fn assert_gone(pid: i32) {
    for _ in 0..100 {
        if unsafe { libc::kill(pid, 0) } != 0 {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    // Avoid leaving a fixture process behind when demonstrating RED.
    unsafe {
        libc::kill(pid, libc::SIGKILL);
    }
    panic!("fixture process {pid} survived cleanup");
}
#[test]
fn cleanup_covers_drop_connect_failure_and_call_failure() {
    for mode in ["normal", "version", "badcall"] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("pids.json");
        let mut cfg = config(mode);
        cfg.command.push(path.to_str().unwrap().into());
        let connected = McpClient::connect(&cfg);
        if mode == "version" {
            assert!(connected.is_err());
        } else {
            let mut c = connected.unwrap();
            if mode == "badcall" {
                assert!(c.call("echo", json!({})).is_err());
                let pids: Vec<i32> =
                    serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
                for pid in pids {
                    assert_gone(pid);
                }
            }
            drop(c);
        }
        let pids: Vec<i32> = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        for pid in pids {
            assert_gone(pid);
        }
    }
}
#[test]
fn rejects_invalid_arguments_and_tool_results() {
    let mut c = McpClient::connect(&config("normal")).unwrap();
    assert!(c.call("echo", json!([])).is_err());
    let mut c = McpClient::connect(&config("badresult")).unwrap();
    assert!(c.call("echo", json!({})).is_err());
}
#[test]
fn aggregate_discovery_is_bounded() {
    assert!(McpClient::connect(&config("bigpages")).is_err());
}
#[test]
fn blocked_stdin_write_also_times_out() {
    let mut c = McpClient::connect(&config("blockedwrite")).unwrap();
    let start = std::time::Instant::now();
    assert!(c
        .call("echo", json!({"data":"x".repeat(1000000)}))
        .unwrap_err()
        .contains("timed out"));
    assert!(start.elapsed().as_secs() >= 29 && start.elapsed().as_secs() < 33);
    assert!(c.call("echo", json!({})).is_err());
}
#[test]
fn notification_flood_and_oversize_lines_have_explicit_limits() {
    for mode in ["flood", "oversize"] {
        let error = McpClient::connect(&config(mode)).err().unwrap();
        assert!(error.contains("limit"), "{mode}: {error}");
    }
    let error = McpClient::connect(&config("malformed")).err().unwrap();
    assert!(!error.contains("secret"));
}
#[test]
fn discovery_shares_one_deadline_across_pages() {
    let start = std::time::Instant::now();
    assert!(McpClient::connect(&config("slowpages")).is_err());
    assert!(start.elapsed().as_secs() >= 29 && start.elapsed().as_secs() < 33);
}
#[test]
fn handshake_discovery_and_real_stdio_call() {
    let mut c = McpClient::connect(&config("normal")).unwrap();
    assert_eq!(
        c.schemas(),
        vec![
            json!({"type":"function","function":{"name":"echo","description":"echo","parameters":{"type":"object"}}})
        ]
    );
    let result = c.call("echo", json!({"message":"hello"})).unwrap();
    assert_eq!(result["content"][0]["text"], "{\"message\": \"hello\"}");
    assert!(c.call("forbidden", json!({})).is_err());
}
