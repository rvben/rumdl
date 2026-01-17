# Standard Flavor

**Alias**: `commonmark`

The default flavor for rumdl. Uses pulldown-cmark which already supports common GFM extensions:

- Tables
- Task lists
- Strikethrough
- Autolinks

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
