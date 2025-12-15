# Comparison with markdownlint

This document provides a detailed comparison between rumdl and markdownlint, covering rule compatibility, intentional design differences, and features unique to each tool.

## Quick Summary

rumdl is **fully compatible** with markdownlint while offering significant performance improvements and design enhancements. All 53 markdownlint rules are implemented, making migration seamless.

**Key Differences:**

- **Performance**: rumdl is significantly faster (30-100x in many cases) thanks to Rust and intelligent caching
- **Rule Coverage**: 100% compatible - all 53 markdownlint rules are implemented with the same behavior
- **Unique Features**: Built-in LSP server, VS Code extension, CommonMark compliance focus, MD057 (relative link validation)
- **Configuration**: Automatic markdownlint config discovery and conversion

## Rule Coverage

### Implemented Rules

rumdl implements **all 53 markdownlint rules** with compatible behavior (100% compatibility):

✅ MD001, MD003, MD004, MD005, MD007, MD009, MD010, MD011, MD012, MD013, MD014, MD018, MD019, MD020, MD021, MD022, MD023, MD024, MD025, MD026, MD027, MD028, MD029, MD030, MD031, MD032,
MD033, MD034, MD035, MD036, MD037, MD038, MD039, MD040, MD041, MD042, MD043, MD044, MD045, MD046, MD047, MD048, MD049, MD050, MD051, MD052, MD053, MD054, MD055, MD056, MD058, MD059, MD060

**Note:** Although rule numbers go from MD001 to MD060 (60 slots), markdownlint only implements 53 rules due to 7 gaps in numbering (see "Skipped Rule Numbers" below).

### Rules Not in markdownlint

rumdl implements additional rules not found in markdownlint:

#### MD057 - Existing relative links

- **Status**: Unique to rumdl
- **Purpose**: Validates that relative file links (e.g., `[docs](../README.md)`) point to files that actually exist
- **Reason**: Catches broken documentation links at lint time
- **Compatibility**: Not enabled by default; opt-in feature

### Skipped Rule Numbers

These rule numbers are not implemented in markdownlint:

- **MD002**: Deprecated and removed from markdownlint v0.13.0 (replaced by MD041); removed from rumdl v0.0.172 for compatibility
- **MD006**: Not implemented in DavidAnson/markdownlint (JavaScript version); removed from rumdl v0.0.172 for compatibility
- **MD008**: Originally planned for "Unordered list spacing" but never implemented
- **MD015, MD016, MD017**: Never assigned in markdownlint
- **MD057**: Reserved by rumdl for "Existing relative links" validation (not in markdownlint)

## Intentional Design Differences

### 1. CommonMark Specification Compliance

**rumdl prioritizes CommonMark specification compliance** over bug-for-bug compatibility with markdownlint's parsing.

**Example - List Continuation vs Code Blocks:**

```markdown
1. List item

    This is a continuation paragraph (4 spaces = continuation)

        This is a code block (8 spaces = continuation indent + 4)
```

- **markdownlint**: May incorrectly treat 4-space indented paragraphs as code blocks
- **rumdl**: Follows CommonMark: 4 spaces = list continuation, 8 spaces = code block within list
- **Rationale**: Reduces false positives and aligns with the official Markdown spec

**References:**

- [CommonMark List Specification](https://spec.commonmark.org/0.31.2/#lists)
- [rumdl Issue #128](https://github.com/rvben/rumdl/issues/128) - False positive fix

### 2. Performance Architecture

**rumdl uses Rust and intelligent caching** for significant performance gains:

- **Cold start**: 30-100x faster than markdownlint on large repositories
- **Incremental**: Only re-lints changed files (Ruff-style caching)
- **Parallel processing**: Multi-threaded file processing and rule execution
- **Zero dependencies**: Single binary, no Node.js runtime required

**Benchmark:** See the [performance comparison](../README.md#performance) in the main README, which shows detailed benchmarks on the Rust Book repository (478 markdown files).

### 3. Auto-fix Mode Differences

Both tools support auto-fixing, but with different philosophies:

**markdownlint:**

- Fixes issues in-place
- Requires `--fix` flag

**rumdl:**

- Two modes: `rumdl fmt` (formatter-style, exits 0) and `rumdl check --fix` (linter-style, exits 0 if all violations fixed, 1 if violations remain)
- `--diff` mode to preview changes
- Parallel file fixing (4.8x faster on multi-file projects)

**Why two modes?**

- `fmt`: Designed for editor integration (doesn't fail on unfixable issues)
- `check --fix`: Designed for CI/CD (fails if violations remain after fixing)

### 4. Configuration Philosophy

**Automatic Discovery:**

rumdl automatically discovers and loads markdownlint config files:

```bash
# rumdl automatically finds and uses these:
.markdownlint.json
.markdownlint.yaml
markdownlint.json
```

**Conversion Tool:**

```bash
# Convert markdownlint config to rumdl format:
rumdl import .markdownlint.json --output .rumdl.toml
```

**Multiple Formats:**

- Native: `.rumdl.toml` (TOML, with JSON schema support)
- Python projects: `pyproject.toml` with `[tool.rumdl]` section
- Markdownlint: Automatic compatibility mode

### 5. Editor Integration

**rumdl includes a built-in Language Server Protocol (LSP) implementation:**

```bash
# Start LSP server
rumdl server

# Install VS Code extension
rumdl vscode
```

**Features:**

- Real-time linting as you type
- Quick fixes for supported rules
- Hover documentation for rules
- Zero configuration required

## Configuration Compatibility

### Markdownlint Config Auto-Detection

rumdl automatically discovers and loads markdownlint configurations:

```yaml
# .markdownlint.yaml (automatically loaded)
MD013: false
MD033:
  allowed_elements: ['br', 'img']
```

### Equivalent rumdl Configuration

```toml
# .rumdl.toml
[global]
disable = ["MD013"]

[MD033]
allowed_elements = ["br", "img"]
```

### Configuration Mapping

Most markdownlint options map directly to rumdl:

| markdownlint                 | rumdl                  |
| ---------------------------- | ---------------------- |
| `default: true`              | `[global]` section     |
| Rule by number (`MD013`)     | Same (`[MD013]`)       |
| Rule by name (`line-length`) | Same (`[line-length]`) |
| Disabling: `"MD013": false`  | `disable = ["MD013"]`  |

### Per-File Ignores

Both support per-file rule configuration:

```toml
# rumdl
[per-file-ignores]
"README.md" = ["MD033"]  # Allow HTML in README
"docs/api/**/*.md" = ["MD013"]  # Relax line length in API docs
```

markdownlint uses glob patterns in separate config files or inline comments.

## Inline Configuration Compatibility

rumdl supports both `rumdl` and `markdownlint` inline comment styles:

```markdown
<!-- markdownlint-disable MD013 -->
This line can be as long as needed
<!-- markdownlint-enable MD013 -->

<!-- rumdl-disable MD013 -->
Alternative syntax also supported
<!-- rumdl-enable MD013 -->
```

Both syntaxes work identically in rumdl for seamless migration.

## CLI Differences

### Command Structure

**markdownlint:**

```bash
markdownlint README.md
markdownlint --fix **/*.md
markdownlint --config .markdownlint.json docs/
```

**rumdl:**

```bash
rumdl check README.md
rumdl check --fix .  # or: rumdl fmt .
rumdl check --config .rumdl.toml docs/
```

### Output Formats

Both support:

- Text output (colored, human-readable)
- JSON output (for tool integration)

rumdl additionally supports:

- GitHub Actions annotations (`--output-format github`)
- Statistics summary (`--statistics`)
- Profiling information (`--profile`)

### Exit Codes

**markdownlint:**

- `0`: No violations
- `1`: Violations found
- `2`: Error occurred

**rumdl (same):**

- `0`: Success (or `rumdl fmt` completed successfully)
- `1`: Violations found (or remain after `--fix`)
- `2`: Tool error

## Migration Guide

### From markdownlint to rumdl

1. **Install rumdl:**

   ```bash
   uv tool install rumdl
   # or: cargo install rumdl
   # or: pip install rumdl
   # or: brew install rumdl (See README.md)
   ```

2. **Test with existing config:**

   ```bash
   # rumdl automatically discovers .markdownlint.json
   rumdl check .
   ```

3. **(Optional) Convert config:**

   ```bash
   rumdl import .markdownlint.json
   ```

4. **Update CI/CD:**

   ```yaml
   # Before (markdownlint)
   - run: markdownlint '**/*.md'

   # After (rumdl)
   - run: rumdl check .
   ```

### Known Migration Issues

None currently known. If you encounter compatibility issues, please [file an issue](https://github.com/rvben/rumdl/issues).

## Feature Comparison Table

| Feature                  | markdownlint       | rumdl                 |
| ------------------------ | ------------------ | --------------------- |
| **Core Functionality**   |                    |                       |
| Rule count               | 53 implemented     | 53 (+1 unique: MD057) |
| Auto-fix                 | ✅                 | ✅                    |
| Configuration file       | ✅ JSON/YAML       | ✅ TOML/JSON/YAML     |
| Inline config            | ✅                 | ✅ (compatible)       |
| Custom rules             | ✅ (JavaScript)    | ❌ (planned)          |
| **Performance**          |                    |                       |
| Single file              | Fast               | Very Fast (10-30x)    |
| Large repos (100+ files) | Slow               | Very Fast (30-100x)   |
| Incremental mode         | ❌                 | ✅ (caching)          |
| Parallel processing      | Partial            | ✅ Full               |
| **Developer Experience** |                    |                       |
| Built-in LSP             | ❌                 | ✅                    |
| VS Code extension        | ✅ (separate)      | ✅ (built-in)         |
| Watch mode               | Via external tools | ✅ `--watch`          |
| Stdin/stdout             | ✅                 | ✅                    |
| Diff preview             | ❌                 | ✅ `--diff`           |
| **Installation**         |                    |                       |
| Node.js required         | ✅                 | ❌                    |
| Python pip               | ❌                 | ✅                    |
| Rust cargo               | ❌                 | ✅                    |
| Single binary            | ❌                 | ✅                    |
| Homebrew                 | ✅                 | ✅                    |
| **Output & Integration** |                    |                       |
| Text format              | ✅                 | ✅                    |
| JSON format              | ✅                 | ✅                    |
| GitHub Actions           | ✅                 | ✅ Enhanced           |
| Statistics               | ❌                 | ✅                    |
| Profiling                | ❌                 | ✅                    |

## CommonMark Compliance

rumdl prioritizes **CommonMark specification compliance** to reduce false positives and align with modern Markdown standards:

| Aspect                    | markdownlint    | rumdl                                  |
| ------------------------- | --------------- | -------------------------------------- |
| List continuation indent  | Custom logic    | CommonMark spec                        |
| Code blocks in lists      | May misdetect   | Spec-compliant (8 spaces)              |
| Heading anchor generation | GitHub-flavored | Multiple styles (GitHub, GitLab, etc.) |
| Reference definitions     | Basic           | Full spec support                      |

### Reporting Compatibility Issues

If you find a compatibility issue with markdownlint:

1. Check [existing issues](https://github.com/rvben/rumdl/issues?q=is%3Aissue+label%3Acompatibility)
2. Verify with both tools: `markdownlint file.md` and `rumdl check file.md`
3. [File an issue](https://github.com/rvben/rumdl/issues/new) with:
   - Markdown sample
   - Expected behavior (markdownlint output)
   - Actual behavior (rumdl output)
   - Versions of both tools

## References

- [markdownlint documentation](https://github.com/DavidAnson/markdownlint)
- [CommonMark specification](https://spec.commonmark.org/)
- [rumdl GitHub repository](https://github.com/rvben/rumdl)
- [rumdl rules documentation](RULES.md)
