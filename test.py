"""CLI regression coverage for MD005 list boundaries (issue #970)."""

import os
from pathlib import Path
import subprocess
import tempfile
import unittest


class MD005ListBoundaries(unittest.TestCase):
    def check_markdown(self, content):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "l2.md"
            path.write_text(content, encoding="utf-8")
            result = subprocess.run(
                [os.environ["FABRIEK_RUMDL_BIN"], "check", "--no-cache",
                 "--enable", "MD005", str(path)],
                cwd=directory, capture_output=True, text=True, check=False,
            )
        self.assertIn(result.returncode, (0, 1), result.stdout + result.stderr)
        return result

    def test_adjacent_ordered_and_bullet_lists(self):
        result = self.check_markdown(
            "1. one\n2. two\n    1. a\n\n- one\n  - three\n    - four\n"
        )
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertNotIn("MD005", result.stdout + result.stderr)

    def test_list_boundary_variants(self):
        for indent in (3, 4):
            for marker in ("1.", "-"):
                for deeper in ("", "      - five\n"):
                    ordered = f"1. one\n2. two\n{' ' * indent}{marker} a\n"
                    bullets = f"- one\n  - three\n    - four\n{deeper}"
                    for first, second in ((ordered, bullets), (bullets, ordered)):
                        for gap in ("", "\n"):
                            content = first + gap + second
                            with self.subTest(content=content):
                                result = self.check_markdown(content)
                                self.assertEqual(
                                    result.returncode, 0, result.stdout + result.stderr
                                )

    def test_genuine_inconsistency_in_second_list(self):
        result = self.check_markdown(
            "1. one\n2. two\n    1. a\n\n- one\n    - three\n   - four\n"
        )
        self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
        self.assertIn(
            "l2.md:7:1: [MD005] Expected indentation of 4 spaces, found 3",
            result.stdout + result.stderr,
        )

    def test_valid_mixed_nested_list(self):
        result = self.check_markdown(
            "1. one\n   - two\n     1. three\n        - four\n"
            "   - five\n2. six\n"
        )
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)


if __name__ == "__main__":
    unittest.main()
