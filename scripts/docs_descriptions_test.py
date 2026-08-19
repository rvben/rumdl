#!/usr/bin/env python3
"""Assert every page in the nav carries its own meta description.

The published set is read from `nav` in zensical.toml rather than from the docs
tree, because most of `docs/` is per-rule reference that the site builds but
never links. A page reachable from the nav is a page a search result can land
on, so that is the set held to this contract.

The check reads the generated HTML rather than the Markdown frontmatter,
because the two disagree in a way that matters: an unquoted colon in a YAML
scalar makes Zensical drop the key and silently fall back to the site-wide
description, so a page whose source looks correct still ships the wrong tag.

The site description counts as a failure rather than as a default. A page
serving it is a page that told search engines nothing specific about itself.
"""

from __future__ import annotations

import re
import sys
import tomllib
from pathlib import Path

MIN_LENGTH = 110
MAX_LENGTH = 155

DESCRIPTION = re.compile(
    r'<meta\s+name="description"\s+content="([^"]*)"', re.IGNORECASE
)


def nav_pages(entry: object) -> list[str]:
    """Collect every Markdown path reachable from a nav entry."""
    if isinstance(entry, str):
        return [entry] if entry.endswith(".md") else []
    if isinstance(entry, list):
        return [page for item in entry for page in nav_pages(item)]
    if isinstance(entry, dict):
        return [page for value in entry.values() for page in nav_pages(value)]
    return []


def built_path(site: Path, page: str) -> Path:
    """Map a nav source path to the HTML the site generator writes for it."""
    stem = page[: -len(".md")]
    if stem == "index":
        return site / "index.html"
    if stem.endswith("/index"):
        return site / stem[: -len("/index")] / "index.html"
    return site / stem / "index.html"


def main() -> int:
    site = Path(sys.argv[1] if len(sys.argv) > 1 else "site")
    config = Path(sys.argv[2] if len(sys.argv) > 2 else "zensical.toml")

    if not site.is_dir():
        print(f"docs descriptions: {site}/ is missing; run `zensical build` first")
        return 1

    project = tomllib.loads(config.read_text())["project"]
    site_description = project["site_description"]
    pages = nav_pages(project["nav"])
    if not pages:
        print(f"docs descriptions: no pages found in {config} nav")
        return 1

    problems: list[str] = []
    seen: dict[str, str] = {}

    for page in pages:
        html = built_path(site, page)
        if not html.is_file():
            problems.append(f"{page}: in the nav but {html} was not built")
            continue
        found = DESCRIPTION.search(html.read_text(encoding="utf-8"))
        if not found:
            problems.append(f"{page}: no meta description at all")
            continue
        description = found.group(1)
        if description == site_description:
            problems.append(f"{page}: still serving the site-wide description")
            continue
        if not MIN_LENGTH <= len(description) <= MAX_LENGTH:
            problems.append(
                f"{page}: {len(description)} chars, outside {MIN_LENGTH}-{MAX_LENGTH}"
            )
        if description in seen:
            problems.append(f"{page}: identical to {seen[description]}")
        seen[description] = page

    if problems:
        print(f"docs descriptions: {len(problems)} problem(s)")
        for problem in problems:
            print(f"  {problem}")
        return 1

    print(f"docs descriptions: {len(pages)} nav pages, each with its own description")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
