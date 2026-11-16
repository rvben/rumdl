#!/usr/bin/env python3
"""Regression tests for scripts/deps_branch.py.

The script decides whether the weekly dependency workflow may force-push or
delete its proposal branch, and it runs only in that scheduled workflow, where
a wrong answer silently destroys a commit someone added by hand. These tests
run it everywhere else.

Hermetic: GitHub API responses are passed in as data and `gh` is never invoked.
Run with:

    python3 scripts/deps_branch_test.py
"""

from __future__ import annotations

import contextlib
import importlib.util
import io
import os
import sys
import tempfile
import unittest
from pathlib import Path

_SPEC = importlib.util.spec_from_file_location("deps_branch", Path(__file__).with_name("deps_branch.py"))
db = importlib.util.module_from_spec(_SPEC)
sys.modules[_SPEC.name] = db
_SPEC.loader.exec_module(db)

REPO = "owner/name"
BRANCH = "deps/upd"
BOT = "41898282+github-actions[bot]@users.noreply.github.com"
HUMAN = "someone@example.com"
TIP = "a" * 40


def commit(author: str = BOT, committer: str = BOT) -> dict:
    return {"commit": {"author": {"email": author}, "committer": {"email": committer}}}


def fake_gh(refs: list[dict], comparison: dict | None = None):
    """Answer the two endpoints the script uses; any other path is a test bug."""
    seen: list[str] = []

    def gh(path: str) -> object:
        seen.append(path)
        if path == f"repos/{REPO}/git/matching-refs/heads/{BRANCH}":
            return refs
        if comparison is not None and path == f"repos/{REPO}/compare/main...{TIP}":
            return comparison
        raise AssertionError(f"unexpected gh api call: {path}")

    gh.seen = seen
    return gh


def ref(name: str, sha: str = TIP) -> dict:
    return {"ref": f"refs/heads/{name}", "object": {"sha": sha}}


def comparison(*commits: dict, ahead_by: int | None = None) -> dict:
    return {"ahead_by": len(commits) if ahead_by is None else ahead_by, "commits": list(commits)}


class InspectTest(unittest.TestCase):
    def inspect(self, gh) -> tuple[str, str]:
        return db.inspect(gh, REPO, "main", BRANCH, BOT)

    def test_absent_branch(self):
        gh = fake_gh([])
        self.assertEqual(self.inspect(gh), ("absent", ""))
        self.assertEqual(len(gh.seen), 1, "no comparison is needed for an absent branch")

    def test_prefix_match_is_not_the_branch(self):
        self.assertEqual(self.inspect(fake_gh([ref(f"{BRANCH}-old", "b" * 40)])), ("absent", ""))

    def test_workflow_commit_only(self):
        gh = fake_gh([ref(BRANCH)], comparison(commit()))
        self.assertEqual(self.inspect(gh), ("bot-only", TIP))

    def test_exact_ref_chosen_among_prefix_matches(self):
        gh = fake_gh([ref(f"{BRANCH}-old", "b" * 40), ref(BRANCH)], comparison(commit()))
        self.assertEqual(self.inspect(gh), ("bot-only", TIP))

    def test_nothing_beyond_base(self):
        self.assertEqual(self.inspect(fake_gh([ref(BRANCH)], comparison())), ("bot-only", TIP))

    def test_hand_authored_commit_on_top(self):
        gh = fake_gh([ref(BRANCH)], comparison(commit(), commit(author=HUMAN, committer=HUMAN)))
        self.assertEqual(self.inspect(gh), ("hand-edited", TIP))

    def test_amended_workflow_commit_keeps_bot_author(self):
        # `git commit --amend` keeps the author and replaces the committer.
        gh = fake_gh([ref(BRANCH)], comparison(commit(committer=HUMAN)))
        self.assertEqual(self.inspect(gh), ("hand-edited", TIP))

    def test_web_edit_committed_by_github(self):
        gh = fake_gh([ref(BRANCH)], comparison(commit(author=HUMAN, committer="noreply@github.com")))
        self.assertEqual(self.inspect(gh), ("hand-edited", TIP))

    def test_commits_beyond_the_compare_listing(self):
        gh = fake_gh([ref(BRANCH)], comparison(commit(), ahead_by=251))
        self.assertEqual(self.inspect(gh), ("hand-edited", TIP))


class MainTest(unittest.TestCase):
    ARGS = ["--repo", REPO, "--base", "main", "--branch", BRANCH, "--bot-email", BOT]

    def test_writes_github_output(self):
        with tempfile.TemporaryDirectory() as tmp:
            output = Path(tmp) / "output"
            output.write_text("earlier=1\n", encoding="utf-8")
            os.environ["GITHUB_OUTPUT"] = str(output)
            try:
                db.main(self.ARGS, gh=fake_gh([ref(BRANCH)], comparison(commit())))
            finally:
                del os.environ["GITHUB_OUTPUT"]
            self.assertEqual(output.read_text(encoding="utf-8"), f"earlier=1\nstate=bot-only\nsha={TIP}\n")

    def test_writes_stdout_without_github_output(self):
        os.environ.pop("GITHUB_OUTPUT", None)
        out = io.StringIO()
        with contextlib.redirect_stdout(out):
            db.main(self.ARGS, gh=fake_gh([]))
        self.assertEqual(out.getvalue(), "state=absent\nsha=\n")


if __name__ == "__main__":
    unittest.main()
