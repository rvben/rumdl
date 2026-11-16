# Azure DevOps Flavor

For projects hosted on [Azure DevOps](https://azure.microsoft.com/en-us/products/devops/) wikis
or repositories that use Azure DevOps Markdown.

**Config name**: `azure_devops`  
**Aliases**: `azure`, `ado`

## Supported Patterns

### Colon Code Fences (Mermaid and other diagrams)

Azure DevOps uses `:::` as a code fence marker for Mermaid diagrams and other
block content:

```markdown
::: mermaid
sequenceDiagram
    Christie->>Josh: Hello Josh, how are you?
    Josh-->>Christie: Great!
:::
```

The opener is `:::` followed by at least one non-whitespace character (the
content type). The closer is a bare `:::`. Up to three leading spaces are
allowed on both lines; four or more leading spaces disqualify the opener.

**Treated as opaque code blocks**: All content between the opener and closer
is treated as a code block. Lint rules that inspect prose (MD013 for line
length, MD034 for bare URLs, link validation, and all similar rules) do not
fire inside colon fences.

**Affected rules**: MD013, MD031, MD034, MD046, MD048, and any rule that
skips code block content.

#### Opener syntax

| Input                            | Treated as opener?                                        |
| -------------------------------- | --------------------------------------------------------- |
| `::: mermaid`                    | Yes                                                       |
| `:::mermaid`                     | Yes (no space between `:::` and type)                     |
| `<1-3 spaces>::: mermaid`        | Yes (0-3 leading spaces allowed)                          |
| `<4 spaces>::: mermaid`          | No (4 spaces = CommonMark indented code block)            |
| `<tab>::: mermaid`               | No (tab-indented)                                         |
| `:::` (no type after)            | No (bare `:::` without a type is a closer, not an opener) |

#### MD031 enforcement

MD031 (blanks around fences) enforces blank lines before and after colon
fences, the same as for backtick/tilde fences:

```markdown
<!-- bad: missing blank lines -->
Some text
::: mermaid
diagram
:::
More text

<!-- good: blank lines present -->
Some text

::: mermaid
diagram
:::

More text
```

## Differences from Pandoc

Azure DevOps `:::lang` and Pandoc `:::` use the same opening syntax, but with
different semantics:

| Platform     | `:::` semantics                           | Content linted? |
| ------------ | ----------------------------------------- | --------------- |
| Azure DevOps | Opaque code fence (like a backtick fence) | No              |
| Pandoc       | Fenced div (transparent Markdown block)   | Yes             |

When using the `azure_devops` flavor, Pandoc div detection is not active.
When using the `pandoc` flavor, colon code fence suppression is not active.

## Rule Behavior Changes

| Rule  | Standard Behavior                   | Azure DevOps Behavior                             |
| ----- | ----------------------------------- | ------------------------------------------------- |
| MD013 | Check line length everywhere        | Skip lines inside colon fences                    |
| MD031 | Blanks around backtick/tilde fences | Also enforce blanks around `:::lang` fences       |
| MD034 | Flag all bare URLs                  | Skip URLs inside colon fences                     |
| MD046 | Detect code block style             | Ignore backtick/tilde markers inside colon fences |
| MD048 | Detect fence style                  | Ignore backtick/tilde markers inside colon fences |

All rules that respect `in_code_block` automatically skip colon fence content.

## Configuration

```toml
[global]
flavor = "azure_devops"
```

Or using an alias:

```toml
[global]
flavor = "ado"
```

Or per-file:

```toml
[per-file-flavor]
"wiki/**/*.md" = "azure_devops"
```

Azure DevOps wiki files use plain `.md` extensions, so there is no automatic
file-extension detection. Opt in explicitly.

## CLI Usage

```bash
rumdl check --flavor azure_devops docs/
rumdl check --flavor azure docs/
rumdl check --flavor ado docs/
```

## When to Use

Use the Azure DevOps flavor when:

- You write Markdown for an Azure DevOps wiki.
- Your files contain `:::mermaid` or `:::` blocks that are treated as opaque diagrams.
- You are getting false positives from MD013 or link rules inside diagram blocks.

Do **not** use this flavor for Pandoc, Docusaurus, or VuePress projects: those
platforms use `:::` with different (non-opaque) semantics. Use
[pandoc](pandoc.md) for Pandoc projects or `standard` for Docusaurus/VuePress.

## See Also

- [Flavors Overview](../flavors.md): compare all flavors
- [Pandoc Flavor](pandoc.md): Pandoc fenced divs (transparent `:::` blocks)
- [Azure DevOps Markdown documentation](https://learn.microsoft.com/en-us/azure/devops/project/wiki/wiki-markdown-guidance)
