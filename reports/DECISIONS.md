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

## 2026-11-13 — Performance & Codebase Cleanup (`autopilot/perf-cleanup`)
- **Measurement-First Baseline:** Built a reproducible `--release` microbenchmark suite (`11` realistic hotspots covering 500-file/16 MiB candidate snapshots, `context::{project_context,list_skills}`, miss/dense-hit `Workspace::search`, 128 KiB `digest` & `secret_like`, 64 KiB `compact_verifier_result`, `evidence::validate_findings`, and 35-turn `agent::run` compaction) against `main` (`0692fa6`).
- **Decision 1 (Eliminate Quadratic & Full-Tree Tax in Context/Snapshot/Search/Evidence):**
  - Switched `context::project_context` (`project_context_value` shared with `runtime.rs`) to direct `symlink_metadata(root/AGENTS.md)` + open/read and scoped `context::list_skills` to `workspace.inspect_subdir("skills")`. Rejected maintaining an always-warm directory cache across runs because it introduces invalidation hazards on candidate mutations.
  - Made `Workspace::snapshot` single-pass (`walk_root`/`walk_dir`/`visit_open_regular_file`), reusing one `Vec<u8>` buffer per walk, caching parent directory creation per subdir, using `DirEntry::file_type()` (`d_type`) + single `open_regular_nofollow` + `fstat`, and removing redundant `exists()`/`resolve` syscalls in `Workspace::write`.
  - Replaced quadratic `.lines()`/`.contains()` scanning in `Workspace::search` with single-pass `O(N)` running newline counting, skipped SHA-256 on non-matching files, and added early-abort `output::json_len_within` to avoid heap serialization just to enforce byte budgets in `Workspace::search`, `runtime.rs`, `agent::rs`, and `model::rs`.
  - Added `evidence::SourceIndex` (`HashSet<&str>` / `Mutex<HashSet<String>>` in `runtime::Shared`) for `O(1)` evidence source checks instead of `O(F*S)` linear scans.
- **Decision 2 (Hardware SHA-256 via `ring`, Streaming Output Compaction, & Single-Pass LUT Secret Scan):**
  - Unified all runtime SHA-256 hashing (`workspace::digest`, `auth.rs`, `judge.rs`, `plugins.rs`) onto BoringSSL assembly inside `ring::digest::SHA256` (already in the dependency graph) with a stack `[u8; 64]` hex buffer, moving `sha2` to `[dev-dependencies]` so vector cross-checks remain covered without extra release binary dependencies.
  - Replaced two-pass `secret_like` scans with a 4-byte `const` LUT single-pass scan preserving exact pattern boundaries and `"private key block"` precedence, and streamed `output::compact_libtest` with zero intermediate `Vec<&str>`/`format!` allocations.
  - Borrowed `ChatCompletionsRequest<'a>` in `model.rs` to eliminate per-turn transcript/tool deep cloning, reused the SSE `line` buffer in `responses_stream`, and moved owned `message` values out of `agent::run`.
- **Decision 3 (Linux `ETXTBSY` Fix & Release Codegen Profile):**
  - Explicitly dropped the writable `File` handle after `sync_all()` before `fs::rename` in `Workspace::write`, paired with transient `ExecutableFileBusy` retry in `tests/workspace_safety.rs::assert_exec_stdout`.
  - Set `codegen-units = 1` alongside existing `lto = "thin"` and `strip = true` in `[profile.release]`. **Rejected** `panic = "abort"` because `src/agent.rs::run_tasks` requires unwinding to catch worker panics cleanly via `std::panic::catch_unwind`.
- **Decision 4 (TR-01 & Research Workbench Synchronization):**
  - Categorized the 3 new inline unit tests (`src/workspace.rs`, `src/output.rs`) under `Workspace, Secret Guard & Output` (`43` tests) in `research/scripts/build_figures.py::TEST_GROUPS`, regenerated `docs/figures/*.svg`, rendered `research/paper/figures/*.pdf`, recompiled `research/paper/zeus-report.pdf`, and synchronized `research/REPORT.md`, `research/index.html`, `research/app.js`, and `research/scripts/verify_workbench.py` (`7,852` LOC, `269` tests across `48` suites).


## 2026-11-13 — Portfolio Audit (`autopilot/repo-audit`)
- **Scope:** Inventoried all `21` repositories on `vikramwish` (`6` public, `15` private; `19` active, `2` archived public coursework repos), cloned all `18` non-archived sibling repos into `/repos/` (excluded via `.git/info/exclude`), scanned working trees + git history for secret/env leaks, checked live URLs (`apart-epistemics`, `zeus`, `haven-key`), checked GitHub Actions history, and ran bounded local build/test smoke checks.
- **Prioritisation Decision:** Before forging brand-new repos, prioritize fixing structural flagship/operational regressions found in the audit:
  1. `openfiles-mac` (PUBLIC, pinned, 78/100): missing CI workflow & MIT `LICENSE`.
  2. `tally` (PUBLIC, pinned, 67/100): missing CI workflow, MIT `LICENSE`, GitHub description/topics/homepage, screenshots, and web preview.
  3. `haven-key` (PRIVATE, 64/100): failing CI on `main` (`43211605135`: phone nav language control assertion) and HTTP `404` on `https://haven-key-4f8z.vercel.app` (verified `vercel whoami` -> `vikramwish` is authenticated for redeployment after fixing `main`).
- **Rejected Alternative:** Immediately jumping to `project-forge` while 2 of 3 pinned public repos lacked CI/licenses and a flagship app's live URL returned `404`.
- **Policy Guardrails:** Left `INFO-5940-A1`, `AEM2850`, and stale archive candidates (`FoodSensing`, `memory-game`, `flip-face-off`, `Sensingdevices`) unmodified on GitHub per Hard Floor (no deleting/archiving/changing visibility of existing repos); flagged owner recommendations in `reports/AUDIT.md`.
