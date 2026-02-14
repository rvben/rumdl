---
icon: lucide/terminal
---

# CLI Commands

Complete reference for rumdl command-line interface.

## Commands

### `check [PATHS...]`

Lint Markdown files and report issues.

```bash
rumdl check .                    # Lint current directory
rumdl check README.md docs/      # Lint specific files/directories
rumdl check --fix .              # Lint and auto-fix issues
```

**Options:**

| Option                 | Description                                          |
| ---------------------- | ---------------------------------------------------- |
| `--fix`                | Auto-fix issues (exits 1 if unfixable issues remain) |
| `--config <PATH>`      | Path to configuration file                           |
| `--disable <RULES>`    | Disable specific rules (e.g., `MD013,MD033`)         |
| `--enable <RULES>`     | Enable only specific rules                           |
| `--exclude <PATTERNS>` | Exclude files matching patterns                      |
| `--include <PATTERNS>` | Include only files matching patterns                 |
| `--watch`              | Watch for changes and re-lint                        |
| `--verbose`            | Show detailed output                                 |
| `--quiet`              | Suppress output except errors                        |
| `--no-exclude`         | Disable exclude patterns defined in config           |

### `fmt [PATHS...]`

Format Markdown files (always exits 0).

```bash
rumdl fmt .                      # Format all files
rumdl fmt README.md              # Format specific file
rumdl fmt -                      # Format stdin to stdout
```

**Options:**

| Option                    | Description                             |
| ------------------------- | --------------------------------------- |
| `--config <PATH>`         | Path to configuration file              |
| `--stdin`                 | Read from stdin                         |
| `--stdin-filename <NAME>` | Filename for stdin (for error messages) |
| `--quiet`                 | Suppress diagnostic output              |

### `init [OPTIONS]`

Create a configuration file.

```bash
rumdl init                       # Create .rumdl.toml
rumdl init --preset google       # Use Google style preset
rumdl init --output custom.toml  # Custom output path
```

**Options:**

| Option            | Description                               |
| ----------------- | ----------------------------------------- |
| `--preset <NAME>` | Use a style preset (`google`, `default`)  |
| `--output <PATH>` | Output file path (default: `.rumdl.toml`) |

### `import <FILE>`

Import configuration from markdownlint.

```bash
rumdl import .markdownlint.json     # Import from markdownlint config
rumdl import .markdownlint.yaml     # Supports JSON and YAML
```

### `rule [<RULE>]`

Show rule documentation.

```bash
rumdl rule                       # List all rules
rumdl rule MD013                 # Show details for specific rule
rumdl rule line-length           # Use rule alias
```

### `config [OPTIONS]`

Show effective configuration.

```bash
rumdl config                     # Show merged configuration
rumdl config --defaults          # Show default values only
rumdl config --no-defaults       # Show non-default values only
```

### `server`

Start the LSP server.

```bash
rumdl server                     # Start Language Server Protocol server
```

See [LSP Integration](../lsp.md) for details.

### `vscode`

Install VS Code extension.

```bash
rumdl vscode                     # Install extension
rumdl vscode --status            # Check installation
rumdl vscode --force             # Force reinstall
```

### `version`

Show version information.

```bash
rumdl --version                  # Short version
rumdl version                    # Detailed version info
```

## Global Options

These options work with all commands:

| Option                  | Description                                            |
| ----------------------- | ------------------------------------------------------ |
| `--help`, `-h`          | Show help                                              |
| `--version`, `-V`       | Show version                                           |
| `--verbose`, `-v`       | Verbose output                                         |
| `--quiet`, `-q`         | Quiet output                                           |
| `--color <WHEN>`        | Color output (`auto`, `always`, `never`)               |
| `--output-format <FMT>` | Output format (see [Output Formats](#output-formats))  |

## Exit Codes

| Code | Meaning                        |
| ---- | ------------------------------ |
| `0`  | Success                        |
| `1`  | Lint violations found          |
| `2`  | Configuration or runtime error |

!!! note "fmt vs check --fix"
    - `rumdl fmt` always exits 0 (formatter mode)
    - `rumdl check --fix` exits 1 if unfixable issues remain

## Usage Examples

### Basic Linting

```bash
# Lint all Markdown files
rumdl check .

# Lint specific directory
rumdl check docs/

# Lint with custom config
rumdl check --config my-config.toml .
```

### Selective Rules

```bash
# Disable specific rules
rumdl check --disable MD013,MD033 .

# Enable only specific rules
rumdl check --enable MD001,MD003 .
```

### File Filtering

```bash
# Exclude directories
rumdl check --exclude "node_modules,dist" .

# Include only specific patterns
rumdl check --include "docs/**/*.md" .

# Combine patterns
rumdl check --include "docs/**/*.md" --exclude "docs/drafts" .
```

### Watch Mode

```bash
# Watch for changes
rumdl check --watch docs/
```

### Stdin/Stdout

```bash
# Format from stdin
cat README.md | rumdl fmt -

# With filename context
cat README.md | rumdl check - --stdin-filename README.md

# Format clipboard (macOS)
pbpaste | rumdl fmt - | pbcopy
```

### Output Formats

Control how warnings are displayed with `--output-format`:

```bash
rumdl check --output-format full .
rumdl check --output-format json .
RUMDL_OUTPUT_FORMAT=github rumdl check .
```

**Human-readable formats:**

| Format    | Description                                                     |
| --------- | --------------------------------------------------------------- |
| `text`    | One line per warning: `file:line:col: [RULE] message` (default) |
| `full`    | Source lines with caret underlines highlighting the violation   |
| `concise` | Minimal: `file:line:col rule message`                           |
| `grouped` | Warnings grouped by file with a header per file                 |

**Machine-readable formats:**

| Format       | Description                             |
| ------------ | --------------------------------------- |
| `json`       | JSON array of all warnings (collected)  |
| `json-lines` | One JSON object per warning (streaming) |
| `sarif`      | SARIF 2.1.0 for static analysis tools   |
| `junit`      | JUnit XML for CI test reporters         |

**CI/CD formats:**

| Format   | Description                                        |
| -------- | -------------------------------------------------- |
| `github` | GitHub Actions annotations (`::warning`/`::error`) |
| `gitlab` | GitLab Code Quality report (JSON)                  |
| `azure`  | Azure Pipelines logging commands                   |
| `pylint` | Pylint-compatible format                           |

**Example: `full` format output:**

```text
MD013 Line length 95 exceeds 80 characters
 --> README.md:42:81
   |
42 | This is a long line that exceeds the configured maximum line length ...
   |                                                                     ^^^
   |
```

**Example: `text` format output (default):**

```text
README.md:42:81: [MD013] Line length 95 exceeds 80 characters
```
