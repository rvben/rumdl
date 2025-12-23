use rumdl_lib::config::{Config, MarkdownFlavor};
/// Test for MD051 false positives with Table of Contents sections
/// This test verifies that headings after a TOC section are properly detected
use rumdl_lib::rules;

#[test]
fn test_md051_toc_section_should_not_skip_headings() {
    let content = r#"# Document

## Table of Contents

- [Configuration](#configuration)
- [Development](#development)
- [Testing](#testing)

## Configuration

Configuration content here.

## Development

Development content here.

## Testing

Testing content here.
"#;

    let config = Config::default();
    let all_rules = rules::all_rules(&config);
    let md051_rules: Vec<_> = all_rules.into_iter().filter(|r| r.name() == "MD051").collect();

    let warnings = rumdl_lib::lint(content, &md051_rules, false, MarkdownFlavor::Standard, None).unwrap();

    // There should be NO warnings - all the linked headings exist
    assert_eq!(
        warnings.len(),
        0,
        "MD051 should not report false positives for headings after TOC. Found warnings: {:?}",
        warnings.iter().map(|w| &w.message).collect::<Vec<_>>()
    );
}

#[test]
fn test_md051_nested_toc_with_valid_headings() {
    let content = r#"# My Project

## Table of Contents

- [Overview](#overview)
- [Features](#features)
  - [Performance](#performance)
  - [Compatibility](#compatibility)
- [Installation](#installation)

## Overview

This is the overview section.

## Features

Main features section.

### Performance

Performance details.

### Compatibility

Compatibility details.

## Installation

Installation instructions.
"#;

    let config = Config::default();
    let all_rules = rules::all_rules(&config);
    let md051_rules: Vec<_> = all_rules.into_iter().filter(|r| r.name() == "MD051").collect();

    let warnings = rumdl_lib::lint(content, &md051_rules, false, MarkdownFlavor::Standard, None).unwrap();

    assert_eq!(
        warnings.len(),
        0,
        "MD051 should not report false positives for nested TOC. Found warnings: {:?}",
        warnings.iter().map(|w| &w.message).collect::<Vec<_>>()
    );
}
