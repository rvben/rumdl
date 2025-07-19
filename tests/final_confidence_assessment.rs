use rumdl::lint_context::LintContext;
use rumdl::rule::Rule;
use rumdl::rules::*;
use rumdl::utils::fix_utils::apply_warning_fixes;
use std::time::Instant;

/// Final comprehensive validation for release confidence
/// This test suite provides a holistic assessment of system readiness
#[test]
fn test_comprehensive_release_validation() {
    println!("🚀 Running Final Release Confidence Assessment");

    // Core functionality validation
    validate_core_functionality();

    // CLI/LSP consistency validation
    validate_cli_lsp_consistency();

    // Performance validation
    validate_performance_characteristics();

    // Unicode and edge case validation
    validate_unicode_and_edge_cases();

    // Integration validation
    validate_integration_scenarios();

    println!("✅ All validation checks passed!");
    println!("🎯 System is ready for release with high confidence");
}

fn validate_core_functionality() {
    println!("📋 Validating Core Functionality...");

    // Programmatically construct test content with trailing spaces to avoid linter issues
    let mut test_content = String::new();
    test_content.push_str("# Test Document\nThis is a test paragraph with trailing spaces");
    test_content.push_str("   "); // Explicitly add 3 trailing spaces
    test_content.push_str("\n\n### Skipping Level 2 (MD001 issue)\n\n##Missing Space Heading\n- List item 1\n- List item 2\n\n```\ncode without language\n```\nAnother paragraph.\n");

    let critical_rules: Vec<Box<dyn Rule>> = vec![
        Box::new(MD001HeadingIncrement),
        Box::new(MD009TrailingSpaces::default()),
        Box::new(MD018NoMissingSpaceAtx),
        Box::new(MD022BlanksAroundHeadings::default()),
        Box::new(MD031BlanksAroundFences),
        Box::new(MD040FencedCodeLanguage),
    ];

    let ctx = LintContext::new(&test_content);
    let mut total_warnings = 0;

    for rule in &critical_rules {
        let warnings = rule.check(&ctx).expect("Rule check should succeed");
        total_warnings += warnings.len();

        // Each rule should find expected issues
        assert!(
            !warnings.is_empty(),
            "Rule {} should find issues in test content",
            rule.name()
        );

        // Fixes should work without errors
        if !warnings.is_empty() {
            let _fixed = rule.fix(&ctx).expect("Rule fix should succeed");
        }
    }

    assert!(
        total_warnings >= 5,
        "Should find multiple issues in test content, found {total_warnings}"
    );

    println!("   ✓ Core rules functioning correctly ({total_warnings} warnings found)");
}

fn validate_cli_lsp_consistency() {
    println!("🔄 Validating CLI/LSP Consistency...");

    let test_cases = [
        // Basic heading issues
        "# Heading\nContent without spacing",
        // Trailing spaces
        "Line with trailing spaces   \nAnother line   ",
        // Mixed issues
        "# Title\nContent   \n##Bad Heading\nMore content",
        // Line ending variations
        "# Heading 1\r\nContent with CRLF\n# Heading 2\nContent with LF",
    ];

    let rules: Vec<Box<dyn Rule>> = vec![
        Box::new(MD009TrailingSpaces::default()),
        Box::new(MD018NoMissingSpaceAtx),
        Box::new(MD022BlanksAroundHeadings::default()),
    ];

    let mut consistency_checks = 0;

    for (i, content) in test_cases.iter().enumerate() {
        for rule in &rules {
            let ctx = LintContext::new(content);
            let warnings = rule.check(&ctx).expect("Rule check should succeed");

            if !warnings.is_empty() {
                let cli_fixed = rule.fix(&ctx).expect("CLI fix should succeed");
                let lsp_fixed = apply_warning_fixes(content, &warnings).expect("LSP fix should succeed");

                assert_eq!(
                    cli_fixed,
                    lsp_fixed,
                    "CLI/LSP inconsistency in test case {} with rule {}",
                    i,
                    rule.name()
                );

                consistency_checks += 1;
            }
        }
    }

    assert!(consistency_checks > 0, "Should have performed consistency checks");
    println!("   ✓ CLI/LSP consistency validated ({consistency_checks} checks passed)");
}

fn validate_performance_characteristics() {
    println!("⚡ Validating Performance Characteristics...");

    // Test responsiveness on typical content
    let typical_content = create_typical_document();
    let rules: Vec<Box<dyn Rule>> = vec![
        Box::new(MD009TrailingSpaces::default()),
        Box::new(MD022BlanksAroundHeadings::default()),
        Box::new(MD031BlanksAroundFences),
    ];

    let start_time = Instant::now();
    let ctx = LintContext::new(&typical_content);

    for rule in &rules {
        let rule_start = Instant::now();
        let warnings = rule.check(&ctx).expect("Rule check should succeed");
        let rule_duration = rule_start.elapsed();

        // Should be very responsive for typical content
        assert!(
            rule_duration.as_millis() < 50,
            "Rule {} took too long on typical content: {}ms",
            rule.name(),
            rule_duration.as_millis()
        );

        // Test fix performance
        if !warnings.is_empty() {
            let fix_start = Instant::now();
            let _fixed = apply_warning_fixes(&typical_content, &warnings).expect("Fix should succeed");
            let fix_duration = fix_start.elapsed();

            assert!(
                fix_duration.as_millis() < 30,
                "Fix for {} took too long: {}ms",
                rule.name(),
                fix_duration.as_millis()
            );
        }
    }

    let total_duration = start_time.elapsed();
    assert!(
        total_duration.as_millis() < 200,
        "Total processing took too long: {}ms",
        total_duration.as_millis()
    );

    println!(
        "   ✓ Performance characteristics validated ({}ms total)",
        total_duration.as_millis()
    );
}

fn validate_unicode_and_edge_cases() {
    println!("🌍 Validating Unicode and Edge Cases...");

    let long_line = format!("# Heading\n{}", "a".repeat(1000));
    let edge_cases = [
        // Unicode headings
        "# 中文标题\n内容",
        // Emoji in content
        "# Test 🚀\nContent with emojis 😀",
        // Very long lines
        &long_line,
        // Empty content
        "",
        // Only whitespace
        "   \n\t\n   ",
        // Mixed line endings
        "# Title\r\n\nContent\n",
    ];

    let rule = MD009TrailingSpaces::default();
    let mut edge_case_checks = 0;

    for (i, content) in edge_cases.iter().enumerate() {
        let ctx = LintContext::new(content);

        // Should not panic or error on edge cases
        let result = rule.check(&ctx);
        assert!(
            result.is_ok(),
            "Rule check failed on edge case {}: {:?}",
            i,
            result.err()
        );

        let warnings = result.unwrap();

        // Test fixes don't break on edge cases
        if !warnings.is_empty() {
            let fix_result = apply_warning_fixes(content, &warnings);
            assert!(
                fix_result.is_ok(),
                "Fix failed on edge case {}: {:?}",
                i,
                fix_result.err()
            );
        }

        edge_case_checks += 1;
    }

    println!("   ✓ Unicode and edge cases validated ({edge_case_checks} cases)");
}

fn validate_integration_scenarios() {
    println!("🔌 Validating Integration Scenarios...");

    // Simulate real-world usage patterns
    let integration_tests = vec![
        // README file scenario
        create_readme_scenario(),
        // Documentation file scenario
        create_docs_scenario(),
        // Mixed content scenario
        create_mixed_content_scenario(),
    ];

    let comprehensive_rules: Vec<Box<dyn Rule>> = vec![
        Box::new(MD001HeadingIncrement),
        Box::new(MD009TrailingSpaces::default()),
        Box::new(MD011NoReversedLinks),
        Box::new(MD022BlanksAroundHeadings::default()),
        Box::new(MD031BlanksAroundFences),
    ];

    for (scenario_name, content) in &integration_tests {
        let ctx = LintContext::new(content);
        let mut total_warnings = 0;

        for rule in &comprehensive_rules {
            let warnings = rule.check(&ctx).expect("Rule check should succeed");
            total_warnings += warnings.len();

            // Test that fixes can be applied successfully
            if !warnings.is_empty() {
                let cli_fixed = rule.fix(&ctx).expect("CLI fix should succeed");
                let lsp_fixed = apply_warning_fixes(content, &warnings).expect("LSP fix should succeed");

                // Consistency check
                assert_eq!(
                    cli_fixed,
                    lsp_fixed,
                    "CLI/LSP inconsistency in {} scenario with rule {}",
                    scenario_name,
                    rule.name()
                );
            }
        }

        println!("   ✓ {scenario_name} scenario: {total_warnings} warnings processed");
    }

    println!("   ✓ Integration scenarios validated");
}

fn create_typical_document() -> String {
    r#"# Project Documentation

This is a typical documentation file that might be found in a real project.

## Getting Started

To get started with this project:

1. Clone the repository
2. Install dependencies
3. Run the tests

### Prerequisites

- Rust 1.70+
- Git
- A text editor

## Configuration

You can configure the project using a `.config.toml` file:

```toml
# Example configuration
setting1 = "value1"
setting2 = 42
```

## API Reference

### Core Functions

#### `process_data(input: &str) -> Result<String, Error>`

Processes the input data and returns a result.

**Parameters:**
- `input`: The input string to process

**Returns:** Processed string or error

## Contributing

We welcome contributions! Please see our contributing guide for details.

### Development Setup

1. Fork the repository
2. Create a feature branch
3. Make your changes
4. Submit a pull request

## License

This project is licensed under the MIT License.
"#
    .to_string()
}

fn create_readme_scenario() -> (&'static str, String) {
    (
        "README",
        r#"# My Project

[![Build Status](https://img.shields.io/badge/build-passing-green)](https://example.com)

A comprehensive tool for processing markdown files.

## Features

- Fast processing
- Multiple output formats
- Extensive configuration options

## Installation

```bash
cargo install my-project
```

## Usage

```bash
# Basic usage
my-project input.md

# With options
my-project --format html input.md
```

## Configuration

Create a config file:

```toml
format = "html"
output_dir = "dist"
```

## Contributing

Please read CONTRIBUTING.md for details.

## License

MIT License - see LICENSE file.
"#
        .to_string(),
    )
}

fn create_docs_scenario() -> (&'static str, String) {
    (
        "Documentation",
        r#"# API Documentation

This document describes the public API.

## Overview

The API provides several endpoints for data processing.

### Authentication

All requests require authentication:

```bash
curl -H "Authorization: Bearer TOKEN" https://api.example.com/data
```

## Endpoints

### GET /data

Retrieves data from the system.

**Parameters:**
- `format`: Output format (json, xml)
- `limit`: Maximum number of results

**Response:**
```json
{
  "data": [...],
  "total": 100
}
```

### POST /data

Creates new data entry.

**Request Body:**
```json
{
  "name": "Example",
  "value": 42
}
```

## Error Handling

The API returns standard HTTP status codes:

- `200`: Success
- `400`: Bad Request
- `401`: Unauthorized
- `500`: Internal Server Error

## Rate Limiting

Requests are limited to 1000 per hour per API key.
"#
        .to_string(),
    )
}

fn create_mixed_content_scenario() -> (&'static str, String) {
    (
        "Mixed Content",
        r#"# Mixed Content Test

This file contains various markdown elements to test comprehensive processing.

## Code Blocks

Here's some Rust code:

```rust
fn main() {
    println!("Hello, world!");
}
```

And some JavaScript:

```javascript
console.log("Hello from JS");
```

## Lists

### Unordered Lists

- Item 1
- Item 2
  - Nested item
  - Another nested item
- Item 3

### Ordered Lists

1. First step
2. Second step
3. Third step

## Links and References

Visit [our website](https://example.com) for more info.

Check out the [documentation][docs] as well.

[docs]: https://docs.example.com

## Tables

| Feature | Status | Notes |
|---------|--------|-------|
| API | ✅ | Complete |
| UI | 🚧 | In Progress |
| Docs | ✅ | Complete |

## Emphasis

This is **bold** text and this is *italic* text.

You can also use ***bold italic*** for emphasis.

## Blockquotes

> This is a blockquote with some important information.
>
> It can span multiple lines and contain other elements.

## Horizontal Rules

---

That's the end of the mixed content test.
"#
        .to_string(),
    )
}
