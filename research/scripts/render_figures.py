#!/usr/bin/env python3
"""Render docs/figures/*.svg to vector PDF (for the LaTeX report) and PNG previews.

Uses headless Chromium through Playwright so the PDFs match what browsers and
GitHub draw. Run with uv (no project dependency is added to the Rust crate)::

    uv run --with playwright python research/scripts/render_figures.py [--png DIR]

If Chromium is missing: ``uv run --with playwright playwright install chromium``.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

from playwright.sync_api import sync_playwright

ROOT = Path(__file__).resolve().parents[2]
SRC = ROOT / "docs" / "figures"
PDF_OUT = ROOT / "research" / "paper" / "figures"


def size(svg: str) -> tuple[int, int]:
    m = re.search(r'<svg[^>]*\bwidth="(\d+)"[^>]*\bheight="(\d+)"', svg)
    if not m:
        raise SystemExit("figure is missing explicit width/height")
    return int(m.group(1)), int(m.group(2))


def main(argv: list[str]) -> int:
    png_dir = Path(argv[argv.index("--png") + 1]) if "--png" in argv else None
    PDF_OUT.mkdir(parents=True, exist_ok=True)
    if png_dir:
        png_dir.mkdir(parents=True, exist_ok=True)
    with sync_playwright() as p:
        browser = p.chromium.launch()
        for svg_path in sorted(SRC.glob("*.svg")):
            svg = svg_path.read_text(encoding="utf-8")
            w, h = size(svg)
            page = browser.new_page(viewport={"width": w, "height": h}, device_scale_factor=2)
            page.set_content(
                "<!doctype html><html><head><style>html,body{margin:0;padding:0;}"
                "svg{display:block}</style></head><body>" + svg + "</body></html>"
            )
            page.pdf(path=str(PDF_OUT / f"{svg_path.stem}.pdf"), width=f"{w}px", height=f"{h}px",
                     print_background=True, page_ranges="1")
            if png_dir:
                page.screenshot(path=str(png_dir / f"{svg_path.stem}.png"), full_page=True)
            page.close()
        browser.close()
    print(f"rendered {len(list(SRC.glob('*.svg')))} figures")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))

