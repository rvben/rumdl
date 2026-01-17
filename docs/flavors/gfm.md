# GFM Flavor

**Alias**: `github`

For content targeting GitHub specifically. Extends standard flavor with GitHub-specific behavior.

## Security-Sensitive HTML Tags

MD033 is configured to flag potentially dangerous HTML elements that GitHub may render:

```markdown
<!-- These are flagged in GFM mode -->
<script>alert('xss')</script>
<iframe src="..."></iframe>
<form action="...">...</form>
```

## Extended Autolinks

MD034 recognizes additional URL schemes from the GFM autolink extension:

- `xmpp:` - XMPP URIs (e.g., `xmpp:user@example.com`)
- Extended email detection with proper protocol handling

## Rule Behavior Changes

| Rule  | Standard Behavior           | GFM Behavior                         |
| ----- | --------------------------- | ------------------------------------ |
| MD033 | Flag all inline HTML        | Warn on security-sensitive tags      |
| MD034 | Detect standard URLs/emails | Include xmpp: and extended autolinks |

## Configuration

```toml
[global]
flavor = "gfm"
```

## When to Use

Use the GFM flavor when:

- Writing README files for GitHub repositories
- Creating GitHub wikis
- Writing content for GitHub Pages
- You want security-focused HTML warnings

## See Also

- [Flavors Overview](../flavors.md) - Compare all flavors
- [Standard Flavor](standard.md) - Base flavor without GFM extensions
