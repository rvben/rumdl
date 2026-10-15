---
description: "Run external linters and formatters such as ruff and shellcheck against the fenced code blocks inside your Markdown. Supported since rumdl 0.3.0."
---

# Code Block Tools

Run external linters and formatters on fenced code blocks in your markdown files.

> **Supported since 0.3.0**: Code block tools are opt-in. Their documented CLI and configuration follow the [stability policy](stability.md). External tools are installed separately.

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

Or, for a Python project keeping everything in `pyproject.toml`, put the same
settings under `[tool.rumdl]`:

```toml
[tool.rumdl.code-block-tools]
enabled = true

[tool.rumdl.code-block-tools.languages]
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

## Per-run control

Use the CLI to override the configured master switch for one invocation:

```bash
# Check the outer Markdown, but skip every configured code-block tool
rumdl check --no-code-block-tools file.md

# Run configured code-block tools without checking the outer Markdown
rumdl check --only-code-block-tools file.md

# Format fenced code blocks only; leave the outer Markdown alone
rumdl fmt --only-code-block-tools file.md
```

`--no-code-block-tools` forces the master switch off but preserves and validates
the rest of the code-block-tools configuration. `--only-code-block-tools`
forces the master switch on, while still respecting each language's
`enabled = false` setting and its configured lint and format tool lists. The two
flags are mutually exclusive.

Only mode changes which rules run over the outer Markdown, not which tool phases
run: `check` runs lint tools, while `check --fix` and `fmt` run lint tools, then
format tools, then lint tools again, exactly as they do without the flag. A
finding a formatter cannot fix is therefore still reported. Rule-selection flags
such as `--enable` and `--disable` select the rumdl rules used for fenced
Markdown configured with `lint = ["rumdl"]`.

`--only-code-block-tools` warns when the resolved configuration has no language
with a tool to run, because such a run checks nothing and would otherwise report
success.

Any setting in this section can also be overridden for a single run with an
inline `--config` snippet, which takes precedence over the config files:

```bash
# The long form of --no-code-block-tools
rumdl check --config 'code-block-tools.enabled = false' file.md

# Turn the tools on for one run when the config leaves them off
rumdl check --config 'code-block-tools.enabled = true' file.md

# Raise the timeout for one run
rumdl check --config 'code-block-tools.timeout = 60000' file.md
```

An override sets the settings it names, so overriding `enabled` or `timeout`
leaves the configured languages and tools alone. The mode flags are the more
explicit route to the master switch and win over an inline `--config` that sets
it the other way, in both directions.

`--disable all` is not a substitute for `--only-code-block-tools`. It empties the
rule set, and that same set is what fenced Markdown configured with
`lint = ["rumdl"]` is linted with, so the built-in tool goes silent while every
external tool keeps reporting.

These per-run flags operate on files and directories. They are rejected with
`--stdin`, `--stdin-batch`, and the `-` stdin path.

## Configuration

### Basic Options

```toml
[code-block-tools]
enabled = false                              # Master switch (default: false)
normalize-language = "linguist"              # Language alias resolution (see below)
on-error = "warn"                            # Override; default "fail", also accepts "skip"
on-missing-language-definition = "ignore"    # See "Missing Language/Tool Handling" below
on-missing-tool-binary = "warn"              # See "Missing Language/Tool Handling" below
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
- `format` - Ordered list of formatters for `rumdl check --fix` and `rumdl fmt`
- `format-mode` - How the `format` list is applied: `"fallback"` (default) or
  `"pipeline"`, described below
- `on-error` - Override global error handling for this language

#### Fallback and pipeline formatting

By default the `format` list is a fallback list: the first formatter that
succeeds supplies the replacement code, even if it makes no changes, and the
rest do not run. Use it to name alternatives, such as
`format = ["ruff:format", "black"]` to use whichever is installed. A failed
formatter follows `on-error`; with `"warn"` or `"skip"`, the next formatter is
tried.

With `format-mode = "pipeline"`, every formatter runs in the order listed, each
on the output of the previous one, and the block is replaced once with the
final result. A formatter that makes no changes does not stop the pipeline. Use
it to combine operations, such as lint fixes followed by formatting:

```toml
[code-block-tools.languages]
shell = { format = ["shuck:lint-fix", "shuck:format"], format-mode = "pipeline" }
```

A formatter that fails in a pipeline follows `on-error`. With `"warn"` or
`"skip"`, its output is discarded and the next formatter runs on the output of
the last one that succeeded. With `"fail"`, the run stops at that block, no
formatter output is written to any code block in the file, and rumdl exits with
code 2. A formatter whose binary is missing follows
`on-missing-tool-binary`, and the pipeline continues past it unless that is
`"fail-fast"`.

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

| Tool ID                     | Language   | Type   | Command                                                            |
| --------------------------- | ---------- | ------ | ------------------------------------------------------------------ |
| `ruff:check`                | Python     | Lint   | `ruff check --output-format=concise -`                             |
| `ruff:format`               | Python     | Format | `ruff format -`                                                    |
| `black`                     | Python     | Format | `black --quiet -`                                                  |
| `prettier`                  | Multi      | Format | `prettier --stdin-filepath=_.EXT`                                  |
| `shellcheck`                | Shell      | Lint   | `shellcheck --shell=bash -`                                        |
| `shfmt`                     | Shell      | Format | `shfmt`                                                            |
| `shuck`                     | Shell      | Lint   | `shuck check --output-format concise -`                            |
| `shuck:format`              | Shell      | Format | `shuck format -`                                                   |
| `rustfmt`                   | Rust       | Format | `rustfmt`                                                          |
| `gofmt`                     | Go         | Format | `gofmt`                                                            |
| `goimports`                 | Go         | Format | `goimports`                                                        |
| `clang-format`              | C/C++      | Format | `clang-format`                                                     |
| `sqlfluff:lint`             | SQL        | Lint   | `sqlfluff lint --dialect ansi --format github-annotation-native -` |
| `sqlfluff:fix`              | SQL        | Format | `sqlfluff fix --dialect ansi -`                                    |
| `jq`                        | JSON       | Both   | `jq .`                                                             |
| `yamlfmt`                   | YAML       | Format | `yamlfmt -`                                                        |
| `taplo`                     | TOML       | Format | `taplo fmt -`                                                      |
| `terraform:format`          | Terraform  | Format | `terraform fmt -`                                                  |
| `nixfmt`                    | Nix        | Format | `nixfmt -`                                                         |
| `stylua`                    | Lua        | Format | `stylua -`                                                         |
| `ormolu`                    | Haskell    | Format | `ormolu --stdin-input-file=_.hs`                                   |
| `elm-format`                | Elm        | Format | `elm-format --stdin`                                               |
| `swift-format`              | Swift      | Format | `swift-format format -`                                            |
| `ktfmt`                     | Kotlin     | Format | `ktfmt -`                                                          |
| `djlint`                    | Jinja/HTML | Both   | `djlint - / djlint - --reformat`                                   |
| `djlint:lint`               | Jinja/HTML | Lint   | `djlint -`                                                         |
| `djlint:reformat`           | Jinja/HTML | Format | `djlint - --reformat`                                              |
| `beautysh`                  | Shell      | Format | `beautysh -`                                                       |
| `tombi`                     | TOML       | Lint   | `tombi lint -`                                                     |
| `tombi:format`              | TOML       | Format | `tombi format -`                                                   |
| `tombi:lint`                | TOML       | Lint   | `tombi lint -`                                                     |
| `oxfmt`                     | Multi      | Format | `oxfmt --stdin-filepath=_.EXT`                                     |
| `bun-fmt`                   | Multi      | Format | `bun fmt --stdin-filepath=_.EXT`                                   |
| `deno-fmt`                  | Multi      | Format | `deno fmt --ext=EXT -`                                             |
| `shuck:lint`                | Shell      | Lint   | `shuck check --output-format concise -`                            |
| `shuck:lint-fix`            | Shell      | Format | `shuck check --output-format concise - --fix`                      |
| `shuck:format-check`        | Shell      | Lint   | `shuck format - --check`                                           |
| `oxfmt:lint`                | JavaScript | Lint   | `oxfmt --stdin-filepath=_.js`                                      |
| `oxfmt:format`              | JavaScript | Format | `oxfmt --stdin-filepath=_.js`                                      |
| `djlint:html:lint`          | HTML       | Lint   | `djlint - --profile=html`                                          |
| `djlint:html:format-check`  | HTML       | Lint   | `djlint - --check --profile=html`                                  |
| `djlint:html:format`        | HTML       | Format | `djlint - --reformat --profile=html`                               |
| `djlint:jinja:lint`         | Jinja      | Lint   | `djlint - --profile=jinja`                                         |
| `djlint:jinja:format-check` | Jinja      | Lint   | `djlint - --check --profile=jinja`                                 |
| `djlint:jinja:format`       | Jinja      | Format | `djlint - --reformat --profile=jinja`                              |
| `rumdl:lint`                | Markdown   | Lint   | `built-in markdown linting`                                        |
| `rumdl:format`              | Markdown   | Format | `built-in markdown formatting`                                     |
| `rumdl`                     | Markdown   | Lint   | `built-in markdown linting`                                        |

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
use the same formatting policy. Most tools use comparison because check flags
vary in exit code, output, and stdin support. The explicit native checks below
are exceptions whose stdin behavior has been verified.

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

### Explicit Lint and Format Variants

Use explicit IDs to distinguish linting, formatting checks, and rewriting:

```toml
[code-block-tools]
enabled = true

[code-block-tools.languages]
markdown = { lint = ["rumdl:lint"], format = ["rumdl:format"] }
javascript = { lint = ["oxfmt:lint"], format = ["oxfmt:format"] }
html = { lint = ["djlint:html:lint", "djlint:html:format-check"], format = ["djlint:html:format"] }
jinja = { lint = ["djlint:jinja:lint", "djlint:jinja:format-check"], format = ["djlint:jinja:format"] }
shell = { lint = ["shuck:lint", "shuck:format-check"], format = ["shuck:format"] }
```

`oxfmt:lint` checks formatting; it does not perform semantic JavaScript linting.
Like the existing bare `oxfmt` ID, `oxfmt:lint` and `oxfmt:format` select the
JavaScript parser. For other languages, use the existing extension variants,
such as `oxfmt:ts`, `oxfmt:json`, or `oxfmt:css`, in either slot.

The djlint HTML and Jinja variants explicitly select their respective profiles.
The existing `djlint`, `djlint:lint`, and `djlint:reformat` IDs continue to use
djlint's configured/default profile.

The `*:format-check` IDs and `oxfmt:lint` belong in the `lint` list and are declined in a `format` list. `shuck:format-check` uses `shuck format - --check` with Shuck 0.2.2 or newer.
`djlint:html:format-check` and `djlint:jinja:format-check` use `djlint - --check --profile=html` or `--profile=jinja` with djLint 1.39.5 or newer. Older, prerelease, and unknown versions use
formatter-output comparison. Version probes obey the tool timeout and are cached across files for each installed binary. Timeout suppression is also keyed by the selected executable, so a hanging tool
in one project does not disable another project’s installation.

`oxfmt:lint` retains comparison: its native check is incompatible with the stdin
filename needed for embedded code. Other formatter IDs also retain comparison.
A native formatting-needed exit becomes one formatting finding; a crash, parse
failure, or other execution error follows `on-error`. Checks never rewrite a block.
User-defined overrides retain their own command and diagnostic behavior.

Use `format = ["shuck:lint-fix"]` to apply shuck's safe lint fixes instead of
formatting. This requires a shuck version supporting `check --fix` over stdin.
Only a successful exit supplies replacement code. If remaining lint findings,
parse errors, or a runtime failure cause a nonzero exit, rumdl retains the block
and applies `on-error`. Configure `shuck:lint` in the lint list to report lint
findings before and after fixing.

### Embedded Markdown Linting and Formatting

`rumdl:lint` and `rumdl:format` run rumdl's own rules on fenced `markdown` and
`md` blocks in-process, without an external executable. Their slots can be
configured independently: `lint = ["rumdl:lint"]` checks examples without
rewriting them, and `format = ["rumdl:format"]` enables rewriting during `fmt`
or `check --fix` without enabling lint reports during `check`.

For compatibility, the legacy `lint = ["rumdl"]` setting continues to enable
both linting and formatting. The bare name also works in the format slot.

This feature is opt-in. Without this configuration, Markdown examples are left
alone, including intentionally broken examples.

## Bun formatting

Bun versions that provide `bun fmt` can format fenced code without changing the
outer Markdown formatting policy. The integration is opt-in:

```toml
[code-block-tools]
enabled = true

[code-block-tools.languages]
javascript = { lint = ["bun-fmt"], format = ["bun-fmt"] }
typescript = { lint = ["bun-fmt:ts"], format = ["bun-fmt:ts"] }
json = { lint = ["bun-fmt:json"], format = ["bun-fmt:json"] }
```

`rumdl check` compares Bun's formatted output with the original code block and
reports formatting differences. `rumdl fmt` applies those changes. Bun discovers
its own formatter configuration from rumdl's working directory. Install a
Bun version with the `fmt` command before enabling these tools; older versions
fail when asked to run it. A `package.json` script named `fmt` takes precedence
over Bun's built-in formatter, as do local entry points such as `fmt.ts` and
package binaries named `fmt`. rumdl rejects these collisions rather than running
the project command; rename the conflicting entry before using `bun-fmt`.

Use an extension-specific variant for other languages, such as `bun-fmt:tsx`,
`bun-fmt:css`, `bun-fmt:yaml`, or `bun-fmt:toml`. See [Bun's formatter
documentation](https://bun.com/docs/format) for its formatting options.

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

The `on-error` option controls behavior when a tool fails. A lint tool fails
when it cannot run to completion: it cannot be started, it times out, or its
output cannot be read. A lint tool that exits non-zero and prints diagnostics
has not failed; its diagnostics are reported as findings. A formatter also
fails when it exits non-zero, or when it prints nothing for a code block that
is not empty (usually a linter configured as a formatter). rumdl never replaces
a block with that empty output, whatever `on-error` says.

| Value    | Behavior                                                                 |
| -------- | ------------------------------------------------------------------------ |
| `"fail"` | Report the failure at the code block and stop processing that file       |
| `"warn"` | Print a warning with the file and line, continue with the next tool      |
| `"skip"` | Continue with the next tool silently                                     |

With `"fail"`, `rumdl check` reports the failure as a `code-block-tools`
finding at the block whose tool failed, next to the findings from the blocks
checked before it, and exits with a failure. `rumdl fmt` and
`rumdl check --fix` report it on stderr and exit with code 2, because the run
is incomplete. Stopping external formatting does not roll back Markdown fixes
or files completed earlier; writes remain atomic per file. Coverage policies set
to `fail-fast` additionally leave the failing file unwritten. With `"warn"`, the
warning goes to stderr (suppressed by `--silent`) and does not change the exit code.

Set globally or per-language:

```toml
[code-block-tools]
on-error = "warn"  # Global override; the default is "fail"

[code-block-tools.languages]
shell = { lint = ["shellcheck"], on-error = "skip" }  # Override for shell
```

## Executable lookup and execution context

Bare executable names default to system lookup, preserving existing PATH-based
configurations and pinned pre-commit environments. Project discovery is opt-in
per executable. It searches from the document directory through ancestors,
nearest first, stopping at the first `.git` directory or file (including submodule
boundaries). Without a repository, the nearest `.rumdl.toml`, `rumdl.toml`, or
`.config/rumdl.toml`, or `pyproject.toml` containing `[tool.rumdl]` bounds the
search; without either marker, only the document directory is searched. A home-level
user configuration is not a project boundary; an explicit Git repository rooted
there still supplies a boundary when project lookup is opted in. Nested
`package.json` and `pyproject.toml` files do not stop workspace lookup, so hoisted
Node installations and workspace virtual environments remain reachable.

In each directory, project search order is `.venv/bin`, `venv/bin`, then
`node_modules/.bin`; on Windows virtual environments use `Scripts` instead of
`bin`. The CLI and LSP start discovery from the document path.

```toml
[code-block-tools.binary-preferences]
ruff = "only-project"
prettier = "only-system"
```

| Preference              | Search order                                    |
| ----------------------- | ----------------------------------------------- |
| `project`               | Project locations, then system lookup           |
| `system`                | System lookup, then project locations           |
| `only-project`          | Project locations only                          |
| `only-system` (default) | System lookup only; preserves existing behavior |

Preferences name the executable, so `ruff` controls both `ruff:check` and
`ruff:format`. Explicit absolute or relative executable paths bypass preferences;
relative paths retain their meaning relative to rumdl's working directory.
System lookup follows Rust's process spawning behavior: PATH on Unix, and the
executable/system directories plus PATH on Windows. Windows bare-name lookup
does not use PATHEXT; use a native executable or configure a shell explicitly for
`.cmd` wrappers, including Node package wrappers. Extensionless Unix shims are
not selected by project lookup on Windows. Project discovery never installs or
downloads a tool.

The selected executable is shared by availability checks, version probes,
execution, and cache identity. Selecting a project binary does not change the
child process working directory: tools inherit rumdl's working directory and use
their own configuration discovery rules. Configured commands execute with the
user's permissions; they are not sandboxed by rumdl.

## Missing Language/Tool Handling

Coverage policies are independent settings. They accept `ignore`, `warn`, `fail`,
and `fail-fast`. `warn` emits a configuration warning on stderr, deduplicated
across the invocation; it becomes fatal with `--deny-config-warnings`. `fail`
reports error diagnostics and continues valid work. `fail-fast` stops at the
first failure, processes files sequentially, and leaves the failing file unwritten.
Files completed earlier may already have been written; this is not a transaction
across files. Machine-readable outputs carry fail-level diagnostics under
`code-block-tools`.

| Setting                          | Default                                   | Applies to                                                                                                                             |
| -------------------------------- | ----------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------- |
| `on-missing-language-tag`        | `ignore`                                  | A fenced block with no language label                                                                                                  |
| `on-unknown-language-tag`        | Inherits `on-missing-language-definition` | A tag absent from the full shared Linguist table, supported compatibility aliases, configured aliases and custom language declarations |
| `on-missing-mode-definition`     | `ignore`                                  | An enabled language with tools only for the opposite lint/format mode                                                                  |
| `on-missing-language-definition` | `ignore`                                  | A recognized language with no tools in either mode                                                                                     |
| `on-missing-tool-binary`         | `warn`                                    | A valid tool with no executable in the allowed lookup locations                                                                        |
| `on-no-tools-run`                | `ignore`                                  | An invocation using neither a tool nor valid cached tool results                                                                       |

Omitting `on-unknown-language-tag` preserves existing strict configurations:
`on-missing-language-definition = "fail"` also rejects unknown tags. An explicit
unknown-tag policy overrides that inheritance. MD040 `custom-languages`, language
configuration keys and configured aliases declare recognized labels. Recognition
is broader than tool routing: extension-only labels retain their literal keys
unless a configured alias selects another language. Language
coverage shares MD040's classification of flavor-specific constructs. Non-code
MyST directives, Pandoc raw blocks and MkDocs attribute-only fences are exempt.
Quarto chunks, Pandoc language classes and MyST code directives remain subject to
tool coverage even though MD040 accepts their syntax. Tool routing still uses the
fence's literal first token and configured aliases: brace syntax is not
implicitly mapped to a plain-language tool configuration. Existing strict
missing-definition configurations therefore continue to reject uncovered chunks;
this does not add execution or metadata stripping for computational notebooks.
For example, `{python}` can still report `Unrecognized code block language tag
'{python}'` under a strict unknown-tag policy even with plain Python tools configured.
See [Quarto's code syntax](https://quarto.org/docs/output-formats/html-code.html)
and [MyST's code directives](https://mystmd.org/guide/directives) for the underlying
formats.

The new settings also accept their snake_case spellings. Malformed TOML, invalid
policy values and commands of the wrong type are configuration parsing errors.
Semantic tool errors are always central configuration warnings: unknown
references, incompatible slots, empty/NUL-containing commands, and format
definitions with `stdout = false`. They are checked even while tools are disabled
and on empty batches, without attaching errors to document lines. Invalid entries
are skipped; valid entries continue. Use `--deny-config-warnings` to reject these
warnings and `--preflight` on formatting runs to prevent writes when planning
finds denied warnings. Use both `--preflight` and `--deny-config-warnings` for
that guarantee. There is no separate invalid-definition severity setting.

For a block, precedence is missing tag, unknown tag, explicitly disabled language,
missing active mode, missing language definition, and missing binary.
Invalid tool entries are validated independently of document coverage. A block gets only its first applicable coverage policy. Custom language
keys and aliases count as recognized even with `normalize-language = "exact"`.
A language with `enabled = false` is intentionally excluded. Valid cached lint
results count as checked for `on-no-tools-run`; cache hits replay coverage warnings
without starting tool processes. Internal embedded Markdown work also counts when
its rule set is nonempty. A spawned tool counts even if it later fails: execution
failures are governed by `on-error` rather than the zero-tools policy.

`on-no-tools-run = "fail"` reports an operational error on stderr and exits 2,
even with `--fail-on never`. Plain JSON remains an array of document findings;
this failure adds no fabricated file entry. SARIF additionally records
`invocations[].toolExecutionNotifications` and `executionSuccessful: false`.
Other finding formats retain their document results, with the operational error
on the diagnostic stream (`--stderr` swaps the streams). Its
`fail-fast` variant checks whether any applicable, available tool can run before
processing files; if none can, it exits without writing files. Otherwise the
zero-tools condition is checked at completion. It applies only when code block
tools are enabled. `--no-code-block-tools` suppresses it.

The CLI applies these policies to files, including `check`, `check --fix`, `fmt`,
and diff previews. Fixing runs execute lint, format, and re-lint phases as needed;
missing-mode policies therefore describe the phase, rather than the command name.
In a lint-only `check`, block coverage under `fail` is an error finding (normally
exit 1), subject to `--fail-on`, including `never`. In formatting phases, the same
gap prevents complete formatting and exits 2; `fail-fast` stops processing and
also exits 2. Invocation errors and denied configuration warnings exit 2 even
with `--fail-on never`. Diff previews never write files. LSP runs external tools on document open/save,
explicit checks, and configuration changes, not on keystrokes. Coverage warnings go to the server log and block failures become diagnostics.
The invocation-wide `on-no-tools-run` assertion is not evaluated independently
on LSP document checks, including explicit requests. Code block tools are not executed through stdin processing.

These settings do not change regular Markdown rules. For example MD040 can
still report an unlabeled block; `--only-code-block-tools` removes document rules
while preserving code block policies.

### `on-missing-language-definition`

Controls what happens when a recognized language has no tools configured in either mode. A language with tools only in the opposite mode uses `on-missing-mode-definition` instead.

| Value         | Behavior                                                     |
| ------------- | ------------------------------------------------------------ |
| `"ignore"`    | Silently skip the block (default)                            |
| `"fail"`      | Record an error, continue processing, exit non-zero at end   |
| `"fail-fast"` | Stop immediately, exit non-zero                              |

`"warn"` reports each uncovered language once per invocation. This changed in
0.3.0; earlier versions accepted it but behaved as `"ignore"`.

### `on-missing-tool-binary`

Controls what happens when a configured tool's binary cannot be found in the locations allowed by its binary preference.

| Value         | Behavior                                                                       |
| ------------- | ------------------------------------------------------------------------------ |
| `"warn"`      | Skip the tool, and name it once for the run as a config warning (default)      |
| `"ignore"`    | Silently skip the tool                                                         |
| `"fail"`      | Record an error, continue processing, exit non-zero at end                     |
| `"fail-fast"` | Stop immediately, exit non-zero                                                |

The tools rumdl drives are installed separately from rumdl, so a machine with
rumdl and none of them is the common case in CI and in a pre-commit hook. Every
block is then skipped and the run reports success without having checked a
single code block, which is why the default says something:

```text
[config warning] code-block tools not installed: ruff. Those code blocks were
not checked. Install them, or set `code-block-tools.on-missing-tool-binary` to
"fail" to stop the run or "ignore" to accept the gap
```

The run still exits 0. `--deny-config-warnings` is what turns that warning into
a failure, and `"ignore"` is the way to accept the gap deliberately, staying
silent even under `--deny-config-warnings` for this setting. Set
`on-no-tools-run = "ignore"` as well when intentionally accepting an entirely
unchecked run.

The check is asked of your configuration rather than of your documents, so it
uses the same resolution as execution: system-only lookup is checked once;
project lookup is checked from each distinct document directory. A tool can therefore be named when no
block in this run would have used it, which is still true and still the thing to
fix.

Under `"fail"` the missing binary is reported against the block instead, and the
config warning does not fire. `rumdl check` reports it as an ordinary finding
and exits 1.

A formatting run exits 2, not 1: a formatter that could not run leaves the
document partly formatted, which is an incomplete run rather than a document
with something wrong in it. It says so on stderr (`Warning: t.md: Tool binary
'ruff' not found in allowed lookup locations for language 'python' at line 3`), and it also carries
the same fact in the machine-readable formats, since a `json`, `sarif`,
`gitlab` or `junit` consumer has nothing but that list to read and an empty one
is indistinguishable from a clean run. It is not added to the `Found N issues`
count, which counts what is wrong with your documents.

### Example: Strict Mode

For CI environments where you want to ensure all code blocks are processed:

```toml
[code-block-tools]
enabled = true
on-missing-language-tag = "fail"
on-unknown-language-tag = "fail"
on-missing-mode-definition = "fail"
on-no-tools-run = "fail"
on-missing-language-definition = "fail"
on-missing-tool-binary = "fail-fast"

[code-block-tools.languages]
python = { lint = ["ruff:check"], format = ["ruff:format"] }
shell = { lint = ["shellcheck"], format = ["shfmt"] }
plaintext = { enabled = false }
```

Use the configuration with denied warnings so invalid tool definitions also fail CI:

```bash
rumdl check --deny-config-warnings docs/
rumdl fmt --preflight --deny-config-warnings docs/
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

### Caching

`rumdl check` can reuse validated Markdown and internal embedded-Markdown results.
Those valid cached checks count for `on-no-tools-run` and replay coverage warnings.

External tools can depend on their own configuration files, environment,
working directory, plugins and version-manager wrappers. Binary identity alone
cannot validate those dependencies. Files with applicable external lint tools
therefore bypass the combined file-result cache and run the tools on each check,
even when their Markdown and rumdl configuration are unchanged. Core Markdown
checks in those files also rerun because their findings share the same cache
entry. Prose-only files and files with only internal embedded-Markdown checks can
still use the cache. This favors current results over external-tool cache speed;
changing a tool configuration requires no manual `rumdl clean`.

Availability checks and binary fingerprints use the same executable selection as
execution. A result from a tool that could not run is never treated as a valid
cached check.

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

rumdl resolves tools itself using the executable preferences above. By default a
bare name uses system lookup; opted-in project lookup also searches the bounded
workspace locations. Explicit paths retain caller-CWD semantics. Shell aliases
and functions are not consulted, so `command -v` resolving an alias does not mean
rumdl can run it.

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
| Built-in tools   | 48             | 339        |
| Custom tools     | Yes            | Yes        |
| Linting          | Yes            | No         |
| Formatting       | Yes            | Yes        |
| Language aliases | Yes (Linguist) | Yes        |
| Integration      | Part of rumdl  | Standalone |

rumdl focuses on common tools with the ability to add custom ones. mdsf has broader tool coverage but only formats (no linting).

## Execution boundaries

| Surface                                      | External lint tools  | External formatters  | Coverage policies          | Writes                                           |
| -------------------------------------------- | -------------------- | -------------------- | -------------------------- | ------------------------------------------------ |
| `check`                                      | Yes                  | No                   | Lint mode                  | None                                             |
| `check --fix`, `fmt`                         | Yes                  | Yes                  | Lint and format modes      | Atomic per file                                  |
| `check --diff`, `fmt --diff`, `fmt --check`  | Yes                  | Yes                  | Lint and format modes      | None                                             |
| `fmt --preflight`, `check --fix --preflight` | Yes                  | Yes                  | Lint and format modes      | After successful batch planning; atomic per file |
| `--only-code-block-tools`                    | According to command | According to command | Enabled                    | According to command                             |
| `--no-code-block-tools`                      | No                   | No                   | Disabled                   | Document fixes only                              |
| LSP open/save and explicit lint requests     | Yes                  | No                   | Lint mode; per document    | None                                             |
| LSP configuration changes                    | Yes                  | No                   | Lint mode; per document    | None                                             |
| LSP keystroke diagnostics                    | No                   | No                   | External policies deferred | None                                             |
| Stdin and stdin batch                        | No                   | No                   | Unsupported                | No external-tool writes                          |

LSP requests do not share the CLI lint cache; each requested external check
runs its tools again. Coverage accounting remains local to that document.

Embedded Markdown checks use rumdl's rules and can run on keystrokes without
spawning a process. LSP code actions and formatting do not run external tools.
See [preflight formatting](usage/cli.md#preflight-formatting) for batch failure
and concurrent-edit boundaries. External formatter convergence is the selected
tool's responsibility; rumdl's embedded Markdown formatting must be idempotent.

The native-process policy and lookup tests are included in the full Linux and
Windows CI suites. Required real-tool CI jobs exercise representative Python,
Rust, Go, and JavaScript tools on Windows and macOS; Linux runs a broader set.
The macOS job also checks external-tool policies, failures, and cache changes.
Tool-specific platform support depends on the installed executable; rumdl neither
installs binaries nor provides a sandbox for them.
