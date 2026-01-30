# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.7] - 2026-01-30

### Added

- **MD060: Loose last column option** ([#356](https://github.com/rvben/rumdl/issues/356))
  - `loose-last-column = true` skips padding the last column in body rows
  - Keeps tables compact while maintaining alignment for other columns
  - Useful for tables with variable-length description columns
  - (thanks @Ravlen for the suggestion)

- **MD060: Separate header/body alignment** ([#348](https://github.com/rvben/rumdl/issues/348))
  - `column-align-header` controls alignment for the header row only
  - `column-align-body` controls alignment for body rows only
  - Existing `column-align` remains as fallback for both
  - (thanks @pygarap for the suggestion)

- **MD018: MagicLink support in MkDocs flavor** ([#355](https://github.com/rvben/rumdl/issues/355))
  - When using `flavor = "mkdocs"`, MD018 skips PyMdown MagicLink-style issue references
  - `#10` and `#123` at the start of lines are not flagged as malformed headings
  - Non-numeric patterns like `#Summary` are still flagged correctly
  - (thanks @kattni for the suggestion)

### Changed

- **Docs: Add MD073 to opt-in rules table**
  - MD073 (TOC validation) now listed in the opt-in rules section of docs/RULES.md

- **Release: Add SchemaStore update reminder**
  - `verify-release-ready.sh` now warns when `rumdl.schema.json` has changed
  - Reminds to submit a PR to SchemaStore after releasing

- **Release: Add opt-in rules documentation check**
  - `verify-release-ready.sh` verifies all opt-in rules are documented in docs/RULES.md
  - Prevents forgetting to document new opt-in rules

- **Release: Add config validation check**
  - `verify-release-ready.sh` now tests all rule config options for validation warnings
  - Catches schema bugs where valid options show "Unknown option" warnings

### Fixed

- **MD060: Fix config validation warnings for optional alignment options**
  - `column-align-header` and `column-align-body` no longer show "Unknown option" warnings
  - Schema now explicitly includes all valid config keys

- **MD073: Add missing indent option to config schema**
  - `indent` option was accepted but not in the validation schema

- **CLI: Remove noisy hints from completions command**
  - Shell completion output is now cleaner without installation hints in the generated script

## [0.1.6] - 2026-01-29

### Added

- **CLI: Shell completions subcommand** ([#327](https://github.com/rvben/rumdl/issues/327))
  - `rumdl completions <shell>` generates shell completions for bash, zsh, fish, powershell, and elvish
  - Auto-detects current shell and provides installation hints
  - (thanks @tpoliaw in [#333](https://github.com/rvben/rumdl/pull/333))

- **CLI: Static rules.json for tooling integration** ([#351](https://github.com/rvben/rumdl/issues/351))
  - Rule metadata now available at `https://raw.githubusercontent.com/rvben/rumdl/main/rules.json`
  - Enables external tools (Alfred workflows, etc.) to access rule data without installing rumdl
  - Release verification ensures rules.json stays in sync

### Fixed

- **MD073: Read indent config from MD007 by default** ([#353](https://github.com/rvben/rumdl/issues/353))
  - TOC indentation now respects MD007's `indent` setting automatically
  - Can be overridden with explicit `[MD073] indent = N`

## [0.1.5] - 2026-01-29

### Added

- **CLI: JSON output for rule metadata** ([#351](https://github.com/rvben/rumdl/issues/351))
  - `rumdl rule -o json` exports all rule data as JSON
  - `rumdl rule -o json-lines` for streaming/piping (one JSON object per line)
  - Filter options: `--fixable` (`-f`), `--category` (`-c`)
  - `--list-categories` shows available categories with rule counts
  - `--explain` includes full documentation in output
  - JSON includes: code, name, aliases, summary, category, fix_availability, url

### Fixed

- **MD073: Validate TOC entry indentation matches heading levels** ([#353](https://github.com/rvben/rumdl/issues/353))
  - TOC entries must now have correct indentation relative to heading depth
  - Example: H2 entries need 2-space indent, H3 entries need 4-space indent

- **CLI: Correctly count fixable issues** ([#349](https://github.com/rvben/rumdl/issues/349))
  - "Run `rumdl fmt` to fix N issues" message now uses capability-based counting
  - Previously could show fixable count for rules that don't actually auto-fix

- **CLI: Correctly count files modified by fmt** ([#347](https://github.com/rvben/rumdl/issues/347))
  - Summary now shows actual number of modified files, not just files with issues

### Changed

- **Docs: Update all documentation URLs to rumdl.dev** ([#352](https://github.com/rvben/rumdl/issues/352))
  - LSP code actions now link to rumdl.dev instead of GitHub
  - Rule URLs in JSON output point to rumdl.dev

## [0.1.4] - 2026-01-28

### Changed

- **MD073: Simplified to marker-based TOC detection only** ([#332](https://github.com/rvben/rumdl/issues/332))
  - Removed heading-based TOC detection (~200 lines of code)
  - Now only supports `<!-- toc -->...<!-- tocstop -->` markers
  - Ensures consistent detect/fix behavior - only validates TOCs users explicitly mark
  - Rule is now opt-in: disabled by default, enable with `[MD073] enabled = true`

### Fixed

- **Rules: Call should_skip() in linting engine for opt-in rules**
  - Opt-in rules (like MD073) now correctly skip when disabled
  - Previously, rules had to check skip internally which caused inconsistent behavior
  - Architectural fix affects all rules with opt-in behavior

- **MD073: Fix HTML anchor handling in heading ID generation**
  - Headings with `<a name="..."></a>` anchors now correctly extract the anchor ID
  - Centralized fix in `header_id_utils.rs` benefits all anchor-related functionality

- **MD032: Fix should_skip() to detect ordered lists**
  - `likely_has_lists()` only checked for unordered markers (`*`, `-`, `+`)
  - Ordered lists (e.g., `1. item`) were incorrectly skipped
  - Now uses pre-computed `list_blocks` which includes all list types

## [0.1.3] - 2026-01-28

### Added

- **MD073: Table of Contents validation rule**
  - Validates TOC entries match actual document headings
  - Detects missing, extra, misordered, and misnamed TOC entries
  - Supports multiple anchor styles: GitHub, GitLab, Markdownlint, VSCode
  - Configurable: `toc-heading`, `anchor-style`, `case-sensitive`

### Fixed

- **MD064: Recognize sentence endings after markdown inline elements** ([#345](https://github.com/rvben/rumdl/issues/345))
  - `allow-sentence-double-space` now works with sentences ending in inline code, emphasis, bold, strikethrough, and other markdown elements
  - Example: `` `code`.  Next sentence `` no longer flags a false positive

## [0.1.2] - 2026-01-27

### Fixed

- **LSP: Only auto-fix on manual saves, not autosave** ([#340](https://github.com/rvben/rumdl/issues/340))
  - Prevents unwanted fixes when VS Code autosaves in the background
  - Auto-fix now only triggers on explicit Cmd+S / Ctrl+S saves

- **MD032: Add auto-fix for lazy continuation lines** ([#342](https://github.com/rvben/rumdl/issues/342))
  - When `allow-lazy-continuation = false`, lazy continuation lines now get proper indentation
  - Detects all inline formatting at line start (emphasis, strong, strikethrough, links, images, code)
  - Handles nested lists, blockquotes, and nested blockquotes correctly

- **MD037: Fix range calculation for emphasis spacing** ([#343](https://github.com/rvben/rumdl/pull/343)) (thanks @kachick)
  - Fix ranges are now stored as absolute positions, preventing double-adjustment in fix mode

### Added

- **Test: Smoke test profile for package managers** ([#341](https://github.com/rvben/rumdl/pull/341)) (thanks @kachick)
  - `make test-smoke` runs stable tests excluding flaky proptest/stress tests
  - Useful for Nix and other package managers requiring deterministic test runs

## [0.1.1] - 2026-01-26

### Fixed

- **MD013: Preserve MkDocs snippet delimiters during reflow** ([#338](https://github.com/rvben/rumdl/issues/338))
  - `-8<-` and `--8<--` delimiters now stay on their own lines when reflowing list items
  - Prevents MkDocs Snippets extension syntax from being corrupted

- **MD013: Detect same-line closing tags for script/style elements** ([#339](https://github.com/rvben/rumdl/issues/339))
  - Self-closing tags like `<script src="..."></script>` no longer cause subsequent lines to be skipped
  - Lines after inline script/style tags are now properly checked for line length

- **MD013: Improve reflow handling for config and MkDocs syntax** ([#335](https://github.com/rvben/rumdl/issues/335), [#337](https://github.com/rvben/rumdl/issues/337))
  - Recognize `abbreviations` config option (was showing "unknown option" error)
  - Preserve attrlist syntax `{: .class }` on its own line during reflow

- **MD013: Require space after period for numbered list detection** ([#336](https://github.com/rvben/rumdl/issues/336))
  - Prevents "failed to converge after 100 iterations" error on certain inputs
  - Version numbers like `1.2.3` no longer mistakenly detected as list items

- **MD013: Handle email autolinks correctly**
  - Email addresses in angle brackets (e.g., `<user@example.com>`) no longer break HTML tag extraction

- **MD005/MD030: Use blockquote-aware indent calculation**
  - List continuation detection now correctly handles blockquote prefixes
  - Multi-line list item detection accounts for blockquote nesting

### Changed

- **Shared blockquote utilities for indent calculation**
  - Centralized logic for calculating indentation within blockquotes
  - Improves consistency across MD005, MD030, and MD032 rules

## [0.1.0] - 2026-01-23

### 🎉 First Stable Release

rumdl reaches production-ready status after validation against 99,291 markdown files
across 51 popular repositories (kubernetes, react, vscode, rust-lang, tensorflow, and more)
with **zero false positives detected**.

### Added

- **CLI: `fmt --check` flag for CI usage**
  - Returns non-zero exit code if files would be modified
  - Enables dry-run formatting checks in CI pipelines

- **Fuzz testing infrastructure**
  - Fix idempotency fuzzer verifies fixes don't oscillate
  - Run with `make fuzz` (requires nightly Rust)

### Fixed

- **MD032: Single-pass idempotency for list blank lines**
  - Fixes edge case where multiple fix passes could produce different results

- **MD050: Correct byte range for fix replacement**
  - Fixes auto-fix for strong emphasis style in certain edge cases

### Changed

- **README: Added "Used By" section** with notable projects using rumdl
- **README: Updated SchemaStore section** to reflect current status

## [0.0.224] - 2026-01-22

### Added

- **GitHub Action: `fail-on-error` and `output-file` inputs** ([#324](https://github.com/rvben/rumdl/issues/324), [#325](https://github.com/rvben/rumdl/issues/325))
  - `fail-on-error`: Control whether the workflow fails on violations (default: `true`)
  - `output-file`: Write lint results to a file for use in subsequent steps

- **LSP: `source.fixAll.rumdl` code action for fix-on-save**
  - Enables VS Code's "Fix All on Save" feature for rumdl
  - Configure with `editor.codeActionsOnSave: { "source.fixAll.rumdl": "explicit" }`

## [0.0.223] - 2026-01-21

### Added

- **MD060: `column-align` option for table cell text alignment** ([#317](https://github.com/rvben/rumdl/issues/317))
  - Options: `left`, `center`, `right` to force alignment for all columns
  - Complements existing table formatting rules

- **Documentation site** ([rumdl.dev](https://rumdl.dev))
  - Rule documentation with examples and configuration guides

### Fixed

- **Embedded markdown respects per-file-ignores and inline config**
  - Both `rumdl check` and `rumdl fmt` now honor `<!-- rumdl-disable -->` comments
  - Per-file-ignores configuration applies to embedded markdown blocks

- **Fix mode re-lint uses filtered rules** ([#319](https://github.com/rvben/rumdl/issues/319))
  - After applying fixes, re-linting respects per-file-ignores and inline config

- **MD030: Auto-fix applies custom spacing config** ([#318](https://github.com/rvben/rumdl/issues/318))
  - Custom `ul_single`, `ol_single`, etc. values now applied during fix

### Changed

- **Documentation improvements**
  - Fixed MD051 location in RULES.md tables (thanks @Ravlen in #315)
  - Added Flavors section to README (closes #316)
  - Added Discord server link to issue templates (thanks @pygarap in #321)

## [0.0.222] - 2026-01-20

### Fixed

- **MD018: Comprehensive false positive fixes** ([#314](https://github.com/rvben/rumdl/issues/314))
  - Skip content inside HTML comments (e.g., Jupyter cell markers `#%%`)
  - Skip indented patterns to match markdownlint behavior
  - Skip YAML comments inside frontmatter
  - Flag all malformed headings at line start

- **Tables inside list items** ([#314](https://github.com/rvben/rumdl/issues/314))
  - Tables embedded in list items are now detected, linted, and formatted correctly
  - Validate delimiter indentation for list-table detection

- **MD009: Correct 1-indexed column in ASCII path**
  - Column numbers in diagnostics now match editor positions

- **Schema: Include all flavor aliases** ([#312](https://github.com/rvben/rumdl/issues/312))
  - Added `gfm`, `github`, `commonmark`, `qmd`, `rmd`, `rmarkdown` to schema
  - JSON schema now matches documentation

- **CLI: Prevent duplicate config and use canonical rule list**
  - Fixes edge case where config could be loaded multiple times

### Performance

- **Optimize code detection and cache hash computation**
  - Faster detection of code blocks and spans

- **Cache per-file globsets**
  - Significant speedup for rules using file-specific patterns

## [0.0.221] - 2026-01-18

### Added

- **Embedded markdown linting in fenced code blocks**
  - Lint markdown content inside ` ```markdown ` code blocks
  - Provides lint warnings for markdown examples in documentation
  - Auto-fix support formats embedded markdown preserving code fence indentation

### Fixed

- **MD064: Remove `max-consecutive-spaces` config, improve sentence detection**
  - Removed conceptually inconsistent `max-consecutive-spaces` option
  - Share sentence detection logic with text reflow (DRY)
  - Add proper abbreviation detection (Dr., Prof., e.g., i.e., Mr., Mrs.)
  - Add CJK sentence-ending punctuation support (。, ！, ？)
  - Fix UTF-8 byte boundary handling for multi-byte characters

- **MD055/MD056: Handle tables inside blockquotes in fix mode**
  - Tables within blockquotes now preserve `> ` prefix when fixed
  - Pipe style and column count fixes work correctly in nested contexts

- **MD060: Improve alignment detection for CJK and delimiter styles**
  - Better handling of CJK character widths in table alignment
  - Improved delimiter row style detection

- **MD069: Respect inline disable comments in fix mode**
  - `<!-- markdownlint-disable MD069 -->` now properly prevents fixes
  - Exposed inline config checks for rule disable detection

- **MD058/MD065: Recognize blockquote continuation lines as blank**
  - Lines with only `>` are now treated as blank for table separation rules
  - Fixes false positives in blockquoted content

- **Text reflow: Recognize sentence boundaries with quotation marks**
  - Sentences ending with closing quotes (`.\"`, `!\"`, `?\"`) now detected correctly
  - Supports both straight and curly quotation marks

### Changed

- **Refactored sentence detection into shared module**
  - New `sentence_utils` module provides reusable sentence boundary detection
  - Used by both MD064 (consecutive spaces) and text reflow (MD013)
  - Reduces code duplication by ~100 lines

## [0.0.220] - 2026-01-17

### Added

- **`RUMDL_OUTPUT_FORMAT` environment variable** ([#297](https://github.com/rvben/rumdl/issues/297))
  - Override output format in CI/CD without modifying config files
  - Precedence: CLI flag → env var → config → default
  - Example: `RUMDL_OUTPUT_FORMAT=github rumdl check .`

### Fixed

- **MD050: Skip math blocks to avoid false positives in Quarto**
  - LaTeX subscripts (`x_1`) and exponentiation (`a**b`) were incorrectly flagged as strong emphasis
  - Now correctly skips content inside `$$...$$` math blocks

### Changed

- **Documentation: Clarify CommonMark 0.31.2 spec version** ([#298](https://github.com/rvben/rumdl/issues/298))
  - Explicitly document that rumdl uses CommonMark 0.31.2 via pulldown-cmark
  - Clarify that `standard` flavor includes GFM extensions (tables, task lists, strikethrough, autolinks)
  - Document `commonmark` as alias for `standard`, `github` as alias for `gfm`
  - Add `output-format` setting documentation with all 12 available formats

## [0.0.219] - 2026-01-17

### Added

- **Comprehensive markdown flavor support**
  - **GFM flavor**: Security-sensitive HTML tag detection in MD033, extended autolinks including `xmpp:` protocol in MD034
  - **MkDocs flavor**: mkdocstrings block support, extended markdown syntax (keys `[[Ctrl]]`, caret `^^sup^^`, mark `==highlight==`, ins `++inserted++`, tilde `~sub~`)
  - **MDX flavor**: JSX attribute detection (`className`, `htmlFor`, `onClick`, etc.), JSX expression handling, ESM import/export support
  - **Quarto flavor**: Pandoc citation syntax (`@ref`, `[@ref]`), shortcode detection (`{{< >}}`), div blocks and callouts (`::: {.callout-*}`), math block handling for emphasis rules

- **Comprehensive UTF-8 stress tests**
  - Systematic testing of all rules against 11 scripts (Bengali, Arabic, Chinese, Japanese, Korean, Thai, Hindi, Russian, Greek, Emoji, ZWJ sequences)
  - Tests for check(), fix(), and fix range character boundaries
  - Catches UTF-8 panics before they reach production

### Fixed

- **MD034: UTF-8 panic with multi-byte text before emails**
  - Fixed crash when email follows Bengali, Chinese, Arabic, or other multi-byte UTF-8 text
  - The `xmpp:` prefix check was slicing at invalid character boundaries
  - Discovered during testing against kubernetes/website

- **MD031: Blank line detection after MkDocs admonition blocks**
  - Corrected blank line requirements after admonition content

- **MkDocs: Nested admonition handling**
  - Implemented stack-based detection for properly nested admonitions

### Changed

- **Documentation: Split flavor docs into separate files**
  - `docs/flavors.md` is now a concise overview with links
  - Each flavor has its own file: `docs/flavors/standard.md`, `gfm.md`, `mkdocs.md`, `mdx.md`, `quarto.md`
  - Easier to find, reference, and maintain flavor-specific documentation

## [0.0.218] - 2026-01-15

### Added

- **Style Guide Preset examples**
  - Google Markdown style guide config (`examples/google-style.toml`)
  - Prettier-compatible markdown config (`examples/prettier-style.toml`)

### Fixed

- **MD032: Eliminate false positives on real-world repos**
  - Tested against Rust Book (478 files) and MDN Content (14,100 files) with zero false positives
  - Treat HTML comments as transparent for blank line checks
  - Skip blank line warning when exiting blockquotes (boundary provides separation)
  - Handle prose numbered items like "Chapter 19." (sentence continuation detection)
  - Fix lazy continuation for code span continuations and nested sublists
  - Remove incorrect uppercase heuristic that was closing lists prematurely

- **Config: Correct rule alias mappings**
  - Add `descriptive-link-text` alias for MD059
  - Fix `table-cell-alignment` alias to map to MD060 (was incorrectly MD059)

- **Config: Warn on per-file-ignores patterns with commas**
  - Detects when patterns contain commas without braces
  - Suggests correct glob syntax: `{*.md,*.txt}` instead of `*.md,*.txt`

## [0.0.217] - 2026-01-14

### Added

- **MD064: `max-consecutive-spaces` config option** (fixes #294)
  - New option to allow configurable consecutive space threshold
  - `max-consecutive-spaces = 2` allows traditional two-space sentence spacing
  - Default remains `1` (flags any 2+ consecutive spaces)

- **Comprehensive Markdown flavor documentation**
  - New `docs/flavors.md` with detailed flavor support guide
  - Coverage of MkDocs, MDX, Quarto, GitHub, and Standard flavors

### Fixed

- **MD032: Detect lazy continuation in nested lists** (fixes #295)
  - Lazy continuation lines in nested lists are now correctly identified
  - Uses `content_column` from `ListItemInfo` to calculate proper indent threshold

- **MD032: Detect blockquote level changes as list breaks**
  - Lines with different blockquote nesting levels now properly break lists
  - Prevents false positives when list items span blockquote boundaries

- **MD033: Calculate correct end_line for multiline HTML tags**
  - HTML tags spanning multiple lines now report accurate end positions
  - Improves LSP diagnostic highlighting for multiline HTML elements

- **Config warnings show relative paths instead of absolute** (fixes #291)
  - Config warning messages now use project-relative paths
  - Makes error messages cleaner and more portable

- **Core correctness and performance improvements**
  - Various fixes for edge cases in line processing
  - Performance optimizations in lint context creation

### Community

- Opened official Discord server for the rumdl community (thanks @pygarap for the suggestion in #281)
  - Join at https://discord.gg/ADTJFSFUyn
  - Channels: #general, #announcements, #help, #development
  - Release notifications automatically posted via GitHub webhook

## [0.0.216] - 2026-01-13

### Added

- **MD072: Custom key order for frontmatter sorting** (fixes #290)
  - New `key-order` config option to define preferred key ordering
  - Keys in the order list appear first, unlisted keys sort alphabetically after
  - Example: `key-order = ["title", "date", "author"]` puts title first

### Fixed

- **MD060: Preserve blockquote prefix when formatting tables**
  - Tables inside blockquotes (e.g., `> | A | B |`) now correctly preserve the prefix
  - Supports nested blockquotes (`>>`, `>>>`) with proper prefix preservation
  - Works with all formatting styles (aligned, compact, tight)

- **MD057: Skip link patterns inside LaTeX math spans** (fixes #289)
  - Link-like patterns inside `$...$` or `$$...$$` are no longer flagged as broken links
  - Prevents false positives from LaTeX commands that resemble markdown links

- **MD030: Skip empty marker lines without content** (fixes #288)
  - Lines with only a list marker (no content) are now skipped
  - Prevents incorrect warnings about spacing on empty list items

- **MD031/MD032/MD058/MD065: Auto-fix preserves nested blockquote prefix** (fixes #268)
  - When inserting blank lines in blockquotes, the blockquote prefix is now preserved
  - Works correctly with nested blockquotes (`>>`, `>>>`)
  - Blank blockquote lines use `>` without trailing space (per markdownlint-cli)

- **MD005/MD007: Prevent oscillation between rules**
  - Ordered and unordered list items are now tracked in separate groups
  - Fixes cases where MD005 and MD007 would fight over indentation

## [0.0.215] - 2026-01-12

### Fixed

- **MD057: Exclude footnote definitions from relative link checking** (fixes #286)
  - Footnote definitions (`[^id]: content`) were incorrectly parsed as reference link definitions
  - This caused MD057 to treat footnote content as URLs (e.g., `[^1]: [link](https://example.com)` was flagged as broken link)
  - Footnotes are now correctly identified and excluded from reference definition parsing
  - Also benefits MD053 (unused reference definitions) which uses the same parsing

- **MD005: Auto-fix preserves blockquote prefix** (fixes #268)
  - When fixing list indentation inside blockquotes, the `>` prefix is now preserved
  - Previously, auto-fix could break blockquote structure

## [0.0.214] - 2026-01-11

### Added

- **Per-file flavor configuration** (fixes #283)
  - Configure different markdown flavors for specific file patterns using glob patterns
  - Example: `"docs/**/*.md" = "gfm"` in `[global.file-flavors]` section
  - Supports all flavor values: `commonmark`, `gfm`, `mkdocs`, `obsidian`, `mdbook`
  - File patterns are matched against relative paths from project root

### Fixed

- **MD028: Skip GFM alerts when checking blank lines in blockquotes** (fixes #126)
  - GFM alerts (`> [!NOTE]`, `> [!TIP]`, `> [!IMPORTANT]`, `> [!WARNING]`, `> [!CAUTION]`) require blank line separation to render correctly
  - MD028 now detects GFM alerts and allows blank lines between them
  - Case-insensitive detection supports both `[!NOTE]` and `[!note]`
  - Regular blockquotes between GFM alerts are still flagged correctly

- **MD032: Auto-fix handles varying blockquote whitespace** (fixes #268)
  - Auto-fix no longer breaks blockquote structure when lines have different spacing
  - Properly handles mixed `>` and `> ` prefixes within the same blockquote

- **MD032: Detect blockquotes before skip conditions** (fixes #284, #285)
  - Blockquote detection now runs first, preventing false positives inside blockquotes
  - Fixes cases where multi-paragraph list items in blockquotes triggered warnings

## [0.0.213] - 2026-01-08

### Fixed

- **MD031/MD046: Detect code blocks inside lists** (fixes #276)
  - MD031 with `list-items=true` now correctly flags fenced code blocks inside lists
  - MD046 with `style=fenced` now correctly flags indented code blocks inside lists
  - Uses pulldown-cmark for accurate CommonMark-compliant code block detection
  - Properly distinguishes between code blocks and footnote/admonition/tab content

- **MD057: Check reference-style link definitions for broken links** (fixes #274)
  - Reference-style link definitions like `[ref]: ./path.md` are now checked
  - Previously only inline links `[text](./path.md)` were validated
  - All existing validations apply: external URLs skipped, .html→.md fallback, URL decoding

- **MD007: "Do What I Mean" behavior for indent configuration** (fixes #273)
  - When `indent` is explicitly set without `style`, automatically use fixed style
  - Setting `indent = 4` now actually uses 4-space increments as expected
  - Warning shown if both `indent` and `style = "text-aligned"` are set (contradictory)

- **MD030: Prevent false positives in math blocks** (fixes #275)
  - Math blocks (`$$...$$`) are now detected and excluded from list marker spacing checks
  - Fixes incorrect warnings about list spacing inside LaTeX display math

## [0.0.212] - 2026-01-07

### Added

- **CLI: Show relative file paths in output by default** (fixes #266)
  - Add `--show-full-path` flag to opt into absolute paths
  - All output formats (text, JSON, SARIF, GitHub, GitLab, JUnit) now consistently use paths relative to project root or CWD

### Fixed

- **LSP: Respect FormattingOptions in formatting handler** (fixes #265)
  - `insertFinalNewline`: Add trailing newline if missing
  - `trimFinalNewlines`: Remove extra trailing newlines at EOF
  - `trimTrailingWhitespace`: Remove trailing whitespace from all lines
  - Applies FormattingOptions after lint fixes to match editor preferences

- **Fix coordinator: Re-check all rules after each fix for idempotency** (fixes #271)
  - `rumdl check --fix` now produces idempotent results
  - After each fix, all rules are re-checked with fresh LintContext
  - Handles cascading fixes (e.g., MD046 → MD040) in a single run

- **MD046: Preserve code indentation and skip MkDocs admonitions** (fixes #269, #270)
  - Converting fenced to indented code blocks now preserves internal indentation
  - MkDocs admonitions (`!!! note`, `??? warning`, `???+ tip`) are properly skipped

- **MD012: Fix EOF blank line removal in LSP formatting** (fixes #264)
  - Warning's fix replacement now correctly removes blank lines at EOF

- **MD072: Improved frontmatter key sorting** (fixes #265)
  - Use line-based reordering to preserve original YAML formatting
  - Fixed config warning about "unknown option: enabled"
  - Compute fix in check() to enable LSP formatting

## [0.0.211] - 2026-01-06

### Fixed

- **MD032: Handle blockquote continuation lines correctly** (fixes #260)
  - Blockquote continuation lines within the same blockquote level no longer incorrectly trigger "list should be surrounded by blank lines" warnings
  - Changed detection from checking if line starts with `>` to comparing blockquote nesting levels
  - Lists inside blockquotes like `> - item\n>   continuation` now lint correctly

- **MD071: Preserve trailing newlines when fixing frontmatter spacing** (fixes #262)
  - Auto-fix no longer removes the file's trailing newline
  - Prevents MD047 (files should end with newline) from triggering after MD071 fix

### Changed

- **Rules: Add aliases for MD070, MD071, MD072**
  - MD070: `nested-code-fence-collision`
  - MD071: `blank-line-after-frontmatter`
  - MD072: `frontmatter-key-sort`

## [0.0.210] - 2026-01-05

### Added

- **MD071: Blank line after frontmatter** (new rule)
  - Enforces a blank line between frontmatter closing delimiter and document content
  - Supports YAML (`---`) and TOML (`+++`) frontmatter formats
  - Auto-fixable

- **MD072: Frontmatter key sort** (new rule, disabled by default)
  - Checks that frontmatter keys are sorted alphabetically
  - Supports YAML and TOML frontmatter
  - Configurable `ignore` list for keys to exclude from sorting
  - Enable with `MD072.enabled = true` in configuration
  - Auto-fixable

### Fixed

- **MD022: Support kramdown Inline Attribute Lists** (fixes #259)
  - IAL syntax like `{: .class #id}` immediately after headings no longer triggers blank line warnings
  - IAL lines are treated as part of the heading element
  - Multiple consecutive IAL lines supported
  - Fix function preserves IAL attachment to headings

- **MD046: Use pulldown-cmark for code block detection**
  - More reliable code block detection using CommonMark-compliant parser
  - Fixes edge cases with nested and consecutive code blocks

- **MD072: Fix JSON depth tracking and regex initialization**
  - Use LazyLock for regex to avoid recompilation
  - Fix JSON nested object depth tracking when extracting keys

- **MD072: Stop at TOML table headers when extracting keys**
  - TOML table headers like `[section]` now properly terminate key extraction

## [0.0.209] - 2026-01-05

### Added

- **MD070: Nested code fence collision detection** (new rule)
  - Detects when inner code fences would prematurely close outer fences
  - Example: outer ` ``` ` containing inner ` ``` ` causes collision
  - Suggests using longer fences (` ```` `) for outer blocks
  - Configurable `languages` option to limit checking to specific info strings (default: `["", "markdown", "md"]`)
  - Auto-fix increases fence length to prevent collisions

### Fixed

- **MD040: Fix false positives on list-indented code blocks** (fixes #257)
  - Refactored to use pulldown-cmark for CommonMark-compliant code block detection
  - List-indented closing fences no longer incorrectly flagged as missing language
  - Eliminates manual fence state tracking that failed for consecutive code blocks

- **CLI: Support `respect-gitignore` in config files** (fixes #258)
  - Added `respect-gitignore` option to `[global]` section in `.rumdl.toml` and `pyproject.toml`
  - CLI flag `--respect-gitignore=false` syntax now works (previously only `--no-respect-gitignore`)

- **Release: Fix pre-built aarch64-linux binaries for 16K page systems** (fixes #256)
  - Pre-built binaries for `aarch64-unknown-linux-gnu` and `aarch64-unknown-linux-musl` now work on Asahi Linux and Raspberry Pi 5
  - Set `JEMALLOC_SYS_WITH_LG_PAGE=16` to configure jemalloc for 64K page compatibility
  - Previously crashed with "Unsupported system page size" on systems with 16K or 64K pages

- **List detection: Filter out YAML/TOML lists in frontmatter**
  - Frontmatter arrays like `tags: [foo, bar]` no longer incorrectly detected as markdown lists
  - Fixes false positives in MD004, MD005, MD007, MD030, and other list-related rules

- **List detection: Handle nested list items and tab indentation**
  - Nested items where content begins on the next line are now correctly parsed
  - Tab-indented nested lists now properly detected and validated

- **MD030: Eliminate false positives on hard-wrapped citations and continuations** (fixes #253)
  - Replaced manual regex-based list detection with pulldown-cmark parser for context-aware list item detection
  - Hard-wrapped citation continuations like `(Doe 2003, p.\n  1234)` no longer trigger false positives
  - Resolves LSP formatting infinite loop (adding trailing space → MD009 → removing space → MD030)
  - Added 16 comprehensive edge case tests covering citations, blockquoted lists, URLs, ISBNs, and mathematical expressions

- **Heading detection: Prevent list items from being detected as setext headings** (fixes #254)
  - Per CommonMark spec 4.3, setext heading content cannot be a list item, ATX heading, blockquote, thematic break, code fence, or HTML block
  - Incomplete list items (e.g., `- Orange` followed by `-`) no longer incorrectly detected as level 2 headings
  - Eliminates false positives in all heading-related rules (MD041, MD022, MD023, MD024, MD025, MD026, MD003, MD043)
  - Added comprehensive validation for all CommonMark setext heading exclusions

- **Output formatters: Correct severity field in JSON, JSON Lines, and Azure Pipelines formats**
  - Fixed bug where severity was always output as "error" regardless of actual severity level
  - JSON/JSON Lines/Azure formatters now correctly output "error", "warning", or "info" based on actual severity

## [0.0.208] - 2026-01-02

### Changed

- **Reflow: Replace manual emphasis parsing with pulldown-cmark**
  - Use pulldown-cmark for CommonMark-compliant emphasis, bold, and strikethrough detection
  - Removes ~100 lines of complex manual regex-based emphasis detection
  - Adds GFM strikethrough (`~~text~~`) support to sentence boundary detection

### Fixed

- **Reflow: Continue emphasis markers across sentence splits** (fixes #251)
  - When splitting emphasized text at sentence boundaries, each line now properly retains its emphasis markers
  - Example: `*Sentence one. Sentence two.*` now correctly splits to `*Sentence one.*` and `*Sentence two.*`

- **Reflow: Fix line_length=0 to mean "no wrapping"**
  - Previously `line_length=0` would wrap after every word due to condition always being true
  - Now correctly treats 0 as "unlimited line length" (no wrapping)

- **MD007: Resolve false positives on nested unordered lists in ordered lists** (fixes #247)
  - Nested unordered lists inside ordered lists now calculate expected indent based on parent's content column
  - Fixes ping-pong bug where MD007 fix would then trigger MD005, destroying nesting structure

- **MD009: Correct byte offset calculation for multi-byte characters**
  - Column positions now correctly account for UTF-8 multi-byte characters
  - Fixes incorrect warning positions when lines contain non-ASCII characters

- **MD027: Continue HTML block detection until blank line after closing tag**
  - CommonMark type 6 HTML blocks (like `<table>`) now correctly extend until the first blank line
  - Prevents false positives for blockquote indentation inside HTML tables

- **MD034: Resolve false positives for URLs in angle brackets and parentheses**
  - URLs inside angle brackets (`<https://example.com>`) no longer trigger bare URL warnings
  - URLs inside parentheses with descriptions are properly handled

- **MD037: Mask inline math spans before emphasis detection**
  - Asterisks inside inline math (`$p * q$`) no longer interfere with emphasis detection
  - Fixes false negatives where valid `* spaced emphasis *` was not detected

- **MD051: URL-decode CJK fragments before anchor matching**
  - Japanese, Korean, and Chinese heading fragments are now properly matched
  - URL-encoded fragments (e.g., `#%E6%97%A5%E6%9C%AC%E8%AA%9E`) correctly resolve to decoded anchors

- **MD057: Resolve false positives for email addresses and file references**
  - Email-style patterns and file references with @ symbols no longer trigger GitHub username warnings

- **MD069: Resolve false positives for nested strikethrough**
  - Properly handles strikethrough markers that appear in nested contexts

- **Config: Handle relative paths in find_project_root_from**
  - Configuration discovery now works correctly with relative file paths

- **Reflow: Preserve Hugo shortcodes as atomic elements**
  - Hugo shortcodes (`{{< >}}` and `{{% %}}`) are no longer broken across lines during text reflow

## [0.0.207] - 2025-12-30

### Added

- **CLI: Add `--flavor` option to override markdown flavor**
  - New CLI flag allows overriding the markdown flavor for a single run
  - Options: `commonmark`, `gfm` (GitHub Flavored Markdown)
  - Useful for testing files against different markdown specifications

### Fixed

- **MD029: Respect CommonMark list start values and preserve user intent** (fixes #247)
  - Ordered lists starting at values other than 1 (e.g., `11.`, `12.`, `13.`) are now validated correctly
  - Auto-fix no longer destroys user intent by changing `11.` to `1.` in properly numbered lists
  - Lists with `start` attribute > 1 will warn but not auto-fix, preserving semantic numbering

- **WASM: Convert byte-based columns to character offsets for JavaScript**
  - Column positions in WASM output now use character offsets instead of byte offsets
  - Fixes incorrect highlighting in editors when files contain multi-byte UTF-8 characters

### Changed

- **Docs: Add mise as an installation option**
  - README now includes mise (formerly rtx) as an alternative installation method

## [0.0.206] - 2025-12-28

### Fixed

- **MD013: Apply inline config before should_skip optimization** (fixes #241)
  - Inline configuration (e.g., `<!-- markdownlint-configure-file {"MD013": {"tables": false}} -->`) was not being applied when files had short lines
  - The should_skip optimization now uses effective config after parsing inline overrides

- **CLI: Warn about unknown rules in --enable/--disable flags** (fixes #243)
  - Using non-existent rule names now produces a warning instead of silently ignoring them
  - Invalid rules in all config entry points (CLI flags, config files) are now validated

- **MD034: Fix panic with multi-byte URLs and unbalanced parentheses**
  - URLs containing non-ASCII characters (e.g., Chinese Wikipedia URLs) followed by unbalanced parentheses no longer cause a panic
  - Fixed by using byte indices instead of character indices for string slicing

## [0.0.205] - 2025-12-27

### Fixed

- **MD037: Skip multi-line inline code spans**
  - Asterisks inside code spans that cross line boundaries no longer trigger false positives
  - Uses pulldown-cmark for accurate multi-line code span detection

- **MD031: Don't require blank line after frontmatter**
  - Code blocks and admonitions immediately after YAML/TOML frontmatter no longer trigger warnings
  - Matches markdownlint behavior for frontmatter handling

- **MD034: Handle URLs with parentheses in path** (fixes #240)
  - URLs like `https://example.com/page_(1)` no longer incorrectly flag the closing parenthesis
  - Centralized URL pattern handling for consistent behavior

- **MD046: Detect mixed whitespace indented code blocks**
  - Indented code blocks using tabs or mixed whitespace are now properly detected
  - 4-space indented fences correctly identified as indented code blocks

- **Heading fixes: Preserve original whitespace**
  - Auto-fix for heading rules now preserves the original spacing around heading text

- **CLI: Support rule aliases in --enable/--disable flags**
  - Rule aliases (e.g., `blanks-around-fences` for MD031) now work with CLI flags

### Changed

- **Regex patterns: Centralize URL detection**
  - Consolidated URL patterns with documentation for maintainability
  - Removed unused URL_REGEX and BARE_URL_REGEX

- **Tab expansion: Consolidate into shared utility**
  - CommonMark-compliant tab expansion now uses shared `visual_indent` utility
  - Consistent indentation detection across all rules

## [0.0.204] - 2025-12-26

### Fixed

- **MD007: Use per-parent indent logic instead of document-wide style**
  - Fixed incorrect warnings when nested lists had different indentation styles
  - Each parent list item now independently tracks its children's indentation

- **MD022: Distinguish horizontal rules from frontmatter delimiters**
  - HR patterns (`---`, `***`, `___`) after headings no longer incorrectly trigger blank line warnings
  - Centralized HR detection in `LineInfo` for consistent behavior across rules

- **LSP: Unify cross-file link extraction with MD057**
  - LSP diagnostics now report correct column positions for MD057 warnings
  - Created shared `extract_cross_file_links` utility used by both CLI and LSP
  - Ensures consistent position tracking between editor and command-line

## [0.0.203] - 2025-12-25

### Added

- **Config: Implement Ruff-style config inheritance model**
  - User config (`~/.config/rumdl/`) now serves as base, project config extends it
  - Project-level `enable`/`disable` are additive to user config by default
  - Clear precedence: CLI flags > project config > user config > defaults

- **MD001: Recognize frontmatter `title` as implicit H1**
  - Documents with `title:` in YAML frontmatter no longer require an H1 heading
  - Aligns with common static site generator behavior (Jekyll, Hugo, etc.)

### Fixed

- **MD057/MD051: Skip wikilinks in cross-file validation** (fixes #235)
  - Wikilinks (`[[page]]`) were incorrectly triggering "relative link does not exist" warnings
  - Wikilinks use a different linking system (Obsidian, etc.) and should not be validated as file paths

- **MD057: Fix LSP diagnostic position for cross-file links** (fixes #234)
  - LSP was pointing to link text `[...]` instead of URL `(...)`
  - Now correctly highlights the URL portion in editor diagnostics

- **MD012: Remove all trailing blank lines at EOF**
  - Previously only removed one trailing blank line per fix pass
  - Now removes all consecutive trailing blank lines in a single fix

- **Config: Add missing aliases for MD062/MD064/MD065 rules** (fixes #232)
  - Added kebab-case aliases for newer rules
  - Fixed MD069 typo in alias registration

- **CLI: Fail fast when `--config` conflicts with `--no-config`/`--isolated`**
  - These flags are contradictory and now produce a clear error instead of silently ignoring one

- **Cross-file validation: Fix duplicate warnings for same link**
  - MD051 and MD057 both contribute links to the workspace index with different column positions
  - Deduplication now ignores column differences, preventing duplicate warnings

## [0.0.202] - 2025-12-24

### Added

- **MD069 (no-duplicate-list-markers): Detect accidental duplicate list markers** (fixes #227)
  - Detects patterns like `- - text` that occur from copy-paste with editor auto-list-continuation
  - Supports all unordered markers: `-`, `*`, `+` and mixed combinations
  - Auto-fix removes the first marker: `- - text` → `- text`
  - Correctly ignores CLI flags (`- --verbose`), emphasis (`* *italic*`), and nested lists
  - Skips code blocks, frontmatter, and HTML comments

- **MD032 (blanks-around-lists): Add `allow-lazy-continuation` config option**
  - When enabled, allows list items to continue without blank lines in specific contexts
  - Useful for documentation styles that prefer compact list formatting

### Fixed

- **LSP: Make severity a first-class field on RuleConfig**
  - Severity configuration now properly propagates through LSP diagnostics
  - Fixes issue where custom severity settings were not reflected in editor diagnostics

### Changed

- **Documentation improvements**
  - Added `.ignore` file support documentation
  - Added LSP server documentation
  - Added footnote rules documentation and cache configuration
  - Added opt-in rules section to RULES.md

## [0.0.201] - 2025-12-24

### Added

- **CLI: Add `--fail-on` flag to control exit code by severity**
  - Configure minimum severity level that triggers non-zero exit code
  - Options: `error` (default), `warning`, `info`
  - `--fail-on warning` exits non-zero on warnings or errors
  - `--fail-on info` exits non-zero on any diagnostic
  - Useful for CI/CD pipelines with different strictness levels

- **Severity: Add Info severity level for low-priority issues**
  - New severity level below Warning for informational diagnostics
  - Allows rules to report suggestions without affecting exit codes by default
  - Can be promoted to affect exit codes via `--fail-on info`

- **MD060 (table-column-count): Smart max-width inheritance from MD013**
  - Automatically inherit `max-width` from MD013 `line-length` when not explicitly set
  - Reduces configuration duplication between MD013 and MD060
  - Explicit MD060 `max-width` still takes precedence

### Fixed

- **MD063 (heading-capitalization): Handle inline code at start of heading** (fixes #228)
  - Headings starting with inline code (e.g., `` `code` text ``) now correctly apply sentence case to subsequent text
  - Previously, the word after inline code was incorrectly lowercased
  - Applies to all non-text segments (code, links, HTML) at heading start

- **MD063 (heading-capitalization): Preserve acronyms and brand names in link text**
  - Link text with internal capitals (e.g., `[GitHub](url)`) is now preserved
  - `preserve_cased_words` setting applies to link text, not just plain text
  - Prevents incorrect lowercasing of brand names within links

- **MD063 (heading-capitalization): Handle inline HTML tags**
  - Inline HTML tags like `<kbd>`, `<abbr>`, `<code>` are now recognized as separate segments
  - Prevents HTML tags from interfering with capitalization detection
  - Supports self-closing tags and tags with attributes

- **MD063 (heading-capitalization): Preserve caret notation for control characters**
  - Caret notation like `^A`, `^C`, `^Z` (representing Ctrl+A, Ctrl+C, Ctrl+Z) is preserved
  - Prevents incorrect lowercasing of control character representations
  - Supports full range: `^@` through `^_` and `^A` through `^Z`

## [0.0.200] - 2025-12-23

### Added

- **CLI: Add `--no-defaults` flag to `rumdl config` command** (fixes #218)
  - Show only non-default configuration values, excluding defaults
  - Useful for seeing only customizations without the noise of default values
  - Works with both smart output (with provenance annotations) and `--output toml/json` formats
  - Mutually exclusive with `--defaults` flag
  - Displays helpful message when all configurations are using defaults
  - Supports filtering of global config, rule configs, and per-file-ignores

- **SARIF: Map severity field to SARIF result levels**
  - Error severity maps to SARIF "error" level
  - Warning severity maps to SARIF "warning" level
  - Improves integration with tools that consume SARIF output

- **Testing: Add O(n²) complexity regression testing infrastructure**
  - Detect potential quadratic time complexity issues in rules
  - Prevent performance regressions in line analysis algorithms

### Fixed

- **MD051 (link-fragments): Support extension-less cross-file links (GitHub-style)** (fixes extension-less link validation)
  - Recognize extension-less paths with fragments as cross-file links (e.g., `[link](page#section)` → `page.md#section`)
  - Resolve extension-less paths by trying markdown extensions (`.md`, `.markdown`, `.mdx`, etc.)
  - Handles GitHub-style links that omit file extensions, matching GitHub's rendering behavior
  - Applied to both link detection and cross-file validation
  - Reduces false positives in repositories using extension-less links

- **MD054 (link-image-style): Fix false positives for task lists and HTML tags** (fixes false positive detection)
  - Skip validation for links/images inside task list items (e.g., `- [ ] [link](url)`)
  - Skip validation for links/images inside HTML tags (e.g., `<a href="url">text</a>`)
  - Reduces false positives when links are part of task lists or HTML structures

- **MD057 (link-target-exists): Skip email addresses and fix absolute path bug** (fixes email and path handling)
  - Skip validation for email addresses (e.g., `mailto:user@example.com`)
  - Fix bug where absolute paths were incorrectly validated
  - Improves accuracy by correctly identifying email links vs file links

- **MD063 (heading-capitalization): Respect lowercase-words when inline code/link is last segment** (fixes #223)
  - When the last word in a heading is inline code or a link, respect `lowercase-words` configuration
  - Previously, inline code/link at the end was not checked against lowercase-words list
  - Ensures consistent capitalization rules apply to all heading segments

- **MD007 (ul-indent): Fix indent=1 edge case and integrate edge case tests** (fixes edge cases)
  - Fix handling of `indent = 1` configuration edge cases
  - Integrate comprehensive edge case tests for better coverage
  - Improves reliability for non-standard indent configurations

- **MD057 (link-target-exists): Point diagnostics to URL instead of link text** (fixes #226)
  - LSP diagnostics now point to the actual non-existent URL rather than the link text
  - Fixes incorrect positioning in editors where diagnostics appeared at wrong location
  - Makes diagnostics semantically correct by highlighting the actual problem
  - Improves both CLI and LSP diagnostic accuracy

- **File processor: Canonicalize file paths to prevent duplicate warnings**
  - Normalize file paths to resolve symbolic links and relative paths
  - Prevents duplicate warnings when the same file is referenced via different paths
  - Improves accuracy when processing files via symlinks or relative paths

### Performance

- **MD007 (ul-indent): Cache mixed list nesting detection and optimize blank line checks**
  - Cache results of mixed list nesting detection to avoid redundant calculations
  - Optimize blank line checks for better performance on large files
  - Reduces processing time for files with complex list structures

### Changed

- **Rules: Categorize severities by document impact**
  - Reorganized rule severities into meaningful categories
  - Structure-critical rules (MD001, MD002, MD003, etc.) remain Error
  - Style-preference rules (MD004, MD007, MD029, etc.) now Warning
  - Better alignment between severity and actual impact on document quality

- **Documentation: Add severity categorization section to RULES.md**
  - Documents the reasoning behind severity assignments
  - Explains the distinction between structural errors and style warnings
  - Helps users understand why rules have their assigned severity levels

- **Tests: Reorganize issue-specific tests into appropriate test files**
  - Moved Issue #210 tests to `tests/rules/md007_test.rs`
  - Moved Issue #209 tests to `tests/rules/md007_test.rs`
  - Moved Issue #197 tests to `tests/cli_integration_tests.rs`
  - Removed `issueXXX`-named test files per project conventions

## [0.0.199] - 2025-12-21

### Fixed

- **MD007 (ul-indent): Smart style auto-detection for custom indent values** (fixes #210)
  - When `indent` is set to a non-default value (e.g., `indent = 4`) without explicit `style`, MD007 now auto-detects the appropriate style
  - Pure unordered lists automatically use `fixed` style (markdownlint compatible)
  - Mixed ordered/unordered lists use `text-aligned` style to avoid oscillation with MD005
  - Previously, `indent = 4` was ignored when style wasn't explicitly set, causing incorrect validation
  - The auto-detection handles edge cases including multi-level mixed nesting, HTML comments, code blocks, frontmatter, and blank line list separation

- **MD063 (heading-capitalization): Fix preserve-cased-words for iOS and ignore-words for first word** (fixes #215, #216)
  - `preserve-cased-words` now correctly detects words starting with lowercase followed by uppercase (e.g., "iOS", "eBay")
  - `ignore-words` in sentence case now properly preserves words at the start of headings (e.g., "nvim" stays "nvim")
  - Previously, "iOS" was incorrectly converted to "ios" and "nvim config" became "Nvim config"

### Changed

- **Dependencies: Migrate from deprecated serde_yaml to serde_yml**
  - Replace unmaintained serde_yaml 0.9.34 with maintained fork serde_yml 0.0.12
  - Eliminates deprecation warnings during compilation
  - Compatible API with no behavior changes

## [0.0.198] - 2025-12-20

### Fixed

- **MD057 (link-target-exists): Support mdBook and doc site .html links**
  - Links to `.html` files now correctly resolve when corresponding `.md` source exists
  - Handles mdBook, Jekyll, Hugo and similar doc generators that compile `.md` to `.html`
  - Applied to both single-file `check()` and cross-file validation
  - Reduces false positives in documentation projects like rustc-dev-guide

## [0.0.197] - 2025-12-19

### Fixed

- **Config: Resolve per-file-ignores paths relative to project root** (fixes #208)
  - Previously, per-file-ignores patterns only matched with relative paths
  - Now works correctly in GitHub Actions where absolute paths are used
  - Patterns like `.github/file.md` now match `/home/runner/work/repo/.github/file.md`

- **MD007 (ul-indent): Remove auto-switch to fixed indentation style** (fixes #209)
  - Fixed oscillation between MD005/MD007 when formatting mixed ordered/unordered lists
  - Previously, setting `indent = 3` would auto-switch to fixed style, causing conflicts
  - Text-aligned style (default) now correctly handles mixed lists, matching markdownlint behavior
  - `rumdl fmt` now converges in a single pass for all list configurations

### Performance

- **Core: Reduce memory allocations and improve cache reliability**
  - Optimized internal data structures for better memory efficiency

## [0.0.196] - 2025-12-19

### Added

- **MD064 (no-multiple-consecutive-spaces): New rule to collapse multiple consecutive spaces** (thanks @mawkler in #202)
  - Flags multiple consecutive spaces (2+) in markdown prose
  - Provides auto-fix to collapse them to single space
  - Skips spaces in code, tables, HTML, front matter, and markdown syntax markers

- **MD065 (blanks-around-horizontal-rules): New rule to enforce blank lines around horizontal rules**
  - Ensures horizontal rules have blank lines before and after
  - Provides auto-fix to add missing blank lines
  - Skips Setext heading underlines

- **MD066 (valid-footnote-definition): New rule to validate footnote definitions**
  - Checks footnote definitions have proper syntax
  - Validates footnote labels are non-empty

- **MD067 (no-empty-footnote-ref): New rule to flag empty footnote references**
  - Detects footnote references with empty labels like `[^]`

- **MD068 (no-space-in-footnote-ref): New rule to flag spaces in footnote references**
  - Detects footnote references with spaces like `[^ note]`

### Fixed

- **WASM: Convert byte offsets to character offsets for JavaScript**
  - Fixes crashes in Obsidian plugin when applying fixes to content with multi-byte UTF-8 characters (Norwegian letters, emoji, etc.)

- **MD027 (no-multiple-space-blockquote): Skip list continuation indentation**
  - No longer reports false positives for properly indented list continuations in blockquotes

- **MD032 (blanks-around-lists): Detect thematic breaks as list terminators**
  - Thematic breaks (horizontal rules) now correctly terminate lists

- **MD036 (no-emphasis-as-heading): Skip emphasis inside HTML comments**
  - No longer reports false positives for emphasis markers in HTML comments

- **MD041 (first-line-heading): Skip badge images before headings**
  - Badge images at the start of a file no longer prevent heading detection

- **MD046 (code-block-style): Skip code blocks inside HTML comments**
  - No longer reports false positives for code blocks in HTML comments

- **MD051 (link-fragments): Recognize HTML and attribute anchors**
  - Cross-file fragment validation now recognizes anchors from HTML id attributes

- **MD057 (relative-links): Check for markdown source when .html link doesn't exist**
  - Links to `.html` files now check for corresponding `.md` source files

## [0.0.195] - 2025-12-17

### Added

- **MD063 (heading-capitalization): New rule for heading capitalization style**
  - Enforces consistent capitalization in headings (Title Case, Sentence case, or lowercase)
  - Preserves all-caps acronyms (e.g., "API", "HTTP", "AWS") in title case mode
  - Configurable via `MD063.style` option

### Fixed

- **MD012 (no-multiple-blanks): Detect blank lines across skipped regions**
  - Fixed detection of consecutive blank lines when frontmatter or code blocks are between them

- **MD013 (line-length): Check individual line lengths in reflow default mode**
  - Each line is now checked separately when reflow-mode is enabled

- **MD014 (commands-show-output): Improve message accuracy**
  - Better error messages and principled skip list for code blocks

- **MD018/MD023: Skip GitHub issue refs and hashtags in heading detection**
  - Lines like `#123` or `#hashtag` no longer falsely detected as ATX headings

- **MD033 (no-inline-html): Fix false positives**
  - Skip angle brackets inside link reference definition titles
  - Remove incorrect indentation-based code block skip

- **MD034 (no-bare-urls): Fix multiple edge cases**
  - Detect `www.example.com` as bare URL
  - Handle link reference definitions with titles
  - Resolve false positives in code spans

- **MD037 (no-space-in-emphasis): Skip template shortcode syntax**
  - Template syntax like `{* ... *}` no longer triggers false positives

- **MD042 (no-empty-links): Only flag empty URLs**
  - Links with text but empty URL are flagged; empty text is no longer an error

- **MD049 (emphasis-style): Report both opening and closing markers**
  - Warnings now point to both `*` or `_` markers in emphasis pairs

- **MD056 (table-column-count): Escape pipes in inline code**
  - Pipes inside inline code in tables no longer cause column count errors

- **MD057 (relative-links): Multiple path handling improvements**
  - Handle URL-encoded filenames (e.g., `%20` for spaces)
  - Support framework aliases (e.g., `@/` for src directory)
  - Handle angle-bracket paths (e.g., `<path/to/file.md>`)
  - Skip validation of absolute URL paths
  - Strip query parameters from URLs before file existence check

- **MD063: Preserve all-caps acronyms in title case headings**
  - Words like "API", "HTTP", "URL" maintain uppercase in title case mode

- **CLI: Fix exit code and messaging**
  - Return exit code 0 when all violations are fixed
  - Show accurate message when all violations are fixed

- **Heading detection: Skip multiline link syntax**
  - Lines inside multiline link definitions no longer falsely detected as headings

- **Internal: Add is_valid field and iterator for CommonMark headings**
  - Improved heading validation for rules that depend on heading structure

### Changed

- **MD059: Update alias to 'descriptive-link-text'** (thanks @martimlobao in #201)
  - Rule can now be referenced as `descriptive-link-text` in configurations

### Documentation

- **MD050: Document intentional deviation from markdownlint**
  - Clarified differences in emphasis style checking behavior

## [0.0.194] - 2025-12-12

### Added

- **LSP: workspace/didChangeConfiguration support**
  - LSP server now responds to configuration change notifications
  - Improved robustness and error handling in language server

- **Config: Typestate pattern for configuration validation**
  - Compile-time guarantees for configuration correctness
  - Invalid configurations are caught earlier in the pipeline

- **CI: GitHub Marketplace branding for GitHub Action**
  - Action now has proper branding for Marketplace visibility

### Fixed

- **MD057 (relative-links): Handle template variables and extensionless links**
  - Template syntax (Handlebars, Mustache, Jinja2) like `{{URL}}` now recognized and skipped
  - Extensionless links like `[Page](page)` now resolve to `page.md`
  - All URI schemes (file://, smb://, macappstores://, etc.) properly excluded (fixes #192)

- **Config: Remove misleading GFM/CommonMark warnings (fixes #195)**
  - `gfm` and `commonmark` flavors now silently work as expected
  - Parser already supports GFM extensions (tables, task lists, strikethrough)

- **Config: Discover markdownlint config via upward directory traversal**
  - `.markdownlint.yaml` and similar files now found in parent directories
  - Matches behavior of other linting tools

- **MD007 (ul-indent): Handle tab characters after blockquote markers**
  - Tab indentation after `>` marker now calculated correctly
  - Fixes false positives for lists in blockquotes with tabs

- **MD012 (no-multiple-blanks): Skip blank lines inside indented code blocks**
  - Blank lines within indented code blocks no longer trigger warnings

- **MD030 (list-marker-space): Analyze blockquoted list content correctly**
  - Lists inside blockquotes now properly analyzed instead of skipped

- **MD053 (link-image-reference-definitions): Recognize colons in reference labels**
  - Reference definitions with colons in the label (e.g., `[foo:bar]:`) now detected

- **lint_context: Check entire line for HTML comment marking**
  - Lines with inline comments (`<!-- comment --> content`) now handled correctly
  - Content after `-->` on the same line is properly processed

- **lint_context: Add missing HTML5 block elements**
  - Added: audio, video, source, track, canvas, svg, iframe, embed, object, menu, summary, noscript, search, template

- **lint_context: Handle nested blockquotes and JS comments in ESM blocks**
  - Improved parsing for complex nested structures

- **table_utils: Exclude list items from table row detection**
  - List items containing pipes no longer confused with table rows

- **lib: Handle mutex poisoning gracefully instead of panicking**
  - Recovers from poisoned mutexes for better stability

### Changed

- **Internal: Replace `Mutex<Option<Arc<T>>>` with `OnceLock`**
  - Cleaner initialization pattern for shared state

- **Internal: Replace tuple return with `FixResult` struct**
  - More descriptive return type for fix operations

### Documentation

- **README: Clarify markdownlint config upward traversal behavior**

## [0.0.193] - 2025-12-10

### Fixed

- **MD032 (blanks-around-lists): Complete fix for false positives (fixes #188, #190)**
  - Pipes in code spans (e.g., `` `foo || bar` ``) no longer trigger false table detection
  - Lazy continuation lines at indent=0 handled per CommonMark spec
  - Removed ad-hoc URL/link exclusion hacks in favor of proper `is_table_line()` detection
  - Fixed regression in v0.0.192 that affected list boundary detection

- **MD057 (relative-links): Resolve links per-file instead of caching base path**
  - Relative link resolution now correctly uses each file's directory
  - Prevents cross-file path resolution errors in multi-file projects

### Changed

- **Internal: Replace naive `contains('|')` checks with `is_table_line()`**
  - Structural separator detection now properly validates table structure
  - Requires pipes at start/end with ≥2 total, or separator line pattern

## [0.0.192] - 2025-12-10

### Added

- **Config: `cache` option to disable caching via config file (closes #153)**
  - Add `cache = false` to `[global]` section to persistently disable caching
  - Eliminates need to pass `--no-cache` flag on every invocation
  - Works in both `.rumdl.toml` and `pyproject.toml`

- **MD052 (reference-links-images): User-configurable `ignore` option**
  - Add patterns to skip specific reference links from validation
  - Useful for links handled by external tools or preprocessors

- **CI: Major version tag (`v0`) for GitHub Action**
  - Users can now reference the action as `rvben/rumdl@v0` for automatic minor updates

### Fixed

- **Inline config: Cross-file rules now respect disable comments (fixes #189)**
  - MD051 and MD057 now honor `<!-- rumdl-disable -->` and `<!-- rumdl-disable-next-line -->` comments
  - Previously these rules ran in a separate phase that bypassed inline config filtering

- **Config: Directory exclude patterns now match files within (fixes #186)**
  - Patterns like `docs/` now correctly exclude all files in that directory
  - Glob patterns are automatically expanded to match contained files

- **MD032 (blanks-around-lists): No longer triggers on pipes in inline code (fixes #188)**
  - Tables inside inline code blocks no longer confuse list boundary detection

- **MD005/MD037/MD041: Resolve false positives**
  - MD041: Setext headers with colons no longer cause issues
  - MD037: List items containing asterisks handled correctly
  - MD005: Sublist parsing improved for complex nested structures

- **Text reflow: Use proper table detection**
  - Replaced simplistic pipe check with robust table structure detection
  - Prevents incorrect reflow of table content

### Changed

- **Internal: Replace `RUMDL_FILE_PATH` env var with `source_file` field**
  - Cleaner API for accessing the current file path in rule implementations
  - No user-facing changes

## [0.0.191] - 2025-12-08

### Fixed

- **CLI: Handle broken pipe gracefully when piping to head (fixes #184)**
  - Output to closed pipes (e.g., `rumdl check . | head`) no longer causes error messages
  - SIGPIPE is now handled properly on Unix systems

- **Config: Resolve exclude patterns relative to config file location (fixes #185)**
  - Exclude patterns like `docs/*` now resolve relative to the project root (config file directory)
  - Previously patterns resolved relative to the current working directory, causing them to fail when running from a different directory with `--config`
  - This matches the behavior of Biome, Black, and markdownlint-cli2
  - **Note:** Users running from the project root (the common case) are unaffected

- **Docs: Correct build status badge to track release workflow**

## [0.0.190] - 2025-12-06

### Added

- **GitHub Action: New `rumdl-action` for CI integration**
  - Supports `version`, `path`, and `config` inputs
  - Enables easy integration of rumdl into GitHub workflows

- **CLI: Add `--rules` as alias for `--enable`**
  - More intuitive option name for specifying which rules to run
  - Example: `rumdl check --rules MD001,MD003 file.md`

### Fixed

- **LSP: Detect client diagnostic capabilities to avoid duplicate diagnostics (fixes #182)**
  - LSP server now checks if the client supports `textDocument/publishDiagnostics`
  - Prevents duplicate diagnostics when client pulls diagnostics instead of receiving pushes

- **MD005 (list-indent): Group sublists by parent content column**
  - Correctly handles ordered lists where marker widths vary (e.g., "1. " vs "10. ")
  - Sublists under parents with different content columns are now grouped separately
  - Prevents false positives when indent differences are caused by marker width variation

- **MD037/MD038: Handle multi-byte UTF-8 characters in emphasis detection**
  - Emphasis detection now correctly handles characters like CJK and emoji
  - Prevents false positives and incorrect column reporting for non-ASCII content

- **MD057 (existing-relative-links): Handle absolute paths as workspace-relative**
  - Absolute paths in markdown (e.g., `/CONTRIBUTING.md`) are now resolved relative to workspace root
  - Fixes false positives for links that reference files from the repository root

- **MD057 (existing-relative-links): Add filesystem fallback for case-sensitivity issues**
  - Improves link validation on case-insensitive filesystems
  - Reduces false positives on macOS and Windows

- **CLI: Canonicalize config path before changing working directory**
  - Config files specified with relative paths now work correctly
  - Fixes issue where config was not found after directory change

## [0.0.189] - 2025-12-05

### Fixed

- **MD061 (link-destination-exists): Default `case_sensitive` to `true` when loading from TOML**
  - Configuration parsing now correctly defaults to case-sensitive path checking
  - Aligns TOML config behavior with programmatic defaults

- **MD022 (blanks-around-headings): Treat HTML comments and frontmatter as transparent**
  - HTML comments between content and headings no longer trigger blank line warnings
  - Frontmatter is now invisible to blank line checking around headings

- **MD033 (no-inline-html): Skip code span content in multi-line span detection**
  - HTML tags inside multi-line code spans are no longer flagged
  - Prevents false positives for code examples containing HTML

- **MD032 (blanks-around-lists): Handle multi-line code spans in list detection**
  - Code spans spanning multiple lines no longer confuse list boundary detection
  - Fixes false positives for lists near complex code span usage

- **MD044 (proper-names): Skip HTML comments by default**
  - Proper name checking now excludes HTML comment content
  - Reduces noise from commented-out content

- **MD029 (ol-prefix): Use pulldown-cmark for list membership detection**
  - More accurate detection of which items belong to which ordered list
  - Handles edge cases with code blocks and other interrupting elements

### Changed

- **MD057 (table-pipe-style): Remove unused `skip_media_files` config option**
  - Simplifies configuration by removing non-functional option

## [0.0.188] - 2025-12-03

### Added

- **MD062 (link-destination-whitespace): New rule for detecting whitespace in link destinations**
  - Flags links with leading/trailing whitespace in destinations like `[text]( url )`
  - Supports auto-fix to trim the whitespace
  - Works in both CLI and LSP environments

- **Cross-file analysis infrastructure: Foundation for multi-file linting**
  - New `WorkspaceIndex` for indexing headings and anchors across files
  - Background index worker for LSP with debouncing and progress reporting
  - Reverse dependency tracking for efficient re-linting when files change
  - MD051 now validates cross-file link fragments against the workspace index

### Fixed

- **MD051 (link-fragments): Fix false positives for cross-file links without fragments**
  - Links like `[text](file.md)` no longer incorrectly flagged for empty fragments
  - Only validates fragments when explicitly provided (e.g., `[text](file.md#anchor)`)

## [0.0.187] - 2025-12-02

### Added

- **WASM: Implement Linter class API with configuration support**
  - New `Linter` class provides object-oriented API for WebAssembly builds
  - Supports configuration options for customizing lint behavior
  - Enables more flexible integration in JavaScript/TypeScript projects

### Fixed

- **MD053 (link-image-reference-definitions): Fix false positives for adjacent footnotes (fixes #177)**
  - Footnote references like `[^1][^2]` no longer trigger "unused reference" warnings
  - Uses pulldown-cmark for proper footnote parsing instead of regex

- **MD034 (no-bare-urls): Use pulldown-cmark for proper link parsing**
  - Bare URL detection now correctly identifies URLs already inside links
  - Prevents false positives for URLs that are properly formatted

- **MD010 (no-hard-tabs): Skip tabs in fenced code blocks**
  - Tabs inside fenced code blocks are no longer flagged
  - Preserves intentional tab characters in code examples

- **MD052 (reference-links-images): Disable shortcut reference checking by default**
  - Shortcut references like `[text]` without a following `[]` no longer trigger warnings
  - Reduces false positives in documents using shortcut reference style

- **MD025/MD023: Fix false positives after single-line code blocks (fixes #175)**
  - Comments like `# text` inside code blocks after single-line code blocks no longer flagged
  - Code block parsing now correctly handles edge cases

- **MD013/MD039: Fix reflow oscillation causing unstable output (fixes #170)**
  - Running `rumdl fmt` multiple times now produces stable output
  - Link formatting no longer conflicts with line length reflow

### Changed

- **Tests: Align test expectations with CommonMark spec and markdownlint reference**
  - Test suite updated to match authoritative behavior
  - Ensures compatibility with CommonMark specification

## [0.0.186] - 2025-12-01

### Added

- **WebAssembly support: Build and publish rumdl as npm package**
  - Use rumdl in browsers and Node.js via `@anthropic/rumdl` npm package
  - Includes `lint()` function for linting markdown content
  - Automated npm publishing via GitHub Actions

### Fixed

- **MD058 (blanks-around-tables): Fix duplicating table content during auto-fix**
  - Auto-fix no longer duplicates table content when adding blank lines
  - Correctly handles tables at various positions in the document

- **MD013 (line-length): Preserve nested linked images during text reflow**
  - Markdown images inside links like `[![alt](img.png)](url)` now preserved during line wrapping
  - Previously, the reflow logic could break such nested constructs

### Changed

- **Docs: Document rumdl-fmt pre-commit hook with version check**
  - Added documentation for the `rumdl-fmt` hook in pre-commit configuration
  - Includes version compatibility information

## [0.0.185] - 2025-11-28

### Added

- **Inline config: Support rule aliases in inline configuration comments**
  - Use human-readable aliases like `line-length` instead of `MD013` in inline comments
  - Example: `<!-- rumdl-disable line-length -->` now works alongside `<!-- rumdl-disable MD013 -->`
  - All rules now document their aliases in the rule documentation

### Fixed

- **MD061 (forbidden-terms): Register rule in mod.rs**
  - MD061 was added in v0.0.184 but was never registered, causing "Unknown rule" errors
  - The rule now works correctly for detecting forbidden terms like TODO, FIXME, etc.

## [0.0.184] - 2025-11-28

### Added

- **MD061 (forbidden-terms): New rule to flag forbidden terms in documents**
  - Configurable list of forbidden terms with case-sensitivity options
  - Useful for style guides, compliance, and consistency enforcement

### Fixed

- **Code blocks: Respect CommonMark fence indentation limit (0-3 spaces)**
  - CommonMark spec limits fenced code block indentation to 0-3 spaces relative to container
  - Document-level fences must have at most 3 spaces of indentation
  - Fences inside list items are correctly recognized (indentation is relative to list content)
  - Prevents false negatives when checking content inside deeply nested structures

- **LSP: Use whole-table fixes for table rules (MD055, MD056, MD058, MD060)**
  - Table rule fixes now replace the entire table instead of individual cells
  - Prevents fix corruption when multiple cells need adjustment
  - Improves reliability of auto-fix in IDEs

## [0.0.183] - 2025-11-27

### Fixed

- **MD056 (table-column-count): Respect MkDocs flavor for pipes in inline code (fixes #165)**
  - In MkDocs/Python-Markdown flavor, pipes inside backticks are NOT cell delimiters
  - Tables with inline code containing pipes (e.g., `` `x | y` ``) no longer trigger false positives
  - GFM flavor behavior unchanged (pipes in code ARE delimiters per spec)

- **MD060 (table-format): Respect MkDocs flavor for pipes in inline code**
  - Consistent with MD056 fix for MkDocs flavor support
  - Table formatting now correctly handles inline code with pipes in MkDocs mode

- **Schema: Use kebab-case for GlobalConfig properties**
  - JSON schema now uses kebab-case (e.g., `line-length`) matching config file conventions
  - Improves IDE auto-completion and validation for configuration files

### Changed

- **Refactor: Consolidate table row parsing into TableUtils**
  - Unified table parsing logic into shared utility module
  - Reduces code duplication between MD056 and MD060

- **Refactor: Decompose MD013 into module structure**
  - Split large `md013_line_length.rs` into separate files:
    - `mod.rs`: Main rule implementation
    - `helpers.rs`: Helper functions
    - `tests.rs`: Unit tests
  - Improves code organization and maintainability

## [0.0.182] - 2025-11-25

### Added

- **cache clean command**
  - Added `rumdl clean` command to clear the cache directory
  - Useful for forcing re-linting of all files

- **MD013 (line-length): Visual width mode for CJK and emoji**
  - Added `visual-width` option that counts CJK characters as double-width
  - Properly handles emoji and other wide characters for accurate line length checking
  - Useful for East Asian language documentation

- **MD013 (line-length): Custom abbreviation support**
  - Added `abbreviations` config option for defining custom abbreviations
  - Custom abbreviations are respected during sentence-per-line reflow
  - Prevents unwanted line breaks after user-defined abbreviations

- **Configuration file discovery: .config/rumdl.toml support**
  - Added support for `.config/rumdl.toml` as a valid configuration location
  - Follows XDG Base Directory-style pattern for project-level config
  - Discovery order: `.rumdl.toml` → `.config/rumdl.toml` → `pyproject.toml`

- **MD022 docs: Per-level configuration documentation**
  - Added documentation for per-level `lines-above` and `lines-below` arrays
  - Shows how to specify different spacing requirements for each heading level (h1-h6)

### Fixed

- **MD042 (no-empty-links): Wiki-style links no longer flagged as empty (fixes discussion #153)**
  - Wiki-style links like `[[Page Name]]`, `[[Folder/Page]]`, and `[[Page|Display Text]]` are now recognized
  - Prevents false positives for Obsidian/Notion-style wiki link syntax
  - Block references like `[[#^block-id]]` are also handled correctly

- **MD056 (table-column-count): Handle escaped pipes correctly in GFM tables (fixes #163)**
  - GFM table parsing now correctly handles backslash-escaped pipes (`\|`)
  - Inline code spans do NOT protect pipes from being cell delimiters (per GFM spec)
  - Only `\|` is treated as literal pipe content

- **MD060 (table-format): Accept tables that are already aligned (fixes #164)**
  - Tables with consistent column widths across all rows are now accepted as aligned
  - Short separator rows (e.g., `:--` instead of `:---`) no longer trigger reformatting
  - Reduces false positives for manually aligned tables

### Changed

- **Documentation: Corrected timestamps in CHANGELOG for versions 0.0.172-0.0.178**

## [0.0.181] - 2025-11-20

### Added

- **cache-dir configuration option**
  - Added support for `cache-dir` in `.rumdl.toml` and `pyproject.toml` config files
  - Precedence: CLI `--cache-dir` → `RUMDL_CACHE_DIR` env var → config file → default
  - Follows Ruff's configuration pattern for cache directory management
- **Termux installation support**
  - Added Termux User Repository install method to README (thanks @ha1ix in #157)

### Changed

- **BREAKING: Cache directory renamed from `.rumdl-cache` to `.rumdl_cache`**
  - Aligns with Ruff's naming convention (`.ruff_cache`)
  - Follows Rust ecosystem preference for underscores over hyphens
  - Migration: Old `.rumdl-cache` directories can be safely deleted

### Fixed

- **LSP: Exclude Unfixable rules from formatting and Fix All (fixes #158)**
  - Filter warnings from Unfixable rules (like MD033) before applying fixes during LSP formatting
  - Prevents unintended destructive changes (e.g., HTML deletion) during document formatting
  - Unfixable rules still available through Quick Fix actions for user choice
  - Fix All action now correctly excludes Unfixable rules
- **Cache directory now created at project root, not CWD (fixes #159)**
  - Cache directory is now anchored to the project root (determined by `.git` location)
  - Prevents multiple cache directories when running from subdirectories
  - Works consistently whether invoked from project root or any subdirectory
  - Behavior:
    - **With .git**: Cache at `.git` parent directory (true project root)
    - **Without .git**: Cache at config file location (fallback)
  - Follows Ruff's fix (PR #7962) for consistent cache placement

## [0.0.180] - 2025-11-19

### Added

- **MD022 (blanks-around-headings): Per-level unlimited blank handling**
  - Added per-level configuration to allow unlimited blank lines around specific heading levels
  - Enables fine-grained control over blank line requirements for different heading levels
  - Supports both global and per-level unlimited blank line settings

### Fixed

- **Range utilities: Handle UTF-8 character boundaries safely (fixes #154)**
  - Fixed panic when processing files with multi-byte UTF-8 characters (CJK, emoji)
  - String slicing operations now safely handle byte indices that fall in the middle of multi-byte characters
  - Added comprehensive test suite covering Korean, Chinese, Japanese characters, emoji, and edge cases
  - All string slicing operations now occur at valid UTF-8 character boundaries

- **MD041 (first-line-heading): HTML comments are now correctly skipped (fixes #155)**
  - HTML comments before headings are now treated as non-visible content and ignored
  - Aligns with documentation and markdownlint behavior
  - Multi-line HTML comments are properly handled using parser metadata
  - Fixed handling of deeply nested HTML headings

## [0.0.179] - 2025-11-18

### Fixed

- **MD041 (first-line-heading): Fixed multiline HTML detection for headings**
  - Replace manual HTML parsing with centralized `ctx.html_tags()` parser
  - Correctly handles nested HTML elements with multiline attributes
  - Fixes Issue #152: No longer incorrectly flags headings when preceded by multiline HTML

- **MD037 (no-space-in-emphasis): Improved warning message clarity**
  - Truncate long emphasis text in warning messages to prevent display issues
  - Long emphasis spans are now shown with ellipsis for better readability

### Performance

- **MD005 (list-indent): Eliminated O(n²) complexity by pre-computing parent relationships**
  - Pre-compute parent list item relationships during initial parsing
  - Reduces algorithmic complexity from O(n²) to O(n) for deeply nested lists
  - Improves performance for files with complex list structures

- **Optimized link/image regex patterns to prevent catastrophic backtracking**
  - Added atomic grouping and possessive quantifiers to regex patterns in `lint_context.rs`
  - Prevents regex engine from exponential backtracking on pathological inputs
  - Improves parsing speed for files with many links and images

### Changed

- **MD033 (no-inline-html): Refactored to use centralized HTML parser**
  - Removed 109 lines of broken `find_multiline_html_tags()` function that detected 0 multiline HTML tags
  - Replaced two-pass approach (single-line regex + multiline detection) with single-pass using `ctx.html_tags()`
  - Now correctly detects all multiline HTML tags (previously missed 100% of them)
  - Follows pattern from MD041 refactoring for consistency
  - Preserves all existing filters (code blocks, kramdown, comments, etc.)

### Added

- **Comprehensive MD005 test coverage**
  - Added 21 new edge case tests for list indentation detection
  - Updated unicode test to verify dynamic parent relationship detection
  - Ensures robustness for nested and complex list structures

## [0.0.178] - 2025-11-17

### Performance

- **Critical: Eliminated O(n²) bottleneck in list block parsing (Issue #148)**
  - Replaced nested loop with forward-scanning O(n) algorithm using state tracking
  - Performance improvement: 900-line files with nested lists now parse in ~20ms (previously 50+ seconds)
  - User-reported case: 890-line file improved from 50.2s to ~0.2s (250x speedup)
  - User-reported case: 1780-line file improved from 347s to ~0.4s (867x speedup)
  - Algorithm now scales linearly O(n) instead of quadratically O(n²)
  - Uses two tracking variables to detect list-breaking content and validate continuation indentation
  - Extracted `reset_tracking_state()` helper to eliminate code duplication
  - Replaced magic number 2 with `UNORDERED_LIST_MIN_CONTINUATION_INDENT` constant

### Changed

- **Code cleanup: Removed unused O(n²) trap methods**
  - Removed `links_on_line()` and `images_on_line()` methods from `LintContext`
  - These methods were never used but could cause performance issues if called in loops

### Added

- **Comprehensive regression tests for list block parsing**
  - 27 new tests covering edge cases: consecutive items, list-breaking content, indentation rules
  - Performance tests verify O(n) scaling for 1000+ item lists
  - Tests reproduce exact Issue #148 pattern (nested lists with brackets)

## [0.0.177] - 2025-11-17

### Fixed

- **MD003 (heading-style): Fixed non-deterministic test failure**
  - Eliminated flaky behavior caused by HashMap iteration order when multiple heading styles had equal counts
  - Added deterministic tiebreaker that prefers ATX style over Setext styles
  - Ensures consistent style detection across all test runs

- **MD044 (proper-names): Added international character support**
  - Added tilde characters (ã, õ) and Nordic å to ASCII normalization
  - Enables proper name matching for international cities like São Paulo, Coimbra, and Århus

- **MD013 (line-length): Fixed infinite hang in sentence-per-line reflow mode**
  - Fixed abbreviation detection causing hang on words ending in letter sequences
  - Corrected logic to properly detect abbreviations vs. sentence endings
  - Issue #150: Resolves infinite loop that occurred with certain text patterns

- **Table style detection improvements**
  - MD055/MD056/MD058: Now analyze all table rows to detect style, not just first row
  - Prevents false positives when first row has different separator style than rest of table

- **Code quality fixes**
  - Resolved Issues #151 and #152 with comprehensive test coverage
  - Eliminated code duplication in MD005, MD042, and other rules
  - Extracted shared helper methods to reduce maintenance burden

### Added

- **MD013: Comprehensive abbreviation detection tests**
  - 42 test cases covering sentence-per-line reflow edge cases
  - Validates correct behavior for abbreviations, acronyms, and punctuation patterns

- **Text reflow test organization**
  - Separated 42 integration tests into dedicated test file (`text_reflow_test.rs`)
  - Follows established codebase pattern for test organization
  - Improves maintainability while keeping 1 private helper test inline

### Changed

- **MD029 (ordered-list-prefix): Improved warning messages**
  - Messages now include contextual style information
  - Shows both configured style and detected document style for clarity
  - Examples: "configured style 'one'" vs "document style 'ordered'"

- **MD029 rule name standardization**
  - Removed deprecated MD029-style suffix variant
  - Unified to single consistent "MD029" identifier across codebase

### Performance

- **Memory optimization in MD005 and MD042**
  - Reduced memory allocations in list consistency and link reference checks
  - More efficient data structure usage

## [0.0.176] - 2025-11-14

### Added

- **Git-style intelligent configuration merging**
  - User and project configs can now intelligently combine instead of simply overriding
  - `disable` array uses union semantics - user can add to project disables
  - `enable` array uses replace semantics - project can enforce specific rules
  - When a rule appears in both `enable` and `disable`, `enable` wins (project can override user)
  - Configuration hierarchy is now explicit and follows Git's pattern:
    - Default (0) < UserConfig (1) < PyprojectToml (2) < ProjectConfig (3) < CLI (4)
  - Renamed config sources for clarity:
    - `ConfigSource::RumdlToml` → `UserConfig` (global user config)
    - `ConfigSource::RumdlToml` → `ProjectConfig` (project-level config files)
    - Removed `Markdownlint` source (now uses `ProjectConfig`)
  - New API methods:
    - `SourcedValue::merge_union()` - additive merging for disable arrays
    - `SourcedValue::merge_override()` - replacement merging for enable arrays
  - Example use cases:
    - User disables MD013 globally → Project enables MD013 → MD013 is enabled ✓
    - Project disables MD001, MD003 → User disables MD013 → All three are disabled ✓
  - 17 comprehensive tests covering merge semantics, precedence, and conflict resolution
  - Matches configuration pattern used by git, eslint, prettier, and other modern tools

### Performance

- **Major memory optimizations eliminating string allocations**
  - Converted `LineInfo` to use byte ranges instead of owned strings (50-80% memory reduction)
  - Eliminates N string allocations where N = number of lines in document
  - Added zero-copy `content(&self, source: &str) -> &str` method for on-demand access
  - Converted `LineIndex` to borrow `&'a str` instead of cloning (eliminates one full document copy per lint operation)
  - Converted `ParsedLink`/`ParsedImage` to use `Cow<'a, str>` (60-80% reduction in heap allocations for link/image parsing)
  - Zero-cost when borrowing, minimal overhead when owning - most data borrowed directly from source

- **Eliminated O(n²) complexity in multiple rules**
  - Fixed quadratic bottlenecks in MD027 (Multiple spaces after blockquote symbol)
  - Fixed quadratic bottlenecks in MD020 (No space inside hashes on closed atx style heading)
  - Fixed quadratic bottlenecks in MD046 (Code block style)
  - All three rules now use pre-computed context data with O(1) lookups

- **Optimized CLI commands**
  - `rumdl config` now executes in ~18ms (eliminated duplicate rule instantiation)
  - Removed duplicate instantiation of ~50 rule objects between main.rs and formatter.rs
  - Cleaner architecture with single source of truth for rule instances

### Fixed

- **Config loading: User config now always loaded as base layer (#131)**
  - Fixed LSP server ignoring user config when finding project-level config files
  - User configuration is now always loaded first (unless `--no-config` is used)
  - Project configs merge on top of user config, CLI flags have highest priority
  - Configuration hierarchy now consistent between CLI and LSP:
    1. User/global config (`~/.config/rumdl/rumdl.toml`) - base layer
    2. Project config (discovered or explicit) - overrides user config
    3. CLI flags - highest priority
  - Matches pattern used by git, eslint, prettier, and other tools
  - Added regression test verifying user config preserved with explicit project config

- **MD035: Frontmatter delimiter false positives (#40)**
  - Fixed incorrect flagging of YAML/TOML frontmatter delimiters (`---`/`+++`) as horizontal rules
  - Rule now correctly skips frontmatter in three places:
    - `most_prevalent_hr_style()` - don't count frontmatter HRs for prevalence
    - `check()` - don't flag frontmatter delimiters as violations
    - `fix()` - don't replace frontmatter delimiters when fixing
  - Uses pre-computed `LineInfo.in_front_matter` field for efficient detection

### Changed

- **Architecture: Consistent use of pre-computed context data**
  - MD035 now uses pre-computed `LineInfo.in_front_matter` instead of function calls
  - All rules now consistently use `ctx.line_index` for line-based operations
  - Eliminates redundant function calls and O(n) scans
  - More efficient with O(1) field access patterns

- **Test infrastructure improvements**
  - Implemented dynamic fixture downloading for performance tests
  - Test downloads now happen on-demand instead of being checked into repository
  - Better handling of large test files for performance benchmarking

- **Test quality improvements**
  - Corrected ESM block test assertions to match actual MDX behavior
  - ESM blocks only exist at TOP of MDX files and end at first non-ESM line
  - Tests now assert correct behavior rather than documenting implementation quirks
  - Fixed MD033 test assertions to properly validate HTML inline rules

### Breaking Changes

**For library users only** (CLI users unaffected):

- `LineInfo.content` field is no longer public - use new `content(source: &str)` method instead
- Since `LineInfo` was primarily internal API, impact should be minimal

## [0.0.175] - 2025-11-12

### Added

- **Universal wiki-link and Obsidian block reference support**
  - Added recognition of `[[wiki-links]]` across all rules
  - Added support for Obsidian block references `[[note#^block-id]]`
  - Prevents false positives in link/heading detection rules
  - Works with all extended markdown flavors that support wiki-links

- **Type-safe configuration wrappers**
  - Added `HeadingLevel` type with validation (1-6 range)
  - Added type-safe wrappers for MD007, MD009, MD010 configuration
  - Added type-safe configuration for MD013 and MD030
  - Added type-safe configuration for MD022, MD012, MD060
  - Compile-time validation prevents invalid configuration values
  - Backward compatible with snake_case field names via serde aliases

- **Memory profiling for non-Linux platforms**
  - Added cross-platform memory profiling support
  - Uses physical memory instead of virtual memory for benchmarking
  - Enables performance analysis on macOS and Windows

### Fixed

- **MD041: mdBook preprocessor directive false positives**
  - MD041 now skips files containing only mdBook preprocessor directives (e.g., `{{#include file.md}}`)
  - These composition/routing files are not standalone content and don't require headings
  - Eliminates false positives on mdBook include-only files
  - Supports all mdBook directives: `{{#include}}`, `{{#playground}}`, `{{#rustdoc_include}}`, etc.
  - Handles files with directives mixed with HTML comments
  - Added comprehensive test coverage for mdBook directive patterns

- **MD042: Inline code in link text false positives**
  - Fixed incorrect flagging of code spans within link text
  - MD042 now correctly handles patterns like `[code with \`backticks\`](url)`
  - Prevents false positives when links contain inline code
  - Added regression test covering this case

- **Config import: markdownlint option mapping (#137)**
  - `rumdl import` now correctly maps markdownlint-specific option names to rumdl equivalents
  - MD013: Maps `stern` → `strict`, warns about incompatible options (code_block_line_length, heading_line_length)
  - MD054: Warns about incompatible `style`/`styles` options (rumdl uses individual boolean flags)
  - Prevents "Unknown option" warnings when importing markdownlint configs
  - Added helpful warnings explaining config model differences

- **MD011: Footnote reference false positives (#147)**
  - Fixed incorrect flagging of valid markdown: `[link](url)[^footnote]`
  - MD011 now skips footnote references (starting with `^`)
  - Already had support for reference links, now extended to footnotes
  - Added regression test covering this case

- **MD013: Front matter line length (#146)**
  - MD013 now correctly skips YAML/TOML front matter blocks
  - Prevents false positives on long lines in document metadata
  - Works with both `---` (YAML) and `+++` (TOML) front matter delimiters
  - Added regression test covering this case

- **Config system: kebab-case support**
  - MD041: Converted to serde-based config with kebab-case support
  - Added backward compatibility aliases for snake_case field names
  - Now properly supports both `front-matter-title` and `front_matter_title`
  - Removed undocumented config options from MD032 and MD038

- **MD004: Simplified configuration**
  - Refactored to use derives instead of manual implementation
  - Cleaner code while maintaining full functionality

- **Test infrastructure improvements**
  - Migrated from deprecated `Command::cargo_bin` to `cargo_bin_cmd!` macro
  - Fixed Homebrew installation docs (removed unnecessary `brew tap`)

### Changed

- **Consistency checking: Prevalence-based style detection**
  - Converted MD003, MD004, MD046, MD048, MD049, MD050, MD055 from "first-found" to "prevalence-based" logic
  - Rules now count ALL occurrences and enforce the most commonly used style
  - Tie-breakers prefer industry standards (GitHub, CommonMark recommendations)
  - More accurate representation of document's actual style intent
  - Better user experience: rules adapt to what you mostly use, not just the first instance
  - Affected rules:
    - MD003 (heading-style): ATX vs Setext vs Closed ATX
    - MD004 (ul-style): dash, asterisk, plus, or consistent
    - MD046 (code-block-style): fenced vs indented
    - MD048 (code-fence-style): backticks vs tildes
    - MD049 (emphasis-style): asterisk vs underscore
    - MD050 (strong-style): asterisk vs underscore
    - MD055 (table-pipe-style): leading/trailing pipe consistency
  - Updated documentation to reflect prevalence-based behavior

- **Dependencies**
  - Upgraded schemars from 0.8 to 1.1
  - Updated all dependencies to latest compatible versions
  - Updated dependencies to latest patch versions

## [0.0.174] - 2025-11-10

### Fixed

- **MD046: False positives with multi-paragraph footnotes (#142)**
  - Fixed incorrect flagging of indented continuation paragraphs in footnote definitions
  - Added comprehensive footnote detection following CommonMark footnote extension spec
  - Implements proper block continuation context tracking for footnotes
  - Prevents confusion between actual indented code blocks and footnote continuations
  - Example that now works correctly:

    ```markdown
    [^1]: First paragraph of footnote.

        Second paragraph (indented, but not a code block).
    ```

  - Added 11 specification-based tests ensuring robust footnote handling

- **CLI: Differentiate `--quiet` and `--silent` flags (#141)**
  - `--quiet`: Shows diagnostics but suppresses progress/summary (matches Ruff behavior)
  - `--silent`: Suppresses all output including diagnostics (exit code still reflects violations)
  - Previously both flags behaved identically, hiding all output
  - Updated help text to clarify the distinction

- **LSP: Server now respects `--config` argument (#140)**
  - Added proper support for `--config` flag in `rumdl server` command
  - Config file path validation with clear error messages for missing files
  - Config path properly passed through entire LSP initialization stack
  - Enables custom configuration files for LSP/editor integrations

## [0.0.173] - 2025-11-08

### Added

- **MD060: Table format rule with Prettier-style auto-compact**
  - New rule enforcing consistent table column alignment (aligned/compact/tight styles)
  - **Auto-compact threshold**: Tables exceeding max-width automatically use compact formatting
  - Configurable via `max-width` setting (0 = inherit from MD013's line-length)
  - Generates informative warnings showing actual width vs threshold
  - Disabled by default (opt-in feature)
  - Handles edge cases: zero-width characters, escaped pipes, HTML comments
  - Respects column alignments (left/center/right) in aligned mode

- **MD043: Wildcard pattern support for heading structures**
  - New wildcard patterns for flexible heading structure validation
  - Allows `*` placeholders in heading text for dynamic content
  - Edge case handling for complex heading hierarchies

- **MD044: HTML elements configuration option**
  - New `html-elements` configuration to customize proper name handling
  - Allows project-specific proper name enforcement

- **RUMDL_CACHE_DIR environment variable**
  - Override default cache directory location via environment variable
  - Useful for CI/CD pipelines and custom cache management

### Fixed

- **Definition list support in reflow mode (#136)**
  - Definition lists (Extended Markdown syntax) are now correctly preserved during text reflow
  - Prevents incorrect joining of terms with definitions (e.g., `Term\n: Definition` no longer becomes `Term : Definition`)
  - Supports PHP Markdown Extra, Kramdown, Pandoc, Hugo, and other extended Markdown flavors
  - Works with all reflow modes including sentence-per-line

- **MD054: Error message alignment**
  - Corrected error messages to match implementation behavior
  - Documentation now accurately reflects link/image style validation

- **MD041: Front matter title config schema**
  - Added missing `front_matter_title` options to configuration schema
  - Fixes config validation errors when using front matter title feature

- **LSP: pyproject.toml validation**
  - LSP now verifies `[tool.rumdl]` section exists before using pyproject.toml
  - Prevents errors when pyproject.toml exists but doesn't contain rumdl config

- **CLI: Rule registration**
  - Fixed MD057, MD059, MD060 rules not being registered in CLI
  - All rules now properly available for use

### Changed

- **Documentation: TOML format migration**
  - Converted all configuration examples from YAML to TOML
  - Reflects modern configuration best practices
  - Improved clarity for MD013 reflow requirement in README

## [0.0.172] - 2025-11-06

### Added

- **MD059: Link text style rule achieving 100% markdownlint compatibility**
  - New rule for enforcing consistent link text formatting
  - Matches markdownlint column reporting behavior
  - Completes full parity with markdownlint rule set

- **pulldown-cmark BrokenLink callback integration**
  - Enhanced link reference validation using parser callback
  - Better detection of undefined reference-style links
  - Improved accuracy for MD052 and related rules

### Fixed

- **Critical: UTF-8 character boundary panic in ordered list detection**
  - Fixed crash when processing markdown with multi-byte UTF-8 characters (Japanese, Chinese, Korean, etc.)
  - Root cause: Character index was incorrectly used as byte index for string slicing
  - Now converts character positions to byte positions using `char_indices().nth()`
  - Discovered during validation on javascript-algorithms repository (193k ⭐)
  - Affected any non-English documentation with multi-byte UTF-8 near numbered lists

- **pulldown-cmark escaped bracket workaround**
  - Workaround for pulldown-cmark bug where `\[` and `\!` are incorrectly parsed as links/images
  - Violates CommonMark spec Example 14 for backslash escapes
  - Filters 90% of false positives with byte-level escape pattern detection
  - Correctly handles: `\[escaped\]`, `\![not an image](url)`
  - Known limitation: `\[text][ref]` reference-style links still produce 1 false positive
  - Bug report filed for upstream fix

- **MD042: False positives on autolinks**
  - Fixed empty link detection incorrectly flagging autolinks like `<https://example.com>`
  - Autolinks are now correctly excluded from empty link checks

- **MD033: HTML tags inside HTML comments**
  - Fixed false positives when HTML tags appear inside HTML comments
  - Centralized HTML comment detection for consistent behavior across rules
  - HTML comments now properly ignored: `<!-- <div>not flagged</div> -->`

- **MD051: GitHub ASCII normalization**
  - Implemented proper GitHub anchor generation with ASCII normalization
  - Handles edge cases like backtick-wrapped angle brackets: `` `<FILE>` `` → `#file`
  - Verified against actual GitHub.com rendering behavior

- **MD052: Complete Jinja template support**
  - Added Jinja template range checks to regex-based shortcut reference detection
  - Fixes false positives on `[reference]` patterns in Jinja templates
  - Completes Jinja support across all link-related rules (MD039, MD042, MD051, MD052)

- **MD039/MD042/MD051: Jinja template checks**
  - Added Jinja template range checks to prevent false positives
  - All rules using `ctx.links`/`ctx.images` now skip Jinja template markers
  - Validated on real-world Jinja templates (pyo3/.towncrier.template.md)

- **Link/image text extraction: Whitespace preservation**
  - Fixed loss of leading/trailing whitespace in link text and image alt text
  - pulldown-cmark strips newlines from Text events, causing whitespace loss
  - Now extracts directly from source bytes between brackets for perfect fidelity
  - Verified: `[\nNewline\n](url)` → text correctly preserved with newlines

### Changed

- **⚠️ BREAKING: Removed MD002 (first-heading-h1) and MD006 (ul-start-left) rules**
  - Removed for 100% markdownlint compatibility
  - markdownlint deprecated these rules in favor of more specific alternatives
  - **Migration**: Remove MD002 and MD006 from your `.rumdl.toml` config if present

- **Refactoring: Modernized codebase with Rust 1.91.0 features**
  - Migrated `once_cell::sync::Lazy` to `std::sync::LazyLock` (stable in Rust 1.80)
  - Improved error handling by replacing `unwrap()`/`expect()` with proper error propagation
  - Replaced defensive fallbacks with `unreachable!()` where appropriate
  - Eliminated unreachable panics and improved code robustness
  - Removed `_fix` boolean redundancy and cleaned up dead code

- **Refactoring: HTML comment detection centralized**
  - MD033 now uses centralized `html_utils::find_html_comments()` for consistency
  - Eliminates duplicate HTML comment parsing logic

### Performance

- **12-15% speed improvement from pulldown-cmark migration**
  - Migrated link/image parsing from regex-based approach to pulldown-cmark parser
  - Benchmark results: 12-15% faster on representative markdown files
  - More accurate CommonMark compliance
  - Better handling of edge cases and complex markdown structures

## [0.0.171] - 2025-11-03

### Added

- **MD060: Table column formatting with markdownlint compatibility**
  - Full table formatting rule with auto-fix capability
  - Four formatting styles: `aligned`, `compact`, `tight`, and `any` (default)
  - `any` style detects existing formatting and enforces consistency
  - Supports CJK characters and emoji
  - Handles alignment indicators (`:---`, `:---:`, `---:`)
  - Skips ZWJ emoji sequences to prevent corruption
  - Configuration: `MD060.enabled = false` (opt-in), `MD060.style = "any"`

### Fixed

- **MD053: CommonMark compliance for list continuations**
  - Fixed false positives when backtick references appear in list continuation paragraphs
  - Enhanced ordered list marker detection for multi-digit lists (10., 123.)
  - Implemented column-based indentation model per CommonMark spec
  - Distinguishes list continuation paragraphs from code-in-lists
  - Validated against 1,562 markdown files with 0 false positives

- **LSP: Global config fallback**
  - Fixed global config loading when no project-specific config exists
  - Now properly falls back to `~/.config/rumdl/.rumdl.toml`
  - Added test coverage for fallback behavior

- **LSP: Fixed test expectations for camelCase serialization**
  - LSP initialization options use camelCase per LSP specification
  - Corrected test assertions to match actual camelCase behavior

- **Fix mode: Type-safe exit code semantics**
  - Added `FixMode` enum for clear fix/check mode distinction
  - Improved code maintainability and type safety

### Changed

- **⚠️ BREAKING: MD013 `tables` default changed to `false`**
  - Previously defaulted to `true`, causing conflicts with table formatting rules
  - Now defaults to `false` for better user experience
  - **Migration**: If you rely on MD013 checking table line lengths, add to `.rumdl.toml`:

    ```toml
    [MD013]
    tables = true
    ```

  - This prevents false positives when using MD060 or other table formatting tools

## [0.0.170] - 2025-10-31

### Added

- **Include non-standard file extensions** (#127)
  - New `--include` CLI flag to check files with non-standard extensions
  - Example: `rumdl check --include "*.txt" --include "*.text"`
  - Useful for documentation files with custom extensions
  - Respects `.rumdl.toml` configuration: `include = ["*.txt", "*.text"]`

### Fixed

- **MD055: Preserve user formatting when fixing pipe placement** (#129)
  - Changed from full table reconstruction to surgical pipe addition/removal
  - User's intentional spacing and alignment are now preserved
  - Follows Unix philosophy: do one thing (fix pipes) well
  - Example: `| Cell 1   | Cell 2` → `| Cell 1   | Cell 2 |` (spacing preserved)

- **MD053: Allow backtick references with `::` and spaces** (#128)
  - Fixed false positives for Rust-style references like `` [`std::vec::Vec`] ``
  - Now correctly handles references containing `::` within backticks
  - Improves accuracy for Rust documentation and technical content

### Documentation

- **MD028: Clarified rule behavior for consecutive blockquotes** (#126)
  - Updated documentation to accurately reflect that MD028 flags consecutive blockquote *starts*
  - Not a bug - working as designed per markdownlint specification
  - Prevents confusion about expected behavior

### Changed

- **Rust toolchain updated to 1.91.0** (from 1.89.0)
  - Ensures compatibility with latest Rust features and improvements
  - Better performance and compilation times

## [0.0.169] - 2025-10-30

### Performance

**MASSIVE PERFORMANCE IMPROVEMENTS**: This release delivers 7-53x faster linting through systematic elimination of O(n²) bottlenecks and algorithmic optimizations. rumdl is now 16-29x faster than
markdownlint-cli2 on real-world repositories.

- **Fix catastrophic O(n²) bottleneck in FilteredLinesIter** ⭐ THE GAME CHANGER
  - Eliminated `content.lines().collect()` from inside Iterator::next() method
  - Single-file optimization delivered 7,600x speedup for affected rules
  - MD011: 6.00s → 0.785ms (7,644x faster)
  - MD012: 5.95s → 1.643ms (3,621x faster)
  - Impact: Stress test (10k lines): 9,987ms → 187ms (53x faster)
  - Impact: Rust Book (478 files): 2,646ms → 269ms (9.8x faster)
  - Impact: PyO3 (76 files): 2,004ms → 262ms (7.6x faster)

- **Pre-compute LintContext data structures to eliminate redundant work**
  - LineIndex: Eliminated 46× content cloning across all rules
  - Jinja template ranges: Pre-computed once instead of O(n×m) scanning
  - Table blocks: Computed once instead of 4× independent scans per document
  - HTML comment ranges: Pre-computed with 50-70x speedup
  - Code block line mapping: Optimized with binary search instead of linear scan
  - Autodoc blocks (MkDocs): Pre-computed to avoid O(n²) scaling

- **Optimize link and image parsing with binary search**
  - Replaced linear search with binary search for code span checks
  - Faster link/image parsing across all reference-based rules

- **Replace regex with byte-level parsing for better performance**
  - List detection: Manual byte scanning instead of regex
  - Blockquote prefix: Byte-level parsing instead of regex
  - Line info parsing: Eliminated redundant blockquote prefix parsing

- **Streaming parser architecture**
  - Replaced AST parsing with pulldown-cmark streaming for lower memory overhead
  - Better scalability for very large documents

- **MD046 optimization**
  - Pre-compute list/tab contexts
  - Use ctx.code_blocks instead of independent detection

### Benchmarks

**vs Previous Version (v0.0.168):**

- Rust Book (478 files): 2,646ms → 269ms (9.8x faster)
- Stress Test (10,514 lines): 9,987ms → 187ms (53.4x faster)
- PyO3 (76 files): 2,004ms → 262ms (7.6x faster)

**vs markdownlint-cli2 (industry standard):**

- Rust Book: 10.3s vs 0.35s (29.4x faster)
- PyO3: 4.4s vs 0.26s (16.9x faster)

**Current Performance:**

- 0.7ms per file (Rust Book)
- 6.3x parallelization efficiency
- Zero O(n²) algorithmic bottlenecks remaining

### Added

- **MDX and Quarto flavor support**
  - New flavors: `MarkdownFlavor::MDX` and `MarkdownFlavor::Quarto`
  - JSX component handling: Skip linting inside JSX elements
  - ESM import/export support: Ignore JavaScript import/export statements
  - Quarto code chunk support: Recognize `{r}`, `{python}` chunk delimiters
  - Essential for modern React-based documentation and data science workflows

### Fixed

- **MD011: Intelligent URL vs text detection in reversed links**
  - Smarter classification of link components as URL or text
  - Reduces false positives in edge cases like `(Generic<T>)[link]`
  - Correctly handles `(http://url)[text]` vs `(text)[#anchor]`
  - Improved heuristics for ambiguous single-word patterns

- **MD013: Preserve HTML blocks in list items during reflow**
  - HTML blocks inside list items are now preserved during auto-fix
  - Prevents broken indentation and tag structure
  - Better handling of complex nested content

### Changed

- Architecture improvements for pre-computation pattern
  - LintContext now owns shared data structures
  - Rules consume pre-computed data instead of duplicating work
  - Consistent binary search APIs for range lookups
  - Foundation for future optimizations

## [0.0.168] - 2025-10-28

### Added

- **CLI: Support for Quarto (.qmd) and RMarkdown (.rmd/.Rmd) files**
  - Added support for `.qmd` (Quarto), `.rmd` and `.Rmd` (RMarkdown) file extensions
  - Enables markdown linting for data science and scientific documentation workflows
  - Particularly useful for Jupyter-based publishing and reproducible research documents

- **LSP: will_save_wait_until for proper auto-fix on save**
  - Implemented LSP `textDocument/willSaveWaitUntil` capability
  - Provides proper auto-fix on save support in compatible editors
  - More reliable than `textDocument/didSave` approach
  - Ensures fixes are applied before file is actually saved to disk

- **MD033: LSP Quick Fix to remove HTML tags while keeping content**
  - New code action removes HTML opening and closing tags while preserving inner content
  - Helps convert inline HTML to plain text when needed
  - Available through editor Quick Fix menu (Ctrl+. or Cmd+.)

### Fixed

- **MD013: Skip auto-fix for list items containing HTML tags**
  - Prevents broken auto-fix when HTML tags are present in list items
  - HTML structure (indentation, tag hierarchy) is now preserved
  - Errors are still reported but no destructive fix is applied
  - Pragmatic solution until full HTML-aware reflow is implemented
  - Fixes issues where tags like `</details>` were split or indentation was lost

- **MD013: Fix incorrect sentence splitting after abbreviations in sentence-per-line mode**
  - No longer incorrectly splits sentences after common abbreviations (e.g., etc., i.e., Dr., Mr.)
  - Improved sentence detection algorithm for better accuracy
  - Prevents unwanted line breaks in the middle of sentences

- **MD013: Prevent autolinks from being parsed as HTML tags**
  - Autolinks like `<https://example.com>` are no longer treated as HTML
  - Fixes false positives in HTML detection that could skip auto-fix unnecessarily
  - Properly distinguishes between markdown autolinks and actual HTML tags

- **MD013: Prevent content duplication in sentence-per-line reflow**
  - Fixed bug where content could be duplicated during paragraph reflow
  - Ensures each sentence appears exactly once in reflowed output
  - Improves reliability of auto-fix in sentence-per-line mode

- **MD013: Improve sentence-per-line error messages and highlighting**
  - Better error messages that clearly indicate sentence-per-line violations
  - More accurate highlighting of problematic text
  - Helps users understand what needs to be fixed

- **MD013: Skip template directives at paragraph start in sentence-per-line mode**
  - Template directives (like `{{ variable }}`) at start of paragraphs no longer cause issues
  - Prevents false positives in templated markdown files
  - Improves compatibility with static site generators and template engines

- **MD013: Treat template directives as paragraph boundaries**
  - Template directives now properly separate paragraphs during reflow
  - Prevents template syntax from being joined with regular content
  - Better handling of mixed template and markdown content

- **MD013: Join single-sentence paragraphs in sentence-per-line mode**
  - Single-sentence paragraphs that span multiple lines are now properly joined
  - Fixes issues where short paragraphs were incorrectly flagged
  - Improves consistency of sentence-per-line formatting

- **MD013: Handle multiple spaces and multi-line paragraphs in sentence-per-line mode**
  - Better handling of paragraphs with inconsistent spacing
  - Multi-line paragraphs are now correctly reflowed
  - Fixes edge cases in whitespace handling

- **MD052: Preserve backtick-wrapped patterns with dots in MkDocs mode**
  - Backtick-wrapped references like `` `[foo.bar]` `` now preserve dots in slugs
  - Matches MkDocs behavior of treating code-wrapped text literally
  - Prevents false positives for code examples in documentation

- **MD052: Support Pandoc citations and inline footnotes in RMarkdown/Quarto**
  - Recognizes Pandoc citation syntax: `[@citation]`, `@citation`, `[-@citation]`
  - Recognizes inline footnote syntax: `^[footnote text]`
  - No longer treats these as reference-style links
  - Essential for academic and scientific writing in RMarkdown and Quarto

- **MD033: Only report opening HTML tags, not closing tags**
  - MD033 violations now only report the opening tag of an HTML element
  - Reduces noise in linting output (one violation per element vs two)
  - Closing tags like `</div>` are no longer separately reported
  - Makes HTML-related warnings clearer and less redundant

- **MD018: Skip CSS selectors and JS code inside HTML blocks**
  - CSS selectors like `#slide-1` inside `<style>` tags no longer trigger MD018
  - JavaScript code inside `<script>` tags is properly ignored
  - Prevents false positives for Quarto and RMarkdown files with embedded HTML/CSS/JS
  - Particularly important for interactive documents and custom styling

- **MD012: Enforce exactly 1 newline at EOF and clean LSP logging**
  - Files now must end with exactly one newline character (not zero, not two)
  - Aligns with POSIX standard and common editor behavior
  - Improved LSP logging with less noise

- **CI: Use cargo run instead of release binary in rumdl pre-commit hook**
  - Pre-commit hooks now use `cargo run` for more reliable execution
  - Prevents issues with stale release binaries during development
  - Better integration with cargo workflow

- **Schema: Allow root-level rule sections like [MD013] in config validation**
  - Configuration schema now correctly allows `[MD013]` style sections
  - Fixes false validation errors for valid TOML config
  - Better compatibility with common configuration patterns

### Changed

- **Refactor: Detect HTML blocks before parsing headings**
  - Reordered LintContext initialization to detect HTML blocks first
  - Headings are no longer detected inside HTML blocks (like `<style>` or `<script>`)
  - Architectural improvement that prevents multiple false positives
  - Benefits MD018 and other heading-related rules
  - More correct parsing aligned with markdown specifications

- **Refactor: Use filtered_lines() for front-matter handling in MD011 and MD012**
  - Improved front-matter handling using consistent filtered_lines API
  - Better code reuse and maintainability
  - More reliable front-matter detection across rules

## [0.0.167] - 2025-10-24

### Added

- **Configuration validation with fuzzy-match suggestions**
  - Comprehensive unknown key detection for `.rumdl.toml` and `pyproject.toml`
  - Intelligent "did you mean?" suggestions using Levenshtein distance algorithm
  - File path context in validation warnings for easy debugging
  - Catches typos in global options, rule names, and rule options
  - Example: `line-length` → suggests `line-length`, `reflw` → suggests `reflow`
  - Zero-dependency implementation with configurable similarity threshold
  - Helps users catch configuration mistakes before they cause confusion

- **MD053: Support for community comment-style references**
  - Recognizes and ignores reference-style link syntax used as comments
  - Supports widely-used patterns: `[//]: # (comment)`, `[comment]: #`, `[note]: #`, `[todo]: #`, `[fixme]: #`, `[hack]: #`
  - Any reference with just `#` as URL is treated as a comment
  - While not in CommonMark/GFM specs, used across 23+ markdown implementations
  - Complements HTML comments with a less HTML-like syntax option
  - Improves compatibility with existing markdown practices

- **MD013: `line-length = 0` to disable all line length checks**
  - Setting `line-length = 0` now completely disables MD013 rule
  - Provides explicit way to turn off line length validation entirely
  - More intuitive than previous workarounds
  - Useful when line length management is handled by other tools or not desired

- **MD051: mdbook template support**
  - Added detection and slug generation for mdbook templates
  - Recognizes `{{#template path/to/file.md}}` syntax
  - Properly generates GitHub-compatible slugs for template-included headings
  - Improves compatibility with mdbook documentation projects

- **LSP: Manual "Reflow paragraph" code action for MD013 warnings**
  - New code action available for MD013 line length warnings when auto-reflow is disabled
  - Allows users to manually reflow specific paragraphs without enabling global reflow in config
  - Appears as "Reflow paragraph" in Quick Fix menu (not marked as preferred, so won't trigger on save)
  - Intelligently detects paragraph boundaries and reflows entire paragraph, not just the flagged line
  - Respects line length limit from warning message or defaults to 80 characters
  - Provides a way to try paragraph reflow before committing to enabling it globally
  - Gives users fine-grained control over which paragraphs to reflow

### Fixed

- **LSP: Preserve trailing newline in reflow action**
  - Manual reflow code action now correctly preserves trailing newlines
  - Prevents unwanted file modifications from reflow operations
  - Maintains document structure integrity

- **LSP: Improve logging and resolve auto-fix issues**
  - Enhanced LSP server logging for better debugging
  - Resolved various auto-fix edge cases and reliability issues

- **MD051: Correct GitHub slug generation for angle brackets**
  - Fixed incorrect slug generation for headings containing angle brackets
  - Now properly handles special characters in anchor generation
  - Improves accuracy of link validation for complex headings

### Changed

- **MD033: Remove unhelpful message suffix**
  - Simplified warning messages for inline HTML detection
  - Removed redundant information to reduce noise
  - Cleaner, more focused error messages

- **Code cleanup: Remove dead code**
  - Removed unused `LinkImageStyle` enum from MD054
  - General refactoring to improve maintainability
  - Fixed clippy warnings

### Documentation

- **MD033: Document mdbook use case for semantic HTML**
  - Added documentation about using semantic HTML in mdbook projects
  - Clarifies when and why inline HTML might be intentionally used
  - Helps users understand legitimate use cases for HTML in markdown

## [0.0.166] - 2025-10-22

### Added

- **MD013: `paragraphs` field to control paragraph line length checks** (resolves #121)
  - New boolean config field `paragraphs` (defaults to `true`) allows disabling line length warnings for paragraph text
  - Enables sentence-per-line formatting workflows without line length validation noise
  - Still checks headings, tables, code blocks, blockquotes, and HTML when `paragraphs: false`
  - Useful for semantic line breaks where sentence length is determined by content, not arbitrary limits
  - Example configuration:

    ```yaml
    MD013:
      paragraphs: false  # Don't warn about long paragraphs
      code-blocks: true  # Still check code blocks
      tables: true       # Still check tables
      reflow: true
      reflow-mode: "sentence-per-line"
    ```

## [0.0.165] - 2025-10-21

### Fixed

- **MD040: Always preserve indentation when adding language tags** (fixes #122)
  - The MD040 rule was incorrectly removing indentation from code blocks when adding language tags
  - This broke list structure when code blocks were part of list items
  - Root cause: The fix logic had conditional behavior that would remove indentation for "standalone" code blocks
  - Now always preserves original indentation regardless of context
  - Removed 50+ lines of unnecessary `is_in_nested_context()` helper logic
  - Added comprehensive tests for various indentation scenarios (0, 2, 4, 6 spaces)

### Added

- **Conventional Commits validation hook**: Git commit-msg hook validates commit message format
  - Enforces Conventional Commits specification for all commits
  - Provides helpful error messages for invalid formats
  - Ensures consistent commit history for changelog generation

- **Automated changelog generation with git-cliff**:
  - Added `make changelog-draft` for previewing CHANGELOG updates
  - Semi-automated workflow: generate draft, enhance with details, commit
  - Conventional Commits integration for automatic categorization

### Changed

- **Pre-push hook optimization**: Use dev profile instead of full suite for faster testing
  - Prevents pre-push hook from hanging on slower machines
  - Maintains adequate test coverage while improving developer experience

## [0.0.164] - 2025-10-21

### Added

- **File-Level Caching (Ruff-inspired)**: Dramatic performance improvements for repeat runs
  - Blake3-based content hashing for fast cache lookups
  - Automatic cache invalidation on content, config, or version changes
  - Cache stored in `.rumdl-cache/{version}/{hash}.json`
  - CLI flags: `--no-cache` to disable, `--cache-dir` to customize location
  - Enabled by default for instant subsequent runs

- **Thread-Safe Parallel Caching**: Best of both worlds - parallelization AND caching
  - Implemented Arc<Mutex<LintCache>> for safe cache sharing across threads
  - Mutex locked ONLY for brief cache get/set operations
  - Full parallelization during expensive linting operations
  - Matches Ruff's architecture for optimal performance

- **Convergence Detection**: Added hash-based detection to identify when fixes have stabilized
  - Stops iteration when content hash remains unchanged
  - More efficient than counting rule applications
  - Returns convergence status in fix results

- **Convergence Failure Warnings**: Report when auto-fix doesn't converge (Ruff-style)
  - Warns if 100 iteration limit reached without convergence
  - Shows rule codes involved in potential infinite loop
  - Encourages bug reports for convergence failures
  - Available via `RUMDL_DEBUG_FIX_PERF` environment variable

### Changed

- **Auto-fix Iteration**: Automatic iteration until convergence (fixes #88)
  - `--fix` now automatically iterates up to 100 passes until content stabilizes (same as Ruff)
  - No need to manually re-run `rumdl check --fix` multiple times
  - Hash-based convergence detection prevents unnecessary iterations
  - Significantly improves user experience for multi-pass fix scenarios

- **Unified Linting Architecture**: Removed ~60 lines of duplicate linting logic
  - Refactored `process_file_collect_warnings` to use `process_file_inner`
  - Single code path for all file processing
  - Cache works for ALL output formats (text, JSON, GitLab, SARIF, JUnit)

- **Parallel File Processing for Fix Mode**: 4.8x speedup on multi-file fixes
  - Previously fix mode was always sequential
  - Now uses parallel processing when safe (multiple independent files)
  - Each file processes all its fix iterations independently

### Fixed

- **Multi-pass Fixes**: No longer require manual re-runs to apply all possible fixes
  - Previously users had to run `rumdl check --fix` multiple times
  - Now automatically handles dependent rule fixes in single command
  - Examples: MD010 (tabs) before MD007 (list indent), MD013 (line length) before MD009 (trailing spaces)

- **Cache Correctness**: Include enabled rules in cache key (Ruff-style)
  - Cache now respects `--enable`/`--disable` CLI flags
  - Different rule configurations create separate cache entries
  - Prevents incorrect cached results when switching rule sets
  - Changed `LintWarning.rule_name` from `Option<&'static str>` to `Option<String>` for proper serialization

- **Cache Parallelization**: Cache now works correctly with parallel processing
  - No mutex contention during parallel file processing
  - All output formats benefit from caching (previously only JSON/GitLab/SARIF/JUnit)

### Performance

- **Single file with cache**: 943ms → 7ms (135x faster)
- **Multi-file (21 files) cold cache**: 14.4s → 4s (parallel processing)
- **Multi-file (21 files) warm cache**: 14.4s → 0.019s (757x faster!)
- **JSON format (17 files) with cache**: 13.9s → 60ms (231x faster)

## [0.0.163] - 2025-10-20

### Changed

- **MD024**: Default `siblings_only` to true for better usability
  - Multiple headings with same text now only flagged if they're direct siblings
  - Reduces false positives in documents with common section headings
  - More intuitive default behavior matching common use cases

### Fixed

- **MD013**: Enforce line length in sentence_per_line mode (fixes #111)
  - Previously, sentence_per_line mode completely ignored line_length setting
  - Now warns about single sentences exceeding configured line_length
  - No auto-fix for long single sentences (requires manual rephrasing)
  - Still auto-fixes multi-sentence lines by splitting on sentence boundaries
  - Cleaned up warning messages by removing verbose parentheticals
  - Maintains semantic integrity (won't split mid-sentence) while respecting configured line_length

- **HTML Comments**: Complete fix to ignore all content inside HTML comments (fixes #119, #20)
  - All rules now properly ignore content within HTML comment blocks (`<!-- ... -->`)
  - Added `in_html_comment` field to `LineInfo` for comprehensive tracking
  - Extended filtered lines API with `skip_html_comments()` method
  - Updated MD013, MD049, and other rules to skip HTML comment content
  - Prevents false positives from commented-out markdown (MD013, MD049, MD005, MD006, MD039, MD042)
  - Better handling of multi-line HTML comments across all linting rules

- **MD046**: Resolve false positives from Issue #118
  - Fixed incorrect flagging of valid code block syntax
  - Improved code block style detection accuracy

- **MD050**: Resolve false positives from Issue #118
  - Fixed incorrect strong style detection in edge cases
  - Better handling of emphasis patterns

- **Tests**: Fixed sentence_per_line_detection test assertion
  - Updated test to match simplified warning message from MD013
  - Test was expecting verbose message after message was simplified in earlier commit

## [0.0.162] - 2025-10-16

### Added

- **Filtered Line Iterator Architecture**: New infrastructure for rule implementation
  - Provides consistent interface for filtering out front matter, code blocks, and HTML blocks
  - Eliminates manual context checking in individual rules
  - Improves code maintainability and reduces duplication
  - Enables easier implementation of new rules

### Fixed

- **MD052**: Skip code blocks in blockquotes when checking references
  - Prevents false positives for reference syntax inside code blocks within blockquotes
  - Properly handles nested markdown structures

- **MD034**: Skip URLs in front matter
  - URLs in YAML/TOML/JSON front matter no longer flagged as bare URLs
  - Improves compatibility with static site generators

- **Tests**: Fixed flaky `profiling::tests::test_concurrent_access` test
  - Added `#[serial_test::serial]` attribute to prevent race conditions
  - Ensures reliable test execution in CI/CD environments

- **Documentation**: Build badge now displays correctly

### Performance

- **MD005**: Optimized continuation detection from O(n²) to O(n)
  - Dramatically faster processing of documents with many list items
  - Eliminates redundant line scanning

- **General**: Consolidated multiple `line_info()` calls for same line
  - Reduced redundant lookups across multiple rules
  - Improved overall linting performance

### Changed

- **Internal Refactoring**: Eliminated manual checks across all rules
  - Removed manual front matter detection from individual rules
  - Removed manual code block detection from individual rules
  - Removed manual HTML block detection from individual rules
  - All rules now use centralized filtering infrastructure

### Documentation

- **Per-File-Ignores**: Added comprehensive documentation for per-file-ignores feature
  - Detailed usage examples with glob patterns
  - Integration with both `.rumdl.toml` and `pyproject.toml`

## [0.0.161] - 2025-10-15

### Added

- **MD013**: Support for backslash hard line breaks for mdformat compatibility (closes #110)
  - Backslash (`\`) at end of line now recognized as hard break alongside two-space breaks
  - Original hard break format (backslash or spaces) preserved during reflow operations
  - Segment-based reflow correctly handles both hard break types
  - Comprehensive test coverage including mdformat compatibility tests
  - Enables seamless migration from mdformat to rumdl

### Fixed

- **MD029**: Recognize properly indented nested content as list continuation
  - Nested list items and paragraphs within list items now correctly identified
  - Improved detection of list item boundaries
  - Better handling of complex list structures

- **MD013**: Preserve semantic line breaks and fix false positives in normalize mode
  - Semantic line breaks (intentional breaks for readability) now preserved
  - Reduced false positives when lines are intentionally kept short
  - Better detection of paragraph boundaries in normalize mode

- **MD044**: Invert code-blocks logic to match MD013 and change default to false
  - Parameter logic now consistent: `true` = check code blocks, `false` = skip code blocks
  - Default changed to `false` (skip code blocks) for better user experience
  - Aligns with MD013's code block handling for consistency across rules

## [0.0.160] - 2025-10-15

### Fixed

- **Configuration**: Fixed `rumdl init --pyproject` command to no longer create `.rumdl.toml` file
  - The command now correctly only adds rumdl configuration to `pyproject.toml`
  - Prevents confusion from having duplicate configuration files

- **MD044**: Corrected field name in templates and documentation
  - Fixed inconsistency in proper names configuration
  - Improved accuracy of documentation examples

- **Configuration System**: Added field aliases and validation warnings for all rules
  - Better backwards compatibility with alternative field names
  - Helpful warnings guide users to correct configuration syntax
  - Improved user experience when migrating configurations

## [0.0.159] - 2025-10-14

### Added

- **JSON Schema Generation**: New `rumdl schema` subcommand for generating JSON schema from configuration
  - `rumdl schema generate` - Generate/update the schema file
  - `rumdl schema check` - Verify schema is up-to-date (used in CI)
  - `rumdl schema print` - Print schema to stdout
  - Schema automatically generated from Rust types using `schemars`
  - Prepared for SchemaStore submission to enable IDE autocomplete/validation

### Fixed

- **MD051 False Positives**: Fixed incorrect handling of backtick headings with angle brackets
  - Previously treated `<FILE>` inside backticks as HTML tags and stripped them
  - Now correctly processes headings like `` `import <FILE> [OPTIONS]` `` → `import-file-options`
  - Removed premature `strip_html_tags()` call; anchor algorithms now handle both markdown and HTML correctly
  - Added regression tests for backtick headings with special characters
  - Fixes false positives in README.md table of contents

### Changed

- **Code Cleanup**: Removed unused `generate_schema` binary (functionality moved to `rumdl schema` subcommand)

## [0.0.158] - 2025-10-14

### Fixed

- **CRLF Line Ending Support**: Fixed byte position calculations in multiple rules for Windows-style line endings
  - Fixed MD034, MD046, MD057 byte position calculations
  - Fixed MD037, MD049, MD011 byte position calculations
  - Fixed MD050, MD037, MD010, MD026 byte position calculations
  - Fixed code_block_utils byte position calculation in `is_in_code_span`
  - All rules now correctly handle CRLF line endings in fixes and diagnostics

- **Test Stability**: Fixed flaky tests with dependency injection pattern
  - Eliminated race conditions from parallel test execution
  - Tests no longer modify global environment variables
  - Added `serial_test` crate for unavoidable global operations
  - All 1731 tests now pass reliably in parallel execution

### Changed

- **Code Architecture**: Major refactoring to improve maintainability
  - Extracted `formatter` module (397 lines) - output formatting logic
  - Extracted `watch` module (491 lines) - watch mode functionality
  - Extracted `file_processor` module (792 lines) - file processing logic
  - Extracted `stdin_processor` module (212 lines) - stdin handling
  - main.rs reduced from 3268 to 1394 lines (57% reduction)
  - Improved code organization and testability

- **Line Ending Handling**: Refactored line ending preservation
  - Line ending detection and normalization now at I/O boundaries
  - Internal code always works with consistent LF line endings
  - More efficient: 1 normalization per file instead of per-rule
  - Cleaner separation of concerns
  - Simplified MD047, MD012, MD022 to always use LF internally
  - Removed unnecessary line ending detection from rules
  - Added comprehensive end-to-end CRLF tests

### Removed

- **Legacy Fix Implementation**: Removed deprecated fix wrapper functions
  - Removed `apply_fixes()` wrapper
  - Removed `apply_fixes_stdin_coordinated()` wrapper
  - Removed `apply_fixes_stdin()` legacy implementation
  - Removed `RUMDL_NO_FIX_COORDINATOR` environment variable
  - Fix Coordinator is now the only fix strategy (3 weeks stable, ~75% faster)

## [0.0.157] - 2025-10-13

### Changed

- **Removed legacy fix implementation** - Removed old single-pass fix implementation and `RUMDL_NO_FIX_COORDINATOR` environment variable. Fix Coordinator is now the only fix strategy, providing ~75%
  faster fixes with better coverage.

### Added

- **MD042**: Full support for MkDocs paragraph anchors (#100)
  - Recognize Python-Markdown `attr_list` extension syntax: `[](){ #anchor }`
  - Support for both anchor IDs (`#id`) and CSS classes (`.class`)
  - Support optional colon syntax: `[](){: #anchor }`
  - UTF-8 boundary validation and DoS prevention (500 char limit)
  - 28 comprehensive tests covering edge cases
  - Complete documentation with links to official Python-Markdown specs
  - References: [attr_list](https://python-markdown.github.io/extensions/attr_list/), [mkdocs-autorefs](https://mkdocstrings.github.io/autorefs/)

- **MD042**: Smart URL detection in empty links (#104)
  - When link text looks like a URL (e.g., `[https://example.com]()`), use it as the destination
  - Supports <http://>, <https://>, <ftp://>, ftps:// protocols
  - More intelligent fixes than placeholder URLs

- **Always respect exclude patterns by default** (#99)
  - Exclude patterns now always respected, even for explicitly provided files
  - Matches behavior of ESLint, Pylint, Mypy
  - Added `--no-exclude` flag to disable all exclusions when needed
  - LSP support for exclude patterns
  - Shows warnings with actionable hints when excluding files

- **Hidden directory scanning** (#102)
  - Now scans hidden directories (like `.documentation`) by default
  - More thorough markdown file discovery

### Fixed

- **MD033**: Code blocks in blockquotes false positives (#105)
  - Fixed incorrect flagging of HTML tags inside fenced code blocks within blockquotes
  - Properly strips blockquote markers before detecting fence markers
  - 25 new tests covering nested blockquotes and edge cases

- **MD034**: Empty link construct false positives (#104)
  - Fixed incorrect flagging of URLs in `[url]()` and `[url][]` patterns
  - Prevents text corruption during formatting
  - Added patterns to properly exclude empty link constructs

- **MD042**: Improved fix quality
  - Removed "useless" placeholder fixes that just create new problems
  - Only provides fixes when we have enough information for valid links
  - No longer auto-fixes `[]()` or `[text]()` with placeholders

### Changed

- **BREAKING**: Exclude patterns now always respected by default
  - Previously: `--force-exclude` flag needed to respect excludes for explicit files
  - Now: Excludes always respected by default
  - Migration: Use `--no-exclude` flag if you need the old behavior

## [0.0.156] - 2025-10-08

### Fixed

- **Build**: Removed feature-gated benchmark binaries that were causing unnecessary reinstalls
  - Benchmark binaries now only built when explicitly requested
  - Reduces package size and installation time

## [0.0.155] - 2025-10-08

### Fixed

- **PyPI Package**: Fixed package structure by removing unused cdylib and dependencies
  - Removed unnecessary C dynamic library configuration
  - Cleaner Python package distribution

## [0.0.154] - 2025-10-08

### Fixed

- **MD013**: Implemented segment-based reflow to preserve hard breaks
  - Properly handles double-space line breaks
  - Integration tests updated for new behavior

### Performance

- **MD034**: Reuse buffers to reduce per-line allocations
- **MD005**: Eliminate LineIndex creation overhead
- **MD030**: Eliminate O(n²) complexity by caching line collection

### Documentation

- Organized LintContext optimization documentation

## [0.0.153] - 2025-10-07

### Performance

- **Major optimization**: 54 rules now use LintContext character frequency caching
  - Significant performance improvement across the board
  - Reduced redundant scanning of document content

- **MD051**: Optimized link fragment validation
  - Faster processing of heading anchors and fragments

### Fixed

- **MD013**: Improved nested list handling in reflow mode
  - Better preservation of list structure during reformatting

## [0.0.152] - 2025-10-06

### Fixed

- **MD013**: Multi-paragraph list reflow improvements and refactoring
  - Better handling of complex list structures
  - More reliable paragraph detection within lists

## [0.0.151] - 2025-10-05

### Fixed

- **MD007**: Fixed tab indentation and cascade behavior
  - Properly handles tabs in list indentation
  - Correct cascade behavior matching markdownlint

## [0.0.150] - 2025-10-04

### Fixed

- **MD007**: Multiple fixes for list indentation
  - Correct blockquote list handling
  - Fixed text-aligned indentation to match markdownlint cascade behavior
  - Updated test expectations for cascade behavior

## [0.0.149] - 2025-10-03

### Added

- **Configuration**: JSON Schema for rumdl.toml configuration (#89)
  - IDE autocomplete and validation support
  - Better configuration documentation

- **Configuration**: Per-file rule ignores (#92)
  - Glob pattern support for ignoring rules on specific files
  - Example: `[per-file-ignores] "docs/*.md" = ["MD013"]`

## [0.0.148] - 2025-10-02

### Fixed

- **MD042**: Display improvements
  - Show exact source text in error messages
  - Correct display of shorthand reference links

- **MkDocs**: Strip backticks from MkDocs auto-references (#97)
  - Prevents false positives on `` [`module.Class`][] `` patterns

## [0.0.147] - 2025-10-01

### Added

- **MkDocs**: Added mkdocstrings support (#94)
  - Recognizes mkdocstrings YAML options
  - Multiple rules migrated to use LintContext for better MkDocs handling

### Fixed

- **MD041**: Removed auto-fix capability (#93)
  - Auto-fixing front-heading violations was unreliable
  - Now only reports issues without attempting fixes

- **MD026**: Corrected documentation to match implementation (#95)
  - Documentation now accurately reflects punctuation handling

- **Jinja2**: Added Jinja2 template support (#96)
  - Prevents false positives in template syntax
  - Better support for MkDocs projects using Jinja2

- **MD013**: Prevent false positives for already-reflowed content
  - Smarter detection of intentional line breaks

- **MD034**: Properly excludes URLs/emails in code spans and HTML
  - No more false positives on inline code URLs

- **MD054**: Fixed column indexing bug
  - Correct error position reporting

- **MD033 & MD032**: Resolved false positives (#90, #91)
  - More accurate HTML tag detection
  - Better handling of code blocks

## [0.0.146] - 2025-09-24

### Added

- **Fix Coordinator**: New intelligent fix system as default behavior (#88)
  - ~75% faster execution on large files (15.6s vs 60.7s for OpenAPI spec)
  - ~90% of issues fixed in single pass (vs 2-3 passes previously required)
  - Topological sort ensures optimal rule ordering based on dependencies
  - Handles cyclic dependencies gracefully
  - Opt-out available via RUMDL_NO_FIX_COORDINATOR=1
  - Debug output available via RUMDL_DEBUG_FIX_PERF=1

### Changed

- Fix mode now uses Fix Coordinator by default for dramatic performance gains
- Fix strategy prioritizes intelligent ordering over bulk fixes

### Performance

- First pass: 87% faster than v0.0.141, 74% faster than v0.0.143
- Completes 3 full passes (35.5s) faster than v0.0.141 does single pass (115.6s)
- Reduces LintContext creations through intelligent batching

## [0.0.145] - 2025-09-23

### Fixed

- **MD032**: Refined to handle nested code blocks correctly
- Various CI test failures and compatibility improvements

## [0.0.144] - 2025-09-22

## [0.0.142] - 2025-09-20

### Fixed

- **MD013**: Refactored to emit warning-based fixes for LSP compatibility (#79)
  - MD013 reflow now works correctly when using LSP formatting in editors like Helix
  - Generates proper warning-based fixes with byte ranges instead of document transforms
  - Preserves trailing newlines and handles multi-line list items correctly

## [0.0.141] - 2025-09-15

### Added

- **MD013**: New normalize mode for combining short lines in paragraphs (related to #76)
  - Added `reflow_mode` configuration with "default" and "normalize" options
  - Normalize mode combines short lines to use the full configured line length
  - Enables bulk removal of manual line breaks by setting high line_length with normalize mode
  - Preserves markdown structure (lists, code blocks, tables, hard breaks)

### Fixed

- **MD013**: Fixed multi-line list item handling to avoid extra spaces when combining
- **MD012**: Fixed line number reporting for EOF blank lines
- **LSP**: Return null instead of empty array when no formatting available (related to #79)
- **MD038**: Resolved false positives and added regression tests

## [0.0.140] - 2025-09-11

### Fixed

- **LSP**: Support formatting documents without textDocument/didOpen (related to #79)
  - Added lazy loading with disk fallback for unopened documents
  - Editors like Helix can now format files without opening them first
  - Implemented DocumentEntry structure to track document source and version
  - Added intelligent caching for disk-loaded documents
  - Maintains full compatibility with traditional LSP clients (VS Code)

- **MD051**: Fixed false positives in large documents with multiline inline code spans
  - Multiline inline code spans were incorrectly treated as code blocks
  - This caused headings after line ~600 to not be detected properly
  - Removed incorrect TOC detection logic that was causing issues

- **MD032**: Fixed false positive for sequential ordered list continuations
  - Sequential ordered list items (1., 2., 3.) no longer incorrectly flagged
  - Added proper detection for list continuations vs separate lists
  - Improved handling of lists interrupted by code blocks

- **MD052**: Fixed false positive for literal brackets in backticks
  - Text like `[from ...]` in inline code no longer flagged as broken reference
  - Added workaround for multiline code span detection issues
  - Properly distinguishes between literal text and reference links

- **Documentation**: Added comprehensive inline configuration documentation
  - Created detailed guide for rumdl-disable/enable comment syntax
  - Documented all supported inline configuration formats
  - Added examples for disabling rules per line, block, and file

- **Fix Counting**: Corrected issue where unfixable warnings were counted as fixed
  - MD013 warnings in table cells now correctly reported as unfixable
  - Fix count now reflects actual fixes applied, not total warnings

## [0.0.139] - 2025-09-09

### Fixed

- **UTF-8 Handling**: Fixed panic when processing files with multi-byte UTF-8 characters (fixes #85)
  - Added proper character boundary checking in code block detection
  - Prevents string slicing panics with German umlauts (ä, ö, ü) and other multi-byte characters
  - Added comprehensive test coverage for various international scripts

- **MD038**: Fixed false positives caused by escaped backticks (fixes #77)
  - Escaped backticks (`\``) no longer create phantom code spans
  - Implemented two-pass algorithm to properly handle escaped characters
  - Reduced false positive count from 179 to 0 in affected documents

- **MD032**: Improved markdownlint compatibility and edge case detection (fixes #77)
  - Changed default configuration to match markdownlint (no blank lines around lists in blockquotes)
  - Added detection for ordered lists starting with numbers other than 1
  - Better compliance with CommonMark specification for list formatting

- **Code Block Detection**: Fixed critical bug where lines after code blocks were marked as inside them
  - Corrected fenced code block end position to include the newline after closing fence
  - Prevented code span detection from running inside fenced code blocks
  - Fixed MD051 false positives where headings after code blocks weren't detected
  - Eliminated invalid overlapping code spans that caused parsing errors

## [0.0.138] - 2025-09-05

### Fixed

- **LSP**: Fixed formatting to return empty array instead of null when no edits available (fixes #79)
  - Helix editor now properly receives LSP formatting responses
  - Added textDocument/rangeFormatting support for better editor compatibility
  - Fixed critical position calculation bug that could cause incorrect text edit ranges

## [0.0.137] - 2025-09-04

### Fixed

- **MD051**: Fixed GitHub anchor generation for headers with arrow patterns (fixes #82)
  - Headers like `WAL->L0 Compaction` now correctly generate `#wal-l0-compaction` anchors
  - Arrow patterns (`->`, `-->`) now convert to the correct number of hyphens based on surrounding spaces

## [0.0.136] - 2025-09-03

## [0.0.135] - 2025-09-03

## [0.0.134] - 2025-09-02

### Added

- **MD051**: HTML anchor tag support for any element with id/name attributes
  - Supports `<a>`, `<span>`, `<div>` and any other HTML element with id attribute
  - Case-sensitive matching for HTML anchors (case-insensitive for Markdown)
  - Handles multiple id attributes (only first is used per HTML spec)

### Fixed

- **MD007**: Implemented proper indentation style configuration for markdownlint compatibility
  - Added IndentStyle enum with TextAligned (default) and Fixed (markdownlint) modes
  - Auto-configures style="fixed" when loading from .markdownlint.yaml files
  - Resolves 5-space indentation detection issues (#77)
- **MD029**: Improved list numbering style compatibility
  - Added OneOrOrdered style (markdownlint default) accepting either all-ones or sequential
  - Changed default from Ordered to OneOrOrdered for better compatibility
- **MD050**: Fixed false positives for emphasis patterns inside HTML `<code>` tags
  - Patterns like `__pycache__`, `__init__` no longer flagged inside code elements
- **MD052**: Fixed reference checking to skip HTML content lines
  - Skip any line starting with '<' to match markdownlint behavior
  - Fixed regex to properly handle nested brackets in references like `[`Union[T, None]`]`
- **MD053**: Improved duplicate reference detection
  - Detect when same reference is defined multiple times
  - Handle case-insensitive duplicates per CommonMark spec
  - Remove overly aggressive filters that skip valid references
- **MD028**: Aligned with markdownlint behavior for blank lines in blockquotes
  - Flag all blank lines between blockquotes as ambiguous
  - Better distinguish blockquote separators from internal blank lines
- **MD005**: Fixed respect for MD007 configuration and nested list handling
- **MD006**: Skip validation for lists inside blockquotes where indentation is expected

## [0.0.133] - 2025-08-30

### Fixed

- **MD028/MD009**: Complete fix for rule conflict where MD028 and MD009 were "fighting each other" (fixes #66)
  - MD028 now only flags truly blank lines inside blockquotes, not `>` or `>` lines
  - MD009 simplified to remove special cases for empty blockquote lines
  - Both rules now correctly follow CommonMark specifications

## [0.0.132] - 2025-08-30

### Added

- **LSP**: Added "Fix all rumdl issues" code action for bulk fixes when multiple fixable diagnostics are present

## [0.0.131] - 2025-08-28

### Fixed

- **MD002**: Implemented markdownlint compatibility - MD002 no longer triggers when first heading is on the first line, regardless of level (fixes #65)
- **MD034**: Added support for multi-line MkDocs snippet blocks where markers appear on separate lines (fixes #70)

## [0.0.130] - 2025-08-27

### Fixed

- **MD052**: Fixed false positives with IPv6 URLs containing brackets (e.g., `http://[::1]:8080/path[0]`)
- **MD053**: Made rule warning-only, removed automatic fixes to prevent accidental removal of intentionally kept references (fixes #69)
- **MD009/MD028**: Resolved formatting loop between trailing spaces and blank blockquote lines
- **MD002/MD041**: Fixed interaction where MD002 incorrectly flagged documents starting with level-1 heading
- **MD011**: Prevented false positives with math-like expressions (e.g., `[0,1]`) outside code blocks
- **MD034**: Improved bare URL detection to avoid false positives with bracketed paths in URLs

## [0.0.129] - 2025-08-26

### Added

- **MkDocs Extended Support**: Enhanced MkDocs compatibility with PyMdown Extensions
  - Snippets syntax (`--8<--`) support (fixes #62)
  - Admonitions (`!!!`, `???`, `???+`) for collapsible note blocks
  - Tabs (`=== "Tab Name"`) for content organization
  - Footnotes (`[^ref]`) for reference-style citations
  - Cross-references and auto-doc blocks

### Fixed

- **MkDocs Validation**: Made validation more lenient to detect malformed syntax
- **Configuration Migration**: Fixed migration of multiple disabled rules from markdownlint config

## [0.0.128] - 2025-08-25

### Fixed

- **MD042/MD052**: Added support for simple identifiers in MkDocs auto-references

## [0.0.127] - 2025-08-25

### Added

- **MkDocs Support**: Added MkDocs markdown flavor (closes #63)
  - New `flavor = "mkdocs"` configuration option
  - MkDocs auto-references (e.g., `[class.Name][]`, `[module.function][]`) are no longer flagged as errors
  - MD042 and MD052 rules now recognize MkDocs-specific patterns
  - Type-safe enum implementation for better extensibility

### Fixed

- **MD005**: Dynamic indent detection to respect user's chosen pattern (fixes #64)
  - Analyzes existing document to detect 2-space vs 4-space indentation
  - Preserves user's indentation style instead of forcing a default

### Changed

- **Configuration**: Renamed MD013 'enable_reflow' to 'reflow' with backwards compatibility

## [0.0.126] - 2025-08-23

### Fixed

- **Build**: Fixed output filename collision warning during `cargo install` (#61)

## [0.0.125] - 2025-08-22

### Added

- **CLI**: Added `--stdin-filename` flag for better stdin processing
  - Specify filename when reading from stdin for better error messages
  - Enables MD057 (relative link checking) to work correctly with stdin
  - Provides proper filename context in all output formats
  - Improves editor integration capabilities

### Fixed

- **CLI**: Fixed `rumdl fmt -` to output original content when no issues found
  - Previously incorrectly output "No issues found in stdin" message
  - Now correctly outputs the original content unchanged
- **MD029**: Corrected list continuity detection and fix functionality
  - Improved handling of sublists and indented content
  - Better markdownlint compatibility

### Changed

- **Build**: Added mise version validation to pre-release checks
  - Prevents CI failures from non-existent mise versions

## [0.0.124] - 2025-08-22

### Added

- **Formatting**: Added stdin/stdout formatting support (closes #59)
  - `rumdl fmt` command for formatting markdown files (alias for `check --fix`)
  - `--stdin` with `--fix` now outputs formatted content to stdout
  - Support for `-` as stdin indicator (Unix convention: `rumdl fmt -`)
  - Clear separation between linting (diagnostics to stderr) and formatting (content to stdout)
  - Documentation updated with formatting examples

### Fixed

- **MD052**: Don't flag GitHub alerts as undefined references (closes #60)
  - GitHub alert syntax (`[!NOTE]`, `[!TIP]`, `[!WARNING]`, `[!IMPORTANT]`, `[!CAUTION]`) no longer flagged
  - Improved compatibility with GitHub-flavored markdown
- **MD009**: Fixed heading trailing space removal
  - Headings now have ALL trailing spaces removed (they serve no purpose in headings)
- **CLI**: Fixed stdin diagnostics output in check mode
  - Diagnostics now correctly output to stderr by default in check mode without `--fix`

## [0.0.123] - 2025-08-21

### Added

- **MD013**: Comprehensive markdown pattern preservation during text reflow
  - Preserves reference links, footnotes, math formulas, wiki links, and more
  - Centralized regex patterns for better maintainability

### Fixed

- **CLI**: Correct unfixable rules status display and fix counts (closes #56)
  - Rules marked as unfixable now show `[unfixable]` in yellow instead of `[fixed]`
  - Fix count now correctly excludes unfixable rules (e.g., "3 of 6" instead of "6 of 6")
  - Added FixCapability enum to Rule trait for compile-time safety
- **MD013**: Preserve reference links during text reflow
  - Reference-style links are now properly preserved when reflowing text
  - Fixed indicator display to correctly show `[fixed]` when issues are resolved
- **Tests**: Mark kramdown definition list doctest as text to fix test failures

### Changed

- **Internal**: Centralized markdown pattern regexes and extended reflow support
  - Improved code organization and reduced duplication
  - Better performance through shared regex compilation

## [0.0.122] - 2025-08-19

### Added

- **Configuration Discovery**: Automatic upward directory traversal to find configuration files (closes #58)
  - Searches parent directories for `.rumdl.toml`, `rumdl.toml`, or `pyproject.toml`
  - Similar behavior to `git`, `ruff`, and `eslint`
  - Stops at `.git` directory boundaries
- **--isolated flag**: New flag to disable all configuration discovery (Ruff-compatible)
  - Alias for `--no-config` for better ecosystem compatibility

## [0.0.121] - 2025-08-19

### Fixed

- **MD051**: Resolved remaining Issue #39 edge cases for link fragment validation
  - Fixed ampersand handling at boundaries: "& text" → "--text", "text &" → "text-"
  - Fixed cross-file link detection to properly ignore absolute paths (e.g., `/tags#anchor`)
  - Improved Liquid template handling to skip links with filters (e.g., `{{ url | relative_url }}`)
  - Fixed test expectations to match actual GitHub behavior for multiple spaces and trailing punctuation
  - Verified Jekyll/kramdown GFM underscore handling works correctly for technical identifiers

### Improved

- **MD051**: Enhanced anchor generation accuracy and security
  - Added comprehensive security hardening (Unicode normalization, RTL/LTR override prevention)
  - Improved emoji detection and boundary handling
  - Better performance with optimized regex patterns and early exit checks
  - Added regression tests for all Issue #39 scenarios

## [0.0.120] - 2025-08-16

### Performance

- Incremental improvements to various rule implementations

## [0.0.119] - 2025-08-15

### Fixed

- **MD051**: Fixed GitHub anchor generation algorithm to correctly handle consecutive spaces
  - "Test & Heading!" now correctly generates "test--heading" instead of "test-heading"
  - Improved compliance with GitHub's official anchor generation behavior
  - Fixed whitespace normalization bug that was collapsing multiple spaces to single spaces

### Improved

- **Code Quality**: Removed all `#[allow(dead_code)]` violations in codebase
  - Removed unused `InternalCodeBlockState` enum from document_structure.rs
  - Removed unused `extract_url_from_link` function from md057_existing_relative_links.rs
  - Removed unused `is_in_code_block` function from md007_ul_indent.rs

## [0.0.118] - 2025-08-14

### Performance

- Incremental improvements to various rule implementations

## [0.0.117] - 2025-08-14

### Fixed

- MD037: Fixed false positive with asterisks in inline code spans (issue #49)
  - Inline code content is now properly masked before emphasis detection
- MD011: Fixed false positive with array access patterns in link titles (issue #50)
  - Context detection now properly skips patterns inside code spans
- MD052: Fixed false positive with square brackets in HTML attributes (issue #51)
  - HTML tag detection prevents reference checking within HTML elements
- Added centralized skip context detection for improved accuracy across rules

## [0.0.116] - 2025-08-13

### Added

- Kramdown-style custom header IDs support (#44)
  - Headers can now have custom IDs using the `{#custom-id}` syntax
  - Custom IDs are preserved when fixing MD051 (link fragments)
  - MD026 (trailing punctuation) now ignores headers with custom IDs
  - Safe character validation: accepts Unicode letters/numbers, hyphens, underscores, and colons
  - Rejects problematic characters like spaces, quotes, brackets, and HTML/CSS special chars

### Fixed

- Pre-release script now correctly handles dynamic versioning in pyproject.toml
- Added Cargo.lock validation and `cargo publish --dry-run` checks to prevent release failures

## [0.0.115] - 2025-08-12

### Fixed

- Various bug fixes and improvements

## [0.0.114] - 2025-08-09

### Fixed

- Various bug fixes and improvements

## [0.0.113] - 2025-08-09

### Fixed

- Various bug fixes and improvements

## [0.0.112] - 2025-08-08

### Fixed

- Various bug fixes and improvements

## [0.0.110] - 2025-08-08

### Changed

- Various bug fixes and improvements

## [0.0.107] - 2025-08-06

### Fixed

- MD036: Remove automatic fix to prevent document corruption when bold/italic text is used as image captions, labels, or warnings (#23)
- MD011: No longer flags patterns like `()[1]` inside inline code as reversed links (#19)
- MD052: No longer flags reference patterns inside HTML comments as undefined references (#20)

## [0.0.106] - 2025-08-05

### Changed

- Moved benchmark binaries from Python package distribution
  - Benchmark tools are now in `benchmark/bin/` directory
  - Added `build-benchmarks` feature flag to explicitly build benchmarks
  - Python package now only includes the main `rumdl` binary
  - Significantly reduced installed package size

## [0.0.105] - 2025-08-05

### Fixed

- MD029: Fixed list continuation detection to properly handle variable marker widths (fixes #16)
  - List items with double-digit markers (e.g., "10. ") now correctly require 4+ spaces for continuation
  - List items with triple-digit markers (e.g., "100. ") now correctly require 5+ spaces for continuation
  - List items with any number of digits now correctly calculate required continuation indentation
- MD027: Improved tab handling and added bounds checking for range calculations
- Installation: Improved update experience for Cursor/Windsurf editors

### Added

- `--update` flag to check for newer versions and update if available
- Version checking with update notifications
- Marketplace-aware installation for VS Code forks
- Comprehensive tests for MD029 with large number markers (triple and quadruple digits)

### Changed

- Clarified `--force` flag behavior in help text

## [0.0.104] - 2025-08-02

### Added

- File-wide inline configuration support with `disable-file`, `enable-file`, and `configure-file` comments
- Support for JSON configuration within inline comments to customize rule behavior per file
- Enhanced inline configuration to handle edge cases with multiple comments on the same line
- Support for enabling specific rules when all rules are disabled

### Fixed

- Process inline configuration comments in order of appearance on the same line
- Skip processing inline configuration comments inside code blocks

## [0.0.102] - 2025-07-24

## [0.0.101] - 2025-07-23

## [0.0.100] - 2025-07-22

### Performance Improvements

- **MD032**: Eliminated redundant DocumentStructure creation through optimization interface delegation
  - Refactored check() method to delegate to check_with_structure() for shared parsing
  - Added fix_with_structure() helper method for optimized fixing operations
- **List Processing**: Major refactoring of complex list block merging logic for better maintainability
  - Extracted merge_adjacent_list_blocks into clean ListBlockMerger struct
  - Introduced BlockSpacing enum for clear categorization of list spacing types
  - Separated compatibility checking, spacing analysis, and merging logic into focused methods
- **Memory Management**: Added comprehensive performance stress tests for deeply nested lists
  - Created benchmarks for up to 20 levels of nesting with measurable performance baselines
  - Established performance thresholds: <3ms parsing, <4ms rule checking for extreme nesting
  - Added memory stress testing to prevent performance regressions

### Code Quality

- Improved separation of concerns in list processing logic
- Enhanced code maintainability through better structured algorithms
- Added comprehensive test coverage for pathological markdown structures

## [0.0.99] - 2025-07-22

### Fixed

- MD034: Added support for `ftps://` URLs
- MD034: Fixed detection of URLs in HTML comments (now properly ignored)
- MD039: Fixed escaped character handling in link text
- MD044: Fixed clippy warnings and improved pattern matching for proper names
- MD052: Enhanced nested bracket handling in reference links and images
- Fixed flaky performance tests by increasing timeout threshold for CI environments
- Improved test stability for Unicode list indentation tests

## [0.0.98] - 2025-07-18

### Added

- Homebrew tap support for easy installation on macOS and Linux
  - Created `homebrew-rumdl` tap repository
  - Added automatic archive creation for macOS builds in release workflow
  - Included SHA256 checksum generation for each platform
  - Set up automated formula updates on new releases
- Homebrew installation instructions in README

### Changed

- Enhanced release workflow to create platform-specific tar.gz archives
- Added repository dispatch to notify homebrew-rumdl on new releases

## [0.0.97] - 2025-07-17

### Changed

- Updated exit code handling for consistency:
  - Configuration errors now return exit code 2 (was 1)
  - File not found errors now return exit code 2 (was 1)
  - Invalid command arguments now return exit code 2 (was 1)
- Standardized exit codes across all error conditions:
  - Exit code 1: Reserved for linting issues found
  - Exit code 2: Tool errors (config parse errors, file not found, invalid arguments)

### Fixed

- Improved consistency in exit code handling across the entire codebase
- Updated all tests to expect correct exit codes for different error scenarios

## [0.0.96] - 2025-07-16

### Added

- MD013: Text reflow/wrapping functionality for automatic line breaking (fixes #13)
  - New `enable_reflow` configuration option (disabled by default)
  - Intelligently wraps long lines while preserving Markdown formatting
  - Preserves bold, italic, links, code spans, and other Markdown elements
  - Proper list continuation indentation that aligns with the text after markers
  - Preserves hard line breaks (two trailing spaces)
  - Does not wrap code blocks, tables, headings, or reference definitions
- Added `pulldown-cmark` dependency (v0.12.2) for improved Markdown parsing
- Comprehensive test coverage for text reflow functionality

### Changed

- MD013: Enhanced fix functionality with optional text reflow (opt-in feature)

### Fixed

- MD013: Fixed list indentation to properly align continuation lines with the text content

## [0.0.95] - 2025-07-15

### Added

- Implemented 3-tier exit code system following Ruff's convention:
  - Exit code 0: Success (no issues found)
  - Exit code 1: Linting violations found
  - Exit code 2: Tool error (config error, file access error, etc.)
- Added exit_codes module for cleaner exit code management

### Changed

- Updated all error handlers to use appropriate exit codes
- CI/CD systems can now distinguish between markdown issues (exit 1) and tool failures (exit 2)

### Documentation

- Updated README with exit code documentation
- Added exit codes section to CLI reference

## [0.0.94] - 2025-07-04

### Performance Improvements

- Implemented lazy code span loading - 3.8x speedup for 94% of rules that don't use code spans
- MD013: 34.5% faster check operations through aggressive early returns
- MD038: 14% faster by leveraging lazy code span loading
- MD044: 93.5% faster with global regex caching
- MD047: 8.3% faster using pre-computed line data
- MD053: 39.7% faster by leveraging pre-parsed reference definitions
- Overall LintContext creation improved by 11.7%

### Fixed

- MD053: Fixed escaped character handling in reference definitions

## [0.0.93] - 2025-07-03

## [0.0.92] - 2025-07-02

### Fixed

- MD036: Align with markdownlint behavior - emphasis ending with punctuation (e.g., `**Note:**`) is no longer flagged

## [0.0.91] - 2025-07-02

## [0.0.90] - 2025-07-01

## [0.0.89] - 2025-07-01

### Added

- Comprehensive unit test coverage for all 54 linting rules (~742 new tests)
- Unit tests for LSP server functionality
- Unit tests for all 11 output formatters
- Unit tests for Python bindings
- Test coverage improved from 75.4% to 77.1%

### Fixed

- MD005: Fixed blockquote handling to correctly ignore intentional separations
- MD054: Fixed overlapping match detection for link/image styles
- Strong style utility module refactored to remove unused code
- MD029: Fixed nested code block detection by implementing proper CommonMark fence closing rules
- MD052: Fixed false positives for arrays and references inside inline code spans
- MD013: Fixed line length calculation to intelligently handle URLs in non-strict mode

### Improved

- Test infrastructure now includes both unit and integration tests
- Better test organization with inline unit tests in implementation files

## [0.0.88] - 2025-06-28

### Added

- 11 new output formatters for enhanced compatibility:
  - `grouped` - Groups violations by file
  - `pylint` - Pylint-compatible format
  - `azure` - Azure Pipeline logging format
  - `concise` - Minimal file:line:col format
  - `github` - GitHub Actions annotation format
  - `gitlab` - GitLab Code Quality report format
  - `json` - Machine-readable JSON format
  - `json_lines` - JSONL format (one JSON object per line)
  - `junit` - JUnit XML format for CI integration
  - `sarif` - SARIF format for security tools
  - `text` - Default human-readable format with colors

### Changed

- **BREAKING**: Upgraded to Rust 2024 edition (requires Rust 1.87.0+)
- Improved code quality by fixing all 283 clippy warnings

### Fixed

- Config `output_format` field now properly merges from configuration files
- Pylint formatter now outputs correct `CMD` codes instead of generic `C` codes

### Optimized

- Removed unused dependencies (`glob`, `walkdir`)
- Reduced binary size with aggressive compilation flags (LTO, strip, opt-level=z)
- Improved performance through better regex compilation and caching

## [0.0.87] - 2025-06-16

## [0.0.86] - 2025-06-14

## [0.0.85] - 2025-06-11

## [0.0.84] - 2025-06-10

### Added

- Type-safe serde-based configuration system for all 24 configurable rules
- Dedicated config modules for each rule with compile-time validation
- Full IDE support with autocomplete for configuration options
- Centralized utilities for common parsing patterns

### Changed

- **BREAKING**: Internal configuration structure refactored (external API unchanged)
- Migrated all rules from manual TOML parsing to serde deserialization
- Improved performance through centralized parsing for:
  - Link and URL detection
  - Code span identification
  - List item processing
  - Block element detection
- Pre-computed line information for better performance
- ~40% reduction in configuration boilerplate code

### Fixed

- MD030: Correct handling of tab characters in list items

### Performance

- Significant performance improvements across multiple rules through:
  - Centralized regex compilation and caching
  - Reduced redundant parsing operations
  - More efficient text processing algorithms
  - Optimized pattern matching for MD044

## [0.0.83] - 2025-06-07

### Fixed

- Various bug fixes and performance improvements

## [0.0.82] - 2025-06-06

### Fixed

- Various bug fixes and stability improvements

## [0.0.81] - 2025-06-04

### Added

- Initial implementation of remaining rules for markdownlint parity

[Unreleased]: https://github.com/rvben/rumdl/compare/v0.0.200...HEAD
[0.0.200]: https://github.com/rvben/rumdl/compare/v0.0.199...v0.0.200
[0.0.163]: https://github.com/rvben/rumdl/compare/v0.0.162...v0.0.163
[0.0.162]: https://github.com/rvben/rumdl/compare/v0.0.161...v0.0.162
[0.0.161]: https://github.com/rvben/rumdl/compare/v0.0.160...v0.0.161
[0.0.160]: https://github.com/rvben/rumdl/compare/v0.0.159...v0.0.160
[0.0.159]: https://github.com/rvben/rumdl/compare/v0.0.158...v0.0.159
[0.0.158]: https://github.com/rvben/rumdl/compare/v0.0.157...v0.0.158
[0.0.157]: https://github.com/rvben/rumdl/compare/v0.0.156...v0.0.157
[0.0.156]: https://github.com/rvben/rumdl/compare/v0.0.155...v0.0.156
[0.0.155]: https://github.com/rvben/rumdl/compare/v0.0.154...v0.0.155
[0.0.154]: https://github.com/rvben/rumdl/compare/v0.0.153...v0.0.154
[0.0.153]: https://github.com/rvben/rumdl/compare/v0.0.152...v0.0.153
[0.0.152]: https://github.com/rvben/rumdl/compare/v0.0.151...v0.0.152
[0.0.151]: https://github.com/rvben/rumdl/compare/v0.0.150...v0.0.151
[0.0.150]: https://github.com/rvben/rumdl/compare/v0.0.149...v0.0.150
[0.0.149]: https://github.com/rvben/rumdl/compare/v0.0.148...v0.0.149
[0.0.148]: https://github.com/rvben/rumdl/compare/v0.0.147...v0.0.148
[0.0.147]: https://github.com/rvben/rumdl/compare/v0.0.146...v0.0.147
[0.0.146]: https://github.com/rvben/rumdl/compare/v0.0.145...v0.0.146
[0.0.145]: https://github.com/rvben/rumdl/compare/v0.0.144...v0.0.145
[0.0.144]: https://github.com/rvben/rumdl/compare/v0.0.143...v0.0.144
[0.0.142]: https://github.com/rvben/rumdl/compare/v0.0.141...v0.0.142
[0.0.140]: https://github.com/rvben/rumdl/compare/v0.0.139...v0.0.140
[0.0.138]: https://github.com/rvben/rumdl/compare/v0.0.137...v0.0.138
[0.0.137]: https://github.com/rvben/rumdl/compare/v0.0.136...v0.0.137
[0.0.136]: https://github.com/rvben/rumdl/compare/v0.0.135...v0.0.136
[0.0.135]: https://github.com/rvben/rumdl/compare/v0.0.134...v0.0.135
[0.0.134]: https://github.com/rvben/rumdl/compare/v0.0.133...v0.0.134
[0.0.133]: https://github.com/rvben/rumdl/compare/v0.0.132...v0.0.133
[0.0.132]: https://github.com/rvben/rumdl/compare/v0.0.131...v0.0.132
[0.0.131]: https://github.com/rvben/rumdl/compare/v0.0.130...v0.0.131
[0.0.130]: https://github.com/rvben/rumdl/compare/v0.0.129...v0.0.130
[0.0.129]: https://github.com/rvben/rumdl/compare/v0.0.128...v0.0.129
[0.0.128]: https://github.com/rvben/rumdl/compare/v0.0.127...v0.0.128
[0.0.127]: https://github.com/rvben/rumdl/compare/v0.0.126...v0.0.127
[0.0.126]: https://github.com/rvben/rumdl/compare/v0.0.125...v0.0.126
[0.0.125]: https://github.com/rvben/rumdl/compare/v0.0.124...v0.0.125
[0.0.124]: https://github.com/rvben/rumdl/compare/v0.0.123...v0.0.124
[0.0.123]: https://github.com/rvben/rumdl/compare/v0.0.122...v0.0.123
[0.0.122]: https://github.com/rvben/rumdl/compare/v0.0.121...v0.0.122
[0.0.121]: https://github.com/rvben/rumdl/compare/v0.0.120...v0.0.121
[0.0.120]: https://github.com/rvben/rumdl/compare/v0.0.119...v0.0.120
[0.0.119]: https://github.com/rvben/rumdl/compare/v0.0.118...v0.0.119
[0.0.118]: https://github.com/rvben/rumdl/compare/v0.0.117...v0.0.118
[0.0.117]: https://github.com/rvben/rumdl/compare/v0.0.116...v0.0.117
[0.0.116]: https://github.com/rvben/rumdl/compare/v0.0.115...v0.0.116
[0.0.115]: https://github.com/rvben/rumdl/compare/v0.0.114...v0.0.115
[0.0.114]: https://github.com/rvben/rumdl/compare/v0.0.113...v0.0.114
[0.0.113]: https://github.com/rvben/rumdl/compare/v0.0.112...v0.0.113
[0.0.112]: https://github.com/rvben/rumdl/compare/v0.0.111...v0.0.112
[0.0.110]: https://github.com/rvben/rumdl/compare/v0.0.109...v0.0.110
[0.0.107]: https://github.com/rvben/rumdl/compare/v0.0.106...v0.0.107
[0.0.106]: https://github.com/rvben/rumdl/compare/v0.0.105...v0.0.106
[0.0.105]: https://github.com/rvben/rumdl/compare/v0.0.104...v0.0.105
[0.0.104]: https://github.com/rvben/rumdl/compare/v0.0.103...v0.0.104
[0.0.102]: https://github.com/rvben/rumdl/compare/v0.0.101...v0.0.102
[0.0.101]: https://github.com/rvben/rumdl/compare/v0.0.100...v0.0.101
[0.0.100]: https://github.com/rvben/rumdl/compare/v0.0.99...v0.0.100
[0.0.99]: https://github.com/rvben/rumdl/compare/v0.0.98...v0.0.99
[0.0.98]: https://github.com/rvben/rumdl/compare/v0.0.97...v0.0.98
[0.0.97]: https://github.com/rvben/rumdl/compare/v0.0.96...v0.0.97
[0.0.96]: https://github.com/rvben/rumdl/compare/v0.0.95...v0.0.96
[0.0.95]: https://github.com/rvben/rumdl/compare/v0.0.94...v0.0.95
[0.0.94]: https://github.com/rvben/rumdl/compare/v0.0.93...v0.0.94
[0.0.93]: https://github.com/rvben/rumdl/compare/v0.0.92...v0.0.93
[0.0.92]: https://github.com/rvben/rumdl/compare/v0.0.91...v0.0.92
[0.0.91]: https://github.com/rvben/rumdl/compare/v0.0.90...v0.0.91
[0.0.90]: https://github.com/rvben/rumdl/compare/v0.0.89...v0.0.90
[0.0.89]: https://github.com/rvben/rumdl/compare/v0.0.88...v0.0.89
[0.0.88]: https://github.com/rvben/rumdl/compare/v0.0.87...v0.0.88
[0.0.87]: https://github.com/rvben/rumdl/compare/v0.0.86...v0.0.87
[0.0.86]: https://github.com/rvben/rumdl/compare/v0.0.85...v0.0.86
[0.0.85]: https://github.com/rvben/rumdl/compare/v0.0.84...v0.0.85
[0.0.84]: https://github.com/rvben/rumdl/compare/v0.0.83...v0.0.84
[0.0.83]: https://github.com/rvben/rumdl/compare/v0.0.82...v0.0.83
[0.0.82]: https://github.com/rvben/rumdl/compare/v0.0.81...v0.0.82
[0.0.81]: https://github.com/rvben/rumdl/releases/tag/v0.0.81
