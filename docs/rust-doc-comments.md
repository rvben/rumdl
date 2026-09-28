---
description: "Lint and format the Markdown inside Rust doc comments (`///` and `//!`) in .rs files, with fixes written back behind the original comment prefixes."
---

# Rust Doc Comments

rumdl lints the Markdown inside Rust doc comments. Each `///` and `//!` block
in a `.rs` file is checked as Markdown, rules that clash with rustdoc
conventions are skipped, and `rumdl fmt` writes fixes back behind the original
comment prefixes without touching the Rust code.

Given this `src/lib.rs`:

```rust
//! #Parsing utilities
//! Helpers for  reading input.

/// Parses `input`.
/// # Errors
/// Returns [`ParseError`] when `input` is empty.
pub fn parse(input: &str) -> Result<(), ParseError> {
    todo!()
}
```

`rumdl check src/lib.rs` reports positions in the `.rs` file:

```text
src/lib.rs:1:6: [MD018] No space after # in heading
src/lib.rs:2:16: [MD064] Multiple consecutive spaces (2) found
src/lib.rs:5:5: [MD022] Expected 1 blank line above heading
src/lib.rs:5:5: [MD022] Expected 1 blank line below heading
```

and `rumdl fmt src/lib.rs` rewrites the doc comments:

```rust
//! # Parsing utilities
//!
//! Helpers for reading input.

/// Parses `input`.
///
/// # Errors
///
/// Returns [`ParseError`] when `input` is empty.
pub fn parse(input: &str) -> Result<(), ParseError> {
    todo!()
}
```

## Enabling it

A plain `rumdl check .` discovers Markdown files only, so Rust files have to
be selected.

### On the command line

A file named on the command line is always linted:

```bash
rumdl check src/lib.rs
rumdl fmt src/lib.rs src/parser.rs
```

A directory named on the command line is searched for Markdown files only, so
`rumdl check src/` finds no Rust files. For a run over a tree, `--include`
takes comma-separated patterns:

```bash
rumdl check . --include "**/*.md,src/**/*.rs"
```

Standard input works when `--stdin-filename` names a `.rs` file:

```bash
rumdl check --stdin --stdin-filename src/lib.rs < src/lib.rs
```

### In your configuration

To lint doc comments on every run, add `.rs` files to
[`include`](global-settings.md#include):

```toml
[global]
include = ["**/*.md", "**/*.rs"]
```

!!! warning "`include` replaces the default file selection"
    Without `include`, rumdl lints every Markdown extension it knows (`.md`,
    `.markdown`, `.mdx`, `.qmd`, `.rmd` and others). Once `include` is set,
    only files matching it are linted, so the example above stops checking
    `.mdx` or `.markdown` files. List a pattern for each extension your
    project uses. The same applies to `--include`.

### With pre-commit

The `rumdl-check` and `rumdl-fmt` hooks only receive Markdown files. To pass
Rust files to them as well, override both `types` and `types_or`:

```yaml
repos:
  - repo: https://github.com/rvben/rumdl-pre-commit
    rev: vX.Y.Z
    hooks:
      - id: rumdl-check
        types: [file]
        types_or: [markdown, rust]
```

Setting `types_or` alone is not enough: pre-commit requires a file to match
both, and the hook's own `types: [markdown]` still excludes `.rs` files.

## What is linted

- Line doc comments: outer `///` and inner `//!`.
- Consecutive lines of the same kind form one block, and each block is linted
  as a standalone Markdown document. A non-doc line, or a switch between
  `///` and `//!`, starts a new block.
- `////` is a regular comment and is ignored, as are ordinary `//` comments
  and the Rust code itself.
- `///content` (no space), `/// content` and `///<tab>content` are all
  recognized, as rustdoc accepts them.

## Rules skipped in doc comments

Some rules conflict with rustdoc conventions and never run on doc comments:

| Rule              | Why it is skipped                                                                           |
| ----------------- | ------------------------------------------------------------------------------------------- |
| [MD025](md025.md) | Several H1 headings (`# Errors`, `# Examples`, `# Safety`) are standard in rustdoc          |
| [MD033](md033.md) | HTML such as `<div class="warning">` is rustdoc syntax for warning blocks                   |
| [MD040](md040.md) | Rustdoc treats an unlabeled code block as Rust                                              |
| [MD041](md041.md) | A doc block is not a standalone document, so it need not start with a heading               |
| [MD047](md047.md) | Same reason: a doc block does not end a file                                                |
| [MD051](md051.md) | Anchors such as `#method.bar` and `#structfield.name` are rustdoc targets, not headings     |
| [MD052](md052.md) | Intra-doc links such as `[crate::io]` are resolved by rustdoc, not by reference definitions |
| [MD054](md054.md) | The shortcut form `[crate::module]` is the canonical intra-doc link syntax                  |

To turn off other rules for Rust files only, use
[`per-file-ignores`](global-settings.md#per-file-ignores):

```toml
[per-file-ignores]
"**/*.rs" = ["MD013"]
```

## Line length

[MD013](md013.md) measures the Markdown text of each line, after the
indentation and the `///` or `//!` prefix are removed. A doc comment on a
method indented by four spaces can therefore run eight columns past
`line-length` in the source file before it is flagged. Code blocks inside doc
comments are not checked for length, since rustfmt formats that code.

## Fixing

`rumdl fmt` and `rumdl check --fix` fix the Markdown in each block and restore
every line's indentation and comment prefix. Lines a fix adds, such as the
blank line MD022 inserts around a heading, get the block's indentation and a
bare `///` or `//!`.

[Code block tools](code-block-tools.md) do not run on code blocks inside doc
comments.

## Limitations

- Only lowercase `.rs` files are read this way.
- Block doc comments (`/** ... */` and `/*! ... */`) and `#[doc = "..."]`
  attributes are not extracted.
- The [language server](lsp.md) and editor extensions do not lint doc
  comments; use the CLI or pre-commit.
- The file must be valid UTF-8, as the Rust compiler requires.
