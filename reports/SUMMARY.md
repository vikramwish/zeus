# Autopilot Summary (`vikramwish/zeus`)

## Shipped / In Progress
- **PR #1 (`autopilot/ci`, merged into `main` as `9dbba43`):** Added GitHub Actions CI (`ubuntu-latest` + `macos-latest` with Seatbelt & `bubblewrap`), Dependabot (`cargo` + `github-actions`), MIT `LICENSE`, CI/License badges in `README.md`, Rust 1.91+ Clippy MSRV-safe fixes (`src/model.rs`, `src/cancel.rs`), verifier inherited-FD `FD_CLOEXEC` hygiene fix + test, and Linux `bubblewrap` merged-`/usr` symlink + `--tmpfs /var/empty` fix + tests.
- **PR #6 (`autopilot/research-design`):** Added `DESIGN.md`, `design-briefs/zeus-research.md`, Technical Report TR-01 (`research/REPORT.md`, `research/paper/zeus_report.tex`, compiled `research/paper/zeus-report.pdf`), reproducible SVG/PDF pipeline (`research/scripts/{extract_field_runs,build_figures,render_figures}.py`, `docs/figures/*.svg`), scrubbed dataset (`research/data/field_runs.json`), interactive workbench (`research/{index.html,styles.css,app.js}`), Playwright verification (`research/scripts/verify_workbench.py` + 1440px/390px screenshots), CI `research` job in `.github/workflows/ci.yml`, GitHub Pages workflow `.github/workflows/pages.yml`, and updated `README.md` links.

## Merged PRs
- **PR #1:** `ci: add GitHub Actions workflow, Dependabot, and MIT license (#1)` -> `9dbba43` (Linux & macOS CI green).
- **PR #6:** `docs(research): add TR-01 monograph, interactive workbench, CI checks, and Pages deployment (#6)` -> squash-merging onto `main` after green Linux, macOS, and Research Playwright CI.

## Live URLs
- **GitHub Pages (`.github/workflows/pages.yml` on `main`):** `https://vikramwish.github.io/zeus/research/`

## Open Risks
- Live multi-model (`Qwen 3.5-9B` / API) inference benchmarks remain bounded to the exploratory `kv.py` field trials documented in TR-01; CI exercises deterministic scripted local HTTP fixtures end-to-end on Linux (`bubblewrap`) and macOS (`Seatbelt`).
- If GitHub repository settings restrict workflow Pages enablement without manual Pages source selection, verify `Settings -> Pages -> Source: GitHub Actions` after PR #6 merges.

## Next Moves
1. Merge PR #6 once Linux, macOS, and Research Playwright CI checks pass and `pr-review` completes, then verify live GitHub Pages deployment at `https://vikramwish.github.io/zeus/research/` and update GitHub repo homepage & topics.
2. Collect `>= 30` domain-matched verified runs per arm to calibrate and graduate *Zeus Judge* (`src/judge.rs`) from shadow abstention into benchmark evaluation alongside held-out multi-model evals.
