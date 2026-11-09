//! Cooperative cancellation from SIGINT/SIGTERM/SIGHUP.
//!
//! The first signal sets a process-wide flag that model, tool, verifier and
//! MCP loops poll between steps; the runtime then records the cancellation and
//! releases its state lock. A second signal restores the default disposition
//! and re-raises, so a wedged run can still be killed; the next run reclaims
//! the stale lock by PID liveness.
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

static CANCELLED: AtomicBool = AtomicBool::new(false);
/// Whole-job deadline as milliseconds since the process-local epoch; 0 = none.
static DEADLINE_MS: AtomicU64 = AtomicU64::new(0);
static DEADLINE_SECS: AtomicU64 = AtomicU64::new(0);
pub const MESSAGE: &str = "run cancelled by signal";

fn epoch() -> std::time::Instant {
    static EPOCH: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
    *EPOCH.get_or_init(std::time::Instant::now)
}
fn now_ms() -> u64 {
    epoch().elapsed().as_millis() as u64
}

/// Arm the whole-job deadline; every cancellation checkpoint then enforces it.
pub fn set_deadline(seconds: u64) {
    DEADLINE_SECS.store(seconds, Ordering::SeqCst);
    DEADLINE_MS.store(now_ms() + seconds * 1000 + 1, Ordering::SeqCst);
}
fn deadline_exceeded() -> bool {
    let deadline = DEADLINE_MS.load(Ordering::SeqCst);
    deadline != 0 && now_ms() >= deadline
}
/// Why work must stop: "cancelled" (signal) or "deadline_exceeded".
pub fn reason() -> Option<&'static str> {
    if cancelled() {
        Some("cancelled")
    } else if deadline_exceeded() {
        Some("deadline_exceeded")
    } else {
        None
    }
}

extern "C" fn on_signal(signal: libc::c_int) {
    if CANCELLED.swap(true, Ordering::SeqCst) {
        // Only async-signal-safe calls: restore default and re-raise.
        unsafe {
            libc::signal(signal, libc::SIG_DFL);
            libc::raise(signal);
        }
    }
}

/// Install handlers without SA_RESTART so blocking reads return EINTR and
/// callers observe the flag promptly. Idempotent.
pub fn install() -> Result<(), String> {
    for signal in [libc::SIGINT, libc::SIGTERM, libc::SIGHUP] {
        let mut action: libc::sigaction = unsafe { std::mem::zeroed() };
        action.sa_sigaction = on_signal as usize;
        action.sa_flags = 0;
        unsafe {
            libc::sigemptyset(&mut action.sa_mask);
            if libc::sigaction(signal, &action, std::ptr::null_mut()) != 0 {
                return Err(format!(
                    "cannot install signal handler: {}",
                    std::io::Error::last_os_error()
                ));
            }
        }
    }
    Ok(())
}

pub fn cancelled() -> bool {
    CANCELLED.load(Ordering::SeqCst)
}

/// Err once a signal has been received or the job deadline has passed;
/// callers stop starting new work.
pub fn check() -> Result<(), String> {
    if cancelled() {
        Err(MESSAGE.into())
    } else if deadline_exceeded() {
        Err(format!(
            "job deadline exceeded after {}s (max_job_seconds)",
            DEADLINE_SECS.load(Ordering::SeqCst)
        ))
    } else {
        Ok(())
    }
}

/// Clear the flag and deadline between independent console tasks.
pub fn reset() {
    CANCELLED.store(false, Ordering::SeqCst);
    DEADLINE_MS.store(0, Ordering::SeqCst);
}
