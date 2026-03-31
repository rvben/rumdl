---
icon: lucide/git-commit
---

# Pre-commit Integration

Use rumdl as a pre-commit hook to catch issues before they're committed.

## Setup

Add to your `.pre-commit-config.yaml`:

```yaml title=".pre-commit-config.yaml"
repos:
  - repo: https://github.com/rvben/rumdl-pre-commit
    rev: v0.0.222  # Use latest version
    hooks:
      - id: rumdl      # Lint + auto-fix, fails if unfixable issues remain
      - id: rumdl-fmt  # Pure format, always exits 0
```

Then install the hooks:

```bash
pre-commit install
```

## Available Hooks

### `rumdl`

Lints and auto-fixes files. Exits 1 if unfixable violations remain. Use this as your primary hook.

```yaml
- id: rumdl
```

### `rumdl-fmt`

Formats files in place and always exits 0. Relies on pre-commit's file-change detection to signal failures. Use alongside `rumdl` when you want to separate formatting from linting.

```yaml
- id: rumdl-fmt
```

!!! tip "Recommended setup"
    Use `rumdl` first for lint coverage, then `rumdl-fmt` for formatting — the same pattern as `ruff` + `ruff-format`.

## Configuration

### Custom Arguments

```yaml
hooks:
  - id: rumdl
    args: [--config, .rumdl.toml, --verbose]
```

### File Filtering

```yaml
hooks:
  - id: rumdl
    files: ^docs/.*\.md$  # Only lint docs/
    exclude: ^docs/drafts/
```

### No Exclude

Exclude patterns from your config are always respected by default (as of v0.0.156).

To disable all configured exclusions, use `--no-exclude` flag.

```yaml
hooks:
  - id: rumdl
    args: [--no-exclude]  # Disable exclude patterns defined in config
```

## Stages

Run hooks at different stages:

```yaml
hooks:
  - id: rumdl
    stages: [commit]  # Default

  - id: rumdl
    stages: [push]    # Run on push instead
```

## Running Manually

```bash
# Run on all files
pre-commit run rumdl --all-files

# Run on staged files only
pre-commit run rumdl
```

## Updating

```bash
# Update to latest version
pre-commit autoupdate --repo https://github.com/rvben/rumdl-pre-commit
```

## Troubleshooting

### Slow First Run

The first run downloads and installs rumdl. Subsequent runs use the cached version.

### Files Not Being Checked

Check your `files` pattern matches your Markdown files:

```yaml
hooks:
  - id: rumdl
    types: [markdown]  # Use file type instead of pattern
```

### Conflicts with Other Formatters

Run rumdl last to ensure consistent formatting:

```yaml
repos:
  - repo: https://github.com/pre-commit/pre-commit-hooks
    hooks:
      - id: trailing-whitespace

  - repo: https://github.com/rvben/rumdl-pre-commit
    hooks:
      - id: rumdl      # Run after other hooks
      - id: rumdl-fmt
```
