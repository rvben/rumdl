#!/usr/bin/env python3
"""Regression tests for generated documentation sitemap completion."""

from __future__ import annotations

import tempfile
import xml.etree.ElementTree as ET
from pathlib import Path

from complete_docs_sitemap import SITEMAP_NAMESPACE, complete_sitemap


def write(path: Path, content: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(content, encoding="utf-8")


def main() -> int:
    with tempfile.TemporaryDirectory(prefix="rumdl-docs-sitemap-") as directory:
        site = Path(directory) / "site"
        write(site / "index.html", "home\n")
        write(site / "rules/index.html", "rules\n")
        write(site / "guides/start/index.html", "start\n")
        write(
            site / "sitemap.xml",
            """<?xml version="1.0" encoding="UTF-8"?>
<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">
  <url><loc>https://rumdl.dev/</loc><lastmod>2026-08-30</lastmod></url>
</urlset>
""",
        )

        assert complete_sitemap(site, "https://rumdl.dev/") == 2
        assert complete_sitemap(site, "https://rumdl.dev/") == 0

        namespace = f"{{{SITEMAP_NAMESPACE}}}"
        sitemap = ET.parse(site / "sitemap.xml")
        locations = [
            node.text
            for node in sitemap.findall(f"{namespace}url/{namespace}loc")
        ]
        assert locations == [
            "https://rumdl.dev/",
            "https://rumdl.dev/guides/start/",
            "https://rumdl.dev/rules/",
        ]
        first = sitemap.find(f"{namespace}url/{namespace}lastmod")
        assert first is not None and first.text == "2026-08-30"

    print("docs sitemap completion test passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
