# Architectural & Autopilot Decisions (`zeus`)

## 2026-11-13 — Initial Autopilot Resumption & CI Baseline
- **Assumption / Scope:** Resumed in `/Users/vikramviswanath/Desktop/zeus` under free-rein autopilot rules, observing an uncommitted `.worktrees/research` (`autopilot/research-design`) technical report/figures draft (`2026-11-13 19:29`) missing its cited `research/index.html` workbench and `DESIGN.md`, alongside missing GitHub Actions CI, missing `LICENSE`, and missing repository topics/homepage metadata.
- **Decision (PR 1 — `autopilot/ci`):**
  - Added `.github/workflows/ci.yml` covering Linux (`ubuntu-latest`: `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test --locked`, `cargo build --release --locked`) and macOS (`macos-latest`: `cargo test --locked` exercising real Seatbelt sandboxing).
  - Added `.github/dependabot.yml` for `cargo` (weekly) and `github-actions` (monthly).
  - Chose **MIT License** (`LICENSE`), matching standard portfolio defaults; owner can relicense if desired before wider distribution.
  - **Rejected alternative:** Linux-only CI was rejected because Zeus's `tests/process_safety.rs` Seatbelt sandbox tests (`#[cfg(target_os = "macos")]`) would never execute in CI without a `macos-latest` runner.
- **Discovery Logged for PR 2 (`autopilot/fix-verifier-fd-hygiene`):**
  - Running `cargo test --locked` under a parent process holding an open socket on fd 4 without `FD_CLOEXEC` exposed that `zeus::process::verify` / `verifier_exec` only cleared inheritance for `launch_fd` and did not enforce `FD_CLOEXEC` across other inherited descriptors `>= 3`, causing `macos::verifier_does_not_inherit_private_launch_channel` to fail unless `4>&-` is passed. Tracked for a dedicated TDD bugfix PR next.
