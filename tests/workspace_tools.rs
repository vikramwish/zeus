use std::fs;
use zeus::workspace::{digest, Workspace};

fn fixture(files: &[(&str, &[u8])]) -> (tempfile::TempDir, Workspace) {
    let tmp = tempfile::tempdir().unwrap();
    let source = tmp.path().join("source");
    fs::create_dir(&source).unwrap();
    for (path, bytes) in files {
        let target = source.join(path);
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::write(target, bytes).unwrap();
    }
    let ws = Workspace::snapshot(&source, &tmp.path().join("candidate")).unwrap();
    (tmp, ws)
}

#[test]
fn search_is_literal_ordered_utf8_and_digest_bound() {
    let (_tmp, ws) = fixture(&[
        ("z.txt", "héllo .*\n".as_bytes()),
        ("a.txt", b"no\n.* and .*\n"),
    ]);
    let found = ws.search(".*", 10).unwrap();
    let matches = found["matches"].as_array().unwrap();
    assert_eq!(matches.len(), 3);
    assert_eq!(matches[0]["path"], "a.txt");
    assert_eq!(matches[0]["line"], 2);
    assert_eq!(matches[2]["content"], "héllo .*");
    assert_eq!(
        matches[0]["source_id"],
        format!("a.txt#{}", digest(b"no\n.* and .*\n"))
    );
    assert_eq!(found["truncated"], false);
    ws.write("new.txt", "fresh .*", None).unwrap();
    assert_eq!(
        ws.search("fresh", 10).unwrap()["matches"][0]["path"],
        "new.txt"
    );
    assert_eq!(
        ws.search("absent", 10).unwrap()["matches"]
            .as_array()
            .unwrap()
            .len(),
        0
    );
}

#[test]
fn search_bounds_results_excerpts_and_inputs() {
    let line = format!("{}needle", "é".repeat(1000));
    let (_tmp, ws) = fixture(&[("a.txt", line.as_bytes()), ("b.txt", b"needle needle")]);
    for (query, count) in [("", 10), ("needle", 0), ("needle", 101)] {
        assert!(ws.search(query, count).is_err());
    }
    assert!(ws.search(&"x".repeat(4097), 10).is_err());
    let result = ws.search("needle", 1).unwrap();
    assert_eq!(result["matches"].as_array().unwrap().len(), 1);
    assert_eq!(result["truncated"], true);
    let item = &result["matches"][0];
    assert!(item["content"].as_str().unwrap().len() <= 512);
    assert!(item["content"].as_str().unwrap().contains("needle"));
    assert_eq!(item["content_truncated"], true);
    assert_eq!(ws.search("needle", 3).unwrap()["truncated"], false);
}

#[test]
fn search_skips_binary_oversize_secrets_and_symlinks_explicitly() {
    let (tmp, ws) = fixture(&[
        ("ok", b"needle"),
        ("binary", b"needle\0"),
        ("invalid", b"needle\xff"),
    ]);
    fs::write(ws.root.join("large"), vec![b'x'; 128 * 1024 + 1]).unwrap();
    // Search must not use inventory's 16 MiB read/hashing or fail on oversized files.
    let giant = fs::File::create(ws.root.join("giant")).unwrap();
    giant.set_len(17 * 1024 * 1024).unwrap();
    fs::write(ws.root.join(".env"), b"needle SECRET").unwrap();
    fs::create_dir(ws.root.join(".aws")).unwrap();
    fs::write(ws.root.join(".aws/config"), b"needle SECRET").unwrap();
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(tmp.path().join("source"), ws.root.join("escape")).unwrap();
        std::os::unix::fs::symlink(tmp.path().join("source/ok"), ws.root.join("link")).unwrap();
    }
    let found = ws.search("needle", 10).unwrap();
    assert_eq!(found["matches"].as_array().unwrap().len(), 1);
    assert_eq!(found["matches"][0]["path"], "ok");
    let skipped = found["skipped"].as_array().unwrap();
    for name in ["binary", "invalid", "large", "giant", ".env", ".aws"] {
        assert!(
            skipped
                .iter()
                .any(|v| v["path"] == name && v["reason"].as_str().is_some_and(|s| !s.is_empty())),
            "{name}"
        );
    }
    #[cfg(unix)]
    for name in ["escape", "link"] {
        assert!(skipped.iter().any(|v| v["path"] == name));
    }
    assert!(!found.to_string().contains("SECRET"));
}

#[test]
fn search_bounds_serialized_output_including_json_escaping() {
    let content = format!("needle{}\n", "\u{0001}".repeat(500)).repeat(100);
    let (_tmp, ws) = fixture(&[("a", content.as_bytes())]);
    let result = ws.search("needle", 100).unwrap();
    assert!(serde_json::to_vec(&result).unwrap().len() <= 128 * 1024);
    assert_eq!(result["truncated"], true);
}

#[test]
fn search_reports_file_byte_directory_and_diagnostic_budget_exhaustion() {
    let (_tmp, ws) = fixture(&[]);
    for i in 0..2001 {
        fs::write(ws.root.join(format!("f{i:04}")), b"").unwrap();
    }
    let result = ws.search("absent", 1).unwrap();
    assert_eq!(result["truncated"], true);
    assert_eq!(result["truncation_reason"], "file or byte limit");

    let (_tmp2, ws2) = fixture(&[]);
    let content = vec![b'x'; 128 * 1024];
    for i in 0..513 {
        fs::write(ws2.root.join(format!("f{i:04}")), &content).unwrap();
    }
    assert_eq!(
        ws2.search("absent", 1).unwrap()["truncation_reason"],
        "file or byte limit"
    );

    let (_tmp3, ws3) = fixture(&[]);
    for i in 0..10001 {
        fs::create_dir(ws3.root.join(format!("d{i:05}"))).unwrap();
    }
    assert_eq!(
        ws3.search("absent", 1).unwrap()["truncation_reason"],
        "directory entry limit"
    );

    let (_tmp4, ws4) = fixture(&[]);
    for i in 0..101 {
        fs::write(ws4.root.join(format!("b{i:03}")), b"\0").unwrap();
    }
    let result = ws4.search("absent", 1).unwrap();
    assert_eq!(result["skipped"].as_array().unwrap().len(), 100);
    assert_eq!(result["truncation_reason"], "diagnostic limit");
}

#[test]
fn edit_rejects_binary_replacement_without_modification() {
    let (_tmp, ws) = fixture(&[("a", b"before")]);
    assert!(ws
        .edit("a", "before", "after\0", &digest(b"before"))
        .is_err());
    assert_eq!(ws.read("a").unwrap().content, "before");
}

#[test]
fn edit_rejects_empty_missing_and_ambiguous_including_overlaps() {
    let (_tmp, ws) = fixture(&[("a.txt", b"aaa repeat repeat")]);
    let before = ws.read("a.txt").unwrap();
    for old in ["", "absent", "repeat", "aa"] {
        assert!(
            ws.edit("a.txt", old, "new", &before.sha256).is_err(),
            "{old:?}"
        );
        assert_eq!(ws.read("a.txt").unwrap().content, before.content);
    }
}

#[test]
fn edit_reuses_path_binary_and_size_guards() {
    let large = vec![b'x'; 128 * 1024 + 1];
    let (tmp, ws) = fixture(&[("a.txt", b"before"), ("binary", b"x\0y"), ("large", &large)]);
    for path in [
        "../source/a.txt",
        "/tmp/a.txt",
        ".env",
        "nested/credentials.json",
        "missing",
        "binary",
        "large",
    ] {
        assert!(
            ws.edit(path, "before", "after", &digest(b"before"))
                .is_err(),
            "{path}"
        );
    }
    assert!(ws
        .edit(
            "a.txt",
            "before",
            &"x".repeat(128 * 1024 + 1),
            &digest(b"before")
        )
        .is_err());
    assert_eq!(ws.read("a.txt").unwrap().content, "before");
    assert_eq!(
        fs::read_to_string(tmp.path().join("source/a.txt")).unwrap(),
        "before"
    );
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(tmp.path().join("source"), ws.root.join("escape")).unwrap();
        std::os::unix::fs::symlink(tmp.path().join("source/a.txt"), ws.root.join("link")).unwrap();
        for path in ["escape/a.txt", "link"] {
            assert!(ws
                .edit(path, "before", "after", &digest(b"before"))
                .is_err());
        }
    }
}

#[test]
fn edit_is_literal_digest_bound_and_source_is_immutable() {
    let before = "héllo .*\nsecond line\n";
    let (tmp, ws) = fixture(&[("a.txt", before.as_bytes())]);
    let hash = ws.read("a.txt").unwrap().sha256;
    ws.edit("a.txt", "héllo .*\nsecond", "你好 $1\nlast", &hash)
        .unwrap();
    assert_eq!(ws.read("a.txt").unwrap().content, "你好 $1\nlast line\n");
    assert!(ws
        .edit("a.txt", "last", "bad", &hash)
        .unwrap_err()
        .contains("stale"));
    assert_eq!(
        fs::read_to_string(tmp.path().join("source/a.txt")).unwrap(),
        before
    );
}
