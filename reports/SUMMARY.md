# Autopilot Summary (`vikramwish/zeus`)

## Shipped / In Progress
- **PR 1 (`autopilot/ci`):** Added GitHub Actions CI (`ubuntu-latest` + `macos-latest`), Dependabot (`cargo` + `github-actions`), MIT `LICENSE`, CI/License badges in `README.md`, and `reports/` audit logs.

## Merged PRs
- Pending PR 1 CI verification and review.

## Live URLs
- Pending GitHub Pages deployment in PR 3 (`autopilot/research-design`).

## Open Risks
- Host-inherited file descriptors `>= 3` without `FD_CLOEXEC` leak into the sandboxed verifier child (to be fixed via TDD in PR 2 `autopilot/fix-verifier-fd-hygiene`).
- Linux `bubblewrap` end-to-end execution remains unexercised on a privileged/unrestricted Linux namespace host (CI Ubuntu tests boundary argument synthesis and fail-closed behavior).

## Next Moves
1. Land PR 1 (`autopilot/ci`) after `pr-review` and green GitHub Actions checks.
2. Fix verifier inherited-FD hygiene in PR 2 (`autopilot/fix-verifier-fd-hygiene`) starting with a failing reproduction test.
3. Finish `autopilot/research-design` (PR 3): `DESIGN.md`, `design-briefs/zeus-research.md`, interactive `research/index.html`, Playwright 1440px/390px verification, GitHub Pages deploy, and README/GitHub metadata polish.
