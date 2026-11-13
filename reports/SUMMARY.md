# Autopilot Summary (`vikramwish/zeus`)

## Shipped / In Progress
- **PR #1 (`autopilot/ci`, merged into `main` as `9dbba43`):** Added GitHub Actions CI (`ubuntu-latest` + `macos-latest` with Seatbelt & `bubblewrap`), Dependabot (`cargo` + `github-actions`), MIT `LICENSE`, CI/License badges in `README.md`, Rust 1.91+ Clippy MSRV-safe fixes (`src/model.rs`, `src/cancel.rs`), verifier inherited-FD `FD_CLOEXEC` hygiene fix + test, and Linux `bubblewrap` merged-`/usr` symlink + `--tmpfs /var/empty` fix + tests.
- **PR #2 (`autopilot/research-design`, ready for PR & merge):** Added `DESIGN.md`, `design-briefs/zeus-research.md`, Technical Report TR-01 (`research/REPORT.md`, `research/paper/zeus_report.tex`, compiled `research/paper/zeus-report.pdf`), reproducible SVG/PDF pipeline (`research/scripts/{extract_field_runs,build_figures,render_figures}.py`, `docs/figures/*.svg`), scrubbed dataset (`research/data/field_runs.json`), interactive workbench (`research/{index.html,styles.css,app.js}`), Playwright verification (`research/scripts/verify_workbench.py` + 1440px/390px screenshots), CI `research` job in `.github/workflows/ci.yml`, GitHub Pages workflow `.github/workflows/pages.yml`, and updated `README.md` links.

## Merged PRs
- **PR #1:** `ci: add GitHub Actions workflow, Dependabot, and MIT license (#1)` -> `9dbba43` (Linux & macOS CI green).

## Live URLs
- **GitHub Pages (configured via `.github/workflows/pages.yml` on merge to `main`):** `https://vikramwish.github.io/zeus/research/`

## Open Risks
- Live multi-model (`Qwen 3.5-9B` / API) inference benchmarks remain bounded to the exploratory `kv.py` field trials documented in TR-01; CI exercises deterministic scripted local HTTP fixtures end-to-end on Linux (`bubblewrap`) and macOS (`Seatbelt`).
- If GitHub repository settings restrict workflow Pages enablement without manual Pages source selection, verify `Settings -> Pages -> Source: GitHub Actions` after PR #2 merges.

## Next Moves
1. Run final local sequential checks (`verify_workbench.py`, `build_figures.py --check`, `cargo fmt/clippy/test`), secret-scan, commit PR #2, verify green CI, `pr-review`, and squash-merge into `main`.
2. Watch GitHub Pages deploy on `main`, verify live `https://vikramwish.github.io/zeus/research/` HTTP 200 & zero console errors, and update GitHub repository description, homepage URL, and topics.
