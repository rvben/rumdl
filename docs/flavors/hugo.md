# Hugo Flavor

For content processed by [Hugo](https://gohugo.io/), the static site generator. Hugo renders Markdown with [Goldmark](https://github.com/yuin/goldmark), which supports GitHub Flavored Markdown plus [Markdown attributes](https://gohugo.io/content-management/markdown-attributes/) (block attribute lists). This flavor extends the standard flavor by recognizing those attribute lists so the blanks-around rules do not flag them as content.

## Supported Patterns

### Block Attribute Lists (Markdown Attributes)

Hugo/Goldmark block attributes add HTML attributes to the preceding block. They must sit on the line directly after the block they modify, with no blank line in between:

````markdown
This is a table:

| Column 1 | Column 2 |
|:---------|:---------|
| Row 1    | Row 1    |
{class="a" id="b"}

# Heading
{class="anchor"}

```rust
let x = 1;
```
{class="highlight"}
````

Because the attribute list must be hard against the block, rules that require blank lines around blocks would otherwise flag it. Under the Hugo flavor these standalone attribute lines are recognized and skipped.

Both the bare Goldmark form (`{class="a" id="b"}`) and the kramdown-style IAL form (`{:.class}`, `{:#id}`) are recognized.

## Rule Behavior Changes

| Rule  | Standard Behavior                        | Hugo Behavior                                                    |
| ----- | ---------------------------------------- | --------------------------------------------------------------- |
| MD022 | Require blank lines around headings      | Allow a block attribute list immediately after a heading        |
| MD031 | Require blank lines around fenced code   | Allow a block attribute list immediately after a fenced block   |
| MD058 | Require blank lines around tables        | Allow a block attribute list immediately after a table          |

In plain CommonMark, a line such as `{class="a"}` is literal text that legitimately needs surrounding blank lines, so this behavior is only enabled under flavors that support attribute lists (Hugo, MkDocs, kramdown).

## Configuration

```toml
[global]
flavor = "hugo"
```

The `goldmark` alias is also accepted:

```toml
[global]
flavor = "goldmark"
```

## When to Use

Use the Hugo flavor when:

- Authoring content for a Hugo site rendered by Goldmark
- Documents use Markdown attributes (`{class="a"}`) attached to tables, headings, or code blocks

## See Also

- [Flavors Overview](../flavors.md) - Compare all flavors
- [Standard Flavor](standard.md) - Base flavor without attribute-list support
- [Hugo Markdown Attributes](https://gohugo.io/content-management/markdown-attributes/)
- [Goldmark](https://github.com/yuin/goldmark)
