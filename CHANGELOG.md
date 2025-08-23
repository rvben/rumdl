# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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

[Unreleased]: https://github.com/rvben/rumdl/compare/v0.0.126...HEAD
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
