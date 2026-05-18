#!/usr/bin/env python3
"""Regression tests for scripts/check-rule-docs.py.

The rule-doc guard is itself the thing that prevents doc drift, and it has
already shipped two logic bugs (an unwrapped count that escaped the sentinel
check, and a regex that missed the dominant "N lint rules" phrasing). A guard
that critical needs its own coverage.

These tests are hermetic: they exercise the check functions against tmp-dir
fixtures with synthetic rule ids and a minimal DOC_FILES set, so no `cargo`
build or registry access is required. Run with:

    python3 scripts/test_check_rule_docs.py
"""

from __future__ import annotations

import importlib.util
import tempfile
import unittest
from contextlib import contextmanager
from pathlib import Path

_SPEC = importlib.util.spec_from_file_location(
    "check_rule_docs", Path(__file__).with_name("check-rule-docs.py")
)
crd = importlib.util.module_from_spec(_SPEC)
_SPEC.loader.exec_module(crd)


@contextmanager
def doc_root(doc_text: str = "", rules_text: str = ""):
    """Tmp dir with a single doc file and a rules.md, plus patched module
    constants so the guard scans exactly these fixtures."""
    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp)
        (root / "doc.md").write_text(doc_text)
        (root / "rules.md").write_text(rules_text)
        old_files, old_ref = crd.DOC_FILES, crd.RULES_REFERENCE
        crd.DOC_FILES = ["doc.md"]
        crd.RULES_REFERENCE = "rules.md"
        try:
            yield root
        finally:
            crd.DOC_FILES, crd.RULES_REFERENCE = old_files, old_ref


def table(ids: list[str]) -> str:
    return "\n".join(f"| [{i}]({i.lower()}.md) | x | y |" for i in ids)


SENTINELS = (
    "<!-- RULE_COUNT -->74<!-- /RULE_COUNT --> "
    "<!-- RULE_COUNT_ADDITIONAL -->21<!-- /RULE_COUNT_ADDITIONAL --> "
    "<!-- RULE_MAX -->MD080<!-- /RULE_MAX -->"
)


class ExpectedValues(unittest.TestCase):
    def test_math_from_registry_ids(self):
        # 74 contiguous ids: total 74, additional 74 - 53 = 21, max MD074.
        vals = crd.expected_values(["MD%03d" % n for n in range(1, 75)])
        self.assertEqual(vals["RULE_COUNT"], "74")
        self.assertEqual(vals["RULE_COUNT_ADDITIONAL"], "21")
        self.assertEqual(vals["RULE_MAX"], "MD074")
        # RULE_MAX is the lexicographic max of the id strings, not the count.
        self.assertEqual(
            crd.expected_values(["MD001", "MD050", "MD080"])["RULE_MAX"], "MD080"
        )


class Sentinels(unittest.TestCase):
    VALUES = {
        "RULE_COUNT": "74",
        "RULE_COUNT_ADDITIONAL": "21",
        "RULE_MAX": "MD080",
    }

    def test_in_sync_is_clean(self):
        with doc_root(doc_text=SENTINELS) as root:
            self.assertEqual(crd.check_sentinels(self.VALUES, root), [])

    def test_drift_is_flagged(self):
        drift_doc = SENTINELS.replace(
            "<!-- RULE_COUNT -->74<!-- /RULE_COUNT -->",
            "<!-- RULE_COUNT -->71<!-- /RULE_COUNT -->",
        )
        with doc_root(doc_text=drift_doc) as root:
            problems = crd.check_sentinels(self.VALUES, root)
            self.assertEqual(len(problems), 1)
            self.assertIn("RULE_COUNT is '71', expected '74'", problems[0])

    def test_write_repairs_drift(self):
        drift_doc = SENTINELS.replace(
            "<!-- RULE_MAX -->MD080<!-- /RULE_MAX -->",
            "<!-- RULE_MAX -->MD077<!-- /RULE_MAX -->",
        )
        with doc_root(doc_text=drift_doc) as root:
            changed = crd.write_sentinels(self.VALUES, root)
            self.assertEqual(changed, ["doc.md"])
            self.assertEqual(crd.check_sentinels(self.VALUES, root), [])
            self.assertIn(
                "<!-- RULE_MAX -->MD080<!-- /RULE_MAX -->",
                (root / "doc.md").read_text(),
            )


class RulesTable(unittest.TestCase):
    def test_every_id_present_is_clean(self):
        ids = ["MD001", "MD050", "MD080"]
        with doc_root(rules_text=table(ids)) as root:
            self.assertEqual(crd.check_rules_table(ids, root), [])

    def test_missing_row_is_flagged(self):
        with doc_root(rules_text=table(["MD001", "MD050"])) as root:
            problems = crd.check_rules_table(["MD001", "MD050", "MD080"], root)
            self.assertEqual(len(problems), 1)
            self.assertIn("missing table rows for MD080", problems[0])

    def test_nonexistent_row_is_flagged(self):
        with doc_root(rules_text=table(["MD001", "MD050", "MD999"])) as root:
            problems = crd.check_rules_table(["MD001", "MD050"], root)
            self.assertEqual(len(problems), 1)
            self.assertIn("nonexistent rules MD999", problems[0])

    def test_duplicate_rows_allowed(self):
        # Opt-in rules legitimately appear in both the overview and category
        # tables; repetition must not be drift.
        dup = table(["MD001", "MD050"]) + "\n" + table(["MD050"])
        with doc_root(rules_text=dup) as root:
            self.assertEqual(crd.check_rules_table(["MD001", "MD050"], root), [])


class UnwrappedCounts(unittest.TestCase):
    def test_sentineled_count_not_flagged(self):
        with doc_root(doc_text=SENTINELS) as root:
            self.assertEqual(crd.check_no_unwrapped_counts(root), [])

    def test_unwrapped_bare_count_is_flagged(self):
        with doc_root(doc_text="rumdl has 99 rules today.") as root:
            problems = crd.check_no_unwrapped_counts(root)
            self.assertEqual(len(problems), 1)
            self.assertIn("unwrapped rule count '99 rules'", problems[0])

    def test_unwrapped_lint_rules_phrasing_is_flagged(self):
        # The regex bug that shipped: "N lint rules" must be caught.
        with doc_root(doc_text="rumdl provides 68 lint rules.") as root:
            problems = crd.check_no_unwrapped_counts(root)
            self.assertEqual(len(problems), 1)
            self.assertIn("68 lint rules", problems[0])

    def test_unwrapped_linting_rules_phrasing_is_flagged(self):
        with doc_root(doc_text="rumdl provides 68 linting rules.") as root:
            problems = crd.check_no_unwrapped_counts(root)
            self.assertEqual(len(problems), 1)
            self.assertIn("68 linting rules", problems[0])

    def test_competitor_counts_allowlisted(self):
        text = (
            "markdownlint ships 53 rules, pymarkdown 46 rules, mado 38 rules."
        )
        with doc_root(doc_text=text) as root:
            self.assertEqual(crd.check_no_unwrapped_counts(root), [])

    def test_markdownlint_rule_phrase_not_flagged(self):
        # "all 53 markdownlint rules" is an allowlisted competitor count and
        # must not be mistaken for an unsynced rumdl claim.
        with doc_root(doc_text="rumdl implements all 53 markdownlint rules") as root:
            self.assertEqual(crd.check_no_unwrapped_counts(root), [])


if __name__ == "__main__":
    unittest.main()
