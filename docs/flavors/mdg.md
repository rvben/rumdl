# Markdown with Gherkin Flavor

For Markdown with Gherkin (`.feature.md`) files used for executable
specifications.

**Config name**: `mdg`
**Alias**: `markdown_with_gherkin`
**Auto-detected suffix**: `.feature.md`

rumdl lints a `.feature.md` as ordinary Markdown. This flavor changes a rule
only where the correction it would make can stop the document from parsing as
Gherkin. Where Gherkin accepts a construct in more than one spelling, the
document is steered toward the one spelling that is both accepted and
unambiguous rather than left alone; where no correction is safe, the correction
is withheld.

## Recognized Lines

Gherkin is line-oriented, and this flavor has three shared Gherkin-specific
line shapes. The rules that use them apply the same shape consistently.

### Structure Heading

An ATX heading whose text splits at its first colon, provided no backtick
precedes that colon:

```markdown
# Feature: Checkout
## Rule: Registered customers
### Scenario Outline: Buy an item
#### Examples: Valid cards
```

The colon alone marks the structure. Keywords are localized, so no list of
them can name them all, and rumdl matches the colon rather than a keyword
table. A backtick before the colon disqualifies it: dialect keywords are one or
two plain words, so such a colon sits inside a code span. `` # See `x: y` Notes ``
names no structure and is treated as ordinary prose. A later colon belongs to
the name — `## Scenario: ratio: two to one` splits after `Scenario:` only.

### Tag Line

A line containing at least one backtick-wrapped tag:

```markdown
`@billing` `@critical`
## Scenario: Reject an expired card
```

The recognition follows Cucumber's `` `(@[^`]+)` `` pattern: the text inside a
matching code span starts with `@`, has at least one further character, and may
contain whitespace. The match may occur anywhere on the line, so adjacent tags,
surrounding text, and a trailing `#` comment do not prevent recognition:

```markdown
`@comment_tag1` #a comment
prefix `@tag with spaces` suffix
```

### Table Row

Two to five whitespace characters, spaces or tabs, followed directly by a pipe:

```markdown
#### Examples:

  | start | eat | left |
  | ----- | --- | ---- |
  | 12    | 5   | 7    |
```

A tab counts as one whitespace character, exactly as a space does. Anything
else in that position — a blockquote marker, a list marker, text — makes the
line ordinary Markdown.

## Supported Patterns

### Headings

MDG parses ATX headings only — one to six `#` characters followed by a space. A
Setext heading never becomes a Gherkin node, and a closing sequence leaks into
the node's name: `# Feature: F #` is named `F #`. MD003 therefore steers every
heading to plain ATX under this flavor, whichever style is configured — the one
spelling a Gherkin structure can carry.

Heading *levels* carry no meaning in the Gherkin syntax tree, so a document can
always satisfy the level rules. MD025's fix demotes a second H1 to an H2 and
MD041's fix relevels the first heading where it stands; the keywords survive
either way.

### Trailing Punctuation

The colon after a keyword is what makes the keyword a keyword, so MD026 must
never be able to delete it. Under this flavor the ASCII colon leaves that
rule's `punctuation` set, whether it arrived from the default or from an
explicit configuration.

Because MD026 matches punctuation only at the very end of a heading, dropping
the colon from the set is complete: no heading whose last character is a colon
is inspected at all. `## Scenario!:` is left exactly as written — it names no
Gherkin structure, but repairing it is not this rule's business.

Only the ASCII colon leaves the set. A full-width `：` carries no structural
meaning, so a `punctuation` value that lists one keeps enforcing it and
`## Scenario：` is flagged.

### Unique Headings

Every keyword accepts a name after the colon, so a repeated structure can
always be given a distinct heading:

```markdown
#### Examples: Valid cards

#### Examples: Expired cards
```

Because uniqueness is achievable without losing a Gherkin node, MD024 is not
relaxed by this flavor. Name your `Examples:` and `Background:` blocks rather
than repeating a bare keyword.

### Heading Capitalization

A keyword only counts when it is spelled exactly, and `Scenario Outline` is two
words. Recasing a heading wholesale therefore destroys structures: sentence
case lowercases the second word and all caps rewrites `Feature:` itself.

Under this flavor MD063 copies the keyword and its colon through verbatim and
recases only the part after it, so `## Scenario Outline: add two numbers`
becomes `## Scenario Outline: Add two numbers`. The split happens before the
heading is parsed into segments, so the keyword keeps its own spacing and never
counts as the heading's first or last word. A heading with no colon, or whose
first colon has a backtick before it, is recased exactly as it is elsewhere,
and a trailing `{#custom-id}` is preserved.

### Tags

rumdl does not insert a blank line between a tag line and the structure heading
directly below it. When the tag line opens the document, Gherkin reads the
inserted blank as the Feature line and the Feature collapses into an unnamed
node. Lower down the blank leaves the parse intact, and the exemption keeps the
tags written against the structure they belong to.

The exemption is limited to a heading that names a structure. An ordinary
heading such as `## Notes` keeps the usual blank-line requirement even when the
line above it holds nothing but tags, and the requirement *below* a heading is
never waived.

### Steps

Steps are unordered Markdown list items. Gherkin keywords are localized, so the
word after the marker cannot be recognized without the complete dialect table;
rather than guess, this flavor treats every unordered list item as a possible
step.

Even with MD013 `reflow` enabled, rumdl keeps such an item on one physical
line: the over-long line is still reported, but no fix is offered, because a
wrapped continuation would change the step's Gherkin text. Ordered list items
are reflowed as usual.

### Doc Strings

A Doc String is a backtick-fenced code block. A tilde fence and an indented
block are not Doc Strings, so MD046 always uses `fenced` and MD048 always uses
`backtick` here. `consistent` resolves to those forms rather than to whichever
form happens to be more prevalent in the document.

Closing an unclosed fence is a repair rather than a style conversion, so it
still happens under this flavor whatever `style` is configured.

An info string on the fence becomes the Doc String's media type. MD040 reports
missing and inconsistent labels, but offers no fix under this flavor: adding
the standard `text` fallback would change the parsed media type from absent to
`text`, while normalizing an existing label would replace an explicitly chosen
media type. Both are observable by step definitions. Add the intended media
type manually, or disable MD040 when unlabelled Doc Strings are intentional.

### Data and Examples Tables

Two spaces is always inside the range Gherkin accepts, so MD060 indents a table
whose every line is a table row to `max(2, the enclosing list item's content
indent)` while formatting its cells and delimiters: two spaces at the top
level, and enough to stay inside a nested or ordered list item when the table
sits in one. Tab indentation is recognized and normalized the same way.

Past five whitespace characters no indentation both keeps the rows inside the
list item and keeps them a table, so MD060 falls back to its ordinary Markdown
handling there. It also falls back when any line of the table is not a table
row — rows carrying a blockquote or list marker are ordinary Markdown, and
re-indenting them would corrupt the construct around them.

Because a row is an indent followed *directly* by a pipe, MD055 always uses
`leading_and_trailing` here, and it preserves the row's existing indent while
restyling it. MD060 owns the indent's width.

An indented run that consists entirely of table rows is withheld from MD046's
indented-code detection, so such a table is never converted into a fence. The
decision is made per blank-line-delimited run, before runs are joined, and it
requires every line of the run to be a row — see [Limitations](#limitations).

### Placeholders

`<https://example.com>` is Gherkin placeholder syntax, not an autolink. Inside
a Scenario Outline it is substituted from the `Examples` column of that name,
so wrapping a bare URL in angle brackets can replace the surrounding text with
whatever that column holds.

MD034 still reports bare URLs and email addresses under this flavor, but offers
no fix. The standard angle-bracket correction carries Gherkin meaning wherever
it lands. Whether to use an explicit Markdown link or disable MD034 depends on
the surrounding prose or step text, so rumdl leaves that choice to the user.

## Rule Behavior Changes

Rules steered toward the form Gherkin accepts:

| Rule  | Standard behavior            | MDG behavior                                                                                            |
| ----- | ---------------------------- | ------------------------------------------------------------------------------------------------------- |
| MD003 | Enforce the configured style | Steer every heading to plain ATX                                                                        |
| MD046 | Enforce the configured style | Always `fenced`; an incompatible configured style is overridden                                         |
| MD048 | Enforce the configured style | Always `backtick`; an incompatible configured style is overridden                                       |
| MD055 | Enforce the configured style | Always `leading_and_trailing`, keeping the row's indent; an incompatible configured style is overridden |

Rules whose correction is adjusted so it cannot break the Gherkin syntax:

| Rule  | Standard behavior                        | MDG behavior                                                   |
| ----- | ---------------------------------------- | -------------------------------------------------------------- |
| MD013 | Reflow long prose when `reflow` is on    | Report an over-long unordered list item without offering a fix |
| MD022 | Require blank lines around headings      | Keep a tag line against the structure heading below it         |
| MD026 | Remove configured trailing punctuation   | Drop the ASCII colon from the `punctuation` set                |
| MD034 | Wrap a bare URL in angle brackets        | Report without a fix; `<…>` is Gherkin placeholder syntax      |
| MD040 | Add `text` to an unlabelled fence        | Report without a fix; label is a Doc String media type         |
| MD046 | Treat an indented block as code          | Exclude a run made entirely of table rows                      |
| MD060 | Format table delimiters and cell spacing | Indent a Gherkin table to `max(2, list item content indent)`   |
| MD063 | Recase the whole heading                 | Recase only the part after the keyword's colon                 |

Rules deliberately left unchanged, because a document can always satisfy them
while staying valid Gherkin:

| Rule  | Why it still applies                                                      |
| ----- | ------------------------------------------------------------------------- |
| MD024 | Every keyword accepts a name, so duplicate headings are always avoidable  |
| MD025 | Heading levels carry no meaning in the Gherkin syntax tree                |
| MD041 | `# Feature: X` is always writable, and the fix relevels in place          |

All other enabled rules lint the Markdown normally.

## Configuration Overrides

Five rules enforce a form this flavor requires over a configured value that
cannot express it. When the value was set explicitly — a defaulted value is
never reported — and the override applies, rumdl prints one line on stderr:

```text
[config warning] MD046: Markdown with Gherkin flavor requires style="fenced" (a Gherkin Doc String is only ever a backtick fence). Overriding style="indented" to style="fenced".
```

| Rule  | Overridden value                                                          | Enforced value                   |
| ----- | ------------------------------------------------------------------------- | -------------------------------- |
| MD003 | any fixed `style` other than `"atx"`                                      | `style = "atx"`                  |
| MD026 | a `punctuation` set holding the ASCII colon                               | the same set without it          |
| MD046 | `style = "indented"`                                                      | `style = "fenced"`               |
| MD048 | `style = "tilde"`                                                         | `style = "backtick"`             |
| MD055 | `style = "no_leading_or_trailing"`, `"leading_only"` or `"trailing_only"` | `style = "leading_and_trailing"` |

`consistent` asks for no particular form and is never reported, even though the
flavor resolves it to the enforced value rather than by prevalence. MD003's
explicit `style = "atx"` already names the enforced form and is not reported.

MD026 reports an explicit colon override as soon as the rule reaches an MDG
document, even when removing the colon leaves no effective punctuation to
check and the rule subsequently skips that document. For example, a file whose
only heading is `#### Examples:` still produces the warning.

Because the flavor is detected per file, the override is only knowable once a
`.feature.md` is checked, not when the configuration is read. Each rule reports
at most one line per process however many files trigger it.

## Limitations

Some rules can still rewrite `.feature.md` content. Each entry below was
reproduced against the current build.

MD003 always converts Setext headings to plain ATX. In a document without an
explicit `# Feature:` heading, Cucumber may derive the feature name from the
leading Markdown content; the inserted `#` and following space then become
literal content in that derived name. Cucumber's
`testdata/good/misc.feature.md` demonstrates this known AST change. Avoid
Setext headings in MDG, or disable MD003 when preserving that parsed feature
name is more important than normalizing headings.

Rules that can turn a line into a tag line:

| Rule  | What happens                                                                                                                      | How to avoid it                                 |
| ----- | --------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------- |
| MD038 | Padding that CommonMark would not strip is removed, so `` `@wip ` `` becomes `` `@wip` ``, a tag line binding to whatever follows | Do not put a tag in a code span on its own line |

Rules that can move a table out of the two-to-five whitespace window:

| Rule  | What happens                                                                                                                        | How to avoid it                                    |
| ----- | ----------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------- |
| MD010 | With `code-blocks = true`, a tab-indented table expands to spaces beyond the window; the default skips it as an indented code block | Use spaces, or leave `code-blocks` at its default  |
| MD030 | A wider marker gap can push a table continuation indent past five                                                                   | Leave `ul-multi` and `ul-single` at their defaults |
| MD046 | An indented run mixing table rows with any other line is fenced whole, dedenting the rows                                           | Separate the note with a blank line                |

Rules that can break the link between a placeholder and its column:

| Rule  | What happens                                                                                                           | How to avoid it                                |
| ----- | ---------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------- |
| MD044 | A configured name recases an `Examples` heading or column but not the matching `<placeholder>`, and rewrites a keyword | Do not put keywords or column names in `names` |
| MD063 | Title case recases a `<placeholder>` inside a structure's name but not the one in the step below it                    | Do not rely on placeholder case                |
| MD054 | `url-inline = false` rewrites an inline link as an autolink, which is placeholder syntax                               | Leave `url-inline` at its default              |

Rules that can fabricate or rewrite Gherkin text:

| Rule                                                   | What happens                                                                                    | How to avoid it                         |
| ------------------------------------------------------ | ----------------------------------------------------------------------------------------------- | --------------------------------------- |
| MD036                                                  | With `fix = true`, `**Scenario: X**` is promoted into `## Scenario: X`, fabricating a structure | Leave MD036's fix disabled              |
| MD009, MD011, MD014, MD037, MD039, MD049, MD062, MD064 | Step and Doc String text can be rewritten by spacing, link, emphasis, or shell-prompt fixes     | Review these fixes before applying them |

## Configuration

The flavor is selected automatically for a file whose name ends in
`.feature.md`, matched without regard to case. `.feature.markdown` is not
matched.

It can also be configured explicitly:

```toml
[global]
flavor = "mdg"
```

Or for selected files:

```toml
[per-file-flavor]
"features/**/*.md" = "mdg"
```

The `markdown_with_gherkin` alias is accepted anywhere `mdg` is.

A configured flavor wins over the suffix: a `per-file-flavor` pattern matching
a `.feature.md` decides that file's flavor, and an explicit non-standard
`[global] flavor` applies to `.feature.md` files too.

`--flavor standard` is not an off switch. Standard is also the default, so
rumdl cannot tell an explicit `standard` from an unset flavor and the
`.feature.md` suffix still decides. Every auto-detected suffix behaves this
way, `.mdx` included. To lint a `.feature.md` file as plain Markdown, name it
in `[per-file-flavor]`:

```toml
[per-file-flavor]
"docs/legacy.feature.md" = "standard"
```

## CLI Usage

```bash
rumdl check --flavor mdg features/
rumdl fmt --flavor markdown_with_gherkin features/login.feature.md
```

## When to Use

Use the Markdown with Gherkin flavor when:

- Writing executable specifications as `.feature.md` files
- A repository mixes feature files with ordinary documentation and both should
  be linted in one run

## See Also

- [Flavors Overview](../flavors.md) — compare all flavors
- [Standard Flavor](standard.md) — the base flavor this one adjusts
- [Markdown with Gherkin specification](https://github.com/cucumber/gherkin/blob/main/MARKDOWN_WITH_GHERKIN.md)
