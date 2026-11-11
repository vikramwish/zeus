use serde_json::{json, Value};
use zeus::{
    config::Job,
    judge::{self, Config},
    policy::Strategy,
};

fn job() -> Job {
    serde_json::from_value(json!({"task":"Fix the parser", "source":"/source", "state_dir":"/state",
        "endpoint":"http://localhost/v1/chat/completions", "model":"fixture", "verifier":["/external/check"]})).unwrap()
}
fn artifact(job: &Job) -> Value {
    let head = |intercept| {
        json!({"training_samples":40,
        "success_logit":[intercept,0,0,0,0,0,0,0,0],
        "log1p_tokens":[2,0,0,0,0,0,0,0,0],"log1p_seconds":[1,0,0,0,0,0,0,0,0]})
    };
    json!({"schema_version":1,"feature_schema":judge::FEATURE_SCHEMA,"model_version":"synthetic-test",
        "domain_sha256":judge::State::from_job(job).domain_sha256,
        "feature_min":[0,0,0,0,0,0,0,0],"feature_max":[1,1,1,1,1,1,1,1],"direct":head(-2),"parallel":head(2)})
}
fn evaluate(job: &Job, model: &Value) -> Value {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("model.json");
    std::fs::write(&path, model.to_string()).unwrap();
    let (record, bytes) = judge::evaluate(
        job,
        &Config {
            model_path: Some(path),
        },
        Strategy::Direct,
    );
    if record.model_sha256.is_some() {
        assert_eq!(bytes.unwrap(), model.to_string().as_bytes());
    }
    serde_json::to_value(record).unwrap()
}
#[test]
fn ranks_independent_outcome_predictions_without_changing_executed_action() {
    let job = job();
    let model = artifact(&job);
    let result = evaluate(&job, &model);
    assert_eq!(
        result["decision"],
        json!({"kind":"rank","preferred":"Parallel"})
    );
    assert_eq!(result["executed_strategy"], "Direct");
    assert_eq!(result["probability_status"], "uncalibrated");
    assert!(
        (result["predictions"][1]["success_probability"]
            .as_f64()
            .unwrap()
            - 0.880797)
            .abs()
            < 0.00001
    );
    let mut both_good = model;
    both_good["direct"]["success_logit"][0] = json!(3);
    let predictions = evaluate(&job, &both_good)["predictions"].clone();
    assert!(
        predictions[0]["success_probability"].as_f64().unwrap()
            + predictions[1]["success_probability"].as_f64().unwrap()
            > 1.8
    );
}
#[test]
fn abstains_on_missing_support_ambiguous_predictions_and_wrong_domain() {
    let job = job();
    for (field, value, reason) in [
        ("domain_sha256", json!("0".repeat(64)), "domain_mismatch"),
        (
            "feature_min",
            json!([1, 1, 1, 1, 1, 1, 1, 1]),
            "outside_support",
        ),
    ] {
        let mut model = artifact(&job);
        model[field] = value;
        assert_eq!(evaluate(&job, &model)["decision"]["reason"], reason);
    }
    let mut model = artifact(&job);
    model["parallel"]["training_samples"] = json!(29);
    assert_eq!(
        evaluate(&job, &model)["decision"]["reason"],
        "insufficient_samples"
    );
    model["parallel"]["training_samples"] = json!(30);
    model["direct"]["success_logit"][0] = json!(2);
    assert_eq!(evaluate(&job, &model)["decision"]["reason"], "ambiguous");
    model["direct"]["log1p_tokens"][0] = json!(-1);
    assert_eq!(
        evaluate(&job, &model)["decision"]["reason"],
        "invalid_prediction"
    );
    let mut no_verifier = job;
    no_verifier.verifier.clear();
    assert_eq!(
        evaluate(&no_verifier, &artifact(&no_verifier))["decision"]["reason"],
        "no_verifier"
    );
}
#[test]
fn malformed_or_oversized_models_are_nonfatal_abstentions() {
    let job = job();
    for value in [
        json!(null),
        json!({"schema_version":1}),
        json!({"garbage":true}),
    ] {
        assert_eq!(
            evaluate(&job, &value)["decision"]["reason"],
            "invalid_model"
        );
    }
    for (field, value) in [
        ("schema_version", json!(2)),
        ("feature_schema", json!("future")),
        ("feature_max", json!([2, 2, 2, 2, 2, 2, 2, 2])),
        ("extra", json!(true)),
    ] {
        let mut model = artifact(&job);
        model[field] = value;
        assert_eq!(
            evaluate(&job, &model)["decision"]["reason"],
            "invalid_model"
        );
    }
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("model.json");
    for bytes in [
        " ".repeat(65537),
        "{\"schema_version\":1,\"schema_version\":1}".into(),
        "{\"schema_version\":NaN}".into(),
    ] {
        std::fs::write(&path, bytes).unwrap();
        let (record, retained) = judge::evaluate(
            &job,
            &Config {
                model_path: Some(path.clone()),
            },
            Strategy::Direct,
        );
        assert!(retained.is_none());
        assert_eq!(
            serde_json::to_value(record).unwrap()["decision"]["reason"],
            "invalid_model"
        );
    }
}
#[test]
fn features_distinguish_absent_limits_and_domain_changes() {
    let mut job = job();
    let before = judge::State::from_job(&job);
    job.autonomy.max_model_calls = Some(1);
    job.autonomy.max_total_tokens = Some(1);
    let after = judge::State::from_job(&job);
    assert_eq!(after.features[4], 1.0);
    assert_eq!(after.features[6], 1.0);
    assert_eq!(before.features[4], 0.0);
    assert_eq!(before.features[6], 0.0);
    assert_eq!(before.domain_sha256, after.domain_sha256);
    job.model = "different".into();
    assert_ne!(
        before.domain_sha256,
        judge::State::from_job(&job).domain_sha256
    );
}
#[cfg(unix)]
#[test]
fn model_symlinks_are_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("link.json");
    std::os::unix::fs::symlink("/dev/zero", &path).unwrap();
    let (record, _) = judge::evaluate(
        &job(),
        &Config {
            model_path: Some(path),
        },
        Strategy::Direct,
    );
    assert_eq!(
        serde_json::to_value(record).unwrap()["decision"]["reason"],
        "invalid_model"
    );
}
