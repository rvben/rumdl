---
description: "Lint GitHub Agentic Workflows while preserving frontmatter templates, runtime imports, and conditional control directives."
---

# GitHub Agentic Workflows

**Config name**: `gh-aw`

The `gh-aw` flavor supports Markdown workflow sources compiled by
[GitHub Agentic Workflows](https://github.github.com/gh-aw/). It is preview
support while the upstream format remains in public preview.

## Configure workflow files

GitHub Agentic Workflows use ordinary `.md` files, so rumdl does not
auto-detect this flavor. Assign it explicitly to the workflow directory:

```toml
[per-file-flavor]
".github/workflows/**/*.md" = "gh-aw"
```

For a repository containing only Agentic Workflow Markdown, it can instead be
the global flavor:

```toml
[global]
flavor = "gh-aw"
```

The CLI accepts the same canonical name:

```bash
rumdl check --flavor gh-aw .github/workflows/
```

## Recognized control directives

rumdl recognizes these complete, standalone control lines:

```markdown
{{#if github.event.issue.pull_request}}
{{/if}}
{{#elseif experiments.output_format == "detailed"}}
{{#else-if experiments.output_format == "detailed"}}
{{#else_if experiments.output_format == "detailed"}}
{{elseif experiments.output_format == "detailed"}}
{{else-if experiments.output_format == "detailed"}}
{{else_if experiments.output_format == "detailed"}}
{{#else}}
{{else}}
{{#endif}}
{{#runtime-import ./shared.md}}
{{#runtime-import? ./optional.md}}
{{#import ./legacy.md}}
```

Both `{{/if}}` and `{{#endif}}` close a conditional. Current gh-aw runtimes
also accept the listed `elseif` spellings and the `#else`/`else` fallback
forms. `runtime-import?` is the optional import form. The older `import` helper remains
recognized so lint fixes do not corrupt workflows that have not yet migrated to
`runtime-import`.

Recognition is deliberately exact. A directive name embedded in prose,
unsupported helpers, malformed directives, and GitHub Actions expressions such
as `${{ github.ref }}` remain ordinary Markdown and are linted normally.

## Rule adjustments

| Rule | `gh-aw` behavior |
| ---- | ---------------- |
| [MD034](../md034.md) | Does not report or rewrite URLs and email-like text on a recognized control line. Body prose is still checked. |
| [MD041](../md041.md) | Skips leading control lines when finding the first content heading. A fix can relevel a heading in place but never moves it across a control boundary. |
| [MD057](../md057.md) | Markdown-looking YAML strings are not treated as body links, and output placeholders such as `{run_url}` are not filesystem paths. Broken body links are still reported. Explicit `check-frontmatter` validation remains available for standalone path-shaped values. |

Paragraph reflow already treats standalone template directives as structural
boundaries. The flavor's regression corpus verifies that formatting leaves all
recognized control lines byte-for-byte unchanged and converges in one pass.

## Scope

The flavor makes Markdown linting and formatting safe around gh-aw syntax. It
does not validate workflow frontmatter schemas, evaluate conditionals, resolve
imports, or compile workflows. Use the gh-aw tooling for those operations.

## Learn more

- [Workflow structure](https://github.github.com/gh-aw/reference/workflow-structure/)
- [Templating](https://github.github.com/gh-aw/reference/templating/)
- [Releases and compatibility](https://github.github.com/gh-aw/reference/releases/)
- [Flavors overview](../flavors.md)
