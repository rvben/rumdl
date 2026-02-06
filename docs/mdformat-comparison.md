# Comparison with mdformat

This document compares rumdl and mdformat, focusing on their formatting capabilities, performance, and feature sets.

## Quick Summary

Both tools format Markdown files, but serve different purposes:

- **mdformat**: Pure formatter focused on consistent Markdown output
- **rumdl**: Combined linter and formatter with 68 rules plus formatting

**Key Differences:**

| Aspect          | mdformat                  | rumdl                           |
| --------------- | ------------------------- | ------------------------------- |
| Primary purpose | Formatting only           | Linting + formatting            |
| Language        | Python                    | Rust                            |
| Performance     | Good                      | Faster (native + caching)       |
| Linting rules   | ❌                        | ✅ 68 rules                     |
| Extensibility   | Plugin ecosystem          | Built-in flavors                |
| CommonMark      | Strict compliance         | Strict compliance               |

## When to Use rumdl

- You want both linting and formatting in one tool
- Performance matters (large repositories, CI/CD pipelines)
- You need rule-based validation (broken links, accessibility, style)
- You want a single binary with no runtime dependencies
- You need built-in support for MkDocs, MDX, Obsidian, or Quarto syntax

## Feature Comparison

### Formatting Capabilities

Both tools format Markdown to a consistent style:

| Feature                    | mdformat | rumdl        |
| -------------------------- | -------- | ------------ |
| Normalize whitespace       | ✅       | ✅           |
| Consistent list markers    | ✅       | ✅           |
| Wrap long lines            | ✅       | ✅           |
| Normalize emphasis         | ✅       | ✅           |
| Normalize code fences      | ✅       | ✅           |
| Table formatting           | Plugin   | ✅ (MD060)   |
| Frontmatter preservation   | Plugin   | ✅ Built-in  |
| GFM support                | Plugin   | ✅ Built-in  |

### Linting (rumdl only)

rumdl provides 68 linting rules that mdformat does not have:

- **Broken link detection** (MD051, MD052, MD057)
- **Accessibility checks** (MD045 - image alt text)
- **Heading structure** (MD001, MD024, MD025, MD043)
- **Style enforcement** (MD003, MD004, MD049, MD050)
- **Error detection** (MD011 - reversed links, MD042 - empty links)

See the [Rules Reference](RULES.md) for the complete list.

### Extended Syntax Support

**mdformat** uses plugins for extended syntax:

```bash
pip install mdformat-gfm mdformat-frontmatter
```

**rumdl** uses built-in flavors:

```toml
[global]
flavor = "gfm"  # or: mkdocs, mdx, obsidian, quarto
```

| Syntax               | mdformat          | rumdl                |
| -------------------- | ----------------- | -------------------- |
| GFM (tables, tasks)  | mdformat-gfm      | Built-in (standard)  |
| Frontmatter          | mdformat-frontmatter | Built-in          |
| Admonitions          | mdformat-admon    | mkdocs flavor        |
| MDX/JSX              | ❌                | mdx flavor           |
| Obsidian             | ❌                | obsidian flavor      |
| Quarto               | ❌                | quarto flavor        |

## Performance

rumdl is significantly faster due to Rust implementation and has additional performance features:

- **No interpreter startup**: Single binary vs Python runtime
- **Parallel processing**: Uses all CPU cores
- **Incremental caching**: Only re-processes changed files (mdformat processes all files each run)

## Installation

**mdformat:**

```bash
pip install mdformat
# With plugins:
pip install mdformat-gfm mdformat-frontmatter
```

**rumdl:**

```bash
# Any of these:
cargo install rumdl
pip install rumdl
brew install rumdl
uv tool install rumdl
```

| Method       | mdformat | rumdl |
| ------------ | -------- | ----- |
| pip          | ✅       | ✅    |
| cargo        | ❌       | ✅    |
| Homebrew     | ❌       | ✅    |
| Single binary| ❌       | ✅    |
| No runtime   | ❌       | ✅    |

## CLI Usage

**mdformat:**

```bash
# Format files
mdformat README.md
mdformat docs/

# Check without modifying
mdformat --check README.md

# Specify line width
mdformat --wrap 80 README.md
```

**rumdl:**

```bash
# Format files (formatter mode)
rumdl fmt README.md
rumdl fmt docs/

# Check and fix (linter mode)
rumdl check --fix README.md

# Preview changes
rumdl fmt --diff README.md

# Check without modifying
rumdl check README.md
```

### Key CLI Differences

| Action              | mdformat              | rumdl                    |
| ------------------- | --------------------- | ------------------------ |
| Format in place     | `mdformat file.md`    | `rumdl fmt file.md`      |
| Check only          | `mdformat --check`    | `rumdl check`            |
| Preview diff        | ❌                    | `rumdl fmt --diff`       |
| Line width          | `--wrap 80`           | `line-length = 80`       |
| Fix + lint          | N/A                   | `rumdl check --fix`      |

## Configuration

**mdformat** uses `.mdformat.toml`:

```toml
# .mdformat.toml
wrap = 80
number = true
end_of_line = "lf"
```

Note: pyproject.toml support requires the `mdformat-pyproject` plugin.

**rumdl** uses `.rumdl.toml` or `pyproject.toml`:

```toml
# .rumdl.toml
[global]
line-length = 80
flavor = "gfm"

[MD004]
style = "dash"  # List marker style

[MD013]
line-length = 80
```

## Editor Integration

**mdformat:**

- VS Code: Via generic formatter extensions
- Pre-commit: `mdformat` hook
- No built-in LSP

**rumdl:**

- VS Code: Built-in extension (`rumdl vscode`)
- Pre-commit: `rumdl` hook
- Built-in LSP server (`rumdl server`)
- Real-time linting and quick fixes

## Migration from mdformat to rumdl

1. **Install rumdl:**

   ```bash
   pip install rumdl
   ```

2. **Replace format command:**

   ```bash
   # Before
   mdformat docs/

   # After
   rumdl fmt docs/
   ```

3. **Update pre-commit config:**

   ```yaml
   # Before
   - repo: https://github.com/executablebooks/mdformat
     rev: 0.7.17
     hooks:
       - id: mdformat

   # After
   - repo: https://github.com/rvben/rumdl
     rev: v0.1.10
     hooks:
       - id: rumdl
   ```

4. **Convert configuration:**

   ```toml
   # mdformat .mdformat.toml
   wrap = 80

   # rumdl equivalent .rumdl.toml
   [global]
   line-length = 80
   ```

## Using Both Tools Together

Some teams use mdformat for formatting and a separate linter. With rumdl, you can:

1. **Replace both** with `rumdl check --fix` (lint + format)
2. **Use rumdl for linting only** and keep mdformat for formatting
3. **Migrate incrementally** by starting with `rumdl check` then adding `rumdl fmt`

## Summary

| Capability              | mdformat           | rumdl                  |
| ----------------------- | ------------------ | ---------------------- |
| Markdown formatting     | ✅ Primary focus   | ✅ Via `rumdl fmt`     |
| Markdown linting        | ❌                 | ✅ 68 rules            |
| Performance             | Good               | Faster (native binary) |
| Extended syntax         | Plugins            | Built-in flavors       |
| Editor integration      | Basic              | LSP + VS Code          |
| Installation complexity | Python + plugins   | Single binary          |

**Bottom line:** If you only need formatting, mdformat works well. If you want linting, better performance, or a single tool for both, rumdl is the better choice.

## See Also

- [Comparison with markdownlint](markdownlint-comparison.md) - For users coming from the markdownlint linter

## References

- [mdformat documentation](https://mdformat.readthedocs.io/)
- [mdformat GitHub](https://github.com/executablebooks/mdformat)
- [rumdl GitHub](https://github.com/rvben/rumdl)
- [rumdl rules documentation](RULES.md)
- [rumdl flavors documentation](flavors.md)
