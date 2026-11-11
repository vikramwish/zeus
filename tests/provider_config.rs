use serde_json::{json, Value};
use zeus::config::{Approvals, Job};

fn load(extra: Value) -> Result<Job, String> {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("source");
    std::fs::create_dir(&source).unwrap();
    std::fs::write(source.join("input.txt"), "fixture").unwrap();
    let mut config = json!({"task":"inspect", "source":source, "state_dir":root.path().join("state"),
        "endpoint":"http://127.0.0.1:9/v1/chat/completions", "model":"custom/model-not-in-a-catalog"});
    config
        .as_object_mut()
        .unwrap()
        .extend(extra.as_object().unwrap().clone());
    let file = root.path().join("job.json");
    std::fs::write(&file, serde_json::to_vec(&config).unwrap()).unwrap();
    Job::load(
        &file,
        Approvals {
            network: true,
            exec: false,
            mcp: false,
            plugins: false,
        },
    )
}

#[test]
fn arbitrary_model_ids_work_with_each_explicit_protocol() {
    for protocol in ["chat_completions", "responses", "anthropic"] {
        let result = load(json!({"protocol":protocol}));
        assert!(result.is_ok(), "{protocol}: {result:?}");
    }
    assert!(load(json!({})).is_ok(), "old jobs remain compatible");
}

#[test]
fn subscription_credentials_are_pinned_to_official_responses_endpoint() {
    let valid = json!({"auth":"chatgpt", "protocol":"responses", "endpoint":"https://api.openai.com/v1/responses"});
    assert!(load(valid.clone()).is_ok());
    for endpoint in [
        "http://127.0.0.1:1234/v1/responses",
        "https://api.openai.com.evil.invalid/v1/responses",
        "https://api.openai.com/v1/chat/completions",
        "https://api.openai.com/v1/responses?redirect=evil",
    ] {
        let mut config = valid.clone();
        config["endpoint"] = json!(endpoint);
        assert!(load(config).unwrap_err().contains("ChatGPT"));
    }
    let mut config = valid.clone();
    config["protocol"] = json!("chat_completions");
    assert!(load(config).unwrap_err().contains("ChatGPT"));
    let mut config = valid;
    config["api_key_env"] = json!("PAID_API_KEY");
    assert!(load(config).unwrap_err().contains("ChatGPT"));
}

#[test]
fn runtime_also_rejects_subscription_endpoint_override_before_loading_credentials() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    std::fs::create_dir(&source).unwrap();
    std::fs::write(source.join("file.txt"), "fixture").unwrap();
    let state = temp.path().join("state");
    let job: Job = serde_json::from_value(json!({"task":"inspect", "source":source,
        "state_dir":state, "endpoint":"http://127.0.0.1:9/v1/responses", "model":"custom",
        "auth":"chatgpt", "protocol":"responses"}))
    .unwrap();
    let error = zeus::runtime::run(job).unwrap_err();
    assert!(error.contains("ChatGPT"), "{error}");
    assert!(!state.exists());
}

#[test]
fn unknown_auth_and_protocol_fail_closed() {
    assert!(load(json!({"auth":"borrow_claude_login"})).is_err());
    assert!(load(json!({"protocol":"guess"})).is_err());
}
