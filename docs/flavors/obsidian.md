# Obsidian Flavor

For content created in [Obsidian](https://obsidian.md/), the popular knowledge management application. Extends standard flavor with Obsidian-specific syntax support.

## Supported Patterns

### Tags with Hash Syntax

MD018 recognizes Obsidian's tag syntax to avoid false positives:

```markdown
This is a #tag not a malformed heading.
Multiple #tags #in-one-line are supported.
Nested #tags/with/hierarchy work too.
```

### Callouts (Admonitions)

MD028 recognizes Obsidian callout syntax to prevent false positives when blank lines appear between callout blocks:

```markdown
> [!NOTE]
> This is a note callout.

> [!WARNING]
> This is a warning.

> [!TIP]+ Foldable (expanded by default)
> Tip content here.

> [!INFO]- Collapsed by default
> Info content here.
```

Supported callout types include:
- `note`, `abstract`, `summary`, `tldr`
- `info`, `todo`, `tip`, `hint`, `important`
- `success`, `check`, `done`
- `question`, `help`, `faq`
- `warning`, `caution`, `attention`
- `failure`, `fail`, `missing`
- `danger`, `error`, `bug`
- `example`, `quote`, `cite`

Custom callout types are also supported.

### Comments

Rules skip Obsidian comment syntax to avoid false positives inside comments:

```markdown
This is visible %%this is hidden%% and visible again.

%%
Multi-line
comments are
also supported
%%
```

Rules affected: MD011, MD012, MD034, MD037, MD044, MD049, MD061, MD064, MD069

### Highlights

MD037 and other emphasis-related rules recognize Obsidian highlight syntax:

```markdown
This is ==highlighted text== in a sentence.
```

### Dataview Inline Queries

MD038 recognizes Dataview plugin inline query syntax:

```markdown
The file name is `= this.file.name`.
Dynamic content: `$= dv.current().field`.
```

### Dataview Inline Fields

MD011 recognizes Dataview inline field syntax to prevent false positives:

```markdown
(status:: active)[link text]
(author:: John Doe)[read more]
(date:: 2024-01-01)[view]
```

These patterns look like reversed links but are valid Dataview inline field syntax.

### Extended Task Checkboxes

MD064 recognizes extended task checkbox syntax beyond the standard `[ ]`, `[x]`, and `[X]`:

```markdown
- [/] In progress
- [-] Cancelled
- [>] Deferred
- [<] Scheduled
- [?] Question
- [!] Important
- [*] Star/highlight
```

These extended checkboxes are commonly used in Obsidian with plugins like Tasks or custom CSS.

### Templater Syntax

MD033 correctly ignores Templater plugin syntax (not flagged as inline HTML):

```markdown
<% tp.date.now() %>
<%* javascript code %>
<%+ expression %>
<% tp.file.title %>
```

## Rule Behavior Changes

| Rule  | Standard Behavior              | Obsidian Behavior                                |
| ----- | ------------------------------ | ------------------------------------------------ |
| MD018 | Flag `#text` without space     | Allow `#tag` syntax                              |
| MD028 | Flag blanks between blockquotes| Recognize callout blocks                         |
| MD033 | Flag inline HTML               | Ignore Templater `<% %>` syntax                  |
| MD037 | Check emphasis spacing         | Recognize `==highlight==` syntax                 |
| MD038 | Check code span spacing        | Allow `= ` and `$= ` Dataview prefixes           |
| MD011 | Check reversed links           | Skip `%%comments%%` and `(field:: value)` patterns |
| MD012 | Check multiple blanks          | Skip content in `%%comments%%`                   |
| MD034 | Check bare URLs                | Skip content in `%%comments%%`                   |
| MD044 | Check proper names             | Skip content in `%%comments%%`                   |
| MD049 | Check emphasis style           | Skip content in `%%comments%%`                   |
| MD061 | Check link fragments           | Skip content in `%%comments%%`                   |
| MD064 | Check multiple consecutive spaces | Skip `%%comments%%`, allow extended checkboxes `[/]`, `[-]`, etc. |
| MD069 | Check reference links          | Skip content in `%%comments%%`                   |

## Configuration

```toml
[global]
flavor = "obsidian"
```

## When to Use

Use the Obsidian flavor when:

- Linting notes from your Obsidian vault
- Using Obsidian-specific syntax like callouts or comments
- Using the Dataview plugin for dynamic content
- Using the Templater plugin for templates
- Using Obsidian's tag syntax extensively

## Plugin Compatibility

The Obsidian flavor is designed to work with common Obsidian plugins:

- **Dataview**: Inline queries with `= ` and `$= ` prefixes
- **Templater**: Template syntax with `<% %>` delimiters
- **Core plugins**: Callouts, tags, wiki-links, block references

## See Also

- [Flavors Overview](../flavors.md) - Compare all flavors
- [Standard Flavor](standard.md) - Base flavor without Obsidian extensions
- [Obsidian Help - Callouts](https://help.obsidian.md/callouts)
- [Obsidian Help - Tags](https://help.obsidian.md/tags)
- [Dataview Documentation](https://blacksmithgu.github.io/obsidian-dataview/)
- [Templater Documentation](https://silentvoid13.github.io/Templater/)
