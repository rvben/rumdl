---
description: "Map Obsidian Linter rules and options to rumdl, start with a vault-ready configuration, and review the behaviors that differ."
---

# Comparison with Obsidian Linter

This page maps every [Obsidian Linter](https://github.com/platers/obsidian-linter) rule to its rumdl counterpart, so that a vault owner can judge a migration before starting it, and so that a rule
request for something rumdl lacks arrives pre-triaged.

> **Last verified: August 2026** against Obsidian Linter's generated rule documentation. Obsidian Linter changes over time; if a row is out of date, please
> [open an issue](https://github.com/rvben/rumdl/issues).

## Quick Summary

rumdl and Obsidian Linter are different kinds of tool that overlap on formatting:

- **rumdl** is a file linter and formatter. It runs as a CLI, in pre-commit, as an LSP server, in VS Code, and inside Obsidian through the [obsidian-rumdl](https://github.com/rvben/obsidian-rumdl)
    plugin, which runs the same rule engine on the notes in a vault. Its rules read only files on disk (the file, and the workspace for cross-file rules), so a run is reproducible in CI.
- **Obsidian Linter** is an editor plugin. Besides formatting rules it has rules that run when text is pasted, rules that read the file system (creation and modification times, the file name), and
    user-defined regex replacements and shell commands.

Of Obsidian Linter's 65 rules, **17 have a rumdl equivalent**, **12 overlap partially** (same idea, different contract), **27 have no rumdl counterpart**, and **9 are out of scope by
construction**: the 8 paste rules and YAML timestamp. A file linter has no paste event, and a formatter that writes the current time into the file on every run cannot be idempotent, so those rows
are not gaps that a rule request could close.

Of the 27 without a counterpart, 12 concern the front matter (11 write or rewrite YAML values, Compact YAML removes blank lines inside it), 2 handle spacing around CJK and fullwidth characters,
and the remaining 13 are one-off transforms.

## How the Two Tools Differ

| Aspect          | rumdl                                                                             | Obsidian Linter                                          |
| --------------- | --------------------------------------------------------------------------------- | -------------------------------------------------------- |
| Where it runs   | CLI, pre-commit, LSP, VS Code, Obsidian plugin, WebAssembly                       | Obsidian only                                            |
| Input           | The file, plus the workspace for cross-file rules                                 | The open note, the clipboard, the file system            |
| Rules           | <!-- RULE_COUNT -->84<!-- /RULE_COUNT --> built in, opt-in ones enabled in config | 65 built in, plus custom regex replacements and commands |
| Obsidian syntax | `flavor = "obsidian"`: callouts, wikilinks, `%%` comments, tags                   | Native                                                   |
| Configuration   | `.rumdl.toml` (also JSON, YAML, `pyproject.toml`)                                 | Plugin settings UI                                       |
| Fix mode        | `rumdl fmt`, `rumdl check --fix`, editor code actions                             | Lint on save or on command                               |

## Rule Mapping

Legend: **Yes** means an equivalent rule exists and every explicit option maps (a difference in how a `consistent` style is inferred is noted on the row, not demoted), **Partial** means the rules
overlap but an option or direction has no counterpart, **None** means rumdl has no such rule, and **Out of scope** means a file linter cannot do it. Where an Obsidian Linter option has a rumdl option,
the Notes column names it.

### YAML Rules (14)

| Obsidian Linter                | rumdl                                  | Notes                                                                                                                                                                                                            |
| ------------------------------ | -------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Add blank line after YAML      | **Yes** [MD071](md071.md)              | Both leave a file that is only front matter alone                                                                                                                                                                |
| YAML key sort                  | **Partial** [MD072](md072.md) (opt-in) | "YAML key priority sort order" is `key-order`. "YAML sort order for other keys" has no equivalent: rumdl always sorts unlisted keys ascending. rumdl skips the fix when the front matter contains a comment line |
| Dedupe YAML array values       | **None**                               |                                                                                                                                                                                                                  |
| Escape YAML special characters | **None**                               |                                                                                                                                                                                                                  |
| Force YAML escape              | **None**                               |                                                                                                                                                                                                                  |
| Format tags in YAML            | **None**                               |                                                                                                                                                                                                                  |
| Format YAML array              | **None**                               |                                                                                                                                                                                                                  |
| Insert YAML attributes         | **None**                               | Content generation                                                                                                                                                                                               |
| Move tags to YAML              | **None**                               | Content generation                                                                                                                                                                                               |
| Remove YAML keys               | **None**                               |                                                                                                                                                                                                                  |
| Sort YAML array values         | **None**                               |                                                                                                                                                                                                                  |
| YAML timestamp                 | **Out of scope**                       | Needs the file's creation and modification times. A formatter that rewrites a date on every run is also the opposite of idempotent                                                                               |
| YAML title                     | **None**                               | Writes the file name into `title`. rumdl knows the file name ([MD041](md041.md) derives a heading from it) but no rule writes front matter values                                                                |
| YAML title alias               | **None**                               | Adds the file name to `aliases`. See YAML title                                                                                                                                                                  |

The general settings "Default escape character" and "YAML aliases section style" configure YAML value formatting and have no rumdl equivalent.

### Heading Rules (5)

| Obsidian Linter                        | rumdl                              | Notes                                                                                                                                                                                                                                                                                                                                                                                                                                                  |
| -------------------------------------- | ---------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Capitalize headings                    | **Yes** [MD063](md063.md) (opt-in) | "Style" Title Case / ALL CAPS / First letter is `style` `title-case` / `all-caps` / `sentence-case`. "Ignore cased words" is `preserve-cased-words`, "Ignore words" is `ignore-words`, "Lowercase words" is `lowercase-words` (rumdl ships its own default list)                                                                                                                                                                                       |
| Header increment                       | **Partial** [MD001](md001.md)      | "Start header increment at heading level 2" has no MD001 option. The nearest is [MD041](md041.md) with `level = 2`, which checks the first heading only                                                                                                                                                                                                                                                                                                |
| Headings start line                    | **Yes** [MD023](md023.md)          | MD023 leaves an indented level-1 heading alone when its first word starts with a lowercase letter or a digit (`  # tag`, `# 123`), reading it as a hashtag or issue reference rather than a heading                                                                                                                                                                                                                                                    |
| Remove trailing punctuation in heading | **Partial** [MD026](md026.md)      | Same `punctuation` concept. Obsidian Linter's default set also contains the fullwidth `。，；：！`; rumdl's default is the ASCII `.,;:!`, and the starter configuration below adds the fullwidth forms. Both leave a trailing HTML entity such as `&amp;` alone                                                                                                                                                                                        |
| File name heading                      | **Partial** [MD041](md041.md)      | Inserts the file name as the H1. MD041 reports a missing H1; with `fix = true` it promotes a title-like first line (at most 80 characters, no sentence-ending punctuation, followed by a blank line or the end of the file) to a heading, or, when the file holds only directive blocks, inserts one derived from the file name (kebab-case and underscores become Title Case). A file that opens with a body paragraph is reported but left unchanged |

### Footnote Rules (3)

| Obsidian Linter              | rumdl                         | Notes                                                                                                                   |
| ---------------------------- | ----------------------------- | ----------------------------------------------------------------------------------------------------------------------- |
| Footnote after punctuation   | **None**                      | `Lorem[^1].` is clean to rumdl                                                                                          |
| Move footnotes to the bottom | **Partial** [MD067](md067.md) | MD067 reports definitions that are out of reference order, without a fix, and does not move them to the end of the file |
| Re-index footnotes           | **None**                      | MD067 checks definition order and never renames an identifier                                                           |

### Content Rules (16)

| Obsidian Linter                       | rumdl                                  | Notes                                                                                                                                                                                                                                                                            |
| ------------------------------------- | -------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Emphasis style                        | **Yes** [MD049](md049.md)              | `style` `consistent` / `asterisk` / `underscore`. See the note on `consistent` below                                                                                                                                                                                             |
| Strong style                          | **Yes** [MD050](md050.md)              | `style` `consistent` / `asterisk` / `underscore`. See the note on `consistent` below                                                                                                                                                                                             |
| Unordered list style                  | **Yes** [MD004](md004.md)              | `style` `consistent` / `dash` / `asterisk` / `plus`. See the note on `consistent` below                                                                                                                                                                                          |
| Ordered list style                    | **Partial** [MD029](md029.md)          | "Number style" ascending is `style = "ordered"`, lazy is `style = "one"`. "Preserve starting number" has no option (rumdl honors the start number under `ordered`). "Ordered list marker end style" has no equivalent: rumdl accepts `.` and `)` and never converts between them |
| No bare URLs                          | **Yes** [MD034](md034.md)              | Both wrap the URL in angle brackets                                                                                                                                                                                                                                              |
| Remove consecutive list markers       | **Yes** [MD069](md069.md)              |                                                                                                                                                                                                                                                                                  |
| Remove multiple spaces                | **Partial** [MD064](md064.md)          | rumdl skips runs that are a multiple of 4 spaces, spaces after a task checkbox, and the item lines of column-aligned lists of two or more items. See the differences below                                                                                                       |
| Default language for code fences      | **Partial** [MD040](md040.md)          | rumdl's fix always inserts `text`; "Programming language" has no equivalent                                                                                                                                                                                                      |
| Blockquote style                      | **Partial** [MD027](md027.md)          | MD027 removes extra spaces after `>` but does not add a missing one. The "no space" style has no equivalent                                                                                                                                                                      |
| Quote style                           | **Partial** [MD088](md088.md) (opt-in) | Only the smart-to-straight direction (`normalize-quotes = true`). Obsidian Linter's default direction is also straight quotes, so the defaults agree; its smart-quote styles have no rumdl equivalent                                                                            |
| Auto-correct common misspellings      | **None**                               | [MD044](md044.md) (proper names) and [MD061](md061.md) (forbidden terms, no fix) are the nearest, but neither is a dictionary                                                                                                                                                    |
| Convert bullet list markers           | **None**                               | `•` and `§` are not recognized as list markers                                                                                                                                                                                                                                   |
| Proper ellipsis                       | **None**                               | MD088 normalizes quotes and dashes toward ASCII and does not touch ellipses                                                                                                                                                                                                      |
| Remove empty list markers             | **None**                               | A `-` on its own line is clean to rumdl                                                                                                                                                                                                                                          |
| Remove hyphenated line breaks         | **None**                               |                                                                                                                                                                                                                                                                                  |
| Line break between lines with content | **None**                               | Inserts trailing double spaces or `<br>`. [MD009](md009.md) only tolerates a `br-spaces` run; nothing inserts one                                                                                                                                                                |

### Spacing Rules (19)

| Obsidian Linter                                                 | rumdl                                           | Notes                                                                                                                                                                                                                                                                                                                |
| --------------------------------------------------------------- | ----------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Consecutive blank lines                                         | **Yes** [MD012](md012.md)                       |                                                                                                                                                                                                                                                                                                                      |
| Empty line around code fences                                   | **Yes** [MD031](md031.md)                       |                                                                                                                                                                                                                                                                                                                      |
| Empty line around horizontal rules                              | **Yes** [MD065](md065.md)                       |                                                                                                                                                                                                                                                                                                                      |
| Empty line around tables                                        | **Yes** [MD058](md058.md)                       |                                                                                                                                                                                                                                                                                                                      |
| Heading blank lines                                             | **Yes** [MD022](md022.md) and [MD071](md071.md) | "Bottom" off is `lines-below = 0`. "Empty line between YAML and header" is MD071                                                                                                                                                                                                                                     |
| Line break at document end                                      | **Yes** [MD047](md047.md)                       |                                                                                                                                                                                                                                                                                                                      |
| Remove empty lines between list markers                         | **Yes** [MD076](md076.md)                       | `style = "tight"`                                                                                                                                                                                                                                                                                                    |
| Remove link spacing                                             | **Partial** [MD039](md039.md)                   | Both trim inline link text (`[ text ](url)`) and neither touches reference-style links (`[ text ][id]`). rumdl also trims image alt text and leaves wikilinks (`[[page\| text ]]`) alone, because a wikilink has no destination to rewrite the text against; Obsidian Linter trims wikilinks and leaves images alone |
| Space after list markers                                        | **Partial** [MD030](md030.md)                   | Marker spacing only (`ul-single`, `ol-single`, `ul-multi`, `ol-multi`). Obsidian Linter also normalizes the space after a `[x]` checkbox, which MD030 ignores and MD064 exempts                                                                                                                                      |
| Trailing spaces                                                 | **Yes** [MD009](md009.md)                       | Obsidian Linter strips hard line breaks along with other trailing spaces by default; `br-spaces = 0` does the same. Its "Two Space Linebreak" option, which keeps 2-space hard line breaks, is rumdl's default. Both skip code blocks                                                                                |
| Convert spaces to tabs                                          | **None**                                        | The opposite of [MD010](md010.md). See "Tabs versus spaces" below                                                                                                                                                                                                                                                    |
| Compact YAML                                                    | **None**                                        | MD012 ignores blank lines inside front matter                                                                                                                                                                                                                                                                        |
| Empty line around blockquotes                                   | **None**                                        | [MD028](md028.md) is about blank lines *inside* a blockquote                                                                                                                                                                                                                                                         |
| Empty line around math blocks                                   | **None**                                        |                                                                                                                                                                                                                                                                                                                      |
| Move math block indicators to their own line                    | **None**                                        |                                                                                                                                                                                                                                                                                                                      |
| Paragraph blank lines                                           | **None**                                        | Puts every line of prose in its own paragraph                                                                                                                                                                                                                                                                        |
| Remove space around characters                                  | **None**                                        | Fullwidth forms and CJK punctuation                                                                                                                                                                                                                                                                                  |
| Remove space before or after characters                         | **None**                                        |                                                                                                                                                                                                                                                                                                                      |
| Space between Chinese Japanese or Korean and English or numbers | **Yes** [MD089](md089.md)                       | Opt-in. Both symbol options map with the same defaults (`symbols-after-cjk`, `symbols-before-cjk`). MD089 differs twice: it spaces a symbol only when Latin text is attached to it, so `你好-世界` is left alone, and it puts the space outside emphasis markers rather than inside them                             |

### Paste Rules (8) and Custom Rules

The eight paste rules (blockquote indentation on paste, double checklist and list marker prevention, proper ellipsis on paste, remove hyphens on paste, remove leading or trailing whitespace on paste,
remove leftover footnotes from a quote on paste, remove multiple blank lines on paste) transform the clipboard contents at paste time. There is no paste event in a CLI, a pre-commit hook or a language
server, so they are **out of scope**.

"Custom regex replacement" and "Custom commands" are **None**: rumdl has no user-defined rules.

## Starter Configuration

This `.rumdl.toml` enables the Obsidian flavor and the opt-in rules that have an Obsidian Linter counterpart, and sets the options whose rumdl default differs from Obsidian Linter's. Drop the sections
for rules you did not use in Obsidian Linter.

```toml
[global]
flavor = "obsidian"
# Opt-in rules with an Obsidian Linter counterpart:
# MD063 Capitalize headings, MD072 YAML key sort, MD088 Quote style,
# MD089 CJK spacing
extend-enable = ["MD063", "MD072", "MD088", "MD089"]

# Capitalize headings
[MD063]
style = "title-case"
preserve-cased-words = true
ignore-words = ["macOS", "iOS", "iPhone", "iPad", "JavaScript", "TypeScript", "AppleScript", "I"]

# YAML key sort: list the keys from "YAML key priority sort order"
[MD072]
key-order = ["title", "aliases", "tags"]

# Quote style (straight quotes, Obsidian Linter's default direction)
[MD088]
normalize-quotes = true

# Remove trailing punctuation in heading: add the fullwidth forms
[MD026]
punctuation = ".,;:!。，；：！"

# Remove empty lines between list markers
[MD076]
style = "tight"

# Trailing spaces: strip hard line breaks too, Obsidian Linter's default.
# Drop this section to keep 2-space hard line breaks ("Two Space Linebreak").
[MD009]
br-spaces = 0
```

Rules that are on by default in rumdl and have a direct counterpart (MD012, MD022, MD023, MD031, MD034, MD047, MD058, MD065, MD069, MD071, MD004, MD049, MD050) need no configuration; the differences
noted on their rows are fixed behavior, not options. To keep a behavior Obsidian Linter never touched, disable the rule:

```toml
[global]
flavor = "obsidian"
disable = ["MD010"]  # keep tabs
```

## Known Behavioral Differences

1. **`consistent` follows the majority, not the first occurrence.** Obsidian Linter's `consistent` style for emphasis, strong and unordered lists takes the first marker in the file as the standard.
    rumdl's [MD004](md004.md), [MD049](md049.md) and [MD050](md050.md) take the most prevalent marker, with a fixed tie-break (dash for lists, asterisk for emphasis). A file whose first list uses `*`
    and whose other lists use `-` converges to `-` under rumdl and to `*` under Obsidian Linter.
2. **MD064 leaves some space runs alone on purpose.** A run of 4, 8 or 12 spaces reads as a replaced tab and is skipped so that MD064 does not undo MD010 at its default width, spaces after a task
    checkbox are skipped, and the item lines of a list of at least two items that are all column-aligned are skipped. Obsidian Linter's "Remove multiple spaces" collapses all of these. See
    [MD064](md064.md) for the full exception list.
3. **MD027 only removes spaces.** `>  text` becomes `> text`, but `>text` is not given a space. Obsidian Linter's "Blockquote style" adds it.
4. **MD039 leaves wikilinks alone.** `[[Page| alias ]]` keeps its spaces under rumdl because the pipe text of a wikilink has no destination it could be normalized against. Obsidian Linter trims it.
5. **MD088 only goes toward ASCII.** It replaces smart quotes with straight ones and, with `normalize-dashes = true`, dashes with hyphens. Obsidian Linter can also convert straight quotes to smart
    quotes; rumdl cannot.
6. **MD040 inserts `text`.** Obsidian Linter's "Default language for code fences" inserts the language you configure. rumdl's fix inserts `text` (except under the `mdg` flavor, where a Doc String's
    label is its media type and is left to the author), and the [MD040](md040.md) options normalize labels that are already present rather than choose a default.
7. **MD001 has no "start at level 2".** rumdl checks that heading levels increment by one; it has no option to shift every heading so the document starts at H2.
8. **MD029 never changes the marker delimiter.** `1)` and `1.` are both accepted and neither is converted to the other.
9. **MD072 sorts every unlisted key ascending.** There is no "None" or "Descending" mode for keys outside `key-order`, and the fix is skipped when the front matter contains a comment line, which a
    reorder could leave next to the wrong key. An inline comment after a value moves with its line.
10. **MD026's default punctuation set is ASCII.** Add the fullwidth forms through `punctuation` as in the starter configuration if your headings use them.

## Tabs Versus Spaces

Obsidian Linter's "Convert spaces to tabs" turns leading spaces into tabs. rumdl's [MD010](md010.md) enforces the opposite convention and replaces tabs with spaces.

MD010 already leaves tabs inside code blocks alone by default (`code-blocks = false`). To keep tabs everywhere, disable the rule with `disable = ["MD010"]`. Nothing in rumdl converts spaces to tabs.
Such a rule would be a new opt-in rule and a product decision, not a compatibility fix, because it reverses a rule that every other rumdl user relies on.

## Candidate Rules

These are the rows above that a file linter could legitimately own. They are listed so that a request for one arrives with the mapping already done; they are not commitments.

- Empty list markers (a `-` or `1.` with nothing after it)
- Blank lines around blockquotes
- Blank lines around math blocks
- Footnote reference placement after punctuation
- Footnote re-indexing (`[^3]` becomes `[^1]` when it is the first reference)
- Moving footnote definitions to the end of the document
- Front matter `title` and `aliases` derived from the file name

YAML value formatting (array style, deduplication, escaping, tag formatting) and the two remaining CJK spacing rules, which remove spaces around fullwidth forms and CJK punctuation, are further from
what rumdl does today and would need a design of their own.

## See Also

- [Obsidian Flavor](flavors/obsidian.md) - the syntax the `obsidian` flavor recognizes and how rules adapt to it
- [obsidian-rumdl](https://github.com/rvben/obsidian-rumdl) - the Obsidian plugin that runs rumdl inside a vault
- [Comparison with markdownlint](markdownlint-comparison.md) - for users coming from markdownlint
- [Comparison with mdformat](mdformat-comparison.md) - for users coming from mdformat
- [Rules Reference](rules.md) - the complete list of rumdl's <!-- RULE_COUNT -->84<!-- /RULE_COUNT --> rules
- [Obsidian Linter documentation](https://platers.github.io/obsidian-linter/) - the rule reference this page was verified against
