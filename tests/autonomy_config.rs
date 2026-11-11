use serde_json::json;
use std::fs;
use zeus::config::{Approvals, Job};

#[test]
fn autonomy_settings_are_explicit_bounded_and_do_not_grant_execution() {
    let tmp = tempfile::tempdir().unwrap();
    fs::create_dir(tmp.path().join("source")).unwrap();
    let job = tmp.path().join("job.json");
    let base = json!({"task":"repair", "source":"source", "state_dir":"state", "endpoint":"http://127.0.0.1:9/", "model":"fixture"});
    let approval = Approvals {
        network: true,
        ..Default::default()
    };
    let mut scoped_mcp = base.clone();
    scoped_mcp["autonomy"] = json!({"write_paths":[]});
    scoped_mcp["mcp"] = json!([{"name":"host","command":["/bin/sh"],"allowed_tools":["write"]}]);
    fs::write(&job, scoped_mcp.to_string()).unwrap();
    assert!(
        Job::load(
            &job,
            Approvals {
                network: true,
                mcp: true,
                exec: true,
                plugins: false,
            }
        )
        .is_err(),
        "host MCP bypasses a constrained model-write scope"
    );
    for autonomy in [
        json!({"write_paths":["src/main.rs"],"max_repairs":0,"max_model_calls":4}),
        json!({"write_paths":[],"max_repairs":1}),
        json!({"max_repairs":8,"max_model_calls":1024}),
    ] {
        let mut value = base.clone();
        value["autonomy"] = autonomy;
        fs::write(&job, value.to_string()).unwrap();
        assert!(
            Job::load(&job, approval).is_ok(),
            "valid autonomy rejected: {value}"
        );
        assert!(Job::load(&job, Approvals::default()).is_err());
        value["verifier"] = json!(["/bin/sh"]);
        fs::write(&job, value.to_string()).unwrap();
        assert!(Job::load(&job, approval)
            .unwrap_err()
            .contains("allow-exec"));
        assert!(!tmp.path().join("state").exists());
    }
    for autonomy in [
        json!({"max_repairs":9}),
        json!({"max_repairs":-1}),
        json!({"max_model_calls":0}),
        json!({"max_model_calls":1025}),
        json!({"write_paths":["../escape"]}),
        json!({"write_paths":["/absolute"]}),
        json!({"write_paths":["a//b"]}),
        json!({"write_paths":["a/./b"]}),
        json!({"write_paths":["a","a"]}),
        json!({"write_paths":["a", "a/b"]}),
        json!({"write_paths":[""]}),
        json!({"unrestricted":true}),
    ] {
        let mut value = base.clone();
        value["autonomy"] = autonomy;
        fs::write(&job, value.to_string()).unwrap();
        assert!(
            Job::load(&job, approval).is_err(),
            "invalid autonomy accepted: {value}"
        );
    }
}
