#!/usr/bin/env python3
"""Generate publication-grade SVG figures for the Zeus technical report, README, and portal.

All figures are hand-laid vector SVG generated reproducibly from verified project data:
* ``research/data/field_runs.json``  scrubbed empirical trials (14 local Qwen-9B runs);
* ``tests/*.rs`` and ``src/*.rs``    static count of all #[test] items across the codebase;
* ``src/policy.rs`` semantics         re-implemented in :func:`choose` for policy verification.

Schematic diagrams explicitly state their illustrative scope on the plate.
Standard library only::

    python3 research/scripts/build_figures.py          # writes docs/figures/*.svg
    python3 research/scripts/build_figures.py --check  # exits non-zero if SVGs are stale
"""

from __future__ import annotations

import datetime as dt
import json
import math
import re
import sys
from pathlib import Path
from xml.sax.saxutils import escape

ROOT = Path(__file__).resolve().parents[2]
OUT = ROOT / "docs" / "figures"
DATA = ROOT / "research" / "data"

# ---------------------------------------------------------------- Design System
PAPER = "#F7F4EC"       # Warm archival paper
CARD_BG = "#FFFFFF"     # Crisp elevated card
PANEL = "#ECE7DA"       # Stone secondary panel
INK = "#15181E"         # Obsidian primary ink
GRAPHITE = "#525760"    # Secondary body ink
MUTED = "#7D838E"       # Muted captions & rules
RULE = "#D4CCBA"        # Card & border stroke
HAIR = "#EAE4D5"        # Subtle grid hairline

BOLT = "#D93B21"        # Vermilion: Zeus bolt, capability boundary, rejections
BOLT_SOFT = "#FDF1EE"
TEAL = "#156654"        # Emerald: Trust anchor, Seatbelt/bwrap, verified pass
TEAL_SOFT = "#EEF6F3"
OCHRE = "#B5760D"       # Amber: Infrastructure errors / timeouts (zero reward)
OCHRE_SOFT = "#FCF5E8"
SLATE = "#28527D"       # Cobalt: Parallel workers, UCB exploration, handoffs
SLATE_SOFT = "#EFF4F9"

NIGHT = "#0C0F16"       # Cover & dark terminal card
NIGHT_CARD = "#141923"
NIGHT_LINE = "#283040"
NIGHT_TEXT = "#ECEFF4"
NIGHT_MUTED = "#8F9AA8"

SERIF = "'Iowan Old Style', 'Charter', 'Palatino Linotype', Palatino, Georgia, serif"
SANS = "-apple-system, BlinkMacSystemFont, 'SF Pro Display', 'Segoe UI', Helvetica, Arial, sans-serif"
MONO = "'SFMono-Regular', ui-monospace, Menlo, Consolas, 'Liberation Mono', monospace"

REPORT_ID = "ZEUS · TECHNICAL REPORT TR-01"
LOCAL_TZ = dt.timezone(dt.timedelta(hours=11))  # Commits and field runs occurred in AEDT


# ---------------------------------------------------------------- SVG Primitives
def t(x, y, s, size=15, fill=INK, family=SERIF, weight="normal", anchor="start",
      style="normal", ls=0, extra=""):
    return (
        f'<text x="{x:.1f}" y="{y:.1f}" font-family="{family}" font-size="{size}" '
        f'fill="{fill}" font-weight="{weight}" text-anchor="{anchor}" '
        f'font-style="{style}" letter-spacing="{ls}" {extra}>{escape(str(s))}</text>'
    )


def sans(x, y, s, size=13, fill=INK, weight="500", anchor="start", ls=0, extra=""):
    return t(x, y, s, size, fill, SANS, weight, anchor, "normal", ls, extra)


def mono(x, y, s, size=12, fill=GRAPHITE, weight="normal", anchor="start", ls=0.4, extra=""):
    return t(x, y, s, size, fill, MONO, weight, anchor, "normal", ls, extra)


def rect(x, y, w, h, fill="none", stroke=INK, sw=1.2, rx=0, extra=""):
    return (
        f'<rect x="{x:.1f}" y="{y:.1f}" width="{w:.1f}" height="{h:.1f}" rx="{rx}" '
        f'fill="{fill}" stroke="{stroke}" stroke-width="{sw}" {extra}/>'
    )


def line(x1, y1, x2, y2, stroke=INK, sw=1.2, extra=""):
    return (
        f'<line x1="{x1:.1f}" y1="{y1:.1f}" x2="{x2:.1f}" y2="{y2:.1f}" '
        f'stroke="{stroke}" stroke-width="{sw}" {extra}/>'
    )


def path(d, stroke=INK, sw=1.2, fill="none", extra=""):
    return f'<path d="{d}" fill="{fill}" stroke="{stroke}" stroke-width="{sw}" {extra}/>'


def circle(cx, cy, r, fill=INK, stroke="none", sw=1.2, extra=""):
    return f'<circle cx="{cx:.1f}" cy="{cy:.1f}" r="{r}" fill="{fill}" stroke="{stroke}" stroke-width="{sw}" {extra}/>'


def pill(x, y, w, h, label, bg=PANEL, stroke=RULE, fg=INK, size=10.5, weight="bold", ls=0.6):
    return (
        rect(x, y, w, h, bg, stroke, 1, rx=h / 2)
        + mono(x + w / 2, y + h / 2 + size * 0.34, label, size, fg, weight, anchor="middle", ls=ls)
    )


def defs(*extra):
    markers = "".join(
        f'<marker id="arrow-{name}" viewBox="0 0 10 10" refX="8.5" refY="5" markerWidth="6.5" '
        f'markerHeight="6.5" orient="auto-start-reverse"><path d="M0 1L9 5L0 9z" fill="{color}"/></marker>'
        for name, color in (
            ("ink", INK), ("bolt", BOLT), ("teal", TEAL), ("ochre", OCHRE),
            ("slate", SLATE), ("muted", MUTED), ("cream", NIGHT_TEXT),
        )
    )
    patterns = (
        f'<pattern id="grid-light" width="20" height="20" patternUnits="userSpaceOnUse">'
        f'<path d="M20 0H0V20" fill="none" stroke="{HAIR}" stroke-width="0.8"/></pattern>'
        f'<pattern id="hatch-amber" width="8" height="8" patternUnits="userSpaceOnUse" patternTransform="rotate(45)">'
        f'<line x1="0" y1="0" x2="0" y2="8" stroke="{OCHRE}" stroke-opacity="0.28" stroke-width="2"/></pattern>'
        f'<pattern id="hatch-model" width="10" height="10" patternUnits="userSpaceOnUse" patternTransform="rotate(45)">'
        f'<line x1="0" y1="0" x2="0" y2="10" stroke="{RULE}" stroke-opacity="0.35" stroke-width="1.4"/></pattern>'
    )
    filters = (
        '<filter id="shadow-card" x="-4%" y="-4%" width="108%" height="112%" filterUnits="userSpaceOnUse">'
        '<feDropShadow dx="0" dy="2" stdDeviation="3.5" flood-color="#15181E" flood-opacity="0.06"/>'
        '</filter>'
        '<filter id="shadow-bolt" x="-30%" y="-30%" width="160%" height="160%">'
        '<feDropShadow dx="0" dy="0" stdDeviation="9" flood-color="#D93B21" flood-opacity="0.45"/>'
        '</filter>'
    )
    return f"<defs>{markers}{patterns}{filters}{''.join(extra)}</defs>"


def journal_plate(n, title, w, h, body, subtitle="", note=""):
    parts = [
        f'<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}" '
        f'role="img" aria-labelledby="t{n} d{n}">',
        f'<title id="t{n}">Figure {n}. {escape(title)}</title>',
        f'<desc id="d{n}">{escape(subtitle or title)}</desc>',
        defs(),
        rect(0, 0, w, h, PAPER, "none", 0),
        rect(10, 10, w - 20, h - 20, "none", RULE, 1, rx=10),
        rect(36, 30, 64, 24, BOLT, "none", 0, rx=4),
        mono(68, 46, f"FIG. {n}", 11.5, "#FFFFFF", "bold", anchor="middle", ls=1.4),
        t(114, 49, title, 21.5, INK, weight="bold"),
        mono(w - 36, 46, REPORT_ID, 11, MUTED, anchor="end", ls=1.3),
        line(36, 66, w - 36, 66, INK, 1.2),
        line(36, 69.5, w - 36, 69.5, RULE, 0.7),
        body,
    ]
    if note:
        parts.append(line(36, h - 42, w - 36, h - 42, RULE, 0.8))
        parts.append(mono(36, h - 22, note, 11, GRAPHITE, ls=0.3))
    parts.append("</svg>")
    return "\n".join(parts) + "\n"


def cap_gate(cx, cy, color=BOLT):
    s = 6.5
    return path(f"M{cx} {cy - s}L{cx + s} {cy}L{cx} {cy + s}L{cx - s} {cy}Z", color, 1.5, CARD_BG)


# ---------------------------------------------------------------- Data Loaders
def load_trials():
    payload = json.loads((DATA / "field_runs.json").read_text(encoding="utf-8"))
    return payload["trials"], payload.get("final_policies", {})


TEST_GROUPS = [
    ("Model Protocol, Retries & Compaction", [
        "agent_protocol", "providers", "provider_config", "provider_cli",
        "model_timeout", "model_budget", "model_retry", "model_truncation", "context_compaction",
    ]),
    ("Sandboxes, Verifier & Interruption", [
        "process_safety", "verifier_integrity", "reconstruction_contract",
        "sandbox_backend", "interruption", "src/process",
    ]),
    ("Workspace, Secret Guard & Output", [
        "workspace_safety", "workspace_tools", "workspace_concurrency",
        "project_context", "output_compaction", "secret_guard", "snapshot_depth",
        "src/workspace", "src/output",
    ]),
    ("Plugins, MCP & Analyzers", [
        "plugin_bundles", "plugin_runtime", "plugin_deadline", "plugin_example",
        "mcp_protocol", "rea_live", "src/runtime",
    ]),
    ("Routing, Judge & Audit Ledger", [
        "policy_learning", "evidence_ledger", "shadow_judge", "audit_trace",
    ]),
    ("Parallel Orchestration & Handoffs", [
        "parallel_execution", "parallel_coding", "planner_contract", "strategy_config",
    ]),
    ("Autonomy, Job Limits & Config", [
        "autonomy_runtime", "autonomy_config", "config_safety", "job_limits",
    ]),
    ("OAuth & Subscription Auth", [
        "auth_flow", "auth_cli",
    ]),
    ("CLI, Retained Sessions & Runtime", [
        "cli_flow", "session_cli", "runtime_features",
    ]),
]


def count_tests():
    counts = {}
    for path_ in sorted((ROOT / "tests").glob("*.rs")) + sorted((ROOT / "src").glob("*.rs")):
        text = path_.read_text(encoding="utf-8")
        n = len(re.findall(r"#\[test\]", text))
        ignored = len(re.findall(r"#\[ignore", text))
        if n:
            key = path_.stem if path_.parent.name == "tests" else f"src/{path_.stem}"
            counts[key] = (n, ignored)
    groups = []
    seen = set()
    for label, files in TEST_GROUPS:
        present = [f for f in files if f in counts]
        seen.update(present)
        total = sum(counts[f][0] for f in present)
        ignored = sum(counts[f][1] for f in present)
        groups.append((label, total, ignored, present))
    rest = [f for f in counts if f not in seen]
    if rest:
        groups.append(("Other Modules", sum(counts[f][0] for f in rest), sum(counts[f][1] for f in rest), rest))
    groups.sort(key=lambda g: -g[1])
    return groups, counts


def count_rust_loc():
    total = 0
    files = 0
    for p in sorted((ROOT / "src").glob("*.rs")):
        files += 1
        total += len(p.read_text(encoding="utf-8").splitlines())
    return files, total


def choose(direct, parallel):
    """Exact mirror of src/policy.rs::Policy::choose for one context."""
    if direct["trials"] == 0:
        return "direct", "direct untried"
    if parallel["trials"] == 0:
        return "parallel", "parallel untried"
    total = direct["trials"] + parallel["trials"]

    def score(arm):
        return arm["successes"] / arm["trials"] + math.sqrt(2 * math.log(total) / arm["trials"])

    sd, sp = score(direct), score(parallel)
    if sp > sd:
        return "parallel", f"UCB {sp:.2f} > {sd:.2f}"
    return "direct", ("tie" if abs(sp - sd) < 1e-12 else "UCB") + f" {sd:.2f}"


# ================================================================ FIGURE 0: Cover
def fig_cover():
    w, h = 1200, 610
    groups, counts = count_tests()
    total_tests = sum(g[1] for g in groups)
    test_files = len(counts)
    src_modules, src_loc = count_rust_loc()

    out = [
        f'<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}" '
        f'role="img" aria-labelledby="ct cd">',
        '<title id="ct">Zeus: Bounded Concurrency for Verifiable Agentic Coding</title>',
        '<desc id="cd">Cover plate. A validated plan feeds three file-scoped workers joined before integration and external verification.</desc>',
        defs(
            f'<pattern id="night-dots" width="24" height="24" patternUnits="userSpaceOnUse">'
            f'<circle cx="2" cy="2" r="1" fill="{NIGHT_LINE}"/></pattern>',
            f'<linearGradient id="bolt-grad" x1="0" y1="0" x2="1" y2="1">'
            f'<stop offset="0%" stop-color="#FF5A3C"/><stop offset="100%" stop-color="{BOLT}"/></linearGradient>'
        ),
        rect(0, 0, w, h, NIGHT, "none", 0),
        rect(600, 0, 600, h, "url(#night-dots)", "none", 0),
        rect(10, 10, w - 20, h - 20, "none", NIGHT_LINE, 1, rx=10),

        # Top header rule
        mono(44, 44, "TECHNICAL REPORT TR-01  ·  SYSTEMS & VERIFICATION", 11.5, BOLT, "bold", ls=1.8),
        mono(w - 44, 44, "RUST 2021  ·  SEATBELT / BUBBLEWRAP  ·  STANDALONE RUNTIME", 11.5, NIGHT_MUTED, anchor="end", ls=1.3),
        line(44, 60, w - 44, 60, NIGHT_LINE, 1),
    ]

    # Left Column: Brand + Value Proposition
    out.append(path("M54 106H104L78 144H104L44 210L62 160H36Z", "none", 0, "url(#bolt-grad)", 'filter="url(#shadow-bolt)"'))
    out.append(t(120, 198, "Zeus", 110, NIGHT_TEXT, family=SANS, weight="bold", ls=-3))
    out.append(t(46, 260, "Bounded concurrency for", 33, NIGHT_TEXT, style="italic"))
    out.append(t(46, 302, "verifiable agentic coding.", 33, NIGHT_TEXT, style="italic"))
    out.append(sans(46, 344, "Workers run concurrently inside disjoint file scopes.", 15.5, NIGHT_MUTED))
    out.append(sans(46, 368, "Authority, sandboxing, and policy rewards stay runtime-owned.", 15.5, NIGHT_MUTED))

    # 4 Compact Verified Stat Cards
    metrics = [
        ("≤3", "WORKERS", "Work-conserving"),
        ("2–12", "TASKS", "Disjoint scopes"),
        (f"{total_tests}", "TESTS", f"{test_files} Rust suites"),
        (f"{src_loc:,}", "RUST LOC", f"{src_modules} core modules"),
    ]
    for i, (val, label, sub) in enumerate(metrics):
        cx = 46 + i * 132
        out.append(rect(cx, 412, 122, 86, NIGHT_CARD, NIGHT_LINE, 1, rx=7))
        out.append(t(cx + 12, 452, val, 30, NIGHT_TEXT, family=SANS, weight="bold"))
        out.append(mono(cx + 12, 472, label, 9.5, BOLT, "bold", ls=1))
        out.append(sans(cx + 12, 488, sub, 9.5, NIGHT_MUTED))

    out.append(mono(46, 548, "github.com/vikramwish/zeus", 12, NIGHT_MUTED, ls=0.8))
    out.append(rect(285, 530, 150, 24, "#102822", TEAL, 1, rx=12))
    out.append(mono(360, 546, "HEAD · 0692fa6+", 10.5, "#6EE7B7", "bold", anchor="middle", ls=0.8))

    # Right Column: Execution Topology DAG
    cx = 892
    out.append(rect(cx - 115, 92, 230, 52, NIGHT_CARD, NIGHT_LINE, 1.2, rx=7))
    out.append(mono(cx, 114, "VALIDATED PLAN", 12.5, NIGHT_TEXT, "bold", anchor="middle", ls=1.6))
    out.append(sans(cx, 132, "JSON ≤64 KiB · 8 KiB/task context", 10.5, NIGHT_MUTED, anchor="middle"))

    xs = [708, 892, 1076]
    scopes = ["src/lexer.rs", "src/parser.rs", "tests/parser.rs"]
    durs = [0.46, 1.0, 0.64]

    for i, (wx, scope, dur) in enumerate(zip(xs, scopes, durs)):
        out.append(path(f"M{cx} 144V176H{wx}V206", NIGHT_LINE, 1.5))
        is_crit = (i == 1)
        bcol = BOLT if is_crit else NIGHT_LINE
        out.append(rect(wx - 76, 206, 152, 72, NIGHT_CARD, bcol, 1.6 if is_crit else 1.1, rx=7))
        out.append(mono(wx, 230, f"WORKER 0{i+1}", 12, NIGHT_TEXT, "bold", anchor="middle", ls=1.4))
        out.append(mono(wx, 248, scope, 10.5, "#7EE787" if is_crit else NIGHT_MUTED, anchor="middle", ls=0.2))
        out.append(line(wx - 54, 264, wx + 54, 264, NIGHT_LINE, 3.5))
        out.append(line(wx - 54, 264, wx - 54 + 108 * dur, 264, BOLT if is_crit else SLATE, 3.5))
        out.append(path(f"M{wx} 278V318", NIGHT_LINE, 1.5))

    out.append(line(676, 318, 1108, 318, NIGHT_TEXT, 2.5))
    out.append(rect(840, 307, 104, 22, NIGHT, NIGHT_TEXT, 1.1, rx=4))
    out.append(mono(892, 322, "JOIN ALL", 10.5, NIGHT_TEXT, "bold", anchor="middle", ls=1.4))

    out.append(path(f"M{cx} 329V364", NIGHT_LINE, 1.5))
    out.append(rect(cx - 115, 364, 230, 52, NIGHT_CARD, NIGHT_LINE, 1.2, rx=7))
    out.append(mono(cx, 386, "INTEGRATOR", 12.5, NIGHT_TEXT, "bold", anchor="middle", ls=1.6))
    out.append(sans(cx, 404, "8 KiB/worker summaries · MCP/Plugins", 10.5, NIGHT_MUTED, anchor="middle"))

    out.append(path(f"M{cx} 416V450", BOLT, 1.8, extra='marker-end="url(#arrow-bolt)"'))
    out.append(rect(cx - 118, 452, 236, 58, "#0F201D", TEAL, 2, rx=8))
    out.append(rect(cx - 123, 447, 246, 68, "none", TEAL, 0.8, rx=10))
    out.append(mono(cx, 478, "EXTERNAL VERIFIER", 13, "#6EE7B7", "bold", anchor="middle", ls=1.8))
    out.append(sans(cx, 497, "Seatbelt / bwrap · UCB1 Reward Gate", 10.5, NIGHT_MUTED, anchor="middle"))

    out.append(path(
        f"M{cx} 144L{cx-10} 170L{cx+10} 182L{cx} 206"
        f"M{cx} 278L{cx+9} 294L{cx-9} 302L{cx} 307",
        BOLT, 2.2, "none", extra='stroke-linejoin="miter"'
    ))

    out.append(mono(w - 44, 548, "FIG. 0 · CONCEPTUAL TOPOLOGY (CRITICAL PATH IN VERMILION)", 10.5, NIGHT_MUTED, anchor="end", ls=0.8))
    out.append("</svg>")
    return "\n".join(out) + "\n"


# ================================================================ FIGURE 1: System Architecture
def fig_system():
    w, h = 1200, 710
    b = []

    # Trusted Host Plane
    b.append(rect(36, 86, w - 72, 224, CARD_BG, RULE, 1, rx=8, extra='filter="url(#shadow-card)"'))
    b.append(pill(52, 100, 178, 24, "TRUSTED HOST PLANE", TEAL_SOFT, TEAL, TEAL, 10.5, "bold", 1.2))
    b.append(sans(242, 117, "Owns candidate snapshots, sandboxed execution, immutable audit ledger, and router state", 12.5, GRAPHITE))

    # Untrusted Model Zone
    b.append(rect(36, 366, w - 72, 250, CARD_BG, RULE, 1, rx=8, extra='filter="url(#shadow-card)"'))
    b.append(rect(36, 366, w - 72, 250, "url(#hatch-model)", "none", 0, rx=8))
    b.append(pill(52, 380, 190, 24, "UNTRUSTED MODEL ZONE", BOLT_SOFT, BOLT, BOLT, 10.2, "bold", 1.1))
    b.append(sans(276, 397, "Model outputs & tool calls are untrusted data, never capability approval", 11.8, GRAPHITE))

    # Capability Boundary Line — placed cleanly clear of x=260 and x=742 arrows
    b.append(line(36, 338, w - 36, 338, BOLT, 1.6, 'stroke-dasharray="7 5"'))
    b.append(rect(330, 325, 370, 26, CARD_BG, BOLT, 1.2, rx=13))
    b.append(mono(515, 342, "ToolHost CAPABILITY BOUNDARY (AT DISPATCH)", 10.5, BOLT, "bold", anchor="middle", ls=0.8))

    # Top Row Host Nodes
    ty, nh = 152, 66
    host_nodes = [
        (56, ty, 140, nh, "job.json", "task · scopes · argv", INK, PANEL),
        (222, ty, 144, nh, "validate + lock", "run.lock · cancel", INK, PANEL),
        (392, ty, 162, nh, "snapshot", "secret-guard · SHA256", INK, PANEL),
        (650, ty, 184, nh, "external verifier", "Seatbelt / bwrap", TEAL, TEAL_SOFT),
        (874, ty, 142, nh, "evidence", "events.jsonl · ts_ms", INK, PANEL),
        (1046, ty, 102, nh, "router", "UCB1 + Judge", INK, PANEL),
    ]
    for x, y, nw, nh_, title, sub, scol, fcol in host_nodes:
        is_ver = title == "external verifier"
        if is_ver:
            b.append(rect(x - 4, y - 4, nw + 8, nh_ + 8, "none", TEAL, 0.8, rx=8))
        b.append(rect(x, y, nw, nh_, fcol, scol, 2 if is_ver else 1.2, rx=6))
        b.append(t(x + nw / 2, y + 27, title, 15.5, scol, family=SANS, weight="bold", anchor="middle"))
        b.append(mono(x + nw / 2, y + 48, sub, 10.2, GRAPHITE, anchor="middle", ls=0.1))

    # Annotations in Host Plane
    b.append(mono(1097, ty - 14, "infra/cancel/timeout", 10, OCHRE, "bold", anchor="middle", ls=0.2))
    b.append(mono(1097, ty + nh + 20, "zero reward update", 10, OCHRE, anchor="middle", ls=0.2))
    b.append(mono(1097, ty + nh + 35, "shadow judge: passive", 10, SLATE, anchor="middle", ls=0.2))

    # Host-privileged extensions callout placed cleanly above the snapshot route (y=238..280)
    b.append(rect(56, 238, 392, 42, PANEL, RULE, 1, rx=6))
    b.append(mono(68, 255, "HOST-PRIVILEGED EXTENSIONS (NOT SANDBOXED)", 9.8, INK, "bold", ls=0.4))
    b.append(sans(68, 271, "MCP & Plugins connect to Integrator only (disabled under write_paths)", 11, GRAPHITE))

    # Bottom Row Model Nodes (planner centered at x=260 cleanly drops through x=242..282 gutter!)
    my = 448
    b.append(rect(184, my, 152, nh, CARD_BG, INK, 1.3, rx=6))
    b.append(t(260, my + 27, "planner", 16, INK, family=SANS, weight="bold", anchor="middle"))
    b.append(mono(260, my + 48, "≤64 KiB + 8 KiB ctx", 10.2, GRAPHITE, anchor="middle", ls=0.1))

    for k in (2, 1):
        b.append(rect(384 + 7 * k, my - 7 * k, 174, nh, CARD_BG, MUTED, 1, rx=6))
    b.append(rect(384, my, 174, nh, SLATE_SOFT, SLATE, 1.6, rx=6))
    b.append(t(471, my + 27, "workers ×3", 16, SLATE, family=SANS, weight="bold", anchor="middle"))
    b.append(mono(471, my + 48, "disjoint file scopes", 10.5, GRAPHITE, anchor="middle", ls=0.1))

    jx = 610
    b.append(line(jx, my - 22, jx, my + nh + 18, INK, 3.5))
    b.append(rect(jx - 32, my - 42, 64, 18, INK, "none", 0, rx=4))
    b.append(mono(jx, my - 29, "JOIN ALL", 9.5, "#FFFFFF", "bold", anchor="middle", ls=0.8))

    b.append(rect(654, my, 176, nh, CARD_BG, INK, 1.3, rx=6))
    b.append(t(742, my + 27, "integrator", 16, INK, family=SANS, weight="bold", anchor="middle"))
    b.append(mono(742, my + 48, "≤8 KiB worker handoffs", 10.5, GRAPHITE, anchor="middle", ls=0.1))

    b.append(rect(910, my, 156, nh, BOLT_SOFT, BOLT, 1.4, rx=6))
    b.append(t(988, my + 27, "repair loop", 16, BOLT, family=SANS, weight="bold", anchor="middle"))
    b.append(mono(988, my + 48, "≤ max_repairs", 10.5, GRAPHITE, anchor="middle", ls=0.1))

    b.append(mono(471, my + nh + 26, "read · search · exact edit", 10.5, TEAL, "bold", anchor="middle", ls=0.2))
    b.append(mono(471, my + nh + 43, "no exec · no MCP · no shell · no recurse", 10.5, BOLT, "bold", anchor="middle", ls=0.2))

    # Edges
    ai = 'marker-end="url(#arrow-ink)"'
    at = 'marker-end="url(#arrow-teal)"'
    ab = 'marker-end="url(#arrow-bolt)"'

    b.append(line(196, ty + nh / 2, 219, ty + nh / 2, INK, 1.4, ai))
    b.append(line(366, ty + nh / 2, 389, ty + nh / 2, INK, 1.4, ai))
    # Snapshot -> Planner routed cleanly at y=296 below HOST-PRIVILEGED card, dropping down gutter x=260
    b.append(path(f"M473 {ty + nh}V296H260V{my - 4}", INK, 1.5, extra=ai))
    b.append(cap_gate(260, 338))
    b.append(line(336, my + nh / 2, 380, my + nh / 2, INK, 1.4, ai))
    b.append(line(572, my + nh / 2, jx - 4, my + nh / 2, INK, 1.4, ai))
    b.append(line(jx + 4, my + nh / 2, 650, my + nh / 2, INK, 1.4, ai))
    # Integrator -> External Verifier cleanly clear right of boundary pill
    b.append(line(742, my, 742, ty + nh + 8, INK, 1.6, ai))
    b.append(cap_gate(742, 338))
    # Verifier PASS -> Evidence -> Router
    b.append(line(834, ty + nh / 2, 870, ty + nh / 2, TEAL, 1.8, at))
    b.append(mono(852, ty + nh / 2 - 14, "PASS", 10, TEAL, "bold", anchor="middle"))
    b.append(line(1016, ty + nh / 2, 1042, ty + nh / 2, TEAL, 1.8, at))
    # Verifier FAIL -> Repair -> Integrator
    b.append(path(f"M802 {ty + nh}V276H988V{my - 4}", BOLT, 1.6, extra=ab))
    b.append(mono(1000, 294, "FAIL", 10.5, BOLT, "bold"))
    b.append(cap_gate(988, 338))
    b.append(line(910, my + nh / 2, 834, my + nh / 2, BOLT, 1.6, ab))

    # Bottom Legend Strip
    ly = 644
    b.append(line(44, ly, 78, ly, INK, 1.5, ai))
    b.append(sans(88, ly + 4, "Verified control / data flow", 12, INK))
    b.append(cap_gate(295, ly))
    b.append(sans(312, ly + 4, "Capability dispatch gate", 12, INK))
    b.append(rect(495, ly - 9, 22, 18, TEAL_SOFT, TEAL, 1.8, rx=3))
    b.append(sans(526, ly + 4, "External trust anchor (outside candidate)", 12, TEAL))
    b.append(line(810, ly, 844, ly, BOLT, 1.5, ab))
    b.append(sans(854, ly + 4, "Bounded repair path (≤ max_repairs)", 12, BOLT))

    return journal_plate(
        1, "System Architecture & Dual Capability Boundary", w, h, "\n".join(b),
        subtitle="Trusted host execution plane separated from untrusted model zone by ToolHost capability checks",
        note="Source: src/runtime.rs, src/agent.rs, src/process.rs, src/plugins.rs. Model outputs never grant filesystem or execution authority."
    )


# ================================================================ FIGURE 2: Scheduler
def schedule_tasks(durations, slots, batch):
    out = []
    if batch:
        clock = 0
        for i in range(0, len(durations), slots):
            group = durations[i:i + slots]
            for s, d in enumerate(group):
                out.append((i + s, s, clock, clock + d))
            clock += max(group)
        return out
    free = [0] * slots
    for task, d in enumerate(durations):
        s = min(range(slots), key=lambda k: (free[k], k))
        out.append((task, s, free[s], free[s] + d))
        free[s] += d
    return out


def fig_scheduler():
    w, h = 1200, 660
    durations = [8, 3, 4, 2, 5, 3, 4]
    slots = 3
    unit = 46
    x0 = 188
    b = []

    configs = [
        ("(a) Synchronous Batch Barrier", True, 92),
        ("(b) Zeus Work-Conserving FIFO (agent::run_tasks)", False, 322),
    ]

    summaries = []
    for title, is_batch, y0 in configs:
        sched = schedule_tasks(durations, slots, is_batch)
        makespan = max(e for *_, e in sched)
        idle = slots * makespan - sum(durations)
        summaries.append((makespan, idle, sched))

        b.append(rect(36, y0, w - 72, 206, CARD_BG, RULE, 1, rx=8, extra='filter="url(#shadow-card)"'))
        b.append(t(56, y0 + 32, title, 18, INK, family=SANS, weight="bold"))

        if is_batch:
            b.append(pill(w - 356, y0 + 16, 148, 26, f"MAKESPAN: {makespan} u", PANEL, RULE, GRAPHITE, 11))
            b.append(pill(w - 196, y0 + 16, 142, 26, f"IDLE: {idle} slot-u", OCHRE_SOFT, OCHRE, OCHRE, 11))
        else:
            b.append(pill(w - 384, y0 + 16, 176, 26, f"MAKESPAN: {makespan} u (-29%)", TEAL_SOFT, TEAL, TEAL, 11))
            b.append(pill(w - 196, y0 + 16, 142, 26, f"IDLE: {idle} slot-u", TEAL_SOFT, TEAL, TEAL, 11))

        top = y0 + 58
        rh, gap = 34, 9
        for s in range(slots):
            ry = top + s * (rh + gap)
            b.append(mono(x0 - 18, ry + 22, f"Worker {s + 1}", 12, GRAPHITE, "bold", anchor="end"))
            b.append(rect(x0, ry, 18 * unit, rh, "#FAF9F6", HAIR, 0.8, rx=4))
            b.append(rect(x0, ry, makespan * unit, rh, "url(#hatch-amber)", "none", 0, rx=4))

        for task, s, start, end in sched:
            ry = top + s * (rh + gap)
            is_slow = (task == 0)
            fill_c = INK if is_slow else (SLATE if not is_batch else CARD_BG)
            text_c = "#FFFFFF" if (is_slow or not is_batch) else INK
            stroke_c = INK if is_slow else (SLATE if not is_batch else GRAPHITE)
            bx = x0 + start * unit + 1.5
            bw = (end - start) * unit - 3
            b.append(rect(bx, ry + 1, bw, rh - 2, fill_c, stroke_c, 1.3, rx=5))
            label = f"T{task} ({end - start}u)" if bw > 70 else f"T{task}"
            b.append(mono(bx + bw / 2, ry + 21, label, 11.5, text_c, "bold", anchor="middle", ls=0.1))

        if is_batch:
            clock = 0
            for i in range(0, len(durations), slots):
                clock += max(durations[i:i + slots])
                if clock < makespan:
                    bx = x0 + clock * unit
                    b.append(line(bx, top - 8, bx, top + 3 * (rh + gap) - 2, BOLT, 1.6, 'stroke-dasharray="4 3"'))
                    b.append(mono(bx, top - 12, "barrier", 10, BOLT, "bold", anchor="middle"))

        axis_y = top + 3 * (rh + gap) + 6
        b.append(line(x0, axis_y, x0 + 18 * unit, axis_y, RULE, 1))
        for u in range(0, 19, 2):
            b.append(line(x0 + u * unit, axis_y, x0 + u * unit, axis_y + 5, RULE, 1))
            b.append(mono(x0 + u * unit, axis_y + 18, str(u), 10.5, MUTED, anchor="middle"))

    b.append(rect(36, 548, w - 72, 56, CARD_BG, RULE, 1, rx=8))
    b.append(rect(52, 568, 20, 14, "url(#hatch-amber)", OCHRE, 0.8, rx=2))
    b.append(sans(80, 579, "Idle slot capacity", 12, GRAPHITE))
    b.append(rect(202, 568, 20, 14, INK, INK, 1, rx=2))
    b.append(sans(230, 579, "Slow task (T0 = 8u)", 12, INK))

    fifo_sched = summaries[1][2]
    comp_order = [f"T{t}" for t, _, _, _ in sorted(fifo_sched, key=lambda x: (x[3], x[0]))]
    b.append(mono(420, 569, f"Completion order: [{', '.join(comp_order)}]", 10.5, SLATE, "bold", ls=0.2))
    b.append(mono(420, 588, "Returned order:   [T0, T1, T2, T3, T4, T5, T6] (input-order slots; errors close admission)",
                  10.5, TEAL, "bold", ls=0.1))

    return journal_plate(
        2, "Work-Conserving FIFO Admission vs. Batch Barriers", w, h, "\n".join(b),
        subtitle="Gantt simulation showing 29.4% makespan reduction and 68.2% idle waste elimination under bounded 3-worker concurrency",
        note=f"Schematic simulation (durations {durations}, 3 slots). Mirrors src/agent.rs::run_tasks (Mutex queue, scoped thread join)."
    )


# ================================================================ FIGURE 3: Ownership & Handoffs
def fig_ownership():
    w, h = 1200, 670
    b = []
    lw = 548

    b.append(t(36, 102, "(a) Structured Plan & Bounded Worker Handoffs", 17.5, INK, family=SANS, weight="bold"))
    b.append(rect(36, 114, lw, 220, NIGHT, "none", 0, rx=8, extra='filter="url(#shadow-card)"'))
    for i, col in enumerate(("#FF5F56", "#FFBD2E", "#27C93F")):
        b.append(circle(54 + i * 16, 132, 5.2, col))
    b.append(mono(36 + lw / 2, 135, "planner_response.json (≤ 64 KiB)", 11, NIGHT_MUTED, anchor="middle"))
    b.append(line(36, 146, 36 + lw, 146, NIGHT_LINE, 1))

    code_lines = [
        ('{"tasks": [', NIGHT_TEXT),
        ('  {"task": "implement lexer",', "#93C5FD"),
        ('   "write_paths": ["src/lexer.rs"],', "#FCD34D"),
        ('   "context": "Keep UTF-8 offsets; <=8192 B"},', "#6EE7B7"),
        ('  {"task": "parse expressions",', "#93C5FD"),
        ('   "write_paths": ["src/parser.rs"]},', "#FCD34D"),
        ('  {"task": "cover parser invariants",', "#93C5FD"),
        ('   "write_paths": ["tests/parser.rs"]}]}', "#FCD34D"),
    ]
    for i, (code, col) in enumerate(code_lines):
        b.append(mono(52, 168 + i * 20, code, 12.2, col, ls=0, extra='xml:space="preserve"'))

    b.append(t(36, 368, "(b) Whole-Plan Pre-Admission Invariants", 17.5, INK, family=SANS, weight="bold"))
    b.append(rect(36, 380, lw, 232, CARD_BG, RULE, 1, rx=8, extra='filter="url(#shadow-card)"'))

    invariants = [
        ("Nonempty exact-file scope", "Every coding task claims explicit files; no dirs"),
        ("Canonical relative ASCII", "Rejects .. traversal, //, symlinks, non-ASCII"),
        ("Globally disjoint claims", "No duplicate paths or ancestor/descendant overlap"),
        ("Case-insensitive guard", "Rejects ASCII case aliases (SRC/ vs src/)"),
        ("Autonomy subset rule", "All claims ⊆ autonomy.write_paths when set"),
        ("Bounded handoff text", "Optional context ≤ 8192 B/task; untrusted data"),
    ]
    for i, (title, desc) in enumerate(invariants):
        y = 408 + i * 34
        b.append(circle(58, y - 4, 9.5, TEAL))
        b.append(path(f"M54 {y - 4}l3 3l5.5 -6", "#FFFFFF", 2))
        b.append(sans(76, y, title, 13.5, INK, "bold"))
        b.append(sans(262, y, desc, 11.8, GRAPHITE))

    rx_col = 612
    rw = w - rx_col - 36

    b.append(t(rx_col, 102, "(c) Candidate Ownership & Secret Isolation Map", 17.5, INK, family=SANS, weight="bold"))
    b.append(rect(rx_col, 114, rw, 220, CARD_BG, RULE, 1, rx=8, extra='filter="url(#shadow-card)"'))

    tree = [
        (0, "candidate/", None, None, None),
        (1, "src/", None, None, None),
        (2, "lexer.rs", "WORKER 01", SLATE_SOFT, SLATE),
        (2, "parser.rs", "WORKER 02", BOLT_SOFT, BOLT),
        (2, "main.rs", "READ-ONLY", PANEL, GRAPHITE),
        (2, "config.env", "SECRET WITHHELD", OCHRE_SOFT, OCHRE),
        (1, "tests/", None, None, None),
        (2, "parser.rs", "WORKER 03", TEAL_SOFT, TEAL),
    ]
    for i, (depth, name, badge, bg, fg) in enumerate(tree):
        y = 140 + i * 24
        x = rx_col + 22 + depth * 20
        if depth:
            b.append(path(f"M{x - 13} {y - 16}V{y - 4}H{x - 3}", RULE, 1))
        b.append(mono(x, y, name, 13, INK if not name.endswith("/") else MUTED, "bold" if not name.endswith("/") else "normal"))
        if badge:
            bx = rx_col + 222
            b.append(pill(bx, y - 14, 138, 19, badge, bg, fg, fg, 9.8, "bold", 0.6))

    b.append(rect(rx_col + rw - 172, 134, 152, 128, TEAL_SOFT, TEAL, 1.6, rx=6))
    b.append(rect(rx_col + rw - 176, 130, 160, 136, "none", TEAL, 0.7, rx=8))
    b.append(t(rx_col + rw - 96, 162, "External Verifier", 14.5, TEAL, family=SANS, weight="bold", anchor="middle"))
    b.append(mono(rx_col + rw - 96, 186, "outside candidate", 10.5, GRAPHITE, anchor="middle"))
    b.append(mono(rx_col + rw - 96, 203, "and state dir;", 10.5, GRAPHITE, anchor="middle"))
    b.append(mono(rx_col + rw - 96, 220, "never writable", 10.5, TEAL, "bold", anchor="middle"))
    b.append(mono(rx_col + rw - 96, 244, "Seatbelt / bwrap", 10, MUTED, anchor="middle"))

    b.append(t(rx_col, 368, "(d) Fail-Closed Claims (Abort Before Any Worker Starts)", 17.5, INK, family=SANS, weight="bold"))
    b.append(rect(rx_col, 380, rw, 232, CARD_BG, RULE, 1, rx=8, extra='filter="url(#shadow-card)"'))

    violations = [
        ("src/", "Directory is ancestor of src/parser.rs", "ANCESTOR"),
        ("SRC/Parser.rs", "ASCII case alias of src/parser.rs", "CASE ALIAS"),
        ("src/../.env", "Parent traversal + excluded secret", "TRAVERSAL"),
        ("src//lexer.rs", "Non-canonical repeated path separator", "SYNTAX"),
        ("context: 9 KiB", "Exceeds 8192-byte per-task handoff bound", "HANDOFF CAP"),
        ("src/lexer.rs ×2", "Duplicate claim across coding tasks", "DISJOINT"),
    ]
    for i, (claim, reason, badge) in enumerate(violations):
        y = 408 + i * 34
        b.append(circle(rx_col + 22, y - 4, 9.5, BOLT))
        b.append(path(f"M{rx_col + 18.5} {y - 7.5}l7 7M{rx_col + 25.5} {y - 7.5}l-7 7", "#FFFFFF", 1.8))
        b.append(mono(rx_col + 40, y, claim, 12.2, INK, "bold"))
        b.append(sans(rx_col + 178, y, reason, 11.5, GRAPHITE))
        b.append(pill(rx_col + rw - 108, y - 14, 92, 18, badge, BOLT_SOFT, BOLT, BOLT, 9, "bold", 0.5))

    return journal_plate(
        3, "Exact-File Write Scope Isolation & Bounded Planner Handoffs", w, h, "\n".join(b),
        subtitle="Whole-plan validation guarantees disjoint file ownership and fail-closed safety before worker threads spawn",
        note="Source: src/runtime.rs (plan validation & worker dispatch), src/workspace.rs (SHA-256 mutation mutex & secret_like guard)."
    )


# ================================================================ FIGURE 4: Routing & Reward Authority
def fig_routing(trials, policies):
    w, h = 1200, 660
    b = []
    cw = 358
    c1, c2, c3 = 36, 421, 806

    # (a) Deterministic UCB1 Policy
    b.append(t(c1, 102, "(a) Deterministic UCB1 Policy", 17.5, INK, family=SANS, weight="bold"))
    b.append(rect(c1, 114, cw, 490, CARD_BG, RULE, 1, rx=8, extra='filter="url(#shadow-card)"'))

    steps = [
        ("1. Direct untried?", "DIRECT", INK, "Explore direct implementer first"),
        ("2. Parallel untried?", "PARALLEL", SLATE, "Explore 3-worker coding next"),
        ("3. Both arms tried?", "ARGMAX UCB1", TEAL, "Deterministic score; ties → Direct"),
    ]
    for i, (q, act, col, desc) in enumerate(steps):
        y = 136 + i * 74
        b.append(rect(c1 + 16, y, cw - 32, 60, PANEL, RULE, 1, rx=6))
        b.append(sans(c1 + 28, y + 24, q, 13.5, INK, "bold"))
        b.append(pill(c1 + cw - 146, y + 10, 116, 22, act, col, col, "#FFFFFF", 10, "bold", 0.6))
        b.append(sans(c1 + 28, y + 46, desc, 11.5, GRAPHITE))

    b.append(rect(c1 + 16, 368, cw - 32, 118, "#F9F7F2", RULE, 1, rx=6))
    b.append(mono(c1 + 28, 390, "UPPER CONFIDENCE BOUND (src/policy.rs)", 10, MUTED, "bold", ls=0.8))
    b.append(
        f'<text x="{c1 + 28}" y="432" font-family="{SERIF}" font-size="22" fill="{INK}" font-style="italic">'
        f'score(a) = s<tspan font-size="14" dy="5">a</tspan><tspan dy="-5">/n</tspan>'
        f'<tspan font-size="14" dy="5">a</tspan><tspan dy="-5"> + √(2 ln N / n</tspan>'
        f'<tspan font-size="14" dy="5">a</tspan><tspan dy="-5">)</tspan></text>'
    )
    b.append(sans(c1 + 28, 468, "Per context · max 1,024 contexts · atomic save", 11.5, GRAPHITE))

    b.append(rect(c1 + 16, 502, cw - 32, 86, SLATE_SOFT, SLATE, 1, rx=6))
    b.append(mono(c1 + 28, 522, "ZEUS JUDGE (SHADOW-ONLY OBSERVATION)", 10, SLATE, "bold", ls=0.6))
    b.append(sans(c1 + 28, 542, "Logs pre-run features & typed Rank/Abstain", 11.5, INK))
    b.append(sans(c1 + 28, 560, "in shadow-decision.json; never alters UCB1.", 11.5, GRAPHITE))

    # (b) Reward Gate & Field Trial Matrix
    b.append(t(c2, 102, "(b) Verifier Reward Gate (14 Field Runs)", 17.5, INK, family=SANS, weight="bold"))
    b.append(rect(c2, 114, cw, 490, CARD_BG, RULE, 1, rx=8, extra='filter="url(#shadow-card)"'))

    gate_rows = [
        ("Verified Pass", "+1 trial, +1 success", TEAL, TEAL_SOFT),
        ("Verified Fail", "+1 trial, +0 success", INK, PANEL),
        ("Infra / Timeout / Cancel", "ZERO policy update", OCHRE, OCHRE_SOFT),
    ]
    for i, (lab, eff, col, bg) in enumerate(gate_rows):
        y = 136 + i * 48
        b.append(rect(c2 + 16, y, cw - 32, 38, bg, col, 1, rx=6))
        b.append(circle(c2 + 34, y + 19, 6, col))
        b.append(sans(c2 + 48, y + 23, lab, 12.5, INK, "bold"))
        b.append(mono(c2 + cw - 28, y + 23, eff, 10.5, col, "bold", anchor="end"))

    b.append(mono(c2 + 18, 304, "14 REAL TRIALS (research/data/field_runs.json)", 9.5, MUTED, "bold", ls=0.4))

    verified = sum(1 for r in trials if r["status"] == "verified")
    rewarded = sum(1 for r in trials if r["reward_recorded"])
    infra = [r for r in trials if r["status"] != "verified"]
    infra_rewarded = sum(1 for r in infra if r["reward_recorded"])

    for i, r in enumerate(trials):
        col_i, row_i = i % 7, i // 7
        cx = c2 + 42 + col_i * 45
        cy = 342 + row_i * 64
        st = r["status"]
        color = TEAL if st == "verified" else (OCHRE if st != "incomplete" else GRAPHITE)
        fill_c = color if st != "incomplete" else CARD_BG
        if r["strategy"] == "parallel":
            b.append(rect(cx - 11, cy - 11, 22, 22, fill_c, color, 1.8, rx=4))
        else:
            b.append(circle(cx, cy, 11.5, fill_c, color, 1.8))
        if r["reward_recorded"]:
            b.append(circle(cx, cy, 17, "none", INK, 1.4))
        b.append(mono(cx, cy + 30, r["trial"], 9.5, GRAPHITE, "bold", anchor="middle", ls=0))

    b.append(rect(c2 + 16, 466, cw - 32, 122, PANEL, RULE, 1, rx=6))
    b.append(mono(c2 + 28, 488, f"Outer ring = reward recorded ({rewarded}/{verified} verified)", 10.5, INK, "bold", ls=0.2))
    b.append(mono(c2 + 28, 509, f"Error isolation: {infra_rewarded}/{len(infra)} errors touched UCB", 10.5, OCHRE, "bold", ls=0.2))
    b.append(mono(c2 + 28, 532, "○ Direct implementer   □ Parallel workers", 10.5, GRAPHITE, ls=0.2))
    b.append(sans(c2 + 28, 558, "Guarantees network/LLM/sandbox failures never", 11.5, GRAPHITE))
    b.append(sans(c2 + 28, 574, "bias strategy selection toward the surviving arm.", 11.5, GRAPHITE))

    # (c) Persisted Campaign Policies — spacious header with clean right-aligned badge!
    b.append(t(c3, 102, "(c) Persisted Router States", 17.5, INK, family=SANS, weight="bold"))
    b.append(rect(c3, 114, cw, 490, CARD_BG, RULE, 1, rx=8, extra='filter="url(#shadow-card)"'))

    py = 134
    for campaign, contexts in policies.items():
        for context, arms in contexts.items():
            nxt, why = choose(arms["direct"], arms["parallel"])
            b.append(rect(c3 + 16, py, cw - 32, 148, PANEL, RULE, 1, rx=7))
            b.append(mono(c3 + 28, py + 23, campaign.upper(), 11.5, BOLT, "bold", ls=0.8))
            nxt_col = TEAL if nxt == "direct" else SLATE
            b.append(pill(c3 + cw - 144, py + 9, 116, 20, f"NEXT: {nxt.upper()}", CARD_BG, nxt_col, nxt_col, 9.5, "bold", 0.3))
            b.append(mono(c3 + 28, py + 43, f"ctx: {context}", 9.2, GRAPHITE, ls=0))
            b.append(mono(c3 + 28, py + 58, f"rule: {why}", 9.2, nxt_col, "bold", ls=0.1))

            for j, arm_name in enumerate(("direct", "parallel")):
                ay = py + 76 + j * 33
                tr, sc = arms[arm_name]["trials"], arms[arm_name]["successes"]
                rate = (sc / tr) if tr > 0 else 0
                b.append(mono(c3 + 28, ay + 15, arm_name.upper()[:3], 11, INK, "bold"))
                b.append(rect(c3 + 66, ay + 3, 182, 16, "#DFDAD0", "none", 0, rx=4))
                if tr > 0:
                    bar_c = TEAL if arm_name == "direct" else SLATE
                    b.append(rect(c3 + 66, ay + 3, 182 * rate, 16, bar_c, "none", 0, rx=4))
                b.append(mono(c3 + 258, ay + 15, f"{sc}/{tr} ({rate * 100:.0f}%)", 11, INK, "bold"))
            py += 158

    return journal_plate(
        4, "Verifier-Anchored Contextual Bandit Routing", w, h, "\n".join(b),
        subtitle="Deterministic UCB1 policy updates strictly from external verification outcomes",
        note="(b)–(c) extracted verbatim from research/data/field_runs.json. Next choices recomputed with src/policy.rs rules."
    )


# ================================================================ FIGURE 5: Field Trials
COMMITS = [
    ("d121e6b", "planner envelopes", "2026-11-01T23:09:34+11:00", 148, 0),
    ("3267d6f", "model deadlines", "2026-11-01T23:29:46+11:00", 132, -24),
    ("3f7e00a", "denied recovery", "2026-11-01T23:36:53+11:00", 158, 24),
]
CLASS_CODE = {
    "transport": "TRN",
    "planner-format": "PLN",
    "capability-request": "CAP",
    "path-contract": "PTH",
    None: "—",
}


def fig_trials(trials):
    w, h = 1200, 680
    b = []
    b.append(rect(36, 86, w - 72, 536, CARD_BG, RULE, 1, rx=8, extra='filter="url(#shadow-card)"'))

    px0, px1, py0, py1 = 104, 866, 200, 480
    ymax = 1000
    n = len(trials)
    step = (px1 - px0) / (n - 1)

    # Top commit legend strip (y=108, cleanly ABOVE all chart pin badges at y=132..176!)
    b.append(mono(56, 112, "HARDENING COMMITS:", 10.5, GRAPHITE, "bold", ls=0.8))
    lx = 212
    for sha, label, _, _, _ in COMMITS:
        b.append(rect(lx, 97, 64, 21, BOLT_SOFT, BOLT, 1, rx=4))
        b.append(mono(lx + 32, 111, sha, 10.5, BOLT, "bold", anchor="middle", ls=0.3))
        b.append(sans(lx + 72, 112, label, 12, INK, "500"))
        lx += 208

    # Y-axis grid
    for v in range(0, ymax + 1, 250):
        y = py1 - (py1 - py0) * v / ymax
        b.append(line(px0 - 10, y, px1 + 14, y, HAIR if v else RULE, 1))
        b.append(mono(px0 - 18, y + 4, f"{v}s", 10.5, MUTED, anchor="end", ls=0))
    b.append(mono(56, py0 - 20, "APPROX. WALL TIME", 10, GRAPHITE, "bold", ls=0.8))

    # Commit vertical markers with staggered badges (zero collisions!)
    times = [dt.datetime.fromisoformat(r["started_at"]) for r in trials]
    for sha, _, when, tag_y, dx in COMMITS:
        ts = dt.datetime.fromisoformat(when)
        idx = sum(1 for x in times if x < ts)
        frac = (ts - times[idx - 1]) / (times[idx] - times[idx - 1]) if 0 < idx < n else 0.5
        cx = px0 + (idx - 1 + frac) * step
        bx = cx + dx
        b.append(line(cx, tag_y + 20, cx, py1, BOLT, 1.3, 'stroke-dasharray="4 3"'))
        if dx != 0:
            b.append(path(f"M{bx} {tag_y + 18}L{cx} {tag_y + 24}", BOLT, 1.1))
        b.append(rect(bx - 30, tag_y, 60, 18, BOLT_SOFT, BOLT, 1, rx=4))
        b.append(mono(bx, tag_y + 13, sha, 9.5, BOLT, "bold", anchor="middle", ls=0.2))

    # Lollipops
    for i, r in enumerate(trials):
        x = px0 + i * step
        v = r["approx_wall_seconds"]
        st = r["status"]
        color = TEAL if st == "verified" else (OCHRE if st != "incomplete" else GRAPHITE)
        y = py1 - (py1 - py0) * min(v, ymax) / ymax
        b.append(line(x, py1, x, y, color, 2.2))
        fill_c = color if st != "incomplete" else CARD_BG
        if r["strategy"] == "parallel":
            b.append(rect(x - 8.5, y - 8.5, 17, 17, fill_c, color, 2, rx=3))
        else:
            b.append(circle(x, y, 8.5, fill_c, color, 2))
        lab = f"{r['verifier_checks']} ok" if st == "verified" else ("killed" if st == "incomplete" else CLASS_CODE.get(r["failure_class"], "?"))
        # Place T12's "82 ok" cleanly to the right of the adjacent 3f7e00a dashed commit marker
        if r["trial"] == "T12":
            b.append(mono(x + 12, y - 10, lab, 10, color, "bold", anchor="start", ls=0.1))
        else:
            b.append(mono(x, y - 14, lab, 10, color, "bold", anchor="middle", ls=0.1))
        b.append(mono(x, py1 + 20, r["trial"], 11, INK, "bold", anchor="middle", ls=0))

    # Campaign brackets
    groups = []
    for i, r in enumerate(trials):
        if groups and groups[-1][0] == r["campaign"]:
            groups[-1][2] = i
        else:
            groups.append([r["campaign"], i, i])
    for name, a, z in groups:
        xa, xz = px0 + a * step - 14, px0 + z * step + 14
        b.append(path(f"M{xa} {py1 + 32}v6h{xz - xa}v-6", MUTED, 1))
        b.append(mono((xa + xz) / 2, py1 + 52, name, 10.5, GRAPHITE, "bold", anchor="middle"))

    days = []
    for i, x in enumerate(times):
        d = x.astimezone(LOCAL_TZ).strftime("%d %b")
        if not days or days[-1][0] != d:
            days.append([d, i, i])
        else:
            days[-1][2] = i
    for d, a, z in days:
        b.append(mono(px0 + (a + z) / 2 * step, py1 + 72, d.upper(), 10.5, INK, "bold", anchor="middle", ls=1))

    # Post-trial mainline hardening banner — sized to fit cleanly inside left of vertical divider!
    b.append(rect(56, 568, 818, 36, PANEL, RULE, 1, rx=6))
    b.append(mono(68, 590, "MAINLINE HARDENING (NOV 10–13):", 9.8, SLATE, "bold", ls=0.3))
    b.append(sans(272, 590, "4ff3aa2 compact · eede4f1 deadline · e4143e4 judge · 9dbba43 ci/bwrap/FD", 11, INK))

    # Right Sidebar
    rx = 910
    b.append(line(890, 102, 890, 604, RULE, 1))
    b.append(sans(rx, 120, "Trial Outcome Tally", 16.5, INK, "bold"))
    b.append(mono(rx, 138, "14 RUNS · QWEN 3.5-9B", 10.5, MUTED, "bold", ls=0.8))

    tallies = [
        ("Verified Pass", sum(r["status"] == "verified" for r in trials), TEAL, TEAL_SOFT),
        ("Transport / Timeout", sum(r["failure_class"] == "transport" for r in trials), OCHRE, OCHRE_SOFT),
        ("Planner JSON Format", sum(r["failure_class"] == "planner-format" for r in trials), OCHRE, OCHRE_SOFT),
        ("Unknown Tool Call", sum(r["failure_class"] == "capability-request" for r in trials), OCHRE, OCHRE_SOFT),
        ("Path Contract Rule", sum(r["failure_class"] == "path-contract" for r in trials), OCHRE, OCHRE_SOFT),
        ("Interrupted / Killed", sum(r["status"] == "incomplete" for r in trials), GRAPHITE, PANEL),
    ]
    for i, (name, cnt, col, bg) in enumerate(tallies):
        y = 166 + i * 48
        b.append(sans(rx, y + 15, name, 13, INK, "500"))
        for k in range(cnt):
            b.append(rect(rx + k * 17, y + 23, 12, 12, col, "none", 0, rx=2))
        b.append(pill(1108, y + 2, 36, 22, str(cnt), bg, col, col, 11.5, "bold", 0))

    b.append(line(rx, 464, 1144, 464, HAIR, 1))
    b.append(mono(rx, 488, "TRN transport   PLN planner", 10.5, GRAPHITE))
    b.append(mono(rx, 507, "CAP capability  PTH path", 10.5, GRAPHITE))
    b.append(mono(rx, 534, "○ Direct   □ Parallel", 11, INK, "bold"))
    b.append(sans(rx, 560, "Fixture: public kv.py (77→82", 11.5, GRAPHITE))
    b.append(sans(rx, 576, "checks). Exploratory, not benchmark.", 11.5, GRAPHITE))

    return journal_plate(
        5, "Empirical Field Trials: Failure Modes Drove Runtime Hardening", w, h, "\n".join(b),
        subtitle="Timeline of 14 exploratory Qwen 9B trials showing failure classification and commit fixes",
        note="Source: research/data/field_runs.json (extracted from local runs/). No failed run updated the router."
    )


# ================================================================ FIGURE 6: Test Corpus
def fig_tests():
    groups, counts = count_tests()
    total = sum(g[1] for g in groups)
    ignored = sum(g[2] for g in groups)
    files = len(counts)
    w = 1200
    row = 50
    h = 200 + row * len(groups) + 60
    b = []

    b.append(rect(36, 86, w - 72, h - 144, CARD_BG, RULE, 1, rx=8, extra='filter="url(#shadow-card)"'))
    b.append(mono(56, 116, "HERMETIC VERIFICATION SUITE", 11, BOLT, "bold", ls=1.4))
    b.append(t(56, 150, f"{total} Deterministic Rust Tests", 28, INK, family=SANS, weight="bold"))
    b.append(sans(490, 148, f"across {files} suites  ·  {ignored} opt-in ignored", 14, GRAPHITE))
    b.append(pill(w - 220, 120, 162, 26, "NO LIVE LLM REQUIRED", TEAL_SOFT, TEAL, TEAL, 10, "bold", 0.6))

    mx = max(g[1] for g in groups)
    x0, bw = 318, 272
    fx = x0 + bw + 44
    for i, (label, n, ign, present) in enumerate(groups):
        y = 188 + i * row
        b.append(sans(x0 - 16, y + 19, label, 13.2, INK, "bold", anchor="end"))
        b.append(rect(x0, y + 3, bw, 22, PANEL, HAIR, 0.8, rx=4))
        bar_w = bw * n / mx
        bar_c = BOLT if i == 0 else (TEAL if i < 3 else SLATE)
        b.append(rect(x0, y + 3, bar_w, 22, bar_c, "none", 0, rx=4))
        b.append(mono(x0 + bar_w + 10, y + 19, str(n), 13, INK, "bold", ls=0))

        # Balanced multi-line wrap within fx=638..1144 (~74 monospace chars max -> target 66 chars/line)
        clean = [p.replace("src/", "src:") for p in present]
        lines = [[]]
        chars = 0
        for name in clean:
            add = len(name) + (2 if lines[-1] else 0)
            if lines[-1] and chars + add > 68:
                lines.append([name])
                chars = len(name)
            else:
                lines[-1].append(name)
                chars += add
        if len(lines) == 1:
            b.append(mono(fx, y + 19, ", ".join(lines[0]), 9.8, GRAPHITE, ls=0))
        elif len(lines) == 2:
            b.append(mono(fx, y + 10, ", ".join(lines[0]) + ",", 9.8, GRAPHITE, ls=0))
            b.append(mono(fx, y + 25, ", ".join(lines[1]), 9.8, MUTED, ls=0))
        else:
            for li, grp in enumerate(lines[:3]):
                suf = "," if li < len(lines) - 1 else ""
                b.append(mono(fx, y + 5 + li * 13.5, ", ".join(grp) + suf, 9.5, GRAPHITE if li == 0 else MUTED, ls=0))

    return journal_plate(
        6, "Deterministic Test Corpus by Subsystem", w, h, "\n".join(b),
        subtitle=f"Static inventory of {total} #[test] functions across {files} Rust suites",
        note="Counted statically from #[test] in tests/*.rs and src/*.rs (includes 1 #[cfg(not(target_os = \"macos\"))] Linux bubblewrap check)."
    )


# ================================================================ FIGURE 7: Judge & Handoffs
def fig_judge_handoffs():
    w, h = 1200, 650
    b = []

    lw = 576
    b.append(t(36, 102, "(a) Zeus Judge: 8-Gate Shadow Abstention (src/judge.rs)", 17, INK, family=SANS, weight="bold"))
    b.append(rect(36, 114, lw, 480, CARD_BG, RULE, 1, rx=8, extra='filter="url(#shadow-card)"'))

    gates = [
        ("1. Artifact Load", "≤64 KiB, O_NOFOLLOW, regular", "InvalidModel"),
        ("2. Schema & Weights", "job_limits_v1, |w| ≤ 100", "InvalidModel"),
        ("3. Domain Binding", "SHA-256(ctx,model,proto,ver)", "DomainMismatch"),
        ("4. Verifier Presence", "External verifier configured", "NoVerifier"),
        ("5. Support Box", "8-D features x_i ∈ [min, max]", "OutsideSupport"),
        ("6. Sample Floor", "Both heads samples ≥ 30", "InsufficientSamples"),
        ("7. Output Range", "log1p_tok, log1p_sec ∈ [0, 30]", "InvalidPrediction"),
        ("8. Margin Gate", "|p_direct - p_parallel| ≥ 0.05", "Ambiguous"),
    ]
    for i, (name, cond, reason) in enumerate(gates):
        y = 132 + i * 48
        b.append(rect(50, y, 396, 36, PANEL, RULE, 1, rx=5))
        b.append(sans(62, y + 23, name, 12.2, INK, "bold"))
        b.append(mono(222, y + 23, cond, 10, GRAPHITE, ls=0))
        b.append(line(446, y + 18, 468, y + 18, BOLT, 1.2, 'marker-end="url(#arrow-bolt)"'))
        b.append(pill(474, y + 7, 126, 22, reason, BOLT_SOFT, BOLT, BOLT, 9, "bold", 0.1))
        if i < len(gates) - 1:
            b.append(line(248, y + 36, 248, y + 48, TEAL, 1.5))

    b.append(rect(50, 522, 548, 54, TEAL_SOFT, TEAL, 1.5, rx=6))
    b.append(mono(64, 542, "OUTPUT: Decision::Rank { preferred }  (probability_status = \"uncalibrated\")", 10.2, TEAL, "bold", ls=0.1))
    b.append(sans(64, 562, "Persists shadow-decision.json & shadow-outcome.json; never alters UCB1 routing.", 11.5, INK))

    rx_col = 636
    rw = w - rx_col - 36
    b.append(t(rx_col, 102, "(b) Deterministic Memory & Handoff Bounds", 17, INK, family=SANS, weight="bold"))

    b.append(rect(rx_col, 114, rw, 230, CARD_BG, RULE, 1, rx=8, extra='filter="url(#shadow-card)"'))
    b.append(mono(rx_col + 20, 140, "1. TRANSCRIPT COMPACTION (src/agent.rs)", 11.5, SLATE, "bold", ls=0.8))
    b.append(sans(rx_col + 20, 162, "Triggered past compaction_threshold_bytes (default 1 MiB) or overflow:", 12, GRAPHITE))

    bx = rx_col + 20
    for i in range(6):
        is_prot = i >= 2
        col = TEAL if is_prot else OCHRE
        bg = TEAL_SOFT if is_prot else OCHRE_SOFT
        bw_ = 74
        b.append(rect(bx + i * 82, 182, bw_, 64, bg, col, 1.3, rx=5))
        b.append(mono(bx + i * 82 + bw_ / 2, 206, f"Result {i + 1}", 10, col, "bold", anchor="middle"))
        b.append(mono(bx + i * 82 + bw_ / 2, 226, "KEPT" if is_prot else "ELIDED", 10, col, "bold", anchor="middle"))
    b.append(line(bx, 258, bx + 156, 258, OCHRE, 1.5))
    b.append(mono(bx + 78, 275, "{\"elided\":true} placeholder", 10, OCHRE, "bold", anchor="middle"))
    b.append(line(bx + 164, 258, bx + 484, 258, TEAL, 1.5))
    b.append(mono(bx + 324, 275, "PROTECT_LAST_TOOL_RESULTS = 4 (intact)", 10, TEAL, "bold", anchor="middle"))
    b.append(sans(bx, 312, "Replaces summarization LLMs: model can re-read any elided file; audit event emitted.", 11.5, GRAPHITE))

    b.append(rect(rx_col, 364, rw, 230, CARD_BG, RULE, 1, rx=8, extra='filter="url(#shadow-card)"'))
    b.append(mono(rx_col + 20, 390, "2. WORKER HANDOFF PROJECTION (src/output.rs)", 11.5, BOLT, "bold", ls=0.8))
    b.append(sans(rx_col + 20, 412, "Full worker outputs always persist to events.jsonl; integrator sees ≤ 8192 B/worker:", 12, GRAPHITE))

    by = 434
    b.append(rect(bx, by, 260, 48, SLATE_SOFT, SLATE, 1.4, rx=5))
    b.append(mono(bx + 130, by + 21, "75% HEAD (≤ 6 KiB)", 11, SLATE, "bold", anchor="middle"))
    b.append(mono(bx + 130, by + 37, "Opening findings & scope", 10, GRAPHITE, anchor="middle"))

    b.append(rect(bx + 266, by, 108, 48, BOLT_SOFT, BOLT, 1.4, rx=5))
    b.append(mono(bx + 320, by + 21, "OMITTED", 10.5, BOLT, "bold", anchor="middle"))
    b.append(mono(bx + 320, by + 37, "byte count", 9.5, GRAPHITE, anchor="middle"))

    b.append(rect(bx + 380, by, 104, 48, TEAL_SOFT, TEAL, 1.4, rx=5))
    b.append(mono(bx + 432, by + 21, "25% TAIL", 10.5, TEAL, "bold", anchor="middle"))
    b.append(mono(bx + 432, by + 37, "Caveats", 10, GRAPHITE, anchor="middle"))

    b.append(rect(bx, 502, 484, 70, PANEL, RULE, 1, rx=6))
    b.append(mono(bx + 14, 522, "JSON PROJECTED PAYLOAD:", 10, INK, "bold"))
    b.append(mono(bx + 14, 540, "{index, audit_event, tool_calls, original_bytes, omitted_bytes, text}", 9.8, SLATE, "bold"))
    b.append(sans(bx + 14, 558, "Caps 12-worker aggregation at ~96 KiB instead of blowing 4 MiB context limits.", 11.5, GRAPHITE))

    return journal_plate(
        7, "Shadow Outcome Scoring & Bounded Context Projections", w, h, "\n".join(b),
        subtitle="Eight-gate typed abstention contract for Zeus Judge alongside deterministic transcript and handoff compaction",
        note="Source: src/judge.rs (Shadow Judge), src/agent.rs (PROTECT_LAST_TOOL_RESULTS=4), src/output.rs (worker_handoff)."
    )


# ---------------------------------------------------------------- Entrypoint
def build():
    trials, policies = load_trials()
    return {
        "zeus-cover.svg": fig_cover(),
        "fig1-system.svg": fig_system(),
        "fig2-scheduler.svg": fig_scheduler(),
        "fig3-ownership.svg": fig_ownership(),
        "fig4-routing.svg": fig_routing(trials, policies),
        "fig5-field-trials.svg": fig_trials(trials),
        "fig6-tests.svg": fig_tests(),
        "fig7-judge-handoffs.svg": fig_judge_handoffs(),
    }


def main(argv):
    outputs = build()
    if "--check" in argv:
        stale = [
            name for name, svg in outputs.items()
            if not (OUT / name).exists() or (OUT / name).read_text(encoding="utf-8") != svg
        ]
        if stale:
            print("stale figures (run python3 research/scripts/build_figures.py): " + ", ".join(stale))
            return 1
        print(f"{len(outputs)} figures up to date")
        return 0
    OUT.mkdir(parents=True, exist_ok=True)
    for name, svg in outputs.items():
        (OUT / name).write_text(svg, encoding="utf-8")
    print(f"wrote {len(outputs)} figures to {OUT.relative_to(ROOT)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
