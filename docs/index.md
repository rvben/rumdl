---
icon: lucide/file-check
---

# rumdl

## A high-performance Markdown linter and formatter, written in Rust

<div class="grid cards" markdown>

-   :zap:{ .lg .middle } **Built for speed**

    ---

    Written in Rust for blazing fast performance. Significantly faster than alternatives.

    [:octicons-arrow-right-24: Benchmarks](#performance)

-   :mag:{ .lg .middle } **67 lint rules**

    ---

    Comprehensive coverage of common Markdown issues with detailed error messages.

    [:octicons-arrow-right-24: View rules](rules/index.md)

-   :wrench:{ .lg .middle } **Auto-formatting**

    ---

    Automatic fixes for most issues with `rumdl fmt` or `rumdl check --fix`.

    [:octicons-arrow-right-24: Quick start](getting-started/quickstart.md)

-   :package:{ .lg .middle } **Zero dependencies**

    ---

    Single binary with no runtime requirements. Install via Cargo, pip, Homebrew, or download.

    [:octicons-arrow-right-24: Installation](getting-started/installation.md)

</div>

## Quick Start

```bash
# Install using Cargo
cargo install rumdl

# Or using pip
pip install rumdl

# Or using Homebrew
brew install rvben/tap/rumdl

# Lint Markdown files
rumdl check .

# Auto-fix issues
rumdl fmt .
```

## Performance

rumdl is designed for speed. Benchmarked on the [Rust Book](https://github.com/rust-lang/book) repository (478 markdown files):

| Linter | Cold Start | Warm Cache |
|--------|------------|------------|
| **rumdl** | **0.15s** | **0.02s** |
| markdownlint-cli2 | 2.8s | 2.8s |
| markdownlint-cli | 5.2s | 5.2s |

With intelligent caching, subsequent runs are even faster - rumdl only re-lints files that have changed.

## Features

- :zap: **Built for speed** with Rust - significantly faster than alternatives
- :mag: **67 lint rules** covering common Markdown issues
- :wrench: **Automatic formatting** with `--fix` for files and stdin/stdout
- :package: **Zero dependencies** - single binary with no runtime requirements
- :gear: **Highly configurable** with TOML-based config files
- :dart: **Multiple Markdown flavors** - GFM, MkDocs, MDX, Quarto support
- :globe_with_meridians: **Multiple installation options** - Rust, Python, standalone binaries
- :snake: **Installable via pip** for Python users
- :straight_ruler: **Modern CLI** with detailed error reporting
- :arrows_counterclockwise: **CI/CD friendly** with non-zero exit code on errors

## Next Steps

<div class="grid cards" markdown>

-   [:octicons-download-24: **Installation**](getting-started/installation.md)

    Install rumdl via Cargo, pip, Homebrew, or download a binary.

-   [:octicons-play-24: **Quick Start**](getting-started/quickstart.md)

    Get up and running with rumdl in minutes.

-   [:octicons-book-24: **Rules Reference**](rules/index.md)

    Explore all 67 linting rules with examples.

-   [:octicons-gear-24: **Configuration**](configuration/global-settings.md)

    Customize rumdl for your project.

</div>
