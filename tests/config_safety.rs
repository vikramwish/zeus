mod common;
use serde_json::json;
use std::{fs, path::Path};
use zeus::config::{Approvals, Job};

fn load(root: &Path, verifier: Vec<String>) -> Result<Job, String> {
    let source = root.join("source");
    fs::create_dir_all(&source).unwrap();
    let path = root.join("job.json");
    fs::write(
        &path,
        serde_json::to_vec(&json!({
            "task":"repair implementation", "source":source,
            "state_dir":root.join("state"), "endpoint":"http://127.0.0.1:9/v1/chat/completions",
            "model":"fixture", "verifier":verifier
        }))
        .unwrap(),
    )
    .unwrap();
    Job::load(
        &path,
        Approvals {
            network: true,
            exec: true,
            mcp: false,
        },
    )
}

#[test]
fn external_entry_points_are_required_without_guessing_interpreter_arguments() {
    let root = tempfile::tempdir().unwrap();
    let script = root.path().join("trusted.sh");
    fs::write(&script, "exit 0\n").unwrap();
    for argv in [
        vec!["/bin/sh".into(), script.display().to_string()],
        vec![common::python3(), "-I".into(), script.display().to_string()],
        vec!["/bin/sh".into()],
        vec![],
    ] {
        assert!(load(root.path(), argv).is_ok());
    }
    for argv in [
        vec!["/bin/sh".into(), "missing.sh".into()],
        vec!["/bin/sh".into(), "-c".into(), "./check.sh".into()],
        vec![common::python3(), "-m".into(), "pytest".into()],
        vec![
            "/bin/sh".into(),
            script.display().to_string(),
            "candidate-test.sh".into(),
        ],
    ] {
        assert!(load(root.path(), argv).is_err());
    }
    let source_script = root.path().join("source/check.sh");
    fs::write(&source_script, "exit 0\n").unwrap();
    assert!(load(
        root.path(),
        vec!["/bin/sh".into(), source_script.display().to_string()]
    )
    .is_err());
    std::os::unix::fs::symlink(&source_script, root.path().join("alias.sh")).unwrap();
    assert!(load(
        root.path(),
        vec![
            "/bin/sh".into(),
            root.path().join("alias.sh").display().to_string()
        ]
    )
    .is_err());
    fs::create_dir(root.path().join("state")).unwrap();
    let state_script = root.path().join("state/check.sh");
    fs::write(&state_script, "exit 0\n").unwrap();
    assert!(load(
        root.path(),
        vec!["/bin/sh".into(), state_script.display().to_string()]
    )
    .is_err());
}

#[test]
fn nonexecutable_external_program_is_rejected_but_script_data_is_allowed() {
    use std::os::unix::fs::PermissionsExt;
    let root = tempfile::tempdir().unwrap();
    let program = root.path().join("trusted.sh");
    fs::write(&program, "#!/bin/sh\nexit 0\n").unwrap();
    for mode in [0o600, 0o601] {
        fs::set_permissions(&program, fs::Permissions::from_mode(mode)).unwrap();
        let error = load(root.path(), vec![program.display().to_string()]).unwrap_err();
        assert!(error.contains("executable"), "{error}");
    }
    assert!(load(
        root.path(),
        vec!["/bin/sh".into(), program.display().to_string()]
    )
    .is_ok());
    assert!(!root.path().join("state").exists());
}

#[test]
fn candidate_relative_verifier_cannot_become_a_reward_authority() {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("source")).unwrap();
    let script = root.path().join("source/check.sh");
    fs::write(&script, "exit 1\n").unwrap();
    for body in ["exit 1\n", "exit 0\n"] {
        fs::write(&script, body).unwrap();
        let result = load(root.path(), vec!["/bin/sh".into(), "check.sh".into()]);
        assert!(
            result.is_err(),
            "candidate verifier was accepted: {result:?}"
        );
        assert!(!root.path().join("state").exists());
    }
}
