//! Helpers shared by integration tests. Each test file that needs one
//! declares `mod common;`.
#![allow(dead_code)]

/// Absolute path to a Python 3 interpreter for verifier fixtures. The job
/// schema requires an absolute executable, so tests resolve one on the host
/// instead of assuming a Homebrew install. System locations the verifier
/// sandbox can read are preferred; PATH is the last resort.
pub fn python3() -> String {
    for candidate in [
        "/opt/homebrew/bin/python3",
        "/usr/local/bin/python3",
        "/usr/bin/python3",
    ] {
        if std::path::Path::new(candidate).is_file() {
            return candidate.to_owned();
        }
    }
    let output = std::process::Command::new("/bin/sh")
        .args(["-c", "command -v python3"])
        .output()
        .expect("sh is available");
    let found = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    assert!(
        std::path::Path::new(&found).is_absolute(),
        "these tests need python3 on the host"
    );
    found
}
