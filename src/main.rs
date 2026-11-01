use std::path::Path;

// Only the host process launcher invokes this path, after sandbox-exec has
// installed Seatbelt. It is not an agent tool or an alternate job/approval path.
// A private inherited channel distinguishes kernel exec errors from test exits.
#[cfg(target_os = "macos")]
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
        #[cfg(target_os = "macos")]
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
        [command, path, flags @ ..] if command == "run" => {
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
            let report = zeus::runtime::run(job)?;
            println!(
                "{}",
                serde_json::to_string_pretty(&report).map_err(|e| e.to_string())?
            );
            match report["status"].as_str() {
                Some("verified" | "already_verified") => Ok(()),
                Some("unverified") => Err(
                    "analysis finished without an external verifier; candidate is unverified"
                        .into(),
                ),
                _ => Err("verification did not pass; see retained report".into()),
            }
        }
        _ => Err(USAGE.into()),
    }
}
fn main() {
    if let Err(error) = execute() {
        eprintln!("zeus: {error}");
        std::process::exit(1);
    }
}
