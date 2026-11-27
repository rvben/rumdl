"""Exercise the link checks with Git revisions and a local HTTP server."""

from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
from threading import Thread
from time import sleep
import unittest


ROOT = Path(__file__).resolve().parents[1]


class LinkCheckTest(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.repo = Path(temporary.name)
        for name in ("docs", "scripts", "assets"):
            (self.repo / name).mkdir()
        for name in (".lychee.toml", "Makefile", "scripts/check_links.py"):
            shutil.copyfile(ROOT / name, self.repo / name)

        self.requests = []
        requests = self.requests

        class Handler(BaseHTTPRequestHandler):
            def do_GET(self):
                requests.append(self.path)
                if self.path.endswith("/slow"):
                    sleep(3)
                self.send_response(int(self.path.split("/")[1]))
                self.end_headers()

            def log_message(self, *_):
                pass

        server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
        self.addCleanup(server.server_close)
        Thread(target=server.serve_forever, daemon=True).start()
        self.addCleanup(server.shutdown)
        self.url = f"http://127.0.0.1:{server.server_port}"

        self.git("init", "-q", "--template=")
        self.git("config", "user.email", "test@example.com")
        self.git("config", "user.name", "Test")
        self.git("config", "commit.gpgsign", "false")
        (self.repo / "docs/index.md").write_text("# Welcome\n", encoding="utf-8")
        (self.repo / "assets/target.txt").write_text("target\n", encoding="utf-8")
        self.write_readme("[Welcome](https://rumdl.dev/#welcome)\n[File](assets/target.txt)\n", 404)
        self.before = self.commit()

    def git(self, *args):
        return subprocess.check_output(["git", *args], cwd=self.repo, text=True).strip()

    def commit(self):
        self.git("add", "--", ".lychee.toml", "Makefile", "README.md", "docs/index.md", "scripts/check_links.py", "assets/target.txt")
        self.git("commit", "-qm", "Test snapshot")
        return self.git("rev-parse", "HEAD")

    def write_readme(self, local="", *codes):
        links = "".join(f"\n[External {code}]({self.url}/{code})\n" for code in codes)
        (self.repo / "README.md").write_text(f"# Readme\n\n{local}{links}", encoding="utf-8")

    def check_links(self, target="check-links", *make_args, **refs):
        env = os.environ.copy()
        for name in ("PRE_COMMIT_FROM_REF", "PRE_COMMIT_TO_REF", "PRE_COMMIT_REMOTE_NAME"):
            env.pop(name, None)
        env.update(refs)
        return subprocess.run(
            ["make", target, *make_args],
            cwd=self.repo,
            env=env,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            text=True,
            check=False,
        )

    def test_unchanged_external_link_is_never_requested(self):
        result = self.check_links()
        self.assertEqual(result.returncode, 0, result.stdout)
        self.assertIn("No new external links", result.stdout)
        self.assertEqual(self.requests, [])

    def test_new_tag_uses_remote_default_branch_as_the_baseline(self):
        self.git("update-ref", "refs/remotes/origin/main", self.before)
        self.git("symbolic-ref", "refs/remotes/origin/HEAD", "refs/remotes/origin/main")
        result = self.check_links(PRE_COMMIT_TO_REF=self.before, PRE_COMMIT_REMOTE_NAME="origin")
        self.assertEqual(result.returncode, 0, result.stdout)
        self.assertIn("No new external links", result.stdout)
        self.assertEqual(self.requests, [])

    def test_new_external_links_report_transient_statuses_and_fail_on_404(self):
        self.write_readme("", 404, 200, 408, 429, 503)
        result = self.check_links()
        self.assertEqual(result.returncode, 0, result.stdout)
        self.assertCountEqual(self.requests, ["/200", "/408", "/429", "/503"])
        for code in (408, 429, 503):
            self.assertIn(f"Inconclusive: {self.url}/{code} (HTTP {code})", result.stdout)

        self.write_readme(f"[New missing page]({self.url}/404/new)\n", 404)
        result = self.check_links()
        self.assertNotEqual(result.returncode, 0, result.stdout)
        self.assertIn("404 Not Found", result.stdout)
        self.assertEqual(self.requests.count("/404"), 0)

    def test_network_timeout_is_inconclusive_alongside_a_broken_link(self):
        slow = f"[Slow page]({self.url}/200/slow)\n"
        self.write_readme(slow, 404)
        result = self.check_links("check-links", "LYCHEE=lychee --timeout 1")
        self.assertEqual(result.returncode, 0, result.stdout)
        self.assertIn(f"Inconclusive: {self.url}/200/slow (Timeout)", result.stdout)

        self.write_readme(slow + f"[New missing page]({self.url}/404/new)\n", 404)
        result = self.check_links("check-links", "LYCHEE=lychee --timeout 1")
        self.assertNotEqual(result.returncode, 0, result.stdout)
        self.assertIn(f"Inconclusive: {self.url}/200/slow (Timeout)", result.stdout)
        self.assertIn("404 Not Found", result.stdout)

    def test_site_link_checks_fragment_offline(self):
        self.write_readme("[Missing heading](https://rumdl.dev/#missing)\n", 404)
        result = self.check_links()
        self.assertNotEqual(result.returncode, 0, result.stdout)
        self.assertIn("#missing", result.stdout)
        self.assertEqual(self.requests, [])

    def test_local_file_is_checked_against_the_pushed_commit(self):
        (self.repo / "assets/target.txt").unlink()
        after = self.commit()
        self.git("switch", "--detach", self.before)
        self.assertTrue((self.repo / "assets/target.txt").is_file())
        result = self.check_links(PRE_COMMIT_FROM_REF=self.before, PRE_COMMIT_TO_REF=after)
        self.assertNotEqual(result.returncode, 0, result.stdout)
        self.assertIn("target.txt", result.stdout)
        self.assertEqual(self.requests, [])

    def test_scheduled_scan_checks_existing_external_links(self):
        result = self.check_links("check-links-external")
        self.assertNotEqual(result.returncode, 0, result.stdout)
        self.assertIn("404 Not Found", result.stdout)
        self.assertEqual(self.requests, ["/404"])


if __name__ == "__main__":
    unittest.main()
