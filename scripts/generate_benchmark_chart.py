#!/usr/bin/env python3
"""
Generate benchmark comparison chart from hyperfine results.

Creates a transparent SVG chart that works in both light and dark modes,
following ruff's minimalistic design principles.
"""

import json
import sys
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
    linters = [r["command"] for r in results]
    times = [r["mean"] * 1000 for r in results]  # Convert to milliseconds

    # Sort by time (fastest first)
    sorted_data = sorted(zip(linters, times), key=lambda x: x[1])
    linters, times = zip(*sorted_data)

    # Create figure - transparent background with compact height
    fig, ax = plt.subplots(figsize=(10, 2.5))
    fig.patch.set_alpha(0.0)  # Transparent figure background
    ax.patch.set_alpha(0.0)  # Transparent axes background

    # Color scheme: vibrant green for rumdl (winner), light gray for others
    colors = []
    for linter in linters:
        if linter == "rumdl":
            colors.append("#10b981")  # Vibrant emerald green for rumdl
        else:
            colors.append("#e5e7eb")  # Very light gray for others

    # Create horizontal bars
    y_pos = range(len(linters))
    bars = ax.barh(y_pos, times, color=colors, height=0.6, edgecolor="none")

    # Set y-axis labels - rumdl stands out with darker text and bold
    ax.set_yticks(y_pos)
    ax.set_yticklabels(linters, fontsize=11)

    # Make rumdl label stand out: bold, larger, and darker
    for i, (tick, linter) in enumerate(zip(ax.get_yticklabels(), linters)):
        if linter == "rumdl":
            tick.set_fontweight("bold")
            tick.set_fontsize(12)
            tick.set_color("#10b981")  # Match the bar color
        else:
            tick.set_color("#9ca3af")  # Medium gray for others

    # Add value labels outside the bars - medium gray for visibility in both modes
    for i, (bar, time) in enumerate(zip(bars, times)):
        width = bar.get_width()
        if time < 1000:
            label = f"{time:.0f}ms"
        else:
            label = f"{time/1000:.1f}s"
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

    # Add subtle gridlines - works in both light and dark modes
    ax.grid(
        axis="x", alpha=0.2, linestyle="-", linewidth=0.5, color="#888888", zorder=0
    )
    ax.set_axisbelow(True)

    # Remove spines
    for spine in ax.spines.values():
        spine.set_visible(False)

    # X-axis: remove label (values already shown on bars), keep ticks subtle
    ax.set_xlabel("")
    ax.tick_params(axis="x", labelsize=9, colors="#666666")

    # No title - let the data speak
    ax.set_title("")

    # Tight layout
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

    # Also save intermediate version to benchmark/results/ for reference
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


def main():
    """Main chart generation workflow."""
    # Ensure we're in the project root
    project_root = Path(__file__).parent.parent
    import os

    os.chdir(project_root)

    print("📊 Generating benchmark comparison chart")
    print("=" * 50)

    generate_chart()

    print("\n" + "=" * 50)
    print("✅ Chart generation complete!")
    print("\nThe chart is ready for use in README.md")


if __name__ == "__main__":
    main()
