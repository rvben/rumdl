# Markdown Flavors

rumdl supports multiple Markdown flavors to accommodate different documentation systems. Each flavor adjusts specific rule behavior where that system differs from standard Markdown.

## Quick Reference

| Flavor     | Use Case                             | Rules Affected                                         |
| ---------- | ------------------------------------ | ------------------------------------------------------ |
| `standard` | Default Markdown with GFM extensions | Baseline behavior                                      |
| `mkdocs`   | MkDocs / Material for MkDocs         | MD024, MD031, MD033, MD040, MD042, MD046, MD052, MD056 |
| `mdx`      | MDX (JSX in Markdown)                | MD033                                                  |
| `quarto`   | Quarto / RMarkdown                   | MD038, MD040                                           |

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

## Standard Flavor

**Alias**: `gfm`, `github`, `commonmark`

The default flavor. Uses pulldown-cmark which already supports GitHub Flavored Markdown (GFM) extensions:

- Tables
- Task lists
- Strikethrough
- Autolinks

All rules use their default behavior.

## MkDocs Flavor

For projects using [MkDocs](https://www.mkdocs.org/) or [Material for MkDocs](https://squidfunk.github.io/mkdocs-material/).

### Supported Patterns

#### Auto-References

MkDocs autorefs plugin allows shorthand links to documented objects:

```markdown
See [ClassName][] for details.
Use [module.function][] in your code.
```

**Affected rules**: MD042 (empty links), MD052 (reference links)

#### Admonitions

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

#### Content Tabs

Material for MkDocs tab syntax:

```markdown
=== "Tab 1"
    Content for tab 1.

=== "Tab 2"
    Content for tab 2.
```

**Affected rules**: MD046 (code block style)

#### Snippets

MkDocs snippets for including external files:

```markdown
--8<-- "path/to/file.md"
;--8<--
```

**Affected rules**: MD024 (duplicate headings), MD052 (reference links)

#### HTML with Markdown Attribute

Allows `markdown="1"` to enable Markdown processing inside HTML:

```markdown
<div markdown="1">
**This** is processed as Markdown.
</div>
```

**Affected rules**: MD033 (inline HTML)

#### Code Block Title Attribute

MkDocs allows `title=` on fenced code blocks:

````markdown
```python title="example.py"
print("Hello")
```
````

**Affected rules**: MD040 (fenced code language)

#### Table Extensions

MkDocs table handling with extensions like `md_in_html`:

**Affected rules**: MD056 (table column count)

### Rule Behavior Changes

| Rule  | Standard Behavior                | MkDocs Behavior                    |
| ----- | -------------------------------- | ---------------------------------- |
| MD024 | Flag duplicate headings          | Skip headings in snippet sections  |
| MD031 | Require blanks around all fences | Respect admonition/tab context     |
| MD033 | Flag all inline HTML             | Allow `markdown="1"` attribute     |
| MD040 | Require language on code blocks  | Allow `title=` without language    |
| MD042 | Flag empty links `[]()`          | Allow auto-references `[Class][]`  |
| MD046 | Detect code block style globally | Account for admonition/tab context |
| MD052 | Flag undefined references        | Allow auto-references and snippets |
| MD056 | Strict column count              | Handle MkDocs table extensions     |

## MDX Flavor

For projects using [MDX](https://mdxjs.com/) (Markdown with JSX).

### Supported Patterns

#### JSX Components

Capitalized tags are treated as JSX components, not HTML:

```markdown
<Button onClick={handleClick}>Click me</Button>

<Card title="Example">
  Content inside component.
</Card>

<MyCustomComponent prop={value} />
```

**Affected rules**: MD033 (inline HTML)

### Rule Behavior Changes

| Rule  | Standard Behavior    | MDX Behavior                     |
| ----- | -------------------- | -------------------------------- |
| MD033 | Flag all inline HTML | Allow capitalized JSX components |

### Limitations

- ESM imports/exports (`import`, `export`) are recognized but don't affect rule behavior
- JSX expressions `{expression}` are not specially handled
- MDX 2.0+ specific syntax may not be fully supported

## Quarto Flavor

For projects using [Quarto](https://quarto.org/) or RMarkdown for scientific publishing.

### Supported Patterns

#### Cell Options

Quarto code cell options using `#|` syntax:

````markdown
```{python}
#| label: fig-example
#| fig-cap: "Example figure"
import matplotlib.pyplot as plt
plt.plot([1, 2, 3])
```
````

**Affected rules**: MD038 (code spans), MD040 (fenced code language)

#### Executable Code Blocks

Code blocks with language in braces:

````markdown
```{r}
summary(data)
```

```{python}
print("Hello")
```
````

**Affected rules**: MD040 (fenced code language)

### Rule Behavior Changes

| Rule  | Standard Behavior           | Quarto Behavior                          |
| ----- | --------------------------- | ---------------------------------------- |
| MD038 | Check all code spans        | Handle Quarto-specific syntax            |
| MD040 | Standard language detection | Recognize `{language}` and `#\|` options |

### Limitations

- Quarto shortcodes (`{{< >}}`) are not specially handled
- Cross-reference syntax (`@fig-example`) is treated as regular text
- YAML front matter extensions are parsed as standard YAML

## Adding Flavor Support

If you encounter a pattern that rumdl doesn't handle correctly for your documentation system:

1. Check if the pattern is already supported (see tables above)
2. Try configuring the relevant rule to allow the pattern
3. Open an issue with:
   - The Markdown content that triggers a false positive
   - The documentation system and version you're using
   - The expected behavior

## See Also

- [Global Settings](global-settings.md) - Configure flavor globally
- [Per-File Configuration](global-settings.md#per-file-flavor) - Override flavor per file
- [Rules Reference](RULES.md) - Complete rule documentation
