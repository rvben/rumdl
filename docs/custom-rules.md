# Experimental custom checks

The custom-checker prototype tests a language-neutral JSON contract using
rumdl's parser and source positions. It is experimental, report-only, and limited
to one standard/GFM Markdown document at a time. It does not run a checker for
you, execute document commands, load lint configuration, cache diagnostics, or
integrate with the language server. Compatibility is not guaranteed yet.

An example project policy (Python 3.9 or newer) checks that instructional Python commands use
`mise exec --`. Run each stage explicitly, or use a shell with pipeline failure
propagation enabled:

```bash
set -o pipefail
rumdl experimental export-elements docs/setup.md \
  | python3 examples/custom-rules/mise_command.py \
  | rumdl experimental report-diagnostics docs/setup.md
```

The example checker always exits 0 when it produces valid diagnostic data.
`report-diagnostics` exits 0 for a clean result, 1 for findings, or 2 for a protocol
or input error. Shell pipeline failure propagation is needed to preserve failures
from earlier stages. No commands are spawned by these rumdl subcommands.

## Element payload

`export-elements FILE` produces a JSON object with:

- `protocol`: `rumdl.elements`.
- `protocol_version`: `1`.
- `path` and `source`: the supplied path and exact UTF-8 source.
- `source_hash`: the BLAKE3 hash of the source bytes.
- `nodes`: selected elements and their context, in source order.

The selected node kinds are `code_span`, `fenced_code`, `indented_code`, `heading`,
`paragraph`, `list_item`, `blockquote`, `html_block`, `table`, `table_row`,
`table_cell`, `footnote_definition`, and `definition_body`. Node `id` is its array
index. `parent` names the nearest exported ancestor; `prev_sibling` names the
previous exported node with that parent. Either can be null. `heading_path`
names the current heading hierarchy, including an enclosing heading when code
appears inside one. Headings inside blockquotes, list items, footnotes, and
definition bodies stop affecting the outline when that container ends. `info`
and `language` describe fenced code; otherwise they are null. Info is the
parser-decoded value, including entity and escape handling.
Language is its first whitespace-delimited word.

Every `range` is a half-open `[start_byte, end_byte]` into the original UTF-8
source. It includes code delimiters. `content_ranges` contains original source
slices without code delimiters. Code-block content slices omit container prefixes;
multiline inline-code interiors can retain continuation-line prefixes. Code-block slices
come from parser text-event ranges; join them in order to read the raw code.
They can be smaller than lines, especially with CRLF input. Inline code retains
its original interior spaces and line endings. These are raw slices, not cooked
CommonMark text: escapes, tabs, and whitespace have not been normalized.
Non-code nodes have an empty `content_ranges` array. Frontmatter is not exported
as nodes; it remains available in `source`.

Python checkers must slice bytes, not Python character indices:

```python
raw = payload["source"].encode("utf-8")
text = raw[start_byte:end_byte].decode("utf-8")
```

For PR text or another in-memory document, use:

```bash
rumdl experimental export-elements --stdin --stdin-filename pr.md
```

Export accepts at most 4 MiB of UTF-8 source and 100,000 nodes. The report stage
currently reads a source file, so save the identical bytes when exporting stdin
if you want to validate returned findings with rumdl.

## Diagnostic payload

A checker returns one JSON object, not log text or a JSON Lines stream:

```json
{
  "protocol": "rumdl.diagnostics",
  "protocol_version": 1,
  "source_hash": "the unchanged source_hash from the element payload",
  "diagnostics": [
    {
      "rule": "project/mise-command",
      "start_byte": 10,
      "end_byte": 16,
      "message": "Run Python tooling through mise exec --"
    }
  ]
}
```

`report-diagnostics FILE` reads this JSON from stdin. It validates the whole
response before reporting findings. It rejects a stale hash, unknown protocol
or version, invalid ranges or UTF-8 boundaries, malformed rule IDs, empty or
oversized messages, control or bidi override/isolate characters, more than 10,000 findings, and responses
larger than 1 MiB. Rule IDs use lowercase letters, digits, and hyphens in two
nonempty parts separated by `/`. Unknown fields are ignored. Empty ranges at a
valid source boundary are allowed. Columns in rumdl's output count characters;
the interchange ranges count bytes. Findings never carry fixes.

## Example policy and limits

The Python example is static analysis. It checks shell fences for `python`,
`python3`, `pip`, `pip3`, and `pytest` at simple shell command starts, including
after `&&`, `||`, `;`, and pipes. Backslash line continuations stay together. It accepts commands wrapped by `mise exec --`
or `mise x --`. Any other leading command also owns its arguments, so wrappers
such as `env python` are not inspected. It checks inline and indented code when
nearby text reads as an instruction. Bare tool names in prose, comments, and Python-language fences are
not reported. Console and shell-session blocks check only `"$ "` prompt lines;
fence languages are matched without case sensitivity, and untagged fences are
skipped. This is a small example policy, not a complete shell analyzer.
Heredoc bodies can be mistaken for commands. Shell substitutions, aliases,
wrappers such as `sudo`, shell keywords, versioned executable names, and
template-generated commands require a more capable checker.

Mark an intentional counterexample or bootstrap step explicitly:

```markdown
<!-- mise-check: counterexample -->

~~~bash
python intentionally-unwrapped.py
~~~
```

A marker applies to the following exported block or its descendants. Before a
list it applies to the first item and its descendants, not later sibling items.
Headings such as "Installing mise" exempt their section. These explicit exceptions avoid
assuming that all mentions of an incorrect command are instructions to run it.

The prototype does not honor `rumdl-disable` comments, per-file ignores, severity
configuration, or custom lint configuration. All configuration flags are rejected.
Flavor-specific constructs such as MkDocs admonitions are outside this first
contract. Before integrating subprocess execution or declaring a stable plugin
API, validate the contract against the requesting project's documentation and
settle rule configuration and disable semantics.

Run the example's end-to-end tests after building rumdl:

```bash
RUMDL=target/debug/rumdl python3 -m unittest discover -s examples/custom-rules
```
