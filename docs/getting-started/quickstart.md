---
icon: lucide/play
---

# Quick Start

Get up and running with rumdl in minutes.

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
echo "# Hello  World" | rumdl fmt -

# Pipe through rumdl
cat README.md | rumdl fmt - > README.fixed.md
```

## Create a Configuration File

Initialize a default configuration:

```bash
# Create .rumdl.toml with defaults
rumdl init

# Create with specific preset
rumdl init --preset google
```

This creates a `.rumdl.toml` file in your current directory.

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
  - repo: https://github.com/rvben/rumdl
    rev: v0.0.222  # Use latest version
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
      - uses: actions/checkout@v4
      - uses: rvben/rumdl-action@main
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

| Code | Meaning |
|------|---------|
| `0` | Success (no issues, or `fmt` mode) |
| `1` | Violations found |
| `2` | Configuration or runtime error |

## Next Steps

- [CLI Commands](../usage/cli.md) - Full command reference
- [Rules Reference](../rules/index.md) - Explore all 66 rules
- [Configuration](../configuration/global-settings.md) - Advanced configuration options
