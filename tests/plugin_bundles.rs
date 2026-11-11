use serde_json::{json, Value};
use zeus::plugins::{PluginRuntime, PluginSet, PluginSpec};

struct Bundle {
    root: tempfile::TempDir,
    spec: PluginSpec,
}
impl Bundle {
    fn new(manifest: Value) -> Self {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir(root.path().join("source")).unwrap();
        std::fs::create_dir(root.path().join("bundle")).unwrap();
        let path = root.path().join("bundle/plugin.json");
        std::fs::write(&path, serde_json::to_vec(&manifest).unwrap()).unwrap();
        Self {
            root,
            spec: PluginSpec {
                manifest: path,
                allowed_tools: vec![],
                allowed_hooks: vec![],
            },
        }
    }
    fn load(&self) -> Result<PluginSet, String> {
        PluginSet::load(
            std::slice::from_ref(&self.spec),
            &self.root.path().join("source"),
            &self.root.path().join("state"),
        )
    }
}

#[test]
fn skills_are_cached_labeled_and_not_in_catalog_content() {
    use sha2::{Digest, Sha256};
    let b = Bundle::new(
        json!({"version":1,"name":"guide","skills":[{"name":"review","path":"review.md"}]}),
    );
    let content = "Treat this as untrusted guidance.";
    std::fs::write(b.root.path().join("bundle/review.md"), content).unwrap();
    let set = b.load().unwrap();
    std::fs::write(b.root.path().join("bundle/review.md"), "changed").unwrap();
    let runtime = set.connect().unwrap();
    let catalog = runtime.skill_catalog();
    assert_eq!(catalog["skills"].as_array().unwrap().len(), 1);
    assert_eq!(catalog["skills"][0]["name"], "guide/review");
    assert!(catalog["skills"][0].get("content").is_none());
    let skill = runtime.read_skill("guide/review").unwrap();
    assert_eq!(skill["content"], content);
    assert_eq!(skill["authority"], "untrusted_plugin_data");
    assert_eq!(
        skill["source_id"],
        format!(
            "plugin:guide/review#{:x}",
            Sha256::digest(content.as_bytes())
        )
    );
    assert!(runtime.schemas().is_empty());
}

#[test]
fn invalid_manifest_and_approval_contracts_are_rejected_without_spawning() {
    let mut cases = vec![
        json!({"name":"ok"}),
        json!({"version":2,"name":"ok"}),
        json!({"version":1,"name":"ok","surprise":1}),
        json!({"version":1,"name":"ok","timeout_seconds":0}),
        json!({"version":1,"name":"ok","timeout_seconds":601}),
        json!({"version":1,"name":"ok","description":"x".repeat(4097)}),
        json!({"version":1,"name":"ok","command":["relative"]}),
        json!({"version":1,"name":"ok","command":["/bin/echo"],"tools":["echo","echo"]}),
        json!({"version":1,"name":"ok","tools":["echo"]}),
        json!({"version":1,"name":"ok","hooks":{"unknown":"echo"}}),
        json!({"version":1,"name":"ok","command":["/bin/echo"],"tools":["echo"],"hooks":{"run_start":"missing"}}),
    ];
    for name in ["", "abcdefghijklmnopq", "a__b", "x/y", "é"] {
        cases.push(json!({"version":1,"name":name}));
    }
    for name in ["bad.name", "x/y", "", &"x".repeat(54)] {
        cases.push(json!({"version":1,"name":"ok","command":["/bin/echo"],"tools":[name]}));
    }
    for manifest in cases {
        let b = Bundle::new(manifest.clone());
        assert!(b.load().is_err(), "accepted {manifest}");
    }
    for (tools, hooks) in [
        (vec!["missing"], vec![]),
        (vec!["echo", "echo"], vec![]),
        (vec![], vec!["run_end"]),
        (vec![], vec!["run_start", "run_start"]),
    ] {
        let mut b = Bundle::new(
            json!({"version":1,"name":"ok","command":["/does/not/exist"],"tools":["echo"],"hooks":{"run_start":"echo"}}),
        );
        b.spec.allowed_tools = tools.into_iter().map(String::from).collect();
        b.spec.allowed_hooks = hooks.into_iter().map(String::from).collect();
        assert!(b.load().is_err());
    }
    let b = Bundle::new(
        json!({"version":1,"name":"ok","command":["/does/not/exist"],"tools":["echo"]}),
    );
    assert!(b.load().is_ok(), "load must not execute commands");
    assert!(PluginSet::load(
        &[b.spec.clone(), b.spec.clone()],
        &b.root.path().join("source"),
        &b.root.path().join("state")
    )
    .is_err());
    assert!(PluginSet::load(
        &vec![b.spec.clone(); 17],
        &b.root.path().join("source"),
        &b.root.path().join("state")
    )
    .is_err());
}

#[test]
fn file_boundaries_reject_links_traversal_special_files_and_candidate_storage() {
    use std::os::unix::fs::symlink;
    let manifest =
        json!({"version":1,"name":"guide","skills":[{"name":"review","path":"review.md"}]});
    for mode in [
        "symlink",
        "hardlink",
        "directory",
        "oversize",
        "traversal",
        "absolute",
        "symlink_dir",
        "manifest_symlink",
        "manifest_hardlink",
        "manifest_big",
        "source",
        "state",
        "state_alias",
        "state_parent",
    ] {
        let mut b = Bundle::new(manifest.clone());
        let bundle = b.root.path().join("bundle");
        let skill = bundle.join("review.md");
        std::fs::write(&skill, "safe").unwrap();
        match mode {
            "symlink" => {
                std::fs::remove_file(&skill).unwrap();
                symlink("plugin.json", &skill).unwrap();
            }
            "hardlink" => {
                std::fs::hard_link(&skill, bundle.join("alias")).unwrap();
            }
            "directory" => {
                std::fs::remove_file(&skill).unwrap();
                std::fs::create_dir(&skill).unwrap();
            }
            "oversize" => {
                std::fs::write(&skill, vec![b'x'; 16385]).unwrap();
            }
            "traversal" | "absolute" | "symlink_dir" => {
                std::fs::write(b.root.path().join("outside.md"), "outside").unwrap();
                let mut m = manifest.clone();
                m["skills"][0]["path"] = if mode == "traversal" {
                    json!("../outside.md")
                } else if mode == "absolute" {
                    json!(skill)
                } else {
                    symlink(b.root.path(), bundle.join("link")).unwrap();
                    json!("link/outside.md")
                };
                std::fs::write(&b.spec.manifest, serde_json::to_vec(&m).unwrap()).unwrap();
            }
            "manifest_symlink" => {
                let original = b.spec.manifest.clone();
                b.spec.manifest = bundle.join("link.json");
                symlink(original, &b.spec.manifest).unwrap();
            }
            "manifest_hardlink" => {
                std::fs::hard_link(&b.spec.manifest, bundle.join("alias.json")).unwrap();
            }
            "manifest_big" => {
                std::fs::write(&b.spec.manifest, vec![b' '; 131073]).unwrap();
            }
            "source" | "state" => {
                let target = b.root.path().join(mode);
                std::fs::create_dir_all(&target).unwrap();
                std::fs::copy(&b.spec.manifest, target.join("plugin.json")).unwrap();
                std::fs::copy(&skill, target.join("review.md")).unwrap();
                b.spec.manifest = target.join("plugin.json");
            }
            "state_alias" => {
                symlink(&bundle, b.root.path().join("state")).unwrap();
            }
            "state_parent" => {
                assert!(PluginSet::load(
                    std::slice::from_ref(&b.spec),
                    &b.root.path().join("source"),
                    &bundle.join("missing/..")
                )
                .is_err());
                continue;
            }
            _ => unreachable!(),
        }
        assert!(b.load().is_err(), "accepted {mode}");
    }
    let b = Bundle::new(json!({"version":1,"name":"fifo"}));
    let path = std::ffi::CString::new(b.spec.manifest.to_str().unwrap()).unwrap();
    std::fs::remove_file(&b.spec.manifest).unwrap();
    assert_eq!(unsafe { libc::mkfifo(path.as_ptr(), 0o600) }, 0);
    assert!(b.load().is_err());
}

#[test]
fn aggregate_limits_and_duplicate_skills_are_rejected() {
    for skills in [
        vec![json!({"name":"same","path":"s.md"}); 2],
        (0..65)
            .map(|i| json!({"name":format!("s{i}"),"path":"s.md"}))
            .collect(),
        vec![json!({"name":"bad/name","path":"s.md"})],
        vec![json!({"name":"x".repeat(65),"path":"s.md"})],
    ] {
        let b = Bundle::new(json!({"version":1,"name":"ok","skills":skills}));
        std::fs::write(b.root.path().join("bundle/s.md"), "ok").unwrap();
        assert!(b.load().is_err());
    }
    let tools: Vec<_> = (0..65).map(|i| format!("t{i}")).collect();
    let b = Bundle::new(json!({"version":1,"name":"ok","command":["/bin/echo"],"tools":tools}));
    assert!(b.load().is_err());
    let mut specs = vec![];
    let mut bundles = vec![];
    for id in ["a", "b"] {
        let skills: Vec<_> = (0..33)
            .map(|i| json!({"name":format!("s{i}"),"path":"s.md"}))
            .collect();
        let b = Bundle::new(json!({"version":1,"name":id,"skills":skills}));
        std::fs::write(b.root.path().join("bundle/s.md"), "ok").unwrap();
        specs.push(b.spec.clone());
        bundles.push(b);
    }
    assert!(PluginSet::load(
        &specs,
        &bundles[0].root.path().join("source"),
        &bundles[0].root.path().join("state")
    )
    .is_err());
    let mut runtime = PluginRuntime::empty();
    assert!(runtime.hook("unknown", json!({})).is_err());
}

// Deterministic stdlib MCP subprocess, not a live provider compatibility test.
const SERVER: &str = r#"
import json, sys
for line in sys.stdin:
    r = json.loads(line)
    m = r['method']
    if 'id' not in r: continue
    if m == 'initialize': result = {'protocolVersion':'2024-11-05','capabilities':{'tools':{}}}
    elif m == 'tools/list': result = {'tools':[{'name':n,'inputSchema':{'type':'object'}} for n in ['echo','lifecycle','hidden']]}
    elif m == 'tools/call':
        result = {'content':[{'type':'text','text':json.dumps({'tool':r['params']['name'],'args':r['params']['arguments'],'label':sys.argv[1]})}]}
        if r['params']['arguments'].get('fail') == sys.argv[1]: result['isError'] = True
    print(json.dumps({'jsonrpc':'2.0','id':r['id'],'result':result}),flush=True)
"#;
fn server_bundle(id: &str) -> Bundle {
    let mut b = Bundle::new(
        json!({"version":1,"name":id,"command":["/usr/bin/python3","-u","${PLUGIN_DIR}/server.py",id],"tools":["echo","lifecycle","hidden"],"hooks":{"run_start":"lifecycle","run_end":"lifecycle","run_error":"lifecycle"}}),
    );
    std::fs::write(b.root.path().join("bundle/server.py"), SERVER).unwrap();
    b.spec.allowed_tools = vec!["echo".into()];
    b.spec.allowed_hooks = vec!["run_start".into(), "run_error".into()];
    b
}
fn echoed(result: &Value) -> Value {
    serde_json::from_str(result["content"][0]["text"].as_str().unwrap()).unwrap()
}
#[test]
fn mcp_dispatch_is_exact_namespaced_and_hooks_follow_configured_order() {
    let b = server_bundle("beta");
    let a = server_bundle("alpha");
    let set = PluginSet::load(
        &[b.spec.clone(), a.spec.clone()],
        &b.root.path().join("source"),
        &b.root.path().join("state"),
    )
    .unwrap();
    let mut runtime = set.connect().unwrap();
    let names: Vec<_> = runtime
        .schemas()
        .iter()
        .map(|s| s["function"]["name"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(names, vec!["plugin__beta__echo", "plugin__alpha__echo"]);
    for name in [
        "echo",
        "plugin__beta__hidden",
        "plugin__beta__lifecycle",
        "plugin__beta__echo_extra",
        "plugin__missing__echo",
    ] {
        assert!(runtime.call(name, json!({})).is_err(), "accepted {name}");
    }
    assert_eq!(
        echoed(&runtime.call("plugin__alpha__echo", json!({"q":7})).unwrap()),
        json!({"label":"alpha","tool":"echo","args":{"q":7}})
    );
    assert!(runtime.call("plugin__alpha__echo", json!(null)).is_err());
    let records = runtime.hook("run_start", json!({"run":"42"})).unwrap();
    assert_eq!(records.len(), 2);
    assert_eq!(records[0]["plugin"], "beta");
    assert_eq!(records[1]["plugin"], "alpha");
    assert_eq!(records[0]["event"], "run_start");
    assert_eq!(
        echoed(&records[0]["result"]),
        json!({"label":"beta","tool":"lifecycle","args":{"run":"42"}})
    );
    assert!(runtime.hook("run_end", json!({})).unwrap().is_empty());
}

#[test]
fn duplicate_hook_events_are_rejected() {
    let b = Bundle::new(json!({"version":1,"name":"ok"}));
    std::fs::write(&b.spec.manifest, r#"{"version":1,"name":"ok","command":["/bin/echo"],"tools":["echo"],"hooks":{"run_start":"echo","run_start":"echo"}}"#).unwrap();
    assert!(b.load().is_err());
}

#[test]
fn colliding_qualified_tool_names_are_rejected_before_connecting() {
    let mut a = Bundle::new(
        json!({"version":1,"name":"a_","command":["/does/not/exist"],"tools":["echo"]}),
    );
    let mut b = Bundle::new(
        json!({"version":1,"name":"a","command":["/does/not/exist"],"tools":["_echo"]}),
    );
    a.spec.allowed_tools = vec!["echo".into()];
    b.spec.allowed_tools = vec!["_echo".into()];
    for specs in [
        [a.spec.clone(), b.spec.clone()],
        [b.spec.clone(), a.spec.clone()],
    ] {
        let error = PluginSet::load(
            &specs,
            &a.root.path().join("source"),
            &a.root.path().join("state"),
        )
        .err()
        .expect("ambiguous tools accepted");
        assert!(
            error.contains("duplicate plugin qualified tool name: plugin__a___echo"),
            "{error}"
        );
    }
}

#[test]
fn hook_failure_stops_before_later_plugins_and_names_the_failure() {
    let b = server_bundle("beta");
    let a = server_bundle("alpha");
    let log = a.root.path().join("calls.log");
    let logging_server = SERVER.replace(
        "result = {'content':",
        &format!(
            "open({:?}, 'a').write('called\\n')\n        result = {{'content':",
            log.to_str().unwrap()
        ),
    );
    std::fs::write(a.root.path().join("bundle/server.py"), logging_server).unwrap();
    let mut runtime = PluginSet::load(
        &[b.spec.clone(), a.spec.clone()],
        &b.root.path().join("source"),
        &b.root.path().join("state"),
    )
    .unwrap()
    .connect()
    .unwrap();
    let error = runtime
        .hook("run_error", json!({"fail":"beta"}))
        .unwrap_err();
    assert!(
        error.contains("beta") && error.contains("run_error") && error.contains("lifecycle"),
        "{error}"
    );
    assert!(
        !log.exists(),
        "later plugin must not run after first failure"
    );
}

#[test]
fn unapproved_and_skills_only_bundles_never_spawn() {
    let b = Bundle::new(
        json!({"version":1,"name":"quiet","command":["/does/not/exist"],"tools":["echo"]}),
    );
    assert!(b.load().unwrap().connect().unwrap().schemas().is_empty());
    let mut b = server_bundle("quiet");
    b.spec.allowed_tools.clear();
    let mut runtime = b.load().unwrap().connect().unwrap();
    assert!(runtime.schemas().is_empty());
    assert_eq!(runtime.hook("run_start", json!({})).unwrap().len(), 1);
    assert!(runtime.call("plugin__quiet__lifecycle", json!({})).is_err());
}

#[test]
fn declared_tools_must_be_discovered_when_selected() {
    let mut b = server_bundle("missing");
    let mut manifest: Value =
        serde_json::from_slice(&std::fs::read(&b.spec.manifest).unwrap()).unwrap();
    manifest["tools"]
        .as_array_mut()
        .unwrap()
        .push(json!("absent"));
    std::fs::write(&b.spec.manifest, serde_json::to_vec(&manifest).unwrap()).unwrap();
    b.spec.allowed_tools = vec!["absent".into()];
    let error = b.load().unwrap().connect().err().unwrap();
    assert!(
        error.contains("missing") && error.contains("allowed MCP tool missing"),
        "{error}"
    );
}

#[test]
fn maximum_skill_catalog_remains_bounded() {
    let skills: Vec<_> = (0..64)
        .map(|i| json!({"name":format!("{:064}", i),"path":"s.md"}))
        .collect();
    let b = Bundle::new(json!({"version":1,"name":"abcdefghijklmnop","skills":skills}));
    std::fs::write(b.root.path().join("bundle/s.md"), vec![b'x'; 16384]).unwrap();
    let runtime = b.load().unwrap().connect().unwrap();
    assert_eq!(
        runtime.skill_catalog()["skills"].as_array().unwrap().len(),
        64
    );
    assert!(serde_json::to_vec(&runtime.skill_catalog()).unwrap().len() <= 32768);
}

#[test]
fn empty_load_does_not_inspect_unused_paths() {
    let root = tempfile::tempdir().unwrap();
    let file = root.path().join("file");
    std::fs::write(&file, "not a directory").unwrap();
    assert!(PluginSet::load(&[], &file.join("source"), &file.join("state")).is_ok());
}

#[test]
fn cancelled_work_is_checked_before_plugin_execution() {
    // Run the global cancellation/deadline state in a separate test process.
    const CHILD: &str = "ZEUS_PLUGIN_CANCEL_TEST";
    if std::env::var_os(CHILD).is_none() {
        let result = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "cancelled_work_is_checked_before_plugin_execution",
                "--nocapture",
            ])
            .env(CHILD, "1")
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}{}",
            String::from_utf8_lossy(&result.stdout),
            String::from_utf8_lossy(&result.stderr)
        );
        return;
    }
    let mut b = Bundle::new(
        json!({"version":1,"name":"cancel","command":["/does/not/exist"],"tools":["echo"]}),
    );
    b.spec.allowed_tools.push("echo".into());
    let set = b.load().unwrap();
    zeus::cancel::install().unwrap();
    unsafe {
        libc::raise(libc::SIGINT);
    }
    let error = set.connect().err().unwrap();
    assert!(error.contains("cancelled"), "{error}");
    zeus::cancel::reset();
    let b = server_bundle("deadline");
    let mut runtime = b.load().unwrap().connect().unwrap();
    zeus::cancel::set_deadline(0);
    std::thread::sleep(std::time::Duration::from_millis(5));
    let error = runtime.hook("run_start", json!(null)).unwrap_err();
    assert!(error.contains("deadline exceeded"), "{error}");
    let error = runtime
        .call("plugin__deadline__echo", json!(null))
        .unwrap_err();
    assert!(error.contains("deadline exceeded"), "{error}");
    zeus::cancel::reset();
}

#[test]
fn empty_runtime_has_no_capabilities() {
    let dir = tempfile::tempdir().unwrap();
    let mut runtime = PluginSet::load(&[], dir.path(), &dir.path().join("state"))
        .unwrap()
        .connect()
        .unwrap();
    assert!(runtime.schemas().is_empty());
    assert_eq!(
        runtime.skill_catalog(),
        json!({"skills":[],"skipped":[],"truncated":false})
    );
    assert!(runtime.call("plugin__none__echo", json!({})).is_err());
    assert!(runtime.read_skill("none/guide").is_err());
    assert!(runtime.hook("run_start", json!({})).unwrap().is_empty());
    assert!(PluginRuntime::empty().schemas().is_empty());
    let spec: PluginSpec = serde_json::from_value(json!({"manifest":"bundle.json"})).unwrap();
    assert!(spec.allowed_tools.is_empty());
    assert!(spec.allowed_hooks.is_empty());
    assert!(serde_json::from_value::<PluginSpec>(json!({"manifest":"x", "unknown":1})).is_err());
}
