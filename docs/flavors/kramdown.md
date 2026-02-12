# Kramdown Flavor

For content processed by [kramdown](https://kramdown.gettalong.org/), the default Markdown renderer for Jekyll sites. Extends standard flavor with kramdown-specific syntax support including Inline Attribute Lists (IALs), extension blocks, and definition lists.

## Supported Patterns

### Inline Attribute Lists (Block IALs)

Block IALs add attributes to the preceding block element. Standalone IAL lines are recognized as kramdown syntax and skipped by rules that would otherwise flag them:

```markdown
# Heading
{:.highlight #custom-id}

```ruby
puts "Hello"
```
{:.language-ruby .numberLines}

> Blockquote content
{:#special-quote .emphasized}
```

### Span IALs

Span IALs attach attributes to inline elements. MD037 recognizes them to avoid false positives on emphasis spacing:

```markdown
*emphasized text*{:.red}
**bold text**{:#important}
[link](https://example.com){:target="_blank"}
```

### Extension Blocks

Kramdown extension blocks (`{::comment}`, `{::nomarkdown}`, etc.) are treated as non-content regions. Content inside extension blocks is skipped by linting rules:

```markdown
{::comment}
This content is hidden from rendering.
It can contain <div>HTML</div> or other syntax.
{:/comment}

{::nomarkdown}
<div class="custom">
  Raw HTML that kramdown won't process
</div>
{:/nomarkdown}
```

The shorthand close `{:/}` is also supported.

### Options Directive

Kramdown options directives are recognized and skipped:

```markdown
{::options parse_block_html="true" /}
```

### Definition Lists

Definition lists use the `: definition` syntax:

```markdown
Term 1
: Definition 1
: Another definition

Term 2
: Definition 2
```

### Footnotes

Kramdown-style footnote definitions and references:

```markdown
Text with a footnote[^1] reference.

[^1]: Footnote content here.
```

### Abbreviations

Abbreviation definitions:

```markdown
*[HTML]: HyperText Markup Language
*[W3C]: World Wide Web Consortium
```

### End-of-Block Markers

The `^` character on its own line ends the current block:

```markdown
First paragraph
^
Second paragraph starts a new block
```

## Rule Behavior Changes

| Rule  | Standard Behavior                | Kramdown Behavior                                     |
| ----- | -------------------------------- | ----------------------------------------------------- |
| MD022 | Check blanks around headings     | Allow IALs immediately after headings                 |
| MD041 | Check first line is heading      | Skip IALs and extension blocks as preamble            |
| MD051 | Use GitHub anchor style          | Default to kramdown anchor generation                 |

Additionally, the `filtered_lines()` API supports `skip_kramdown_extension_blocks()` for rules that need to skip content inside `{::comment}` and `{::nomarkdown}` blocks.

## Anchor Generation

When kramdown flavor is active, MD051 (link fragment validation) defaults to kramdown's anchor generation algorithm instead of GitHub's. This matches how Jekyll generates heading IDs.

To explicitly configure anchor style:

```toml
[MD051]
anchor-style = "kramdown"       # Pure kramdown style
# anchor-style = "kramdown-gfm" # Jekyll with GFM input
```

## Configuration

```toml
[global]
flavor = "kramdown"
```

The `jekyll` alias is also accepted:

```toml
[global]
flavor = "jekyll"
```

### Auto-Detection

Files with the `.kramdown` extension are automatically detected as kramdown flavor.

For Jekyll projects, add `flavor = "kramdown"` (or `flavor = "jekyll"`) to your `.rumdl.toml`.

## When to Use

Use the kramdown flavor when:

- Building Jekyll sites that use kramdown as the Markdown renderer
- Using kramdown-specific syntax like IALs, ALDs, or extension blocks
- Documents contain `{:.class}` or `{:#id}` attribute syntax
- Using kramdown's `{::comment}` or `{::nomarkdown}` extension blocks

## See Also

- [Flavors Overview](../flavors.md) - Compare all flavors
- [Standard Flavor](standard.md) - Base flavor without kramdown extensions
- [kramdown Documentation](https://kramdown.gettalong.org/)
- [Jekyll Documentation](https://jekyllrb.com/docs/configuration/markdown/)
