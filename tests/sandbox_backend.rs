use std::path::Path;
use zeus::process::bubblewrap_args;

// The bubblewrap command line is pure data, so its invariants are checked on
// every platform even though bwrap itself only runs on Linux.
#[test]
fn bubblewrap_profile_mirrors_the_seatbelt_boundary() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("candidate");
    std::fs::create_dir(&root).unwrap();
    let exe = temp.path().join("python3");
    let launcher = temp.path().join("zeus");
    let script = temp.path().join("check.py");
    let args = bubblewrap_args(&root, &exe, &launcher, std::slice::from_ref(&script)).unwrap();
    let joined = args.join(" ");
    let s = |p: &Path| p.to_str().unwrap().to_string();
    // Isolation: no network, no inherited namespaces, dies with the host.
    for required in ["--unshare-all", "--die-with-parent", "--new-session"] {
        assert!(
            args.iter().any(|a| a == required),
            "{required} missing: {joined}"
        );
    }
    // Writable: only the candidate. Never the whole filesystem.
    assert!(joined.contains(&format!("--bind {} {}", s(&root), s(&root))));
    assert!(!joined.contains("--bind / /"), "{joined}");
    assert!(!joined.contains("--ro-bind / /"), "{joined}");
    assert!(joined.contains(&format!("--chdir {}", s(&root))));
    // Read-only: the executable, the launcher and the explicit external files.
    for file in [&exe, &launcher, &script] {
        assert!(
            joined.contains(&format!("--ro-bind {} {}", s(file), s(file))),
            "{joined}"
        );
    }
    // Inert home, as on macOS; tmpfs gives --remount-ro a real mount point on Linux.
    assert!(
        joined.contains("--tmpfs /var/empty --remount-ro /var/empty")
            && !joined.contains("--dir /var/empty"),
        "{joined}"
    );
    // Real system directories bind read-only; merged-/usr symlinks map via --symlink.
    for dir in ["/usr", "/bin", "/etc"] {
        let Ok(meta) = std::fs::symlink_metadata(dir) else {
            assert!(
                !joined.contains(&format!("--ro-bind {dir} {dir}")),
                "{joined}"
            );
            continue;
        };
        if meta.file_type().is_symlink() && Path::new(dir).is_dir() {
            let target = std::fs::read_link(dir).unwrap();
            assert!(
                joined.contains(&format!("--symlink {} {dir}", target.to_string_lossy()))
                    && !joined.contains(&format!("--ro-bind {dir} {dir}")),
                "{dir}: {joined}"
            );
        } else {
            assert_eq!(
                meta.is_dir(),
                joined.contains(&format!("--ro-bind {dir} {dir}")),
                "{dir}: {joined}"
            );
        }
    }
    assert!(!joined.contains("--ro-bind /nonexistent-zeus-dir"));
    // The user's home must never be exposed.
    if let Some(home) = std::env::var_os("HOME") {
        let home = home.to_string_lossy();
        assert!(
            !joined.contains(&format!("--ro-bind {home} {home}")),
            "{joined}"
        );
    }
}

#[test]
fn bubblewrap_profile_rejects_unencodable_paths() {
    let temp = tempfile::tempdir().unwrap();
    let bad = temp.path().join("has\nnewline");
    assert!(bubblewrap_args(temp.path(), &bad, temp.path(), &[]).is_err());
}

#[test]
fn verify_fails_closed_without_a_supported_sandbox() {
    // On macOS this exercises the real Seatbelt path and must succeed; on any
    // other host without bwrap it must refuse rather than run unsandboxed.
    let temp = tempfile::tempdir().unwrap();
    let launcher = std::env::current_exe().unwrap();
    let result = zeus::process::verify(
        &["/bin/echo".into(), "ok".into()],
        temp.path(),
        5,
        &launcher,
    );
    if cfg!(target_os = "macos") {
        assert!(
            result.is_ok() || result.unwrap_err().contains("launch"),
            "unexpected error"
        );
    } else if cfg!(target_os = "linux") {
        if let Err(error) = result {
            assert!(
                error.contains("sandbox unavailable") || error.contains("launch"),
                "{error}"
            );
        }
    } else {
        assert!(result.unwrap_err().contains("sandbox unavailable"));
    }
}
