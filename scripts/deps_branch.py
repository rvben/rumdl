#!/usr/bin/env python3
"""Tell the dependency workflow whether it may rewrite its proposal branch.

The weekly refresh keeps one rolling proposal branch. It force-pushes that
branch when it finds updates, and closes the proposal and deletes the branch
when main already carries every update. Both would destroy a commit someone
pushed onto the branch by hand, such as a compile fix for an upgrade. This
script inspects the commits the branch carries beyond the base, so the workflow
only ever rewrites a branch that holds nothing but its own commits.

Writes two lines to $GITHUB_OUTPUT when set, otherwise to stdout:

    state=absent | bot-only | hand-edited
        absent       the branch does not exist
        bot-only     every commit beyond the base was authored and committed
                     by the workflow's own identity
        hand-edited  at least one commit was authored or committed by anyone
                     else, or the commits could not all be inspected
    sha=<tip>
        The branch tip that was inspected, empty when absent. Pass it to
        `git push --force-with-lease`, so a commit pushed after this check
        fails the push instead of being overwritten.

Usage:
    deps_branch.py --repo OWNER/NAME --base main --branch deps/upd --bot-email EMAIL
"""

from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
from typing import Callable

GhApi = Callable[[str], object]


def gh_api(path: str) -> object:
    result = subprocess.run(["gh", "api", path], check=True, capture_output=True, text=True)
    return json.loads(result.stdout)


def branch_tip(gh: GhApi, repo: str, branch: str) -> str | None:
    """Return the branch's tip SHA, or None when the branch does not exist.

    matching-refs answers an absent branch with an empty list rather than a
    404, so absence is never confused with a failed request. It also matches
    by prefix, hence the exact comparison.
    """
    refs = gh(f"repos/{repo}/git/matching-refs/heads/{branch}")
    for ref in refs:
        if ref["ref"] == f"refs/heads/{branch}":
            return ref["object"]["sha"]
    return None


def classify(comparison: dict, bot_email: str) -> str:
    commits = comparison["commits"]
    # The compare API lists at most 250 commits. Anything it leaves out cannot
    # be shown to be the workflow's own, so it counts as hand-made.
    if len(commits) < comparison["ahead_by"]:
        return "hand-edited"
    for commit in commits:
        identities = {commit["commit"]["author"]["email"], commit["commit"]["committer"]["email"]}
        if identities != {bot_email}:
            return "hand-edited"
    return "bot-only"


def inspect(gh: GhApi, repo: str, base: str, branch: str, bot_email: str) -> tuple[str, str]:
    sha = branch_tip(gh, repo, branch)
    if sha is None:
        return "absent", ""
    return classify(gh(f"repos/{repo}/compare/{base}...{sha}"), bot_email), sha


def main(argv: list[str], gh: GhApi = gh_api) -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--repo", required=True)
    parser.add_argument("--base", required=True)
    parser.add_argument("--branch", required=True)
    parser.add_argument("--bot-email", required=True)
    args = parser.parse_args(argv)

    state, sha = inspect(gh, args.repo, args.base, args.branch, args.bot_email)
    lines = f"state={state}\nsha={sha}\n"
    output = os.environ.get("GITHUB_OUTPUT")
    if output:
        with open(output, "a", encoding="utf-8") as handle:
            handle.write(lines)
    else:
        sys.stdout.write(lines)
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
