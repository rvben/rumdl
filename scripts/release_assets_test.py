#!/usr/bin/env python3
"""Regression tests for scripts/release_assets.py.

The script is the only thing standing between a release workflow re-run and a
silently replaced archive, and it runs only at release time, where a logic bug
surfaces after the tag is pushed. These tests run it everywhere else.

Hermetic: live release state is passed in as data and `gh` is never invoked.
Run with:

    python3 scripts/release_assets_test.py
"""

from __future__ import annotations

import contextlib
import hashlib
import importlib.util
import io
import json
import re
import sys
import tempfile
import unittest
from pathlib import Path

_SPEC = importlib.util.spec_from_file_location("release_assets", Path(__file__).with_name("release_assets.py"))
ra = importlib.util.module_from_spec(_SPEC)
sys.modules[_SPEC.name] = ra  # dataclasses resolve annotations through sys.modules
_SPEC.loader.exec_module(ra)

MAC = "rumdl-v1.2.3-aarch64-apple-darwin.tar.gz"
WIN = "rumdl-v1.2.3-x86_64-pc-windows-msvc.zip"
LINUX = "rumdl-v1.2.3-x86_64-unknown-linux-musl.tar.gz"

# The real v0.2.76 aarch64-apple-darwin digests from issue #909: the first
# upload, and the rebuild that replaced it.
ORIGINAL = "28f9af1569fac063af2f5adcadcdcfc23ba7eac4ceadd975bd381612e3ed24a6"
REBUILT = "10ec95ee46e1d3f67560250db97725681a5fa4bc488f383615cc54b06c4bdbc7"
OTHER = "f7efe829339cd70c13a5b9d0f334094e4c1c30f935fa143edd4d5d8839746655"


def sidecar(name: str) -> str:
    return name + ".sha256"


def unix_sidecar(digest: str, name: str) -> str:
    return f"{digest}  {name}\n"


class ParseSidecarTest(unittest.TestCase):
    def test_sha256sum_format(self):
        self.assertEqual(ra.parse_sidecar(unix_sidecar(ORIGINAL, MAC), "x"), ORIGINAL)

    def test_powershell_format_is_uppercase_with_crlf(self):
        self.assertEqual(ra.parse_sidecar(OTHER.upper() + "\r\n", "x"), OTHER)

    def test_empty_file_is_an_error(self):
        with self.assertRaisesRegex(ra.ReleaseAssetError, "empty"):
            ra.parse_sidecar("\n", "x")

    def test_non_digest_is_an_error_not_a_guess(self):
        for text in ("Not Found\n", ORIGINAL[:-1] + "  f\n", "sha256:" + ORIGINAL):
            with self.subTest(text=text), self.assertRaises(ra.ReleaseAssetError):
                ra.parse_sidecar(text, "x")


class NormalizeDigestTest(unittest.TestCase):
    def test_github_digest(self):
        self.assertEqual(ra.normalize_digest("sha256:" + ORIGINAL), ORIGINAL)

    def test_missing_digest_stays_missing(self):
        self.assertIsNone(ra.normalize_digest(None))
        self.assertIsNone(ra.normalize_digest(""))

    def test_unknown_algorithm_is_an_error(self):
        with self.assertRaises(ra.ReleaseAssetError):
            ra.normalize_digest("sha512:" + ORIGINAL)


class MakePlanTest(unittest.TestCase):
    def test_first_publish_uploads_everything(self):
        plan = ra.make_plan([MAC, WIN], {})
        self.assertEqual(plan.upload, [MAC, WIN])
        self.assertEqual(plan.kept, {})
        self.assertTrue(plan.new_archives)

    def test_backfill_over_a_published_release_uploads_nothing(self):
        # The #909 run: every archive already live, so the rebuild is discarded
        # and consumers of the archives are not re-notified.
        live = {MAC: ORIGINAL, sidecar(MAC): None, WIN: OTHER, sidecar(WIN): None}
        plan = ra.make_plan([MAC, WIN], live)
        self.assertEqual(plan.upload, [])
        self.assertEqual(plan.kept, {MAC: ORIGINAL, WIN: OTHER})
        self.assertFalse(plan.new_archives)

    def test_partial_release_uploads_only_the_missing_platforms(self):
        live = {MAC: ORIGINAL, sidecar(MAC): None}
        plan = ra.make_plan([MAC, WIN], live)
        self.assertEqual(plan.upload, [WIN])
        self.assertEqual(plan.kept, {MAC: ORIGINAL})
        self.assertTrue(plan.new_archives)

    def test_archive_without_its_sidecar_is_refused(self):
        with self.assertRaisesRegex(ra.ReleaseAssetError, MAC.replace(".", r"\.")):
            ra.make_plan([MAC], {MAC: ORIGINAL})

    def test_sidecar_without_its_archive_is_refused(self):
        with self.assertRaisesRegex(ra.ReleaseAssetError, "without their archive/checksum partner"):
            ra.make_plan([MAC], {sidecar(MAC): None})

    def test_live_archive_this_run_did_not_build_is_left_alone(self):
        live = {LINUX: OTHER, sidecar(LINUX): None}
        plan = ra.make_plan([MAC], live)
        self.assertEqual(plan.upload, [MAC])
        self.assertEqual(plan.live_only, [LINUX])

    def test_sidecars_are_not_mistaken_for_archives(self):
        self.assertFalse(ra.is_archive(sidecar(MAC)))
        self.assertFalse(ra.is_archive(sidecar(WIN)))


class VerifyReleaseTest(unittest.TestCase):
    def verify(self, local, live, snapshot, sidecars, hashes=None):
        def read_sidecar(name):
            return sidecars[name]

        def hash_asset(name):
            if hashes is None or name not in hashes:
                raise AssertionError(f"unexpected download of {name}")
            return hashes[name]

        return ra.verify_release(local, live, snapshot, read_sidecar, hash_asset)

    def test_untouched_backfill_is_clean(self):
        live = {MAC: ORIGINAL, sidecar(MAC): None}
        sidecars = {sidecar(MAC): unix_sidecar(ORIGINAL, MAC)}
        self.assertEqual(self.verify([MAC], live, {MAC: ORIGINAL}, sidecars), [])

    def test_replaced_archive_is_the_909_failure(self):
        # What overwrite_files: true did to v0.2.76: archive and sidecar both
        # replaced, so they agree with each other. Only the snapshot catches it.
        live = {MAC: REBUILT, sidecar(MAC): None}
        sidecars = {sidecar(MAC): unix_sidecar(REBUILT, MAC)}
        problems = self.verify([MAC], live, {MAC: ORIGINAL}, sidecars)
        self.assertEqual(len(problems), 1)
        self.assertIn("replaced during this run", problems[0])
        self.assertIn(ORIGINAL, problems[0])
        self.assertIn(REBUILT, problems[0])

    def test_archive_and_sidecar_that_disagree(self):
        live = {MAC: ORIGINAL, sidecar(MAC): None}
        sidecars = {sidecar(MAC): unix_sidecar(REBUILT, MAC)}
        problems = self.verify([MAC], live, {}, sidecars)
        self.assertEqual(len(problems), 1)
        self.assertIn("does not match its checksum file", problems[0])

    def test_windows_sidecar_matches_case_insensitively(self):
        live = {WIN: OTHER, sidecar(WIN): None}
        sidecars = {sidecar(WIN): OTHER.upper() + "\r\n"}
        self.assertEqual(self.verify([WIN], live, {}, sidecars), [])

    def test_archive_missing_after_upload(self):
        problems = self.verify([MAC], {}, {}, {})
        self.assertEqual(
            problems,
            [f"{MAC}: not on the release after the upload", f"{sidecar(MAC)}: not on the release after the upload"],
        )

    def test_previously_live_archive_that_vanished(self):
        live = {WIN: OTHER, sidecar(WIN): None}
        sidecars = {sidecar(WIN): unix_sidecar(OTHER, WIN)}
        problems = self.verify([WIN], live, {MAC: ORIGINAL}, sidecars)
        self.assertEqual(problems, [f"{MAC}: was live before the upload and is gone now"])

    def test_archive_without_recorded_digest_is_downloaded_and_hashed(self):
        live = {MAC: None, sidecar(MAC): None}
        sidecars = {sidecar(MAC): unix_sidecar(ORIGINAL, MAC)}
        self.assertEqual(self.verify([MAC], live, {}, sidecars, hashes={MAC: ORIGINAL}), [])
        problems = self.verify([MAC], live, {}, sidecars, hashes={MAC: REBUILT})
        self.assertEqual(len(problems), 1)
        self.assertIn("does not match its checksum file", problems[0])


class LocalArchivesTest(unittest.TestCase):
    def build(self, root: Path, job: str, name: str, data: bytes, recorded: str | None = None):
        d = root / job
        d.mkdir(parents=True, exist_ok=True)
        (d / name).write_bytes(data)
        digest = recorded or hashlib.sha256(data).hexdigest()
        (d / sidecar(name)).write_text(unix_sidecar(digest, name))

    def test_collects_archives_across_jobs(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            self.build(root, "release-aarch64-apple-darwin", MAC, b"mac")
            self.build(root, "release-x86_64-pc-windows-msvc", WIN, b"win")
            (root / "wheel-x").mkdir()
            (root / "wheel-x" / "rumdl-1.2.3.tar.gz").write_bytes(b"not a release archive")
            self.assertEqual(sorted(ra.local_archives(root)), [MAC, WIN])

    def test_archive_that_does_not_match_its_own_sidecar(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            self.build(root, "release-aarch64-apple-darwin", MAC, b"mac", recorded=ORIGINAL)
            with self.assertRaisesRegex(ra.ReleaseAssetError, "hashes to"):
                ra.local_archives(root)

    def test_archive_built_without_a_sidecar(self):
        with tempfile.TemporaryDirectory() as tmp:
            d = Path(tmp) / "release-aarch64-apple-darwin"
            d.mkdir()
            (d / MAC).write_bytes(b"mac")
            with self.assertRaisesRegex(ra.ReleaseAssetError, "without its"):
                ra.local_archives(Path(tmp))

    def test_no_archives_is_an_error(self):
        with tempfile.TemporaryDirectory() as tmp, self.assertRaisesRegex(ra.ReleaseAssetError, "no release archives"):
            ra.local_archives(Path(tmp))


class CommandTest(unittest.TestCase):
    """The plan -> upload -> verify sequence through the real entry points."""

    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.root = Path(self.tmp.name)
        self.artifacts = self.root / "artifacts"
        d = self.artifacts / "release-aarch64-apple-darwin"
        d.mkdir(parents=True)
        self.rebuilt = b"rebuilt bytes"
        (d / MAC).write_bytes(self.rebuilt)
        (d / sidecar(MAC)).write_text(unix_sidecar(hashlib.sha256(self.rebuilt).hexdigest(), MAC))
        self.snapshot = self.root / "snapshot.json"
        self.output = self.root / "github_output"
        self.output.write_text("")
        self.live = {}
        self.sidecars = {}
        self._orig = (ra.fetch_live, ra._download)
        ra.fetch_live = lambda tag: dict(self.live)
        ra._download = lambda tag, name: self.sidecars[name].encode()
        self._env = ra.os.environ.get("GITHUB_OUTPUT")
        ra.os.environ["GITHUB_OUTPUT"] = str(self.output)

    def tearDown(self):
        ra.fetch_live, ra._download = self._orig
        if self._env is None:
            ra.os.environ.pop("GITHUB_OUTPUT", None)
        else:
            ra.os.environ["GITHUB_OUTPUT"] = self._env
        self.tmp.cleanup()

    def run_cmd(self, command):
        args = [command, "--tag", "v1.2.3", "--artifacts", str(self.artifacts), "--snapshot", str(self.snapshot)]
        self.stdout = io.StringIO()
        with contextlib.redirect_stdout(self.stdout):
            return ra.main(args)

    def test_backfill_keeps_the_original_and_verifies(self):
        self.live = {MAC: ORIGINAL, sidecar(MAC): None}
        self.sidecars = {sidecar(MAC): unix_sidecar(ORIGINAL, MAC)}
        self.assertEqual(self.run_cmd("plan"), 0)
        self.assertEqual(self.output.read_text(), "new_archives=false\n")
        self.assertEqual(json.loads(self.snapshot.read_text()), {MAC: ORIGINAL})
        # overwrite_files: false leaves the release untouched.
        self.assertEqual(self.run_cmd("verify"), 0)

    def test_verify_fails_when_the_upload_overwrote(self):
        self.live = {MAC: ORIGINAL, sidecar(MAC): None}
        self.assertEqual(self.run_cmd("plan"), 0)
        rebuilt = hashlib.sha256(self.rebuilt).hexdigest()
        self.live = {MAC: rebuilt, sidecar(MAC): None}
        self.sidecars = {sidecar(MAC): unix_sidecar(rebuilt, MAC)}
        self.assertEqual(self.run_cmd("verify"), 1)
        self.assertIn(f"::error::{MAC}: replaced during this run (was {ORIGINAL}, now {rebuilt})", self.stdout.getvalue())

    def test_first_publish_signals_new_archives(self):
        self.assertEqual(self.run_cmd("plan"), 0)
        self.assertEqual(self.output.read_text(), "new_archives=true\n")
        self.assertEqual(json.loads(self.snapshot.read_text()), {})
        rebuilt = hashlib.sha256(self.rebuilt).hexdigest()
        self.live = {MAC: rebuilt, sidecar(MAC): None}
        self.sidecars = {sidecar(MAC): unix_sidecar(rebuilt, MAC)}
        self.assertEqual(self.run_cmd("verify"), 0)

    def test_half_published_pair_fails_plan_before_any_upload(self):
        self.live = {MAC: ORIGINAL}
        self.assertEqual(self.run_cmd("plan"), 1)
        self.assertIn("::error::these assets are live without their archive/checksum partner", self.stdout.getvalue())
        self.assertFalse(self.snapshot.exists())
        self.assertEqual(self.output.read_text(), "")


# The v0.2.77 tag run whose PyPI sdist upload hit the project size limit after
# every wheel was published: the release job started, then failed.
TAG = "v0.2.77"
SHA = "bb84a49a85625b31ecf6c3e58b7bca7c75fbfefb"
REPO = "rvben/rumdl"
TARGETS = ra.BUILD_TARGETS


def source_run(**overrides):
    run = {
        "id": 35864801239,
        "path": ".github/workflows/release.yml",
        "repository": {"full_name": REPO},
        "head_repository": {"full_name": REPO},
        "head_branch": TAG,
        "head_sha": SHA,
        "status": "completed",
        "conclusion": "failure",
        "html_url": "https://github.com/rvben/rumdl/actions/runs/35864801239",
    }
    run.update(overrides)
    return run


def release_job(conclusion="failure", steps=("success", "success", "failure")):
    return {"name": "release", "conclusion": conclusion, "steps": [{"conclusion": c} for c in steps]}


def source_jobs(release=None):
    jobs = [{"name": f"Build {t}", "conclusion": "success", "steps": [{"conclusion": "success"}]} for t in TARGETS]
    return jobs + [release if release is not None else release_job()]


def source_artifacts(drop=(), expired=()):
    names = [f"{kind}-{t}" for t in TARGETS for kind in ("wheel", "release")]
    names += ["sdist", "wasm-pkg", "npm-cli-packages"]
    return [{"name": n, "expired": n in expired} for n in names if n not in drop]


class CheckSourceRunTest(unittest.TestCase):
    def check(self, run=None, jobs=None, artifacts=None, tag=TAG, sha=SHA):
        return ra.check_source_run(
            run if run is not None else source_run(),
            jobs if jobs is not None else source_jobs(),
            artifacts if artifacts is not None else source_artifacts(),
            repo=REPO,
            tag=tag,
            sha=sha,
        )

    def assertOnlyProblem(self, problems, fragment):
        self.assertEqual(len(problems), 1, problems)
        self.assertIn(fragment, problems[0])

    def test_run_whose_publish_failed_after_the_gates_is_accepted(self):
        self.assertEqual(self.check(), [])

    def test_successful_run_is_accepted(self):
        run = source_run(conclusion="success")
        self.assertEqual(self.check(run=run, jobs=source_jobs(release_job("success", ("success",) * 3))), [])

    def test_other_workflow_is_refused(self):
        self.assertOnlyProblem(self.check(run=source_run(path=".github/workflows/ci.yml")), "not .github/workflows/release.yml")

    def test_run_from_a_fork_is_refused(self):
        problems = self.check(run=source_run(head_repository={"full_name": "someone/rumdl"}))
        self.assertOnlyProblem(problems, "head repository is 'someone/rumdl'")

    def test_dry_run_of_the_same_commit_on_another_ref_is_refused(self):
        # A throwaway dry-run tag can point at the release commit, but its
        # archives are named for that tag, not the release.
        self.assertOnlyProblem(self.check(run=source_run(head_branch="ci-dryrun-x")), "ran on 'ci-dryrun-x', not v0.2.77")

    def test_other_commit_is_refused(self):
        self.assertOnlyProblem(self.check(sha="0" * 40), "not v0.2.77's commit")

    def test_unfinished_run_is_refused(self):
        self.assertOnlyProblem(self.check(run=source_run(status="in_progress", conclusion=None)), "still in_progress")

    def test_run_that_failed_before_the_release_job_is_refused(self):
        # A failed test or build skips the release job, which then has no steps.
        problems = self.check(jobs=source_jobs(release_job("skipped", ())))
        self.assertOnlyProblem(problems, "never started its release job")

    def test_run_cancelled_before_the_release_job_started_is_refused(self):
        problems = self.check(jobs=source_jobs(release_job("cancelled", ())))
        self.assertOnlyProblem(problems, "never started its release job")

    def test_run_without_a_release_job_is_refused(self):
        jobs = [j for j in source_jobs() if j["name"] != "release"]
        self.assertOnlyProblem(self.check(jobs=jobs), "never started its release job")

    def test_expired_artifact_is_refused(self):
        problems = self.check(artifacts=source_artifacts(expired=("wheel-x86_64-pc-windows-msvc",)))
        self.assertOnlyProblem(problems, "artifact wheel-x86_64-pc-windows-msvc has expired")

    def test_each_required_artifact_is_required(self):
        for name in ra.REQUIRED_ARTIFACTS:
            with self.subTest(name=name):
                self.assertOnlyProblem(self.check(artifacts=source_artifacts(drop=(name,))), f"no {name} artifact")

    def test_archive_without_its_wheel_is_refused(self):
        problems = self.check(artifacts=source_artifacts(drop=("wheel-x86_64-unknown-linux-musl",)))
        self.assertOnlyProblem(problems, "no wheel-x86_64-unknown-linux-musl artifact")

    def test_wheel_without_its_archive_is_refused(self):
        problems = self.check(artifacts=source_artifacts(drop=("release-aarch64-apple-darwin",)))
        self.assertOnlyProblem(problems, "no release-aarch64-apple-darwin artifact")

    def test_target_missing_both_artifacts_is_refused(self):
        target = "x86_64-pc-windows-msvc"
        problems = self.check(artifacts=source_artifacts(drop=(f"release-{target}", f"wheel-{target}")))
        self.assertEqual(
            problems,
            [
                f"run 35864801239 has no release-{target} artifact",
                f"run 35864801239 has no wheel-{target} artifact",
            ],
        )

    def test_build_targets_match_the_release_workflow_matrix(self):
        workflow = (Path(__file__).resolve().parent.parent / ra.RELEASE_WORKFLOW).read_text()
        build = re.search(r"^  build:\n(.*?)^  \S", workflow, re.M | re.S)
        self.assertIsNotNone(build, "no top-level build job in release.yml")
        matrix = tuple(re.findall(r"^ +target: (\S+)$", build.group(1), re.M))
        self.assertEqual(matrix, ra.BUILD_TARGETS)

    def test_recovery_run_that_built_nothing_is_refused(self):
        # A from_run recovery run starts its release job but uploads no
        # artifacts of its own, so it cannot be the source of another one.
        problems = self.check(artifacts=[])
        self.assertIn("has no release-* archives", "\n".join(problems))
        for name in ra.REQUIRED_ARTIFACTS:
            self.assertIn(f"no {name} artifact", "\n".join(problems))


class CheckSourceRunCommandTest(unittest.TestCase):
    """check-source-run through the real entry point, with `gh` replaced."""

    def setUp(self):
        self.responses = {}
        self.calls = []
        self._orig = ra._gh
        ra._gh = self.fake_gh
        self._env = ra.os.environ.get("GH_REPO")
        ra.os.environ["GH_REPO"] = REPO

    def tearDown(self):
        ra._gh = self._orig
        if self._env is None:
            ra.os.environ.pop("GH_REPO", None)
        else:
            ra.os.environ["GH_REPO"] = self._env

    def fake_gh(self, *args):
        self.calls.append(args)
        path = args[-1]
        for suffix, (code, body) in self.responses.items():
            if path.endswith(suffix):
                return ra.subprocess.CompletedProcess(args, code, stdout=body, stderr="" if code == 0 else body)
        raise AssertionError(f"unexpected gh call {args}")

    def serve(self, run, jobs, artifacts):
        base = "repos/rvben/rumdl/actions/runs/35864801239"
        self.responses = {
            base: (0, json.dumps(run)),
            "/jobs?filter=latest&per_page=100": (0, json.dumps([{"jobs": jobs}])),
            "/artifacts?per_page=100": (0, json.dumps([{"artifacts": artifacts[:4]}, {"artifacts": artifacts[4:]}])),
        }

    def run_cmd(self, sha=SHA):
        self.stdout = io.StringIO()
        with contextlib.redirect_stdout(self.stdout):
            return ra.main(["check-source-run", "--run", "35864801239", "--tag", TAG, "--sha", sha])

    def test_valid_source_run_passes_across_artifact_pages(self):
        self.serve(source_run(), source_jobs(), source_artifacts())
        self.assertEqual(self.run_cmd(), 0, self.stdout.getvalue())
        self.assertIn(f"{2 * len(TARGETS) + 3} artifacts built from", self.stdout.getvalue())
        self.assertTrue(all("--paginate" in call for call in self.calls[1:]), self.calls)

    def test_mismatched_commit_fails_with_error_annotation(self):
        self.serve(source_run(), source_jobs(), source_artifacts())
        self.assertEqual(self.run_cmd(sha="0" * 40), 1)
        self.assertIn("::error::run 35864801239 built bb84a49a", self.stdout.getvalue())

    def test_unknown_run_fails(self):
        self.responses = {"repos/rvben/rumdl/actions/runs/35864801239": (1, "gh: Not Found (HTTP 404)")}
        self.assertEqual(self.run_cmd(), 1)
        self.assertIn("::error::run 35864801239 not found in rvben/rumdl", self.stdout.getvalue())

    def test_non_numeric_run_id_is_rejected_by_the_parser(self):
        with contextlib.redirect_stderr(io.StringIO()), self.assertRaises(SystemExit):
            ra.main(["check-source-run", "--run", "123;id", "--tag", TAG, "--sha", SHA])


if __name__ == "__main__":
    unittest.main()
