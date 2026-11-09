//! Cooperative cancellation from SIGINT/SIGTERM/SIGHUP.
//!
//! The first signal sets a process-wide flag that model, tool, verifier and
//! MCP loops poll between steps; the runtime then records the cancellation and
//! releases its state lock. A second signal restores the default disposition
//! and re-raises, so a wedged run can still be killed; the next run reclaims
//! the stale lock by PID liveness.
use std::sync::atomic::{AtomicBool, Ordering};

static CANCELLED: AtomicBool = AtomicBool::new(false);
pub const MESSAGE: &str = "run cancelled by signal";

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

/// Err once a signal has been received; callers stop starting new work.
pub fn check() -> Result<(), String> {
    if cancelled() {
        Err(MESSAGE.into())
    } else {
        Ok(())
    }
}

/// Clear the flag between independent console tasks.
pub fn reset() {
    CANCELLED.store(false, Ordering::SeqCst);
}
