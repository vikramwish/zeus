use serde::Serialize;
use std::path::Path;
use std::{
    fs,
    io::Read,
    path::PathBuf,
    process::{Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

/// Verifier isolation backends, selected by host OS and never by the job.
enum Backend {
    /// macOS `sandbox-exec` with a deny-default Seatbelt profile.
    Seatbelt,
    /// Linux bubblewrap: unshared namespaces, read-only system binds, writable candidate.
    Bubblewrap(PathBuf),
}

fn select_backend() -> Result<Backend, String> {
    if cfg!(target_os = "macos") {
        return Ok(Backend::Seatbelt);
    }
    if cfg!(target_os = "linux") {
        for candidate in ["/usr/bin/bwrap", "/usr/local/bin/bwrap", "/bin/bwrap"] {
            if Path::new(candidate).is_file() {
                return Ok(Backend::Bubblewrap(candidate.into()));
            }
        }
        return Err("sandbox unavailable: bubblewrap (bwrap) is not installed; the verifier never runs unsandboxed".into());
    }
    Err("sandbox unavailable: execution is supported only on macOS (Seatbelt) and Linux (bubblewrap)".into())
}

/// bubblewrap arguments mirroring the Seatbelt profile: read-only system
/// directories that exist on this host, the executable, launcher and explicit
/// external files read-only, only the candidate writable, inert home, no
/// network or other inherited namespaces. Pure so it can be tested anywhere.
pub fn bubblewrap_args(
    root: &Path,
    executable: &Path,
    launcher: &Path,
    files: &[PathBuf],
) -> Result<Vec<String>, String> {
    fn plain(path: &Path) -> Result<String, String> {
        let s = path.to_str().ok_or("non-UTF8 sandbox path")?;
        if s.contains(['\n', '\r', '\0']) || s.is_empty() || !path.is_absolute() {
            return Err("invalid sandbox path".into());
        }
        Ok(s.to_owned())
    }
    let mut args: Vec<String> = ["--unshare-all", "--die-with-parent", "--new-session"]
        .map(String::from)
        .to_vec();
    args.extend(["--dev", "/dev", "--proc", "/proc"].map(String::from));
    for dir in [
        "/usr", "/lib", "/lib64", "/lib32", "/bin", "/sbin", "/etc", "/opt", "/nix",
    ] {
        if Path::new(dir).is_dir() {
            args.extend(["--ro-bind", dir, dir].map(String::from));
        }
    }
    for file in [executable, launcher]
        .into_iter()
        .chain(files.iter().map(PathBuf::as_path))
    {
        let p = plain(file)?;
        args.extend(["--ro-bind".into(), p.clone(), p]);
    }
    let root = plain(root)?;
    args.extend(["--bind".into(), root.clone(), root.clone()]);
    args.extend(["--chdir".into(), root]);
    args.extend(["--dir", "/var/empty", "--remount-ro", "/var/empty"].map(String::from));
    Ok(args)
}

#[derive(Debug, Serialize)]
pub struct ProcessResult {
    pub success: bool,
    pub stdout: String,
    pub stderr: String,
    pub exit_code: Option<i32>,
    pub timed_out: bool,
    pub output_limit: bool,
}
fn literal(path: &Path) -> Result<String, String> {
    let s = path.to_str().ok_or("non-UTF8 sandbox path")?;
    if s.contains(['\n', '\r', '\0']) {
        return Err("invalid sandbox path".into());
    }
    Ok(format!(
        "\"{}\"",
        s.replace('\\', "\\\\").replace('"', "\\\"")
    ))
}
fn capture(
    mut reader: impl Read + std::os::fd::AsRawFd + Send + 'static,
    over: Arc<AtomicBool>,
    expired: Arc<AtomicBool>,
    deadline: Instant,
) -> std::thread::JoinHandle<std::io::Result<Vec<u8>>> {
    std::thread::spawn(move || {
        let mut result = Vec::new();
        let mut buf = [0; 8192];
        loop {
            let now = Instant::now();
            if now >= deadline {
                expired.store(true, Ordering::Relaxed);
                break;
            }
            // A daemonized descendant can retain a pipe after its leader exits.
            // Poll before reading so joining collectors also respects the deadline.
            let mut pollfd = libc::pollfd {
                fd: reader.as_raw_fd(),
                events: libc::POLLIN,
                revents: 0,
            };
            let millis = (deadline - now).as_millis().clamp(1, 100) as i32;
            let ready = unsafe { libc::poll(&mut pollfd, 1, millis) };
            if ready < 0 {
                let error = std::io::Error::last_os_error();
                if error.kind() == std::io::ErrorKind::Interrupted {
                    continue;
                }
                return Err(error);
            }
            if ready == 0 {
                continue;
            }
            match reader.read(&mut buf) {
                Ok(0) => break,
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(error) => return Err(error),
                Ok(n) => {
                    let remaining = (128 * 1024usize).saturating_sub(result.len());
                    result.extend_from_slice(&buf[..n.min(remaining)]);
                    if n > remaining {
                        over.store(true, Ordering::Relaxed);
                        break;
                    }
                }
            }
        }
        Ok(result)
    })
}

/// Run only user-selected verifier argv. macOS Seatbelt is a required boundary;
/// unsupported hosts fail closed rather than silently running unsandboxed.
/// `launcher` must be the trusted Zeus executable, selected by the host (never
/// by the candidate/model). Its native exec helper runs inside Seatbelt and
/// reports launch errors over a private close-on-exec channel.
pub fn verify(
    argv: &[String],
    cwd: &Path,
    timeout_secs: u64,
    launcher: &Path,
) -> Result<ProcessResult, String> {
    if argv.is_empty()
        || argv.len() > 128
        || argv.iter().any(|a| a.len() > 32768 || a.contains('\0'))
    {
        return Err("invalid verifier argv".into());
    }
    if !(1..=120).contains(&timeout_secs) {
        return Err("verifier timeout must be 1..120 seconds".into());
    }
    let backend = select_backend()?;
    verify_sandboxed(argv, cwd, timeout_secs, launcher, backend)
}

fn verify_sandboxed(
    argv: &[String],
    cwd: &Path,
    timeout_secs: u64,
    launcher: &Path,
    backend: Backend,
) -> Result<ProcessResult, String> {
    use std::os::{fd::AsRawFd, unix::net::UnixStream};
    let root = fs::canonicalize(cwd).map_err(|e| e.to_string())?;
    let launcher = fs::canonicalize(launcher).map_err(|e| format!("verifier launcher: {e}"))?;
    if launcher.starts_with(&root) {
        return Err("verifier launcher must be outside candidate".into());
    }
    let executable = Path::new(&argv[0]);
    if !executable.is_absolute() {
        return Err("verifier executable must be absolute".into());
    }
    let executable = fs::canonicalize(executable).map_err(|e| e.to_string())?;
    if executable.starts_with(&root) {
        return Err("verifier executable must be outside candidate".into());
    }
    let tmp = root.join(".native-tmp");
    if let Ok(meta) = fs::symlink_metadata(&tmp) {
        if !meta.is_dir() || meta.file_type().is_symlink() {
            return Err("unsafe verifier scratch path".into());
        }
    }
    fs::create_dir_all(&tmp).map_err(|e| e.to_string())?;
    // Explicit external regular-file arguments (e.g. immutable check.py) can be read, not modified.
    let mut external_files = Vec::new();
    for arg in &argv[1..] {
        let path = PathBuf::from(arg);
        if path.is_absolute() && path.is_file() {
            external_files.push(fs::canonicalize(path).map_err(|e| e.to_string())?);
        }
    }
    let mut command = match &backend {
        Backend::Bubblewrap(bwrap) => {
            let mut command = Command::new(bwrap);
            command.args(bubblewrap_args(
                &root,
                &executable,
                &launcher,
                &external_files,
            )?);
            command
        }
        Backend::Seatbelt => {
            let mut command = Command::new("/usr/bin/sandbox-exec");
            command.args([
                "-p",
                &seatbelt_profile(&root, &executable, &launcher, &external_files)?,
            ]);
            command
        }
    };
    // Private launch-status channel, not stdout/stderr or a candidate file.
    // The native helper closes this descriptor on successful exec. The target
    // and all of its descendants therefore cannot forge launch diagnostics.
    let (launch_reader, launch_writer) = UnixStream::pair().map_err(|e| e.to_string())?;
    let launch_fd = launch_writer.as_raw_fd();
    command
        .arg(&launcher)
        .arg("--internal-verifier-exec")
        .arg(launch_fd.to_string())
        .arg(&executable)
        .args(&argv[1..])
        .current_dir(&root)
        .env_clear()
        .env("PATH", "/usr/bin:/bin:/usr/sbin:/sbin:/opt/homebrew/bin")
        // Never use a writable candidate directory for interpreter startup.
        // No read/write sandbox grant is added for this inert system home.
        .env("HOME", "/var/empty")
        .env("TMPDIR", &tmp)
        .env("PYTHONDONTWRITEBYTECODE", "1")
        // Disable Python user-site startup explicitly. Also keep cwd
        // off Python's implicit import path (3.11+); explicit trusted wrappers
        // remain responsible for imports and other interpreter-specific flags.
        .env("PYTHONNOUSERSITE", "1")
        .env("PYTHONSAFEPATH", "1")
        .env("LANG", "en_US.UTF-8")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    supervise(
        command,
        launch_fd,
        launch_reader,
        launch_writer,
        timeout_secs,
    )
}

fn seatbelt_profile(
    root: &Path,
    executable: &Path,
    launcher: &Path,
    external_files: &[PathBuf],
) -> Result<String, String> {
    let mut profile = String::from("(version 1)(deny default)(allow process*)(allow sysctl-read)(allow mach-lookup)(allow file-read-metadata)");
    // System executables need root-directory reads at startup. Literal means only
    // that directory, not its descendants (in particular not users' homes).
    profile.push_str("(allow file-read-data (literal \"/\"))");
    for path in [
        "/System",
        "/Library",
        "/usr",
        "/bin",
        "/sbin",
        "/opt/homebrew",
        "/private/etc",
        "/private/var/db/dyld",
    ] {
        profile.push_str(&format!(
            "(allow file-read* (subpath {}))",
            literal(Path::new(path))?
        ));
    }
    for path in ["/dev/null", "/dev/random", "/dev/urandom"] {
        profile.push_str(&format!(
            "(allow file-read* (literal {}))",
            literal(Path::new(path))?
        ));
    }
    profile.push_str("(allow file-write* (literal \"/dev/null\"))");
    profile.push_str(&format!(
        "(allow file-read* file-write* (subpath {}))",
        literal(root)?
    ));
    profile.push_str(&format!(
        "(allow file-read* (literal {}))",
        literal(executable)?
    ));
    profile.push_str(&format!(
        "(allow file-read* (literal {}))",
        literal(launcher)?
    ));
    for path in external_files {
        profile.push_str(&format!("(allow file-read* (literal {}))", literal(path)?));
    }
    Ok(profile)
}

#[cfg(unix)]
unsafe fn ensure_cloexec(fd: i32) -> std::io::Result<()> {
    let flags = libc::fcntl(fd, libc::F_GETFD);
    if flags != -1
        && (flags & libc::FD_CLOEXEC) == 0
        && libc::fcntl(fd, libc::F_SETFD, flags | libc::FD_CLOEXEC) == -1
    {
        return Err(std::io::Error::last_os_error());
    }
    Ok(())
}

#[cfg(unix)]
fn non_cloexec_fds(launch_fd: i32) -> Vec<i32> {
    let mut fds = Vec::new();
    for dir in ["/dev/fd", "/proc/self/fd"] {
        let Ok(entries) = fs::read_dir(dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let Some(fd) = entry
                .file_name()
                .to_str()
                .and_then(|name| name.parse::<i32>().ok())
            else {
                continue;
            };
            if fd < 3 || fd == launch_fd {
                continue;
            }
            let flags = unsafe { libc::fcntl(fd, libc::F_GETFD) };
            if flags != -1 && (flags & libc::FD_CLOEXEC) == 0 {
                fds.push(fd);
            }
        }
        break;
    }
    fds
}

/// Shared supervision for every backend: launch handshake, output capture
/// with deadline and size bounds, CPU/file/descriptor limits, process-group
/// cleanup, and cancellation.
fn supervise(
    mut command: Command,
    launch_fd: i32,
    launch_reader: std::os::unix::net::UnixStream,
    launch_writer: std::os::unix::net::UnixStream,
    timeout_secs: u64,
) -> Result<ProcessResult, String> {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        let extra_fds = non_cloexec_fds(launch_fd);
        unsafe {
            command.pre_exec(move || {
                // Only this child inherits the channel through sandbox-exec;
                // the helper restores CLOEXEC before executing verifier code.
                if libc::fcntl(launch_fd, libc::F_SETFD, 0) == -1 {
                    return Err(std::io::Error::last_os_error());
                }
                // Prevent parent-inherited descriptors (e.g. IDE/CI sockets)
                // from leaking into the sandbox or verifier child on exec.
                for &fd in extra_fds.iter() {
                    ensure_cloexec(fd)?;
                }
                for fd in 3..256 {
                    if fd != launch_fd {
                        ensure_cloexec(fd)?;
                    }
                }
                if libc::setpgid(0, 0) != 0 {
                    return Err(std::io::Error::last_os_error());
                }
                let cpu = libc::rlimit {
                    rlim_cur: timeout_secs + 1,
                    rlim_max: timeout_secs + 1,
                };
                let file = libc::rlimit {
                    rlim_cur: 16 * 1024 * 1024,
                    rlim_max: 16 * 1024 * 1024,
                };
                let descriptors = libc::rlimit {
                    rlim_cur: 128,
                    rlim_max: 128,
                };
                for (resource, limit) in [
                    (libc::RLIMIT_CPU, cpu),
                    (libc::RLIMIT_FSIZE, file),
                    (libc::RLIMIT_NOFILE, descriptors),
                ] {
                    if libc::setrlimit(resource, &limit) != 0 {
                        return Err(std::io::Error::last_os_error());
                    }
                }
                Ok(())
            });
        }
    }
    let start = Instant::now();
    let deadline = start + Duration::from_secs(timeout_secs);
    let mut child = command
        .spawn()
        .map_err(|e| format!("verifier spawn failed: {e}"))?;
    drop(launch_writer);
    let pid = child.id() as i32;
    let over = Arc::new(AtomicBool::new(false));
    let expired = Arc::new(AtomicBool::new(false));
    let launch = capture(launch_reader, over.clone(), expired.clone(), deadline);
    let stdout = capture(
        child.stdout.take().ok_or("missing stdout")?,
        over.clone(),
        expired.clone(),
        deadline,
    );
    let stderr = capture(
        child.stderr.take().ok_or("missing stderr")?,
        over.clone(),
        expired.clone(),
        deadline,
    );
    let mut timed_out = false;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Ok(status),
            Err(error) => break Err(error),
            Ok(None) => (),
        }
        let cancelled = crate::cancel::cancelled();
        if start.elapsed() >= Duration::from_secs(timeout_secs)
            || over.load(Ordering::Relaxed)
            || cancelled
        {
            timed_out = start.elapsed() >= Duration::from_secs(timeout_secs);
            #[cfg(unix)]
            unsafe {
                libc::kill(-pid, libc::SIGKILL);
            }
            let _ = child.kill();
            break child.wait();
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    // Kill same-group descendants even on success. setsid/setpgid escape this
    // cleanup: this is not a general hostile-process containment facility.
    #[cfg(unix)]
    unsafe {
        libc::kill(-pid, libc::SIGKILL);
    }
    let out = stdout.join();
    let err = stderr.join();
    let launch = launch
        .join()
        .map_err(|_| "verifier launch collector panicked")?
        .map_err(|e| format!("verifier launch channel failed: {e}"))?;
    let out = out
        .map_err(|_| "stdout collector panicked")?
        .map_err(|e| e.to_string())?;
    let err = err
        .map_err(|_| "stderr collector panicked")?
        .map_err(|e| e.to_string())?;
    timed_out |= expired.load(Ordering::Relaxed);
    crate::cancel::check()?;
    let status = status.map_err(|e| e.to_string())?;
    let output_limit = over.load(Ordering::Relaxed);
    if launch != b"R" {
        return Err(format!(
            "verifier launch failed: {}; {}",
            String::from_utf8_lossy(&launch),
            String::from_utf8_lossy(&err)
        ));
    }
    Ok(ProcessResult {
        success: status.success() && !timed_out && !output_limit,
        stdout: String::from_utf8_lossy(&out).into_owned(),
        stderr: String::from_utf8_lossy(&err).into_owned(),
        exit_code: status.code(),
        timed_out,
        output_limit,
    })
}
