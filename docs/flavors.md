# Markdown Flavors

rumdl supports multiple Markdown flavors to accommodate different documentation systems. Each flavor adjusts specific rule behavior where that system differs from standard Markdown.

## Quick Reference

| Flavor                                  | Use Case                             | Rules Affected                                                                            |
| --------------------------------------- | ------------------------------------ | ----------------------------------------------------------------------------------------- |
| [standard](flavors/standard.md)         | Default Markdown with GFM extensions | Baseline behavior                                                                         |
| [gfm](flavors/gfm.md)                   | GitHub Flavored Markdown             | MD033, MD034                                                                              |
| [mkdocs](flavors/mkdocs.md)             | MkDocs / Material for MkDocs         | MD024, MD031, MD033, MD038, MD040, MD042, MD046, MD049, MD050, MD052, MD056               |
| [mdx](flavors/mdx.md)                   | MDX (JSX in Markdown)                | MD013, MD033, MD037, MD039, MD044, MD049                                                  |
| [obsidian](flavors/obsidian.md)         | Obsidian knowledge base              | MD011, MD012, MD018, MD028, MD033, MD034, MD037, MD038, MD044, MD049, MD061, MD064, MD069 |
| [pandoc](flavors/pandoc.md)             | Pandoc Markdown                      | MD022, MD029, MD031, MD032, MD034, MD037, MD040, MD042, MD051, MD052                      |
| [quarto](flavors/quarto.md)             | Quarto / RMarkdown                   | MD022, MD029, MD031, MD032, MD034, MD037, MD038, MD040, MD042, MD049, MD050, MD051, MD052 |
| [kramdown](flavors/kramdown.md)         | Jekyll / kramdown                    | MD022, MD041, MD051                                                                       |
| [azure_devops](flavors/azure_devops.md) | Azure DevOps wikis                   | MD013, MD031, MD034, MD046, MD048                                                         |
| [myst](flavors/myst.md)                 | MyST / Jupyter Book / Sphinx         | MD013, MD031, MD038, MD040, MD046, MD048                                                  |
| [hugo](flavors/hugo.md)                 | Hugo / Goldmark                      | MD022, MD031, MD058                                                                       |

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
| `.kramdown`        | `kramdown`      |
| `.md`, `.markdown` | `standard`      |

## Specification Versions

rumdl uses [pulldown-cmark](https://github.com/pulldown-cmark/pulldown-cmark) for Markdown parsing, which implements [CommonMark 0.31.2](https://spec.commonmark.org/0.31.2/) (January 2024).

The `standard` flavor includes CommonMark plus widely-adopted GFM extensions (tables, task lists, strikethrough, autolinks). Other flavors build on this baseline with additional syntax support.

## Flavor Details

- **[Standard](flavors/standard.md)** - CommonMark 0.31.2 + GFM extensions (tables, task lists, strikethrough, autolinks)
- **[GFM](flavors/gfm.md)** - GitHub-specific features: security-sensitive HTML warnings, extended autolinks
- **[MkDocs](flavors/mkdocs.md)** - Admonitions, content tabs, autorefs, mkdocstrings, extended syntax
- **[MDX](flavors/mdx.md)** - JSX components, JSX attributes, expressions, ESM imports
- **[Obsidian](flavors/obsidian.md)** - Callouts, comments, highlights, Dataview queries, Templater syntax, tags
- **[Pandoc](flavors/pandoc.md)** - Fenced divs, attribute lists, citations, footnotes, definition lists, math, raw format blocks, grid/multi-line tables, line blocks, sub/superscripts, example lists
- **[Quarto](flavors/quarto.md)** - Citations, shortcodes, div blocks, math blocks, executable code
- **[Kramdown](flavors/kramdown.md)** - IALs, ALDs, extension blocks, kramdown anchor generation
- **[Azure DevOps](flavors/azure_devops.md)** - Colon code fences (`:::mermaid … :::`) treated as opaque code blocks
- **[MyST](flavors/myst.md)** - Directives (`:::{name}`, `` ```{name} ``), roles (`{role}`content``), `%` comments
- **[Hugo](flavors/hugo.md)** - Goldmark Markdown attributes (`{class="a" id="b"}`) attached to tables, headings, and code blocks

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
- [Rules Reference](rules.md) - Complete rule documentation
