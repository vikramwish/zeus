use zeus::evidence;

use evidence::{validate_findings, write_report, EvidenceKind, Finding};

#[test]
fn report_persists_records_with_explicit_coverage_without_overwrite() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("report.json");
    let sources = vec!["tool:event-1".to_owned()];
    let records = vec![
        finding(EvidenceKind::Observation, &sources[0]),
        finding(EvidenceKind::Inference, &sources[0]),
        finding(EvidenceKind::Unknown, ""),
    ];
    write_report(&path, "artifact.bin", &records, &sources).unwrap();
    let bytes = std::fs::read(&path).unwrap();
    let report: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(report["target"], "artifact.bin");
    assert_eq!(report["coverage"], "Findings cover cited evidence only; not original-source recovery or universal equivalence.");
    assert_eq!(report["findings"], serde_json::to_value(&records).unwrap());
    assert!(write_report(&path, "changed", &[], &[]).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
}

#[test]
fn invalid_reports_do_not_create_output_and_io_failures_propagate() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("report.json");
    assert!(write_report(
        &path,
        "artifact",
        &[finding(EvidenceKind::Observation, "invented")],
        &[]
    )
    .is_err());
    assert!(!path.exists());
    assert!(write_report(
        &dir.path().join("missing/report.json"),
        "artifact",
        &[],
        &[]
    )
    .is_err());
    assert!(write_report(dir.path(), "artifact", &[], &[]).is_err());
}

#[cfg(unix)]
#[test]
fn existing_and_dangling_report_symlinks_are_never_followed() {
    use std::os::unix::fs::symlink;
    let dir = tempfile::tempdir().unwrap();
    let destination = dir.path().join("destination");
    let report = dir.path().join("report.json");
    symlink(&destination, &report).unwrap();
    assert!(write_report(&report, "artifact", &[], &[]).is_err());
    assert!(!destination.exists());
    std::fs::write(&destination, b"preserve me").unwrap();
    assert!(write_report(&report, "artifact", &[], &[]).is_err());
    assert_eq!(std::fs::read(&destination).unwrap(), b"preserve me");
    assert!(std::fs::symlink_metadata(&report)
        .unwrap()
        .file_type()
        .is_symlink());
}

#[test]
fn concurrent_report_creators_have_exactly_one_winner() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("report.json");
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    let handles: Vec<_> = (0..2)
        .map(|n| {
            let path = path.clone();
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                barrier.wait();
                write_report(&path, &n.to_string(), &[], &[])
            })
        })
        .collect();
    let successes = handles
        .into_iter()
        .map(|h| h.join().unwrap())
        .filter(Result::is_ok)
        .count();
    assert_eq!(successes, 1);
    let report: serde_json::Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    assert!(["0", "1"].contains(&report["target"].as_str().unwrap()));
}

fn finding(kind: EvidenceKind, source: &str) -> Finding {
    Finding {
        kind,
        claim: "A model claim, not independently verified".into(),
        source: source.into(),
        limitations: vec![],
    }
}

#[test]
fn validation_binds_all_citations_exactly_and_allows_unsourced_unknowns() {
    let sources = vec!["tool:event-1".to_owned()];
    for kind in [
        EvidenceKind::Observation,
        EvidenceKind::Inference,
        EvidenceKind::Unknown,
    ] {
        assert!(validate_findings(&[finding(kind.clone(), &sources[0])], &sources).is_ok());
        for invalid in ["tool:event-2", " tool:event-1", "tool:event-1 "] {
            assert!(validate_findings(&[finding(kind.clone(), invalid)], &sources).is_err());
        }
    }
    assert!(validate_findings(&[finding(EvidenceKind::Unknown, "")], &sources).is_ok());
    for kind in [EvidenceKind::Observation, EvidenceKind::Inference] {
        assert!(validate_findings(&[finding(kind, "")], &["".into()]).is_err());
    }
    for claim in ["", " \n\t"] {
        let mut item = finding(EvidenceKind::Unknown, "");
        item.claim = claim.into();
        assert!(validate_findings(&[item], &sources).is_err());
    }
    assert!(validate_findings(&[], &[]).is_ok());
}

#[test]
fn changed_artifact_bytes_invalidate_old_inventory_bound_citations() {
    use sha2::{Digest, Sha256};
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("artifact.bin");
    let source_id = || {
        format!(
            "artifact.bin#{:x}",
            Sha256::digest(std::fs::read(&path).unwrap())
        )
    };
    std::fs::write(&path, b"original artifact").unwrap();
    let original = source_id();
    let item = finding(EvidenceKind::Observation, &original);
    assert!(
        validate_findings(std::slice::from_ref(&item), std::slice::from_ref(&original)).is_ok()
    );
    std::fs::write(&path, b"changed artifact").unwrap();
    let changed = source_id();
    assert_ne!(original, changed);
    assert!(validate_findings(&[item], std::slice::from_ref(&changed)).is_err());
    assert!(validate_findings(&[finding(EvidenceKind::Inference, &changed)], &[changed]).is_ok());
}

#[test]
fn untrusted_records_have_fixed_byte_and_count_bounds() {
    let mut item = finding(EvidenceKind::Unknown, "");
    item.claim = "x".repeat(8192);
    item.limitations = vec!["x".repeat(2048); 32];
    assert!(validate_findings(&vec![item.clone(); 256], &[]).is_ok());
    assert!(validate_findings(&vec![item.clone(); 257], &[]).is_err());
    item.claim.push('x');
    assert!(validate_findings(&[item.clone()], &[]).is_err());
    item.claim = "é".repeat(4097);
    assert!(validate_findings(&[item.clone()], &[]).is_err());
    item.claim = "claim".into();
    item.limitations.push("another".into());
    assert!(validate_findings(&[item.clone()], &[]).is_err());
    item.limitations = vec!["x".repeat(2049)];
    assert!(validate_findings(&[item.clone()], &[]).is_err());
    item.limitations.clear();
    item.source = "s".repeat(4096);
    assert!(validate_findings(&[item.clone()], &[item.source.clone()]).is_ok());
    item.source.push('s');
    assert!(validate_findings(&[item.clone()], &[item.source.clone()]).is_err());
}

#[test]
fn evidence_kinds_round_trip_without_promoting_claims() {
    for (kind, name) in [
        (EvidenceKind::Observation, "observation"),
        (EvidenceKind::Inference, "inference"),
        (EvidenceKind::Unknown, "unknown"),
    ] {
        let finding = Finding {
            kind,
            claim: "Unverified model-authored claim".into(),
            source: "tool:event-1".into(),
            limitations: vec!["No execution coverage".into()],
        };
        let encoded = serde_json::to_value(&finding).unwrap();
        assert_eq!(encoded["kind"], name);
        let decoded: Finding = serde_json::from_value(encoded.clone()).unwrap();
        assert_eq!(serde_json::to_value(decoded).unwrap(), encoded);
    }
    assert!(serde_json::from_str::<Finding>(
        r#"{"kind":"verified","claim":"x","source":"s","limitations":[]}"#
    )
    .is_err());
    assert!(serde_json::from_str::<Finding>(r#"{"kind":"observation","claim":"x"}"#).is_err());
}
