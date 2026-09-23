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

Usage:
    release_assets.py plan   --tag vX.Y.Z --artifacts DIR --snapshot FILE
    release_assets.py verify --tag vX.Y.Z --artifacts DIR --snapshot FILE
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


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    sub = parser.add_subparsers(dest="command", required=True)
    for name, func in (("plan", cmd_plan), ("verify", cmd_verify)):
        p = sub.add_parser(name)
        p.add_argument("--tag", required=True)
        p.add_argument("--artifacts", required=True, help="directory holding release-*/ artifacts")
        p.add_argument("--snapshot", required=True, help="JSON file recording the digests kept by `plan`")
        p.set_defaults(func=func)
    args = parser.parse_args(argv)
    try:
        return args.func(args)
    except ReleaseAssetError as exc:
        print(f"::error::{exc}")
        return 1


if __name__ == "__main__":
    sys.exit(main())
