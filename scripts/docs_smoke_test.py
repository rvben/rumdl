#!/usr/bin/env python3
"""Structural smoke tests for the built documentation site.

Runs after `zensical build` and before deploy. Asserts invariants that would
have caught the issue #583 landing-page mangling (Material grid cards rewritten
as `text` code fences). Intentionally narrow: flags known failure modes without
locking in the exact HTML output.

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


GRID_CARDS_OPEN = re.compile(r'<div[^>]*class="[^"]*\bgrid\b[^"]*\bcards\b[^"]*"[^>]*>', re.IGNORECASE)
LANGUAGE_TEXT_CODE = re.compile(
    r'<code[^>]*class="[^"]*\blanguage-text\b[^"]*"[^>]*>([^<]*)</code>',
    re.IGNORECASE,
)

# Prose fragments authored on the landing page. If any appears inside a
# `language-text` code block, the grid-card continuation content was re-fenced
# — exactly the #583 regression.
LANDING_PROSE_FRAGMENTS = (
    "Built for speed",
    "71 lint rules",
    "Auto-formatting",
    "Zero dependencies",
    "Written in Rust",
    "Comprehensive coverage",
    "Single binary",
)

# Pages expected to render at least one grid-cards block with >=4 cards.
GRID_CARD_PAGES: dict[str, int] = {
    "index.html": 4,
}


def read(path: Path) -> str:
    return path.read_text(encoding="utf-8", errors="replace")


def count_list_items_in_grid(html: str) -> int:
    """Count <li> elements appearing inside the first `grid cards` container."""
    match = GRID_CARDS_OPEN.search(html)
    if not match:
        return 0
    start = match.end()
    depth = 1
    i = start
    while i < len(html) and depth > 0:
        open_div = html.find("<div", i)
        close_div = html.find("</div>", i)
        if close_div == -1:
            break
        if open_div != -1 and open_div < close_div:
            depth += 1
            i = open_div + 4
        else:
            depth -= 1
            i = close_div + len("</div>")
    container = html[start:i]
    return len(re.findall(r"<li\b", container, re.IGNORECASE))


def check_page(path: Path, report: Report) -> None:
    rel = path.name
    html = read(path)

    expected_cards = GRID_CARD_PAGES.get(rel)
    if expected_cards is not None:
        if not GRID_CARDS_OPEN.search(html):
            report.fail(rel, "missing `<div class=\"grid cards\">` container")
        else:
            n_cards = count_list_items_in_grid(html)
            if n_cards < expected_cards:
                report.fail(
                    rel,
                    f"grid cards rendered only {n_cards} <li> item(s), expected >= {expected_cards}",
                )

        for fragment in LANDING_PROSE_FRAGMENTS:
            for code_match in LANGUAGE_TEXT_CODE.finditer(html):
                if fragment in code_match.group(1):
                    report.fail(
                        rel,
                        f"landing-page prose {fragment!r} rendered inside "
                        "<code class=\"language-text\"> — grid-card continuation "
                        "was mangled into a code fence (see issue #583)",
                    )
                    break


def main(argv: list[str]) -> int:
    if len(argv) != 2:
        print("usage: docs_smoke_test.py <site_dir>", file=sys.stderr)
        return 2

    site = Path(argv[1])
    if not site.is_dir():
        print(f"error: {site!s} is not a directory", file=sys.stderr)
        return 2

    report = Report()
    for rel in GRID_CARD_PAGES:
        path = site / rel
        if not path.is_file():
            report.fail(rel, f"page not found at {path!s}")
            continue
        check_page(path, report)

    if report.failures:
        print("docs smoke test FAILED:", file=sys.stderr)
        for failure in report.failures:
            print(f"  [{failure.page}] {failure.message}", file=sys.stderr)
        return 1

    print("docs smoke test passed")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
