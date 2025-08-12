# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.0.115] - 2025-08-12

## [0.0.114] - 2025-08-09

## [0.0.113] - 2025-08-09

## [0.0.112] - 2025-08-08

## [0.0.110] - 2025-08-08

## [0.0.110] - 2025-08-08

## [0.0.107] - 2025-08-06

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

[Unreleased]: https://github.com/rvben/rumdl/compare/v0.0.115...HEAD
[0.0.115]: https://github.com/rvben/rumdl/compare/v0.0.114...v0.0.115
[0.0.114]: https://github.com/rvben/rumdl/compare/v0.0.113...v0.0.114
[0.0.113]: https://github.com/rvben/rumdl/compare/v0.0.112...v0.0.113
[0.0.112]: https://github.com/rvben/rumdl/compare/v0.0.111...v0.0.112
[0.0.110]: https://github.com/rvben/rumdl/compare/v0.0.109...v0.0.110
[0.0.110]: https://github.com/rvben/rumdl/compare/v0.0.109...v0.0.110
[0.0.107]: https://github.com/rvben/rumdl/compare/v0.0.106...v0.0.107
[0.0.107]: https://github.com/rvben/rumdl/compare/v0.0.106...v0.0.107
[0.0.105]: https://github.com/rvben/rumdl/compare/v0.0.104...v0.0.105
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
