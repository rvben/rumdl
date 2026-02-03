# Comparison with markdownlint

This document provides a detailed comparison between rumdl and markdownlint, covering rule compatibility, intentional design differences, and features unique to each tool.

## Quick Summary

rumdl is **fully compatible** with markdownlint while offering significant performance improvements and design enhancements. All 53 markdownlint rules are implemented, making migration seamless.

**Key Differences:**

- **Performance**: rumdl is significantly faster (30-100x in many cases) thanks to Rust and intelligent caching
- **Rule Coverage**: 100% compatible - all 53 markdownlint rules are implemented with the same behavior
- **Unique Features**: 14 additional rules (MD057, MD061-MD073), built-in LSP server, VS Code extension, 6 Markdown flavors
- **Configuration**: Automatic markdownlint config discovery and conversion

## Rule Coverage

### Implemented Rules

rumdl implements **67 rules total**: all 53 markdownlint rules plus 14 unique rules.

**Markdownlint-compatible rules (53):** All markdownlint rules are implemented with full compatibility. See the [Rules Reference](RULES.md) for the complete list.

**Note:** Rule numbers MD001-MD060 have gaps (MD002, MD006, MD008, MD015-MD017 were never implemented in markdownlint). rumdl maintains these gaps for compatibility.

### Rules Unique to rumdl

rumdl implements 14 additional rules not found in markdownlint:

| Rule   | Name                           | Description                                                |
| ------ | ------------------------------ | ---------------------------------------------------------- |
| MD057  | Relative links                 | Validates that relative file links point to existing files |
| MD061  | Forbidden terms                | Flags usage of configurable forbidden terms                |
| MD062  | Link destination whitespace    | No whitespace in link destinations                         |
| MD063  | Heading capitalization         | Enforces consistent heading capitalization style           |
| MD064  | No multiple consecutive spaces | Flags multiple consecutive spaces in content               |
| MD065  | Blanks around horizontal rules | Horizontal rules should have surrounding blank lines       |
| MD066  | Footnote validation            | Validates footnote references have definitions             |
| MD067  | Footnote definition order      | Footnotes should appear in order of reference              |
| MD068  | Empty footnote definitions     | Footnote definitions should not be empty                   |
| MD069  | No duplicate list markers      | Flags duplicate markers like `- - text` from copy-paste    |
| MD070  | Nested code fence              | Detects nested fence collisions                            |
| MD071  | Blank line after frontmatter   | Frontmatter should be followed by a blank line             |
| MD072  | Frontmatter key sort           | Frontmatter keys should be sorted (opt-in)                 |
| MD073  | TOC validation                 | Table of Contents should match headings (opt-in)           |

**Opt-in rules:** MD060, MD063, MD072, and MD073 are disabled by default. Enable them explicitly in your configuration.

## Intentional Design Differences

### 1. CommonMark Specification Compliance

**rumdl prioritizes CommonMark specification compliance** over bug-for-bug compatibility with markdownlint's parsing.

**Example - List Continuation vs Code Blocks:**

<!-- markdownlint-disable MD046 -->

```markdown
1. List item

    This is a continuation paragraph (4 spaces = continuation)

        This is a code block (8 spaces = continuation indent + 4)
```

<!-- markdownlint-enable MD046 -->

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

### 6. Markdown Flavors

**rumdl supports 6 Markdown flavors** to adapt rule behavior for different documentation systems:

| Flavor     | Use Case                     | Key Adjustments                        |
| ---------- | ---------------------------- | -------------------------------------- |
| `standard` | Default Markdown             | CommonMark + GFM extensions            |
| `gfm`      | GitHub Flavored Markdown     | Security-sensitive HTML, autolinks     |
| `mkdocs`   | MkDocs / Material for MkDocs | Admonitions, tabs, mkdocstrings        |
| `mdx`      | MDX (JSX in Markdown)        | JSX components, ESM imports            |
| `obsidian` | Obsidian knowledge base      | Callouts, Dataview, Templater, wikilinks |
| `quarto`   | Quarto / RMarkdown           | Citations, shortcodes, executable code |

**Configuration:**

```toml
[global]
flavor = "mkdocs"

[per-file-flavor]
"docs/**/*.md" = "mkdocs"
"**/*.mdx" = "mdx"
```

markdownlint does not have built-in flavor support; users must configure individual rules manually.

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

| Feature                  | markdownlint       | rumdl                       |
| ------------------------ | ------------------ | --------------------------- |
| **Core Functionality**   |                    |                             |
| Rule count               | 53 implemented     | 67 (53 compatible + 14 new) |
| Auto-fix                 | ✅                 | ✅                          |
| Configuration file       | ✅ JSON/YAML       | ✅ TOML/JSON/YAML           |
| Inline config            | ✅                 | ✅ (compatible)             |
| Custom rules             | ✅ (JavaScript)    | ❌                          |
| Markdown flavors         | ❌                 | ✅ 6 flavors                |
| **Performance**          |                    |                             |
| Single file              | Fast               | Very Fast (10-30x)          |
| Large repos (100+ files) | Slow               | Very Fast (30-100x)         |
| Incremental mode         | ❌                 | ✅ (caching)                |
| Parallel processing      | Partial            | ✅ Full                     |
| **Developer Experience** |                    |                             |
| Built-in LSP             | ❌                 | ✅                          |
| VS Code extension        | ✅ (separate)      | ✅ (built-in)               |
| Watch mode               | Via external tools | ✅ `--watch`                |
| Stdin/stdout             | ✅                 | ✅                          |
| Diff preview             | ❌                 | ✅ `--diff`                 |
| **Installation**         |                    |                             |
| Node.js required         | ✅                 | ❌                          |
| Python pip               | ❌                 | ✅                          |
| Rust cargo               | ❌                 | ✅                          |
| Single binary            | ❌                 | ✅                          |
| Homebrew                 | ✅                 | ✅                          |
| **Output & Integration** |                    |                             |
| Text format              | ✅                 | ✅                          |
| JSON format              | ✅                 | ✅                          |
| GitHub Actions           | ✅                 | ✅ Enhanced                 |
| Statistics               | ❌                 | ✅                          |
| Profiling                | ❌                 | ✅                          |

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

## See Also

- [Comparison with mdformat](mdformat-comparison.md) - For users coming from the mdformat formatter

## References

- [markdownlint documentation](https://github.com/DavidAnson/markdownlint)
- [CommonMark specification](https://spec.commonmark.org/)
- [rumdl GitHub repository](https://github.com/rvben/rumdl)
- [rumdl rules documentation](RULES.md)
- [rumdl flavors documentation](flavors.md)
