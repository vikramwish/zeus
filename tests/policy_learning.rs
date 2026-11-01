use policy::{Policy, Strategy};
use zeus::policy;

#[test]
fn explores_untried_arms_then_uses_rewards_per_context() {
    let mut p = Policy::default();
    p.update("a", Strategy::Direct, false).unwrap();
    assert_eq!(p.choose("a").unwrap(), Strategy::Parallel);
    p.update("a", Strategy::Parallel, true).unwrap();
    assert_eq!(p.choose("a").unwrap(), Strategy::Parallel);
    assert_eq!(p.choose("b").unwrap(), Strategy::Direct);
    p.update("b", Strategy::Direct, true).unwrap();
    p.update("b", Strategy::Parallel, false).unwrap();
    assert_eq!(p.choose("b").unwrap(), Strategy::Direct);
    let state = serde_json::to_value(&p).unwrap();
    assert_eq!(state["contexts"]["a"][0]["trials"], 1);
    assert_eq!(state["contexts"]["a"][1]["successes"], 1);
}

#[test]
fn rejects_invalid_contexts_without_mutation() {
    let mut p = Policy::default();
    for key in ["".to_owned(), "  ".to_owned(), "x".repeat(257)] {
        assert!(p.choose(&key).is_err());
        assert!(p.update(&key, Strategy::Direct, true).is_err());
    }
    assert_eq!(
        serde_json::to_value(p).unwrap(),
        serde_json::json!({"contexts": {}})
    );
}

#[test]
fn rejects_corrupt_deserialized_state_before_use() {
    for arms in [
        serde_json::json!([{"trials": 0,"successes": 1},{"trials": 1,"successes": 0}]),
        serde_json::json!([{"trials": u64::MAX,"successes": 0},{"trials": 1,"successes": 0}]),
    ] {
        let mut p: Policy =
            serde_json::from_value(serde_json::json!({"contexts": {"a": arms}})).unwrap();
        let before = serde_json::to_value(&p).unwrap();
        assert!(p.choose("a").is_err());
        assert!(p.update("a", Strategy::Direct, true).is_err());
        assert_eq!(serde_json::to_value(p).unwrap(), before);
    }
}

#[test]
fn saturated_counts_reject_update_without_mutation() {
    let mut p: Policy = serde_json::from_value(serde_json::json!({"contexts": {"a": [
        {"trials": u64::MAX, "successes": u64::MAX}, {"trials": 0, "successes": 0}
    ]}}))
    .unwrap();
    let before = serde_json::to_value(&p).unwrap();
    assert!(p.update("a", Strategy::Parallel, true).is_err());
    assert_eq!(serde_json::to_value(p).unwrap(), before);
}

#[test]
fn context_capacity_is_bounded_without_evicting_evidence() {
    let mut p = Policy::default();
    for i in 0..1024 {
        p.update(&format!("c{i}"), Strategy::Direct, false).unwrap();
    }
    let before = serde_json::to_value(&p).unwrap();
    assert!(p.update("overflow", Strategy::Direct, true).is_err());
    assert!(p.choose("overflow").is_err());
    assert_eq!(serde_json::to_value(&p).unwrap(), before);
    p.update("c0", Strategy::Parallel, true).unwrap();
    assert_eq!(p.choose("c0").unwrap(), Strategy::Parallel);
}

#[test]
fn persists_and_reloads_decisions_and_counts() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("policy.json");
    let mut p = Policy::load(&path).unwrap();
    p.save(&path).unwrap();
    let old_file = std::fs::File::open(&path).unwrap();
    p.update("a", Strategy::Direct, false).unwrap();
    p.update("a", Strategy::Parallel, true).unwrap();
    p.save(&path).unwrap();
    let loaded = Policy::load(&path).unwrap();
    assert_eq!(loaded.choose("a").unwrap(), Strategy::Parallel);
    assert_eq!(
        serde_json::to_value(&loaded).unwrap(),
        serde_json::to_value(&p).unwrap()
    );
    // An already-open reader retains the complete old inode, proving replacement.
    let previous: Policy = serde_json::from_reader(old_file).unwrap();
    assert_eq!(previous.choose("a").unwrap(), Strategy::Direct);
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
}

#[test]
fn corrupt_files_fail_closed_instead_of_resetting_learning() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("policy.json");
    let invalid = [
        "",
        "{",
        "{}",
        "null",
        "{\"contexts\":{},\"unknown\":1}",
        r#"{"contexts":{"a":[{"trials":0,"successes":1},{"trials":0,"successes":0}]}}"#,
        r#"{"contexts":{"a":[{"trials":1,"successes":NaN},{"trials":0,"successes":0}]}}"#,
        r#"{"contexts":{"a":[{"trials":1,"successes":1e309},{"trials":0,"successes":0}]}}"#,
        r#"{"contexts":{"a":[{"trials":-1,"successes":0},{"trials":0,"successes":0}]}}"#,
        r#"{"contexts":{"a":[{"trials":1.5,"successes":0},{"trials":0,"successes":0}]}}"#,
        r#"{"contexts":{"a":[{"trials":1,"successes":0,"extra":1},{"trials":0,"successes":0}]}}"#,
        r#"{"contexts":{" ":[{"trials":1,"successes":0},{"trials":0,"successes":0}]}}"#,
    ];
    for data in invalid {
        std::fs::write(&path, data).unwrap();
        assert!(Policy::load(&path).is_err(), "accepted {data}");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), data);
    }
    assert!(Policy::load(dir.path()).is_err());
}

#[test]
fn oversized_deserialized_context_table_is_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("policy.json");
    let arms = serde_json::json!([{"trials":1,"successes":0},{"trials":0,"successes":0}]);
    let contexts: serde_json::Map<String, serde_json::Value> =
        (0..1025).map(|i| (format!("c{i}"), arms.clone())).collect();
    let data = serde_json::json!({"contexts":contexts});
    std::fs::write(&path, serde_json::to_vec(&data).unwrap()).unwrap();
    assert!(Policy::load(&path).is_err());
    let p: Policy = serde_json::from_value(data).unwrap();
    assert!(p.choose("c0").is_err());
}

#[test]
fn invalid_save_preserves_last_good_state() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("policy.json");
    Policy::default().save(&path).unwrap();
    let before = std::fs::read(&path).unwrap();
    let p: Policy = serde_json::from_value(serde_json::json!({"contexts":{"a":[
        {"trials":0,"successes":1},{"trials":0,"successes":0}
    ]}}))
    .unwrap();
    assert!(p.save(&path).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), before);
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
}

#[test]
fn load_bounds_input_size() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("policy.json");
    let data = format!("{}{{\"contexts\":{{}}}}", " ".repeat(2 * 1024 * 1024));
    std::fs::write(&path, data).unwrap();
    assert!(Policy::load(&path).is_err());
}

#[test]
fn duplicate_contexts_cannot_silently_replace_evidence() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("policy.json");
    let arms = r#"[{"trials":1,"successes":0},{"trials":0,"successes":0}]"#;
    std::fs::write(
        &path,
        format!(r#"{{"contexts":{{"a":{arms},"a":{arms}}}}}"#),
    )
    .unwrap();
    assert!(Policy::load(&path).is_err());
}

// These exercise existing routing behavior; synthetic verifier outcomes are not
// evidence of model skill gains or real-world task improvements.
#[test]
fn synthetic_outcomes_shift_routing_beyond_equal_exposure() {
    let mut p = Policy::default();
    let mut wins = [0; 2];
    for _ in 0..400 {
        for (i, (context, winner)) in [
            ("direct-wins", Strategy::Direct),
            ("parallel-wins", Strategy::Parallel),
        ]
        .into_iter()
        .enumerate()
        {
            let chosen = p.choose(context).unwrap();
            let verified = chosen == winner;
            wins[i] += usize::from(verified);
            p.update(context, chosen, verified).unwrap();
        }
    }
    // Equal exposure yields exactly 200 successes in each toy environment.
    assert!(wins.iter().all(|&n| n > 350));
    println!("synthetic successes per 400 rounds: {wins:?}; equal exposure: [200, 200]");
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("policy.json");
    p.save(&path).unwrap();
    let loaded = Policy::load(&path).unwrap();
    assert_eq!(
        serde_json::to_value(&p).unwrap(),
        serde_json::to_value(&loaded).unwrap()
    );
    for c in ["direct-wins", "parallel-wins"] {
        assert_eq!(p.choose(c).unwrap(), loaded.choose(c).unwrap());
    }
}

#[test]
fn confidence_bonus_explores_even_when_empirical_mean_is_worse() {
    let mut p = Policy::default();
    for _ in 0..100 {
        p.update("a", Strategy::Direct, true).unwrap();
    }
    p.update("a", Strategy::Parallel, false).unwrap();
    assert_eq!(p.choose("a").unwrap(), Strategy::Parallel);
}

#[test]
fn exact_ucb_ties_choose_direct() {
    let mut p = Policy::default();
    p.update("a", Strategy::Parallel, true).unwrap();
    assert_eq!(p.choose("a").unwrap(), Strategy::Direct);
    p.update("a", Strategy::Direct, true).unwrap();
    assert_eq!(p.choose("a").unwrap(), Strategy::Direct);
}

#[test]
fn failed_rename_cleans_temporary_file_and_preserves_destination() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("existing-directory");
    std::fs::create_dir(&target).unwrap();
    std::fs::write(target.join("sentinel"), b"unchanged").unwrap();
    assert!(Policy::default().save(&target).is_err());
    assert_eq!(
        std::fs::read(target.join("sentinel")).unwrap(),
        b"unchanged"
    );
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
    assert!(Policy::default()
        .save(&dir.path().join("absent/policy.json"))
        .is_err());
}

#[test]
fn unseen_context_chooses_direct_deterministically() {
    let policy = Policy::default();
    assert_eq!(policy.choose("rust:small").unwrap(), Strategy::Direct);
    assert_eq!(policy.choose("rust:small").unwrap(), Strategy::Direct);
}
