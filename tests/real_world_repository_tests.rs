use rumdl_lib::lint_context::LintContext;
use rumdl_lib::rule::Rule;
use rumdl_lib::rules::*;
use rumdl_lib::utils::fix_utils::apply_warning_fixes;
use std::time::Instant;

/// Test rumdl on real-world markdown content from large repositories
#[test]
fn test_large_repository_simulation() {
    let test_files = vec![
        ("README.md", create_readme_content()),
        ("CONTRIBUTING.md", create_contributing_content()),
        ("CHANGELOG.md", create_changelog_content()),
    ];

    let rules: Vec<Box<dyn Rule>> = vec![
        Box::new(MD001HeadingIncrement),
        Box::new(MD009TrailingSpaces::default()),
        Box::new(MD011NoReversedLinks),
        Box::new(MD022BlanksAroundHeadings::default()),
        Box::new(MD031BlanksAroundFences::default()),
        Box::new(MD032BlanksAroundLists::default()),
    ];

    for (filename, content) in &test_files {
        println!("Testing file: {filename}");
        let start_time = Instant::now();

        let ctx = LintContext::new(content);
        let mut all_warnings = Vec::new();

        for rule in &rules {
            let rule_start = Instant::now();
            let warnings = rule.check(&ctx).expect("Rule check should succeed");
            let rule_duration = rule_start.elapsed();

            // Performance validation - no rule should take more than 1 second
            assert!(
                rule_duration.as_millis() < 1000,
                "Rule {} took too long on {}: {}ms",
                rule.name(),
                filename,
                rule_duration.as_millis()
            );

            all_warnings.extend(warnings);
        }

        let total_check_duration = start_time.elapsed();
        println!(
            "  Total check time: {}ms, {} warnings",
            total_check_duration.as_millis(),
            all_warnings.len()
        );

        // Test CLI/LSP consistency for critical rules
        let critical_rules = vec![
            Box::new(MD022BlanksAroundHeadings::new()) as Box<dyn Rule>,
            Box::new(MD009TrailingSpaces::default()) as Box<dyn Rule>,
        ];

        for rule in &critical_rules {
            let warnings = rule.check(&ctx).expect("Rule check should succeed");
            if !warnings.is_empty() {
                let cli_start = Instant::now();
                let cli_fixed = rule.fix(&ctx).expect("CLI fix should succeed");
                let cli_duration = cli_start.elapsed();

                let lsp_start = Instant::now();
                let lsp_fixed = apply_warning_fixes(content, &warnings).expect("LSP fix should succeed");
                let lsp_duration = lsp_start.elapsed();

                // Performance check
                assert!(
                    cli_duration.as_millis() < 1000,
                    "CLI fix for {} took too long on {}: {}ms",
                    rule.name(),
                    filename,
                    cli_duration.as_millis()
                );
                assert!(
                    lsp_duration.as_millis() < 1000,
                    "LSP fix for {} took too long on {}: {}ms",
                    rule.name(),
                    filename,
                    lsp_duration.as_millis()
                );

                // Consistency check
                assert_eq!(
                    cli_fixed,
                    lsp_fixed,
                    "Rule {} produced different CLI vs LSP results on {}",
                    rule.name(),
                    filename
                );
            }
        }
    }
}

fn create_readme_content() -> String {
    r#"# Project Name

[![Build Status](https://img.shields.io/badge/build-passing-brightgreen)](https://example.com)
[![Coverage](https://img.shields.io/badge/coverage-95%25-brightgreen)](https://example.com)

A comprehensive markdown linter and formatter for maintaining consistent documentation.

## Features

- **Fast Performance**: Process thousands of files in seconds
- **Extensive Rules**: 50+ built-in linting rules
- **Auto-fixing**: Automatically fix many common issues
- **LSP Support**: Real-time linting in your favorite editor
- **Configurable**: Customize rules for your project needs

## Quick Start

### Installation

```bash
# Using cargo
cargo install rumdl

# Using npm
npm install -g rumdl

# Using homebrew
brew install rumdl
```

### Basic Usage

```bash
# Check all markdown files
rumdl check .

# Auto-fix issues
rumdl fix *.md

# Generate config file
rumdl init
```

## Configuration

Create a `.rumdl.toml` file in your project root:

```toml
# Global settings
line-length = 100
exclude = ["node_modules", "vendor"]

# Rule-specific configuration
[MD013]
line-length = 120
code-blocks = false

[MD033]
allowed_elements = ["br", "hr"]
```

## Rules

For a complete list of rules, see our documentation.

### Popular Rules

- **MD001**: Heading levels should only increment by one level at a time
- **MD009**: Trailing spaces should be removed
- **MD022**: Headings should be surrounded by blank lines
- **MD031**: Fenced code blocks should be surrounded by blank lines

## Contributing

We welcome contributions! Please see our Contributing Guide for details.

### Development Setup

1. Clone the repository
2. Install dependencies: `cargo build`
3. Run tests: `cargo test`
4. Submit a pull request

## License

This project is licensed under the MIT License - see the LICENSE file for details.

## Acknowledgments

- Thanks to all contributors who have helped improve this project
- Inspired by markdownlint and other linting tools
- Built with Rust for maximum performance and reliability
"#
    .to_string()
}

fn create_contributing_content() -> String {
    r#"# Contributing to Project

Thank you for your interest in contributing! This document provides guidelines for contributors.

## Getting Started

### Prerequisites

- Rust 1.70 or later
- Git
- A text editor or IDE

### Setting Up Development Environment

1. Fork the repository on GitHub
2. Clone your fork locally:
   ```bash
   git clone https://github.com/yourusername/project.git
   cd project
   ```
3. Build the project:
   ```bash
   cargo build
   ```
4. Run tests to ensure everything works:
   ```bash
   cargo test
   ```

## Development Workflow

### Branch Naming

Use descriptive branch names:
- `feature/add-new-rule`
- `fix/heading-detection-bug`
- `docs/update-api-reference`

### Commit Messages

Follow conventional commit format:
- `feat: add support for custom rules`
- `fix: resolve unicode handling in headings`
- `docs: update installation instructions`
- `test: add edge case tests for MD022`

## Testing

### Running Tests

```bash
# Run all tests
cargo test

# Run specific test
cargo test test_name

# Run with output
cargo test -- --nocapture

# Run release tests (faster)
cargo test --release
```

### Writing Tests

- Add unit tests for new functionality
- Include edge cases and error conditions
- Test both CLI and LSP behavior for consistency
- Update integration tests when adding new rules

## Pull Request Process

1. **Create an Issue**: Discuss major changes before implementing
2. **Write Tests**: Ensure new code has appropriate test coverage
3. **Update Documentation**: Add or update relevant documentation
4. **Run Full Test Suite**: Verify all tests pass
5. **Submit PR**: Use the provided template and link to related issues

### PR Checklist

- [ ] Tests pass locally
- [ ] Code follows style guidelines
- [ ] Documentation updated
- [ ] No breaking changes (or properly documented)
- [ ] Commit messages follow conventions

## Style Guidelines

### Rust Code

- Follow `rustfmt` formatting (run `cargo fmt`)
- Use `clippy` for linting (run `cargo clippy`)
- Write clear, self-documenting code
- Add comments for complex logic
- Use meaningful variable and function names

### Documentation

- Use clear, concise language
- Provide examples where helpful
- Keep line length under 100 characters
- Use proper markdown formatting

Thank you for contributing to make this project better for everyone!
"#
    .to_string()
}

fn create_changelog_content() -> String {
    r#"# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- New rule MD059 for checking table formatting
- Support for custom rule plugins
- Performance optimizations for large files

### Changed
- Improved error messages with more context
- Updated CLI help text for clarity

### Fixed
- Unicode handling in heading detection
- Memory leak in long-running LSP sessions

## [0.3.0] - 2024-01-15

### Added
- LSP (Language Server Protocol) support
- Real-time linting in editors
- WebAssembly build for browser usage
- New rules: MD055, MD056, MD057, MD058
- Configuration inheritance from parent directories
- JSON output format for CI/CD integration

### Changed
- **Breaking**: Rule configuration format updated
- Improved performance by 40% on average
- CLI commands reorganized for better UX
- Default line length changed from 80 to 100

### Deprecated
- Old configuration format (will be removed in v1.0)
- `--strict` flag (use `--config strict.toml` instead)

### Fixed
- Edge cases in list item detection
- False positives in MD032 for nested lists
- Incorrect ranges for some Unicode characters
- Memory usage with very large files

### Security
- Input sanitization for configuration files
- Path traversal prevention in file operations

## [0.2.5] - 2023-12-10

### Added
- Support for markdownlint configuration files
- New rule MD054 for link style consistency
- Parallel processing for multiple files
- Progress bar for long operations

### Fixed
- Regression in MD022 heading detection
- Performance issue with regular expressions
- Incorrect fix ranges for multi-byte characters

## [0.2.4] - 2023-11-20

### Fixed
- Critical bug in MD009 trailing spaces rule
- Compatibility with Windows line endings
- Edge case in code block language detection

## [0.2.3] - 2023-11-15

### Added
- New rule MD053 for link reference definitions
- Support for ignore patterns in configuration
- Benchmark suite for performance testing

### Changed
- Improved CLI error messages
- Better handling of malformed markdown

### Fixed
- False positives in MD031 fenced code blocks
- Incorrect line counting with tabs
- Memory leak in file watching mode

## [0.1.0] - 2023-05-25

### Added
- Initial release
- Basic markdown linting
- Rules: MD001, MD002, MD003
- CLI interface
- Basic documentation

### Known Issues
- Limited rule coverage
- No configuration support
- Performance not optimized
"#
    .to_string()
}

#[test]
fn test_memory_usage_with_large_content() {
    // Test memory behavior with a very large single file
    let large_content = create_large_content(50000); // ~50k lines

    let rules: Vec<Box<dyn Rule>> = vec![
        Box::new(MD009TrailingSpaces::default()),
        Box::new(MD022BlanksAroundHeadings::default()),
    ];

    for rule in &rules {
        let ctx = LintContext::new(&large_content);

        let start_time = Instant::now();
        let warnings = rule.check(&ctx).expect("Rule check should succeed");
        let check_duration = start_time.elapsed();

        // Should handle large content efficiently (under 5 seconds)
        assert!(
            check_duration.as_secs() < 5,
            "Rule {} took too long on large content: {}s",
            rule.name(),
            check_duration.as_secs()
        );

        // Test fix performance if there are warnings
        if !warnings.is_empty() {
            let start_time = Instant::now();
            let _fixed = rule.fix(&ctx).expect("Fix should succeed");
            let fix_duration = start_time.elapsed();

            assert!(
                fix_duration.as_secs() < 10,
                "Rule {} fix took too long on large content: {}s",
                rule.name(),
                fix_duration.as_secs()
            );
        }
    }
}

fn create_large_content(lines: usize) -> String {
    let mut content = String::with_capacity(lines * 50); // Rough estimate

    for i in 1..=lines {
        if i % 100 == 1 {
            content.push_str(&format!("# Section {}\n", i / 100 + 1));
        } else if i % 20 == 1 {
            content.push_str(&format!("## Subsection {i}\n"));
        } else if i % 5 == 0 {
            content.push_str("```rust\n");
            content.push_str(&format!("let var_{i} = {i};\n"));
            content.push_str("```\n");
        } else {
            content.push_str(&format!("This is line {i} with some content.\n"));
        }
    }

    content
}
