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
      - id: rumdl      # Lint only
      - id: rumdl-fmt  # Auto-format
```

Then install the hooks:

```bash
pre-commit install
```

## Available Hooks

### `rumdl`

Lints files and fails if any issues are found.

```yaml
- id: rumdl
```

### `rumdl-fmt`

Auto-formats files and fails if unfixable issues remain.

```yaml
- id: rumdl-fmt
```

!!! tip "Recommended for CI"
    Use `rumdl-fmt` for the best developer experience - it auto-fixes what it can and only fails when manual intervention is needed.

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

### Force Exclude

By default, pre-commit passes files explicitly, which bypasses exclude patterns in your config. To enforce excludes:

=== "In config file"

    ```toml title=".rumdl.toml"
    [global]
    exclude = ["generated/*.md", "vendor/**"]
    force_exclude = true
    ```

=== "In pre-commit"

    ```yaml
    hooks:
      - id: rumdl
        args: [--force-exclude]
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
      - id: rumdl-fmt  # Run after other hooks
```
