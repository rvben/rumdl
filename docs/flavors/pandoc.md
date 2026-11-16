# Pandoc Flavor

For projects using [Pandoc](https://pandoc.org/) Markdown: academic
papers, books, and reports converted via Pandoc.

The Pandoc flavor is the foundation that the [Quarto](quarto.md) flavor
extends. Use Pandoc when you write Pandoc Markdown directly without using
the Quarto toolchain.

## Supported Patterns

### Fenced Divs

Pandoc fenced div syntax with attributes:

```markdown
::: {.note}
A note block.
:::

::: {#myid .class}
Generic div.
:::
```

**Affected rules**: MD031 (blanks around fences), MD022 (blanks around
headings), MD032 (blanks around lists)

### Pandoc Attribute Lists

Attribute syntax `{#id .class key="value"}` on headings, images, and code:

```markdown
# My Heading {#myid .my-class}

![Image](path.png){.my-image width="50%"}

`code`{.python}
```

### Bracketed Spans

Spans of inline content with attributes:

```markdown
This is [some text]{.smallcaps} and [more]{.highlight}.
```

**Affected rules**: MD037 (no space in emphasis)

### Citations

Pandoc citation syntax:

```markdown
According to @smith2020, the results show...

Multiple citations [@smith2020; @jones2021] confirm this.

Suppress author: [-@smith2020] showed...

In-text: @smith2020 [p. 42] argues that...
```

**Affected rules**: MD042 (empty links), MD051 (link fragments),
MD052 (reference links)

### Inline Footnotes

```markdown
Here is a sentence^[with an inline footnote] and some more text.
```

**Affected rules**: MD042, MD052

### Implicit Header References

A bracketed phrase whose Pandoc slug matches an existing heading:

```markdown
# My Section

Refer to [My Section] for details.
```

**Affected rules**: MD042, MD051, MD052

#### Duplicate Heading Disambiguation

Pandoc's `auto_identifiers` extension assigns the second heading sharing the
same slug `<base>-1`, the third `<base>-2`, etc. MD051 resolves both forms:

```markdown
# A.

first

# A.

second

See [first](#a.) and [second](#a.-1).
```

### Example Lists

```markdown
(@) The first example.
(@good) The second example.
(@) The third example.

As shown in (@good), this approach works.
```

**Affected rules**: MD029 (ordered list prefix), MD042, MD052

### Definition Lists

```markdown
Term
:   Definition.

Another term
:   Another definition.
```

### Raw Format Blocks

````markdown
```{=html}
<div class="raw">Embedded HTML</div>
```

```{=latex}
\section{Raw LaTeX}
```
````

**Affected rules**: MD040 (fenced code language)

### Code Block Attributes

Pandoc fenced code blocks may declare their language via attribute syntax.
The first `.class` inside the attribute block is the language; additional
classes, ids, and key=value pairs are decoration.

````markdown
```{.python}
print("hi")
```

```{.haskell .numberLines}
main = putStrLn "hi"
```

```{#snippet .python startFrom="10"}
print(1)
```
````

A brace block with no `.class` (e.g. `{#myid}` or `{}`) declares no language
and is still flagged by MD040.

**Affected rules**: MD040 (fenced code language)

### Math

```markdown
Inline math $\alpha + \beta$ and display math:

$$
E = mc^2
$$
```

**Affected rules**: MD037, MD049, MD050

### Subscripts and Superscripts

```markdown
H~2~O and 2^10^.
```

**Affected rules**: MD037

### Inline Code Attributes

```markdown
Use `print()`{.python} to display.
```

The attribute block lives outside the code span, so MD038 still flags
genuine leading or trailing whitespace inside the backticks
(e.g. `` ` print()`{.python} ``).

### Pipe Tables with Captions

```markdown
| col1 | col2 |
|------|------|
| a    | b    |

: Caption for the table.
```

### Grid Tables

```markdown
+---------+---------+
| Header  | Header  |
+=========+=========+
| Cell    | Cell    |
+---------+---------+
```

**Affected rules**: MD055, MD056, MD058, MD060, MD075

### Multi-Line Tables

```markdown
-------------------------------------------------------------
 Centered   Default           Right Left
  Header    Aligned         Aligned Aligned
----------- ------- --------------- -------------------------
   First    row                12.0 Example of a row that
                                    spans multiple lines.

  Second    row                 5.0 Another row.
-------------------------------------------------------------
```

**Affected rules**: MD055, MD056, MD058, MD060, MD075

### Line Blocks

```markdown
| The Lord of the Rings
| by J.R.R. Tolkien
```

**Affected rules**: MD034 (no bare URLs), MD042, MD056

### Multi-Block YAML Metadata

Pandoc allows multiple YAML metadata blocks anywhere in a document:

```markdown
---
title: My Document
---

# Heading

---
author: Jane Doe
---
```

## Rule Behavior Changes

| Rule  | Standard Behavior           | Pandoc Behavior                                                      |
| ----- | --------------------------- | -------------------------------------------------------------------- |
| MD022 | Blanks around headings      | Treat `:::` div markers as transparent (don't require extra blanks)  |
| MD029 | Validate ordered prefixes   | Skip `(@)` / `(@label)` example markers                              |
| MD031 | Blanks around fences        | Allow Pandoc fenced divs without extra blanks                        |
| MD032 | Blanks around lists         | Treat `:::` div markers as transparent                               |
| MD034 | Flag all bare URLs          | Skip URLs inside line blocks and metadata blocks                     |
| MD037 | Check emphasis spacing      | Skip bracketed spans and sub/superscripts                            |
| MD040 | Standard language detection | Recognize `{=format}` raw-format and `{.class …}` code-attribute declarations |
| MD042 | Flag empty links            | Skip citations, footnotes, example refs, implicit header refs        |
| MD051 | Validate link fragments     | Resolve fragments against Pandoc heading slugs                       |
| MD052 | Flag undefined references   | Skip citations, footnotes, example refs, implicit header refs        |

### Parser-Level Exclusions

Some Pandoc constructs are excluded by rumdl's parser for **all** flavors,
not by a Pandoc-specific guard. These rules therefore behave identically
under Pandoc and Standard:

- **Math blocks** (`$$...$$`, `$...$`): MD037, MD049, MD050 skip math
  contexts universally.
- **Grid tables** (`+---+---+`), **multi-line tables**
  (`---------- -------`), **line blocks** (`| line\n| line`), and
  **pipe-table captions** (`: caption`): MD055, MD056, MD058, MD060,
  MD075 iterate the table-block scanner, which only recognizes pipe
  tables with `|`-bounded delimiter rows. The other table shapes never
  enter the iteration source, under any flavor.

## Limitations

- Smart-typography linting (smart quotes, em-dashes, ellipses, emoji
  shortcodes, `\` hard line breaks) is out of scope: these are rendering
  features that do not trigger rumdl rules.
- `[TOC]` placeholder syntax is **not** Pandoc syntax (Pandoc generates
  table-of-contents via the `--toc` flag) and is not recognized.
- Pandoc filter / Lua-filter / template execution is out of scope; rumdl
  is a linter.

## Configuration

```toml
[global]
flavor = "pandoc"
```

Or per-file:

```toml
[per-file-flavor]
"papers/**/*.md" = "pandoc"
```

Pandoc files use `.md`, so there is no automatic file-extension
detection; opt in explicitly.

## When to Use

Use the Pandoc flavor when:

- You write Markdown that you process via the `pandoc` CLI.
- You use Pandoc citations, fenced divs, attribute lists, or other Pandoc
  extensions.
- You write academic papers, books, or reports in Markdown.

If you use the Quarto toolchain, use the [Quarto flavor](quarto.md)
instead: it includes everything Pandoc does plus Quarto-specific syntax
(executable code blocks, shortcodes, `#|` cell options).

## See Also

- [Flavors Overview](../flavors.md): compare all flavors
- [Quarto Flavor](quarto.md): Pandoc + Quarto extensions
- [Pandoc Documentation](https://pandoc.org/MANUAL.html#pandocs-markdown)
