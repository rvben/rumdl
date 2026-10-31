#!/usr/bin/env python3
"""Regression tests for the generated documentation publication boundary."""

from __future__ import annotations

import json
import os
import subprocess
import sys
import tempfile
import xml.etree.ElementTree as ET
from pathlib import Path

from sanitize_docs_site import repository_env, sanitize_site

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


def check_boundary() -> None:
    with tempfile.TemporaryDirectory(prefix="rumdl-docs-boundary-") as directory:
        root = Path(directory)
        site = root / "site"
        subprocess.run(
            ["git", "init", "-q", str(root)], check=True, env=repository_env()
        )
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


def check_boundary_inside_hook(variables: tuple[str, ...]) -> None:
    """Run the boundary checks as a git hook in a linked worktree would.

    Git exports `GIT_DIR` to hooks and prek adds `GIT_WORK_TREE`; in a linked
    worktree both point at that worktree, whose repository config is shared
    with the main checkout. Every git command these checks start must still
    act on its own directory and leave that shared config alone.
    """
    with tempfile.TemporaryDirectory(prefix="rumdl-docs-hook-repo-") as directory:
        main_checkout = Path(directory) / "main"
        worktree = Path(directory) / "worktree"
        isolated = repository_env()
        subprocess.run(
            ["git", "init", "-q", str(main_checkout)], check=True, env=isolated
        )
        subprocess.run(
            [
                "git",
                "-c",
                "user.name=docs-test",
                "-c",
                "user.email=docs-test@example.com",
                "-c",
                "commit.gpgsign=false",
                "commit",
                "-q",
                "--allow-empty",
                "-m",
                "initial",
            ],
            cwd=main_checkout,
            check=True,
            env=isolated,
        )
        subprocess.run(
            ["git", "worktree", "add", "-q", str(worktree)],
            cwd=main_checkout,
            check=True,
            env=isolated,
        )
        config = main_checkout / ".git" / "config"
        before = config.read_text(encoding="utf-8")
        locations = {
            "GIT_DIR": str(main_checkout / ".git" / "worktrees" / worktree.name),
            "GIT_WORK_TREE": str(worktree),
        }
        saved = {name: os.environ.get(name) for name in variables}
        os.environ.update({name: locations[name] for name in variables})
        try:
            check_boundary()
        finally:
            for name, value in saved.items():
                if value is None:
                    os.environ.pop(name, None)
                else:
                    os.environ[name] = value
        assert config.read_text(encoding="utf-8") == before, (
            f"checks run with {' and '.join(variables)} rewrote the hook's "
            f"repository config:\n{config.read_text(encoding='utf-8')}"
        )


def main() -> int:
    check_boundary()
    check_boundary_inside_hook(("GIT_DIR",))
    check_boundary_inside_hook(("GIT_DIR", "GIT_WORK_TREE"))
    print("docs publication boundary test passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
