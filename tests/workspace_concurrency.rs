use std::{fs, sync::Barrier};
use zeus::workspace::{digest, Workspace};

// All contenders use the same observed version. Regardless of scheduling,
// exactly one may commit; atomic rename alone is not a compare-and-swap.
fn contend(existing: bool, edits: bool) {
    let tmp = tempfile::tempdir().unwrap();
    let source = tmp.path().join("source");
    fs::create_dir(&source).unwrap();
    let initial = "a".repeat(128 * 1024);
    if existing {
        fs::write(source.join("file.txt"), &initial).unwrap();
    }
    let ws = Workspace::snapshot(&source, &tmp.path().join("candidate")).unwrap();
    let expected = digest(initial.as_bytes());
    let barrier = Barrier::new(16);
    let results = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..16)
            .map(|i| {
                let (ws, barrier, expected, initial) = (&ws, &barrier, &expected, &initial);
                scope.spawn(move || {
                    let replacement = format!("worker-{i}");
                    barrier.wait();
                    let result = if edits {
                        ws.edit("file.txt", initial, &replacement, expected)
                    } else {
                        ws.write(
                            "file.txt",
                            &replacement,
                            existing.then_some(expected.as_str()),
                        )
                    };
                    (replacement, result)
                })
            })
            .collect();
        handles
            .into_iter()
            .map(|h| h.join().unwrap())
            .collect::<Vec<_>>()
    });
    let winners: Vec<_> = results
        .iter()
        .filter(|(_, result)| result.is_ok())
        .collect();
    assert_eq!(
        winners.len(),
        1,
        "multiple writers committed from one observed version: {results:?}"
    );
    assert_eq!(ws.read("file.txt").unwrap().content, winners[0].0);
    for (_, result) in results.iter().filter(|(_, result)| result.is_err()) {
        assert!(result.as_ref().unwrap_err().contains("expected_sha256"));
    }
    if existing {
        assert_eq!(
            fs::read_to_string(source.join("file.txt")).unwrap(),
            initial
        );
    } else {
        assert!(!source.join("file.txt").exists());
    }
    assert!(fs::read_dir(&ws.root).unwrap().all(|entry| !entry
        .unwrap()
        .file_name()
        .to_string_lossy()
        .starts_with(".native-write-")));
}

#[test]
fn concurrent_replacements_have_one_digest_winner() {
    contend(true, false);
}

#[test]
fn concurrent_creations_have_one_winner() {
    contend(false, false);
}

#[test]
fn concurrent_literal_edits_have_one_digest_winner() {
    contend(true, true);
}
