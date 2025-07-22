# Global Settings Reference

This document provides a comprehensive reference for rumdl's global configuration settings. Global settings control rumdl's overall behavior and apply to all rules and operations.

## Overview

Global settings are configured in the `[global]` section of your configuration file (`.rumdl.toml` or
`pyproject.toml`). These settings control file selection, rule enablement, and general linting behavior.

## Quick Reference

| Setting | Type | Default | Description |
|---------|------|---------|-------------|
| [`enable`](#enable) | `string[]` | `[]` | Enable only specific rules |
| [`disable`](#disable) | `string[]` | `[]` | Disable specific rules |
| [`exclude`](#exclude) | `string[]` | `[]` | Files/directories to exclude |
| [`include`](#include) | `string[]` | `[]` | Files/directories to include |
| [`respect_gitignore`](#respect_gitignore) | `boolean` | `true` | Respect .gitignore files |
| [`line_length`](#line_length) | `integer` | `80` | Default line length for rules |

## Configuration Examples

### TOML Configuration (`.rumdl.toml`)

```toml
[global]
# Enable only specific rules
enable = ["MD001", "MD003", "MD013"]

# Disable problematic rules
disable = ["MD013", "MD033"]

# Exclude files and directories
exclude = [
    "node_modules",
    "build",
    "dist",
    "*.tmp.md",
    "docs/generated/**"
]

# Include only specific files
include = [
    "README.md",
    "docs/**/*.md",
    "**/*.markdown"
]

# Don't respect .gitignore files
respect_gitignore = false

# Set global line length (used by MD013 and other line-length rules)
line_length = 120
```

### pyproject.toml Configuration

```toml
[tool.rumdl]
# Global options at root level (both snake_case and kebab-case supported)
enable = ["MD001", "MD003", "MD013"]
disable = ["MD013", "MD033"]
exclude = ["node_modules", "build", "dist"]
include = ["docs/*.md", "README.md"]
respect_gitignore = true
line_length = 120
```

## Detailed Settings Reference

### `enable`

**Type**: `string[]`
**Default**: `[]` (all rules enabled)
**CLI Equivalent**: `--enable`

Enables only the specified rules. When this option is set, all other rules are disabled except those explicitly listed.

```toml
[global]
enable = ["MD001", "MD003", "MD013", "MD022"]
```

**Usage Notes**:
- Rule IDs are case-insensitive but conventionally uppercase (e.g., "MD001")
- If both `enable` and `disable` are specified, `enable` takes precedence
- An empty array means all rules are enabled (default behavior)
- Useful for gradually adopting rumdl or focusing on specific rule categories

**Example CLI usage**:

```bash
rumdl check --enable MD001,MD003,MD013 .
```

### `disable`

**Type**: `string[]`
**Default**: `[]` (no rules disabled)
**CLI Equivalent**: `--disable`

Disables the specified rules. All other rules remain enabled.

```toml
[global]
disable = ["MD013", "MD033", "MD041"]
```

**Usage Notes**:
- Rule IDs are case-insensitive but conventionally uppercase
- Commonly disabled rules include:
  - `MD013`: Line length (for projects with longer lines)
  - `MD033`: Inline HTML (for projects that use HTML in Markdown)
  - `MD041`: First line heading (for files that don't start with headings)
- Can be combined with rule-specific configuration to fine-tune behavior

**Example CLI usage**:

```bash
rumdl check --disable MD013,MD033 .
```

### `exclude`

**Type**: `string[]`
**Default**: `[]` (no files excluded)
**CLI Equivalent**: `--exclude`

Specifies files and directories to exclude from linting. Supports glob patterns.

```toml
[global]
exclude = [
    "node_modules",           # Exclude entire directory
    "build/**",              # Exclude directory and all subdirectories
    "*.tmp.md",              # Exclude files with specific pattern
    "docs/generated/**",     # Exclude generated documentation
    ".git",                  # Exclude version control directory
    "vendor/",               # Exclude third-party code
]
```

**Supported Patterns**:
- `directory/` - Exclude entire directory
- `**/*.ext` - Exclude all files with extension in any subdirectory
- `*.pattern` - Exclude files matching pattern in current directory
- `path/**/file` - Exclude specific files in any subdirectory of path

**Usage Notes**:
- Patterns are relative to the project root
- Exclude patterns are processed before include patterns
- More specific patterns take precedence over general ones
- Useful for excluding generated files, dependencies, and temporary files

**Example CLI usage**:

```bash
rumdl check --exclude "node_modules,build,*.tmp.md" .
```

### `include`

**Type**: `string[]`
**Default**: `[]` (all Markdown files included)
**CLI Equivalent**: `--include`

Specifies files and directories to include in linting. When set, only matching files are processed.

```toml
[global]
include = [
    "README.md",             # Include specific file
    "docs/**/*.md",          # Include all .md files in docs/
    "**/*.markdown",         # Include all .markdown files
    "CHANGELOG.md",          # Include specific files
    "src/**/*.md",           # Include documentation in source
]
```

**Usage Notes**:
- If `include` is empty, all Markdown files are included (subject to exclude patterns)
- When `include` is specified, only matching files are processed
- Combine with `exclude` for fine-grained control
- Useful for limiting linting to specific documentation areas

**Example CLI usage**:

```bash
rumdl check --include "docs/**/*.md,README.md" .
```

### `respect_gitignore`

**Type**: `boolean`
**Default**: `true`
**CLI Equivalent**: `--respect-gitignore` / `--respect-gitignore=false`

Controls whether rumdl respects `.gitignore` files when scanning for Markdown files.

```toml
[global]
respect_gitignore = true   # Default: respect .gitignore
# or
respect_gitignore = false  # Ignore .gitignore files
```

**Behavior**:
- `true` (default): Files and directories listed in `.gitignore` are automatically excluded
- `false`: `.gitignore` files are ignored, all Markdown files are considered

**Usage Notes**:
- This setting only affects directory scanning, not explicitly provided file paths
- Useful for linting files that are normally ignored (e.g., generated docs)
- When disabled, you may need more specific `exclude` patterns
- Respects `.gitignore` files at any level in the directory tree

**Example CLI usage**:

```bash
# Don't respect .gitignore files
rumdl check --respect-gitignore=false .
```

### `line_length`

**Type**: `integer`
**Default**: `80`
**CLI Equivalent**: None (rule-specific only)

Sets the global default line length used by rules that check line length (primarily MD013). Rules can override this value with their own configuration.

```toml
[global]
line_length = 120  # Set global line length to 120 characters
```

**Behavior**:
- Used as the default line length for MD013 and other line-length-related rules
- Rule-specific configurations override the global setting
- Useful for projects that want a consistent line length across all line-length rules

**Usage Notes**:
- Must be a positive integer
- Common values: 80 (traditional), 100 (relaxed), 120 (modern)
- Individual rules can still override this setting in their own configuration
- When importing from markdownlint configs, top-level `line-length` is mapped to this setting

**Example with rule override**:

```toml
[global]
line_length = 100  # Global default

[MD013]
line_length = 120  # MD013 uses 120, overriding global setting
```

## Configuration Precedence

Settings are applied in the following order (later sources override earlier ones):

1. **Built-in defaults**
2. **Configuration file** (`.rumdl.toml` or `pyproject.toml`)
3. **Command-line arguments**

### Example: Precedence in Action

Given this configuration file:

```toml
[global]
disable = ["MD013", "MD033"]
exclude = ["node_modules", "build"]
```

And this command:

```bash
rumdl check --disable MD001,MD013 --exclude "temp/**" docs/
```

The final configuration will be:
- `disable`: `["MD001", "MD013"]` (CLI overrides file)
- `exclude`: `["temp/**"]` (CLI overrides file)
- Paths: `["docs/"]` (CLI argument)

## File Selection Logic

rumdl processes files using the following logic:

1. **Start with candidate files**:

- If paths are provided via CLI: use those files/directories
- Otherwise: recursively scan current directory for `.md` and `.markdown` files

2. **Apply .gitignore filtering** (if `respect_gitignore = true`):

- Skip files/directories listed in `.gitignore` files

3. **Apply include patterns** (if specified):

- Keep only files matching at least one include pattern

4. **Apply exclude patterns**:

- Remove files matching any exclude pattern

5. **Apply rule filtering**:

- Process remaining files with enabled rules only

### Example: File Selection

Given this configuration:

```toml
[global]
include = ["docs/**/*.md", "README.md"]
exclude = ["docs/temp/**", "*.draft.md"]
respect_gitignore = true
```

File selection process:
1. Start with: `docs/guide.md`, `docs/temp/test.md`, `README.md`, `notes.draft.md`
2. Apply includes: `docs/guide.md`, `docs/temp/test.md`, `README.md` (notes.draft.md excluded)
3. Apply excludes: `docs/guide.md`, `README.md` (docs/temp/test.md excluded)
4. Final files: `docs/guide.md`, `README.md`

## Common Configuration Patterns

### Strict Documentation Projects

For projects with strict documentation standards:

```toml
[global]
# Enable only essential rules
enable = [
    "MD001",  # Heading increment
    "MD003",  # Heading style
    "MD022",  # Blanks around headings
    "MD025",  # Single title
    "MD032",  # Blanks around lists
]

# Focus on documentation directories
include = [
    "README.md",
    "CHANGELOG.md",
    "docs/**/*.md",
    "guides/**/*.md",
]

# Set consistent line length for documentation
line_length = 100

# Exclude generated or temporary content
exclude = [
    "node_modules/**",
    "build/**",
    "docs/api/**",      # Generated API docs
    "*.draft.md",       # Draft documents
]
```

### Legacy Project Migration

For gradually adopting rumdl in existing projects:

```toml
[global]
# Start with basic rules only
enable = [
    "MD001",  # Heading increment
    "MD022",  # Blanks around headings
    "MD025",  # Single title
]

# Exclude problematic areas initially
exclude = [
    "legacy-docs/**",
    "third-party/**",
    "vendor/**",
]

# Process only main documentation
include = [
    "README.md",
    "docs/user-guide/**/*.md",
]
```

### Open Source Projects

For open source projects with community contributions:

```toml
[global]
# Disable rules that might be too strict for contributors
disable = [
    "MD013",  # Line length (can be restrictive)
    "MD033",  # Inline HTML (often needed for badges/formatting)
    "MD041",  # First line heading (README might start with badges)
]

# Include all documentation but exclude dependencies
include = ["**/*.md"]
exclude = [
    "node_modules/**",
    "vendor/**",
    ".git/**",
    "coverage/**",
]
```

### Development Workflow

For active development with frequent documentation changes:

```toml
[global]
# Enable rules that help maintain consistency
enable = [
    "MD003",  # Heading style
    "MD004",  # List style
    "MD022",  # Blanks around headings
    "MD032",  # Blanks around lists
    "MD047",  # Single trailing newline
]

# Include work-in-progress docs but exclude temp files
include = ["docs/**/*.md", "*.md"]
exclude = [
    "*.tmp.md",
    "*.draft.md",
    ".backup/**",
    "node_modules/**",
]

# Don't respect .gitignore to catch uncommitted docs
respect_gitignore = false
```

## Validation and Debugging

### View Effective Configuration

To see how your global settings are applied:

```bash
# View full effective configuration
rumdl config

# View only global settings
rumdl config get global

# View specific global setting
rumdl config get global.exclude
```

### Test File Selection

To see which files would be processed:

```bash
# Dry run with verbose output
rumdl check --verbose --dry-run .

# Or use a simple script to list files
find . -name "*.md" -o -name "*.markdown" | head -10
```

### Common Configuration Issues

1. **No files found**: Check your `include`/`exclude` patterns and `respect_gitignore` setting
2. **Too many files**: Add more specific `exclude` patterns or limit `include` patterns
3. **Rules not applying**: Verify rule names in `enable`/`disable` lists (case-insensitive but check spelling)
4. **Performance issues**: Exclude large directories like `node_modules`, `vendor`, or build outputs

## Integration with CI/CD

### GitHub Actions

```yaml
- name: Lint Markdown
  run: |
    rumdl check --output json . > lint-results.json
    rumdl check .  # Also show human-readable output
```

### Pre-commit Hook

```yaml
- repo: https://github.com/rvben/rumdl-pre-commit
  rev: v0.0.99
  hooks:
    - id: rumdl
      args: [--config=.rumdl.toml]
```

## See Also

- [Configuration Guide](../README.md#configuration) - Basic configuration setup
- [Rules Reference](RULES.md) - Complete list of available rules
- [CLI Reference](../README.md#command-line-interface) - Command-line options
- [Rule-specific Configuration](../README.md#configuration-file-example) - Configuring individual rules
