use zeus::workspace::Workspace;

fn nested(root: &std::path::Path, depth: usize) -> std::path::PathBuf {
    let mut path = root.to_path_buf();
    for _ in 0..depth {
        path.push("d");
    }
    std::fs::create_dir_all(&path).unwrap();
    std::fs::write(path.join("leaf.txt"), "leaf").unwrap();
    path
}

// The walker recurses once per directory level; the depth is bounded so a
// pathological tree yields a named error instead of unbounded recursion.
#[test]
fn snapshot_refuses_pathological_directory_depth() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    nested(&source, 140);
    let error = Workspace::snapshot(&source, &temp.path().join("candidate")).unwrap_err();
    assert!(error.contains("depth limit"), "{error}");
    assert!(!temp.path().join("candidate").join("d/d/d").exists());
}

#[test]
fn snapshot_accepts_deep_but_ordinary_trees() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    nested(&source, 40);
    let workspace = Workspace::snapshot(&source, &temp.path().join("candidate")).unwrap();
    assert!(workspace
        .inventory
        .entries
        .iter()
        .any(|e| e.path.ends_with("leaf.txt")));
}
