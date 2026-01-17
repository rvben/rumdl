# Global Settings Reference

This document provides a comprehensive reference for rumdl's global configuration settings. Global settings control rumdl's overall behavior and apply to all rules and operations.

## Overview

Global settings are configured in the `[global]` section of your configuration file (`.rumdl.toml` or
`pyproject.toml`). These settings control file selection, rule enablement, and general linting behavior.

## Quick Reference

| Setting                                   | Type       | Default        | Description                               |
| ----------------------------------------- | ---------- | -------------- | ----------------------------------------- |
| [`enable`](#enable)                       | `string[]` | `[]`           | Enable only specific rules                |
| [`disable`](#disable)                     | `string[]` | `[]`           | Disable specific rules                    |
| [`per-file-ignores`](#per-file-ignores)   | `table`    | `{}`           | Disable specific rules for specific files |
| [`exclude`](#exclude)                     | `string[]` | `[]`           | Files/directories to exclude              |
| [`include`](#include)                     | `string[]` | `[]`           | Files/directories to include              |
| [`respect_gitignore`](#respect_gitignore) | `boolean`  | `true`         | Respect .gitignore files                  |
| [`line_length`](#line_length)             | `integer`  | `80`           | Default line length for rules             |
| [`flavor`](#flavor)                       | `string`   | `"standard"`   | Markdown flavor to use                    |
| [`per-file-flavor`](#per-file-flavor)     | `table`    | `{}`           | Per-file flavor overrides                 |
| [`output-format`](#output-format)         | `string`   | `"text"`       | Output format for linting results         |
| [`cache`](#cache)                         | `boolean`  | `true`         | Enable result caching                     |
| [`cache_dir`](#cache_dir)                 | `string`   | `.rumdl_cache` | Directory for cache files                 |

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

# Set markdown flavor (standard, mkdocs)
flavor = "mkdocs"

# Per-file flavor overrides (pattern → flavor)
[per-file-flavor]
"**/*.mdx" = "mdx"
"notebooks/**/*.qmd" = "quarto"

# Disable specific rules for specific files
[per-file-ignores]
"README.md" = ["MD033"]  # Allow inline HTML in README
"docs/api/**/*.md" = ["MD013", "MD041"]  # Generated API docs
"SUMMARY.md" = ["MD025"]  # MkDocs/mdBook table of contents
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
flavor = "standard"

# Per-file rule ignores (both snake_case and kebab-case supported)
[tool.rumdl.per-file-ignores]
"README.md" = ["MD033"]
"SUMMARY.md" = ["MD025"]
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

### `per-file-ignores`

**Type**: `table` (file patterns mapped to rule arrays)
**Default**: `{}` (no per-file ignores)
**CLI Equivalent**: None (configuration file only)

Disables specific rules for specific files or file patterns. This is useful when certain files have different requirements than the rest of your documentation.

```toml
[per-file-ignores]
# Disable inline HTML check for GitHub README (often has badges)
"README.md" = ["MD033"]

# Disable line length and first-line heading for generated API docs
"docs/api/**/*.md" = ["MD013", "MD041"]

# Allow multiple top-level headings in table of contents files
"SUMMARY.md" = ["MD025"]
"**/TOC.md" = ["MD025"]

# Disable heading style check for legacy documentation
"docs/legacy/**/*.md" = ["MD003", "MD022"]
```

**Pattern Syntax**:

Patterns use standard glob syntax:

- `*` matches any characters except path separators
- `**` matches any characters including path separators (recursive)
- `?` matches a single character
- `{a,b}` matches either `a` or `b` (brace expansion)

To match multiple specific files, use **brace expansion**:

```toml
[per-file-ignores]
# Match both AGENTS.md and README.md
"{AGENTS.md,README.md}" = ["MD033"]

# Match multiple directories
"{docs,guides}/**/*.md" = ["MD013"]
```

Alternatively, use separate entries (more verbose but equivalent):

```toml
[per-file-ignores]
"AGENTS.md" = ["MD033"]
"README.md" = ["MD033"]
```

> **Note**: Commas are literal characters in glob patterns. The pattern `"A.md,B.md"` matches a file literally named `A.md,B.md`, not two separate files. Use `"{A.md,B.md}"` to match multiple files.

**Usage Notes**:

- Rule IDs are case-insensitive but conventionally uppercase
- More specific patterns take precedence over general ones
- Useful for handling special files like:
  - `README.md` files (often contain badges, HTML, custom formatting)
  - Generated documentation (API docs, changelogs)
  - Table of contents files (`SUMMARY.md`, `TOC.md`)
  - Legacy or third-party documentation
- Combines with global `enable`/`disable` settings

**Behavior**:

1. Global rules are applied first
2. Per-file ignores override global settings for matching files
3. If a file matches multiple patterns, all ignores are combined

**Example with precedence**:

```toml
[global]
disable = ["MD013"]  # Disable line length globally

[per-file-ignores]
"README.md" = ["MD033", "MD041"]  # Also disable HTML and first-line heading for README
"docs/**/*.md" = ["MD033"]  # Allow HTML in docs
```

Result:

- `README.md`: MD013, MD033, MD041 disabled
- `docs/guide.md`: MD013, MD033 disabled
- `other.md`: MD013 disabled

**Common Use Cases**:

1. **MkDocs/mdBook Projects**:

   ```toml
   [per-file-ignores]
   "SUMMARY.md" = ["MD025"]  # Table of contents has multiple H1 headings
   ```

2. **GitHub Projects with Badges**:

   ```toml
   [per-file-ignores]
   "README.md" = ["MD033", "MD041"]  # Allow HTML badges, may not start with heading
   ```

3. **Generated Documentation**:

   ```toml
   [per-file-ignores]
   "docs/api/**/*.md" = ["MD013", "MD024", "MD041"]  # Relax rules for generated files
   ```

4. **Mixed Documentation Sources**:

   ```toml
   [per-file-ignores]
   "vendor/**/*.md" = ["MD013", "MD033", "MD041"]  # Third-party docs
   "legacy/**/*.md" = ["MD003", "MD022", "MD032"]  # Old docs with different style
   ```

5. **Documentation Generators with HTML Links**:

   For documentation generators (mdBook, Jekyll, Hugo) that compile markdown to HTML and place sources in different locations:

   ```toml
   [per-file-ignores]
   # mdBook projects - HTML links in book/ point to book/src/*.md sources
   "book/**/*.md" = ["MD057"]

   # Jekyll projects - HTML links in _posts/ point to generated files
   "_posts/**/*.md" = ["MD057"]
   "_docs/**/*.md" = ["MD057"]

   # Hugo projects - HTML links in content/ point to generated files
   "content/**/*.md" = ["MD057"]
   ```

   See [MD057 documentation](md057.md#handling-complex-generator-patterns) for more details.

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

- `true` (default): Files and directories listed in ignore files are automatically excluded
- `false`: Ignore files are not considered, all Markdown files are scanned

**Supported ignore files**:

- `.gitignore` - Standard Git ignore patterns
- `.ignore` - Additional ignore patterns (used by ripgrep, fd, and other tools)

Both file types use the same gitignore pattern syntax and are respected at any level in the directory tree.

**Usage Notes**:

- This setting only affects directory scanning, not explicitly provided file paths
- Useful for linting files that are normally ignored (e.g., generated docs)
- When disabled, you may need more specific `exclude` patterns
- Use `.ignore` for rumdl-specific exclusions without affecting Git

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

### `flavor`

**Type**: `string`
**Default**: `"standard"`
**CLI Equivalent**: `--flavor`

Specifies the Markdown flavor to use for parsing and linting. Different flavors have different parsing rules and feature support.

```toml
[global]
flavor = "mkdocs"  # Use MkDocs flavor
```

**Available Flavors**:

- `"standard"` (default): [CommonMark 0.31.2](https://spec.commonmark.org/0.31.2/) + GFM extensions (tables, task lists, strikethrough, autolinks)
- `"gfm"`: GitHub Flavored Markdown with security-sensitive HTML warnings and extended autolinks
- `"mkdocs"`: MkDocs-specific extensions (admonitions, content tabs, autorefs, mkdocstrings)
- `"mdx"`: MDX with JSX components, attributes, expressions, and ESM imports
- `"quarto"`: Quarto/RMarkdown for scientific publishing (citations, shortcodes, div blocks)

**Aliases**: `"commonmark"` is an alias for `"standard"`, `"github"` is an alias for `"gfm"`

**Behavior**:

- The `standard` flavor is based on CommonMark 0.31.2 with widely-adopted GFM extensions enabled by default
- Each flavor adjusts specific rule behavior where that system differs from standard Markdown
- See [Flavors Overview](flavors.md) for detailed rule adjustments per flavor

**Usage Notes**:

- Choose the flavor that matches your documentation system
- Use `standard` for generic Markdown or when you want the strictest linting
- Use `gfm` for GitHub-hosted documentation with security-conscious HTML handling
- Use `mkdocs` for MkDocs or Material for MkDocs projects
- Use `mdx` for React/Next.js documentation with JSX components
- Use `quarto` for scientific documents with R/Python code execution

**Example CLI usage**:

```bash
# Use MkDocs flavor for linting
rumdl check --flavor mkdocs docs/
```

### `per-file-flavor`

**Type**: `table` (file patterns mapped to flavors)
**Default**: `{}` (no per-file overrides)
**CLI Equivalent**: None (configuration file only)

Specifies Markdown flavors for specific files or file patterns. This allows different parts of your project to use different Markdown dialects.

```toml
[per-file-flavor]
"docs/**/*.md" = "mkdocs"
"**/*.mdx" = "mdx"
"**/*.qmd" = "quarto"
"examples/**/*.md" = "standard"
```

**Available Flavors**:

- `"standard"` (default): Standard Markdown with GFM extensions (tables, task lists, strikethrough)
- `"gfm"` or `"github"`: Alias for standard (pulldown-cmark already supports GFM)
- `"commonmark"`: Alias for standard
- `"mkdocs"`: MkDocs-specific extensions (auto-references, admonitions)
- `"mdx"`: MDX flavor with JSX and ESM support
- `"quarto"`: Quarto/RMarkdown for scientific publishing

**Behavior**:

- Uses "first match wins" semantics - order matters in the configuration
- Patterns are matched against relative paths from the project root
- Falls back to global `flavor` setting if no pattern matches
- Falls back to auto-detection by file extension if no global flavor is set

**Pattern Syntax**:

- `*` matches any characters except path separators
- `**` matches any characters including path separators
- `?` matches a single character
- Patterns are relative to the project root

**Usage Notes**:

- Useful for projects with mixed documentation (e.g., MkDocs site + MDX components)
- Order patterns from most specific to least specific
- Auto-detection works for common extensions: `.mdx` → MDX, `.qmd`/`.Rmd` → Quarto

**Example: Mixed Documentation Project**:

```toml
[global]
flavor = "standard"  # Default for files not matching any pattern

[per-file-flavor]
# MkDocs documentation
"docs/**/*.md" = "mkdocs"

# React components with MDX
"src/components/**/*.mdx" = "mdx"

# Jupyter/Quarto notebooks
"notebooks/**/*.qmd" = "quarto"

# Keep README and CHANGELOG as standard
"README.md" = "standard"
"CHANGELOG.md" = "standard"
```

**Example: Monorepo with Multiple Doc Systems**:

```toml
[per-file-flavor]
"packages/website/docs/**/*.md" = "mkdocs"
"packages/storybook/**/*.mdx" = "mdx"
"packages/api/docs/**/*.md" = "standard"
```

### `output-format`

**Type**: `string`
**Default**: `"text"`
**CLI Equivalent**: `--output-format`
**Environment Variable**: `RUMDL_OUTPUT_FORMAT`

Specifies the output format for linting results.

```toml
[global]
output-format = "github"  # Use GitHub Actions format
```

**Available Formats**:

- `"text"` (default): Human-readable output with colors
- `"full"`: Detailed output with context
- `"concise"`: Minimal output (one line per warning)
- `"grouped"`: Group warnings by file
- `"json"`: JSON output for programmatic use
- `"json-lines"`: One JSON object per line (streaming)
- `"github"`: GitHub Actions annotations
- `"gitlab"`: GitLab CI format
- `"pylint"`: PyLint-compatible format
- `"azure"`: Azure Pipelines format
- `"sarif"`: SARIF format for security tools
- `"junit"`: JUnit XML format for CI systems

**Precedence**:

1. CLI flag (`--output-format`) wins
2. Environment variable (`RUMDL_OUTPUT_FORMAT`) overrides config
3. Config file setting (`output-format`)
4. Default (`"text"`)

**Usage Notes**:

- Use `github` format in GitHub Actions for inline annotations
- Use `json` or `sarif` for integration with other tools
- The environment variable is useful for CI/CD pipelines where you want to override the project config

**Example CLI usage**:

```bash
# Use GitHub format for Actions
rumdl check --output-format github .

# Or via environment variable
RUMDL_OUTPUT_FORMAT=github rumdl check .
```

**Example GitHub Actions workflow**:

```yaml
- name: Lint Markdown
  env:
    RUMDL_OUTPUT_FORMAT: github
  run: rumdl check .
```

### `cache`

**Type**: `boolean`
**Default**: `true`
**CLI Equivalent**: `--no-cache` (to disable)

Controls whether rumdl caches linting results to speed up subsequent runs.

```toml
[global]
cache = true   # Enable caching (default)
# or
cache = false  # Disable caching
```

**Behavior**:

- `true` (default): Results are cached based on file content hashes
- `false`: Every run processes all files from scratch

**Usage Notes**:

- Caching significantly speeds up repeated linting of unchanged files
- Cache is automatically invalidated when file content changes
- Disable caching during development when debugging rule changes
- Use `--no-cache` CLI flag for one-time cache bypass without changing config

**Example CLI usage**:

```bash
# Disable cache for this run only
rumdl check --no-cache .
```

### `cache_dir`

**Type**: `string`
**Default**: `.rumdl_cache`
**CLI Equivalent**: `--cache-dir`
**Environment Variable**: `RUMDL_CACHE_DIR`

Specifies the directory where rumdl stores cache files.

```toml
[global]
cache_dir = ".rumdl_cache"      # Default location
# or
cache_dir = "/tmp/rumdl-cache"  # Custom location
```

**Behavior**:

- Cache files are stored in this directory
- Directory is created automatically if it doesn't exist
- Each file's cache entry is based on its content hash

**Usage Notes**:

- Default `.rumdl_cache` is relative to the current working directory
- Use absolute paths for consistent behavior across different working directories
- Consider adding the cache directory to `.gitignore`
- Environment variable `RUMDL_CACHE_DIR` can override config file settings

**Example CLI usage**:

```bash
# Use custom cache directory
rumdl check --cache-dir /tmp/rumdl-cache .

# Or via environment variable
RUMDL_CACHE_DIR=/tmp/rumdl-cache rumdl check .
```

**Adding to .gitignore**:

```gitignore
# rumdl cache
.rumdl_cache/
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
  rev: v0.0.200
  hooks:
    - id: rumdl
      args: [--config=.rumdl.toml]
```

## See Also

- [Configuration Guide](../README.md#configuration) - Basic configuration setup
- [Rules Reference](RULES.md) - Complete list of available rules
- [CLI Reference](../README.md#command-line-interface) - Command-line options
- [Rule-specific Configuration](../README.md#configuration-file-example) - Configuring individual rules
