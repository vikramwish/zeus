use serde_json::json;
use std::fs;
use zeus::config::{Approvals, Job};

#[test]
fn caller_can_select_parallel_or_direct_without_rewriting_policy() {
    let tmp = tempfile::tempdir().unwrap();
    fs::create_dir(tmp.path().join("source")).unwrap();
    let path = tmp.path().join("job.json");
    let base = json!({
        "task":"implement two independent changes", "source":"source", "state_dir":"state",
        "endpoint":"http://127.0.0.1:9/v1/chat/completions", "model":"fixture"
    });
    for strategy in ["parallel", "direct", "Parallel", "Direct"] {
        let mut job = base.clone();
        job["strategy"] = json!(strategy);
        fs::write(&path, job.to_string()).unwrap();
        assert!(
            Job::load(
                &path,
                Approvals {
                    network: true,
                    ..Default::default()
                }
            )
            .is_ok(),
            "explicit {strategy} rejected"
        );
        assert!(Job::load(&path, Approvals::default())
            .unwrap_err()
            .contains("allow-network"));
        assert!(!tmp.path().join("state").exists());
    }
    fs::write(&path, base.to_string()).unwrap();
    assert!(Job::load(
        &path,
        Approvals {
            network: true,
            ..Default::default()
        }
    )
    .is_ok());
    for invalid in [json!("unlimited"), json!(true), json!({"Parallel":true})] {
        let mut job = base.clone();
        job["strategy"] = invalid;
        fs::write(&path, job.to_string()).unwrap();
        assert!(Job::load(
            &path,
            Approvals {
                network: true,
                ..Default::default()
            }
        )
        .is_err());
    }
}
