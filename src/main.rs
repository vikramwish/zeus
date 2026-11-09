use std::path::Path;

// Only the host process launcher invokes this path, after sandbox-exec has
// installed Seatbelt. It is not an agent tool or an alternate job/approval path.
// A private inherited channel distinguishes kernel exec errors from test exits.
#[cfg(unix)]
fn verifier_exec(fd: &str, argv: &[String]) -> Result<(), String> {
    use std::{ffi::CString, io::Write, os::fd::FromRawFd};
    let fd: i32 = fd.parse().map_err(|_| "invalid launch descriptor")?;
    if fd < 3 || argv.is_empty() || !Path::new(&argv[0]).is_absolute() {
        return Err("invalid internal verifier invocation".into());
    }
    // Validate before taking ownership: a normal CLI invocation cannot supply
    // an absent descriptor or redirect the protocol onto standard output.
    let mut stat = std::mem::MaybeUninit::<libc::stat>::uninit();
    unsafe {
        if libc::fstat(fd, stat.as_mut_ptr()) != 0
            || stat.assume_init().st_mode & libc::S_IFMT != libc::S_IFSOCK
            || libc::fcntl(fd, libc::F_SETFD, libc::FD_CLOEXEC) == -1
        {
            return Err("invalid private verifier launch channel".into());
        }
    }
    let mut channel = unsafe { std::fs::File::from_raw_fd(fd) };
    let args = argv
        .iter()
        .map(|arg| CString::new(arg.as_str()))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    let mut pointers: Vec<_> = args.iter().map(|arg| arg.as_ptr()).collect();
    pointers.push(std::ptr::null());
    // Rust's startup ignores SIGPIPE; preserve normal Command/exec semantics
    // rather than leaking the helper runtime's signal disposition to tests.
    if unsafe { libc::signal(libc::SIGPIPE, libc::SIG_DFL) } == libc::SIG_ERR {
        return Err(std::io::Error::last_os_error().to_string());
    }
    channel.write_all(b"R").map_err(|e| e.to_string())?;
    // No interpreter fallback: only a successful kernel exec closes the
    // channel without an error. No target code ever inherits this descriptor.
    unsafe {
        libc::execv(args[0].as_ptr(), pointers.as_ptr());
    }
    let error = std::io::Error::last_os_error();
    let _ = write!(channel, "exec failed: {error}");
    Err(format!("verifier exec failed: {error}"))
}

const USAGE: &str = "Usage:
  zeus inspect <path>
  zeus run <job.json> [--allow-network] [--allow-exec] [--allow-mcp]
  zeus console <job.json> [--allow-network] [--allow-exec] [--allow-mcp]
  zeus sessions list <state-dir>
  zeus sessions search <state-dir> <query>
  zeus sessions show <state-dir> <run-id>
  zeus auth login chatgpt [--new-account]
  zeus auth status chatgpt
  zeus auth logout chatgpt
  zeus models chatgpt

Subscription login opens your browser. Models lists your account's available models.
API keys/local models use job protocol, endpoint, model and optional api_key_env.
No automatic subscription-to-paid-API fallback.";

fn print_json(value: &impl serde::Serialize) -> Result<(), String> {
    println!(
        "{}",
        serde_json::to_string_pretty(value).map_err(|e| e.to_string())?
    );
    Ok(())
}

fn auth_command(action: &str, new_account: bool) -> Result<(), String> {
    use zeus::auth::{self, AuthStore, LoginMode};
    let store = AuthStore::new().map_err(|e| e.to_string())?;
    match action {
        "status" => print_json(&auth::status(&store).map_err(|e| e.to_string())?),
        "logout" => {
            auth::logout(&store).map_err(|e| e.to_string())?;
            print_json(&auth::status(&store).map_err(|e| e.to_string())?)
        }
        "login" => {
            let mode = if new_account {
                LoginMode::NewAccount
            } else {
                LoginMode::Continue
            };
            eprintln!("Continue with ChatGPT in your browser; waiting up to five minutes for authorization.");
            let status = auth::login(&store, mode, |url| {
                #[cfg(target_os = "macos")]
                let opener = "/usr/bin/open";
                #[cfg(not(target_os = "macos"))]
                let opener = "xdg-open";
                let status = std::process::Command::new(opener)
                    .arg(url)
                    .stdin(std::process::Stdio::null())
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .status()
                    .map_err(|_| auth::AuthError::Callback)?;
                if status.success() {
                    Ok(())
                } else {
                    Err(auth::AuthError::Callback)
                }
            })
            .map_err(|e| e.to_string())?;
            print_json(&status)
        }
        _ => Err(USAGE.into()),
    }
}

fn run_job(job: zeus::config::Job) -> Result<(), String> {
    let report = zeus::runtime::run(job)?;
    print_json(&report)?;
    match report["status"].as_str() {
        Some("verified" | "already_verified") => Ok(()),
        Some("unverified") => {
            Err("analysis finished without an external verifier; candidate is unverified".into())
        }
        _ => Err("verification did not pass; see retained report".into()),
    }
}

const CONSOLE_HELP: &str = "Independent task console, not transcript resume. Each task snapshots the original source again.\nEnter a task, or /run to run the configured task.\n/help | /list | /search <query> | /show <exact-run-id> | /quit\nApprovals and job settings are fixed for this console invocation; no pending calls are replayed.";

fn console(job: zeus::config::Job) -> Result<(), String> {
    use std::io::{BufRead, Read, Write};
    eprintln!("{CONSOLE_HELP}");
    let stdin = std::io::stdin();
    let mut input = stdin.lock();
    let mut failed = false;
    loop {
        eprint!("zeus> ");
        std::io::stderr().flush().map_err(|e| e.to_string())?;
        let mut bytes = Vec::new();
        let count = input
            .by_ref()
            .take(32771)
            .read_until(b'\n', &mut bytes)
            .map_err(|e| e.to_string())?;
        if count == 0 {
            break;
        }
        if bytes.len() > 32770 {
            return Err("console line exceeds 32768 task bytes".into());
        }
        let line = String::from_utf8(bytes).map_err(|_| "console input must be UTF-8")?;
        let line = line.trim_end_matches(['\r', '\n']);
        if line.len() > 32768 {
            return Err("console line exceeds 32768 task bytes".into());
        }
        let command = line.trim();
        if command.is_empty() {
            continue;
        }
        let result = match command {
            "/quit" => break,
            "/help" => {
                eprintln!("{CONSOLE_HELP}");
                Ok(())
            }
            "/list" => zeus::session::list(&job.state_dir, None).and_then(|v| print_json(&v)),
            "/run" => run_job(job.clone()),
            text if text.starts_with("/search ") => {
                zeus::session::list(&job.state_dir, Some(text[8..].trim()))
                    .and_then(|v| print_json(&v))
            }
            text if text.starts_with("/show ") => {
                zeus::session::show(&job.state_dir, text[6..].trim()).and_then(|v| print_json(&v))
            }
            text if text.starts_with('/') => {
                Err("unknown console command; use /help (no resume or shell commands)".into())
            }
            _ => {
                let mut next = job.clone();
                next.task = line.to_owned();
                run_job(next)
            }
        };
        if let Err(error) = result {
            eprintln!("zeus: {error}");
            failed = true;
        }
        // A signal cancels the current task only; the console stays usable.
        zeus::cancel::reset();
    }
    if failed {
        Err("one or more console tasks or commands failed or remained unverified".into())
    } else {
        Ok(())
    }
}

fn execute() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.as_slice() {
        [flag] if flag == "--help" || flag == "-h" => {
            println!("{USAGE}");
            Ok(())
        }
        [command, action, provider]
            if command == "auth"
                && provider == "chatgpt"
                && ["login", "status", "logout"].contains(&action.as_str()) =>
        {
            auth_command(action, false)
        }
        [command, action, provider, flag]
            if command == "auth"
                && action == "login"
                && provider == "chatgpt"
                && flag == "--new-account" =>
        {
            auth_command(action, true)
        }
        [command, provider] if command == "models" && provider == "chatgpt" => {
            let store = zeus::auth::AuthStore::new().map_err(|e| e.to_string())?;
            print_json(&zeus::auth::list_models(&store).map_err(|e| e.to_string())?)
        }
        #[cfg(unix)]
        [command, fd, argv @ ..] if command == "--internal-verifier-exec" => {
            verifier_exec(fd, argv)
        }
        [command, path] if command == "inspect" => {
            let inventory = zeus::workspace::inspect(Path::new(path))?;
            println!(
                "{}",
                serde_json::to_string_pretty(&inventory).map_err(|e| e.to_string())?
            );
            Ok(())
        }
        [command, action, state] if command == "sessions" && action == "list" => {
            print_json(&zeus::session::list(Path::new(state), None)?)
        }
        [command, action, state, query] if command == "sessions" && action == "search" => {
            print_json(&zeus::session::list(Path::new(state), Some(query))?)
        }
        [command, action, state, id] if command == "sessions" && action == "show" => {
            print_json(&zeus::session::show(Path::new(state), id)?)
        }
        [command, path, flags @ ..] if command == "run" || command == "console" => {
            let approval = zeus::config::Approvals {
                network: flags.iter().any(|f| f == "--allow-network"),
                exec: flags.iter().any(|f| f == "--allow-exec"),
                mcp: flags.iter().any(|f| f == "--allow-mcp"),
            };
            if flags
                .iter()
                .any(|f| !["--allow-network", "--allow-exec", "--allow-mcp"].contains(&f.as_str()))
            {
                return Err("unknown run flag".into());
            }
            let job = zeus::config::Job::load(Path::new(path), approval)?;
            zeus::cancel::install()?;
            if command == "console" {
                console(job)
            } else {
                run_job(job)
            }
        }
        _ => Err(USAGE.into()),
    }
}
fn main() {
    if let Err(error) = execute() {
        eprintln!("zeus: {error}");
        // Conventional 128+SIGINT so wrappers can tell a cancel from a failure.
        std::process::exit(if zeus::cancel::cancelled() { 130 } else { 1 });
    }
}
