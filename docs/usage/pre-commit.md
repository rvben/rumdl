---
description: "Catch Markdown problems before they are committed, with a lint hook that fails on violations and a format hook that always exits zero."
icon: lucide/git-commit
---

# Pre-commit Integration

Use the `rumdl` pre-commit hook for a read-only lint check and add `rumdl-fmt`
only when you want files formatted automatically. Both hooks use the same rumdl
configuration as the CLI and CI, so repositories can enforce one Markdown rule
set throughout the authoring loop.

> **Last verified: September 2026.** Hook names and exit behavior match the current
> rumdl pre-commit integration.

## Setup

Add to your `.pre-commit-config.yaml`:

```yaml title=".pre-commit-config.yaml"
repos:
  - repo: https://github.com/rvben/rumdl-pre-commit
    rev: v0.2.66  # Use latest version
    hooks:
      - id: rumdl      # Lint only; add args [--fix] to auto-fix
      - id: rumdl-fmt  # Pure format, always exits 0
```

Then install the hooks:

```bash
pre-commit install
```

## Available Hooks

### `rumdl`

Lints files and exits 1 if violations are found. Non-destructive by default. Use this as your primary hook.

```yaml
- id: rumdl
```

To auto-fix violations in place, opt in with `args` (the same model as ruff's linter hook):

```yaml
- id: rumdl
  args: [--fix]
```

### `rumdl-fmt`

Formats files in place and always exits 0. Relies on pre-commit's file-change detection to signal failures. Use alongside `rumdl` when you want to separate formatting from linting.

```yaml
- id: rumdl-fmt
```

!!! tip "Recommended setup"
    Use `rumdl` first for lint coverage, then `rumdl-fmt` for formatting - the same pattern as `ruff` + `ruff-format`.

## Configuration

### Custom Arguments

```yaml
hooks:
  - id: rumdl
    args: [--config, .rumdl.toml, --verbose]
```

### Code-block tools

Both hooks accept the [code-block tool](../code-block-tools.md) mode flags through `args`. To check the outer Markdown and skip the configured tools:

```yaml
hooks:
  - id: rumdl
    args: [--no-code-block-tools]
  - id: rumdl-fmt
    args: [--no-code-block-tools]
```

To run only the configured tools and leave the outer Markdown alone:

```yaml
hooks:
  - id: rumdl
    args: [--only-code-block-tools, --deny-config-warnings]
  - id: rumdl-fmt
    args: [--only-code-block-tools, --deny-config-warnings]
```

Only mode has two ways to pass while checking nothing, and they need different
guards.

With no tools configured it exits 0 with a config warning, which
`--deny-config-warnings` turns into a failure.

The second way is easy to miss under pre-commit specifically. The hook
environment installs rumdl and nothing else, so the tools it drives (`ruff`,
`shellcheck`, `shfmt`, `prettier`) have to come from the machine running the
hook, and on a CI runner they are often absent. The default
`on-missing-tool-binary = "ignore"` then skips them silently, with no warning
for `--deny-config-warnings` to catch, and the hook reports success. Set the
config to fail instead:

```toml title=".rumdl.toml"
[code-block-tools]
enabled = true
on-missing-tool-binary = "fail"
```

A missing binary is then reported as a violation (`Tool binary 'ruff' not found
in PATH`) and the hook fails, rather than quietly checking none of your code
blocks.

To keep the guard on the hook rather than on the whole project, pass the same
setting inline instead (rumdl 0.2.66 and later):

```yaml
hooks:
  - id: rumdl
    args:
      - --only-code-block-tools
      - --deny-config-warnings
      - --config
      - 'code-block-tools.on-missing-tool-binary = "fail"'
```

To run both modes as separate hooks, give each entry its own `alias` and `name`. pre-commit uses the alias for `pre-commit run <alias>` and the name in its output, and each entry can carry its own
`files`, `exclude` or `stages`:

```yaml
hooks:
  - id: rumdl
    alias: rumdl-no-code-block-tools
    name: rumdl check (no code-block tools)
    args: [--no-code-block-tools]
  - id: rumdl
    alias: rumdl-only-code-block-tools
    name: rumdl check (only code-block tools)
    args: [--only-code-block-tools, --deny-config-warnings]
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
