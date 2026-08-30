#!/usr/bin/env python3
"""Verify benchmark publication values cannot drift from benchmark results."""

from __future__ import annotations

import json
import os
import shutil
import stat
import tempfile
from pathlib import Path
from unittest.mock import patch

from generate_benchmark_chart import update_benchmark_docs


def expect_runtime_error(action, message: str) -> None:
    try:
        action()
    except RuntimeError:
        return
    raise AssertionError(message)


def main() -> int:
    project = Path(__file__).resolve().parent.parent
    source_results = {
        "results": [
            {"command": "rumdl", "mean": 0.2168176648},
            {"command": "markdownlint-cli", "mean": 2.7151225715},
            {"command": "markdownlint-cli2", "mean": 2.2066130442},
            {"command": "remark-lint", "mean": 0.6710917107},
            {"command": "pymarkdown", "mean": 0.2398861368},
            {"command": "mado", "mean": 0.0765390177},
            {"command": "mdformat", "mean": 4.0133914283},
            {"command": "Prettier", "mean": 4.8289023321},
        ]
    }

    with tempfile.TemporaryDirectory(prefix="rumdl-benchmark-docs-") as directory:
        root = Path(directory)
        docs = root / "docs"
        docs.mkdir()
        for name in (
            "benchmarks.md",
            "index.md",
            "comparison.md",
            "markdownlint-comparison.md",
        ):
            shutil.copyfile(project / "docs" / name, docs / name)

        synthetic = json.loads(json.dumps(source_results))
        means = {
            "rumdl": 0.1,
            "markdownlint-cli2": 1.5,
            "markdownlint-cli": 2.0,
        }
        for result in synthetic["results"]:
            if result["command"] in means:
                result["mean"] = means[result["command"]]
        synthetic["rumdl_benchmark"] = {
            "recorded_at": "2026-08-29T12:00:00+00:00",
            "target": {"markdown_files": 500, "git_revision": "abc123"},
            "environment": {"system": "TestOS", "release": "1", "machine": "test"},
            "tools": {
                result["command"]: {"version": "test", "command": "test"}
                for result in synthetic["results"]
            },
        }
        results_path = root / "results.json"
        results_path.write_text(json.dumps(synthetic), encoding="utf-8")

        update_benchmark_docs(results_path, docs)
        benchmark = (docs / "benchmarks.md").read_text(encoding="utf-8")
        homepage = (docs / "index.md").read_text(encoding="utf-8")
        comparison = (docs / "comparison.md").read_text(encoding="utf-8")
        markdownlint = (docs / "markdownlint-comparison.md").read_text(encoding="utf-8")
        benchmark_text = " ".join(benchmark.split())
        assert "500 Markdown files in 100 ms" in benchmark_text
        assert "15.0–20.0 times faster" in benchmark_text
        assert "Last benchmark run: August 2026." in benchmark
        assert "predates benchmark metadata capture" not in benchmark
        assert "records the target revision" in benchmark
        assert "August 2026 snapshot" in homepage
        assert "August 2026 cold-start snapshot" in comparison
        assert 'class="rm-benchmark__featured"' in homepage
        published = benchmark + homepage + comparison + markdownlint
        assert "Swipe horizontally to compare all columns." in published
        assert "100 ms" in homepage and "15.0×" in homepage and "20.0×" in homepage
        assert (
            "100 ms" in comparison and "15.0x" in comparison and "20.0x" in comparison
        )
        for stale in (
            "February 2026",
            "478",
            "217 ms",
            "10.2x",
            "12.5x",
            "reproducible Rust Book",
        ):
            assert stale not in published, f"stale benchmark value survived: {stale}"

        incomplete = json.loads(json.dumps(synthetic))
        incomplete["results"] = [
            result
            for result in incomplete["results"]
            if result["command"] != "markdownlint-cli2"
        ]
        incomplete_path = root / "incomplete.json"
        incomplete_path.write_text(json.dumps(incomplete), encoding="utf-8")
        expect_runtime_error(
            lambda: update_benchmark_docs(incomplete_path, docs),
            "missing required tools must fail",
        )

        broken_docs = root / "broken-docs"
        shutil.copytree(docs, broken_docs)
        broken = (broken_docs / "benchmarks.md").read_text(encoding="utf-8")
        (broken_docs / "benchmarks.md").write_text(
            broken.replace("<!-- BENCHMARK_SUMMARY_START -->", ""),
            encoding="utf-8",
        )
        expect_runtime_error(
            lambda: update_benchmark_docs(results_path, broken_docs),
            "missing generated markers must fail",
        )

        atomic_docs = root / "atomic-docs"
        atomic_docs.mkdir()
        originals = {}
        for name in ("benchmarks.md", "index.md", "comparison.md"):
            source = project / "docs" / name
            destination = atomic_docs / name
            shutil.copyfile(source, destination)
            originals[destination] = (
                destination.read_text(encoding="utf-8"),
                stat.S_IMODE(destination.stat().st_mode),
            )

        real_replace = os.replace
        replace_count = 0

        def fail_second_replace(source, destination):
            nonlocal replace_count
            replace_count += 1
            if replace_count == 2:
                raise OSError("simulated second document replacement failure")
            return real_replace(source, destination)

        try:
            with patch(
                "generate_benchmark_chart.os.replace",
                side_effect=fail_second_replace,
            ):
                update_benchmark_docs(results_path, atomic_docs)
        except OSError:
            pass
        else:
            raise AssertionError("a failed document replacement must propagate")
        for path, (content, mode) in originals.items():
            assert path.read_text(encoding="utf-8") == content
            assert stat.S_IMODE(path.stat().st_mode) == mode

    print("benchmark documentation test passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
