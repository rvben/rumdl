# MyST Flavor

For projects using [MyST Markdown](https://mystmd.org/) (Markedly Structured Text) —
the Markdown flavor used by Jupyter Book, Sphinx via MyST-Parser, and the MyST
document engine.

**Config name**: `myst`
**Aliases**: `mystmd`

## Supported Patterns

### Colon Directives

MyST uses `:::` (or more colons) as structural containers for directives:

```markdown
:::{note}
This is a note admonition with **Markdown** content.
:::
```

Nesting is supported by increasing the colon count:

```markdown
::::{warning}
Outer directive.

:::{tip}
Inner directive.
:::
::::
```

The opener is 3+ colons immediately followed by `{name}` (e.g., `:::{note}`,
`::::{warning}`). The closer is the same number of colons (or more) without a
`{name}` suffix.

**Treated as Markdown content**: The body of colon directives is linted as
normal Markdown prose. Rules that check prose (MD013, MD034, link validation)
still fire inside colon directives.

### Backtick Directives

MyST also uses backtick code fences with a `{name}` info string:

````markdown
```{note}
This is also a note directive.
```
````

For **content-bearing directives** (note, warning, tip, hint, important,
caution, danger, admonition, seealso, topic, sidebar, margin, exercise,
solution, dropdown, tab-item, grid, card, figure), the body is linted as
Markdown — `in_code_block` is cleared.

For **code-bearing directives** (code-cell, code-block, raw, eval-rst,
literalinclude), the body remains treated as a code block and is not linted.

### Directive Options

Directives can have options specified as `:key: value` lines immediately after
the opener:

````markdown
```{figure} image.png
:alt: An image description
:width: 80%

Caption text with **Markdown** formatting.
```
````

Option lines are part of the directive and are not linted as prose.

### Roles (Inline Directives)

MyST roles provide inline semantic markup:

```markdown
See {ref}`my-label` for details.
The equation {math}`E=mc^2` is famous.
```

The pattern is `{rolename}` followed by backtick-delimited content. These are
not flagged as malformed inline code.

### Comments

MyST uses `%` for single-line comments:

```markdown
% This is a comment that won't appear in output.
Regular paragraph text.
```

Comment lines are excluded from line-length checks.

## Rule Behavior Changes

| Rule  | Standard Behavior                   | MyST Behavior                                                  |
| ----- | ----------------------------------- | -------------------------------------------------------------- |
| MD013 | Check line length everywhere        | Skip `%` comment lines                                         |
| MD031 | Blanks around backtick/tilde fences | Also enforce blanks around colon directives (`:::{name}`)      |
| MD038 | Flag spaces in inline code          | Skip role syntax (`{role}`content``)                           |
| MD040 | Require language on code fences     | Accept `{name}` as valid directive info string                 |
| MD046 | Detect code block style             | Skip colon directives from style detection                     |
| MD048 | Detect fence style                  | Skip colon/backtick directives from fence style detection      |

## Content vs Code Directives

rumdl distinguishes between directives whose body contains Markdown and those
whose body contains code:

| Category | Directives                                                                                                     | Body linted? |
| -------- | -------------------------------------------------------------------------------------------------------------- | ------------ |
| Content  | note, warning, tip, hint, important, caution, danger, admonition, seealso, topic, sidebar, margin, exercise, solution, dropdown, tab-item, grid, card, figure | Yes          |
| Code     | code-cell, code-block, raw, eval-rst, literalinclude                                                           | No           |

Unrecognized directive names default to code-block behavior (body not linted).

## Configuration

```toml
[global]
flavor = "myst"
```

Or using the alias:

```toml
[global]
flavor = "mystmd"
```

Or per-file:

```toml
[per-file-flavor]
"docs/**/*.md" = "myst"
```

## CLI Usage

```bash
rumdl check --flavor myst docs/
rumdl check --flavor mystmd docs/
```

## When to Use

Use the MyST flavor when:

- You write Markdown for Jupyter Book or Sphinx with MyST-Parser.
- Your files contain `:::{directive}` or `` ```{directive} `` blocks.
- You use role syntax like `{ref}`label`` or `{math}`expression``.
- You are getting false positives from MD040 (missing language) on directive fences.

Do **not** use this flavor for Pandoc projects — Pandoc uses `:::` with
different semantics (fenced divs without `{name}`). Use
[pandoc](pandoc.md) for Pandoc projects.

## See Also

- [Flavors Overview](../flavors.md) — compare all flavors
- [Pandoc Flavor](pandoc.md) — Pandoc fenced divs (transparent `:::` blocks)
- [MyST Markdown documentation](https://mystmd.org/guide)
