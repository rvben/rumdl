#!/usr/bin/env python3
"""Exercise release size reporting against real ZIP and tar containers."""

import io
import json
import subprocess
import sys
import tarfile
import tempfile
import unittest
import zipfile
from pathlib import Path

from artifact_sizes import executable_size


class ArtifactSizesTest(unittest.TestCase):
    def test_report_tracks_package_and_executable_bytes_without_host_paths(self):
        for windows in (False, True):
            with (
                self.subTest(windows=windows),
                tempfile.TemporaryDirectory() as directory,
            ):
                root = Path(directory)
                name = "rumdl.exe" if windows else "rumdl"
                binary = root / name
                binary.write_bytes(b"rumdl executable" * 100)
                wheel = root / "rumdl-test.whl"
                with zipfile.ZipFile(
                    wheel, "w", compression=zipfile.ZIP_DEFLATED
                ) as package:
                    package.write(binary, f"rumdl.data/scripts/{name}")
                    package.writestr("rumdl/__init__.py", "# Python wrapper\n")
                archive = root / ("rumdl-test.zip" if windows else "rumdl-test.tar.gz")
                if windows:
                    with zipfile.ZipFile(
                        archive, "w", compression=zipfile.ZIP_DEFLATED
                    ) as package:
                        package.write(binary, name)
                else:
                    with tarfile.open(archive, "w:gz") as package:
                        package.add(binary, arcname=name)
                output = root / "sizes.json"
                summary = root / "summary.md"
                summary.write_text("Existing summary\n")
                subprocess.run(
                    [
                        sys.executable,
                        str(Path(__file__).with_name("artifact_sizes.py")),
                        "--target",
                        "test-target",
                        "--version",
                        "v1.2.3",
                        "--revision",
                        "abc123",
                        "--binary",
                        str(binary),
                        "--wheels",
                        str(root),
                        "--archive",
                        str(archive),
                        "--output",
                        str(output),
                        "--summary",
                        str(summary),
                    ],
                    check=True,
                    capture_output=True,
                    text=True,
                )
                report = json.loads(output.read_text())
                self.assertNotIn(directory, output.read_text())
                self.assertEqual(report["revision"], "abc123")
                for row, path in zip(report["artifacts"], (binary, wheel, archive)):
                    self.assertEqual(row["artifact_bytes"], path.stat().st_size)
                    self.assertEqual(row["executable_bytes"], binary.stat().st_size)
                self.assertTrue(summary.read_text().startswith("Existing summary\n"))

    def test_missing_or_ambiguous_executable_is_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for count in (0, 2):
                for suffix in (".whl", ".tar.gz"):
                    with self.subTest(count=count, suffix=suffix):
                        archive = root / f"package{suffix}"
                        if suffix == ".whl":
                            with zipfile.ZipFile(archive, "w") as package:
                                for index in range(count):
                                    package.writestr(f"{index}/rumdl", "binary")
                        else:
                            with tarfile.open(archive, "w:gz") as package:
                                for index in range(count):
                                    member = tarfile.TarInfo(f"{index}/rumdl")
                                    member.size = 6
                                    package.addfile(member, io.BytesIO(b"binary"))
                        with self.assertRaisesRegex(ValueError, f"found {count}"):
                            executable_size(archive)


if __name__ == "__main__":
    unittest.main()
