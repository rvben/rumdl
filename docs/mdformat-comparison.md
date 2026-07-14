# Comparison with mdformat

This document compares rumdl and mdformat, focusing on their formatting capabilities, performance, and feature sets.

## Quick Summary

Both tools format Markdown files, but serve different purposes:

- **mdformat**: Pure formatter focused on consistent Markdown output
- **rumdl**: Combined linter and formatter with <!-- RULE_COUNT -->76<!-- /RULE_COUNT --> rules plus formatting

**Key Differences:**

| Aspect          | mdformat                  | rumdl                           |
| --------------- | ------------------------- | ------------------------------- |
| Primary purpose | Formatting only           | Linting + formatting            |
| Language        | Python                    | Rust                            |
| Performance     | Good                      | Faster (native + caching)       |
| Linting rules   | ❌                        | ✅ <!-- RULE_COUNT -->76<!-- /RULE_COUNT --> rules                     |
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

rumdl provides <!-- RULE_COUNT -->76<!-- /RULE_COUNT --> linting rules that mdformat does not have:

- **Broken link detection** (MD051, MD052, MD057)
- **Accessibility checks** (MD045 - image alt text)
- **Heading structure** (MD001, MD024, MD025, MD043)
- **Style enforcement** (MD003, MD004, MD049, MD050)
- **Error detection** (MD011 - reversed links, MD042 - empty links)

See the [Rules Reference](rules.md) for the complete list.

### Extended Syntax Support

**mdformat** uses plugins for extended syntax:

```bash
pip install mdformat-gfm mdformat-frontmatter
```

**rumdl** uses built-in flavors:

```toml
[global]
flavor = "gfm"  # or: mkdocs, mdx, pandoc, quarto, obsidian, kramdown, azure_devops
```

| Syntax              | mdformat             | rumdl               |
| ------------------- | -------------------- | ------------------- |
| GFM (tables, tasks) | mdformat-gfm         | Built-in (standard) |
| Frontmatter         | mdformat-frontmatter | Built-in            |
| Admonitions         | mdformat-admon       | mkdocs flavor       |
| MDX/JSX             | ❌                   | mdx flavor          |
| Obsidian            | ❌                   | obsidian flavor     |
| Quarto              | ❌                   | quarto flavor       |

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
   - repo: https://github.com/rvben/rumdl-pre-commit
     rev: v0.2.34
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

Some teams run mdformat as the formatter and rumdl as the linter. The pipeline
is "format first, lint after": mdformat normalizes the file, then rumdl checks
the structural rules that mdformat does not cover (broken links, heading
hierarchy, alt text, etc.).

This works, but a few rumdl rules have opinions about syntax that mdformat also
rewrites. With default rumdl settings, two of those rules will fight mdformat
in a `mdformat -> rumdl --fix -> mdformat` loop. Use the preset below to make
rumdl yield to mdformat's canonical output on render-identical details:

```toml
# .rumdl.toml -- run rumdl alongside mdformat
# Pins rumdl rules whose defaults disagree with mdformat output.

# Horizontal rules: mdformat emits 70 underscores. Match it so
# rumdl does not rewrite back to "---".
[MD035]
style = "______________________________________________________________________"

# Code blocks: mdformat converts indented blocks to fenced.
[MD046]
style = "fenced"

# Code fence character: mdformat uses backticks.
[MD048]
style = "backtick"

# Unordered list markers: mdformat uses "-".
[MD004]
style = "dash"

# Heading style: mdformat uses ATX.
[MD003]
style = "atx"
```

`<br>` handling is not in the preset because rumdl's MD033 auto-fix is off by default. If you enable it (`[MD033] fix = true`), also set `br-style = "backslash"` to match mdformat.

### Alternatives

1. **Replace both** with `rumdl check --fix`: rumdl handles lint and the formatting subset most projects need. Simpler toolchain, single binary.
2. **Use mdformat + rumdl together** with the preset above when you specifically need mdformat plugins (Jupyter Book, MyST, code-block reformatting via Black/Prettier).
3. **Migrate incrementally** by starting with `rumdl check` then adding `rumdl fmt`.

## Summary

| Capability              | mdformat           | rumdl                  |
| ----------------------- | ------------------ | ---------------------- |
| Markdown formatting     | ✅ Primary focus   | ✅ Via `rumdl fmt`     |
| Markdown linting        | ❌                 | ✅ <!-- RULE_COUNT -->76<!-- /RULE_COUNT --> rules            |
| Performance             | Good               | Faster (native binary) |
| Extended syntax         | Plugins            | Built-in flavors       |
| Editor integration      | Basic              | LSP + VS Code          |
| Installation complexity | Python + plugins   | Single binary          |

**Bottom line:** If you only need formatting, mdformat works well. If you want linting, better performance, or a single tool for both, rumdl is the better choice.

## See Also

- [Tool Comparison Matrix](comparison.md) - Broad comparison of Markdown linters and formatters
- [Comparison with markdownlint](markdownlint-comparison.md) - For users coming from the markdownlint linter

## References

- [mdformat documentation](https://mdformat.readthedocs.io/)
- [mdformat GitHub](https://github.com/executablebooks/mdformat)
- [rumdl GitHub](https://github.com/rvben/rumdl)
- [rumdl rules documentation](rules.md)
- [rumdl flavors documentation](flavors.md)
