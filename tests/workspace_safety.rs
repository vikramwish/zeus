use std::fs;
use workspace::Workspace;
use zeus::workspace;

#[cfg(unix)]
fn assert_exec_stdout(path: &std::path::Path, expected: &[u8]) {
    use std::io::ErrorKind;
    use std::process::Command;

    for attempt in 0..10 {
        match Command::new(path).output() {
            Ok(output) => {
                assert!(output.status.success());
                assert_eq!(output.stdout, expected);
                return;
            }
            Err(err) if err.kind() == ErrorKind::ExecutableFileBusy && attempt < 9 => {
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            Err(err) => panic!("failed running {}: {err}", path.display()),
        }
    }
}

#[cfg(unix)]
#[test]
fn snapshot_preserves_rwx_without_privilege_bits() {
    use std::os::unix::fs::PermissionsExt;

    for mode in [0o751, 0o7751, 0o640, 0o7640] {
        let tmp = tempfile::tempdir().unwrap();
        let source = tmp.path().join("script.sh");
        let content = "#!/bin/sh\nprintf 'before'\n";
        fs::write(&source, content).unwrap();
        fs::set_permissions(&source, fs::Permissions::from_mode(mode)).unwrap();
        assert_eq!(
            fs::metadata(&source).unwrap().permissions().mode() & 0o7777,
            mode
        );
        if mode & 0o100 != 0 {
            assert_exec_stdout(&source, b"before");
        }

        let ws = Workspace::snapshot(&source, &tmp.path().join("candidate")).unwrap();
        let candidate = ws.root.join("script.sh");
        if mode & 0o100 != 0 {
            assert_exec_stdout(&candidate, b"before");
        }
        assert_eq!(
            fs::metadata(&candidate).unwrap().permissions().mode() & 0o7777,
            mode & 0o777
        );
        assert_eq!(
            fs::metadata(&source).unwrap().permissions().mode() & 0o7777,
            mode
        );
        assert_eq!(fs::read_to_string(&source).unwrap(), content);
    }
}

#[cfg(unix)]
#[test]
fn atomic_edit_preserves_rwx_without_privilege_bits() {
    use std::os::unix::fs::PermissionsExt;

    for mode in [0o751, 0o7751, 0o640, 0o7640] {
        let tmp = tempfile::tempdir().unwrap();
        let source = tmp.path().join("script.sh");
        let before = "#!/bin/sh\nprintf 'before'\n";
        fs::write(&source, before).unwrap();
        fs::set_permissions(&source, fs::Permissions::from_mode(0o751)).unwrap();
        let ws = Workspace::snapshot(&source, &tmp.path().join("candidate")).unwrap();
        let candidate = ws.root.join("script.sh");
        // Exercise edits independently of snapshot's permission sanitization.
        fs::set_permissions(&candidate, fs::Permissions::from_mode(mode)).unwrap();
        assert_eq!(
            fs::metadata(&candidate).unwrap().permissions().mode() & 0o7777,
            mode
        );
        if mode & 0o100 != 0 {
            assert_exec_stdout(&candidate, b"before");
        }

        let read = ws.read("script.sh").unwrap();
        let after = "#!/bin/sh\nprintf 'after'\n";
        ws.write("script.sh", after, Some(&read.sha256)).unwrap();
        if mode & 0o100 != 0 {
            assert_exec_stdout(&candidate, b"after");
        }
        assert_eq!(
            fs::metadata(&candidate).unwrap().permissions().mode() & 0o7777,
            mode & 0o777
        );
        assert_eq!(fs::read_to_string(&candidate).unwrap(), after);
        assert_eq!(
            fs::metadata(&source).unwrap().permissions().mode() & 0o7777,
            0o751
        );
        assert_eq!(fs::read_to_string(&source).unwrap(), before);
    }
}

#[test]
fn sensitive_files_and_symlinks_are_excluded_and_cannot_be_written() {
    use std::os::unix::fs::symlink;
    let tmp = tempfile::tempdir().unwrap();
    let source = tmp.path().join("source");
    fs::create_dir(&source).unwrap();
    fs::write(source.join("ok.txt"), "ok").unwrap();
    fs::write(source.join(".env"), "PRIVATE_KEY=not-a-real-secret").unwrap();
    symlink(tmp.path(), source.join("escape")).unwrap();
    let ws = Workspace::snapshot(&source, &tmp.path().join("candidate")).unwrap();
    assert_eq!(ws.inventory.entries.len(), 1);
    assert_eq!(ws.inventory.skipped.len(), 2);
    for bad in [
        "../outside.txt",
        "/outside.txt",
        ".env",
        "nested/.env.production",
    ] {
        assert!(ws.write(bad, "bad", None).is_err(), "{bad}");
    }
    symlink(tmp.path(), ws.root.join("escape")).unwrap();
    assert!(ws.write("escape/outside.txt", "bad", None).is_err());
    assert!(ws.read("escape/outside.txt").is_err());
}

#[test]
fn snapshot_cannot_overwrite_existing_directory_or_live_inside_source() {
    let tmp = tempfile::tempdir().unwrap();
    let source = tmp.path().join("source");
    fs::create_dir(&source).unwrap();
    fs::write(source.join("x"), "x").unwrap();
    assert!(Workspace::snapshot(&source, &source.join("copy")).is_err());
    let existing = tmp.path().join("existing");
    fs::create_dir(&existing).unwrap();
    assert!(Workspace::snapshot(&source, &existing).is_err());
}

#[test]
fn inspect_keeps_unknown_binary_explicit_and_read_rejects_it() {
    let tmp = tempfile::tempdir().unwrap();
    let source = tmp.path().join("opaque.bin");
    fs::write(&source, [0xff, 0, 0x42]).unwrap();
    let ws = Workspace::snapshot(&source, &tmp.path().join("candidate")).unwrap();
    assert_eq!(ws.inventory.entries[0].kind, "binary-unknown");
    assert!(ws.read("opaque.bin").is_err());
}

#[test]
fn snapshot_edit_isolated_from_original_and_requires_current_digest() {
    let tmp = tempfile::tempdir().unwrap();
    let source = tmp.path().join("source");
    fs::create_dir(&source).unwrap();
    fs::write(source.join("main.rs"), "before").unwrap();
    let ws = Workspace::snapshot(&source, &tmp.path().join("candidate")).unwrap();
    let read = ws.read("main.rs").unwrap();
    assert!(ws.write("main.rs", "after", None).is_err());
    ws.write("main.rs", "after", Some(&read.sha256)).unwrap();
    assert_eq!(ws.read("main.rs").unwrap().content, "after");
    assert_eq!(
        fs::read_to_string(source.join("main.rs")).unwrap(),
        "before"
    );
    assert!(ws.write("main.rs", "stale", Some(&read.sha256)).is_err());
}
