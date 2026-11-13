# Architectural & Autopilot Decisions (`zeus`)

## 2026-11-13 — Autopilot Resumption, CI Baseline & CI-Discovered Fixes
- **Assumption / Scope:** Resumed in `/Users/vikramviswanath/Desktop/zeus` under free-rein autopilot rules, observing an uncommitted `.worktrees/research` (`autopilot/research-design`) technical report/figures draft (`2026-11-13 19:29`) missing its cited `research/index.html` workbench and `DESIGN.md`, alongside missing GitHub Actions CI, missing `LICENSE`, and missing repository topics/homepage metadata.
- **Decision 1 (CI Matrix & Hygiene):**
  - Added `.github/workflows/ci.yml` covering Linux (`ubuntu-latest`: `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test --locked`, `cargo build --release --locked`) and macOS (`macos-latest`: `cargo test --locked` exercising Seatbelt sandboxing).
  - Added `.github/dependabot.yml` for `cargo` (weekly) and `github-actions` (monthly).
  - Chose **MIT License** (`LICENSE`), matching standard portfolio defaults; owner can relicense if desired before wider distribution.
  - **Rejected alternative:** Linux-only CI was rejected because `tests/process_safety.rs` Seatbelt sandbox tests (`#[cfg(target_os = "macos")]`) would never execute in CI without a `macos-latest` runner.
- **Decision 2 (Rust 1.91+ Clippy Compatibility, MSRV-Safe):**
  - Linux CI ran a newer Rust stable where `AtomicUsize::fetch_update` is deprecated in favor of `try_update` and direct `on_signal as usize` triggers `-D function-casts-as-integer`.
  - Replaced `.fetch_update` in `src/model.rs` with a `compare_exchange_weak` loop (preserving Rust 1.85 MSRV compatibility rather than calling `try_update`) and cast `on_signal as *const () as usize` in `src/cancel.rs`.
- **Decision 3 (Verifier Inherited File Descriptor Hygiene):**
  - Both the local Antigravity runner (leaking socket fd 4) and GitHub Actions `macos-latest` (leaking socket fd 9) exposed that parent-inherited descriptors `>= 3` without `FD_CLOEXEC` were inherited into the sandboxed verifier child.
  - Started with a failing regression test (`macos::verifier_does_not_inherit_caller_non_cloexec_descriptors` in `tests/process_safety.rs`), then marked non-`launch_fd` descriptors `>= 3` as `FD_CLOEXEC` in `src/process.rs::supervise` (`pre_exec`) and `src/main.rs::verifier_exec`.
  - **Rejected alternative:** Suppressing or scoping the test to ignore non-Zeus sockets was rejected because parent-inherited descriptors are a genuine sandbox leak vector when Zeus is invoked by an IDE or CI runner.
- **Decision 4 (Linux Bubblewrap Merged-`/usr` Symlinks & `--tmpfs /var/empty` Mount Point):**
  - Enabling `bubblewrap` end-to-end execution on Ubuntu CI (`ubuntu-26.04`) revealed two real `bubblewrap_args` Linux bugs: (1) `--dir /var/empty --remount-ro /var/empty` attempted `MS_REMOUNT` on a non-mountpoint subdirectory (`EINVAL`), and (2) `Path::new(dir).is_dir()` followed merged-`/usr` symlinks (`/lib`, `/lib64`, `/bin`, `/sbin`) and emitted `--ro-bind` instead of `--symlink`, plus redundantly bound executables under an already read-only `/usr` mount.
  - Fixed `src/process.rs::bubblewrap_args` using `fs::symlink_metadata` (`--symlink` for directory symlinks, `--ro-bind` for real directories), skipped redundant nested file binds inside bound system directories, switched `/var/empty` to `--tmpfs /var/empty --remount-ro /var/empty`, added unit test in `src/process.rs`, updated `tests/sandbox_backend.rs`, updated `tests/process_safety.rs::unsupported_platform_fails_closed` for Linux hosts where `bwrap` is installed, replaced a hardcoded `/opt/homebrew/bin/python3` path in `tests/runtime_features.rs` with `common::python3()`, and handled early-exit `BrokenPipe` on stdin in `tests/session_cli.rs::console`.
  - **Rejected alternative:** Falling back to unsandboxed verifier execution or skipping `autonomy_runtime` repair-budget tests on Linux was rejected because fail-closed verifier sandboxing is a core security boundary.

## 2026-11-13 — PR #6 (`autopilot/research-design`): TR-01 Interactive Workbench, Figure Sync & GitHub Pages
- **Design & Frontend Architecture:**
  - Authored `DESIGN.md` and `design-briefs/zeus-research.md` (`claude-design-bridge` aligned tokens: archival paper `#F7F4EC`, vermilion bolt `#D93B21`, emerald teal `#156654`, cobalt `#28527D`, night obsidian `#0C0F16`, editorial serif + geometric sans + tabular monospace).
  - Built zero-build static workbench (`research/index.html`, `research/styles.css`, `research/app.js`) matching TR-01 semantics verbatim across five interactive labs (`FifoSchedulerLab`, `ScopeValidatorLab`, `RouterJudgeSandbox`, `FieldTrialsExplorer`, `FigurePlatesGallery`) with responsive breakpoints (`>=1040px`, `720–1039px`, `<=719px`) and an embedded fallback dataset alongside `fetch('./data/field_runs.json')`.
  - **Rejected alternative:** Introducing React/Vite/Node bundling solely for a static documentation lab was rejected to preserve Zeus's zero-JS-toolchain simplicity and ensure offline / GitHub Pages portability.
- **Reproducible Metrics & Report Sync (`9dbba43`):**
  - Merged post-PR #1 `main` (`9dbba43`) and added `"src/process"` under `Sandboxes, Verifier & Interruption` in `research/scripts/build_figures.py::TEST_GROUPS` so Figure 6 reflects all `266` static `#[test]` items (`46` suites) and `7,245` Rust LOC (`18` modules) cleanly without an orphan `"Other Modules"` row.
  - Synchronized `research/REPORT.md`, `research/paper/zeus_report.tex`, compiled `research/paper/zeus-report.pdf`, `docs/figures/*.svg`, `research/index.html`, `research/app.js`, and `research/scripts/verify_workbench.py` against `9dbba43`, updating the Linux `bubblewrap` empirical note to reflect passing end-to-end CI execution.
- **CI & Live Deployment:**
  - Extended `.github/workflows/ci.yml` with a parallel `research` job checking `build_figures.py --check` and Playwright headless verification (`verify_workbench.py` at `1440px` and `390px`).
  - Added `.github/workflows/pages.yml` (`actions/configure-pages@v5` + `actions/deploy-pages@v4`) publishing `research/` and `docs/figures/` with a root redirect to `./research/`.
