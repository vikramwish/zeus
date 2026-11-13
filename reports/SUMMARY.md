# Autopilot Summary (`vikramwish/zeus`)

## Shipped / In Progress
- **PR #1 (`autopilot/ci`, merged into `main` as `9dbba43`):** Added GitHub Actions CI (`ubuntu-latest` + `macos-latest` with Seatbelt & `bubblewrap`), Dependabot (`cargo` + `github-actions`), MIT `LICENSE`, CI/License badges in `README.md`, Rust 1.91+ Clippy MSRV-safe fixes, verifier inherited-FD `FD_CLOEXEC` hygiene fix + test, and Linux `bubblewrap` merged-`/usr` symlink + `--tmpfs /var/empty` fix + tests.
- **PR #6 (`autopilot/research-design`, merged into `main` as `0692fa6`):** Added `DESIGN.md`, `design-briefs/zeus-research.md`, Technical Report TR-01 (`research/REPORT.md`, `research/paper/zeus_report.tex`, compiled `research/paper/zeus-report.pdf`), reproducible SVG/PDF pipeline, scrubbed dataset (`research/data/field_runs.json`), interactive workbench (`research/{index.html,styles.css,app.js}`), CI `research` Playwright job, and GitHub Pages workflow `.github/workflows/pages.yml`.
- **PR #7 (`autopilot/perf-cleanup`, merged into `main` as `38db577`):** Codebase performance optimization and cleanup across `src/{workspace,output,context,agent,model,evidence,runtime,auth,judge,plugins}.rs`, `Cargo.toml`, and `tests/workspace_safety.rs`, plus TR-01 / SVG / LaTeX / workbench synchronization (`7,852` Rust LOC across `18` modules; `269` tests across `48` suites).
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
- **Branch `autopilot/repo-audit` (PR pending):** Full 21-repository `vikramwish` portfolio audit (`reports/AUDIT.md` + `reports/repos.json`) with 100-point scoring, 6-pin plan, zero-leak security verification, and prioritized remediation queue.

## Merged PRs
- **PR #1:** `ci: add GitHub Actions workflow, Dependabot, and MIT license (#1)` -> `9dbba43`
- **PR #6:** `docs(research): add TR-01 monograph, interactive workbench, CI checks, and Pages deployment (#6)` -> `0692fa6`
- **PR #7:** `perf,fix: optimize snapshot/context/search/hash/compaction hot paths & sync TR-01 (#7)` -> `38db577`

## Live URLs
- **Zeus Pages / TR-01 Workbench:** `https://vikramwish.github.io/zeus/research/` (HTTP `200`)
- **Epistemic Prism (`apart-epistemics`):** `https://vikramwish.github.io/apart-epistemics/` (HTTP `200`)
- **Haven & Key (`haven-key`, private):** `https://haven-key-4f8z.vercel.app` (currently HTTP `404` — queued for fix/redeploy after green CI)

## Open Risks
- Live multi-model (`Qwen 3.5-9B` / API) inference benchmarks remain bounded to the exploratory `kv.py` field trials documented in TR-01; CI exercises deterministic scripted local HTTP fixtures end-to-end on Linux (`bubblewrap`) and macOS (`Seatbelt`).
- **Portfolio fixes queued:** `openfiles-mac` (no CI / `LICENSE`), `tally` (no CI / `LICENSE` / GitHub metadata / live web demo), `haven-key` (failing CI run `43211605135` on `main` & Vercel `404`), `memory-game` (`5,356` public tracked files incl. `node_modules/`), `8ballco` (failing apt DNS step on PR #1).

## Next Moves
1. Land `reports/AUDIT.md` on `zeus` via PR + `pr-review`, then execute Cycle 1 fixes: `openfiles-mac` (CI + MIT license + README), `tally` (CI + MIT license + metadata + static web deploy), and `haven-key` (fix CI regression + Vercel redeploy), plus profile pins update.
2. Execute Cycle 2 builds: `project-forge` + `feature-builder` on a new TypeScript/Edge project in a fresh lane, and a clean-room `rea-clean-room` teardown & original rebuild (setting `PATH="/opt/homebrew/opt/node@24/bin:$PATH"` for `rea-agents`).
