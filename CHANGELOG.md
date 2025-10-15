# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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

- **Removed legacy fix implementation** - Removed old single-pass fix implementation and `RUMDL_NO_FIX_COORDINATOR` environment variable. Fix Coordinator is now the only fix strategy, providing ~75% faster fixes with better coverage.

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
  - Supports http://, https://, ftp://, ftps:// protocols
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
  - MD028 now only flags truly blank lines inside blockquotes, not `>` or `> ` lines
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

[Unreleased]: https://github.com/rvben/rumdl/compare/v0.0.144...HEAD
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
