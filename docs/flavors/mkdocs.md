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

## Extension Support Reference

The MkDocs flavor provides lint-aware support for the common Python-Markdown and PyMdown Extensions used in the MkDocs ecosystem. This means rumdl will not flag valid extension syntax as errors and will preserve extension constructs during auto-fix.

### Python-Markdown Extensions

| Extension | Status | Description |
|-----------|--------|-------------|
| [abbr](https://python-markdown.github.io/extensions/abbreviations/) | ✅ Supported | Abbreviation definitions `*[HTML]: Hypertext Markup Language` |
| [admonition](https://python-markdown.github.io/extensions/admonition/) | ✅ Supported | Admonition blocks `!!! note` |
| [attr_list](https://python-markdown.github.io/extensions/attr_list/) | ✅ Supported | Attribute lists `{#id .class}` |
| [def_list](https://python-markdown.github.io/extensions/definition_lists/) | ✅ Supported | Definition lists with `:` markers |
| [footnotes](https://python-markdown.github.io/extensions/footnotes/) | ✅ Supported | Footnotes `[^1]` and definitions |
| [md_in_html](https://python-markdown.github.io/extensions/md_in_html/) | ✅ Supported | `markdown="1"` attribute on HTML |
| [toc](https://python-markdown.github.io/extensions/toc/) | ✅ Supported | `[TOC]` markers are preserved |
| [tables](https://python-markdown.github.io/extensions/tables/) | ✅ Supported | Standard table support |
| [meta](https://python-markdown.github.io/extensions/meta_data/) | ✅ Supported | YAML frontmatter detection |
| [fenced_code](https://python-markdown.github.io/extensions/fenced_code_blocks/) | ✅ Supported | Fenced code blocks with attributes |
| [codehilite](https://python-markdown.github.io/extensions/code_hilite/) | N/A | Rendering-only (no linting needed) |

### PyMdown Extensions

| Extension | Status | Description |
|-----------|--------|-------------|
| [arithmatex](https://facelessuser.github.io/pymdown-extensions/extensions/arithmatex/) | ✅ Supported | Math blocks `$$ ... $$` and inline `$...$` |
| [caret](https://facelessuser.github.io/pymdown-extensions/extensions/caret/) | ✅ Supported | Superscript `^text^` and insert `^^text^^` |
| [mark](https://facelessuser.github.io/pymdown-extensions/extensions/mark/) | ✅ Supported | Highlighted text `==text==` |
| [tilde](https://facelessuser.github.io/pymdown-extensions/extensions/tilde/) | ✅ Supported | Subscript `~text~` and strikethrough `~~text~~` |
| [details](https://facelessuser.github.io/pymdown-extensions/extensions/details/) | ✅ Supported | Collapsible blocks `??? note` and `???+ note` |
| [emoji](https://facelessuser.github.io/pymdown-extensions/extensions/emoji/) | ✅ Supported | Emoji/icon shortcodes `:material-check:` |
| [highlight](https://facelessuser.github.io/pymdown-extensions/extensions/highlight/) | N/A | Rendering-only (no linting needed) |
| [inlinehilite](https://facelessuser.github.io/pymdown-extensions/extensions/inlinehilite/) | ✅ Supported | Inline code highlighting `` `#!python code` `` |
| [keys](https://facelessuser.github.io/pymdown-extensions/extensions/keys/) | ✅ Supported | Keyboard keys `++ctrl+alt+del++` |
| [smartsymbols](https://facelessuser.github.io/pymdown-extensions/extensions/smartsymbols/) | ✅ Supported | Smart symbols `(c)`, `(tm)`, `-->` |
| [snippets](https://facelessuser.github.io/pymdown-extensions/extensions/snippets/) | ✅ Supported | File inclusion `--8<-- "file.md"` |
| [superfences](https://facelessuser.github.io/pymdown-extensions/extensions/superfences/) | ✅ Supported | Custom fences with language + attributes |
| [tabbed](https://facelessuser.github.io/pymdown-extensions/extensions/tabbed/) | ✅ Supported | Content tabs `=== "Tab"` |
| [tasklist](https://facelessuser.github.io/pymdown-extensions/extensions/tasklist/) | ✅ Supported | Task lists `- [x] Task` (standard GFM) |
| [betterem](https://facelessuser.github.io/pymdown-extensions/extensions/betterem/) | ✅ Supported | Standard emphasis handling applies |
| [critic](https://facelessuser.github.io/pymdown-extensions/extensions/critic/) | ✅ Supported | Critic markup `{++add++}`, `{--del--}` |

### mkdocstrings

| Feature | Status | Description |
|---------|--------|-------------|
| [Auto-doc blocks](https://mkdocstrings.github.io/) | ✅ Supported | `::: module.Class` with YAML options |
| [Cross-references](https://mkdocstrings.github.io/) | ✅ Supported | `[module.Class][]` reference links |

## See Also

- [Flavors Overview](../flavors.md) - Compare all flavors
- [MkDocs Documentation](https://www.mkdocs.org/)
- [Material for MkDocs](https://squidfunk.github.io/mkdocs-material/)
- [PyMdown Extensions](https://facelessuser.github.io/pymdown-extensions/)
- [mkdocstrings](https://mkdocstrings.github.io/)
