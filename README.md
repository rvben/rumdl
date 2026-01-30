# rumdl - A high-performance Markdown linter, written in Rust

<div align="center">

![rumdl Logo](https://raw.githubusercontent.com/rvben/rumdl/main/assets/logo.png)

[![Build Status](https://img.shields.io/github/actions/workflow/status/rvben/rumdl/ci.yml)](https://github.com/rvben/rumdl/actions)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![Crates.io](https://img.shields.io/crates/v/rumdl)](https://crates.io/crates/rumdl)
[![PyPI](https://img.shields.io/pypi/v/rumdl)](https://pypi.org/project/rumdl/)
[![GitHub release (latest by date)](https://img.shields.io/github/v/release/rvben/rumdl)](https://github.com/rvben/rumdl/releases/latest)
[![GitHub stars](https://img.shields.io/github/stars/rvben/rumdl)](https://github.com/rvben/rumdl/stargazers)
[![Discord](https://img.shields.io/badge/Discord-Join%20us-5865F2?logo=discord&logoColor=white)](https://discord.gg/ADTJFSFUyn)
[![Sponsor](https://img.shields.io/badge/Sponsor-%E2%9D%A4-pink?logo=github-sponsors)](https://github.com/sponsors/rvben)

## A modern Markdown linter and formatter, built for speed with Rust

| [**Docs**](https://rumdl.dev)
| [**Rules**](https://rumdl.dev/RULES)
| [**Configuration**](https://rumdl.dev/global-settings)
| [**Markdown Flavors**](https://rumdl.dev/flavors)
| [**vs markdownlint**](https://rumdl.dev/markdownlint-comparison) |

</div>

## Quick Start

```bash
# Install using Cargo
cargo install rumdl

# Lint Markdown files in the current directory
rumdl check .

# Format files (exits 0 on success, even if unfixable violations remain)
rumdl fmt .

# Auto-fix and report unfixable violations (exits 0 if all fixed, 1 if violations remain)
rumdl check --fix .

# Create a default configuration file
rumdl init
```

## Overview

rumdl is a high-performance Markdown linter and formatter that helps ensure consistency and best practices in your Markdown files. Inspired by [ruff](https://github.com/astral-sh/ruff) 's approach to
Python linting, rumdl brings similar speed and developer experience improvements to the Markdown ecosystem.

It offers:

- ⚡️ **Built for speed** with Rust - significantly faster than alternatives
- 🔍 **57 lint rules** covering common Markdown issues
- 🛠️ **Automatic formatting** with `--fix` for files and stdin/stdout
- 📦 **Zero dependencies** - single binary with no runtime requirements
- 🔧 **Highly configurable** with TOML-based config files
- 🎯 **Multiple Markdown flavors** - [GFM, MkDocs, MDX, Quarto](docs/flavors.md) support with auto-detection
- 🌐 **Multiple installation options** - Rust, Python, standalone binaries
- 🐍 **Installable via pip** for Python users
- 📏 **Modern CLI** with detailed error reporting
- 🔄 **CI/CD friendly** with non-zero exit code on errors

### Performance

rumdl is designed for speed. Benchmarked on the [Rust Book](https://github.com/rust-lang/book) repository (478 markdown files, October 2025):

![Cold start benchmark comparison](assets/benchmark.svg)

With intelligent caching, subsequent runs are even faster - rumdl only re-lints files that have changed, making it ideal for watch mode and editor integration.

## Table of Contents

- [Installation](#installation)
  - [Using Homebrew (macOS/Linux)](#using-homebrew-macoslinux)
  - [Using Cargo (Rust)](#using-cargo-rust)
  - [Using pip (Python)](#using-pip-python)
  - [Using uv](#using-uv)
  - [Using mise](#using-mise)
  - [Using Nix (macOS/Linux)](#using-nix-macoslinux)
  - [Using Termux User Repository (TUR) (Android)](#using-termux-user-repository-tur-android)
  - [Using Archlinux User Repository](#using-archlinux-user-repository)
  - [Download binary](#download-binary)
  - [Editor Plugins](#editor-plugins)
- [Usage](#usage)
  - [Stdin/Stdout Formatting](#stdinstdout-formatting)
  - [Editor Integration](#editor-integration)
- [Pre-commit Integration](#pre-commit-integration)
  - [Excluding Files in Pre-commit](#excluding-files-in-pre-commit)
- [CI/CD Integration](#cicd-integration)
  - [GitHub Actions](#github-actions)
    - [Inputs](#inputs)
    - [Examples](#examples)
- [Rules](#rules)
- [Flavors](#flavors)
  - [Supported Flavors](#supported-flavors)
  - [Configuring Flavors](#configuring-flavors)
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
- [LSP](#lsp)
- [Configuration](#configuration)
  - [Configuration Discovery](#configuration-discovery)
  - [Editor Support (JSON Schema)](#editor-support-json-schema)
  - [Global Configuration](#global-configuration)
  - [Markdownlint Migration](#markdownlint-migration)
  - [Inline Configuration](#inline-configuration)
  - [Configuration File Example](#configuration-file-example)
  - [Style Guide Presets](#style-guide-presets)
  - [Initializing Configuration](#initializing-configuration)
  - [Configuration in pyproject.toml](#configuration-in-pyprojecttoml)
  - [Configuration Output](#configuration-output)
    - [Effective Configuration (`rumdl config`)](#effective-configuration-rumdl-config)
    - [Example output](#example-output)
  - [Defaults Only (`rumdl config --defaults`)](#defaults-only-rumdl-config---defaults)
  - [Non-Defaults Only (`rumdl config --no-defaults`)](#non-defaults-only-rumdl-config---no-defaults)
- [Output Style](#output-style)
  - [Output Format](#output-format)
    - [Text Output (Default)](#text-output-default)
    - [JSON Output](#json-output)
- [Development](#development)
  - [Prerequisites](#prerequisites)
  - [Building](#building)
  - [Testing](#testing)
  - [JSON Schema Generation](#json-schema-generation)
- [Used By](#used-by)
- [Sponsors](#sponsors)
- [License](#license)

## Installation

Choose the installation method that works best for you:

### Using Homebrew (macOS/Linux)

```bash
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
uvx rumdl check .
```

### Using mise

For dependency management with [mise](https://github.com/jdx/mise):

```bash
# List available versions
mise ls-remote rumdl

# Install the latest version
mise install rumdl

# Use a specific version for the project
mise use rumdl@0.1.7
```

### Using Nix (macOS/Linux)

```bash
nix-channel --update
nix-env --install --attr nixpkgs.rumdl
```

Alternatively, you can use flakes to run it without installation.

```bash
nix run --extra-experimental-features 'flakes nix-command' nixpkgs/nixpkgs-unstable#rumdl -- --version
```

### Using Termux User Repository (TUR) (Android)

After enabling the TUR repo using

```bash
pkg install tur-repo
```

```bash
pkg install rumdl
```

### Using Archlinux User Repository

[![rumdl on AUR](https://img.shields.io/aur/version/rumdl?label=rumdl)](https://aur.archlinux.org/packages/rumdl/)
[![rumdl-bin on AUR](https://img.shields.io/aur/version/rumdl-bin?label=rumdl-bin)](https://aur.archlinux.org/packages/rumdl-bin/)

rumdl is available on the [AUR](https://wiki.archlinux.org/index.php/Arch_User_Repository):

- [rumdl](https://aur.archlinux.org/packages/rumdl/) (release package)
- [rumdl-bin](https://aur.archlinux.org/packages/rumdl-bin/) (binary package)

You can install it using your [AUR helper](https://wiki.archlinux.org/index.php/AUR_helpers) of choice.

```bash
yay -Sy rumdl
# OR
yay -Sy rumdl-bin
```

### Download binary

```bash
# Linux/macOS
curl -LsSf https://github.com/rvben/rumdl/releases/latest/download/rumdl-linux-x86_64.tar.gz | tar xzf - -C /usr/local/bin

# Windows PowerShell
Invoke-WebRequest -Uri "https://github.com/rvben/rumdl/releases/latest/download/rumdl-windows-x86_64.zip" -OutFile "rumdl.zip"
Expand-Archive -Path "rumdl.zip" -DestinationPath "$env:USERPROFILE\.rumdl"
```

### Editor Plugins

| Editor                              | Install                                                                                          |
| ----------------------------------- | ------------------------------------------------------------------------------------------------ |
| VS Code / Cursor / Windsurf         | `rumdl vscode` or [Marketplace](https://marketplace.visualstudio.com/items?itemName=rumdl.rumdl) |
| JetBrains (PyCharm, IntelliJ, etc.) | [JetBrains Marketplace](https://plugins.jetbrains.com/plugin/29943-rumdl)                        |

All plugins provide real-time linting, formatting on save, hover documentation, and automatic configuration discovery.

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

# Watch mode for continuous linting
rumdl check --watch docs/

# Combine include and exclude patterns
rumdl check --include "docs/**/*.md" --exclude "docs/temp,docs/drafts" .

# Don't respect gitignore files (note: --respect-gitignore defaults to true)
rumdl check --respect-gitignore=false .

# Force exclude patterns even for explicitly specified files (useful for pre-commit)
rumdl check excluded.md --force-exclude  # Will respect exclude patterns in config
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

You can use `rumdl` as a pre-commit hook to check and format your Markdown files.

The recommended way is to use the official pre-commit hook repository:

[rumdl-pre-commit repository](https://github.com/rvben/rumdl-pre-commit)

Add the following to your `.pre-commit-config.yaml`:

```yaml
repos:
  - repo: https://github.com/rvben/rumdl-pre-commit
    rev: v0.1.7
    hooks:
      - id: rumdl      # Lint only (fails on issues)
      - id: rumdl-fmt  # Auto-format and fail if issues remain
```

Two hooks are available:

- **`rumdl`** — Lints files and fails if any issues are found
- **`rumdl-fmt`** — Auto-formats files and fails if unfixable violations remain (recommended for CI)

When you run `pre-commit install` or `pre-commit run`, pre-commit will automatically install `rumdl` in an isolated Python environment using pip. You do **not** need to install rumdl manually.

### Excluding Files in Pre-commit

By default, when pre-commit passes files explicitly to rumdl, the exclude patterns in your `.rumdl.toml` configuration file are ignored. This is intentional behavior - if you explicitly specify a
file, it gets checked.

However, for pre-commit workflows where you want to exclude certain files even when they're passed explicitly, you have two options:

1. **Use `force_exclude` in your configuration file:**

   ```toml
   # .rumdl.toml
   [global]
   exclude = ["generated/*.md", "vendor/**"]
   force_exclude = true  # Enforce excludes even for explicitly provided files
   ```

2. **Use the `--force-exclude` flag in your pre-commit config:**

   ```yaml
   repos:
     - repo: https://github.com/rvben/rumdl-pre-commit
       rev: v0.1.7
       hooks:
         - id: rumdl
           args: [--force-exclude]  # Respect exclude patterns from config
   ```

## CI/CD Integration

### GitHub Actions

We have a companion Action you can use to integrate rumdl directly in your workflow:

```yaml
jobs:
  rumdl-check:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v6
      - uses: rvben/rumdl@0.1.0v0
```

The `v0` tag always points to the latest stable release, following GitHub Actions conventions.

#### Inputs

| Input         | Description                            | Default        |
| ------------- | -------------------------------------- | -------------- |
| `version`     | Version of rumdl to install            | latest         |
| `path`        | Path to lint                           | workspace root |
| `config`      | Path to config file                    | auto-detected  |
| `report-type` | Output format: `logs` or `annotations` | `logs`         |

#### Examples

**Lint specific directory with pinned version:**

```yaml
- uses: rvben/rumdl@0.1.0v0
  with:
    version: "0.0.189"
    path: docs/
```

**Use custom config and show annotations in PR:**

```yaml
- uses: rvben/rumdl@0.1.0v0
  with:
    config: .rumdl.toml
    report-type: annotations
```

The `annotations` report type displays issues directly in the PR's "Files changed" tab with error/warning severity levels and precise locations.

## Rules

rumdl implements 54 lint rules for Markdown files. Here are some key rule categories:

| Category       | Description                              | Example Rules       |
| -------------- | ---------------------------------------- | ------------------- |
| **Headings**   | Proper heading structure and formatting  | MD001, MD002, MD003 |
| **Lists**      | Consistent list formatting and structure | MD004, MD005, MD007 |
| **Whitespace** | Proper spacing and line length           | MD009, MD010, MD012 |
| **Code**       | Code block formatting and language tags  | MD040, MD046, MD048 |
| **Links**      | Proper link and reference formatting     | MD034, MD039, MD042 |
| **Images**     | Image alt text and references            | MD045, MD052        |
| **Style**      | Consistent style across document         | MD031, MD032, MD035 |

For a complete list of rules and their descriptions, see our [documentation](https://rumdl.dev/rules/) or run:

```bash
rumdl rule
```

## Flavors

rumdl supports multiple Markdown flavors to accommodate different documentation systems. Each flavor adjusts rule behavior for syntax specific to that system, reducing false positives.

### Supported Flavors

| Flavor                                              | Use Case                     | Key Features                                      |
| --------------------------------------------------- | ---------------------------- | ------------------------------------------------- |
| [standard](docs/flavors/standard.md)                | Default Markdown             | CommonMark + GFM extensions (tables, task lists)  |
| [gfm](docs/flavors/gfm.md)                          | GitHub Flavored Markdown     | Extended autolinks, security-sensitive HTML       |
| [mkdocs](docs/flavors/mkdocs.md)                    | MkDocs / Material for MkDocs | Admonitions, content tabs, mkdocstrings           |
| [mdx](docs/flavors/mdx.md)                          | MDX (JSX in Markdown)        | JSX components, ESM imports, expressions          |
| [quarto](docs/flavors/quarto.md)                    | Quarto / RMarkdown           | Citations, shortcodes, executable code blocks     |

### Configuring Flavors

Set a global flavor in your configuration:

```toml
[global]
flavor = "mkdocs"
```

Or configure per-file patterns:

```toml
[per-file-flavor]
"docs/**/*.md" = "mkdocs"
"**/*.mdx" = "mdx"
"**/*.qmd" = "quarto"
```

When no flavor is configured, rumdl auto-detects based on file extension (`.mdx` → mdx, `.qmd`/`.Rmd` → quarto, `.md` → standard).

For complete flavor documentation, see the [Flavors Guide](docs/flavors.md).

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
- `--diff`: Show diff of what would be fixed instead of fixing files
- `-w, --watch`: Run in watch mode by re-running whenever files change
- `-l, --list-rules`: List all available rules
- `-d, --disable <rules>`: Disable specific rules (comma-separated)
- `-e, --enable <rules>`: Enable only specific rules (comma-separated)
- `--exclude <patterns>`: Exclude specific files or directories (comma-separated glob patterns)
- `--include <patterns>`: Include only specific files or directories (comma-separated glob patterns)
- `--respect-gitignore`: Respect .gitignore files when scanning directories (does not apply to explicitly provided paths)
- `--force-exclude`: Enforce exclude patterns even for explicitly specified files (useful for pre-commit hooks)
- `-v, --verbose`: Show detailed output
- `--profile`: Show profiling information
- `--statistics`: Show rule violation statistics summary
- `-q, --quiet`: Quiet mode
- `-o, --output <format>`: Output format: `text` (default) or `json`
- `--stdin`: Read from stdin instead of files

#### `fmt [PATHS...]`

Format Markdown files and output the result. Always exits with code 0 on successful formatting, making it ideal for editor integration.

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
- `--no-defaults`: Show only non-default configuration values (exclude defaults)
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

- `0`: Success (no violations found, or all violations were fixed)
- `1`: Violations found (or remain after `--fix`)
- `2`: Tool error

**Note:** `rumdl fmt` exits 0 on successful formatting (even if unfixable violations remain), making it compatible with editor integrations. `rumdl check --fix` exits 0 if all violations are fixed, or
1 if violations remain after fixing (useful for pre-commit hooks and CI/CD).

### Usage Examples

```bash
# Lint all Markdown files in the current directory
rumdl check .

# Format files (exits 0 on success, even if unfixable violations remain)
rumdl fmt .

# Auto-fix and report unfixable violations (exits 0 if all fixed, 1 if violations remain)
rumdl check --fix .

# Preview what would be fixed without modifying files
rumdl check --diff .

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

# Show only non-default configuration values
rumdl config --no-defaults

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

## LSP

rumdle is also available as LSP server for editor integration.

For editor-specific information on setting up the LSP, refer to our [LSP documentation](https://rumdl.dev/lsp/)

## Configuration

rumdl can be configured in several ways:

1. Using a `.rumdl.toml` or `rumdl.toml` file in your project directory or parent directories
2. Using a `<project>/.config/rumdl.toml` file (following the [config-dir convention](https://github.com/pi0/config-dir))
3. Using the `[tool.rumdl]` section in your project's `pyproject.toml` file (for Python projects)
4. Using command-line arguments
5. Using a **global user config** at `~/.config/rumdl/rumdl.toml` (see [Global Configuration](#global-configuration) below)
6. **Automatic markdownlint compatibility**: rumdl automatically discovers and loads existing markdownlint config files (`.markdownlint.json`, `.markdownlint.yaml`, etc.)

### Configuration Discovery

rumdl automatically searches for configuration files by traversing up the directory tree from the current working directory, similar to tools like `git` , `ruff` , and `eslint` . This means you can
run rumdl from any subdirectory of your project and it will find the configuration file at the project root.

The search follows these rules:

- Searches upward for `.rumdl.toml`, `rumdl.toml`, `<dir>/.config/rumdl.toml`, or `pyproject.toml` (with `[tool.rumdl]` section)
- Precedence order: `.rumdl.toml` > `rumdl.toml` > `<dir>/.config/rumdl.toml` > `pyproject.toml`
- Stops at the first configuration file found
- Stops searching when it encounters a `.git` directory (project boundary)
- Maximum traversal depth of 100 directories
- Falls back to markdownlint config files (`.markdownlint.yaml`, etc.) using the same upward traversal
- Falls back to user configuration if no project configuration is found (see Global Configuration below)

To disable all configuration discovery and use only built-in defaults, use the `--isolated` flag:

```bash
# Use discovered configuration (default behavior)
rumdl check .

# Ignore all configuration files
rumdl check --isolated .
```

### Editor Support (JSON Schema)

rumdl provides a JSON Schema for `.rumdl.toml` configuration files, enabling autocomplete, validation, and inline documentation in supported editors like VS Code, IntelliJ IDEA, and others.

The schema is available at `https://raw.githubusercontent.com/rvben/rumdl/main/rumdl.schema.json`.

**Automatic Setup (via SchemaStore):**

The schema is registered with [SchemaStore](https://www.schemastore.org/), so editors with TOML support will automatically provide autocomplete and validation for `.rumdl.toml` and `rumdl.toml` files.

**VS Code:** Install the "Even Better TOML" extension - schema association is automatic.

**Manual Schema Association:**

If your editor doesn't support SchemaStore, add this comment to your config file:

```toml
# yaml-language-server: $schema=https://raw.githubusercontent.com/rvben/rumdl/main/rumdl.schema.json
```

### Global Configuration

When no project configuration is found, rumdl will check for a user-level configuration file in your platform's standard config directory:

**Location:**

- **Linux/macOS**: `~/.config/rumdl/` (respects `XDG_CONFIG_HOME` if set)
- **Windows**: `%APPDATA%\rumdl\`

**Files checked (in order):**

1. `.rumdl.toml`
2. `rumdl.toml`
3. `pyproject.toml` (must contain `[tool.rumdl]` section)

This allows you to set personal preferences that apply to all projects without local configuration.

**Example:** Create `~/.config/rumdl/rumdl.toml`:

```toml
[global]
line-length = 100
disable = ["MD013", "MD041"]

[MD007]
indent = 2
```

**Note:** User configuration is only used when no project configuration exists. Project configurations always take precedence.

### Markdownlint Migration

rumdl provides seamless compatibility with existing markdownlint configurations:

**Automatic Discovery**: rumdl automatically detects and loads markdownlint config files by traversing up the directory tree (just like `.rumdl.toml`):

- `.markdownlint.json` / `.markdownlint.jsonc`
- `.markdownlint.yaml` / `.markdownlint.yml`
- `markdownlint.json` / `markdownlint.yaml`

This means you can place a `.markdownlint.yaml` at your project root and run rumdl from any subdirectory - it will find and use the config automatically.

** Explicit Import**: Convert markdownlint configs to rumdl format:

```bash
# Convert to .rumdl.toml
rumdl import .markdownlint.json

# Convert to JSON format
rumdl import --format json .markdownlint.yaml --output config.json

# Preview conversion
rumdl import --dry-run .markdownlint.json
```

For comprehensive documentation on global settings (file selection, rule enablement, etc.), see our [Global Settings Reference](docs/global-settings.md).

### Inline Configuration

rumdl supports inline HTML comments to disable or configure rules for specific sections of your Markdown files. This is useful for making exceptions without changing global configuration:

```markdown
<!-- rumdl-disable MD013 -->
This line can be as long as needed without triggering the line length rule.
<!-- rumdl-enable MD013 -->
```

Note: `markdownlint-disable`/`markdownlint-enable` comments are also supported for compatibility with existing markdownlint configurations.

For complete documentation on inline configuration options, see our [Inline Configuration Reference](docs/inline-configuration.md).

### Configuration File Example

Here's an example `.rumdl.toml` configuration file:

```toml
# Global settings
line-length = 100
exclude = ["node_modules", "build", "dist"]
respect-gitignore = true
flavor = "mkdocs"  # Use MkDocs flavor (see Flavors section)

# Disable specific rules
disabled-rules = ["MD013", "MD033"]

# Per-file flavor overrides
[per-file-flavor]
"**/*.mdx" = "mdx"

# Disable specific rules for specific files
[per-file-ignores]
"README.md" = ["MD033"]  # Allow HTML in README
"SUMMARY.md" = ["MD025"]  # Allow multiple H1 in table of contents
"docs/api/**/*.md" = ["MD013", "MD041"]  # Relax rules for generated docs

# Configure individual rules
[MD007]
indent = 2

[MD013]
line-length = 100
code-blocks = false
tables = false
reflow = true  # Enable automatic line wrapping (required for --fix)

[MD025]
level = 1
front-matter-title = "title"

[MD044]
names = ["rumdl", "Markdown", "GitHub"]

[MD048]
code-fence-style = "backtick"
```

### Style Guide Presets

Ready-to-use configurations for popular style guides are available in the [`examples/`](examples/) directory:

- **[Google Style](examples/google-style.rumdl.toml)** - Google's Markdown style guide
- **[Prettier-compatible](examples/prettier-compatible.rumdl.toml)** - Aligns with Prettier's markdown formatting

Copy one to your project as `.rumdl.toml` to use it.

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

The `rumdl config` command prints the **full effective configuration** (defaults + all overrides), showing every key and its value, annotated with the source of each value. The output is colorized and
the `[from ...]` annotation is globally aligned for easy scanning.

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

- ** Keys** are cyan, **values** are yellow, and the `[from ...]` annotation is colored by source:
  - Green: CLI
  - Blue: `.rumdl.toml`
  - Magenta: `pyproject.toml`
  - Yellow: default
- The `[from ...]` column is aligned across all sections.

### Defaults Only (`rumdl config --defaults`)

The `rumdl config --defaults` command shows only the default configuration values, useful for understanding what the built-in defaults are.

### Non-Defaults Only (`rumdl config --no-defaults`)

The `rumdl config --no-defaults` command shows only configuration values that differ from defaults, making it easy to see what you've customized. This is particularly useful when you want to see only
your project-specific or user-specific overrides without the noise of default values.

**Example:**

```bash
$ rumdl config --no-defaults
[global]
disable = ["MD013"]                    [from project config]
line_length = 100                      [from pyproject.toml]

[MD004]
style = "asterisk"                     [from project config]
```

This helps you quickly identify what customizations you've made to the default configuration.

The `--defaults` flag prints only the default configuration as TOML, suitable for copy-paste or reference:

```toml
[global]
enable = []
disable = []
exclude = []
include = []
respect_gitignore = true
force_exclude = false  # Set to true to exclude files even when explicitly specified

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

- Rust 1.91 or higher
- Make (for development commands)

### Building

```bash
make build
```

### Testing

```bash
make test
```

### JSON Schema Generation

If you modify the configuration structures in `src/config.rs`, regenerate the JSON schema:

```bash
# Generate/update the schema
make schema
# Or: rumdl schema generate

# Check if schema is up-to-date (useful in CI)
make check-schema
# Or: rumdl schema check

# Print schema to stdout
rumdl schema print
```

The schema is automatically generated from the Rust types using `schemars` and should be kept in sync with the configuration structures.

## Used By

rumdl is used by these notable open source projects:

| Project                                                                                     | Stars                                                                                              |
| ------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------- |
| [PyO3/pyo3](https://github.com/PyO3/pyo3)                                                   | ![stars](https://img.shields.io/github/stars/PyO3/pyo3?style=flat-square)                          |
| [beeware/toga](https://github.com/beeware/toga)                                             | ![stars](https://img.shields.io/github/stars/beeware/toga?style=flat-square)                       |
| [beeware/briefcase](https://github.com/beeware/briefcase)                                   | ![stars](https://img.shields.io/github/stars/beeware/briefcase?style=flat-square)                  |
| [scop/bash-completion](https://github.com/scop/bash-completion)                             | ![stars](https://img.shields.io/github/stars/scop/bash-completion?style=flat-square)               |
| [chrisgrieser/nvim-scissors](https://github.com/chrisgrieser/nvim-scissors)                 | ![stars](https://img.shields.io/github/stars/chrisgrieser/nvim-scissors?style=flat-square)         |
| [chrisgrieser/nvim-spider](https://github.com/chrisgrieser/nvim-spider)                     | ![stars](https://img.shields.io/github/stars/chrisgrieser/nvim-spider?style=flat-square)           |
| [chrisgrieser/nvim-various-textobjs](https://github.com/chrisgrieser/nvim-various-textobjs) | ![stars](https://img.shields.io/github/stars/chrisgrieser/nvim-various-textobjs?style=flat-square) |
| [chrisgrieser/shimmering-focus](https://github.com/chrisgrieser/shimmering-focus)           | ![stars](https://img.shields.io/github/stars/chrisgrieser/shimmering-focus?style=flat-square)      |
| [chrisgrieser/shimmering-obsidian](https://github.com/chrisgrieser/shimmering-obsidian)     | ![stars](https://img.shields.io/github/stars/chrisgrieser/shimmering-obsidian?style=flat-square)   |
| [matrix-org/matrix.org](https://github.com/matrix-org/matrix.org)                           | ![stars](https://img.shields.io/github/stars/matrix-org/matrix.org?style=flat-square)              |
| [mikavilpas/yazi.nvim](https://github.com/mikavilpas/yazi.nvim)                             | ![stars](https://img.shields.io/github/stars/mikavilpas/yazi.nvim?style=flat-square)               |

*Using rumdl? [Let us know!](https://github.com/rvben/rumdl/issues/307)*

## Sponsors

rumdl is free and open source. If it saves you time, consider [sponsoring the project](https://github.com/sponsors/rvben).

<!-- sponsors -->
<!-- sponsors -->

## License

rumdl is licensed under the MIT License. See the [LICENSE](LICENSE) file for details.
