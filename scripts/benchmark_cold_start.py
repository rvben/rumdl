#!/usr/bin/env python3
"""
Cold start benchmark comparison for markdown linters.

Runs hyperfine to measure true cold start performance (no internal caching,
but warm OS disk cache after warmup runs).
"""

import argparse
import subprocess
import sys
from pathlib import Path


def check_hyperfine():
    """Check if hyperfine is installed."""
    try:
        subprocess.run(["hyperfine", "--version"], capture_output=True, check=True)
        return True
    except (subprocess.CalledProcessError, FileNotFoundError):
        print("❌ hyperfine not found. Install it with: brew install hyperfine")
        return False


def check_linters():
    """Check which linters are available."""
    linters = {
        "rumdl": "./target/release/rumdl check --no-cache",
        "markdownlint-cli2": 'markdownlint-cli2 "**/*.md"',
        "mdl": "mdl",
    }

    available = {}

    # Check rumdl binary
    rumdl_binary = "./target/release/rumdl"
    if Path(rumdl_binary).exists():
        available["rumdl"] = linters["rumdl"]
        print(f"✅ Found rumdl (release binary)")
    else:
        print("⚠️  rumdl binary not found. Run: cargo build --release")

    # Check other linters
    for name, cmd in linters.items():
        if name == "rumdl":
            continue
        try:
            # Extract just the command name (before any args like "**/*.md")
            cmd_parts = cmd.split()
            # Just check if command exists (don't require success exit code)
            subprocess.run([cmd_parts[0], "--help"], capture_output=True)
            available[name] = cmd
            print(f"✅ Found {name}")
        except FileNotFoundError:
            print(f"⚠️  {name} not found")

    return available


def run_benchmark(linters, target_repo, warmup=2, min_runs=3):
    """Run hyperfine benchmark and save results to JSON."""
    print(f"\n🔥 Running cold start benchmarks on {target_repo}...")
    print("This will take a few minutes for accurate measurements.\n")

    # Build hyperfine command
    commands = []
    names = []

    for name, cmd in linters.items():
        # markdownlint-cli2 needs to be run from within the target directory
        if name == "markdownlint-cli2":
            full_cmd = f"cd {target_repo} && {cmd}"
        else:
            full_cmd = f"{cmd} {target_repo}"
        commands.extend(["--command-name", name, full_cmd])
        names.append(name)

    # Run hyperfine with cache clearing
    # Note: sync flushes file system buffers between runs. OS disk cache remains warm
    # after warmup runs, which represents realistic "cold start" (no application cache,
    # but OS has files cached). rumdl uses --no-cache to disable internal caching.
    hyperfine_cmd = [
        "hyperfine",
        "--warmup",
        str(warmup),
        "--min-runs",
        str(min_runs),
        "--prepare",
        "sync",  # Flush file system buffers between runs
        "--ignore-failure",  # Ignore non-zero exit codes (linters finding issues)
        "--export-json",
        "benchmark/results/cold_start.json",
        "--style",
        "full",
        *commands,
    ]

    try:
        subprocess.run(hyperfine_cmd, check=True)
        print("\n✅ Benchmark complete!")
        print(f"   Results saved to: benchmark/results/cold_start.json")
        return True
    except subprocess.CalledProcessError as e:
        print(f"\n❌ Benchmark failed: {e}")
        return False


def main():
    """Main benchmark workflow."""
    parser = argparse.ArgumentParser(
        description="Run cold start benchmarks for markdown linters"
    )
    parser.add_argument(
        "--target",
        default="../rust-book",
        help="Target repository to benchmark (default: ../rust-book)",
    )
    parser.add_argument(
        "--warmup", type=int, default=2, help="Number of warmup runs (default: 2)"
    )
    parser.add_argument(
        "--min-runs",
        type=int,
        default=3,
        help="Minimum number of benchmark runs (default: 3)",
    )
    args = parser.parse_args()

    # Ensure we're in the project root
    project_root = Path(__file__).parent.parent
    import os

    os.chdir(project_root)

    print("🚀 Markdown Linter Cold Start Benchmark")
    print("=" * 50)

    # Check prerequisites
    if not check_hyperfine():
        sys.exit(1)

    # Check available linters
    linters = check_linters()
    if not linters:
        print("\n❌ No linters found to benchmark")
        sys.exit(1)

    # Validate target repository
    target_repo = Path(args.target)
    if not target_repo.exists():
        print(f"\n❌ Target repository not found: {target_repo}")
        print(f"   Please ensure the repository exists at {target_repo.absolute()}")
        sys.exit(1)

    # Create results directory
    Path("benchmark/results").mkdir(parents=True, exist_ok=True)

    # Run benchmark
    if not run_benchmark(linters, str(target_repo), args.warmup, args.min_runs):
        sys.exit(1)

    print("\n" + "=" * 50)
    print("✅ Benchmark complete!")
    print("\nNext step: Run scripts/generate_benchmark_chart.py to create the chart")


if __name__ == "__main__":
    main()
