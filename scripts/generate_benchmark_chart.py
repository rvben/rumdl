#!/usr/bin/env python3
"""
Generate benchmark comparison chart from hyperfine results.

Creates a transparent SVG chart that works in both light and dark modes,
following ruff's minimalistic design principles.
"""

import json
import os
import re
import shutil
import stat
import sys
import tempfile
import textwrap
from datetime import datetime
from pathlib import Path


def generate_chart():
    """Generate transparent SVG chart from benchmark results."""
    # Read results
    result_file = Path("benchmark/results/cold_start.json")
    if not result_file.exists():
        print(f"❌ Benchmark results not found: {result_file}")
        print("   Run scripts/benchmark_cold_start.py first")
        sys.exit(1)

    with open(result_file) as f:
        data = json.load(f)

    # Import matplotlib here to provide better error message
    try:
        import matplotlib.pyplot as plt
    except ImportError:
        print("❌ matplotlib not found")
        print("   This script uses uv to automatically install matplotlib")
        sys.exit(1)

    # Extract data
    results = data["results"]
    tools = [r["command"] for r in results]
    times = [r["mean"] * 1000 for r in results]  # Convert to milliseconds

    # Sort by time (fastest first)
    sorted_data = sorted(zip(tools, times), key=lambda x: x[1])
    tools, times = zip(*sorted_data)

    # Dynamic figure height: 0.5 inches per bar, minimum 2.5
    n_bars = len(tools)
    fig_height = max(2.5, 0.5 * n_bars + 0.5)

    # Create figure - transparent background
    fig, ax = plt.subplots(figsize=(10, fig_height))
    fig.patch.set_alpha(0.0)
    ax.patch.set_alpha(0.0)

    # Use the product accent to identify rumdl without implying that the
    # highlighted row is the fastest result in the complete benchmark.
    colors = []
    for tool in tools:
        if tool == "rumdl":
            colors.append("#F04B50")
        else:
            colors.append("#e5e7eb")  # Very light gray

    # Create horizontal bars
    y_pos = range(len(tools))
    bars = ax.barh(y_pos, times, color=colors, height=0.6, edgecolor="none")

    # Set y-axis labels
    ax.set_yticks(y_pos)
    ax.set_yticklabels(tools, fontsize=11)

    # Make rumdl label stand out
    for tick, tool in zip(ax.get_yticklabels(), tools):
        if tool == "rumdl":
            tick.set_fontweight("bold")
            tick.set_fontsize(12)
            tick.set_color("#F04B50")
        else:
            tick.set_color("#9ca3af")

    # Add value labels outside the bars
    for bar, time in zip(bars, times):
        width = bar.get_width()
        if time < 1000:
            label = f"{time:.0f}ms"
        else:
            label = f"{time / 1000:.1f}s"
        ax.text(
            width + (max(times) * 0.01),
            bar.get_y() + bar.get_height() / 2,
            label,
            ha="left",
            va="center",
            fontsize=10,
            color="#666666",
            fontweight="500",
        )

    # Subtle gridlines
    ax.grid(
        axis="x", alpha=0.2, linestyle="-", linewidth=0.5, color="#888888", zorder=0
    )
    ax.set_axisbelow(True)

    # Remove spines
    for spine in ax.spines.values():
        spine.set_visible(False)

    # X-axis: keep ticks subtle, no label (values on bars)
    ax.set_xlabel("")
    ax.tick_params(axis="x", labelsize=9, colors="#666666")

    # No title
    ax.set_title("")

    plt.tight_layout()

    # Save as SVG to assets/
    output_path = Path("assets/benchmark.svg")
    plt.savefig(
        output_path,
        bbox_inches="tight",
        facecolor="none",
        transparent=True,
        pad_inches=0.2,
        format="svg",
    )
    print(f"✅ Chart saved to {output_path}")

    # Zensical only publishes files below docs/, while the repository README
    # reads the root asset directly. Keep both production locations identical.
    public_output_path = Path("docs/assets/benchmark.svg")
    public_output_path.parent.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(output_path, public_output_path)
    print(f"✅ Documentation chart saved to {public_output_path}")

    # Also save to benchmark/results/ for reference
    intermediate_path = Path("benchmark/results/cold_start_comparison.svg")
    plt.savefig(
        intermediate_path,
        bbox_inches="tight",
        facecolor="none",
        transparent=True,
        pad_inches=0.2,
        format="svg",
    )
    print(f"✅ Intermediate chart saved to {intermediate_path}")


CATEGORIES = {
    "rumdl": "Lint",
    "markdownlint-cli": "Lint",
    "markdownlint-cli2": "Lint",
    "remark-lint": "Lint",
    "pymarkdown": "Lint",
    "mado": "Lint",
    "mdformat": "Format",
    "Prettier": "Format",
}
REQUIRED_COMPARISONS = {"rumdl", "markdownlint-cli", "markdownlint-cli2"}
REQUIRED_DOC_BLOCKS = {
    "benchmarks.md": (
        "BENCHMARK_SUMMARY",
        "BENCHMARK_TABLE",
        "BENCHMARK_METADATA_LIMITATION",
    ),
    "index.md": ("BENCHMARK_HOMEPAGE_INTRO", "BENCHMARK_HOMEPAGE_TABLE"),
    "comparison.md": (
        "BENCHMARK_COMPARISON_INTRO",
        "BENCHMARK_COMPARISON_TABLE",
    ),
}


def format_duration(mean_s):
    """Format a hyperfine mean consistently across every published surface."""
    return f"{mean_s * 1000:.0f} ms" if mean_s < 1 else f"{mean_s:.1f} s"


def result_lookup(data):
    """Validate benchmark data and return results by public tool name."""
    results = data.get("results")
    if not isinstance(results, list):
        raise TypeError("benchmark results are missing or invalid")
    lookup = {
        result["command"]: result
        for result in results
        if isinstance(result, dict) and "command" in result and "mean" in result
    }
    missing = sorted(REQUIRED_COMPARISONS - lookup.keys())
    if missing:
        raise RuntimeError(f"benchmark is missing required tools: {', '.join(missing)}")
    if lookup["rumdl"]["mean"] <= 0:
        raise RuntimeError("rumdl benchmark mean must be positive")
    return lookup


def generated_block(name, body):
    return f"<!-- {name}_START -->\n\n{body}\n\n<!-- {name}_END -->\n\n"


def replace_generated_block(content, name, body, path):
    pattern = re.compile(
        rf"<!-- {re.escape(name)}_START -->\n.*?<!-- {re.escape(name)}_END -->\n*",
        re.DOTALL,
    )
    content, count = pattern.subn(generated_block(name, body), content)
    if count != 1:
        raise RuntimeError(f"{path}: expected exactly one {name} generated block")
    return content


def result_rows(data):
    lookup = result_lookup(data)
    rumdl_mean = lookup["rumdl"]["mean"]
    rows = []
    for result in sorted(lookup.values(), key=lambda item: item["mean"]):
        name = result["command"]
        mean_s = result["mean"]
        ratio = mean_s / rumdl_mean
        ratio_text = f"{ratio:.2f}x" if ratio < 0.1 else f"{ratio:.1f}x"
        rows.append(
            (name, CATEGORIES.get(name, "Lint"), format_duration(mean_s), ratio_text)
        )
    return lookup, rows


def validate_benchmark_docs(results_file, docs_root=Path("docs")):
    """Validate all inputs and generated markers before writing any output."""
    with open(results_file) as file:
        data = json.load(file)
    result_rows(data)

    sources = {}
    for name, markers in REQUIRED_DOC_BLOCKS.items():
        path = docs_root / name
        if not path.is_file():
            raise RuntimeError(f"{path}: required benchmark publication is missing")
        content = path.read_text()
        sources[name] = content
        for marker in markers:
            start = content.count(f"<!-- {marker}_START -->")
            end = content.count(f"<!-- {marker}_END -->")
            if start != 1 or end != 1:
                raise RuntimeError(
                    f"{path}: expected exactly one {marker} generated block"
                )

    benchmark_path = docs_root / "benchmarks.md"
    if (
        len(re.findall(r"Last benchmark run: \w+ \d{4}\.", sources["benchmarks.md"]))
        != 1
    ):
        raise RuntimeError(f"{benchmark_path}: benchmark run date marker is missing")
    return data, sources


def write_documents_atomically(updates):
    """Publish a set of text files with rollback if any replacement fails."""
    originals = {path: path.read_text() for path in updates}
    original_modes = {path: stat.S_IMODE(path.stat().st_mode) for path in updates}
    temporary_paths = {}
    replaced = []
    try:
        for path, content in updates.items():
            with tempfile.NamedTemporaryFile(
                mode="w",
                encoding="utf-8",
                dir=path.parent,
                prefix=f".{path.name}.",
                delete=False,
            ) as temporary:
                temporary.write(content)
                temporary.flush()
                os.fsync(temporary.fileno())
                temporary_paths[path] = Path(temporary.name)
            os.chmod(temporary_paths[path], original_modes[path])

        for path, temporary in temporary_paths.items():
            os.replace(temporary, path)
            replaced.append(path)
    except Exception:
        rollback_failures = []
        for path in replaced:
            try:
                path.write_text(originals[path])
                os.chmod(path, original_modes[path])
            except OSError as error:
                rollback_failures.append(f"{path}: {error}")
        if rollback_failures:
            raise RuntimeError(
                "benchmark publication failed and rollback was incomplete: "
                + "; ".join(rollback_failures)
            )
        raise
    finally:
        for temporary in temporary_paths.values():
            try:
                temporary.unlink(missing_ok=True)
            except OSError:
                pass


def update_benchmark_docs(results_file, docs_root=Path("docs")):
    """Update every published benchmark value from one hyperfine result file."""
    data, sources = validate_benchmark_docs(results_file, docs_root)
    lookup, rows = result_rows(data)
    rumdl_mean = lookup["rumdl"]["mean"]
    cli2_mean = lookup["markdownlint-cli2"]["mean"]
    cli_mean = lookup["markdownlint-cli"]["mean"]
    ratios = sorted((cli2_mean / rumdl_mean, cli_mean / rumdl_mean))
    file_count = (
        data.get("rumdl_benchmark", {}).get("target", {}).get("markdown_files") or 478
    )

    recorded_at = data.get("rumdl_benchmark", {}).get("recorded_at")
    if recorded_at:
        date_str = datetime.fromisoformat(recorded_at).strftime("%B %Y")
    else:
        date_match = re.search(
            r"Last benchmark run: (\w+ \d{4})\.", sources["benchmarks.md"]
        )
        if not date_match:
            raise RuntimeError(
                f"{docs_root / 'benchmarks.md'}: benchmark run date marker is missing"
            )
        date_str = date_match.group(1)

    if ratios[0] > 1:
        comparison = (
            f"rumdl was about {ratios[0]:.1f}–{ratios[1]:.1f} times faster than "
            "the tested markdownlint CLIs in this workload"
        )
    else:
        comparison = "the three tools had different measured times in this workload"
    summary = (
        f"In the {date_str} Rust Book cold-start benchmark, rumdl checked {file_count} "
        f"Markdown files in {format_duration(rumdl_mean)}. The same run measured "
        f"markdownlint-cli2 at {format_duration(cli2_mean)} and markdownlint-cli at "
        f"{format_duration(cli_mean)}. These results support a specific claim: "
        f"{comparison}. They do not establish that rumdl is faster than every "
        "Markdown tool."
    )

    benchmark_path = docs_root / "benchmarks.md"
    content = sources["benchmarks.md"]
    content = replace_generated_block(
        content, "BENCHMARK_SUMMARY", textwrap.fill(summary, width=88), benchmark_path
    )
    canonical_header = (
        "| Tool                  | Type   | Mean   | Relative to rumdl |\n"
        "| --------------------- | ------ | ------ | ----------------- |"
    )
    canonical_rows = [
        f"| {f'**{name}**':<21} | {category:<6} | {mean:<6} | {ratio:<17} |"
        for name, category, mean, ratio in rows
    ]
    content = replace_generated_block(
        content,
        "BENCHMARK_TABLE",
        '<p class="rm-table-hint" aria-hidden="true">'
        "Swipe horizontally to compare all columns.</p>\n"
        '<div class="rm-table-scroll" role="region" '
        'aria-label="Markdown tool benchmark results" tabindex="0" markdown>\n\n'
        + canonical_header
        + "\n"
        + "\n".join(canonical_rows)
        + "\n\n</div>",
        benchmark_path,
    )

    if recorded_at:
        content, count = re.subn(
            r"Last benchmark run: \w+ \d{4}\.",
            f"Last benchmark run: {date_str}.",
            content,
        )
        if count != 1:
            raise RuntimeError(
                f"{benchmark_path}: benchmark run date marker is missing"
            )

    metadata = data.get("rumdl_benchmark", {})
    target = metadata.get("target", {})
    has_reproduction_metadata = bool(
        metadata.get("recorded_at")
        and target.get("git_revision")
        and metadata.get("tools")
    )
    if has_reproduction_metadata:
        limitation = (
            "- This published result records the target revision, tool versions, and "
            "non-personal environment metadata needed to reproduce the tested setup."
        )
    else:
        limitation = (
            "- This published result predates benchmark metadata capture, so its exact "
            "tool versions and target revision were not retained. Treat it as directional "
            "until the next metadata-bearing run."
        )
    content = replace_generated_block(
        content,
        "BENCHMARK_METADATA_LIMITATION",
        textwrap.fill(limitation, width=88, subsequent_indent="  "),
        benchmark_path,
    )

    homepage_path = docs_root / "index.md"
    homepage = sources["index.md"]
    homepage = replace_generated_block(
        homepage,
        "BENCHMARK_HOMEPAGE_INTRO",
        (
            f"<p>The published {date_str} snapshot measures the Rust Book repository "
            "with application caches disabled.</p>"
        ),
        homepage_path,
    )
    homepage_rows = []
    for name in ("rumdl", "markdownlint-cli2", "markdownlint-cli"):
        result = lookup[name]
        ratio = result["mean"] / rumdl_mean
        row_class = ' class="rm-benchmark__featured"' if name == "rumdl" else ""
        homepage_rows.append(
            f'<tr{row_class}><th scope="row">{name}</th>'
            f"<td>{format_duration(result['mean'])}</td><td>{ratio:.1f}×</td></tr>"
        )
    homepage_table = (
        '<p class="rm-table-hint" aria-hidden="true">'
        "Swipe horizontally to compare all columns.</p>\n"
        '<div class="rm-table-scroll" role="region" '
        'aria-label="Markdownlint CLI benchmark comparison" tabindex="0">\n'
        '<table>\n<thead><tr><th scope="col">Linter</th><th scope="col">Mean time</th>'
        '<th scope="col">Relative to rumdl</th></tr></thead>\n<tbody>\n'
        + "\n".join(homepage_rows)
        + "\n</tbody>\n</table>\n</div>"
    )
    homepage = replace_generated_block(
        homepage, "BENCHMARK_HOMEPAGE_TABLE", homepage_table, homepage_path
    )

    comparison_path = docs_root / "comparison.md"
    comparison_content = sources["comparison.md"]
    comparison_content = replace_generated_block(
        comparison_content,
        "BENCHMARK_COMPARISON_INTRO",
        (
            f"The published {date_str} cold-start snapshot checks the Rust Book "
            "repository with application caches disabled. It measures full command "
            "latency, including runtime and launcher overhead."
        ),
        comparison_path,
    )
    comparison_header = (
        "| Tool                  | Type   | Mean   | vs rumdl |\n"
        "| --------------------- | ------ | ------ | -------- |"
    )
    comparison_rows = [
        f"| {f'**{name}**':<21} | {category:<6} | {mean:<6} | {ratio:<8} |"
        for name, category, mean, ratio in rows
    ]
    comparison_content = replace_generated_block(
        comparison_content,
        "BENCHMARK_COMPARISON_TABLE",
        '<p class="rm-table-hint" aria-hidden="true">'
        "Swipe horizontally to compare all columns.</p>\n"
        '<div class="rm-table-scroll" role="region" '
        'aria-label="Markdown tool benchmark comparison" tabindex="0" markdown>\n\n'
        + comparison_header
        + "\n"
        + "\n".join(comparison_rows)
        + "\n\n</div>",
        comparison_path,
    )
    write_documents_atomically(
        {
            benchmark_path: content,
            homepage_path: homepage,
            comparison_path: comparison_content,
        }
    )
    print("✅ Updated benchmark values across canonical and summary pages")


def main():
    """Main chart generation workflow."""
    project_root = Path(__file__).parent.parent

    os.chdir(project_root)

    print("📊 Generating benchmark comparison chart")
    print("=" * 50)

    results_path = Path("benchmark/results/cold_start.json")
    validate_benchmark_docs(results_path)
    generate_chart()
    update_benchmark_docs(results_path)

    print("\n" + "=" * 50)
    print("✅ Chart generation complete!")
    print("\nThe chart and docs/benchmarks.md are up to date")


if __name__ == "__main__":
    main()
