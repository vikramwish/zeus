# Autopilot Summary (`vikramwish/zeus`)

## Shipped / In Progress
- **PR #1 (`autopilot/ci`, merged into `main` as `9dbba43`):** Added GitHub Actions CI (`ubuntu-latest` + `macos-latest` with Seatbelt & `bubblewrap`), Dependabot (`cargo` + `github-actions`), MIT `LICENSE`, CI/License badges in `README.md`, Rust 1.91+ Clippy MSRV-safe fixes, verifier inherited-FD `FD_CLOEXEC` hygiene fix + test, and Linux `bubblewrap` merged-`/usr` symlink + `--tmpfs /var/empty` fix + tests.
- **PR #6 (`autopilot/research-design`, merged into `main` as `0692fa6`):** Added `DESIGN.md`, `design-briefs/zeus-research.md`, Technical Report TR-01 (`research/REPORT.md`, `research/paper/zeus_report.tex`, compiled `research/paper/zeus-report.pdf`), reproducible SVG/PDF pipeline, scrubbed dataset (`research/data/field_runs.json`), interactive workbench (`research/{index.html,styles.css,app.js}`), CI `research` Playwright job, and GitHub Pages workflow `.github/workflows/pages.yml`.
- **Branch `autopilot/perf-cleanup` (PR pending):** Codebase performance optimization and cleanup across `src/{workspace,output,context,agent,model,evidence,runtime,auth,judge,plugins}.rs`, `Cargo.toml`, and `tests/workspace_safety.rs`, plus TR-01 / SVG / LaTeX / workbench synchronization (`7,852` Rust LOC across `18` modules; `269` tests across `48` suites).
  - Measured release benchmarks (`0692fa6` baseline -> `16c4d87` final Rust state):
    - `context_project_context_500files`: `54.688ms -> 0.018ms` (`~3,038x`)
    - `context_list_skills_500files`: `58.787ms -> 0.308ms` (`~191x`)
    - `output_compact_nonlibtest_64k`: `0.053ms -> 0.001ms` (`~53x`)
    - `evidence_validate_200x1000`: `0.109ms -> 0.013ms` (`~8.4x`)
    - `workspace_digest_128k` (`ring` SHA-256 + stack hex LUT): `0.211ms -> 0.039ms` (`~5.4x`)
    - `output_compact_libtest_64k`: `0.065ms -> 0.013ms` (`~5.0x`)
    - `workspace_search_miss_16m`: `59.142ms -> 18.779ms` (`~3.1x`)
    - `workspace_search_dense_10x120k`: `5.415ms -> 1.835ms` (`~3.0x`)
    - `workspace_snapshot_16m_500files`: `173.028ms -> 87.739ms` (`~2.0x`)
    - `agent_run_35turns_with_compaction`: `14.238ms -> 10.335ms` (`~1.4x`)
    - `workspace_secret_like_128k`: `0.098ms -> 0.075ms` (`~1.3x`)

## Merged PRs
- **PR #1:** `ci: add GitHub Actions workflow, Dependabot, and MIT license (#1)` -> `9dbba43`
- **PR #6:** `docs(research): add TR-01 monograph, interactive workbench, CI checks, and Pages deployment (#6)` -> `0692fa6`

## Live URLs
- **GitHub Pages (`.github/workflows/pages.yml` on `main`):** `https://vikramwish.github.io/zeus/research/`

## Open Risks
- Live multi-model (`Qwen 3.5-9B` / API) inference benchmarks remain bounded to the exploratory `kv.py` field trials documented in TR-01; CI exercises deterministic scripted local HTTP fixtures end-to-end on Linux (`bubblewrap`) and macOS (`Seatbelt`).
- Linux `ETXTBSY` hardening (`drop(file)` before rename + retry helper) is verified structurally by compilation and local macOS tests; Linux CI execution will provide final Linux runner verification once the PR opens.

## Next Moves
1. Run full Rust, Clippy, research figure, and Playwright checks, commit docs/reports, open PR, run `pr-review`, monitor CI green (`Linux`, `macOS`, `Research`), and squash-merge onto `main`.
2. Check GitHub Pages deployment on `main` and move to the next candidate project or feature under `project-forge`.
