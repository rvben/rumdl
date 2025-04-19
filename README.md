# rumdl - A high-performance Markdown linter, written in Rust

<div align="center">

![rumdl Logo](https://raw.githubusercontent.com/rvben/rumdl/main/assets/logo.png)

[![Build Status](https://img.shields.io/github/actions/workflow/status/rvben/rumdl/build.yml?branch=main)](https://github.com/rvben/rumdl/actions)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![Crates.io](https://img.shields.io/crates/v/rumdl)](https://crates.io/crates/rumdl)
[![PyPI](https://img.shields.io/pypi/v/rumdl)](https://pypi.org/project/rumdl/)
[![GitHub release (latest by date)](https://img.shields.io/github/v/release/rvben/rumdl)](https://github.com/rvben/rumdl/releases/latest)
[![GitHub stars](https://img.shields.io/github/stars/rvben/rumdl)](https://github.com/rvben/rumdl/stargazers)

## A modern Markdown linter and formatter, built for speed with Rust

| [**Docs**](https://github.com/rvben/rumdl/blob/main/docs/RULES.md) | [**Rules**](https://github.com/rvben/rumdl/blob/main/docs/RULES.md) | [**Configuration**](#configuration) |

</div>

## Table of Contents

- [rumdl - A high-performance Markdown linter, written in Rust](#rumdl---a-high-performance-markdown-linter-written-in-rust)
  - [A modern Markdown linter and formatter, built for speed with Rust](#a-modern-markdown-linter-and-formatter-built-for-speed-with-rust)
  - [Table of Contents](#table-of-contents)
  - [Quick Start](#quick-start)
  - [Overview](#overview)
  - [Installation](#installation)
    - [Using Cargo (Rust)](#using-cargo-rust)
    - [Using pip (Python)](#using-pip-python)
    - [Download binary](#download-binary)
  - [Usage](#usage)
  - [Rules](#rules)
  - [Command-line Interface](#command-line-interface)
    - [Commands](#commands)
    - [Options (for `check`)](#options-for-check)
    - [Example Help Output](#example-help-output)
  - [Configuration](#configuration)
    - [Configuration File Example](#configuration-file-example)
    - [Initializing Configuration](#initializing-configuration)
    - [Configuration in pyproject.toml](#configuration-in-pyprojecttoml)
  - [Output Style](#output-style)
    - [Output Format](#output-format)
  - [Development](#development)
    - [Prerequisites](#prerequisites)
    - [Building](#building)
    - [Testing](#testing)
  - [Using rumdl with pre-commit](#using-rumdl-with-pre-commit)
  - [License](#license)

## Quick Start

```bash
# Install using Cargo
cargo install rumdl

# Lint Markdown files in the current directory
rumdl check .

# Automatically fix issues
rumdl check --fix .

# Create a default configuration file
rumdl init
```

## Overview

rumdl is a high-performance Markdown linter and fixer that helps ensure consistency and best practices in your Markdown files. It offers:

- ⚡️ **Built for speed** with Rust
- 🔍 **50+ lint rules** covering common Markdown issues
- 🛠️ **Automatic fixing** with `--fix` for most rules
- 📦 **Zero dependencies** - single binary with no runtime requirements
- 🔧 **Highly configurable** with TOML-based config files
- 🌐 **Multiple installation options** - Rust, Python, standalone binaries
- 🐍 **Installable via pip** for Python users
- 📏 **Modern CLI** with detailed error reporting
- 🔄 **CI/CD friendly** with non-zero exit code on errors

## Installation

Choose the installation method that works best for you:

### Using Cargo (Rust)

```bash
cargo install rumdl
```

### Using pip (Python)

```bash
pip install rumdl
```

### Download binary

```bash
# Linux/macOS
curl -LsSf https://github.com/rvben/rumdl/releases/latest/download/rumdl-linux-x86_64.tar.gz | tar xzf - -C /usr/local/bin

# Windows PowerShell
Invoke-WebRequest -Uri "https://github.com/rvben/rumdl/releases/latest/download/rumdl-windows-x86_64.zip" -OutFile "rumdl.zip"
Expand-Archive -Path "rumdl.zip" -DestinationPath "$env:USERPROFILE\.rumdl"
```

## Usage

Getting started with rumdl is simple:

```bash
# Lint a single file
rumdl check README.md

# Lint all Markdown files in current directory and subdirectories
rumdl check .

# Automatically fix issues
rumdl check --fix README.md

# Create a default configuration file
rumdl init
```

Common usage examples:

```bash
# Lint with custom configuration
rumdl check --config my-config.toml docs/

# Disable specific rules
rumdl check --disable MD013,MD033 README.md

# Enable only specific rules
rumdl check --enable MD001,MD003 README.md

# Exclude specific files/directories
rumdl check --exclude "node_modules,dist" .

# Include only specific files/directories
rumdl check --include "docs/*.md,README.md" .

# Combine include and exclude patterns
rumdl check --include "docs/**/*.md" --exclude "docs/temp,docs/drafts" .

# Ignore gitignore rules
rumdl check --no-respect-gitignore .
```

## Rules

rumdl implements over 50 lint rules for Markdown files. Here are some key rule categories:

|  Category | Description | Example Rules |
|-----------|-------------|---------------|
| **Headings** | Proper heading structure and formatting | MD001, MD002, MD003 |
| **Lists** | Consistent list formatting and structure | MD004, MD005, MD007 |
| **Whitespace** | Proper spacing and line length | MD009, MD010, MD012 |
| **Code** | Code block formatting and language tags | MD040, MD046, MD048 |
| **Links** | Proper link and reference formatting | MD034, MD039, MD042 |
| **Images** | Image alt text and references | MD045, MD052 |
| **Style** | Consistent style across document | MD031, MD032, MD035 |

For a complete list of rules and their descriptions, see our [documentation](https://github.com/rvben/rumdl/blob/main/docs/RULES.md) or run:

```bash
rumdl --list-rules
```

## Command-line Interface

```bash
rumdl <command> [options] [file or directory...]
```

### Commands

- `check`: Lint Markdown files and print warnings/errors (main subcommand)
- `init`: Create a default `.rumdl.toml` configuration file in the current directory

### Options (for `check`)

- `-c, --config <file>`: Use custom configuration file
- `--fix`: Automatically fix issues where possible
- `-l, --list-rules`: List all available rules
- `-d, --disable <rules>`: Disable specific rules (comma-separated)
- `-e, --enable <rules>`: Enable only specific rules (comma-separated)
- `--exclude <patterns>`: Exclude specific files or directories (comma-separated glob patterns)
- `--include <patterns>`: Include only specific files or directories (comma-separated glob patterns)
- `--respect-gitignore`: Respect .gitignore files when scanning directories (default: true)
- `--no-respect-gitignore`: Don't respect .gitignore files (same as --ignore-gitignore)
- `-v, --verbose`: Show detailed output
- `--profile`: Show profiling information
- `-q, --quiet`: Suppress all output except errors

### Example Help Output

```bash
rumdl --help

Usage: rumdl <COMMAND> [OPTIONS] [PATHS...]

Commands:
  check    Lint Markdown files and print warnings/errors
  init     Create a default configuration file

Options (for 'check' subcommand):
  --fix                  Automatically fix fixable issues
  --rules <RULES>        Comma-separated list of rules to enable
  --config <FILE>        Path to configuration file
  --format <FORMAT>      Output format (default: text)
  --help                 Print help information
  --version              Print version information

If no command is given, 'check' is assumed (deprecated).
```

## Configuration

rumdl can be configured in several ways:

1. Using a `.rumdl.toml` file in your project directory
2. Using the `[tool.rumdl]` section in your project's `pyproject.toml` file (for Python projects)
3. Using command-line arguments

### Configuration File Example

Here's an example `.rumdl.toml` configuration file:

```toml
# Global settings
line-length = 100
exclude = ["node_modules", "build", "dist"]
respect-gitignore = true

# Disable specific rules
disabled-rules = ["MD013", "MD033"]

# Configure individual rules
[MD007]
indent = 2

[MD013]
line-length = 100
code-blocks = false
tables = false

[MD025]
level = 1
front-matter-title = "title"

[MD044]
names = ["rumdl", "Markdown", "GitHub"]

[MD048]
code-fence-style = "backtick"
```

### Initializing Configuration

To create a configuration file, use the `init` command:

```bash
# Create a .rumdl.toml file (for any project)
rumdl init

# Create or update a pyproject.toml file with rumdl configuration (for Python projects)
rumdl init --pyproject
```

### Configuration in pyproject.toml

For Python projects, you can include rumdl configuration in your `pyproject.toml` file, keeping all project configuration in one place. Example:

```toml
[tool.rumdl]
# Global options at root level
line-length = 100
disable = ["MD033"]
include = ["docs/*.md", "README.md"]
exclude = [".git", "node_modules"]
ignore-gitignore = false

# Rule-specific configuration
[tool.rumdl.MD013]
code_blocks = false
tables = false

[tool.rumdl.MD044]
names = ["rumdl", "Markdown", "GitHub"]
```

Both kebab-case (`line-length`, `ignore-gitignore`) and snake_case (`line_length`, `ignore_gitignore`) formats are supported for compatibility with different Python tooling conventions.

## Output Style

rumdl produces clean, colorized output similar to modern linting tools:

```text
README.md:12:1: [MD022] Headings should be surrounded by blank lines [*]
README.md:24:5: [MD037] Spaces inside emphasis markers: "* incorrect *" [*]
README.md:31:76: [MD013] Line length exceeds 80 characters
README.md:42:3: [MD010] Hard tabs found, use spaces instead [*]
```

When running with `--fix`, rumdl shows which issues were fixed:

```text
README.md:12:1: [MD022] Headings should be surrounded by blank lines [fixed]
README.md:24:5: [MD037] Spaces inside emphasis markers: "* incorrect *" [fixed]
README.md:42:3: [MD010] Hard tabs found, use spaces instead [fixed]

Fixed 3 issues in 1 file
```

For a more detailed view, use the `--verbose` option:

```text
✓ No issues found in CONTRIBUTING.md
README.md:12:1: [MD022] Headings should be surrounded by blank lines [*]
README.md:24:5: [MD037] Spaces inside emphasis markers: "* incorrect *" [*]
README.md:42:3: [MD010] Hard tabs found, use spaces instead [*]

Found 3 issues in 1 file (2 files checked)
Run with `--fix` to automatically fix issues
```

### Output Format

rumdl uses a consistent output format for all issues:

```text
{file}:{line}:{column}: [{rule*id}] {message} [{fix*indicator}]
```

The output is colorized by default:

- Filenames appear in blue and underlined
- Line and column numbers appear in cyan
- Rule IDs appear in yellow
- Error messages appear in white
- Fixable issues are marked with `[*]` in green
- Fixed issues are marked with `[fixed]` in green

## Development

### Prerequisites

- Rust 1.70 or higher
- Make (for development commands)

### Building

```bash
make build
```

### Testing

```bash
make test
```

## Using rumdl with pre-commit

You can use `rumdl` as a pre-commit hook in your own projects to check your Markdown files.

Add the following to your `.pre-commit-config.yaml`:

```yaml
repos:
-   repo: https://github.com/rvben/rumdl
    rev: v0.0.45  # Use the latest release tag
    hooks:
    -   id: rumdl-check  # Just check for issues
    # Or use rumdl-fix to automatically fix issues:
    # -   id: rumdl-fix
```

Then run `pre-commit install`.

Pre-commit will automatically download and install `rumdl` using `cargo`. `rumdl` will respect any `.rumdl.toml` or `pyproject.toml` configuration file present in your repository.

## License

rumdl is licensed under the MIT License. See the [LICENSE](LICENSE) file for details.
