# Needs Owner (`vikramwish/zeus`)

- No hard blockers currently.
- Optional: Run `design-briefs/zeus-research.md` in Claude Design (`claude.ai` -> Artifacts -> Claude Design -> Import `vikramwish/zeus` -> Export -> Send to local coding agent) if you want a design-studio refinement pass over the deployed interactive TR-01 workbench (`research/index.html`).
- Note: Deterministic tests now exercise both macOS Seatbelt and Linux `bubblewrap` sandboxes end-to-end in GitHub Actions CI (`ci.yml`); only live non-mocked LLM multi-model benchmarks remain dependent on local/remote model endpoints outside CI.
