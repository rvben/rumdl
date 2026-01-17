# Markdown Flavors

rumdl supports multiple Markdown flavors to accommodate different documentation systems. Each flavor adjusts specific rule behavior where that system differs from standard Markdown.

## Quick Reference

| Flavor                            | Use Case                             | Rules Affected                                                              |
| --------------------------------- | ------------------------------------ | --------------------------------------------------------------------------- |
| [standard](flavors/standard.md)   | Default Markdown with GFM extensions | Baseline behavior                                                           |
| [gfm](flavors/gfm.md)             | GitHub Flavored Markdown             | MD033, MD034                                                                |
| [mkdocs](flavors/mkdocs.md)       | MkDocs / Material for MkDocs         | MD024, MD031, MD033, MD038, MD040, MD042, MD046, MD049, MD050, MD052, MD056 |
| [mdx](flavors/mdx.md)             | MDX (JSX in Markdown)                | MD013, MD033, MD037, MD039, MD044, MD049                                    |
| [quarto](flavors/quarto.md)       | Quarto / RMarkdown                   | MD034, MD037, MD038, MD040, MD042, MD049, MD050, MD051, MD052               |

## Configuration

### Global Flavor

Set the default flavor for all files:

```toml
[global]
flavor = "mkdocs"
```

### Per-File Flavor

Override flavor for specific file patterns:

```toml
[per-file-flavor]
"docs/**/*.md" = "mkdocs"
"**/*.mdx" = "mdx"
"**/*.qmd" = "quarto"
```

### Auto-Detection

When no flavor is configured, rumdl auto-detects based on file extension:

| Extension          | Detected Flavor |
| ------------------ | --------------- |
| `.mdx`             | `mdx`           |
| `.qmd`, `.Rmd`     | `quarto`        |
| `.md`, `.markdown` | `standard`      |

## Specification Versions

rumdl uses [pulldown-cmark](https://github.com/pulldown-cmark/pulldown-cmark) for Markdown parsing, which implements [CommonMark 0.31.2](https://spec.commonmark.org/0.31.2/) (January 2024).

The `standard` flavor includes CommonMark plus widely-adopted GFM extensions (tables, task lists, strikethrough, autolinks). Other flavors build on this baseline with additional syntax support.

## Flavor Details

- **[Standard](flavors/standard.md)** - CommonMark 0.31.2 + GFM extensions (tables, task lists, strikethrough, autolinks)
- **[GFM](flavors/gfm.md)** - GitHub-specific features: security-sensitive HTML warnings, extended autolinks
- **[MkDocs](flavors/mkdocs.md)** - Admonitions, content tabs, autorefs, mkdocstrings, extended syntax
- **[MDX](flavors/mdx.md)** - JSX components, JSX attributes, expressions, ESM imports
- **[Quarto](flavors/quarto.md)** - Citations, shortcodes, div blocks, math blocks, executable code

## Adding Flavor Support

If you encounter a pattern that rumdl doesn't handle correctly for your documentation system:

1. Check if the pattern is already supported in the flavor documentation
2. Try configuring the relevant rule to allow the pattern
3. Open an issue with:
   - The Markdown content that triggers a false positive
   - The documentation system and version you're using
   - The expected behavior

## See Also

- [Global Settings](global-settings.md) - Configure flavor globally
- [Per-File Configuration](global-settings.md#per-file-flavor) - Override flavor per file
- [Rules Reference](RULES.md) - Complete rule documentation
