# GitHub Portfolio Audit — `vikramwish`

*Audited: 2026-11-13 (`21` repositories in [repos.json](repos.json): `19` active/non-archived, `2` archived public coursework forks/repos; `6` public, `15` private).*

## Executive Summary

Two recently built public repositories—[`zeus`](https://github.com/vikramwish/zeus) (**100/100**, Rust concurrent agent runtime + TR-01 workbench) and [`apart-epistemics`](https://github.com/vikramwish/apart-epistemics) (**97/100**, Python Track 4 epistemic diversity evaluation suite + live GitHub Pages dashboard)—already clear flagship quality with green CI and live pages. Below them, the portfolio's highest-leverage gaps are structural rather than algorithmic: two of the **three currently pinned public repositories** ([`openfiles-mac`](https://github.com/vikramwish/openfiles-mac) and [`tally`](https://github.com/vikramwish/tally)) lack GitHub Actions CI and an `LICENSE` file (`tally` also lacks a GitHub description, topics, homepage/demo, and screenshots), while [`haven-key`](https://github.com/vikramwish/haven-key) (private flagship Next.js/Payload build) currently has both a **failing CI test on `main`** (mobile nav localization assertion) and a **Vercel `404` on its configured homepage (`https://haven-key-4f8z.vercel.app`)**. Across the whole account, automated secret scans found **zero leaked credentials or `.env` files** in current trees or Git history.

### GitHub Profile Pins (3 of 6 slots currently used)

- **Currently pinned:** `zeus`, `tally`, `openfiles-mac`
- **Recommended 6 pins (after Cycle 1 polish + Cycle 2 builds):**
  1. `zeus` *(FLAGSHIP — concurrent Rust agent runtime & TR-01 workbench)*
  2. `apart-epistemics` *(FLAGSHIP — multi-agent diversity eval suite + GitHub Pages)*
  3. `openfiles-mac` *(upgrade from POLISH -> FLAGSHIP via macOS CI, MIT `LICENSE`, screenshots)*
  4. `tally` *(upgrade from POLISH -> FLAGSHIP via CI, MIT `LICENSE`, metadata, static web demo)*
  5. *New Public Forge Build #1* *(planned in next cycle, distinct domain & TypeScript/edge stack)*
  6. *Clean-Room Rebuild or New Public Build #2* *(complete 6/6 public flagship shelf for SWE / SDET / DevOps roles)*

---

## Portfolio Scorecard (100-Point Rubric)

Rubric weights: README `20` · Runs `20` · Tests `15` · CI `10` · Hygiene `10` · Discoverability `10` · Design `10` · Recency `5`.
Tiers: **FLAGSHIP** (`80–100`), **POLISH** (`50–79`), **ARCHIVE-CANDIDATE** (`< 50`).

| Repo | Tier | Score | Biggest Gap | Top 3 Fixes | Effort |
|---|---:|---:|---|---|---|
| `zeus` (PUBLIC, pinned) | FLAGSHIP | **100** | 4 open Dependabot PRs (`#2–#5`) left untouched by policy | 1) Owner triage of Dependabot PRs `#2–#5` · 2) Keep benchmarks reproducible · 3) Maintain release/Pages sync | S |
| `apart-epistemics` (PUBLIC) | FLAGSHIP | **97** | Not pinned yet; 5 open `actions/*` Dependabot PRs; no Python lockfile | 1) Pin to GitHub profile · 2) Owner triage of `actions/*` PRs · 3) Add `uv.lock` / pinned export for hermetic CI | S |
| `med-sim-thesis` (PRIVATE) | FLAGSHIP | **86** | Missing `LICENSE` & `.env.example`; bare Python lacks `requirements.txt` venv | 1) Add `LICENSE` & `.env.example` · 2) Document `uv`/venv test command in README · 3) Clean compiled static bundle check in CI | S |
| `openfiles-mac` (PUBLIC, pinned) | POLISH | **78** | Zero `.github/workflows` CI and missing MIT `LICENSE` | 1) Add `macos-14` CI running `./scripts/test.sh` · 2) Add MIT `LICENSE` & CI badge · 3) Add UI screenshots / release notes in README | M |
| `vikramwish` (PUBLIC) | POLISH | **74** | Profile README only links static links; no PR CI check | 1) Add lightweight Node CI running `node scripts/test-terminal.mjs` · 2) Add MIT `LICENSE` · 3) Refresh showcase card for 6 pinned repos | S |
| `inkchat` (PRIVATE) | POLISH | **68** | No CI running `sh tests/run.sh`, no `LICENSE`, empty topics | 1) Add Ubuntu CI running `sh tests/run.sh` · 2) Add `LICENSE` · 3) Add e-ink architecture/GIF preview & repo topics | S |
| `tally` (PUBLIC, pinned) | POLISH | **67** | Empty GitHub metadata (`description`, `topics`, `homepage`), no CI, no `LICENSE` | 1) Add Node CI (`tsc --noEmit`, Jest, Expo web build + Playwright smoke) · 2) Add MIT `LICENSE`, description, topics & screenshots · 3) Deploy static web preview (`homepage`) | M |
| `haven-key` (PRIVATE) | POLISH | **64** | **Red CI on `main`** (`43211605135`: nav language assertion) & **Vercel `404`** | 1) Fix failing mobile nav/language regression & verify CI green · 2) Redeploy Vercel & restore live `200` · 3) Add `LICENSE`, metadata, remove stray `AGENTS copy.md` | M |
| `openfiles` (PRIVATE, fork) | POLISH | **63** | Duplicate predecessor fork of `yatinbayya/OpenFiles` with 1 open PR | 1) Decide whether to retire/archive in favor of `openfiles-mac` (owner only) · 2) Port any needed cloud changes from open PR · 3) Add CI if kept active | S |
| `nav-studio` (PRIVATE) | POLISH | **62** | No CI; Playwright tests hit `ERR_CONNECTION_REFUSED` without running `:5173` server | 1) Add self-starting preview webServer in test script + Node CI · 2) Clean `oxlint` warnings (`29`) · 3) Add `LICENSE` & metadata | M |
| `ng136-harness` (PRIVATE) | POLISH | **58** | No CI; bare Python tests hit 21 errors / 5 failures without installed deps | 1) Add Python CI installing `requirements.txt` & running `unittest` · 2) Add `LICENSE` & `.env.example` · 3) Add lockfile (`uv.lock`) | M |
| `polar-browser-app` (PRIVATE) | POLISH | **54** | No CI; missing root `LICENSE` file; workflow test times out without Playwright browser | 1) Add CI with Playwright Chromium + unit suite separation · 2) Add `LICENSE` matching `package.json` · 3) Document source-vs-prebuilt asset policy | M |
| `8ballco` (PRIVATE) | POLISH | **52** | Open PR #1 CI failing on transient Ubuntu `apt` DNS step; unmerged backend PR; empty metadata | 1) Harden apt/Playwright install & re-run PR CI · 2) Land or close PR #1 (`feat/payload-backend-and-venue-map`) · 3) Add `LICENSE` & repo metadata | M |
| `gpf-close` (PRIVATE) | ARCHIVE-CANDIDATE | **49** | No CI; bare Python lacks `pdfplumber`/`openpyxl`; missing `LICENSE` & metadata | 1) Add Python CI installing `requirements.txt` · 2) Add `LICENSE` & metadata · 3) Archive if coursework/one-off contest script | S |
| `med-sim-usyd` (PRIVATE) | ARCHIVE-CANDIDATE | **41** | No LaTeX CI, plain-text README, unstructured `AppendixSomething/` naming | 1) Add `latex.yml` build CI · 2) Clean folder naming / stray `PneqNP.pdf` · 3) Convert README to Markdown | M |
| `flip-face-off` (PRIVATE) | ARCHIVE-CANDIDATE | **23** | Unedited Lovable boilerplate README, zero tests, stale (`2025-08`) | 1) Archive unless actively developed (owner decision) · 2) Remove conflicting `bun.lockb` · 3) Write real README if revived | S |
| `memory-game` (PUBLIC) | ARCHIVE-CANDIDATE | **22** | **5,356 tracked files (`node_modules/` committed)**, `backup_untracked/` collision breaks Jest, no `.gitignore` | 1) **Privatize or archive via owner**, or strip `node_modules/` + add `.gitignore` · 2) Add real tests & remove `backup_untracked/` · 3) Add `LICENSE` & metadata | M |
| `Sensingdevices` (PRIVATE) | ARCHIVE-CANDIDATE | **19** | 10-line README, no `.gitignore`, no tests/CI, raw CSV logs committed | 1) Archive if past research data snapshot (owner only) · 2) Add `.gitignore` & `requirements.txt` · 3) Move raw CSVs out of Git history if active | S |
| `FoodSensing` (PRIVATE) | ARCHIVE-CANDIDATE | **8** | Empty README, 2 orphan files (`DHTserver.c`, `Validation.py`), stale since `2024-03` | 1) Recommend owner archive · 2) Consolidate into research archive notes · 3) N/A unless hardware work resumes | S |
| `INFO-5940-A1` (PUBLIC, archived) | ARCHIVED | — | Archived public coursework assignment fork | See Course-Integrity Note below (do not modify under Hard Floor) | — |
| `AEM2850` (PUBLIC, archived) | ARCHIVED | — | Archived public coursework repository | See Course-Integrity Note below (do not modify under Hard Floor) | — |

---

## Security Findings (Secret & `.env` Hygiene Scan)

Automated inspection across working trees and full commit histories (`git log --all` `.env*` search + pattern match for `KEY|TOKEN|SECRET|PASSWORD|sk-|ghp_|BEGIN .* PRIVATE KEY`) found **zero leaked real credentials or tracked `.env` files**:

- **Tracked `.env` / `.env.*` files (excluding `.env.example`):** `0` currently tracked and `0` in Git history across all `19` active repositories.
- **Flagged synthetic / benign hits verified by inspection:**
  - `zeus` (`tests/secret_guard.rs`, commit `7b23cf3`): deliberate synthetic test fixture (`sk-abcdefghi...`) testing secret redaction.
  - `openfiles-mac` & `openfiles` (`Tests/OpenFilesChecks/LibraryTests.swift`, commit `a05ea06`): synthetic PEM header test fixture (`-----BEGIN RSA PRIVATE KEY-----`) testing sensitive-file classification.
  - `med-sim-thesis` (`reports/research/*.md`, `thesis_app/static/assets/index-*.css|js`): CSS utility class tokens and prose identifiers (`sk-background`, `sk-management`, `sk-image-link`, `sk-categorize`), not API keys.
  - `polar-browser-app` (commit `c0f6383`): minified Vite frontend bundle strings, not live API tokens.

*Operational hygiene note:* `memory-game` commits `5,356` `node_modules/` files publicly without a `.gitignore` or `LICENSE`, creating unnecessary supply-chain/footprint noise on the public profile.

---

## Course-Integrity Note (Owner Action Recommended)

Two public **archived** repositories—`INFO-5940-A1` (archived public assignment fork) and `AEM2850` (archived public coursework repository)—remain publicly visible on the `vikramwish` profile. Under repository safety policy (never archive, delete, or change visibility of pre-existing repos), these are left untouched. **Owner recommendation:** review whether institutional course-integrity policies advise making these archived repositories private or removing them.

---

## Immediate Remediation Queue

1. **`openfiles-mac` (PUBLIC, pinned):** Add macOS Swift test CI (`./scripts/test.sh`), MIT `LICENSE`, CI badge, and visual/usage polish in `README.md`.
2. **`tally` (PUBLIC, pinned):** Add Node/Jest/TypeScript CI + static web export, MIT `LICENSE`, GitHub description/topics/homepage, and UI screenshots.
3. **`haven-key` (PRIVATE, red CI & `404` Vercel URL):** Fix the failing mobile nav localization assertion on `main`, pass Playwright/unit CI, and restore the `https://haven-key-4f8z.vercel.app` deployment.
4. **Update profile pins & build next portfolio projects (`project-forge` + `rea-clean-room`)** across distinct languages/domains so all `6` profile pins represent clean, verified public flagships.
