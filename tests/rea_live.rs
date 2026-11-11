//! Opt-in real REA smoke test; not a scripted MCP backend or decompilation benchmark.
use serde_json::json;
use zeus::mcp::{McpClient, McpConfig};

#[test]
#[ignore = "requires explicitly approved installed REA and Go executables"]
fn installed_rea_inspects_a_real_compiled_go_artifact() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("main.go");
    std::fs::write(
        &source,
        "package main\nimport \"fmt\"\nfunc main(){fmt.Println(\"zeus-artifact-probe\")}\n",
    )
    .unwrap();
    let artifact = temp.path().join("probe");
    let build = std::process::Command::new("/opt/homebrew/bin/go")
        .args(["build", "-o"])
        .arg(&artifact)
        .arg(&source)
        .output()
        .unwrap();
    assert!(
        build.status.success(),
        "{}",
        String::from_utf8_lossy(&build.stderr)
    );
    let hash = zeus::workspace::digest(&std::fs::read(&artifact).unwrap());
    let mut client = McpClient::connect(&McpConfig {
        name: "rea".into(),
        command: vec![
            "/usr/bin/env".into(),
            "REA_MCP_OUTPUT_SCHEMA_PROFILE=off".into(),
            "/opt/homebrew/bin/rea-agents".into(),
            "--mcp".into(),
        ],
        allowed_tools: vec!["inspect_go_binary".into()],
        timeout_seconds: zeus::mcp::DEFAULT_TIMEOUT_SECONDS,
    })
    .unwrap();
    assert_eq!(client.schemas().len(), 1);
    let evidence = client
        .call(
            "inspect_go_binary",
            json!({"path":artifact,"detail":"complete"}),
        )
        .unwrap();
    let serialized = evidence.to_string();
    assert!(
        serialized.contains(&hash),
        "evidence must bind to actual artifact hash: {serialized}"
    );
    assert!(
        serialized.contains("go1."),
        "expected observed Go compiler build identity: {serialized}"
    );
    assert!(client.call("execute_rizin_command", json!({})).is_err());
    println!("REAL_REA_PROOF: Go compiler metadata observed for SHA-256 {hash}; target was not executed; unlisted execution tool denied.");
}
