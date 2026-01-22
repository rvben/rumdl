#!/usr/bin/env python3
"""
Discover projects using rumdl with 500+ stars.

Searches GitHub for repositories that use rumdl and reports any with 500+ stars
that aren't already in the README's "Used By" section.

Usage:
    uv run scripts/update-used-by.py
"""

import json
import re
import subprocess
import sys
from pathlib import Path

MIN_STARS = 500
KNOWN_REPOS_MARKER = "## Used By"


def run_gh(args: list[str]) -> str:
    """Run a gh CLI command and return output."""
    result = subprocess.run(
        ["gh", *args],
        capture_output=True,
        text=True,
        timeout=30,
    )
    return result.stdout if result.returncode == 0 else ""


def get_known_repos() -> set[str]:
    """Extract repos already listed in README."""
    readme = Path(__file__).parent.parent / "README.md"
    if not readme.exists():
        return set()

    content = readme.read_text()
    # Match GitHub repo URLs in the Used By section
    pattern = r"github\.com/([^/]+/[^/)]+)"
    return set(re.findall(pattern, content))


def search_repos() -> set[str]:
    """Search GitHub for repos using rumdl."""
    repos = set()
    searches = [
        (["search", "code", "tool.rumdl", "--filename", "pyproject.toml", "--json", "repository", "--limit", "100"]),
        (["search", "code", "--filename", ".rumdl.toml", "--json", "repository", "--limit", "100"]),
        (["search", "code", "rumdl", "--filename", ".pre-commit-config.yaml", "--json", "repository", "--limit", "100"]),
    ]

    for args in searches:
        try:
            output = run_gh(args)
            if output:
                for item in json.loads(output):
                    repo = item.get("repository", {}).get("nameWithOwner", "")
                    if repo and not repo.startswith("rvben/"):
                        repos.add(repo)
        except (json.JSONDecodeError, subprocess.TimeoutExpired):
            continue

    return repos


def get_stars(repo: str) -> int:
    """Get star count for a repo."""
    try:
        output = run_gh(["api", f"repos/{repo}", "--jq", ".stargazers_count"])
        return int(output.strip()) if output.strip() else 0
    except (ValueError, subprocess.TimeoutExpired):
        return 0


def main():
    print("🔍 Discovering projects using rumdl...")

    known = get_known_repos()
    print(f"   Known repos in README: {len(known)}")

    repos = search_repos()
    print(f"   Found {len(repos)} repos referencing rumdl")

    # Check stars for repos not already known
    new_notable = []
    for repo in sorted(repos):
        if repo in known:
            continue
        stars = get_stars(repo)
        if stars >= MIN_STARS:
            new_notable.append((repo, stars))
            print(f"   ⭐ NEW: {repo} ({stars:,} stars)")

    print()
    if new_notable:
        print(f"🎉 Found {len(new_notable)} new project(s) with {MIN_STARS}+ stars!")
        print()
        print("Add to README.md 'Used By' section:")
        print()
        for repo, stars in sorted(new_notable, key=lambda x: -x[1]):
            print(f"| [{repo}](https://github.com/{repo}) | "
                  f"![stars](https://img.shields.io/github/stars/{repo}?style=flat-square) |")
        return 1  # Exit 1 to indicate action needed
    else:
        print(f"✅ No new projects with {MIN_STARS}+ stars found")
        return 0


if __name__ == "__main__":
    sys.exit(main())
