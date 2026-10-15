#!/usr/bin/env python3
"""Regression tests for the nextest profile coverage guard."""

import copy
import importlib.util
from pathlib import Path
import unittest

SPEC = importlib.util.spec_from_file_location(
    "check_test_selection", Path(__file__).with_name("check-test-selection.py")
)
guard = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(guard)


def case(*, selected=True, ignored=False):
    return {
        "ignored": ignored,
        "filter-match": {"status": "matches" if selected else "mismatch"},
    }


class ProfileCoverage(unittest.TestCase):
    def test_either_profile_covers_a_test(self):
        all_tests = {("lib", "ordinary"): case(), ("lib", "timed"): case()}
        ci = {("lib", "ordinary"): case(), ("lib", "timed"): case(selected=False)}
        performance = {("lib", "ordinary"): case(selected=False), ("lib", "timed"): case()}
        self.assertEqual(guard.uncovered_tests(all_tests, ci, performance), [])

    def test_reports_omitted_test_with_its_binary(self):
        all_tests = {("lib", "same_name"): case(), ("bin", "same_name"): case()}
        ci = {("lib", "same_name"): case(), ("bin", "same_name"): case(selected=False)}
        self.assertEqual(guard.uncovered_tests(all_tests, ci, ci), [("bin", "same_name")])

    def test_explicitly_ignored_tests_may_be_omitted(self):
        all_tests = {("lib", "manual"): case(ignored=True)}
        excluded = {("lib", "manual"): case(selected=False, ignored=True)}
        self.assertEqual(guard.uncovered_tests(all_tests, excluded, excluded), [])

    def test_different_inventories_fail_closed(self):
        all_tests = {("lib", "ordinary"): case()}
        with self.assertRaisesRegex(ValueError, "inventory differs"):
            guard.uncovered_tests(all_tests, {}, all_tests)

    def test_inconsistent_ignore_status_fails_closed(self):
        all_tests = {("lib", "ordinary"): case()}
        changed = copy.deepcopy(all_tests)
        changed["lib", "ordinary"]["ignored"] = True
        with self.assertRaisesRegex(ValueError, "ignored status"):
            guard.uncovered_tests(all_tests, changed, all_tests)


if __name__ == "__main__":
    unittest.main()
