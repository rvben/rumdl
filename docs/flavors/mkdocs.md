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

Allows `markdown`, `markdown="1"`, or `markdown="block"` to enable Markdown processing inside HTML elements. This includes Material for MkDocs grid cards pattern:

```markdown
<div class="grid cards" markdown>

-   :zap:{ .lg .middle } **Built for speed**

    ---

    Written in Rust for blazing fast performance.

</div>
```

Supported elements: `div`, `section`, `article`, `aside`, `details`, `figure`, `footer`, `header`, `main`, `nav`.

**Affected rules**: MD030 (list marker space), MD033 (inline HTML), MD035 (HR style)

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

### PyMdown Blocks

[PyMdown Blocks](https://facelessuser.github.io/pymdown-extensions/extensions/blocks/) syntax using `///` fences is recognized:

```markdown
/// details | Summary
    type: warning

Content inside the block.

///

/// caption
Figure 1: Example diagram
///

/// html | div.custom-class

Custom HTML content with markdown.

///
```

Block types include: `admonition`, `details`, `caption`, `html`, `tab`, and custom blocks.

**Affected rules**: MD012, MD018, MD022, MD025, MD030, MD033, MD036, MD057, MD059, MD064 (all skip content inside blocks)

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

| Rule  | Standard Behavior                | MkDocs Behavior                         |
| ----- | -------------------------------- | --------------------------------------- |
| MD024 | Flag duplicate headings          | Skip headings in snippet sections       |
| MD030 | Check list marker spacing        | Skip inside markdown-enabled HTML       |
| MD031 | Require blanks around all fences | Respect admonition/tab/mkdocstrings     |
| MD033 | Flag all inline HTML             | Allow `markdown` attribute on elements  |
| MD035 | Check horizontal rule style      | Skip inside markdown-enabled HTML       |
| MD038 | Flag spaces in code spans        | Handle keys/caret/mark syntax           |
| MD040 | Require language on code blocks  | Allow `title=` without language         |
| MD042 | Flag empty links `[]()`          | Allow auto-references `[Class][]`       |
| MD046 | Detect code block style globally | Account for admonition/tab context      |
| MD049 | Check emphasis consistency       | Handle mark/inserted syntax             |
| MD050 | Check strong consistency         | Handle mark/caret/tilde syntax          |
| MD051 | Validate all fragment links       | Skip footnote and option anchors        |
| MD052 | Flag undefined references        | Allow auto-references and snippets      |
| MD056 | Strict column count              | Handle MkDocs table extensions          |

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

The MkDocs flavor provides lint-aware support for the common Python-Markdown and PyMdown Extensions used in the MkDocs ecosystem.

### Support Levels

Each extension has one of these support levels:

| Level | Meaning |
|-------|---------|
| Lint-safe | rumdl won't flag valid extension syntax as violations |
| Fix-safe | `--fix` preserves extension constructs unchanged |
| Format-aware | Reflow/formatting respects extension structure (e.g., preserves indentation during MD013 reflow) |

### Python-Markdown Extensions

| Extension | Level | Description |
|-----------|-------|-------------|
| [abbr](https://python-markdown.github.io/extensions/abbreviations/) | Lint-safe, Fix-safe | Abbreviation definitions `*[HTML]: Hypertext Markup Language` |
| [admonition](https://python-markdown.github.io/extensions/admonition/) | Format-aware | Admonition blocks `!!! note` with indentation-aware reflow |
| [attr_list](https://python-markdown.github.io/extensions/attr_list/) | Lint-safe, Fix-safe | Attribute lists `{#id .class}` |
| [def_list](https://python-markdown.github.io/extensions/definition_lists/) | Lint-safe, Fix-safe | Definition lists with `:` markers |
| [footnotes](https://python-markdown.github.io/extensions/footnotes/) | Lint-safe, Fix-safe | Footnotes `[^1]`, definitions, and anchor links `#fn:1` |
| [md_in_html](https://python-markdown.github.io/extensions/md_in_html/) | Lint-safe, Fix-safe | `markdown="1"` attribute on HTML elements |
| [toc](https://python-markdown.github.io/extensions/toc/) | Lint-safe, Fix-safe | `[TOC]` markers are preserved |
| [tables](https://python-markdown.github.io/extensions/tables/) | Lint-safe, Fix-safe | Standard table support |
| [meta](https://python-markdown.github.io/extensions/meta_data/) | Lint-safe, Fix-safe | YAML frontmatter detection |
| [fenced_code](https://python-markdown.github.io/extensions/fenced_code_blocks/) | Lint-safe, Fix-safe | Fenced code blocks with attributes |
| [codehilite](https://python-markdown.github.io/extensions/code_hilite/) | N/A | Rendering-only (no linting impact) |

### PyMdown Extensions

| Extension | Level | Description |
|-----------|-------|-------------|
| [arithmatex](https://facelessuser.github.io/pymdown-extensions/extensions/arithmatex/) | Lint-safe, Fix-safe | Math blocks `$$ ... $$` and inline `$...$` |
| [betterem](https://facelessuser.github.io/pymdown-extensions/extensions/betterem/) | Lint-safe, Fix-safe | Standard emphasis handling applies |
| [blocks](https://facelessuser.github.io/pymdown-extensions/extensions/blocks/) | Lint-safe, Fix-safe | PyMdown Blocks `/// type` with `///` fence syntax |
| [caret](https://facelessuser.github.io/pymdown-extensions/extensions/caret/) | Lint-safe, Fix-safe | Superscript `^text^` and insert `^^text^^` |
| [critic](https://facelessuser.github.io/pymdown-extensions/extensions/critic/) | Lint-safe, Fix-safe | Critic markup `{++add++}`, `{--del--}` |
| [details](https://facelessuser.github.io/pymdown-extensions/extensions/details/) | Lint-safe, Fix-safe | Collapsible blocks `??? note` and `???+ note` |
| [emoji](https://facelessuser.github.io/pymdown-extensions/extensions/emoji/) | Lint-safe, Fix-safe | Emoji/icon shortcodes `:material-check:` |
| [highlight](https://facelessuser.github.io/pymdown-extensions/extensions/highlight/) | N/A | Rendering-only (no linting impact) |
| [inlinehilite](https://facelessuser.github.io/pymdown-extensions/extensions/inlinehilite/) | Lint-safe, Fix-safe | Inline code highlighting `` `#!python code` `` |
| [keys](https://facelessuser.github.io/pymdown-extensions/extensions/keys/) | Lint-safe, Fix-safe | Keyboard keys `++ctrl+alt+del++` |
| [mark](https://facelessuser.github.io/pymdown-extensions/extensions/mark/) | Lint-safe, Fix-safe | Highlighted text `==text==` |
| [smartsymbols](https://facelessuser.github.io/pymdown-extensions/extensions/smartsymbols/) | Lint-safe, Fix-safe | Smart symbols `(c)`, `(tm)`, `-->` |
| [snippets](https://facelessuser.github.io/pymdown-extensions/extensions/snippets/) | Lint-safe, Fix-safe | File inclusion `--8<-- "file.md"` |
| [superfences](https://facelessuser.github.io/pymdown-extensions/extensions/superfences/) | Lint-safe, Fix-safe | Custom fences with language + attributes |
| [tabbed](https://facelessuser.github.io/pymdown-extensions/extensions/tabbed/) | Format-aware | Content tabs `=== "Tab"` with indentation-aware reflow |
| [tasklist](https://facelessuser.github.io/pymdown-extensions/extensions/tasklist/) | Lint-safe, Fix-safe | Task lists `- [x] Task` (standard GFM) |
| [tilde](https://facelessuser.github.io/pymdown-extensions/extensions/tilde/) | Lint-safe, Fix-safe | Subscript `~text~` and strikethrough `~~text~~` |

### mkdocstrings

| Feature | Level | Description |
|---------|-------|-------------|
| [Auto-doc blocks](https://mkdocstrings.github.io/) | Format-aware | `::: module.Class` with YAML options, indentation-aware reflow |
| [Cross-references](https://mkdocstrings.github.io/) | Lint-safe, Fix-safe | `[module.Class][]` reference links |

## See Also

- [Flavors Overview](../flavors.md) - Compare all flavors
- [MkDocs Documentation](https://www.mkdocs.org/)
- [Material for MkDocs](https://squidfunk.github.io/mkdocs-material/)
- [PyMdown Extensions](https://facelessuser.github.io/pymdown-extensions/)
- [mkdocstrings](https://mkdocstrings.github.io/)
