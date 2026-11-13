#!/usr/bin/env python3
"""Extract a scrubbed field-trial dataset from local Zeus run directories.

Zeus retains every run under its state directory (``runs/`` in this repo, which
is git-ignored because it contains absolute paths, candidate copies and raw
model transcripts). This script reads those retained audits and writes only the
fields needed for the technical report to ``research/data/field_runs.json``:

* no absolute paths, prompts, model transcripts or candidate contents;
* timestamps come from the run identifier (``run-<unix-ns>-<pid>``);
* wall time is approximated from the last write to ``events.jsonl``.

Usage::

    python3 research/scripts/extract_field_runs.py [runs_dir]

The committed JSON is the artefact; this script documents how it was produced.
Standard library only.
"""

from __future__ import annotations

import datetime as dt
import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
OUT = ROOT / "research" / "data" / "field_runs.json"
RUN_RE = re.compile(r"^run-(\d{19})-(\d+)$")

# Failure classes and the hardening commit that addresses each class.
FAILURE_CLASSES = [
    (re.compile(r"model request failed"), "transport", "3267d6f"),
    (re.compile(r"planner must return"), "planner-format", "d121e6b"),
    (re.compile(r"unknown tool requested"), "capability-request", "3f7e00a"),
    (re.compile(r"path components"), "path-contract", None),
]

CAMPAIGNS = {
    "acceptance": "acceptance",
    "router-training": "router-training",
    "model-test": "model-test",
}


def campaign_for(run_dir: Path, runs_root: Path) -> str:
    rel = run_dir.relative_to(runs_root).parts
    if len(rel) == 1:
        return "smoke"
    head = rel[0]
    for prefix, name in CAMPAIGNS.items():
        if head.startswith(prefix):
            return name
    return "other"


def read_events(path: Path) -> list[dict]:
    events = []
    with path.open(encoding="utf-8") as handle:
        for line in handle:
            line = line.strip()
            if line:
                events.append(json.loads(line))
    return events


def checks(text: str) -> tuple[int | None, int | None]:
    match = re.search(r"(PASS|FAIL): (\d+) checks, (\d+) failures", text or "")
    if match:
        return int(match.group(2)), int(match.group(3))
    fails = len(re.findall(r"^FAIL:", text or "", flags=re.M))
    return None, (fails or None)


def classify(error: str | None) -> tuple[str | None, str | None]:
    if not error:
        return None, None
    for pattern, name, commit in FAILURE_CLASSES:
        if pattern.search(error):
            return name, commit
    return "other", None


def extract(runs_root: Path) -> list[dict]:
    rows = []
    for events_path in sorted(runs_root.rglob("events.jsonl")):
        run_dir = events_path.parent
        match = RUN_RE.match(run_dir.name)
        if not match:
            continue
        start_ns = int(match.group(1))
        started = dt.datetime.fromtimestamp(start_ns / 1e9, tz=dt.timezone.utc)
        finished = dt.datetime.fromtimestamp(events_path.stat().st_mtime, tz=dt.timezone.utc)
        events = read_events(events_path)
        start = next((e for e in events if e.get("kind") == "start"), {"data": {}})
        kinds = [e.get("kind") for e in events]

        report_path = run_dir / "report.json"
        error_path = run_dir / "error.json"
        status, error, policy_updated = "incomplete", None, False
        final_checks = final_failures = None
        if report_path.exists():
            report = json.loads(report_path.read_text(encoding="utf-8"))
            status = report.get("status", "unknown")
            policy_updated = bool(report.get("policy_updated"))
            verification = report.get("verification") or {}
            final_checks, final_failures = checks(verification.get("stdout", ""))
        elif error_path.exists():
            failure = json.loads(error_path.read_text(encoding="utf-8"))
            status = failure.get("status", "error")
            error = failure.get("error")
            policy_updated = bool(failure.get("policy_updated"))

        baseline = next((e["data"] for e in events if e.get("kind") == "baseline"), {})
        baseline_text = (baseline.get("stdout") or "") + (baseline.get("stderr") or "")
        _, baseline_failures = checks(baseline_text)
        if baseline_failures is None and baseline.get("exit_code") not in (None, 0):
            baseline_failures = None
        learning = run_dir / "learning.json"
        failure_class, fix_commit = classify(error)

        rows.append(
            {
                "campaign": campaign_for(run_dir, runs_root),
                "started_at": started.isoformat(timespec="seconds"),
                "approx_wall_seconds": max(0, round((finished - started).total_seconds())),
                "model": start["data"].get("model"),
                "strategy": (start["data"].get("strategy") or "").lower() or None,
                "status": status,
                "failure_class": failure_class,
                "addressed_by": fix_commit,
                "error": error,
                "baseline_failures": baseline_failures,
                "verifier_checks": final_checks,
                "verifier_failures": final_failures,
                "tool_requests": kinds.count("tool_request"),
                "parallel_workers": kinds.count("worker"),
                "planned": "planner" in kinds,
                "reward_recorded": learning.exists(),
                "policy_updated_in_report": policy_updated,
            }
        )
    rows.sort(key=lambda r: r["started_at"])
    for index, row in enumerate(rows, start=1):
        row["trial"] = f"T{index:02d}"
    return [{"trial": r.pop("trial"), **r} for r in rows]


def main() -> int:
    runs_root = Path(sys.argv[1]) if len(sys.argv) > 1 else ROOT / "runs"
    if not runs_root.is_dir():
        print(f"no retained runs at {runs_root}; committed dataset left unchanged", file=sys.stderr)
        return 0
    rows = extract(runs_root)
    policies = {}
    for policy_path in sorted(runs_root.rglob("policy.json")):
        campaign = campaign_for(policy_path.parent / "run-0000000000000000000-0", runs_root)
        contexts = json.loads(policy_path.read_text(encoding="utf-8")).get("contexts", {})
        policies[campaign] = {
            context: {"direct": arms[0], "parallel": arms[1]} for context, arms in contexts.items()
        }
    payload = {
        "description": (
            "Scrubbed field trials of Zeus on the public kv.py persistence-repair "
            "fixture with a local 9B model. Exploratory engineering runs, not a "
            "held-out benchmark."
        ),
        "source": "research/scripts/extract_field_runs.py over local retained runs",
        "trials": rows,
        "final_policies": policies,
    }
    OUT.parent.mkdir(parents=True, exist_ok=True)
    OUT.write_text(json.dumps(payload, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
    print(f"wrote {len(rows)} trials to {OUT.relative_to(ROOT)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
