"""Run with RUMDL=/path/to/rumdl python -m unittest discover -s examples/custom-rules."""
import ast
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

import mise_command

RUMDL = os.environ.get("RUMDL", str(Path(__file__).resolve().parents[2] / "target/debug/rumdl"))


def check(source):
    result = subprocess.run([RUMDL, "experimental", "export-elements", "--stdin",
                             "--stdin-filename", "guide.md"], input=source,
                            text=True, capture_output=True, check=True)
    return mise_command.check(json.loads(result.stdout))["diagnostics"]


class MiseCommandTests(unittest.TestCase):
    def test_instructional_commands(self):
        for command in ["python manage.py migrate", "$ pip install x", "cd app && pytest",
                        "mise exec -- python x.py && python y.py",
                        "cmd |& python f.py", "a &&(python x)"]:
            with self.subTest(command=command):
                self.assertEqual(len(check(f"```bash\n{command}\n```\n")), 1)
        self.assertEqual(len(check("Run `python3 run.py` to start.\n")), 1)

    def test_mentions_wrappers_comments_and_other_languages(self):
        for source in ["The `python` binary and `pip` command.\n",
                       "`pip install foo` is deprecated.\n",
                       "`python run.py` is legacy; `pip install foo` is an example.\n",
                       "```console\n$ mise exec -- pip --version\npip 24.0 from somewhere\n```\n",
                       "Run `python` to open the interpreter.\n",
                       "```bash\nmise exec -- python x.py\nmise x -- pip install x\n```\n",
                       "```python\npython = 'mention'\n```\n",
                       "```bash\n# python foo\necho 'python x.py'\n```\n"]:
            with self.subTest(source=source):
                self.assertEqual(check(source), [])

    def test_explicit_counterexamples_and_bootstrap_sections(self):
        for source in ["<!-- mise-check: counterexample -->\n\n```bash\npython bad.py\n```\n",
                       "# Installing mise\n\n```sh\npython bootstrap.py\n```\n",
                       "<!-- mise-check: bootstrap -->\n\nRun `python bootstrap.py`.\n"]:
            with self.subTest(source=source):
                self.assertEqual(check(source), [])
        self.assertEqual(len(check("# Installing mise\n\nBootstrap.\n\n# Use the app\n\n```bash\npython app.py\n```\n")), 1)

    def test_nested_blocks_and_original_utf8_ranges(self):
        source = "# Setup café 😀\r\n\r\n- Run this:\r\n\r\n  ```bash\r\n  python x.py\r\n  ```\r\n"
        findings = check(source)
        self.assertEqual(len(findings), 1)
        finding = findings[0]
        self.assertEqual(source.encode()[finding["start_byte"]:finding["end_byte"]], b"python x.py")

    def test_quoted_bootstrap_heading_does_not_exempt_later_instructions(self):
        source = "# Setup\n\n> # Installing mise\n>\n> ```sh\n> python bootstrap.py\n> ```\n\n```bash\npython app.py\n```\n"
        findings = check(source)
        self.assertEqual(len(findings), 1)
        self.assertEqual(source.encode()[findings[0]["start_byte"]:findings[0]["end_byte"]], b"python app.py")

    def test_instruction_inside_a_table_cell(self):
        source = "| Step | Command |\n| --- | --- |\n| 1 | Run `python x.py` |\n"
        self.assertEqual(len(check(source)), 1)

    def test_nested_prose_does_not_instruct_parent_code(self):
        self.assertEqual(check("- `pip install x` is legacy.\n  - Run the tests.\n"), [])
        self.assertEqual(check("- `pip install foo` is legacy.\n\n      python x.py\n"), [])
        self.assertEqual(len(check("- Setup\n\n  Run this:\n\n      pytest\n")), 1)

    def test_descriptive_inline_prose_overrides_previous_instruction(self):
        for prefix in ["Install the dependencies.", "## Install", "## Test"]:
            with self.subTest(prefix=prefix):
                self.assertEqual(check(f"{prefix}\n\n`pip install foo` is deprecated.\n"), [])
        self.assertEqual(len(check("Install the dependencies:\n\n`pip install foo`\n")), 1)

    def test_line_continuations_keep_wrapped_commands_together(self):
        wrapped = "```bash\nmise exec -- \\\n  python run.py\n```\n"
        self.assertEqual(check(wrapped), [])
        bare = "```bash\npython \\\n  run.py\n```\n"
        self.assertEqual(len(check(bare)), 1)

    def test_document_commands_are_never_executed(self):
        with tempfile.TemporaryDirectory() as directory:
            sentinel = Path(directory) / "must-not-exist"
            source = f'```bash\npython -c "from pathlib import Path; Path({str(sentinel)!r}).touch()"\n```\n'
            self.assertEqual(len(check(source)), 1)
            self.assertFalse(sentinel.exists())
        tree = ast.parse(Path(mise_command.__file__).read_text())
        imports = [node for node in ast.walk(tree) if isinstance(node, (ast.Import, ast.ImportFrom))]
        self.assertFalse(any((getattr(node, "module", "") or "").split(".")[0] in {"subprocess", "os"}
                             or any(alias.name.split(".")[0] in {"subprocess", "os"} for alias in node.names)
                             for node in imports))


if __name__ == "__main__":
    unittest.main()
