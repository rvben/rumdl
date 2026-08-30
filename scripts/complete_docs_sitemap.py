#!/usr/bin/env python3
"""Add every generated documentation page to the public sitemap."""

from __future__ import annotations

import sys
import tomllib
import xml.etree.ElementTree as ET
from pathlib import Path
from urllib.parse import urlparse

SITEMAP_NAMESPACE = "http://www.sitemaps.org/schemas/sitemap/0.9"
XML_NAMESPACE = f"{{{SITEMAP_NAMESPACE}}}"


def generated_route(site: Path, page: Path) -> str:
    relative = page.relative_to(site)
    if relative == Path("index.html"):
        return ""
    return f"{relative.parent.as_posix()}/"


def normalized_site_url(site_url: str) -> str:
    parsed = urlparse(site_url)
    if parsed.scheme not in {"http", "https"} or not parsed.netloc:
        raise RuntimeError(f"invalid documentation site URL: {site_url!r}")
    return f"{site_url.rstrip('/')}/"


def complete_sitemap(site: Path, site_url: str) -> int:
    """Append missing generated routes while preserving generator metadata."""
    if site.is_symlink() or not site.is_dir():
        raise RuntimeError("refusing a missing or symlinked generated site")

    sitemap_path = site / "sitemap.xml"
    if sitemap_path.is_symlink() or not sitemap_path.is_file():
        raise RuntimeError("generated sitemap.xml is missing or symlinked")

    resolved_site = site.resolve()
    pages = sorted(site.rglob("index.html"))
    for page in pages:
        if page.is_symlink() or not page.resolve().is_relative_to(resolved_site):
            raise RuntimeError(f"refusing generated page outside site: {page}")

    tree = ET.parse(sitemap_path)
    root = tree.getroot()
    if root.tag != f"{XML_NAMESPACE}urlset":
        raise RuntimeError("generated sitemap.xml has an unexpected root element")

    locations = {
        node.text or ""
        for node in root.findall(f"{XML_NAMESPACE}url/{XML_NAMESPACE}loc")
    }
    base_url = normalized_site_url(site_url)
    added = 0
    for page in pages:
        location = base_url + generated_route(site, page)
        if location in locations:
            continue
        entry = ET.SubElement(root, f"{XML_NAMESPACE}url")
        ET.SubElement(entry, f"{XML_NAMESPACE}loc").text = location
        locations.add(location)
        added += 1

    if added:
        ET.register_namespace("", SITEMAP_NAMESPACE)
        tree.write(sitemap_path, encoding="utf-8", xml_declaration=True)
    return added


def main(argv: list[str]) -> int:
    if len(argv) != 2:
        print("usage: complete_docs_sitemap.py <site_dir>", file=sys.stderr)
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
            f"error: refusing to complete anything except {expected_site}",
            file=sys.stderr,
        )
        return 2
    if not site.is_dir():
        print(f"error: generated site not found at {site}", file=sys.stderr)
        return 2

    config = tomllib.loads((root / "zensical.toml").read_text(encoding="utf-8"))
    added = complete_sitemap(site, config["project"]["site_url"])
    print(f"docs sitemap: {added} generated route(s) added")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
