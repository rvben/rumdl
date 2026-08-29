#!/usr/bin/env python3
"""Structural smoke tests for the built documentation site.

Runs after `zensical build` and before deploy. Asserts a small set of durable
homepage, asset, and conversion-path invariants without locking in exact HTML.

Usage: python3 scripts/docs_smoke_test.py <site_dir>
"""

from __future__ import annotations

import re
import sys
from dataclasses import dataclass, field
from pathlib import Path


@dataclass
class Failure:
    page: str
    message: str


@dataclass
class Report:
    failures: list[Failure] = field(default_factory=list)

    def fail(self, page: str, message: str) -> None:
        self.failures.append(Failure(page, message))


HOME_REQUIRED_MARKERS = (
    'class="rm-home"',
    'class="rm-home-nav"',
    '>Documentation<',
    'id="rm-hero-title"',
    'class="rm-terminal-shot"',
    'class="rm-terminal-shot__window"',
    'src="images/homepage-terminal.png"',
    'class="rm-install rm-install--primary"',
    'class="rm-hero__alternatives"',
    'class="rm-next__primary"',
    'class="md-sidebar md-sidebar--primary" data-md-component="sidebar" data-md-type="navigation" hidden',
    ">Quickstart</a>",
    "rm-performance",
    'href="https://rumdl.dev/getting-started/installation/"',
    'href="https://rumdl.dev/playground/"',
    'href="https://rumdl.dev/markdownlint-comparison/"',
    "217 ms",
    'data-rm-event="cta_select"',
    'data-rm-command="uvx_check"',
)

HOME_FORBIDDEN_MARKERS = (
    'class="rm-proof"',
    'class="rm-terminal"',
    "0.15s",
    "0.02s",
    "5.2s",
    ">Try on your repository<",
)
REQUIRED_ASSETS = (
    "stylesheets/rumdl.css",
    "javascripts/rumdl.js",
    "images/homepage-terminal.png",
    "images/social-preview.jpg",
)
REQUIRED_ROUTES = ("playground/index.html",)
PLAYGROUND_REQUIRED_MARKERS = (
    'for="pg-example"',
    'id="pg-announcer"',
    'aria-live="polite"',
    'id="pg-view-tabs"',
    'aria-labelledby="pg-input-heading"',
    'id="pg-undo-btn"',
    'id="pg-config-form"',
    'id="pg-share-btn"',
    'data-warning-fix=',
    "ArrowRight",
    "SHARE_PREFIX",
)
CODE_FENCE_MARKERS = ("rm-hero__aside", "rm-terminal-shot", "rm-section")
SOCIAL_PREVIEW_MARKERS = (
    '<meta property="og:image" content="https://rumdl.dev/images/social-preview.jpg">',
    '<meta property="og:image:type" content="image/jpeg">',
    '<meta property="og:image:width" content="1200">',
    '<meta property="og:image:height" content="630">',
    (
        '<meta property="og:image:alt" content="rumdl documentation, with document '
        'lines accelerating through a coral heading marker">'
    ),
    '<meta name="twitter:card" content="summary_large_image">',
    '<meta name="twitter:image" content="https://rumdl.dev/images/social-preview.jpg">',
    (
        '<meta name="twitter:image:alt" content="rumdl documentation, with document '
        'lines accelerating through a coral heading marker">'
    ),
)
LANGUAGE_TEXT_CODE = re.compile(
    r'<code[^>]*class="[^"]*\blanguage-text\b[^"]*"[^>]*>([^<]*)</code>',
    re.IGNORECASE,
)


def read(path: Path) -> str:
    return path.read_text(encoding="utf-8", errors="replace")


def check_page(path: Path, report: Report) -> None:
    rel = path.relative_to(path.parent).as_posix()
    html = read(path)

    for marker in HOME_REQUIRED_MARKERS:
        if marker not in html:
            report.fail(rel, f"missing required homepage marker {marker!r}")

    for marker in HOME_FORBIDDEN_MARKERS:
        if marker in html:
            report.fail(rel, f"contains superseded benchmark value {marker!r}")

    for marker in SOCIAL_PREVIEW_MARKERS:
        if marker not in html:
            report.fail(rel, f"missing social-preview metadata {marker!r}")

    for marker in CODE_FENCE_MARKERS:
        for code_match in LANGUAGE_TEXT_CODE.finditer(html):
            if marker in code_match.group(1):
                report.fail(
                    rel,
                    f"homepage component {marker!r} rendered inside a text code fence",
                )
                break


def check_playground(path: Path, report: Report) -> None:
    html = read(path)
    for marker in PLAYGROUND_REQUIRED_MARKERS:
        if marker not in html:
            report.fail("playground/index.html", f"missing playground marker {marker!r}")


def main(argv: list[str]) -> int:
    if len(argv) != 2:
        print("usage: docs_smoke_test.py <site_dir>", file=sys.stderr)
        return 2

    site = Path(argv[1])
    if not site.is_dir():
        print(f"error: {site!s} is not a directory", file=sys.stderr)
        return 2

    report = Report()
    homepage = site / "index.html"
    if not homepage.is_file():
        report.fail("index.html", f"page not found at {homepage!s}")
    else:
        check_page(homepage, report)

    playground = site / "playground" / "index.html"
    if playground.is_file():
        check_playground(playground, report)

    for rel in (*REQUIRED_ASSETS, *REQUIRED_ROUTES):
        path = site / rel
        if not path.is_file():
            report.fail(rel, f"required launch surface not found at {path!s}")

    if report.failures:
        print("docs smoke test FAILED:", file=sys.stderr)
        for failure in report.failures:
            print(f"  [{failure.page}] {failure.message}", file=sys.stderr)
        return 1

    print("docs smoke test passed")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
