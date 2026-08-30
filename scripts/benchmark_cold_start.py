#!/usr/bin/env python3
"""
Cold start benchmark comparison for markdown linters and formatters.

Runs hyperfine to measure true cold start performance (no internal caching,
but warm OS disk cache after warmup runs).

All tools run via npx/uvx or pre-built binaries — no global installs required.
"""

import argparse
import json
import os
import platform
import subprocess
import sys
import urllib.request
import tarfile
import zipfile
from datetime import datetime, timezone
from pathlib import Path

# Mapping of tool name -> (command template, category)
# {target} is replaced with the shell-quoted target directory path
TOOLS = {
    "rumdl": {
        "cmd": "./target/release/rumdl check --no-cache '{target}'",
        "category": "lint",
        "check": lambda: Path("./target/release/rumdl").exists(),
        "check_msg": "Run: cargo build --release",
    },
    "markdownlint-cli": {
        "cmd": "npx markdownlint-cli '{target}'",
        "category": "lint",
        "check": lambda: _has_npx(),
        "check_msg": "npx required (install Node.js)",
    },
    "markdownlint-cli2": {
        # markdownlint-cli2 must be run from within the target directory
        "cmd": "cd '{target}' && npx markdownlint-cli2 '**/*.md'",
        "category": "lint",
        "check": lambda: _has_npx(),
        "check_msg": "npx required (install Node.js)",
    },
    "remark-lint": {
        "cmd": "npx remark --use remark-preset-lint-recommended --quiet '{target}'",
        "category": "lint",
        "check": lambda: _has_npx(),
        "check_msg": "npx required (install Node.js)",
    },
    "pymarkdown": {
        "cmd": "uvx pymarkdownlnt scan '{target}'",
        "category": "lint",
        "check": lambda: _has_uvx(),
        "check_msg": "uvx required (install uv)",
    },
    "mado": {
        "cmd": "'{mado_bin}' check '{target}'",
        "category": "lint",
        "check": lambda: _ensure_mado(),
        "check_msg": "Failed to download mado binary",
    },
    "mdformat": {
        "cmd": "uvx mdformat --check '{target}'",
        "category": "format",
        "check": lambda: _has_uvx(),
        "check_msg": "uvx required (install uv)",
    },
    "Prettier": {
        "cmd": "npx prettier --check '{target}/**/*.md'",
        "category": "format",
        "check": lambda: _has_npx(),
        "check_msg": "npx required (install Node.js)",
    },
}

VERSION_COMMANDS = {
    "rumdl": ["./target/release/rumdl", "--version"],
    "markdownlint-cli": ["npx", "markdownlint-cli", "--version"],
    "markdownlint-cli2": ["npx", "markdownlint-cli2", "--version"],
    "remark-lint": ["npx", "remark", "--version"],
    "pymarkdown": ["uvx", "pymarkdownlnt", "version"],
    "mado": ["benchmark/.tools/mado", "--version"],
    "mdformat": ["uvx", "mdformat", "--version"],
    "Prettier": ["npx", "prettier", "--version"],
}

MADO_VERSION = "v0.3.0"
MADO_TOOLS_DIR = Path("benchmark/.tools")


def _capture(command, cwd=None):
    """Return one line of command output without failing the benchmark."""
    try:
        result = subprocess.run(
            command,
            cwd=cwd,
            capture_output=True,
            check=True,
            text=True,
            timeout=30,
        )
    except (subprocess.CalledProcessError, FileNotFoundError, subprocess.TimeoutExpired):
        return "unknown"

    output = (result.stdout or result.stderr).strip().splitlines()
    return output[0] if output else "unknown"


def record_metadata(results_file, tools, target_path, markdown_files):
    """Attach reproducibility metadata without recording personal paths."""
    with open(results_file) as file:
        data = json.load(file)

    revision = _capture(["git", "rev-parse", "HEAD"], cwd=target_path)
    versions = {
        name: _capture(VERSION_COMMANDS[name])
        for name in tools
        if name in VERSION_COMMANDS
    }
    data["rumdl_benchmark"] = {
        "schema": 1,
        "recorded_at": datetime.now(timezone.utc).isoformat(),
        "target": {
            "markdown_files": markdown_files,
            "git_revision": revision,
        },
        "environment": {
            "system": platform.system(),
            "release": platform.release(),
            "machine": platform.machine(),
        },
        "tools": {
            name: {
                "version": versions.get(name, "unknown"),
                "command": tool["cmd"],
            }
            for name, tool in tools.items()
        },
    }

    with open(results_file, "w") as file:
        json.dump(data, file, indent=2)
        file.write("\n")


def _has_npx():
    try:
        subprocess.run(["npx", "--version"], capture_output=True, check=True)
        return True
    except (subprocess.CalledProcessError, FileNotFoundError):
        return False


def _has_uvx():
    try:
        subprocess.run(["uvx", "--version"], capture_output=True, check=True)
        return True
    except (subprocess.CalledProcessError, FileNotFoundError):
        return False


def _mado_bin_path():
    return MADO_TOOLS_DIR / "mado"


def _ensure_mado():
    """Download mado binary if not present. Returns True on success."""
    bin_path = _mado_bin_path()
    if bin_path.exists():
        return True

    system = platform.system()
    machine = platform.machine()

    if system == "Darwin":
        os_name = "macOS"
    elif system == "Linux":
        os_name = "Linux-gnu"
    else:
        os_name = "Windows-msvc"

    if machine in ("arm64", "aarch64"):
        arch = "arm64"
    else:
        arch = "x86_64"

    if system == "Windows":
        asset_name = f"mado-{os_name}-{arch}.zip"
    else:
        asset_name = f"mado-{os_name}-{arch}.tar.gz"

    url = f"https://github.com/akiomik/mado/releases/download/{MADO_VERSION}/{asset_name}"

    print(f"   Downloading mado {MADO_VERSION} from {url}...")
    MADO_TOOLS_DIR.mkdir(parents=True, exist_ok=True)

    try:
        archive_path = MADO_TOOLS_DIR / asset_name
        urllib.request.urlretrieve(url, archive_path)

        if asset_name.endswith(".tar.gz"):
            with tarfile.open(archive_path, "r:gz") as tar:
                # Extract the mado binary
                for member in tar.getmembers():
                    if member.name.endswith("/mado") or member.name == "mado":
                        member.name = "mado"
                        tar.extract(member, MADO_TOOLS_DIR)
                        break
        elif asset_name.endswith(".zip"):
            with zipfile.ZipFile(archive_path) as zf:
                for name in zf.namelist():
                    if name.endswith("/mado") or name.endswith("/mado.exe"):
                        data = zf.read(name)
                        with open(bin_path, "wb") as f:
                            f.write(data)
                        break

        archive_path.unlink()

        if bin_path.exists():
            bin_path.chmod(0o755)
            return True

        print(f"   ⚠️  Could not find mado binary in archive")
        return False

    except Exception as e:
        print(f"   ⚠️  Failed to download mado: {e}")
        return False


def check_hyperfine():
    """Check if hyperfine is installed."""
    try:
        subprocess.run(["hyperfine", "--version"], capture_output=True, check=True)
        return True
    except (subprocess.CalledProcessError, FileNotFoundError):
        print("❌ hyperfine not found. Install it with: brew install hyperfine")
        return False


def discover_tools(selected=None):
    """Discover which tools are available. Returns dict of name -> command."""
    available = {}

    tool_names = selected if selected else list(TOOLS.keys())

    for name in tool_names:
        if name not in TOOLS:
            print(f"⚠️  Unknown tool: {name}")
            continue

        tool = TOOLS[name]
        if tool["check"]():
            available[name] = tool
            print(f"✅ Found {name} ({tool['category']})")
        else:
            print(f"⚠️  {name} not available: {tool['check_msg']}")

    return available


def run_benchmark(tools, target_repo, warmup=2, min_runs=3):
    """Run hyperfine benchmark and save results to JSON."""
    print(f"\n🔥 Running cold start benchmarks on {target_repo}...")
    print(f"   Tools: {', '.join(tools.keys())}")
    print(f"   Warmup: {warmup}, Min runs: {min_runs}\n")

    # Resolve absolute path for target repo
    target_abs = str(Path(target_repo).resolve())

    # Build hyperfine command
    commands = []
    mado_bin = str(_mado_bin_path().resolve())

    for name, tool in tools.items():
        cmd = tool["cmd"]
        cmd = cmd.replace("{target}", target_abs)
        cmd = cmd.replace("{mado_bin}", mado_bin)
        commands.extend(["--command-name", name, cmd])

    # Run hyperfine
    # sync flushes file system buffers between runs. OS disk cache remains warm
    # after warmup runs — realistic "cold start" (no app cache, warm OS cache).
    hyperfine_cmd = [
        "hyperfine",
        "--warmup",
        str(warmup),
        "--min-runs",
        str(min_runs),
        "--prepare",
        "sync",
        "--ignore-failure",
        "--export-json",
        "benchmark/results/cold_start.json",
        "--style",
        "full",
        *commands,
    ]

    try:
        subprocess.run(hyperfine_cmd, check=True)
        print("\n✅ Benchmark complete!")
        print("   Results saved to: benchmark/results/cold_start.json")
        return True
    except subprocess.CalledProcessError as e:
        print(f"\n❌ Benchmark failed: {e}")
        return False


def main():
    """Main benchmark workflow."""
    parser = argparse.ArgumentParser(
        description="Run cold start benchmarks for markdown linters and formatters"
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
    parser.add_argument(
        "--tools",
        nargs="+",
        choices=list(TOOLS.keys()),
        default=None,
        help="Select specific tools to benchmark (default: all available)",
    )
    args = parser.parse_args()

    # Resolve target path before chdir (so relative paths work from user's CWD)
    target_path = Path(args.target).resolve()

    # Ensure we're in the project root
    project_root = Path(__file__).parent.parent
    os.chdir(project_root)

    print("🚀 Markdown Linter Cold Start Benchmark")
    print("=" * 50)

    # Check prerequisites
    if not check_hyperfine():
        sys.exit(1)

    # Discover available tools
    tools = discover_tools(args.tools)
    if not tools:
        print("\n❌ No tools found to benchmark")
        sys.exit(1)

    # Validate target repository
    if not target_path.exists():
        print(f"\n❌ Target repository not found: {target_path}")
        sys.exit(1)

    # Count markdown files
    md_count = sum(1 for _ in target_path.rglob("*.md"))
    print(f"\n📁 Target: {target_path} ({md_count} markdown files)")

    # Create results directory
    Path("benchmark/results").mkdir(parents=True, exist_ok=True)

    # Run benchmark
    if not run_benchmark(tools, str(target_path), args.warmup, args.min_runs):
        sys.exit(1)

    record_metadata(
        Path("benchmark/results/cold_start.json"),
        tools,
        target_path,
        md_count,
    )

    print("\n" + "=" * 50)
    print("✅ Benchmark complete!")
    print("\nNext step: Run scripts/generate_benchmark_chart.py to create the chart")


if __name__ == "__main__":
    main()
