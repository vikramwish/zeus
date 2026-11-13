# Zeus Research Portal & Technical Report — Design System (`DESIGN.md`)

**Direction:** Archival Systems Monograph & Obsidian Execution Terminal  
**Scope:** Visual specification governing `docs/figures/*.svg`, `research/paper/zeus_report.tex`, and the interactive systems workbench (`research/index.html`).

## 1. Visual Language & Spatial Rhythm
Zeus separates untrusted probabilistic reasoning from deterministic host enforcement; the design language mirrors that dual-plane contract visually:
- **Obsidian Execution Masthead (`#0C0F16`):** Grounded technical cover band framing the Zeus bolt emblem, live source metrics, and primary report navigation.
- **Warm Archival Paper Canvas (`#F7F4EC`):** Editorial reading ground alternating with crisp white laboratory plates (`#FFFFFF`) and stone control tracks (`#ECE7DA`) bordered by hairline 1px rules (`#D4CCBA`).
- **Measured Asymmetry:** Left-anchored technical narrative columns paired with wider interactive simulation canvases and fixed-width telemetry pills (`font-variant-numeric: tabular-nums`).

**Forbidden:** Purple-to-blue gradients, Inter-on-white SaaS boilerplate, floating glassmorphic blurs, emoji section markers, or unanchored decorative animations.

## 2. Color Tokens
All interactive surfaces and plates use the shared palette from `research/scripts/build_figures.py`:
- `--paper`: `#F7F4EC` (Primary page canvas)
- `--surface`: `#FFFFFF` (Laboratory plate / card fill)
- `--panel`: `#ECE7DA` (Inset control surface / metric track)
- `--rule`: `#D4CCBA` (Card & divider borders)
- `--hair`: `#EAE4D5` (Subtle grid hairlines)
- `--ink`: `#15181E` (Primary obsidian ink)
- `--graphite`: `#4A505A` (Secondary prose, WCAG AA >= 4.5:1 on `--paper` and `--surface`)
- `--muted`: `#5E646E` (Captions & table labels, accessible contrast on `--paper`)
- `--bolt`: `#D93B21` / `--bolt-text`: `#B82D16` (Zeus vermilion accent, capability gates, rejections)
- `--teal`: `#156654` / `--teal-soft`: `#EEF6F3` (Seatbelt / bwrap trust anchor, verified pass)
- `--slate`: `#28527D` / `--slate-soft`: `#EFF4F9` (Parallel workers, UCB exploration, worker handoffs)
- `--ochre`: `#8F5706` / `--ochre-soft`: `#FCF5E8` (Transport/timeout infrastructure failures, zero router update)
- `--night`: `#0C0F16`, `--night-card`: `#141923`, `--night-line`: `#283040`, `--night-text`: `#ECEFF4`, `--night-muted`: `#9BA6B4`

## 3. Typography & Hierarchy
- **Editorial Display / Section Titles (`--font-serif`):** `'Iowan Old Style', 'Charter', 'Palatino Linotype', Palatino, Georgia, serif` for report headings (`clamp(2rem, 3.6vw, 3.1rem)` hero, `1.55rem` section heads) with tight optical tracking (`-0.02em`).
- **UI & Technical Copy (`--font-sans`):** `'SF Pro Text', -apple-system, BlinkMacSystemFont, 'Segoe UI', system-ui, sans-serif` at `15.5px` base with `1.6` line-height.
- **Telemetry, Paths & Schemas (`--font-mono`):** `'SFMono-Regular', ui-monospace, Menlo, Consolas, monospace` with `tabular-nums` for durations, SHA-256 bindings, file scopes, and UCB scores.

## 4. Accessibility, Responsiveness & Motion
- **Contrast:** Every body, label, pill, and dark-terminal token satisfies WCAG 2.1 AA (`>= 4.5:1` normal text, `>= 3:1` UI borders/graphics).
- **Keyboard & Focus:** Clear `2px solid #D93B21` focus-visible rings on tabs, sliders, editors, and buttons; semantic `<header>`, `<nav>`, `<main>`, `<section>`, `<footer>` landmarks and `aria-live="polite"` simulator summaries.
- **Breakpoints:** Desktop `>= 1040px` (two-column interactive workbench), tablet `720px–1039px`, and single-column mobile (`390px`) with zero horizontal scroll (`scrollWidth <= innerWidth`) and horizontally scrollable tables/Gantt wrappers where needed.
- **Motion:** Subtle `160ms ease` border/tab transitions only; disabled automatically under `@media (prefers-reduced-motion: reduce)`.

