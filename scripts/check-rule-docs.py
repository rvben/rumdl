#!/usr/bin/env python3
"""Keep rule-count claims in the docs in sync with the rule registry.

The number of implemented rules is stated in prose across the README and
docs/. Hand-maintained, those numbers drift every time a rule is added and
disagree with each other. The single source of truth is the compiled rule
registry, surfaced by `rumdl rule`.

Three numbers are machine-owned, each wrapped in an HTML-comment sentinel so
the surrounding prose stays human-written:

  <!-- RULE_COUNT -->74<!-- /RULE_COUNT -->                 total rules
  <!-- RULE_COUNT_ADDITIONAL -->21<!-- /RULE_COUNT_ADDITIONAL -->  total - markdownlint base
  <!-- RULE_MAX -->MD080<!-- /RULE_MAX -->                  highest rule id

`--write` rewrites the value inside every sentinel from the registry (the
autofix). The default mode verifies and fails on drift (the CI/pre-push
guard). It also asserts every registry rule id appears exactly once in the
docs/rules.md category tables, and that no table row references a rule that
does not exist.
"""

from __future__ import annotations

import argparse
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

# markdownlint (DavidAnson) ships 53 rules; rumdl implements all of them, so
# "additional rules" is total minus this constant. Bump only if markdownlint
# itself changes its rule set and the comparison docs are re-baselined.
MARKDOWNLINT_BASE = 53

# Files whose rule-count sentinels are kept in sync.
DOC_FILES = [
    "README.md",
    "docs/index.md",
    "docs/rules.md",
    "docs/comparison.md",
    "docs/markdownlint-comparison.md",
    "docs/mdformat-comparison.md",
    "docs/getting-started/quickstart.md",
]

RULES_REFERENCE = "docs/rules.md"

# A table row in docs/rules.md, e.g. `| [MD080](md080.md) | ... | ... |`.
RULES_TABLE_ROW = re.compile(r"\|\s*\[(MD\d{3})\]\(")


def registry_rule_ids() -> list[str]:
    """Authoritative rule ids from the compiled registry via `rumdl rule`."""
    try:
        out = subprocess.run(
            ["cargo", "run", "-q", "--bin", "rumdl", "--", "rule"],
            cwd=ROOT,
            capture_output=True,
            text=True,
            check=True,
        ).stdout
    except subprocess.CalledProcessError as exc:
        sys.exit(f"error: `rumdl rule` failed:\n{exc.stderr}")
    ids = sorted(set(re.findall(r"\bMD\d{3}\b", out)))
    if not ids:
        sys.exit("error: no rule ids parsed from `rumdl rule` output")
    return ids


def expected_values(ids: list[str]) -> dict[str, str]:
    total = len(ids)
    return {
        "RULE_COUNT": str(total),
        "RULE_COUNT_ADDITIONAL": str(total - MARKDOWNLINT_BASE),
        "RULE_MAX": max(ids),
    }


def sentinel_pattern(name: str) -> re.Pattern[str]:
    # Inner value is any run without a newline or the opening of a comment.
    return re.compile(
        rf"(<!-- {name} -->)([^\n<]*)(<!-- /{name} -->)",
    )


def check_sentinels(values: dict[str, str]) -> list[str]:
    drift: list[str] = []
    for rel in DOC_FILES:
        path = ROOT / rel
        content = path.read_text()
        for name, expected in values.items():
            for m in sentinel_pattern(name).finditer(content):
                found = m.group(2)
                if found != expected:
                    line_no = content.count("\n", 0, m.start()) + 1
                    drift.append(
                        f"  {rel}:{line_no}: {name} is {found!r}, expected {expected!r}"
                    )
    return drift


def write_sentinels(values: dict[str, str]) -> list[str]:
    changed: list[str] = []
    for rel in DOC_FILES:
        path = ROOT / rel
        original = path.read_text()
        content = original
        for name, expected in values.items():
            content = sentinel_pattern(name).sub(
                lambda m, e=expected: m.group(1) + e + m.group(3),
                content,
            )
        if content != original:
            path.write_text(content)
            changed.append(rel)
    return changed


def check_rules_table(ids: list[str]) -> list[str]:
    """Every registry id must appear in docs/rules.md; no nonexistent rows.

    A rule may legitimately appear more than once: opt-in rules are listed
    both in the "Opt-in Rules" overview table and in their category table.
    Repetition is therefore allowed; only absence and nonexistent rows are
    drift.
    """
    content = (ROOT / RULES_REFERENCE).read_text()
    seen = set(RULES_TABLE_ROW.findall(content))
    registry = set(ids)

    problems: list[str] = []
    missing = sorted(registry - seen)
    extra = sorted(seen - registry)
    if missing:
        problems.append(
            f"  {RULES_REFERENCE}: missing table rows for {', '.join(missing)}"
        )
    if extra:
        problems.append(
            f"  {RULES_REFERENCE}: table rows for nonexistent rules {', '.join(extra)}"
        )
    return problems


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--write",
        action="store_true",
        help="rewrite sentinel values from the registry instead of checking",
    )
    args = parser.parse_args()

    ids = registry_rule_ids()
    values = expected_values(ids)

    if args.write:
        changed = write_sentinels(values)
        if changed:
            print("Updated rule-count sentinels in:")
            for rel in changed:
                print(f"  {rel}")
        else:
            print("Rule-count sentinels already in sync.")
        # Surface table gaps even in write mode; the table is curated, not
        # generated, so it cannot be auto-fixed here.
        table_problems = check_rules_table(ids)
        if table_problems:
            print(
                f"\ndocs/rules.md table is out of sync (total {values['RULE_COUNT']} rules):",
                file=sys.stderr,
            )
            for line in table_problems:
                print(line, file=sys.stderr)
            return 1
        return 0

    problems = check_sentinels(values) + check_rules_table(ids)
    if problems:
        print(
            f"Rule-doc drift detected. Registry has {values['RULE_COUNT']} rules "
            f"({values['RULE_COUNT_ADDITIONAL']} beyond markdownlint, "
            f"max {values['RULE_MAX']}):\n",
            file=sys.stderr,
        )
        for line in problems:
            print(line, file=sys.stderr)
        print(
            "\nRun `make sync-rule-docs` to update the count sentinels. Add or "
            "remove docs/rules.md table rows by hand to resolve table drift.",
            file=sys.stderr,
        )
        return 1

    print(
        f"Rule docs in sync: {values['RULE_COUNT']} rules "
        f"({values['RULE_COUNT_ADDITIONAL']} beyond markdownlint, max {values['RULE_MAX']})."
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
