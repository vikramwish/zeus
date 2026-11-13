# Claude Design brief: Zeus Technical Report & Interactive Systems Lab

**Repo to import:** https://github.com/vikramwish/zeus  (Import, then GitHub, in Claude Design)
**What it is:** An interactive research monograph and systems workbench for Zeus, a standalone Rust runtime for concurrent, file-scoped coding agents under external verification.
**Who it's for:** Systems engineers, SRE/DevOps leads, and SDET reviewers evaluating Zeus's concurrency, fail-closed path isolation, sandboxed verification, UCB1 routing, and Zeus Judge shadow scoring in the first 10 seconds.

## Aesthetic direction
- Direction: Archival Systems Monograph meets Dark Execution Terminal — warm archival paper (`#F7F4EC`), crisp card plates (`#FFFFFF`), 1px stone rules (`#D4CCBA`), anchored by an obsidian masthead (`#0C0F16`) and vermilion bolt accent (`#D93B21`).
- References: Bell Labs / Usenix technical monographs, Stripe's interactive systems essays, and classic hardware lab notebooks.
- Type: Editorial serif (`Iowan Old Style` / `Charter` / `Palatino`) paired with system grotesque body and `SFMono-Regular` telemetry/code.
- Avoid: generic SaaS look, purple/blue gradients, Inter-everywhere defaults, decorative 3D blobs, emoji bullets.

## Screens and states
1. **Editorial Masthead & Proof Strip**: Dark obsidian header (`#0C0F16`), Zeus bolt mark, verified test/LOC metrics, quick actions to PDF report (`paper/zeus-report.pdf`), Markdown TR-01 (`REPORT.md`), and GitHub source.
2. **Work-Conserving FIFO vs Batch Barrier Lab**: Interactive Gantt comparison (`k ∈ {1,2,3}` workers, preset and custom comma-separated durations, mid-run failure admission halt), live makespan / idle slot-unit readouts, and Graham greedy bound.
3. **Pre-Admission Scope Isolation Validator (`Theorem 1`)**: Editable JSON coding plan + `autonomy.write_paths` allowlist with presets (`Disjoint Valid`, `Case Alias`, `Ancestor Overlap`, `Traversal`, `Handoff >8 KiB`) and live fail-closed invariant breakdown.
4. **UCB1 Bandit Router & Zeus Judge 8-Gate Sandbox**: Step-by-step interactive outcome simulator (Verified Pass, Verified Fail, Infrastructure Error) proving `0` reward corruption on errors, paired with an 8-gate fail-closed `Zeus Judge` abstention inspector.
5. **14 Field Trials Explorer (`T01`–`T14`) & Figures Gallery**: Filterable dataset view over `research/data/field_runs.json` with empty/no-match state, error-class tags, commit provenance, plus interactive plate switcher for `fig1`–`fig7` and `zeus-cover.svg`.
For every interactive lab, also handle: empty input, validation error state, no-match filter state, and mobile layout (`390px`).

## Components (use these names)
- `<MastheadHero>`, `<FifoSchedulerLab>`, `<ScopeValidatorLab>`, `<RouterJudgeSandbox>`, `<FieldTrialsExplorer>`, `<FigurePlatesGallery>`

## Content
Use real metrics, invariants, JSON schemas, and scrubbed field-trial rows from `research/REPORT.md`, `research/data/field_runs.json`, `src/runtime.rs`, `src/policy.rs`, `src/judge.rs`, and `docs/figures/*.svg`. No lorem ipsum.

## Deliverables
- Interactive static web workbench (`research/index.html`) matching `DESIGN.md` tokens
- Responsive desktop (`1440px`) and mobile (`390px`) layouts verified with Playwright
- Design tokens (`DESIGN.md`)
- Export: "Send to local coding agent" (or Part C fallback implementation in `research/index.html`)

