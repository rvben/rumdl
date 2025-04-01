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
    - [Options](#options)
  - [Configuration](#configuration)
  - [Output Style](#output-style)
    - [Output Format](#output-format)
  - [Development](#development)
    - [Prerequisites](#prerequisites)
    - [Building](#building)
    - [Testing](#testing)
  - [License](#license)

## Quick Start

```bash
# Install using Cargo
cargo install rumdl

# Check Markdown files in the current directory
rumdl .

# Automatically fix issues
rumdl --fix .

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
# Check a single file
rumdl README.md

# Check all Markdown files in current directory and subdirectories
rumdl .

# Automatically fix issues
rumdl --fix README.md

# Create a default configuration file
rumdl init
```

Common usage examples:

```bash
# Check with custom configuration
rumdl --config my-config.toml docs/

# Disable specific rules
rumdl --disable MD013,MD033 README.md

# Enable only specific rules
rumdl --enable MD001,MD003 README.md

# Exclude specific files/directories
rumdl --exclude "node_modules,dist" .

# Include only specific files/directories
rumdl --include "docs/*.md,README.md" .

# Combine include and exclude patterns
rumdl --include "docs/**/*.md" --exclude "docs/temp,docs/drafts" .
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
rumdl [options] [file or directory...]
rumdl <command> [options]
```

### Commands

- `init`: Create a default `.rumdl.toml` configuration file in the current directory

### Options

- `-c, --config <file>`: Use custom configuration file
- `-f, --fix`: Automatically fix issues where possible
- `-l, --list-rules`: List all available rules
- `-d, --disable <rules>`: Disable specific rules (comma-separated)
- `-e, --enable <rules>`: Enable only specific rules (comma-separated)
- `--exclude <patterns>`: Exclude specific files or directories (comma-separated glob patterns)
- `--include <patterns>`: Include only specific files or directories (comma-separated glob patterns)
- `--respect-gitignore`: Respect .gitignore files when scanning directories
- `-v, --verbose`: Show detailed output

## Configuration

rumdl can be configured using a TOML configuration file. By default, it looks for `rumdl.toml` or `.rumdl.toml` in the current directory.

You can create a default configuration file using the `init` command:

```bash
rumdl init
```

This will create a `.rumdl.toml` file in the current directory with default settings that you can customize.

Example configuration file:

```toml
# Global configuration options
[global]
# List of rules to disable
disable = ["MD013", "MD033"]

# List of rules to enable exclusively (if provided, only these rules will run)
# enable = ["MD001", "MD003", "MD004"]

# List of file/directory patterns to include for linting (if provided, only these will be linted)
include = [
    # Documentation files
    "docs/**/*.md",
    "README.md",
    "CONTRIBUTING.md",
]

# List of file/directory patterns to exclude from linting
exclude = [
    # Common directories to exclude
    ".git",
    ".github",
    "node_modules",
    "vendor",
    "dist",
    "build",

    # Specific files or patterns
    "CHANGELOG.md",
    "LICENSE.md",
    "generated/*.md",
    "**/temp_*.md",
]

# Whether to respect .gitignore files when scanning directories
respect_gitignore = false

# Rule-specific configurations
[MD002]
level = 1  # Expected level for first heading

[MD003]
style = "atx"  # Heading style (atx, atx_closed, setext)
```

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

## License

MIT License