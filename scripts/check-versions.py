#!/usr/bin/env python3
"""Verify version references in vership.toml-tracked files match Cargo.toml.

vership's text-mode `version_files` rules silently no-op when their `search`
pattern (with `{prev}` substituted) is absent from the matched file. Once any
release misses such an update, drift becomes self-perpetuating: every
subsequent bump still searches for `{prev}`, never finds it, and skips again.

This script enforces the invariant that every text-mode rule's pattern is
present in its target file with the *current* Cargo.toml version. Run it as
a CI check on every push and as vership's `pre-bump` hook so drift is caught
fast and noisily, not silently and forever.
"""

from __future__ import annotations

import re
import sys
import tomllib
from glob import glob
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent


def cargo_version() -> str:
    text = (ROOT / "Cargo.toml").read_text()
    match = re.search(r'^version\s*=\s*"([^"]+)"', text, re.MULTILINE)
    if not match:
        sys.exit("error: failed to parse version from Cargo.toml")
    return match.group(1)


def search_to_regex(search: str) -> re.Pattern[str]:
    """Convert a vership search template into a regex with a version capture.

    `search` is a literal string with a single `{prev}` placeholder. Everything
    else is escaped so regex metacharacters in the literal don't activate.
    """
    parts = search.split("{prev}")
    if len(parts) != 2:
        sys.exit(f"error: search pattern must contain exactly one {{prev}}: {search!r}")
    return re.compile(re.escape(parts[0]) + r"(\d+\.\d+\.\d+)" + re.escape(parts[1]))


def main() -> int:
    version = cargo_version()
    config = tomllib.loads((ROOT / "vership.toml").read_text())
    rules = [r for r in config.get("version_files", []) if "search" in r]

    if not rules:
        print("no text-mode version_files rules configured; nothing to check")
        return 0

    drift: list[str] = []
    for rule in rules:
        pattern = search_to_regex(rule["search"])
        for match in glob(rule["glob"], root_dir=str(ROOT), recursive=True):
            file_path = ROOT / match
            content = file_path.read_text()
            stale = [m for m in pattern.finditer(content) if m.group(1) != version]
            if not stale:
                continue
            for m in stale:
                line_no = content.count("\n", 0, m.start()) + 1
                drift.append(
                    f"  {match}:{line_no}: rule {rule['search']!r} — found "
                    f"version {m.group(1)}, expected {version}"
                )

    if drift:
        print(
            f"Version drift detected. Cargo.toml is at {version} but vership-tracked",
            file=sys.stderr,
        )
        print("references in the following files are out of sync:\n", file=sys.stderr)
        for line in drift:
            print(line, file=sys.stderr)
        print(
            "\nvership's text-mode version_files rules silently no-op when their "
            "{prev}\nsearch string is missing. Update the affected files to match "
            "Cargo.toml,\nthen commit. Future bumps will stay in sync as long as "
            "this check runs.",
            file=sys.stderr,
        )
        return 1

    print(f"Version references in sync at {version}.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
