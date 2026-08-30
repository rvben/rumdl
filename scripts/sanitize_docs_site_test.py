#!/usr/bin/env python3
"""Regression tests for the generated documentation publication boundary."""

from __future__ import annotations

import json
import subprocess
import sys
import tempfile
import xml.etree.ElementTree as ET
from pathlib import Path

from sanitize_docs_site import sanitize_site

SITEMAP_NS = "http://www.sitemaps.org/schemas/sitemap/0.9"


def write(path: Path, content: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(content, encoding="utf-8")


def expect_runtime_error(action, message: str) -> None:
    try:
        action()
    except RuntimeError:
        return
    raise AssertionError(message)


def main() -> int:
    with tempfile.TemporaryDirectory(prefix="rumdl-docs-boundary-") as directory:
        root = Path(directory)
        site = root / "site"
        subprocess.run(["git", "init", "-q", str(root)], check=True)
        write(root / ".gitignore", "docs/private.md\ndocs/private/secret.txt\n")
        write(root / "docs/private.md", "# Private\n")
        write(root / "docs/private/secret.txt", "private asset\n")
        write(site / "private/index.html", "private page\n")
        write(site / "private/secret.txt", "private asset\n")
        write(site / "private-not/index.html", "public prefix collision\n")
        write(site / "public/index.html", "public page missing from sitemap\n")
        write(
            site / "search.json",
            json.dumps(
                {
                    "items": [
                        {"location": "private/", "title": "Private"},
                        {"location": "private-not/", "title": "Public"},
                    ]
                }
            ),
        )
        write(
            site / "sitemap.xml",
            """<?xml version="1.0" encoding="UTF-8"?>
<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">
  <url><loc>https://rumdl.dev/private/</loc></url>
  <url><loc>https://rumdl.dev/private-not/</loc></url>
</urlset>
""",
        )

        result = sanitize_site(root, site)
        assert result == (2, 2, 1, 1), result
        assert not (site / "private/index.html").exists()
        assert not (site / "private/secret.txt").exists()
        assert (site / "private-not/index.html").is_file()

        search = json.loads((site / "search.json").read_text(encoding="utf-8"))
        assert [item["location"] for item in search["items"]] == ["private-not/"]
        sitemap = ET.parse(site / "sitemap.xml")
        locations = [
            node.text
            for node in sitemap.findall(f"{{{SITEMAP_NS}}}url/{{{SITEMAP_NS}}}loc")
        ]
        assert locations == ["https://rumdl.dev/private-not/"]

        script = Path(__file__).with_name("sanitize_docs_site.py")
        refused = subprocess.run(
            [sys.executable, str(script), str(root / "outside")],
            capture_output=True,
            check=False,
            text=True,
        )
        assert refused.returncode == 2
        assert "refusing to sanitize" in refused.stderr

        with tempfile.TemporaryDirectory(
            prefix="rumdl-site-root-escape-"
        ) as escape_dir:
            escape = Path(escape_dir)
            write(escape / "index.html", "root target must survive\n")
            linked_site = root / "linked-site"
            linked_site.symlink_to(escape, target_is_directory=True)
            expect_runtime_error(
                lambda: sanitize_site(root, linked_site),
                "a symlinked generated-site root must be rejected",
            )
            assert (escape / "index.html").read_text(encoding="utf-8") == (
                "root target must survive\n"
            )

        with tempfile.TemporaryDirectory(prefix="rumdl-docs-escape-") as escape_dir:
            escape = Path(escape_dir)
            write(escape / "index.html", "must survive\n")
            with (root / ".gitignore").open("a", encoding="utf-8") as ignore:
                ignore.write("docs/escape.md\n")
            write(root / "docs/escape.md", "# Escaped\n")
            (site / "escape").symlink_to(escape, target_is_directory=True)
            expect_runtime_error(
                lambda: sanitize_site(root, site),
                "a generated route must not escape through a symlink",
            )
            assert (escape / "index.html").read_text(
                encoding="utf-8"
            ) == "must survive\n"

    print("docs publication boundary test passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
