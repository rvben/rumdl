# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.0.92] - 2025-07-02

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

[Unreleased]: https://github.com/rvben/rumdl/compare/v0.0.92...HEAD
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