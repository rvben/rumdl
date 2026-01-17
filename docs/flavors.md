# Markdown Flavors

rumdl supports multiple Markdown flavors to accommodate different documentation systems. Each flavor adjusts specific rule behavior where that system differs from standard Markdown.

## Quick Reference

| Flavor     | Use Case                             | Rules Affected                                                              |
| ---------- | ------------------------------------ | --------------------------------------------------------------------------- |
| `standard` | Default Markdown with GFM extensions | Baseline behavior                                                           |
| `gfm`      | GitHub Flavored Markdown             | MD033 (security tags), MD034 (extended autolinks)                           |
| `mkdocs`   | MkDocs / Material for MkDocs         | MD024, MD031, MD033, MD038, MD040, MD042, MD046, MD049, MD050, MD052, MD056 |
| `mdx`      | MDX (JSX in Markdown)                | MD013, MD033, MD037, MD039, MD044, MD049                                    |
| `quarto`   | Quarto / RMarkdown                   | MD034, MD037, MD038, MD040, MD042, MD049, MD050, MD051, MD052               |

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

**Alias**: `commonmark`

The default flavor. Uses pulldown-cmark which already supports common GFM extensions:

- Tables
- Task lists
- Strikethrough
- Autolinks

All rules use their default behavior.

## GFM Flavor

**Alias**: `github`

For content targeting GitHub specifically. Extends standard flavor with:

### Security-Sensitive HTML Tags

MD033 is configured to flag potentially dangerous HTML elements that GitHub may render:

```markdown
<!-- These are flagged in GFM mode -->
<script>alert('xss')</script>
<iframe src="..."></iframe>
<form action="...">...</form>
```

### Extended Autolinks

MD034 recognizes additional URL schemes from the GFM autolink extension:

- `xmpp:` - XMPP URIs (e.g., `xmpp:user@example.com`)
- Extended email detection with proper protocol handling

### Rule Behavior Changes

| Rule  | Standard Behavior              | GFM Behavior                              |
| ----- | ------------------------------ | ----------------------------------------- |
| MD033 | Flag all inline HTML           | Warn on security-sensitive tags           |
| MD034 | Detect standard URLs/emails    | Include xmpp: and extended autolinks      |

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

#### mkdocstrings Blocks

mkdocstrings autodoc syntax is recognized:

```markdown
::: module.path
    options:
        show_source: true

::: package.submodule.Class
```

**Affected rules**: MD031 (blanks around fences), MD038 (code spans)

#### Extended Markdown Syntax

MkDocs extensions for special formatting:

```markdown
++inserted text++     <!-- ins extension -->
==marked text==       <!-- mark extension -->
^^superscript^^       <!-- caret extension -->
~subscript~           <!-- tilde extension -->
[[keyboard keys]]     <!-- keys extension -->
```

**Affected rules**: MD038 (code spans), MD049 (emphasis style), MD050 (strong style)

### Rule Behavior Changes

| Rule  | Standard Behavior                | MkDocs Behavior                          |
| ----- | -------------------------------- | ---------------------------------------- |
| MD024 | Flag duplicate headings          | Skip headings in snippet sections        |
| MD031 | Require blanks around all fences | Respect admonition/tab/mkdocstrings      |
| MD033 | Flag all inline HTML             | Allow `markdown="1"` attribute           |
| MD038 | Flag spaces in code spans        | Handle keys/caret/mark syntax            |
| MD040 | Require language on code blocks  | Allow `title=` without language          |
| MD042 | Flag empty links `[]()`          | Allow auto-references `[Class][]`        |
| MD046 | Detect code block style globally | Account for admonition/tab context       |
| MD049 | Check emphasis consistency       | Handle mark/inserted syntax              |
| MD050 | Check strong consistency         | Handle mark/caret/tilde syntax           |
| MD052 | Flag undefined references        | Allow auto-references and snippets       |
| MD056 | Strict column count              | Handle MkDocs table extensions           |

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

#### JSX Attributes

Elements with JSX-specific attributes are recognized as JSX, not HTML:

```markdown
<!-- These use JSX attributes and are not flagged -->
<div className="container">...</div>
<label htmlFor="input-id">Label</label>
<button onClick={handler}>Click</button>
<input onChange={handleChange} />
```

JSX-specific attributes include: `className`, `htmlFor`, `onClick`, `onChange`, `onSubmit`, `dangerouslySetInnerHTML`, and other camelCase event handlers.

**Affected rules**: MD033 (inline HTML)

#### JSX Expressions

JSX expressions and comments are recognized:

```markdown
The value is {computedValue}.

{/* This is a JSX comment */}

<Component prop={expression} />
```

**Affected rules**: MD037 (no space in emphasis), MD039 (no space in links), MD044 (proper names), MD049 (emphasis style)

#### ESM Imports/Exports

MDX 2.0+ supports ESM anywhere in the document:

```markdown
import { Component } from './component'
export const metadata = { title: 'Page' }

# Heading

<Component />
```

**Affected rules**: MD013 (line length - ESM lines can be longer)

### Rule Behavior Changes

| Rule  | Standard Behavior        | MDX Behavior                             |
| ----- | ------------------------ | ---------------------------------------- |
| MD013 | Check all line lengths   | Allow longer ESM import/export lines     |
| MD033 | Flag all inline HTML     | Allow JSX components and JSX attributes  |
| MD037 | Check emphasis spacing   | Skip JSX expressions                     |
| MD039 | Check link spacing       | Skip JSX expressions                     |
| MD044 | Check proper names       | Skip inside JSX expressions              |
| MD049 | Check emphasis style     | Skip JSX expressions                     |

### Limitations

- Complex nested JSX expressions may not be fully parsed
- MDX compile-time expressions are treated as runtime expressions

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

#### Pandoc Citations

Quarto supports Pandoc citation syntax:

```markdown
According to @smith2020, the results show...

Multiple citations [@smith2020; @jones2021] confirm this.

Suppress author: [-@smith2020] showed...

In-text: @smith2020 [p. 42] argues that...
```

**Affected rules**: MD042 (empty links), MD051 (link fragments), MD052 (reference links)

#### Shortcodes

Quarto/Hugo shortcodes are recognized:

```markdown
{{< video https://youtube.com/watch?v=xxx >}}

{{< include _content.qmd >}}

{{% callout note %}}
This is a callout.
{{% /callout %}}
```

**Affected rules**: MD034 (bare URLs - URLs in shortcodes not flagged), MD042, MD051, MD052

#### Div Blocks and Callouts

Quarto div syntax with attributes:

```markdown
::: {.callout-note}
This is a note callout.
:::

::: {.column-margin}
Marginal content here.
:::

::: {#fig-layout layout-ncol=2}
![Caption A](imageA.png)

![Caption B](imageB.png)
:::
```

**Affected rules**: MD031 (blanks around fences)

#### Math Blocks

LaTeX math blocks are recognized and excluded from emphasis checking:

```markdown
$$
E = mc^2
$$

Inline math $\alpha + \beta$ is also recognized.
```

**Affected rules**: MD037 (no space in emphasis), MD049 (emphasis style), MD050 (strong style)

### Rule Behavior Changes

| Rule  | Standard Behavior           | Quarto Behavior                          |
| ----- | --------------------------- | ---------------------------------------- |
| MD034 | Flag all bare URLs          | Skip URLs inside shortcodes              |
| MD037 | Check emphasis spacing      | Skip math blocks                         |
| MD038 | Check all code spans        | Handle Quarto-specific syntax            |
| MD040 | Standard language detection | Recognize `{language}` and `#\|` options |
| MD042 | Flag empty links            | Skip citations and shortcodes            |
| MD049 | Check emphasis consistency  | Skip math blocks                         |
| MD050 | Check strong consistency    | Skip math blocks                         |
| MD051 | Validate link fragments     | Skip citations and shortcodes            |
| MD052 | Flag undefined references   | Skip citations and shortcodes            |

### Limitations

- Complex Pandoc filter syntax may not be fully recognized
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
