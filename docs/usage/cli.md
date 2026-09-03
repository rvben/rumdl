---
description: "Every rumdl command and flag, including which stream findings are written to, what each exit code means, and the machine readable formats."
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

| Option                       | Description                                               |
| ---------------------------- | --------------------------------------------------------- |
| `--fix`                      | Auto-fix issues (exits 1 if unfixable issues remain)      |
| `--config <PATH>`            | Path to configuration file                                |
| `--disable <RULES>`          | Disable specific rules (e.g., `MD013,MD033`), or `all`    |
| `--enable <RULES>`           | Enable only specific rules                                |
| `--exclude <PATTERNS>`       | Exclude files matching patterns                           |
| `--include <PATTERNS>`       | Include only files matching patterns                      |
| `--no-code-block-tools`      | Skip configured tools; keep checking the outer Markdown   |
| `--only-code-block-tools`    | Run configured tools; skip the outer Markdown             |
| `--stdin-batch`              | Read NUL-delimited path/content pairs from stdin          |
| `--stdin-batch-closed-world` | Resolve batch links only within the supplied document set |
| `--watch`                    | Watch for changes and re-lint                             |
| `--verbose`                  | Show detailed output                                      |
| `--quiet`                    | Print diagnostics, but suppress summaries                 |
| `--silent`                   | Suppress diagnostics and summaries                        |
| `--no-exclude`               | Disable exclude patterns defined in config                |
| `--stderr`                   | Write diagnostics to stderr instead of stdout             |
| `--deny-config-warnings`     | Treat configuration warnings as errors (exit code 2)      |

Findings go to stdout, whether the document came from a path or from `--stdin`,
so `--output-format json` redirects the same way in both. `--stderr` moves them;
config warnings and errors are always on stderr. The exception is a document
rewritten on stdout - `check --fix --stdin` and `fmt --stdin` - where stdout
belongs to the document and diagnostics go to stderr.

The closing summary is written for a person, so a machine-readable format never
carries one and needs no `--quiet` to keep its output parseable.

#### Batch stdin

`--stdin-batch` checks many caller-supplied snapshots in one process. The byte
stream is a repeated `path NUL content NUL` sequence and must end in NUL:

```text
path\0content\0path\0content\0
```

Paths and contents must be UTF-8, paths must be non-empty and unique after path
normalization, and content may be empty. Each path is both the diagnostic name
and the filesystem context used for configuration and relative links.

By default, supplied documents take precedence and links to documents omitted
from the batch fall back to the on-disk workspace. Add
`--stdin-batch-closed-world` to prohibit that fallback. Batch input never reads
or writes the persistent workspace-index cache, because supplied content may
differ from the file saved at the same path.

Batch mode is check-only and cannot be combined with paths, `--stdin`,
`--stdin-filename`, `--fix`, `--diff`, `--check`, or `--watch`.

### `fmt [PATHS...]`

Format Markdown files (applies fixes like `rumdl check --fix`, but keeps formatter-style exit codes).

```bash
rumdl fmt .                      # Format all files
rumdl fmt README.md              # Format specific file
rumdl fmt --silent -             # Format stdin to stdout without diagnostics
```

**Options:**

| Option                    | Description                                                 |
| ------------------------- | ----------------------------------------------------------- |
| `--config <PATH>`         | Path to configuration file                                  |
| `--diff`                  | Show a diff of what would change instead of rewriting files |
| `--check`                 | Exit 1 if formatting changes would be needed                |
| `--stdin`                 | Read from stdin                                             |
| `--stdin-filename <NAME>` | Filename for stdin (for error messages)                     |
| `--no-code-block-tools`   | Skip configured tools; format the outer Markdown            |
| `--only-code-block-tools` | Format configured fenced blocks only                        |
| `--output-format <FMT>`   | Output format for any remaining diagnostics                 |
| `--watch`                 | Re-run formatting when files change                         |
| `--quiet`                 | Print diagnostics, but suppress summaries                   |
| `--silent`                | Suppress diagnostics and summaries                          |
| `--deny-config-warnings`  | Treat configuration warnings as errors (exit code 2)        |

Use `--silent` whenever stdout should contain only formatted Markdown. Plain `rumdl fmt -` may also emit remaining diagnostics.

The code-block-tool mode flags are mutually exclusive and cannot be combined
with `--stdin`, `--stdin-batch`, or the `-` stdin path.
`--only-code-block-tools` drops the outer document's rules and leaves the tool
phases as they are, so `fmt` still runs configured lint tools and reports what a
formatter could not fix.

### `init [OPTIONS]`

Create a configuration file.

```bash
rumdl init                       # Create .rumdl.toml
rumdl init --preset google       # Use Google style preset
rumdl init --output custom.toml  # Custom output path
```

**Options:**

| Option            | Description                                         |
| ----------------- | --------------------------------------------------- |
| `--pyproject`     | Generate configuration for pyproject.toml           |
| `--preset <NAME>` | Use a style preset (`default`, `google`, `relaxed`) |
| `--output <PATH>` | Output file path (default: `.rumdl.toml`)           |

### `import <FILE>`

Import configuration from markdownlint.

```bash
rumdl import .markdownlint.json      # Import from markdownlint config
rumdl import .markdownlint.jsonc     # JSONC comments are supported
rumdl import .markdownlint.yaml      # YAML also works
rumdl import --dry-run .markdownlint.json
rumdl import --format json .markdownlint.yaml --output rumdl-config.json
```

**Options:**

| Option            | Description                               |
| ----------------- | ----------------------------------------- |
| `--dry-run`       | Show the converted config without writing |
| `--format <FMT>`  | Output format: `toml` or `json`           |
| `--output <PATH>` | Output file path (default: `.rumdl.toml`) |

### `rule [<RULE>]`

Show rule documentation.

```bash
rumdl rule                       # List all rules
rumdl rule MD013                 # Show details for specific rule
rumdl rule line-length           # Use rule alias
rumdl rule --list-categories     # Discover rule categories
rumdl rule MD013 --output-format json
rumdl rule MD013 --output-format json --explain
```

**Options:**

| Option                  | Description                                      |
| ----------------------- | ------------------------------------------------ |
| `--list-categories`     | List rule categories and exit                    |
| `--category <NAME>`     | Filter listed rules by category                  |
| `--fixable`             | Show only fixable rules                          |
| `--output-format <FMT>` | Structured output such as `json` or `json-lines` |
| `--explain`             | Include full documentation in JSON-based output  |

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
rumdl vscode --update            # Update the installed extension
rumdl vscode --force             # Force reinstall
```

### `version`

Show version information.

```bash
rumdl --version                  # Short version
rumdl version                    # Detailed version info
```

## Global Options

These options are commonly used with `check` and `fmt`:

| Option                  | Description                                               |
| ----------------------- | --------------------------------------------------------- |
| `--help`, `-h`          | Show help                                                 |
| `--version`, `-V`       | Show version                                              |
| `--verbose`, `-v`       | Verbose output                                            |
| `--quiet`, `-q`         | Print diagnostics, but suppress summaries                 |
| `--color <WHEN>`        | Color output (`auto`, `always`, `never`)                  |
| `--no-config`           | Ignore discovered configuration and use built-in defaults |
| `--output-format <FMT>` | Output format (see [Output Formats](#output-formats))     |

## Exit Codes

| Code | Meaning                        |
| ---- | ------------------------------ |
| `0`  | Success                        |
| `1`  | Lint violations found          |
| `2`  | Configuration or runtime error |

!!! note "fmt vs check --fix"
    - `rumdl fmt` always exits 0 (formatter mode)
    - `rumdl check --fix` exits 1 if unfixable issues remain

!!! note "Failing on configuration problems"
    Configuration problems (an unknown rule or option in a config file or a CLI
    flag, an unknown rule in an inline `rumdl-disable-line` comment, a shadowed
    config file, a subdirectory config that could not be loaded, an
    `.editorconfig` property rumdl cannot apply, or a run in which every Markdown
    file found was filtered out) are non-fatal warnings by default and do not
    affect the exit code. Pass `--deny-config-warnings` to make any of them exit
    with code `2`, so CI catches a typo'd rule name. This
    is distinct from `--fail-on`, which governs the severity of Markdown
    violations (exit `1`); a config problem exits `2` and takes precedence over
    Markdown violations.

!!! note "When nothing gets checked"
    Checking zero files and checking every file cleanly both exit `0` with no
    findings, so rumdl reports which one happened on stderr. A directory holding
    no Markdown says so plainly; a run whose files were all filtered out instead
    reports how many were found and which setting removed them:

    ```text
    No markdown files left to check: 12 files found were filtered out.
      12 by ignore files (.gitignore, .ignore, .markdownlintignore); pass --respect-gitignore=false to keep them
    ```

    The notice never shares a stream with the selected output, so it stays out
    of `--output-format json` and the other machine-readable formats: it goes to
    stderr, or to stdout when `--stderr` routes diagnostics the other way. It
    survives `--quiet`; use `--silent` to suppress it, or
    `--deny-config-warnings` to fail the run instead.

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

# Disable every rule; an --enable list still survives
rumdl check --disable all .
```

### Code block tools

[Code block tools](../code-block-tools.md) run external linters and formatters
over fenced code blocks. They are configured separately from the Markdown rules,
so either side can be run without the other while the rest of the configuration
keeps applying:

```bash
# Markdown rules only, configured tools skipped
rumdl check --no-code-block-tools .
rumdl fmt --no-code-block-tools .

# Configured tools only, outer Markdown left alone
rumdl check --only-code-block-tools .
rumdl fmt --only-code-block-tools .
```

Reach for `--only-code-block-tools` rather than `--disable all` here. The latter
empties the rule set that fenced Markdown configured with `lint = ["rumdl"]` is
linted with, so the built-in tool reports nothing while external tools carry on.

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
cat README.md | rumdl fmt --silent -

# With filename context
cat README.md | rumdl check - --stdin-filename README.md

# Format clipboard (macOS)
pbpaste | rumdl fmt --silent - | pbcopy
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

See [Output Formats](../output-formats.md) for the field-level reference for each
machine-readable format.

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
