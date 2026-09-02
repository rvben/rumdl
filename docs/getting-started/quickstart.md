---
description: "Check a project, fix what can be fixed automatically, format through a pipe, and write your first config file, in a handful of commands."
icon: lucide/play
---

# Quick Start

Start with a read-only check on an existing repository. This command downloads
rumdl for the run, does not change files, and automatically discovers common
markdownlint configuration files:

```bash
uvx rumdl check .
```

Compare the result before changing your configuration, editor, or CI workflow.
If you already have rumdl installed, use `rumdl check .` instead.

## Basic Usage

### Check for issues

```bash
# Lint all Markdown files in current directory
rumdl check .

# Lint specific files
rumdl check README.md docs/

# Lint with verbose output
rumdl check --verbose .
```

### Fix issues automatically

```bash
# Auto-fix all issues (formatter mode - always exits 0)
rumdl fmt .

# Auto-fix with violation reporting (exits 1 if unfixable issues remain)
rumdl check --fix .
```

### Stdin/stdout formatting

```bash
# Format from stdin
echo "# Hello  World" | rumdl fmt --silent -

# Pipe through rumdl
cat README.md | rumdl fmt --silent - > README.fixed.md
```

## Create a Configuration File

Initialize a default configuration:

```bash
# Create .rumdl.toml with defaults
rumdl init

# Create with specific preset
rumdl init --preset google
```

This creates a `.rumdl.toml` file in your current directory. rumdl can also read
`rumdl.toml`, `.config/rumdl.toml`, or a `[tool.rumdl]` section in `pyproject.toml` -
see [Configuration Files](../configuration/index.md) for the full list and how rumdl
picks one.

## Example Configuration

```toml title=".rumdl.toml"
[global]
# Exclude files/directories
exclude = ["node_modules", "vendor", ".git"]

# Set line length limit
line-length = 120

[MD013]  # Line length
line_length = 120
code_blocks = false  # Don't check code blocks

[MD033]  # No inline HTML
allowed_elements = ["br", "details", "summary"]

[MD041]  # First line heading
enabled = false  # Disable this rule
```

## Common Workflows

### Editor Integration

For real-time linting in VS Code:

```bash
rumdl vscode
```

### Pre-commit Hook

Add to `.pre-commit-config.yaml`:

```yaml
repos:
  - repo: https://github.com/rvben/rumdl-pre-commit
    rev: v0.2.63  # Use latest version
    hooks:
      - id: rumdl
```

### CI/CD Pipeline

```yaml title=".github/workflows/lint.yml"
name: Lint Markdown
on: [push, pull_request]

jobs:
  lint:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v6
      - uses: rvben/rumdl@v0
```

## Understanding Output

rumdl outputs issues in a clear format:

```text
README.md:10:1: MD022 Headings should be surrounded by blank lines [heading-blank-lines]
README.md:15:81: MD013 Line length [Expected: 80; Actual: 95] [line-length]
docs/guide.md:5:1: MD041 First line in a file should be a top-level heading [first-line-heading]

Found 3 issues in 2 files
```

Each line shows:

- **File path** and **line:column**
- **Rule ID** (e.g., MD022)
- **Description** of the issue
- **Rule alias** in brackets

## Exit Codes

| Code | Meaning                            |
| ---- | ---------------------------------- |
| `0`  | Success (no issues, or `fmt` mode) |
| `1`  | Violations found                   |
| `2`  | Configuration or runtime error     |

## Next Steps

- [CLI Commands](../usage/cli.md) - Full command reference
- [Rules Reference](../rules.md) - Explore all <!-- RULE_COUNT -->83<!-- /RULE_COUNT --> rules
- [Configuration](../global-settings.md) - Advanced configuration options
