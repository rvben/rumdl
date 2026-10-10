#!/usr/bin/env python3
"""Record download and executable sizes without extracting release packages.

Reports contain artifact basenames and build identifiers, never host paths.
No size budget is enforced: collect comparable release baselines first.
"""

from __future__ import annotations

import argparse
import json
import tarfile
import zipfile
from pathlib import Path, PurePosixPath


def executable_size(path: Path) -> int:
    """Read the single rumdl executable's uncompressed size from an archive."""
    if path.name.endswith((".whl", ".zip")):
        with zipfile.ZipFile(path) as archive:
            sizes = [
                member.file_size
                for member in archive.infolist()
                if not member.is_dir()
                and PurePosixPath(member.filename).name in ("rumdl", "rumdl.exe")
            ]
    elif path.name.endswith(".tar.gz"):
        with tarfile.open(path, "r:gz") as archive:
            sizes = [
                member.size
                for member in archive
                if member.isfile()
                and PurePosixPath(member.name).name in ("rumdl", "rumdl.exe")
            ]
    else:
        raise ValueError(f"unsupported package: {path.name}")
    if len(sizes) != 1:
        raise ValueError(
            f"{path.name}: expected one rumdl executable, found {len(sizes)}"
        )
    return sizes[0]


def measure(kind: str, path: Path) -> dict:
    return {
        "kind": kind,
        "name": path.name,
        "artifact_bytes": path.stat().st_size,
        "executable_bytes": (
            path.stat().st_size if kind == "binary" else executable_size(path)
        ),
    }


def markdown(report: dict) -> str:
    lines = [
        f"### Artifact sizes: {report['target']} ({report['version']})",
        "",
        "Package bytes measure download size; executable bytes exclude package compression and Python wrappers.",
        "",
        "| Kind | Artifact | Artifact bytes | Executable bytes |",
        "| --- | --- | ---: | ---: |",
    ]
    for row in report["artifacts"]:
        lines.append(
            f"| {row['kind']} | {row['name']} | {row['artifact_bytes']:,} | {row['executable_bytes']:,} |"
        )
    return "\n".join(lines) + "\n"


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--target", required=True)
    parser.add_argument("--version", required=True)
    parser.add_argument("--revision", required=True)
    parser.add_argument("--binary", required=True, type=Path)
    parser.add_argument(
        "--wheels",
        required=True,
        type=Path,
        help="directory containing this target's wheels",
    )
    parser.add_argument("--archive", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument(
        "--summary",
        type=Path,
        help="append Markdown to this file (e.g. GITHUB_STEP_SUMMARY)",
    )
    args = parser.parse_args()
    try:
        wheels = sorted(args.wheels.glob("*.whl"))
        if not wheels:
            raise ValueError("no wheels found")
        report = {
            "schema_version": 1,
            "target": args.target,
            "version": args.version,
            "revision": args.revision,
            "artifacts": [measure("binary", args.binary)]
            + [measure("wheel", path) for path in wheels]
            + [measure("archive", args.archive)],
        }
        args.output.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
        summary = markdown(report)
        if args.summary:
            with args.summary.open("a", encoding="utf-8") as stream:
                stream.write(summary)
        print(summary, end="")
    except (OSError, ValueError, tarfile.TarError, zipfile.BadZipFile) as error:
        parser.exit(1, f"artifact sizes: {error}\n")


if __name__ == "__main__":
    main()
