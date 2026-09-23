#!/usr/bin/env python3
"""Keep a GitHub Release's archives write-once across release workflow runs.

Rust builds are not byte-reproducible, so every run of the release workflow
produces archives with new digests even from an unchanged tag. A recovery
dispatch over an already-published release (a PyPI backfill, a re-run after a
later publish step failed) must therefore never replace an archive that is
already live: anything that pinned the first digest (mise, aqua, lockfiles,
Nix, the Homebrew tap) would then reject the download as tampered. The
registries enforce this themselves (crates.io, PyPI and npm keep the first
upload of a version); a GitHub Release does not, so this script does.

    plan    Before the upload. Compares the run's local archives with the live
            release, refuses a platform whose archive and .sha256 sidecar are
            only half published (uploading the missing half would pair a
            rebuilt checksum with the original archive, or the reverse), and
            records the live digests as a snapshot. Emits
            `new_archives=true|false` to $GITHUB_OUTPUT: whether this run
            publishes any archive, which decides whether consumers of the
            archives (Homebrew tap, VS Code extension) are notified.

    verify  After the upload. Every archive of this run is live, every live
            archive's digest equals the hash in its live sidecar, and every
            archive recorded in the plan snapshot still has its recorded
            digest.

The upload itself must skip existing assets (`overwrite_files: false` on
softprops/action-gh-release); `verify` is what proves it did.

    check-source-run
            Before a recovery run publishes anything from an earlier run's
            artifacts (the `from_run` dispatch input) instead of rebuilding.
            Republishing the original bytes keeps every store consistent: PyPI
            skips files it already has instead of rejecting a rebuilt
            duplicate, and nothing downstream sees a second digest. The source
            run must be this workflow, on this tag and commit, finished, past
            the gates of its own release job, with its artifacts unexpired.

Usage:
    release_assets.py plan   --tag vX.Y.Z --artifacts DIR --snapshot FILE
    release_assets.py verify --tag vX.Y.Z --artifacts DIR --snapshot FILE
    release_assets.py check-source-run --run ID --tag vX.Y.Z --sha SHA
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import subprocess
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import Callable

ARCHIVE_SUFFIXES = (".tar.gz", ".zip")
SIDECAR_SUFFIX = ".sha256"
_HEX64 = re.compile(r"^[0-9a-f]{64}$")


class ReleaseAssetError(Exception):
    """A condition that must stop the release before (or after) publishing."""


def is_archive(name: str) -> bool:
    return name.endswith(ARCHIVE_SUFFIXES)


def sidecar_name(archive: str) -> str:
    return archive + SIDECAR_SUFFIX


def parse_sidecar(text: str, source: str) -> str:
    """Return the lowercase hex digest a .sha256 sidecar records.

    Two formats are published: `sha256sum`/`shasum` output (`<hex>  <name>`)
    for the tarballs, and PowerShell's `Get-FileHash` (uppercase hex, CRLF) for
    the Windows zip. Anything else is an error, never a guess.
    """
    fields = text.split()
    if not fields:
        raise ReleaseAssetError(f"{source}: empty checksum file")
    digest = fields[0].lower()
    if not _HEX64.match(digest):
        raise ReleaseAssetError(f"{source}: first field is not a sha256 hex digest: {fields[0]!r}")
    return digest


def normalize_digest(digest: str | None) -> str | None:
    """GitHub reports asset digests as `sha256:<hex>`; older assets have none."""
    if not digest:
        return None
    algo, _, value = digest.partition(":")
    if algo != "sha256" or not _HEX64.match(value):
        raise ReleaseAssetError(f"unexpected asset digest format: {digest!r}")
    return value


@dataclass(frozen=True)
class Plan:
    upload: list[str]
    kept: dict[str, str | None]
    live_only: list[str]

    @property
    def new_archives(self) -> bool:
        return bool(self.upload)


def make_plan(local_archives: list[str], live: dict[str, str | None]) -> Plan:
    """Decide which of this run's archives may be uploaded.

    `live` maps every live asset name to its digest (None when GitHub has not
    recorded one). An archive is uploaded only when neither it nor its sidecar
    is live, and kept only when both are; a half-published pair is refused.
    """
    upload: list[str] = []
    kept: dict[str, str | None] = {}
    half: list[str] = []
    for archive in sorted(local_archives):
        has_archive = archive in live
        has_sidecar = sidecar_name(archive) in live
        if has_archive and has_sidecar:
            kept[archive] = live[archive]
        elif not has_archive and not has_sidecar:
            upload.append(archive)
        else:
            present = archive if has_archive else sidecar_name(archive)
            half.append(present)
    if half:
        raise ReleaseAssetError(
            "these assets are live without their archive/checksum partner: "
            + ", ".join(half)
            + ". Uploading the missing half from this run would pair a rebuilt file with the "
            "original, so the checksum would not match the archive. Delete the orphan by hand "
            "(nobody can have installed from an incomplete pair) and re-run."
        )
    local = set(local_archives)
    live_only = sorted(name for name in live if is_archive(name) and name not in local)
    return Plan(upload=upload, kept=kept, live_only=live_only)


def verify_release(
    local_archives: list[str],
    live: dict[str, str | None],
    snapshot: dict[str, str | None],
    read_sidecar: Callable[[str], str],
    hash_asset: Callable[[str], str],
) -> list[str]:
    """Return every violation of the write-once contract (empty when clean).

    `read_sidecar(name)` returns a live sidecar's text; `hash_asset(name)`
    downloads a live archive and hashes it, used only when GitHub has no
    digest on record for it.
    """
    problems: list[str] = []
    for archive in sorted(local_archives):
        if archive not in live:
            problems.append(f"{archive}: not on the release after the upload")
        if sidecar_name(archive) not in live:
            problems.append(f"{sidecar_name(archive)}: not on the release after the upload")

    for archive in sorted(name for name in live if is_archive(name)):
        if sidecar_name(archive) not in live:
            continue  # reported above for this run's archives
        actual = live[archive] or hash_asset(archive)
        recorded = parse_sidecar(read_sidecar(sidecar_name(archive)), sidecar_name(archive))
        if actual != recorded:
            problems.append(
                f"{archive}: live digest {actual} does not match its checksum file ({recorded})"
            )

    for archive, before in sorted(snapshot.items()):
        after = live.get(archive)
        if archive not in live:
            problems.append(f"{archive}: was live before the upload and is gone now")
        elif before is not None and after != before:
            problems.append(
                f"{archive}: replaced during this run (was {before}, now {after}); "
                "published archives must never change"
            )
    return problems


RELEASE_WORKFLOW = ".github/workflows/release.yml"
# Artifacts the publish steps read besides the per-target wheel-*/release-* pairs.
REQUIRED_ARTIFACTS = ("sdist", "wasm-pkg", "npm-cli-packages")
# The `build` job's matrix in release.yml; each target uploads release-<target>
# and wheel-<target>. release_assets_test.py fails when the two lists drift.
BUILD_TARGETS = (
    "x86_64-unknown-linux-gnu",
    "aarch64-unknown-linux-gnu",
    "x86_64-unknown-linux-musl",
    "aarch64-unknown-linux-musl",
    "x86_64-pc-windows-msvc",
    "x86_64-apple-darwin",
    "aarch64-apple-darwin",
)


def check_source_run(
    run: dict,
    jobs: list[dict],
    artifacts: list[dict],
    *,
    repo: str,
    tag: str,
    sha: str,
) -> list[str]:
    """Return every reason `run` cannot supply this release's artifacts (empty when it can).

    `run`, `jobs` and `artifacts` are the GitHub API objects for the source
    run, its latest attempt's jobs, and its artifacts.
    """
    rid = run.get("id")
    problems: list[str] = []
    if run.get("path") != RELEASE_WORKFLOW:
        problems.append(f"run {rid} is {run.get('path')!r}, not {RELEASE_WORKFLOW}")
    for key in ("repository", "head_repository"):
        full_name = (run.get(key) or {}).get("full_name")
        if full_name != repo:
            problems.append(f"run {rid} {key.replace('_', ' ')} is {full_name!r}, not {repo}")
    if run.get("head_branch") != tag:
        problems.append(
            f"run {rid} ran on {run.get('head_branch')!r}, not {tag}; its archives are named for that ref"
        )
    if run.get("head_sha") != sha:
        problems.append(f"run {rid} built {run.get('head_sha')}, not {tag}'s commit {sha}")
    if run.get("status") != "completed":
        problems.append(f"run {rid} is still {run.get('status')}; wait for it to finish")

    # The release job starts only once every build, test and package smoke test
    # it needs has succeeded, so a started release job is the proof that the
    # artifacts passed the same gates as a normal release.
    release_jobs = [job for job in jobs if job.get("name") == "release"]
    started = any(
        step.get("conclusion") == "success" for job in release_jobs for step in job.get("steps") or []
    )
    if not started:
        problems.append(
            f"run {rid} never started its release job, so its builds did not all pass the release gates"
        )

    names = {a["name"] for a in artifacts}
    for artifact in sorted(artifacts, key=lambda a: a["name"]):
        if artifact.get("expired"):
            problems.append(f"run {rid} artifact {artifact['name']} has expired")
    for name in REQUIRED_ARTIFACTS:
        if name not in names:
            problems.append(f"run {rid} has no {name} artifact")
    # Every target is required, not just those whose artifacts remain: a target
    # with both artifacts deleted would otherwise drop out of the release unseen.
    if not any(n.startswith("release-") for n in names):
        problems.append(
            f"run {rid} has no release-* archives (a recovery run that did not build has none to lend)"
        )
    for target in BUILD_TARGETS:
        for kind in ("release", "wheel"):
            if f"{kind}-{target}" not in names:
                problems.append(f"run {rid} has no {kind}-{target} artifact")
    return problems


def local_archives(artifacts: Path) -> dict[str, Path]:
    """This run's archives, keyed by asset name, each checked against its sidecar."""
    found: dict[str, Path] = {}
    for path in sorted(artifacts.glob("release-*/rumdl-*")):
        if not is_archive(path.name):
            continue
        if path.name in found:
            raise ReleaseAssetError(f"{path.name}: built by more than one job")
        sidecar = path.with_name(sidecar_name(path.name))
        if not sidecar.is_file():
            raise ReleaseAssetError(f"{path}: built without its {SIDECAR_SUFFIX} file")
        recorded = parse_sidecar(sidecar.read_text(), str(sidecar))
        actual = hashlib.sha256(path.read_bytes()).hexdigest()
        if recorded != actual:
            raise ReleaseAssetError(f"{sidecar}: records {recorded}, but the archive hashes to {actual}")
        found[path.name] = path
    if not found:
        raise ReleaseAssetError(f"no release archives under {artifacts}/release-*/")
    return found


def _gh(*args: str) -> subprocess.CompletedProcess[str]:
    return subprocess.run(["gh", *args], capture_output=True, text=True)


def fetch_live(tag: str) -> dict[str, str | None]:
    """Live assets of the release for `tag`; empty when no such release exists."""
    result = _gh("release", "view", tag, "--json", "assets")
    if result.returncode != 0:
        if result.stderr.strip() == "release not found":
            return {}
        raise ReleaseAssetError(f"gh release view {tag} failed: {result.stderr.strip()}")
    assets = json.loads(result.stdout)["assets"]
    return {a["name"]: normalize_digest(a.get("digest")) for a in assets}


def _download(tag: str, name: str) -> bytes:
    result = subprocess.run(
        ["gh", "release", "download", tag, "--pattern", name, "--output", "-"],
        capture_output=True,
    )
    if result.returncode != 0:
        raise ReleaseAssetError(f"downloading {name} failed: {result.stderr.decode().strip()}")
    return result.stdout


def _write_output(key: str, value: str) -> None:
    path = os.environ.get("GITHUB_OUTPUT")
    if path:
        with open(path, "a", encoding="utf-8") as fh:
            fh.write(f"{key}={value}\n")


def cmd_plan(args: argparse.Namespace) -> int:
    archives = local_archives(Path(args.artifacts))
    live = fetch_live(args.tag)
    plan = make_plan(list(archives), live)

    print(f"GitHub release {args.tag}: {len(live)} live assets")
    for name in plan.upload:
        print(f"  upload  {name}")
    for name, digest in plan.kept.items():
        print(f"  keep    {name} (sha256:{digest or 'unrecorded'}); this run's build is discarded")
    for name in plan.live_only:
        print(f"  note    {name} is live but this run did not build it; left as is")

    Path(args.snapshot).write_text(json.dumps(plan.kept, indent=2, sort_keys=True) + "\n")
    _write_output("new_archives", "true" if plan.new_archives else "false")
    print(f"new_archives={'true' if plan.new_archives else 'false'}")
    return 0


def cmd_verify(args: argparse.Namespace) -> int:
    archives = local_archives(Path(args.artifacts))
    live = fetch_live(args.tag)
    snapshot = json.loads(Path(args.snapshot).read_text())
    problems = verify_release(
        list(archives),
        live,
        snapshot,
        read_sidecar=lambda name: _download(args.tag, name).decode("utf-8"),
        hash_asset=lambda name: hashlib.sha256(_download(args.tag, name)).hexdigest(),
    )
    if problems:
        for problem in problems:
            print(f"::error::{problem}")
        return 1
    archive_count = sum(1 for name in live if is_archive(name))
    print(
        f"GitHub release {args.tag}: {archive_count} archives, each matching its checksum file; "
        f"{len(snapshot)} previously published archive(s) unchanged"
    )
    return 0


def _gh_api_pages(path: str) -> list[dict]:
    result = _gh("api", "--paginate", "--slurp", path)
    if result.returncode != 0:
        raise ReleaseAssetError(f"gh api {path} failed: {result.stderr.strip()}")
    return json.loads(result.stdout)


def cmd_check_source_run(args: argparse.Namespace) -> int:
    repo = os.environ.get("GH_REPO")
    if not repo:
        raise ReleaseAssetError("GH_REPO must name the repository (owner/name)")
    base = f"repos/{repo}/actions/runs/{args.run}"
    result = _gh("api", base)
    if result.returncode != 0:
        raise ReleaseAssetError(f"run {args.run} not found in {repo}: {result.stderr.strip()}")
    run = json.loads(result.stdout)
    jobs = [job for page in _gh_api_pages(f"{base}/jobs?filter=latest&per_page=100") for job in page["jobs"]]
    artifacts = [a for page in _gh_api_pages(f"{base}/artifacts?per_page=100") for a in page["artifacts"]]
    problems = check_source_run(run, jobs, artifacts, repo=repo, tag=args.tag, sha=args.sha)
    if problems:
        for problem in problems:
            print(f"::error::{problem}")
        return 1
    print(
        f"Publishing the artifacts of run {args.run} ({run.get('html_url')}): "
        f"{len(artifacts)} artifacts built from {args.sha} for {args.tag}; nothing is rebuilt"
    )
    return 0


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    sub = parser.add_subparsers(dest="command", required=True)
    for name, func in (("plan", cmd_plan), ("verify", cmd_verify)):
        p = sub.add_parser(name)
        p.add_argument("--tag", required=True)
        p.add_argument("--artifacts", required=True, help="directory holding release-*/ artifacts")
        p.add_argument("--snapshot", required=True, help="JSON file recording the digests kept by `plan`")
        p.set_defaults(func=func)
    p = sub.add_parser("check-source-run")
    p.add_argument("--run", required=True, type=int, help="id of the earlier release run")
    p.add_argument("--tag", required=True)
    p.add_argument("--sha", required=True, help="commit the tag points at ($GITHUB_SHA)")
    p.set_defaults(func=cmd_check_source_run)
    args = parser.parse_args(argv)
    try:
        return args.func(args)
    except ReleaseAssetError as exc:
        print(f"::error::{exc}")
        return 1


if __name__ == "__main__":
    sys.exit(main())
