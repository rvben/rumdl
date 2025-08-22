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

## Quick Start

```bash
# Install using Cargo
cargo install rumdl

# Lint Markdown files in the current directory
rumdl check .

# Format files (automatically fix issues)
rumdl fmt .
# Alternative: rumdl check --fix .

# Create a default configuration file
rumdl init
```

## Overview

rumdl is a high-performance Markdown linter and formatter that helps ensure consistency and best practices
in your Markdown files. Inspired by [ruff](https://github.com/astral-sh/ruff)'s approach to Python linting,
rumdl brings similar speed and developer experience improvements to the Markdown ecosystem.

It offers:

- ⚡️ **Built for speed** with Rust - significantly faster than alternatives
- 🔍 **54 lint rules** covering common Markdown issues
- 🛠️ **Automatic formatting** with `--fix` for files and stdin/stdout
- 📦 **Zero dependencies** - single binary with no runtime requirements
- 🔧 **Highly configurable** with TOML-based config files
- 🌐 **Multiple installation options** - Rust, Python, standalone binaries
- 🐍 **Installable via pip** for Python users
- 📏 **Modern CLI** with detailed error reporting
- 🔄 **CI/CD friendly** with non-zero exit code on errors

## Table of Contents

- [rumdl - A high-performance Markdown linter, written in Rust](#rumdl---a-high-performance-markdown-linter-written-in-rust)
  - [A modern Markdown linter and formatter, built for speed with Rust](#a-modern-markdown-linter-and-formatter-built-for-speed-with-rust)
  - [Quick Start](#quick-start)
  - [Overview](#overview)
  - [Table of Contents](#table-of-contents)
  - [Installation](#installation)
    - [Using Homebrew (macOS/Linux)](#using-homebrew-macoslinux)
    - [Using Cargo (Rust)](#using-cargo-rust)
    - [Using pip (Python)](#using-pip-python)
    - [Using uv](#using-uv)
    - [Download binary](#download-binary)
    - [VS Code Extension](#vs-code-extension)
  - [Usage](#usage)
    - [Stdin/Stdout Formatting](#stdinstdout-formatting)
    - [Editor Integration](#editor-integration)
  - [Pre-commit Integration](#pre-commit-integration)
  - [CI/CD Integration](#cicd-integration)
    - [GitHub Actions](#github-actions)
  - [Rules](#rules)
  - [Command-line Interface](#command-line-interface)
    - [Commands](#commands)
      - [`check [PATHS...]`](#check-paths)
      - [`fmt [PATHS...]`](#fmt-paths)
      - [`init [OPTIONS]`](#init-options)
      - [`import <FILE> [OPTIONS]`](#import-file-options)
      - [`rule [<rule>]`](#rule-rule)
      - [`config [OPTIONS] [COMMAND]`](#config-options-command)
      - [`server [OPTIONS]`](#server-options)
      - [`vscode [OPTIONS]`](#vscode-options)
      - [`version`](#version)
    - [Global Options](#global-options)
    - [Exit Codes](#exit-codes)
    - [Usage Examples](#usage-examples)
  - [Configuration](#configuration)
    - [Markdownlint Migration](#markdownlint-migration)
    - [Configuration File Example](#configuration-file-example)
    - [Initializing Configuration](#initializing-configuration)
    - [Configuration in pyproject.toml](#configuration-in-pyprojecttoml)
    - [Configuration Output](#configuration-output)
      - [Effective Configuration (`rumdl config`)](#effective-configuration-rumdl-config)
      - [Example output](#example-output)
    - [Defaults Only (`rumdl config --defaults`)](#defaults-only-rumdl-config---defaults)
  - [Output Style](#output-style)
    - [Output Format](#output-format)
      - [Text Output (Default)](#text-output-default)
      - [JSON Output](#json-output)
  - [Development](#development)
    - [Prerequisites](#prerequisites)
    - [Building](#building)
    - [Testing](#testing)
  - [License](#license)

## Installation

Choose the installation method that works best for you:

### Using Homebrew (macOS/Linux)

```bash
brew tap rvben/rumdl
brew install rumdl
```

### Using Cargo (Rust)

```bash
cargo install rumdl
```

### Using pip (Python)

```bash
pip install rumdl
```

### Using uv

For faster installation and better dependency management with [uv](https://github.com/astral-sh/uv):

```bash
# Install directly
uv tool install rumdl

# Or run without installing
uv tool run rumdl check .
```

### Download binary

```bash
# Linux/macOS
curl -LsSf https://github.com/rvben/rumdl/releases/latest/download/rumdl-linux-x86_64.tar.gz | tar xzf - -C /usr/local/bin

# Windows PowerShell
Invoke-WebRequest -Uri "https://github.com/rvben/rumdl/releases/latest/download/rumdl-windows-x86_64.zip" -OutFile "rumdl.zip"
Expand-Archive -Path "rumdl.zip" -DestinationPath "$env:USERPROFILE\.rumdl"
```

### VS Code Extension

For the best development experience, install the rumdl VS Code extension directly from the command line:

```bash
# Install the VS Code extension
rumdl vscode

# Check if the extension is installed
rumdl vscode --status

# Force reinstall the extension
rumdl vscode --force
```

The extension provides:

- 🔍 Real-time linting as you type
- 💡 Quick fixes for common issues
- 🎨 Code formatting on save
- 📋 Hover tooltips with rule documentation
- ⚡ Lightning-fast performance with zero lag

The CLI will automatically detect VS Code, Cursor, or Windsurf and install the appropriate extension. See the [VS Code extension documentation](https://github.com/rvben/rumdl/blob/main/docs/vscode-extension.md) for more details.

## Usage

Getting started with rumdl is simple:

```bash
# Lint a single file
rumdl check README.md

# Lint all Markdown files in current directory and subdirectories
rumdl check .

# Format a specific file
rumdl fmt README.md

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

# Don't respect gitignore files (note: --respect-gitignore defaults to true)
rumdl check --respect-gitignore=false .
```

### Stdin/Stdout Formatting

rumdl supports formatting via stdin/stdout, making it ideal for editor integrations and CI pipelines:

```bash
# Format content from stdin and output to stdout
cat README.md | rumdl fmt - > README_formatted.md
# Alternative: cat README.md | rumdl fmt --stdin > README_formatted.md

# Use in a pipeline
echo "# Title   " | rumdl fmt -
# Output: # Title

# Format clipboard content (macOS example)
pbpaste | rumdl fmt - | pbcopy

# Provide filename context for better error messages (useful for editor integrations)
cat README.md | rumdl check - --stdin-filename README.md
```

### Editor Integration

For editor integration, use stdin/stdout mode with the `--quiet` flag to suppress diagnostic messages:

```bash
# Format selection in editor (example for vim)
:'<,'>!rumdl fmt - --quiet

# Format entire buffer
:%!rumdl fmt - --quiet
```

## Pre-commit Integration

You can use `rumdl` as a pre-commit hook to check and fix your Markdown files.

The recommended way is to use the official pre-commit hook repository:

[rumdl-pre-commit repository](https://github.com/rvben/rumdl-pre-commit)

Add the following to your `.pre-commit-config.yaml`:

```yaml
repos:
  - repo: https://github.com/rvben/rumdl-pre-commit
    rev: v0.0.99  # Use the latest release tag
    hooks:
      - id: rumdl
        # To only check (default):
        # args: []
        # To automatically fix issues:
        # args: [--fix]
```

- By default, the hook will only check for issues.
- To automatically fix issues, add `args: [--fix]` to the hook configuration.

When you run `pre-commit install` or `pre-commit run`, pre-commit will automatically install `rumdl` in an isolated Python environment using pip. You do **not** need to install rumdl manually.

## CI/CD Integration

### GitHub Actions

rumdl can output annotations directly in GitHub Actions format, making issues appear inline in pull requests:

```yaml
- name: Lint Markdown
  run: rumdl check --output-format github .
```

This produces annotations that GitHub automatically displays in the PR's "Files changed" tab. Supports error/warning severity levels and precise issue locations.

## Rules

rumdl implements 54 lint rules for Markdown files. Here are some key rule categories:

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

#### `check [PATHS...]`

Lint Markdown files and print warnings/errors (main subcommand)

**Arguments:**

- `[PATHS...]`: Files or directories to lint. If provided, these paths take precedence over include patterns

**Options:**

- `-f, --fix`: Automatically fix issues where possible
- `-l, --list-rules`: List all available rules
- `-d, --disable <rules>`: Disable specific rules (comma-separated)
- `-e, --enable <rules>`: Enable only specific rules (comma-separated)
- `--exclude <patterns>`: Exclude specific files or directories (comma-separated glob patterns)
- `--include <patterns>`: Include only specific files or directories (comma-separated glob patterns)
- `--respect-gitignore`: Respect .gitignore files when scanning directories (does not apply to explicitly provided paths)
- `-v, --verbose`: Show detailed output
- `--profile`: Show profiling information
- `--statistics`: Show rule violation statistics summary
- `-q, --quiet`: Quiet mode
- `-o, --output <format>`: Output format: `text` (default) or `json`
- `--stdin`: Read from stdin instead of files

#### `fmt [PATHS...]`

Format Markdown files (alias for `check --fix`)

**Arguments:**

- `[PATHS...]`: Files or directories to format. If provided, these paths take precedence over include patterns

**Options:**

All the same options as `check` are available (except `--fix` which is always enabled), including:
- `--stdin`: Format content from stdin and output to stdout
- `-d, --disable <rules>`: Disable specific rules during formatting
- `-e, --enable <rules>`: Format using only specific rules
- `--exclude/--include`: Control which files to format
- `-q, --quiet`: Suppress diagnostic output

**Examples:**

```bash
# Format all Markdown files in current directory
rumdl fmt

# Format specific file
rumdl fmt README.md

# Format from stdin (using dash syntax)
cat README.md | rumdl fmt - > formatted.md
# Alternative: cat README.md | rumdl fmt --stdin > formatted.md
```

#### `init [OPTIONS]`

Create a default configuration file in the current directory

**Options:**

- `--pyproject`: Generate configuration for `pyproject.toml` instead of `.rumdl.toml`

#### `import <FILE> [OPTIONS]`

Import and convert markdownlint configuration files to rumdl format

**Arguments:**

- `<FILE>`: Path to markdownlint config file (JSON/YAML)

**Options:**

- `-o, --output <path>`: Output file path (default: `.rumdl.toml`)
- `--format <format>`: Output format: `toml` or `json` (default: `toml`)
- `--dry-run`: Show converted config without writing to file

#### `rule [<rule>]`

Show information about a rule or list all rules

**Arguments:**

- `[rule]`: Rule name or ID (optional). If provided, shows details for that rule. If omitted, lists all available rules

#### `config [OPTIONS] [COMMAND]`

Show configuration or query a specific key

**Options:**

- `--defaults`: Show only the default configuration values
- `--output <format>`: Output format (e.g. `toml`, `json`)

**Subcommands:**

- `get <key>`: Query a specific config key (e.g. `global.exclude` or `MD013.line_length`)
- `file`: Show the absolute path of the configuration file that was loaded

#### `server [OPTIONS]`

Start the Language Server Protocol server for editor integration

**Options:**

- `--port <PORT>`: TCP port to listen on (for debugging)
- `--stdio`: Use stdio for communication (default)
- `-v, --verbose`: Enable verbose logging

#### `vscode [OPTIONS]`

Install the rumdl VS Code extension

**Options:**

- `--force`: Force reinstall even if already installed
- `--status`: Show installation status without installing

#### `version`

Show version information

### Global Options

These options are available for all commands:

- `--color <mode>`: Control colored output: `auto` (default), `always`, `never`
- `--config <file>`: Path to configuration file
- `--no-config`: Ignore all configuration files and use built-in defaults

### Exit Codes

rumdl uses exit codes following Ruff's convention for easy CI/CD integration:

- `0`: Success - no issues found
- `1`: Linting violations found
- `2`: Tool error (configuration error, file access error, invalid arguments, etc.)

This allows scripts and CI/CD systems to distinguish between "the tool found problems in your files" (exit 1) and "the tool couldn't run properly" (exit 2).

### Usage Examples

```bash
# Lint all Markdown files in the current directory
rumdl check .

# Format files (automatically fix issues)
rumdl fmt .
# Alternative: rumdl check --fix .

# Create a default configuration file
rumdl init

# Create or update a pyproject.toml file with rumdl configuration
rumdl init --pyproject

# Import a markdownlint config file
rumdl import .markdownlint.json

# Convert markdownlint config to JSON format
rumdl import --format json .markdownlint.yaml --output rumdl-config.json

# Preview conversion without writing file
rumdl import --dry-run .markdownlint.json

# Show information about a specific rule
rumdl rule MD013

# List all available rules
rumdl rule

# Query a specific config key
rumdl config get global.exclude

# Show the path of the loaded configuration file
rumdl config file

# Show configuration as JSON instead of the default format
rumdl config --output json

# Lint content from stdin
echo "# My Heading" | rumdl check --stdin

# Get JSON output for integration with other tools
rumdl check --output json README.md

# Show statistics summary of rule violations
rumdl check --statistics .

# Disable colors in output
rumdl check --color never README.md

# Use built-in defaults, ignoring all config files
rumdl check --no-config README.md

# Show version information
rumdl version
```

## Configuration

rumdl can be configured in several ways:

1. Using a `.rumdl.toml` file in your project directory or parent directories
2. Using the `[tool.rumdl]` section in your project's `pyproject.toml` file (for Python projects)
3. Using command-line arguments
4. **Automatic markdownlint compatibility**: rumdl automatically discovers and loads existing markdownlint config files (`.markdownlint.json`, `.markdownlint.yaml`, etc.)

### Configuration Discovery

rumdl automatically searches for configuration files by traversing up the directory tree from the current
working directory, similar to tools like `git`, `ruff`, and `eslint`. This means you can run rumdl from any
subdirectory of your project and it will find the configuration file at the project root.

The search follows these rules:
- Searches upward for `.rumdl.toml`, `rumdl.toml`, or `pyproject.toml` (with `[tool.rumdl]` section)
- Stops at the first configuration file found
- Stops searching when it encounters a `.git` directory (project boundary)
- Maximum traversal depth of 100 directories

To disable all configuration discovery and use only built-in defaults, use the `--isolated` flag:

```bash
# Use discovered configuration (default behavior)
rumdl check .

# Ignore all configuration files
rumdl check --isolated .
```

### Markdownlint Migration

rumdl provides seamless compatibility with existing markdownlint configurations:

**Automatic Discovery**: rumdl automatically detects and loads markdownlint config files:

- `.markdownlint.json` / `.markdownlint.jsonc`
- `.markdownlint.yaml` / `.markdownlint.yml`
- `markdownlint.json` / `markdownlint.yaml`

**Explicit Import**: Convert markdownlint configs to rumdl format:

```bash
# Convert to .rumdl.toml
rumdl import .markdownlint.json

# Convert to JSON format
rumdl import --format json .markdownlint.yaml --output config.json

# Preview conversion
rumdl import --dry-run .markdownlint.json
```

For comprehensive documentation on global settings (file selection, rule enablement, etc.), see our [Global Settings Reference](docs/global-settings.md).

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

### Configuration Output

#### Effective Configuration (`rumdl config`)

The `rumdl config` command prints the **full effective configuration** (defaults + all overrides), showing every key and its value, annotated with the source of each value.
The output is colorized and the `[from ...]` annotation is globally aligned for easy scanning.

#### Example output

```text
[global]
  enable             = []                             [from default]
  disable            = ["MD033"]                      [from .rumdl.toml]
  include            = ["README.md"]                  [from .rumdl.toml]
  respect_gitignore  = true                           [from .rumdl.toml]

[MD013]
  line_length        = 200                            [from .rumdl.toml]
  code_blocks        = true                           [from .rumdl.toml]
  ...
```

- **Keys** are cyan, **values** are yellow, and the `[from ...]` annotation is colored by source:
  - Green: CLI
  - Blue: `.rumdl.toml`
  - Magenta: `pyproject.toml`
  - Yellow: default
- The `[from ...]` column is aligned across all sections.

### Defaults Only (`rumdl config --defaults`)

The `--defaults` flag prints only the default configuration as TOML, suitable for copy-paste or reference:

```toml
[global]
enable = []
disable = []
exclude = []
include = []
respect_gitignore = true

[MD013]
line_length = 80
code_blocks = true
...
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
Run `rumdl fmt` to automatically fix issues
```

### Output Format

#### Text Output (Default)

rumdl uses a consistent output format for all issues:

```text
{file}:{line}:{column}: [{rule_id}] {message} [{fix_indicator}]
```

The output is colorized by default:

- Filenames appear in blue and underlined
- Line and column numbers appear in cyan
- Rule IDs appear in yellow
- Error messages appear in white
- Fixable issues are marked with `[*]` in green
- Fixed issues are marked with `[fixed]` in green

#### JSON Output

For integration with other tools and automation, use `--output json`:

```bash
rumdl check --output json README.md
```

This produces structured JSON output:

```json
{
  "summary": {
    "total_files": 1,
    "files_with_issues": 1,
    "total_issues": 2,
    "fixable_issues": 1
  },
  "files": [
    {
      "path": "README.md",
      "issues": [
        {
          "line": 12,
          "column": 1,
          "rule": "MD022",
          "message": "Headings should be surrounded by blank lines",
          "fixable": true,
          "severity": "error"
        }
      ]
    }
  ]
}
```

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

rumdl is licensed under the MIT License. See the [LICENSE](LICENSE) file for details.
