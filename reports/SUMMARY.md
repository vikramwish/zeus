# Autopilot Summary (`vikramwish/zeus`)

## Shipped / In Progress
- **PR 1 (`autopilot/ci`):** Added GitHub Actions CI (`ubuntu-latest` + `macos-latest` with Seatbelt & `bubblewrap`), Dependabot (`cargo` + `github-actions`), MIT `LICENSE`, CI/License badges in `README.md`, Rust 1.91+ Clippy MSRV-safe fixes (`src/model.rs`, `src/cancel.rs`), verifier inherited-FD `FD_CLOEXEC` hygiene fix + test, and Linux `bubblewrap` merged-`/usr` symlink + `--tmpfs /var/empty` fix + tests.

## Merged PRs
- Pending PR #1 green CI and merge.

## Live URLs
- Pending GitHub Pages deployment in PR 2 (`autopilot/research-design`).

## Open Risks
- Live multi-model (`Qwen 3.5-9B` / API) inference benchmarks remain bounded to the exploratory `kv.py` field trials documented in TR-01; CI exercises deterministic scripted local HTTP fixtures end-to-end on Linux (`bubblewrap`) and macOS (`Seatbelt`).

## Next Moves
1. Confirm PR #1 passes both Linux and macOS CI checks, complete `pr-review`, and squash-merge.
2. Complete PR 2 (`autopilot/research-design`): `DESIGN.md`, `design-briefs/zeus-research.md`, interactive `research/index.html`, refresh synced figures/PDF (`build_figures.py`, `render_figures.py`, `zeus_report.tex`), Playwright 1440px/390px verification, GitHub Pages deploy, and README/GitHub metadata polish.
