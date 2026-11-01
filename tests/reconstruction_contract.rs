use std::process::{Command, Output};

// Public verifier sensitivity controls, not model acceptance evidence.
fn repaired_source() -> String {
    include_str!("../examples/reconstruction/source/kv.py").replace(
        "    # INTENTIONAL BUG: mutations vanish at process exit. Implement persistence here.\n    pass",
        "    import tempfile\n    temporary = None\n    try:\n        with tempfile.NamedTemporaryFile(mode='w', encoding='utf-8', dir=path.parent, delete=False) as stream:\n            temporary = Path(stream.name)\n            json.dump(state, stream, ensure_ascii=True, allow_nan=False)\n        temporary.replace(path)\n    finally:\n        if temporary is not None:\n            temporary.unlink(missing_ok=True)",
    )
}

fn verify(source: &str) -> Output {
    let candidate = tempfile::tempdir().unwrap();
    std::fs::write(candidate.path().join("kv.py"), source).unwrap();
    Command::new("python3")
        .args([
            "-I",
            concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/examples/reconstruction/verify.py"
            ),
        ])
        .current_dir(candidate.path())
        .output()
        .unwrap()
}

#[test]
fn public_verifier_accepts_atomic_persistence_control() {
    let output = verify(&repaired_source());
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn public_verifier_rejects_invalid_json_exception_on_write_failure() {
    let mutant = repaired_source().replace(
        "except (OSError, ValueError, RecursionError):",
        "except (OSError, ValueError, RecursionError, json.JSONEncodeError):",
    );
    let output = verify(&mutant);
    assert!(
        !output.status.success(),
        "write-error mutant passed: {}",
        String::from_utf8_lossy(&output.stdout)
    );
    assert!(String::from_utf8_lossy(&output.stderr).contains("state write failure"));
}

#[test]
fn public_verifier_rejects_unhandled_state_read_failure() {
    let mutant = repaired_source().replace(
        "except (ValueError, OSError, RecursionError):",
        "except (ValueError, RecursionError):",
    );
    let output = verify(&mutant);
    assert!(
        !output.status.success(),
        "read-error mutant passed: {}",
        String::from_utf8_lossy(&output.stdout)
    );
    assert!(String::from_utf8_lossy(&output.stderr).contains("state read failure"));
}
