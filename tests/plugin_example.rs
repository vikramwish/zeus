//! Exercise the shipped example over real MCP stdio, without a model/network.
use serde_json::json;
use zeus::plugins::{PluginSet, PluginSpec};

#[test]
fn shipped_echo_bundle_runs_tools_skills_and_hooks() {
    let temp = tempfile::tempdir().unwrap();
    let manifest =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/plugins/echo/plugin.json");
    let mut runtime = PluginSet::load(
        &[PluginSpec {
            manifest,
            allowed_tools: vec!["echo".into()],
            allowed_hooks: vec!["run_start".into(), "run_end".into(), "run_error".into()],
        }],
        &temp.path().join("source"),
        &temp.path().join("state"),
    )
    .unwrap()
    .connect()
    .unwrap();
    assert_eq!(runtime.schemas().len(), 1);
    let result = runtime
        .call("plugin__echo__echo", json!({"message":"hello Zeus"}))
        .unwrap();
    assert_eq!(result["content"][0]["text"], "hello Zeus");
    assert!(runtime.read_skill("echo/usage").unwrap()["content"]
        .as_str()
        .unwrap()
        .contains("plugin__echo__echo"));
    for event in ["run_start", "run_end", "run_error"] {
        let records = runtime.hook(event, json!({"fixture":true})).unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0]["event"], event);
    }
}
