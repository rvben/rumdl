#!/usr/bin/env python3
"""Remove ignored documentation sources from a generated public site.

Zensical builds every Markdown file below docs/, including local files that Git
correctly ignores. This post-build boundary removes their rendered pages,
copied assets, and search-index entries without touching the local source.
"""

from __future__ import annotations

import json
import subprocess
import sys
import xml.etree.ElementTree as ET
from pathlib import Path
from urllib.parse import urlparse


def ignored_files(root: Path, docs: Path) -> list[Path]:
    candidates = [path for path in docs.rglob("*") if path.is_file()]
    if not candidates:
        return []

    payload = (
        b"\0".join(str(path.relative_to(root)).encode("utf-8") for path in candidates)
        + b"\0"
    )
    result = subprocess.run(
        ["git", "check-ignore", "-z", "--stdin"],
        cwd=root,
        input=payload,
        capture_output=True,
        check=False,
    )
    if result.returncode not in (0, 1):
        raise RuntimeError(result.stderr.decode("utf-8", errors="replace"))
    return [root / item.decode("utf-8") for item in result.stdout.split(b"\0") if item]


def output_path(site: Path, relative: Path) -> Path:
    if relative.suffix.lower() != ".md":
        return site / relative
    stem = relative.with_suffix("")
    if stem.name == "index":
        return site / stem.parent / "index.html"
    return site / stem / "index.html"


def search_prefix(relative: Path) -> str:
    stem = relative.with_suffix("")
    if stem.name == "index":
        stem = stem.parent
    value = stem.as_posix().strip("/")
    return f"{value}/" if value else ""


def remove_empty_directories(site: Path) -> None:
    directories = sorted(
        (path for path in site.rglob("*") if path.is_dir()),
        key=lambda path: len(path.parts),
        reverse=True,
    )
    for directory in directories:
        try:
            directory.rmdir()
        except OSError:
            pass


def route_matches(location: str, prefixes: set[str]) -> bool:
    route = urlparse(location).path.strip("/")
    normalized = f"{route}/" if route else ""
    return any(normalized.startswith(prefix) for prefix in prefixes)


def require_within(site: Path, path: Path) -> None:
    resolved_site = site.resolve()
    resolved_path = path.resolve(strict=False)
    if not resolved_path.is_relative_to(resolved_site):
        raise RuntimeError(f"refusing path outside generated site: {path}")


def sanitize_site(root: Path, site: Path) -> tuple[int, int, int, int]:
    """Remove ignored documentation output and verify no route survives."""
    if site.is_symlink() or not site.resolve().is_relative_to(root.resolve()):
        raise RuntimeError("refusing a symlinked or external generated site")
    docs = root / "docs"
    ignored = ignored_files(root, docs)
    removed = 0
    prefixes: set[str] = set()
    generated_paths: list[Path] = []
    for source in ignored:
        relative = source.relative_to(docs)
        generated = output_path(site, relative)
        require_within(site, generated)
        generated_paths.append(generated)
        if generated.is_file():
            generated.unlink()
            removed += 1
        if relative.suffix.lower() == ".md":
            prefix = search_prefix(relative)
            if prefix:
                prefixes.add(prefix)

    search_path = site / "search.json"
    require_within(site, search_path)
    search_removed = 0
    if search_path.is_file() and prefixes:
        data = json.loads(search_path.read_text(encoding="utf-8"))
        items = data.get("items", [])
        public_items = [
            item
            for item in items
            if not route_matches(item.get("location", ""), prefixes)
        ]
        search_removed = len(items) - len(public_items)
        data["items"] = public_items
        search_path.write_text(
            json.dumps(data, ensure_ascii=False, separators=(",", ":")),
            encoding="utf-8",
        )

    sitemap_path = site / "sitemap.xml"
    require_within(site, sitemap_path)
    sitemap_removed = 0
    if sitemap_path.is_file() and prefixes:
        tree = ET.parse(sitemap_path)
        sitemap_root = tree.getroot()
        namespace = "{http://www.sitemaps.org/schemas/sitemap/0.9}"
        for entry in list(sitemap_root.findall(f"{namespace}url")):
            location = entry.find(f"{namespace}loc")
            if location is not None and route_matches(location.text or "", prefixes):
                sitemap_root.remove(entry)
                sitemap_removed += 1
        ET.register_namespace("", "http://www.sitemaps.org/schemas/sitemap/0.9")
        tree.write(sitemap_path, encoding="utf-8", xml_declaration=True)

    remove_empty_directories(site)

    survivors = [path for path in generated_paths if path.exists()]
    if survivors:
        raise RuntimeError(
            "ignored documentation output survived sanitization: "
            + ", ".join(str(path.relative_to(site)) for path in survivors)
        )
    if search_path.is_file() and prefixes:
        data = json.loads(search_path.read_text(encoding="utf-8"))
        if any(
            route_matches(item.get("location", ""), prefixes)
            for item in data.get("items", [])
        ):
            raise RuntimeError("ignored documentation route survived in search.json")
    if sitemap_path.is_file() and prefixes:
        tree = ET.parse(sitemap_path)
        locations = (
            node.text or ""
            for node in tree.findall(
                "{http://www.sitemaps.org/schemas/sitemap/0.9}url/{http://www.sitemaps.org/schemas/sitemap/0.9}loc"
            )
        )
        if any(route_matches(location, prefixes) for location in locations):
            raise RuntimeError("ignored documentation route survived in sitemap.xml")

    return len(ignored), removed, search_removed, sitemap_removed


def main(argv: list[str]) -> int:
    if len(argv) != 2:
        print("usage: sanitize_docs_site.py <site_dir>", file=sys.stderr)
        return 2

    root = Path(__file__).resolve().parent.parent
    requested_site = root / argv[1]
    expected_path = root / "site"
    if requested_site.is_symlink() or expected_path.is_symlink():
        print("error: refusing a symlinked generated site", file=sys.stderr)
        return 2
    site = requested_site.resolve()
    expected_site = expected_path.resolve()
    if site != expected_site or not site.is_relative_to(root.resolve()):
        print(
            f"error: refusing to sanitize anything except {expected_site}",
            file=sys.stderr,
        )
        return 2
    if not site.is_dir():
        print(f"error: generated site not found at {site}", file=sys.stderr)
        return 2

    ignored_count, removed, search_removed, sitemap_removed = sanitize_site(root, site)
    print(
        "docs publication boundary: "
        f"{ignored_count} ignored source file(s), "
        f"{removed} output file(s) removed, "
        f"{search_removed} search item(s) removed, "
        f"{sitemap_removed} sitemap route(s) removed"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
