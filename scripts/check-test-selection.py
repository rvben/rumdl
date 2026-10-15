#!/usr/bin/env python3
"""Fail if a non-ignored test belongs to neither nextest CI profile."""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent


def list_tests(profile: str, config_file: str | None, *, unfiltered=False) -> dict:
    command = [
        "cargo", "nextest", "list", "--locked", "--message-format", "json",
        "--profile", profile,
    ]
    if config_file:
        command.extend(["--config-file", config_file])
    if unfiltered:
        command.append("--ignore-default-filter")
    result = subprocess.run(command, cwd=ROOT, check=True, stdout=subprocess.PIPE, text=True)
    inventory = json.loads(result.stdout)
    tests = {}
    for binary, suite in inventory["rust-suites"].items():
        for name, test in suite["testcases"].items():
            if test["filter-match"]["status"] not in ("matches", "mismatch"):
                raise ValueError(f"unknown filter status for {binary}: {name}")
            tests[binary, name] = test
    if not tests:
        raise ValueError("nextest returned an empty test inventory")
    return tests


def uncovered_tests(all_tests: dict, ci: dict, performance: dict) -> list:
    for profile, tests in (("ci", ci), ("performance", performance)):
        if tests.keys() != all_tests.keys():
            raise ValueError(f"{profile} inventory differs from the unfiltered inventory")
        for key, test in tests.items():
            if test["ignored"] != all_tests[key]["ignored"]:
                raise ValueError(f"{profile} disagrees on ignored status for {key}")
    return sorted(
        key for key, test in all_tests.items()
        if not test["ignored"]
        and ci[key]["filter-match"]["status"] != "matches"
        and performance[key]["filter-match"]["status"] != "matches"
    )


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--config-file", help="nextest configuration to check")
    args = parser.parse_args()
    try:
        all_tests = list_tests("ci", args.config_file, unfiltered=True)
        ci = list_tests("ci", args.config_file)
        performance = list_tests("performance", args.config_file)
        uncovered = uncovered_tests(all_tests, ci, performance)
    except (OSError, subprocess.CalledProcessError, ValueError, KeyError, TypeError) as error:
        print(f"error: could not verify test selection: {error}", file=sys.stderr)
        return 2
    if uncovered:
        print("Non-ignored tests selected by neither ci nor performance:", file=sys.stderr)
        for binary, name in uncovered:
            print(f"  {binary}: {name}", file=sys.stderr)
        return 1
    count = sum(not test["ignored"] for test in all_tests.values())
    print(f"All {count} non-ignored tests are selected by ci or performance.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
