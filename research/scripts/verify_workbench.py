#!/usr/bin/env python3
"""Verify the Zeus TR-01 interactive research workbench with Playwright."""

from __future__ import annotations

import argparse
import functools
import http.server
import socketserver
import threading
from pathlib import Path
from playwright.sync_api import sync_playwright

ROOT = Path(__file__).resolve().parents[2]


class QuietHandler(http.server.SimpleHTTPRequestHandler):
    def log_message(self, format: str, *args: object) -> None:  # noqa: A002
        return


def serve_repo(directory: Path):
    handler = functools.partial(QuietHandler, directory=str(directory))
    server = socketserver.ThreadingTCPServer(("127.0.0.1", 0), handler)
    port = server.server_address[1]
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    return server, port


def verify(screenshots_dir: Path | None) -> None:
    server, port = serve_repo(ROOT)
    url = f"http://127.0.0.1:{port}/research/index.html"
    try:
        with sync_playwright() as p:
            browser = p.chromium.launch()
            for label, viewport in [("desktop-1440", {"width": 1440, "height": 900}), ("mobile-390", {"width": 390, "height": 844})]:
                context = browser.new_context(viewport=viewport)
                page = context.new_page()
                console_errors: list[str] = []
                page_errors: list[str] = []
                failed_responses: list[str] = []

                page.on("console", lambda msg: console_errors.append(msg.text) if msg.type == "error" else None)
                page.on("pageerror", lambda err: page_errors.append(str(err)))
                page.on("response", lambda resp: failed_responses.append(f"{resp.status} {resp.url}") if resp.status >= 400 else None)

                page.goto(url, wait_until="networkidle")

                # Check horizontal overflow
                overflow_info = page.evaluate("""() => {
                  const vw = window.innerWidth;
                  const sw = document.documentElement.scrollWidth;
                  if (sw <= vw) return null;
                  const offenders = [];
                  for (const el of document.querySelectorAll('*')) {
                    const r = el.getBoundingClientRect();
                    if (r.right > vw + 1) {
                      offenders.push(`${el.tagName.toLowerCase()}#${el.id}.${el.className}: right=${Math.round(r.right)}`);
                      if (offenders.length >= 6) break;
                    }
                  }
                  return { vw, sw, offenders };
                }""")
                assert not overflow_info, f"{label}: horizontal overflow detected: {overflow_info}"

                # Lab 1: FIFO Scheduler interactions & empty validation state
                assert "266" in page.locator("#meta-tests").inner_text() or "suites" in page.locator("#meta-tests").inner_text()
                page.locator("[data-sched-k='2']").click()
                page.locator("#sched-durations").fill("")
                assert not page.locator("#sched-error").is_hidden(), f"{label}: empty scheduler error missing"
                page.locator("[data-sched-preset='failstop']").click()
                assert page.locator("#sched-error").is_hidden(), f"{label}: preset failed to recover scheduler"
                page.locator("[data-sched-preset='skewed']").click()

                # Lab 2: Scope isolation presets including failure & handoff >8 KiB
                for preset, expect_pass in [("caseAlias", False), ("ancestor", False), ("traversal", False), ("handoff", False), ("disjoint", True)]:
                    page.locator(f"[data-scope-preset='{preset}']").click()
                    verdict_cls = page.locator("#scope-verdict").get_attribute("class") or ""
                    if expect_pass:
                        assert "status-banner-pass" in verdict_cls, f"{label}: {preset} expected pass"
                    else:
                        assert "status-banner-fail" in verdict_cls, f"{label}: {preset} expected fail"

                # Lab 3: UCB1 & Zeus Judge
                before_ucb = page.locator("#ucb-score-direct").inner_text()
                page.locator("#ucb-btn-infra").click()
                assert page.locator("#ucb-score-direct").inner_text() == before_ucb, "infra error altered UCB score"
                page.locator("#ucb-btn-pass").click()
                assert page.locator("#ucb-score-direct").inner_text() != before_ucb
                page.locator("#j-samples").fill("10")
                assert "insufficient_samples" in page.locator("#judge-decision").inner_text()
                page.locator("#j-samples").fill("42")
                assert "Rank" in page.locator("#judge-decision").inner_text()

                # Lab 4: Field trials search + no-match empty state + recovery
                page.locator("#trial-search").fill("NO_SUCH_TRIAL_999")
                assert not page.locator("#trials-empty").is_hidden(), f"{label}: empty state did not appear"
                page.locator("#trials-clear-btn").click()
                assert page.locator("#trials-tbody tr").count() == 14, f"{label}: expected 14 trials after clear"

                # Lab 5: Figure plate switching + SVG non-zero box
                page.locator("[data-plate-id='fig4']").click()
                box = page.locator("#plate-image").bounding_box()
                assert box and box["width"] > 200 and box["height"] > 100, f"{label}: plate image failed to render"
                page.locator("[data-plate-id='fig1']").click()

                if screenshots_dir is not None:
                    screenshots_dir.mkdir(parents=True, exist_ok=True)
                    page.evaluate("window.scrollTo(0, 0)")
                    page.screenshot(path=str(screenshots_dir / f"research-{label}.png"), full_page=True)

                assert not console_errors, f"{label} console errors: {console_errors}"
                assert not page_errors, f"{label} page errors: {page_errors}"
                assert not failed_responses, f"{label} failed HTTP responses: {failed_responses}"
                context.close()
            browser.close()
    finally:
        server.shutdown()


def main() -> None:
    parser = argparse.ArgumentParser(description="Verify research/index.html with Playwright.")
    parser.add_argument("--screenshots-dir", type=Path, default=None, help="Directory to write 1440px and 390px PNG captures.")
    args = parser.parse_args()
    verify(args.screenshots_dir)
    print("Workbench Playwright verification passed (1440px & 390px).")


if __name__ == "__main__":
    main()
