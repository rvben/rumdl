# Standard Flavor

**Alias**: `commonmark`

The default flavor for rumdl, based on [CommonMark 0.31.2](https://spec.commonmark.org/0.31.2/) (January 2024) via [pulldown-cmark](https://github.com/pulldown-cmark/pulldown-cmark).

## Included Extensions

In addition to strict CommonMark, the standard flavor includes these widely-adopted GFM extensions:

- Tables
- Task lists
- Strikethrough
- Autolinks

These extensions are enabled by default because they are supported by most Markdown renderers (GitHub, GitLab, VS Code, etc.).

## Strict CommonMark vs Standard

The `standard` flavor is **not** strict CommonMark - it includes GFM extensions for practical usability.
If you need strict CommonMark compliance, you can disable specific rules that rely on GFM features.

All rules use their default behavior with no flavor-specific adjustments.

## When to Use

Use the standard flavor when:

- Writing generic Markdown documentation
- Your content doesn't use syntax from specialized systems (MkDocs, MDX, Quarto)
- You want the strictest linting behavior

## Configuration

Standard is the default, so no configuration is needed. You can explicitly set it:

```toml
[global]
flavor = "standard"
```

## See Also

- [Flavors Overview](../flavors.md) - Compare all flavors
- [GFM Flavor](gfm.md) - For GitHub-specific content
