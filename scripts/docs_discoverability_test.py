#!/usr/bin/env python3
"""Check canonical, social, structured, and crawl metadata in built docs."""

from __future__ import annotations

import html
import json
import re
import sys
import xml.etree.ElementTree as ET
from pathlib import Path

import tomllib

CANONICAL = re.compile(r'<link rel="canonical" href="([^"]+)">')
DESCRIPTION = re.compile(r'<meta name="description" content="([^"]+)">')
TITLE = re.compile(r"<title>(.*?)</title>", re.DOTALL)
OG_TYPE = re.compile(r'<meta property="og:type" content="([^"]+)">')
OG_TITLE = re.compile(r'<meta property="og:title" content="([^"]+)">')
OG_DESCRIPTION = re.compile(r'<meta property="og:description" content="([^"]+)">')
OG_URL = re.compile(r'<meta property="og:url" content="([^"]+)">')
OG_IMAGE = re.compile(r'<meta property="og:image" content="([^"]+)">')
OG_IMAGE_TYPE = re.compile(r'<meta property="og:image:type" content="image/jpeg">')
OG_IMAGE_WIDTH = re.compile(r'<meta property="og:image:width" content="1200">')
OG_IMAGE_HEIGHT = re.compile(r'<meta property="og:image:height" content="630">')
OG_IMAGE_ALT = re.compile(r'<meta property="og:image:alt" content="([^"]+)">')
TWITTER_CARD = re.compile(
    r'<meta name="twitter:card" content="summary_large_image">'
)
TWITTER_TITLE = re.compile(r'<meta name="twitter:title" content="([^"]+)">')
TWITTER_DESCRIPTION = re.compile(r'<meta name="twitter:description" content="([^"]+)">')
TWITTER_IMAGE = re.compile(r'<meta name="twitter:image" content="([^"]+)">')
TWITTER_IMAGE_ALT = re.compile(
    r'<meta name="twitter:image:alt" content="([^"]+)">'
)
JSON_LD = re.compile(
    r'<script type="application/ld\+json">\s*(.*?)\s*</script>', re.DOTALL
)


def nav_pages(entry: object) -> list[str]:
    if isinstance(entry, str):
        return [entry] if entry.endswith(".md") else []
    if isinstance(entry, list):
        return [page for item in entry for page in nav_pages(item)]
    if isinstance(entry, dict):
        return [page for value in entry.values() for page in nav_pages(value)]
    return []


def built_path(site: Path, page: str) -> Path:
    stem = page.removesuffix(".md")
    if stem == "index":
        return site / "index.html"
    if stem.endswith("/index"):
        stem = stem.removesuffix("/index")
    return site / stem / "index.html"


def generated_url(site: Path, site_url: str, path: Path) -> str:
    relative = path.relative_to(site)
    if relative == Path("index.html"):
        return site_url
    return f"{site_url}{relative.parent.as_posix()}/"


def group(pattern: re.Pattern[str], source: str) -> str | None:
    match = pattern.search(source)
    return html.unescape(match.group(1).strip()) if match else None


def generated_source_block(source: str, marker: str) -> str | None:
    match = re.search(
        rf"<!-- {re.escape(marker)}_START -->\s*(.*?)\s*<!-- {re.escape(marker)}_END -->",
        source,
        re.DOTALL,
    )
    return match.group(1) if match else None


def check_benchmark_sources(failures: list[str]) -> None:
    benchmark_path = Path("docs/benchmarks.md")
    homepage_path = Path("docs/index.md")
    comparison_path = Path("docs/comparison.md")
    if not all(
        path.is_file() for path in (benchmark_path, homepage_path, comparison_path)
    ):
        failures.append("benchmark sources: publication source is missing")
        return

    benchmark = benchmark_path.read_text(encoding="utf-8")
    homepage = homepage_path.read_text(encoding="utf-8")
    comparison = comparison_path.read_text(encoding="utf-8")

    for marker, source in (
        ("BENCHMARK_SUMMARY", benchmark),
        ("BENCHMARK_TABLE", benchmark),
        ("BENCHMARK_HOMEPAGE_INTRO", homepage),
        ("BENCHMARK_HOMEPAGE_TABLE", homepage),
        ("BENCHMARK_COMPARISON_INTRO", comparison),
        ("BENCHMARK_COMPARISON_TABLE", comparison),
    ):
        if (
            source.count(f"<!-- {marker}_START -->") != 1
            or source.count(f"<!-- {marker}_END -->") != 1
        ):
            failures.append(
                f"benchmark sources: {marker} markers are missing or duplicated"
            )

    canonical_table = generated_source_block(benchmark, "BENCHMARK_TABLE") or ""
    homepage_table = generated_source_block(homepage, "BENCHMARK_HOMEPAGE_TABLE") or ""
    homepage_intro = (
        generated_source_block(homepage, "BENCHMARK_HOMEPAGE_INTRO") or ""
    )
    comparison_intro = (
        generated_source_block(comparison, "BENCHMARK_COMPARISON_INTRO") or ""
    )
    comparison_table = (
        generated_source_block(comparison, "BENCHMARK_COMPARISON_TABLE") or ""
    )
    summary = generated_source_block(benchmark, "BENCHMARK_SUMMARY") or ""
    summary_text = " ".join(summary.split())
    run_date = re.search(r"Last benchmark run: (\w+ \d{4})\.", benchmark)
    if not run_date:
        failures.append("benchmarks.md: benchmark run date is missing")
    elif any(
        run_date.group(1) not in publication
        for publication in (summary, homepage_intro, comparison_intro)
    ):
        failures.append("benchmark sources: published run dates are inconsistent")
    rows = {
        match.group("name"): (match.group("mean").strip(), match.group("ratio"))
        for match in re.finditer(
            r"^\| \*\*(?P<name>.+?)\*\*\s+\|\s*[^|]+\|\s*"
            r"(?P<mean>[^|]+?)\s*\|\s*(?P<ratio>[0-9.]+x)\s*\|$",
            canonical_table,
            re.MULTILINE,
        )
    }
    required = {"rumdl", "markdownlint-cli2", "markdownlint-cli"}
    if not required.issubset(rows):
        failures.append("benchmarks.md: required canonical benchmark rows are missing")
    for name, (mean, ratio) in rows.items():
        if (
            f"**{name}**" not in comparison_table
            or mean not in comparison_table
            or ratio not in comparison_table
        ):
            failures.append(f"comparison.md: benchmark row for {name} is inconsistent")
        if name in required:
            homepage_ratio = ratio.removesuffix("x") + "×"
            if (
                name not in homepage_table
                or mean not in homepage_table
                or homepage_ratio not in homepage_table
            ):
                failures.append(f"index.md: benchmark row for {name} is inconsistent")

    if required.issubset(rows):
        cli_ratios = sorted(
            float(rows[name][1].removesuffix("x"))
            for name in ("markdownlint-cli2", "markdownlint-cli")
        )
        for name in required:
            if rows[name][0] not in summary_text:
                failures.append(
                    f"benchmarks.md: summary mean for {name} is inconsistent"
                )
        if f"{cli_ratios[0]:.1f}–{cli_ratios[1]:.1f} times faster" not in summary_text:
            failures.append("benchmarks.md: comparison summary is inconsistent")

    root_chart = Path("assets/benchmark.svg")
    docs_chart = Path("docs/assets/benchmark.svg")
    if (
        not root_chart.is_file()
        or not docs_chart.is_file()
        or root_chart.read_bytes() != docs_chart.read_bytes()
    ):
        failures.append("benchmark chart: README and documentation copies differ")


def main(argv: list[str]) -> int:
    site = Path(argv[1] if len(argv) > 1 else "site")
    config_path = Path(argv[2] if len(argv) > 2 else "zensical.toml")
    if not site.is_dir():
        print(f"docs discoverability: {site}/ is missing", file=sys.stderr)
        return 2

    project = tomllib.loads(config_path.read_text(encoding="utf-8"))["project"]
    site_url = project["site_url"]
    social_image_url = f"{site_url}images/social-preview.jpg"
    nav = nav_pages(project["nav"])
    failures: list[str] = []

    for page in nav:
        path = built_path(site, page)
        if not path.is_file():
            failures.append(f"{page}: generated page is missing")

    generated_pages = sorted(site.rglob("index.html"))
    for path in generated_pages:
        html = path.read_text(encoding="utf-8")
        expected = generated_url(site, site_url, path)
        canonical = CANONICAL.search(html)
        og_url = OG_URL.search(html)
        structured = JSON_LD.search(html)

        label = path.relative_to(site).as_posix()

        if not canonical or canonical.group(1) != expected:
            failures.append(f"{label}: incorrect or missing canonical URL")
        descriptions = (
            group(DESCRIPTION, html),
            group(OG_DESCRIPTION, html),
            group(TWITTER_DESCRIPTION, html),
        )
        if not descriptions[0] or len(set(descriptions)) != 1:
            failures.append(
                f"{label}: page and social descriptions are missing or inconsistent"
            )
        titles = (group(OG_TITLE, html), group(TWITTER_TITLE, html))
        if not group(TITLE, html) or not titles[0] or len(set(titles)) != 1:
            failures.append(
                f"{label}: page or social titles are missing or inconsistent"
            )
        if not og_url or og_url.group(1) != expected:
            failures.append(f"{label}: incorrect or missing Open Graph URL")
        social_images = (group(OG_IMAGE, html), group(TWITTER_IMAGE, html))
        if len(set(social_images)) != 1 or social_images[0] != social_image_url:
            failures.append(f"{label}: social preview images are missing or inconsistent")
        social_image_alts = (group(OG_IMAGE_ALT, html), group(TWITTER_IMAGE_ALT, html))
        if not social_image_alts[0] or len(set(social_image_alts)) != 1:
            failures.append(
                f"{label}: social preview alternative text is missing or inconsistent"
            )
        if not all(
            pattern.search(html)
            for pattern in (OG_IMAGE_TYPE, OG_IMAGE_WIDTH, OG_IMAGE_HEIGHT)
        ):
            failures.append(f"{label}: social preview image metadata is incomplete")
        if not TWITTER_CARD.search(html):
            failures.append(f"{label}: missing Twitter card metadata")
        if not structured:
            failures.append(f"{label}: missing JSON-LD")
        else:
            try:
                data = json.loads(structured.group(1))
            except json.JSONDecodeError as error:
                failures.append(f"{label}: invalid JSON-LD: {error}")
            else:
                expected_type = "WebSite" if expected == site_url else "TechArticle"
                expected_og_type = "website" if expected == site_url else "article"
                if (
                    data.get("url") != expected
                    or data.get("description") != descriptions[0]
                    or data.get("@type") != expected_type
                    or data.get("image") != social_image_url
                ):
                    failures.append(f"{label}: inconsistent JSON-LD identity")
                if group(OG_TYPE, html) != expected_og_type:
                    failures.append(f"{label}: incorrect Open Graph content type")

    sitemap_path = site / "sitemap.xml"
    if not sitemap_path.is_file():
        failures.append("sitemap.xml: missing")
    else:
        namespace = {"s": "http://www.sitemaps.org/schemas/sitemap/0.9"}
        root = ET.fromstring(sitemap_path.read_text(encoding="utf-8"))
        locations = {node.text for node in root.findall("s:url/s:loc", namespace)}
        expected_locations = {
            generated_url(site, site_url, page) for page in generated_pages
        }
        missing = sorted(expected_locations - locations)
        extra = sorted(locations - expected_locations)
        if missing or extra:
            failures.append(
                f"sitemap.xml: generated-page mismatch; missing={missing}, extra={extra}"
            )

    robots = site / "robots.txt"
    if not robots.is_file():
        failures.append("robots.txt: missing")
    else:
        text = robots.read_text(encoding="utf-8")
        if "User-agent: *" not in text or f"Sitemap: {site_url}sitemap.xml" not in text:
            failures.append("robots.txt: crawl rule or sitemap reference is missing")

    benchmark = site / "benchmarks" / "index.html"
    if benchmark.is_file() and "Limitations" not in benchmark.read_text(
        encoding="utf-8"
    ):
        failures.append("benchmarks.md: limitations section is missing")

    if not (site / "assets" / "benchmark.svg").is_file():
        failures.append("assets/benchmark.svg: public benchmark chart is missing")

    social_image = site / "images" / "social-preview.jpg"
    if not social_image.is_file() or social_image.stat().st_size == 0:
        failures.append("images/social-preview.jpg: social preview image is missing")

    check_benchmark_sources(failures)

    if failures:
        print(
            f"docs discoverability FAILED: {len(failures)} problem(s)", file=sys.stderr
        )
        for failure in failures:
            print(f"  {failure}", file=sys.stderr)
        return 1

    print(f"docs discoverability passed: {len(generated_pages)} canonical public pages")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
