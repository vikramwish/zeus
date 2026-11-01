use zeus::process;

fn verify(
    argv: &[String],
    cwd: &std::path::Path,
    timeout: u64,
) -> Result<process::ProcessResult, String> {
    process::verify(
        argv,
        cwd,
        timeout,
        std::path::Path::new(env!("CARGO_BIN_EXE_zeus")),
    )
}

#[cfg(target_os = "macos")]
mod macos {
    use super::{process, verify};
    use std::os::unix::fs::{symlink, PermissionsExt};
    use std::{
        fs,
        path::Path,
        time::{Duration, Instant},
    };

    fn candidate() -> tempfile::TempDir {
        let scratch = std::path::PathBuf::from(std::env::var_os("HOME").unwrap())
            .join(".hermes/cache/scratch");
        fs::create_dir_all(&scratch).unwrap();
        tempfile::tempdir_in(scratch).unwrap()
    }

    fn shell(root: &Path, script: &str, seconds: u64) -> process::ProcessResult {
        verify(
            &["/bin/sh".into(), "-c".into(), script.into()],
            root,
            seconds,
        )
        .unwrap()
    }

    fn quote(path: &Path) -> String {
        format!("'{}'", path.to_str().unwrap().replace('\'', "'\\''"))
    }

    #[test]
    fn verifier_executes_in_candidate_with_captured_output() {
        let temp = candidate();
        fs::write(temp.path().join("answer.txt"), "42").unwrap();
        let result = verify(&["/bin/cat".into(), "answer.txt".into()], temp.path(), 3).unwrap();
        assert!(result.success, "{result:?}");
        assert_eq!(result.stdout, "42");
        assert_eq!(result.exit_code, Some(0));
    }

    #[test]
    fn external_writes_and_symlink_writes_are_denied() {
        let temp = candidate();
        let outside = candidate();
        let protected = outside.path().join("protected");
        fs::write(&protected, "unchanged").unwrap();
        symlink(&protected, temp.path().join("escape")).unwrap();
        let ok = shell(temp.path(), "printf local > local; cat local", 3);
        assert!(ok.success, "{ok:?}");
        for target in [
            quote(&protected),
            "escape".into(),
            quote(&outside.path().join("new")),
        ] {
            let result = shell(temp.path(), &format!("printf changed > {target}"), 3);
            assert!(!result.success, "{result:?}");
            assert!(
                result.stderr.contains("Operation not permitted"),
                "{result:?}"
            );
        }
        assert_eq!(fs::read_to_string(protected).unwrap(), "unchanged");
        assert!(!outside.path().join("new").exists());
    }

    #[test]
    fn unselected_home_files_cannot_be_read_even_through_symlinks() {
        let temp = candidate();
        let outside = candidate();
        let secret = outside.path().join("secret");
        fs::write(&secret, "private-value").unwrap();
        symlink(&secret, temp.path().join("escape")).unwrap();
        // Embed path in shell code, not an explicitly selected file argument.
        for target in [quote(&secret), "escape".into()] {
            let result = shell(temp.path(), &format!("cat {target}"), 3);
            assert!(!result.success, "{result:?}");
            assert!(
                result.stderr.contains("Operation not permitted"),
                "{result:?}"
            );
            assert!(!result.stdout.contains("private-value"));
        }
    }

    #[test]
    fn network_connection_to_live_loopback_listener_is_denied() {
        let temp = candidate();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let port = listener.local_addr().unwrap().port();
        let result = verify(
            &[
                "/usr/bin/curl".into(),
                "--noproxy".into(),
                "*".into(),
                "--connect-timeout".into(),
                "1".into(),
                "--max-time".into(),
                "2".into(),
                format!("http://127.0.0.1:{port}/"),
            ],
            temp.path(),
            3,
        )
        .unwrap();
        assert!(!result.success && !result.timed_out, "{result:?}");
        assert_eq!(result.exit_code, Some(7), "{result:?}");
        assert_eq!(
            listener.accept().unwrap_err().kind(),
            std::io::ErrorKind::WouldBlock
        );
    }

    #[test]
    fn timeout_kills_verifier_promptly() {
        let temp = candidate();
        let start = Instant::now();
        let result = shell(temp.path(), "exec /bin/sleep 30", 1);
        assert!(result.timed_out && !result.success, "{result:?}");
        assert!(start.elapsed() < Duration::from_secs(4));
    }

    #[test]
    fn stdout_and_stderr_are_bounded() {
        for redirect in ["", " >&2"] {
            let temp = candidate();
            let start = Instant::now();
            let result = shell(temp.path(), &format!("exec /usr/bin/yes{redirect}"), 3);
            assert!(
                result.output_limit && !result.success && !result.timed_out,
                "{result:?}"
            );
            assert!(result.stdout.len() <= 128 * 1024);
            assert!(result.stderr.len() <= 128 * 1024);
            assert!(start.elapsed() < Duration::from_secs(3));
        }
    }

    #[test]
    fn children_are_cleaned_up_after_success_and_timeout() {
        for wait in [false, true] {
            let temp = candidate();
            let script = format!(
                "(sleep 30; printf escaped > survived) & echo $! > child.pid; {}",
                if wait { "wait" } else { "exit 0" }
            );
            let start = Instant::now();
            // Leave room for sandbox/helper startup under concurrent test load.
            // The child outlives this deadline, so ESRCH below proves cleanup,
            // rather than merely observing a naturally completed child.
            let result = shell(temp.path(), &script, 5);
            assert_eq!(result.timed_out, wait, "{result:?}");
            assert_eq!(result.success, !wait, "{result:?}");
            assert!(start.elapsed() < Duration::from_secs(8));
            let pid: i32 = fs::read_to_string(temp.path().join("child.pid"))
                .unwrap()
                .trim()
                .parse()
                .unwrap();
            // Reaping an orphan is asynchronous; poll a bounded interval rather
            // than assuming a fixed sleep is enough on every loaded machine.
            let reaped = Instant::now() + Duration::from_secs(3);
            while unsafe { libc::kill(pid, 0) } == 0 && Instant::now() < reaped {
                std::thread::sleep(Duration::from_millis(10));
            }
            assert_eq!(unsafe { libc::kill(pid, 0) }, -1, "child {pid} survived");
            assert_eq!(
                std::io::Error::last_os_error().raw_os_error(),
                Some(libc::ESRCH)
            );
            assert!(!temp.path().join("survived").exists());
        }
    }

    #[test]
    fn detached_pipe_holder_cannot_extend_verifier_deadline() {
        let temp = candidate();
        let start = Instant::now();
        let result = verify(&["/usr/bin/perl".into(), "-MPOSIX".into(), "-e".into(),
            "if (fork() == 0) { POSIX::setsid() >= 0 or die; open(my $f, '>', 'detached.pid') or die; print $f $$; close($f); sleep 3; exit 0; } while (!-e 'detached.pid') { select(undef, undef, undef, 0.01); } exit 0;".into()], temp.path(), 1).unwrap();
        let elapsed = start.elapsed();
        let pid: i32 = fs::read_to_string(temp.path().join("detached.pid"))
            .unwrap()
            .trim()
            .parse()
            .unwrap();
        // A detached child deliberately escapes group cleanup; never leak the fixture.
        unsafe {
            libc::kill(pid, libc::SIGKILL);
        }
        assert!(
            elapsed < Duration::from_secs(2),
            "pipe drain exceeded deadline: {elapsed:?}"
        );
        assert!(result.timed_out && !result.success, "{result:?}");
    }

    #[test]
    fn executable_must_be_external_including_symlink_aliases() {
        let temp = candidate();
        let outside = candidate();
        let executable = temp.path().join("verifier");
        fs::write(&executable, "#!/bin/sh\nexit 0\n").unwrap();
        fs::set_permissions(&executable, fs::Permissions::from_mode(0o755)).unwrap();
        let alias = outside.path().join("alias");
        symlink(&executable, &alias).unwrap();
        for path in [&executable, &alias] {
            let err = verify(&[path.to_str().unwrap().into()], temp.path(), 3).unwrap_err();
            assert!(err.contains("outside candidate"), "{err}");
        }
        let external = outside.path().join("trusted-verifier");
        fs::write(&external, "#!/bin/sh\nprintf external\n").unwrap();
        fs::set_permissions(&external, fs::Permissions::from_mode(0o755)).unwrap();
        let result = verify(&[external.to_str().unwrap().into()], temp.path(), 3).unwrap();
        assert!(result.success, "{result:?}");
        assert_eq!(result.stdout, "external");
    }

    #[test]
    fn explicitly_selected_external_script_is_read_only() {
        let temp = candidate();
        let outside = candidate();
        let script = outside.path().join("check.sh");
        let contents = "printf selected; printf tampered >> \"$0\"\n";
        fs::write(&script, contents).unwrap();
        let result = verify(
            &["/bin/sh".into(), script.to_str().unwrap().into()],
            temp.path(),
            3,
        )
        .unwrap();
        assert_eq!(result.stdout, "selected", "{result:?}");
        assert!(!result.success, "{result:?}");
        assert!(
            result.stderr.contains("Operation not permitted"),
            "{result:?}"
        );
        assert_eq!(fs::read_to_string(script).unwrap(), contents);
    }

    #[test]
    fn verifier_preserves_default_sigpipe_behavior() {
        let temp = candidate();
        let result = shell(temp.path(), "kill -PIPE $$; printf survived", 10);
        assert_eq!(result.exit_code, None, "{result:?}");
        assert!(!result.success);
        assert!(result.stdout.is_empty());
    }

    #[test]
    fn verifier_does_not_inherit_private_launch_channel() {
        let temp = candidate();
        let result = verify(&["/opt/homebrew/bin/python3".into(), "-I".into(), "-c".into(),
            "import os, stat\nfor fd in range(3, 128):\n try: mode = os.fstat(fd).st_mode\n except OSError: continue\n assert not stat.S_ISSOCK(mode), ('inherited launch socket', fd)\nprint('channel closed')\n".into()], temp.path(), 10).unwrap();
        assert!(result.success, "{result:?}");
        assert_eq!(result.stdout.trim(), "channel closed");
    }

    #[test]
    fn launcher_without_exec_handshake_is_never_a_test_outcome() {
        let temp = candidate();
        for launcher in ["/usr/bin/true", "/usr/bin/false"] {
            let error = process::verify(
                &["/usr/bin/true".into()],
                temp.path(),
                10,
                Path::new(launcher),
            )
            .unwrap_err();
            assert!(error.contains("launch failed"), "{error}");
        }
    }

    #[test]
    fn kernel_exec_errors_are_not_test_results() {
        let temp = candidate();
        let outside = candidate();
        let executable = outside.path().join("verifier");
        for (body, mode) in [
            ("#!/bin/sh\nexit 0\n", 0o600),
            ("not an executable format\n", 0o700),
            ("#!/definitely-missing-zeus-interpreter\n", 0o700),
        ] {
            fs::write(&executable, body).unwrap();
            fs::set_permissions(&executable, fs::Permissions::from_mode(mode)).unwrap();
            let error = verify(&[executable.display().to_string()], temp.path(), 10).unwrap_err();
            assert!(error.contains("exec failed"), "{error}");
        }
    }

    #[test]
    fn candidate_home_cannot_supply_shell_startup_code() {
        let temp = candidate();
        let outside = candidate();
        fs::create_dir(temp.path().join(".native-home")).unwrap();
        fs::write(temp.path().join(".native-home/.zshenv"), "exit 0\n").unwrap();
        let script = outside.path().join("check.zsh");
        fs::write(&script, "print EXTERNAL_ASSERTION_REACHED\nexit 1\n").unwrap();
        let result = verify(
            &["/bin/zsh".into(), script.display().to_string()],
            temp.path(),
            10,
        )
        .unwrap();
        assert_eq!(result.exit_code, Some(1), "{result:?}");
        assert!(result.stdout.contains("EXTERNAL_ASSERTION_REACHED"));
    }

    #[test]
    fn environment_is_cleared_with_inert_home_and_candidate_tmp() {
        let temp = candidate();
        let result = verify(&["/usr/bin/env".into()], temp.path(), 3).unwrap();
        assert!(result.success, "{result:?}");
        let root = fs::canonicalize(temp.path()).unwrap();
        let expected = [
            "PATH=/usr/bin:/bin:/usr/sbin:/sbin:/opt/homebrew/bin".to_string(),
            "HOME=/var/empty".into(),
            format!("TMPDIR={}", root.join(".native-tmp").display()),
            "PYTHONDONTWRITEBYTECODE=1".into(),
            "PYTHONNOUSERSITE=1".into(),
            "PYTHONSAFEPATH=1".into(),
            "LANG=en_US.UTF-8".into(),
        ];
        let actual: std::collections::BTreeSet<_> = result.stdout.lines().collect();
        assert_eq!(actual, expected.iter().map(String::as_str).collect());
    }

    #[test]
    fn scratch_symlinks_are_rejected_before_execution() {
        let temp = candidate();
        let outside = candidate();
        symlink(outside.path(), temp.path().join(".native-tmp")).unwrap();
        let err = verify(&["/bin/echo".into()], temp.path(), 3).unwrap_err();
        assert!(err.contains("unsafe verifier scratch path"), "{err}");
    }
}

#[cfg(not(target_os = "macos"))]
#[test]
fn unsupported_platform_fails_closed() {
    let err = verify(&["/bin/echo".into()], std::path::Path::new("/"), 1).unwrap_err();
    assert!(err.contains("sandbox unavailable"));
}
