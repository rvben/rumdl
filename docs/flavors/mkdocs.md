# MkDocs Flavor

For projects using [MkDocs](https://www.mkdocs.org/) or [Material for MkDocs](https://squidfunk.github.io/mkdocs-material/).

## Supported Patterns

### Auto-References

MkDocs autorefs plugin allows shorthand links to documented objects:

```markdown
See [ClassName][] for details.
Use [module.function][] in your code.
```

**Affected rules**: MD042 (empty links), MD052 (reference links)

### Admonitions

MkDocs admonition syntax is recognized:

```markdown
!!! note "Title"
    Content inside admonition.

!!! warning
    Warning content.

??? tip "Collapsible"
    Hidden content.
```

**Affected rules**: MD031 (blanks around fences), MD046 (code block style)

### Content Tabs

Material for MkDocs tab syntax:

```markdown
=== "Tab 1"
    Content for tab 1.

=== "Tab 2"
    Content for tab 2.
```

**Affected rules**: MD046 (code block style)

### Snippets

MkDocs snippets for including external files:

```markdown
--8<-- "path/to/file.md"
;--8<--
```

**Affected rules**: MD024 (duplicate headings), MD052 (reference links)

### HTML with Markdown Attribute

Allows `markdown="1"` to enable Markdown processing inside HTML:

```markdown
<div markdown="1">
**This** is processed as Markdown.
</div>
```

**Affected rules**: MD033 (inline HTML)

### Code Block Title Attribute

MkDocs allows `title=` on fenced code blocks:

````markdown
```python title="example.py"
print("Hello")
```
````

**Affected rules**: MD040 (fenced code language)

### Table Extensions

MkDocs table handling with extensions like `md_in_html`:

**Affected rules**: MD056 (table column count)

### mkdocstrings Blocks

mkdocstrings autodoc syntax is recognized:

```markdown
::: module.path
    options:
        show_source: true

::: package.submodule.Class
```

**Affected rules**: MD031 (blanks around fences), MD038 (code spans)

### Extended Markdown Syntax

MkDocs extensions for special formatting:

```markdown
++inserted text++     <!-- ins extension -->
==marked text==       <!-- mark extension -->
^^superscript^^       <!-- caret extension -->
~subscript~           <!-- tilde extension -->
[[keyboard keys]]     <!-- keys extension -->
```

**Affected rules**: MD038 (code spans), MD049 (emphasis style), MD050 (strong style)

## Rule Behavior Changes

| Rule  | Standard Behavior                | MkDocs Behavior                     |
| ----- | -------------------------------- | ----------------------------------- |
| MD024 | Flag duplicate headings          | Skip headings in snippet sections   |
| MD031 | Require blanks around all fences | Respect admonition/tab/mkdocstrings |
| MD033 | Flag all inline HTML             | Allow `markdown="1"` attribute      |
| MD038 | Flag spaces in code spans        | Handle keys/caret/mark syntax       |
| MD040 | Require language on code blocks  | Allow `title=` without language     |
| MD042 | Flag empty links `[]()`          | Allow auto-references `[Class][]`   |
| MD046 | Detect code block style globally | Account for admonition/tab context  |
| MD049 | Check emphasis consistency       | Handle mark/inserted syntax         |
| MD050 | Check strong consistency         | Handle mark/caret/tilde syntax      |
| MD052 | Flag undefined references        | Allow auto-references and snippets  |
| MD056 | Strict column count              | Handle MkDocs table extensions      |

## Configuration

```toml
[global]
flavor = "mkdocs"
```

Or for specific directories:

```toml
[per-file-flavor]
"docs/**/*.md" = "mkdocs"
```

## When to Use

Use the MkDocs flavor when:

- Building documentation with MkDocs
- Using Material for MkDocs theme
- Using mkdocstrings for API documentation
- Using PyMdown Extensions

## See Also

- [Flavors Overview](../flavors.md) - Compare all flavors
- [MkDocs Documentation](https://www.mkdocs.org/)
- [Material for MkDocs](https://squidfunk.github.io/mkdocs-material/)
