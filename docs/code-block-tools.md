---
description: "Run external linters and formatters such as ruff and shellcheck against the fenced code blocks inside your Markdown. Preview feature."
---

# Code Block Tools [preview]

Run external linters and formatters on fenced code blocks in your markdown files.

> **Preview Feature**: This feature is experimental and may change in future versions.

## Overview

Code block tools let you lint and format code embedded in markdown:

- **Lint mode** (`rumdl check`): Run linters on code blocks and report issues
- **Fix mode** (`rumdl check --fix`): Run formatters to auto-fix code blocks

This is similar to [mdsf](https://github.com/hougesen/mdsf) but integrated directly into rumdl.

## Quick Start

Add to your `.rumdl.toml`:

```toml
[code-block-tools]
enabled = true

[code-block-tools.languages]
python = { lint = ["ruff:check"], format = ["ruff:format"] }
shell = { lint = ["shellcheck"], format = ["shfmt"] }
```

Then run:

```bash
# Lint code blocks
rumdl check file.md

# Format code blocks
rumdl check --fix file.md
```

## Configuration

### Basic Options

```toml
[code-block-tools]
enabled = false                              # Master switch (default: false)
normalize-language = "linguist"              # Language alias resolution (see below)
on-error = "warn"                            # Error handling: "fail", "warn", or "skip"
on-missing-language-definition = "ignore"   # See "Missing Language/Tool Handling" below
on-missing-tool-binary = "ignore"            # See "Missing Language/Tool Handling" below
timeout = 30000                              # Tool timeout in milliseconds
```

### Language Configuration

Configure tools per language:

```toml
[code-block-tools.languages]
python = { lint = ["ruff:check"], format = ["ruff:format"] }
javascript = { format = ["prettier"] }
shell = { lint = ["shellcheck"], format = ["shfmt"], on-error = "skip" }
json = { lint = ["jq"], format = ["jq"] }
```

Each language can have:

- `enabled` - Whether tools are enabled for this language (default: `true`)
- `lint` - List of tool IDs to run during `rumdl check`
- `format` - List of tool IDs to run during `rumdl check --fix`
- `on-error` - Override global error handling for this language

### Disabling Tools for a Language

Set `enabled = false` to acknowledge a language without configuring tools.
This is useful in strict mode where you want to declare that a language
is intentionally without lint/format tools:

```toml
[code-block-tools]
enabled = true
on-missing-language-definition = "fail"

[code-block-tools.languages]
python = { lint = ["ruff:check"], format = ["ruff:format"] }
plaintext = { enabled = false }
text = { enabled = false }
```

With this configuration, `plaintext` and `text` code blocks are silently skipped without triggering strict mode errors, while unconfigured languages still produce errors.

### Language Aliases

Map language tags to canonical names:

```toml
[code-block-tools.language-aliases]
py = "python"
sh = "shell"
bash = "shell"
```

With `normalize-language = "linguist"` (default), common aliases are resolved automatically using GitHub's Linguist data. Set to `"exact"` to disable alias resolution.

## Built-in Tools

rumdl includes definitions for common tools:

<!-- BEGIN builtin-tools (generated) -->

| Tool ID            | Language   | Type   | Command                                                            |
| ------------------ | ---------- | ------ | ------------------------------------------------------------------ |
| `ruff:check`       | Python     | Lint   | `ruff check --output-format=concise -`                             |
| `ruff:format`      | Python     | Format | `ruff format -`                                                    |
| `black`            | Python     | Format | `black --quiet -`                                                  |
| `prettier`         | Multi      | Format | `prettier --stdin-filepath=_.EXT`                                  |
| `shellcheck`       | Shell      | Lint   | `shellcheck --shell=bash -`                                        |
| `shfmt`            | Shell      | Format | `shfmt`                                                            |
| `shuck`            | Shell      | Lint   | `shuck check --output-format concise -`                            |
| `shuck:format`     | Shell      | Format | `shuck format -`                                                   |
| `rustfmt`          | Rust       | Format | `rustfmt`                                                          |
| `gofmt`            | Go         | Format | `gofmt`                                                            |
| `goimports`        | Go         | Format | `goimports`                                                        |
| `clang-format`     | C/C++      | Format | `clang-format`                                                     |
| `sqlfluff:lint`    | SQL        | Lint   | `sqlfluff lint --dialect ansi --format github-annotation-native -` |
| `sqlfluff:fix`     | SQL        | Format | `sqlfluff fix --dialect ansi -`                                    |
| `jq`               | JSON       | Both   | `jq .`                                                             |
| `yamlfmt`          | YAML       | Format | `yamlfmt -`                                                        |
| `taplo`            | TOML       | Format | `taplo fmt -`                                                      |
| `terraform:format` | Terraform  | Format | `terraform fmt -`                                                  |
| `nixfmt`           | Nix        | Format | `nixfmt -`                                                         |
| `stylua`           | Lua        | Format | `stylua -`                                                         |
| `ormolu`           | Haskell    | Format | `ormolu --stdin-input-file=_.hs`                                   |
| `elm-format`       | Elm        | Format | `elm-format --stdin`                                               |
| `swift-format`     | Swift      | Format | `swift-format format -`                                            |
| `ktfmt`            | Kotlin     | Format | `ktfmt -`                                                          |
| `djlint`           | Jinja/HTML | Both   | `djlint - / djlint - --reformat`                                   |
| `djlint:lint`      | Jinja/HTML | Lint   | `djlint -`                                                         |
| `djlint:reformat`  | Jinja/HTML | Format | `djlint - --reformat`                                              |
| `beautysh`         | Shell      | Format | `beautysh -`                                                       |
| `tombi`            | TOML       | Lint   | `tombi lint -`                                                     |
| `tombi:format`     | TOML       | Format | `tombi format -`                                                   |
| `tombi:lint`       | TOML       | Lint   | `tombi lint -`                                                     |
| `oxfmt`            | Multi      | Format | `oxfmt --stdin-filepath=_.EXT`                                     |
| `deno-fmt`         | Multi      | Format | `deno fmt --ext=EXT -`                                             |
| `rumdl`            | Markdown   | Lint   | `built-in markdown linting`                                        |

<!-- END builtin-tools (generated) -->

**Note**: Tools must be installed separately. rumdl does not install them for you.

**YAML linting**: The built-in `yamlfmt` tool only *formats* YAML; there is no
built-in YAML linter. To lint YAML blocks, wire in a custom tool such as
[ryl](https://github.com/owenlamont/ryl) (see
[Linting YAML blocks with ryl](#linting-yaml-blocks-with-ryl)).

### Tool IDs and Slots

A tool with more than one mode is registered as `tool:mode` (`ruff:check`,
`ruff:format`, `sqlfluff:lint`, `tombi:format`). A bare name resolves to the
variant that fits the slot it is written in, so `lint = ["sqlfluff"]` runs
`sqlfluff:lint` and `format = ["tombi"]` runs `tombi:format`. `terraform-fmt` is
kept as an alias of `terraform:format`, so a config written either way works.

**A formatter in a `lint` slot is a formatting check.** rumdl runs the formatter,
compares its output with the block, and reports `Code block is not formatted` when
they differ:

```toml
[code-block-tools.languages]
python = { lint = ["black"], format = ["black"] }
```

The comparison is exactly what `rumdl fmt` would rewrite, so `check` and `fmt`
cannot disagree. rumdl does not pass a tool's own `--check` or `--diff` flag:
those disagree across tools on exit code, on what they print, and on whether the
flag is even accepted next to the stdin argument the tool requires.

**A linter in a `format` slot is declined.** A linter writes its report to stdout,
which is where the formatted code would come from, so running one would replace
the block with its own output. rumdl skips such a tool and reports the
configuration instead:

```text
Tool in code-block-tools.languages.python.format cannot format: ruff:check is a linter (move it to lint)
```

An id that names no tool at all is reported the same way, with a suggestion:

```text
Unknown tool in code-block-tools.languages.python.format: blackk (did you mean: black?)
```

Both warnings are emitted whether or not `enabled` is set, so a typo surfaces
before the feature is switched on.

### Embedded Markdown Linting

The special `rumdl` tool enables linting of markdown content inside fenced code blocks:

```toml
[code-block-tools]
enabled = true

[code-block-tools.languages.markdown]
lint = ["rumdl"]
```

This runs rumdl's own lint rules on markdown code blocks, useful for documentation that includes markdown examples. Unlike external tools, `rumdl` is built-in and requires no additional installation.

**Note**: This feature is opt-in. Without this configuration, markdown code blocks are not linted, allowing you to show intentionally "broken" markdown examples in documentation.

## Custom Tools

Define custom tools in your config:

```toml
[code-block-tools.tools.my-formatter]
command = ["my-tool", "--format", "-"]
stdin = true
stdout = true
```

Then use in language config:

```toml
[code-block-tools.languages]
mylang = { format = ["my-formatter"] }
```

## Error Handling

The `on-error` option controls behavior when tools fail:

| Value    | Behavior                           |
| -------- | ---------------------------------- |
| `"fail"` | Stop processing, return error      |
| `"warn"` | Log warning, continue processing   |
| `"skip"` | Silently skip, continue processing |

Set globally or per-language:

```toml
[code-block-tools]
on-error = "warn"  # Global default

[code-block-tools.languages]
shell = { lint = ["shellcheck"], on-error = "skip" }  # Override for shell
```

## Missing Language/Tool Handling

Two additional options control behavior when configuration or tools are missing:

### `on-missing-language-definition`

Controls what happens when a code block has a language tag, but no tools are configured for that language in the current mode (`lint` for `rumdl check`, `format` for `rumdl check --fix`).

| Value         | Behavior                                                     |
| ------------- | ------------------------------------------------------------ |
| `"ignore"`    | Silently skip the block (default, backward compatible)       |
| `"fail"`      | Record an error, continue processing, exit non-zero at end   |
| `"fail-fast"` | Stop immediately, exit non-zero                              |

### `on-missing-tool-binary`

Controls what happens when a configured tool's binary cannot be found in PATH.

| Value         | Behavior                                                     |
| ------------- | ------------------------------------------------------------ |
| `"ignore"`    | Silently skip the tool (default, backward compatible)        |
| `"fail"`      | Record an error, continue processing, exit non-zero at end   |
| `"fail-fast"` | Stop immediately, exit non-zero                              |

### Example: Strict Mode

For CI environments where you want to ensure all code blocks are processed:

```toml
[code-block-tools]
enabled = true
on-missing-language-definition = "fail"
on-missing-tool-binary = "fail-fast"

[code-block-tools.languages]
python = { lint = ["ruff:check"], format = ["ruff:format"] }
shell = { lint = ["shellcheck"], format = ["shfmt"] }
plaintext = { enabled = false }
```

With this configuration:

- A Python code block without ruff installed will fail immediately
- A `plaintext` code block is silently skipped (acknowledged but no tools needed)
- A JavaScript code block (not configured at all) will record an error but continue
- The final exit code will be non-zero if any errors were recorded

## How It Works

1. **Extract**: Parse markdown to find fenced code blocks with language tags
2. **Resolve**: Map language tag to canonical name (e.g., `py` → `python`)
3. **Lookup**: Find configured tools for that language
4. **Execute**: Run tools via stdin/stdout
5. **Report/Apply**: Show lint diagnostics or apply formatted output

### Line Number Mapping

Tool output references lines within the code block. rumdl maps these to the actual markdown file line numbers so diagnostics point to the correct location.

A tool that reports a position only in prose (`jq`'s "at line 1, column 9") is
mapped from that prose. A tool that reports no position at all is anchored on the
opening fence, which is the most precise place rumdl can honestly point to. The
built-in definitions ask for a machine-readable format where the tool has one, so
findings land on their own line rather than on the fence: `sqlfluff:lint` uses
GitHub annotations and `djlint` uses an explicit `--linter-output-format`.

### Indented Code Blocks

For code blocks inside lists or blockquotes, rumdl:

1. Strips the indentation before sending to tools
2. Re-applies indentation to formatted output

## Examples

### Python with Ruff

```toml
[code-block-tools]
enabled = true

[code-block-tools.languages]
python = { lint = ["ruff:check"], format = ["ruff:format"] }
```

### Linting YAML blocks with ryl

rumdl has a built-in `yamlfmt` tool for formatting YAML, but no built-in YAML
linter. To lint YAML code blocks, wire in [ryl](https://github.com/owenlamont/ryl)
(a fast yamllint-compatible linter) as a custom tool:

```toml
[code-block-tools]
enabled = true

[code-block-tools.tools.ryl]
command = ["ryl", "-"]

[code-block-tools.languages.yaml]
lint = ["ryl"]
```

ryl reads each block from stdin via `-`; rumdl parses its diagnostics and remaps
the line numbers back to their real positions in the markdown file.

### Multi-language Project

```toml
[code-block-tools]
enabled = true
on-error = "warn"

[code-block-tools.languages]
python = { lint = ["ruff:check"], format = ["ruff:format"] }
javascript = { lint = ["eslint"], format = ["prettier"] }
typescript = { lint = ["eslint"], format = ["prettier"] }
shell = { lint = ["shellcheck"], format = ["shfmt"] }
json = { lint = ["jq"], format = ["jq"] }
yaml = { format = ["yamlfmt"] }
```

### Formatting Only (No Linting)

```toml
[code-block-tools]
enabled = true

[code-block-tools.languages]
python = { format = ["black"] }
rust = { format = ["rustfmt"] }
go = { format = ["gofmt"] }
```

## Troubleshooting

### Tool not found

Ensure the tool is installed and in your PATH:

```bash
command -v ruff  # Should show path
ruff --version  # Should show version
```

rumdl resolves tools itself, the same way it spawns them: a bare name is looked up
in `PATH` (with `.exe` appended on Windows) and a name containing a path
separator is used as written. Nothing else is consulted, so a `command -v` that
finds the tool through a shell alias or function does not mean rumdl will.

### No output from tool

Check the tool works with stdin:

```bash
echo 'x=1' | ruff check --output-format=concise -
```

### Timeout errors

Increase the timeout for slow tools:

```toml
[code-block-tools]
timeout = 60000  # 60 seconds
```

### Wrong language detected

Use explicit aliases:

```toml
[code-block-tools.language-aliases]
py3 = "python"
zsh = "shell"
```

## Comparison with mdsf

| Feature          | rumdl          | mdsf       |
| ---------------- | -------------- | ---------- |
| Built-in tools   | 34             | 339        |
| Custom tools     | Yes            | Yes        |
| Linting          | Yes            | No         |
| Formatting       | Yes            | Yes        |
| Language aliases | Yes (Linguist) | Yes        |
| Integration      | Part of rumdl  | Standalone |

rumdl focuses on common tools with the ability to add custom ones. mdsf has broader tool coverage but only formats (no linting).
