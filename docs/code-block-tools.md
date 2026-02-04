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
enabled = false                    # Master switch (default: false)
normalize-language = "linguist"    # Language alias resolution (see below)
on-error = "warn"                  # Error handling: "fail", "warn", or "skip"
timeout = 30000                    # Tool timeout in milliseconds
```

### Language Configuration

Configure tools per language:

```toml
[code-block-tools.languages]
python = { lint = ["ruff:check"], format = ["ruff:format"] }
javascript = { format = ["prettier"] }
shell = { lint = ["shellcheck"], format = ["shfmt"], on-error = "skip" }
json = { format = ["jq"] }
```

Each language can have:

- `lint` - List of tool IDs to run during `rumdl check`
- `format` - List of tool IDs to run during `rumdl check --fix`
- `on-error` - Override global error handling for this language

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

| Tool ID | Language | Type | Command |
|---------|----------|------|---------|
| `ruff:check` | Python | Lint | `ruff check --output-format=concise -` |
| `ruff:format` | Python | Format | `ruff format -` |
| `black` | Python | Format | `black --quiet -` |
| `isort` | Python | Format | `isort -` |
| `shellcheck` | Shell | Lint | `shellcheck -` |
| `shfmt` | Shell | Format | `shfmt` |
| `prettier` | Multi | Format | `prettier --stdin-filepath=_.EXT` |
| `eslint` | JS/TS | Lint | `eslint --stdin --format=compact` |
| `rustfmt` | Rust | Format | `rustfmt` |
| `gofmt` | Go | Format | `gofmt` |
| `goimports` | Go | Format | `goimports` |
| `clang-format` | C/C++ | Format | `clang-format` |
| `stylua` | Lua | Format | `stylua -` |
| `taplo:format` | TOML | Format | `taplo format -` |
| `yamlfmt` | YAML | Format | `yamlfmt -` |
| `jq` | JSON | Format | `jq .` |
| `sql-formatter` | SQL | Format | `sql-formatter` |
| `pg_format` | SQL | Format | `pg_format` |
| `nixfmt` | Nix | Format | `nixfmt` |
| `terraform:fmt` | Terraform | Format | `terraform fmt -` |
| `mix:format` | Elixir | Format | `mix format -` |
| `dart:format` | Dart | Format | `dart format` |
| `ktlint` | Kotlin | Both | `ktlint --format` / `ktlint` |
| `scalafmt` | Scala | Format | `scalafmt --stdin` |
| `ormolu` | Haskell | Format | `ormolu` |
| `fourmolu` | Haskell | Format | `fourmolu` |
| `latexindent` | LaTeX | Format | `latexindent` |
| `markdownlint` | Markdown | Lint | `markdownlint --stdin` |
| `typos` | Multi | Lint | `typos --format=brief -` |
| `cljfmt` | Clojure | Format | `cljfmt fix -` |
| `rubocop` | Ruby | Both | `rubocop -a` / `rubocop` |

**Note**: Tools must be installed separately. rumdl does not install them for you.

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

| Value | Behavior |
|-------|----------|
| `"fail"` | Stop processing, return error |
| `"warn"` | Log warning, continue processing |
| `"skip"` | Silently skip, continue processing |

Set globally or per-language:

```toml
[code-block-tools]
on-error = "warn"  # Global default

[code-block-tools.languages]
shell = { lint = ["shellcheck"], on-error = "skip" }  # Override for shell
```

## How It Works

1. **Extract**: Parse markdown to find fenced code blocks with language tags
2. **Resolve**: Map language tag to canonical name (e.g., `py` → `python`)
3. **Lookup**: Find configured tools for that language
4. **Execute**: Run tools via stdin/stdout
5. **Report/Apply**: Show lint diagnostics or apply formatted output

### Line Number Mapping

Tool output references lines within the code block. rumdl maps these to the actual markdown file line numbers so diagnostics point to the correct location.

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
json = { format = ["jq"] }
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
which ruff  # Should show path
ruff --version  # Should show version
```

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

| Feature | rumdl | mdsf |
|---------|-------|------|
| Built-in tools | 31 | 339 |
| Custom tools | Yes | Yes |
| Linting | Yes | No |
| Formatting | Yes | Yes |
| Language aliases | Yes (Linguist) | Yes |
| Integration | Part of rumdl | Standalone |

rumdl focuses on common tools with the ability to add custom ones. mdsf has broader tool coverage but only formats (no linting).
