# rumdl Rules Reference

## A comprehensive reference of all Markdown linting rules

## Introduction

rumdl implements 50+ rules for checking Markdown files. This document provides a comprehensive reference of all available rules, organized by category.
Each rule has a brief description and a link to its detailed documentation.

## Rule Categories

- [Heading Rules](#heading-rules) - Rules related to heading structure and formatting
- [List Rules](#list-rules) - Rules for list formatting and structure
- [Whitespace Rules](#whitespace-rules) - Rules for spacing, indentation, and line length
- [Formatting Rules](#formatting-rules) - Rules for general Markdown formatting
- [Code Block Rules](#code-block-rules) - Rules specific to code blocks and fences
- [Link and Image Rules](#link-and-image-rules) - Rules for links, references, and images
- [Table Rules](#table-rules) - Rules for table formatting and structure
- [Other Rules](#other-rules) - Miscellaneous rules that don't fit the other categories

## Heading Rules

| Rule ID | Rule Name | Description |
|---------|-----------|-------------|
| [MD001](md001.md) | Heading increment | Headings should only increment by one level at a time |
| [MD002](md002.md) | First heading h1 | First heading should be a top-level heading |
| [MD003](md003.md) | Heading style | Heading style should be consistent |
| [MD018](md018.md) | No space atx | No space after hash on atx style heading |
| [MD019](md019.md) | Multiple space atx | Multiple spaces after hash on atx style heading |
| [MD020](md020.md) | No space closed atx | No space inside hashes on closed atx style heading |
| [MD021](md021.md) | Multiple space closed atx | Multiple spaces inside hashes on closed atx style heading |
| [MD022](md022.md) | Blanks around headings | Headings should be surrounded by blank lines |
| [MD023](md023.md) | Heading start left | Headings must start at the beginning of the line |
| [MD024](md024.md) | Multiple headings | Multiple headings with the same content |
| [MD025](md025.md) | Single title | Multiple top-level headings in the same document |
| [MD036](md036.md) | No emphasis as heading | Emphasis used instead of a heading |
| [MD041](md041.md) | First line h1 | First line in a file should be a top-level heading |
| [MD043](md043.md) | Required headings | Required heading structure |

## List Rules

| Rule ID | Rule Name | Description |
|---------|-----------|-------------|
| [MD004](md004.md) | UL style | Unordered list style |
| [MD005](md005.md) | List indent | Inconsistent indentation for list items at the same level |
| [MD006](md006.md) | Start bullets | Consider starting bulleted lists at the beginning of the line |
| [MD007](md007.md) | UL indent | Unordered list indentation |
| [MD008](md008.md) | UL space | Unordered list spacing |
| [MD029](md029.md) | OL prefix | Ordered list item prefix |
| [MD030](md030.md) | List marker space | Spaces after list markers |
| [MD032](md032.md) | Blanks around lists | Lists should be surrounded by blank lines |
| [MD051](md051.md) | Link fragments | Link fragments should be valid heading IDs |

## Whitespace Rules

| Rule ID | Rule Name | Description |
|---------|-----------|-------------|
| [MD009](md009.md) | No trailing spaces | No trailing spaces |
| [MD010](md010.md) | No hard tabs | No hard tabs |
| [MD012](md012.md) | No multiple blanks | No multiple consecutive blank lines |
| [MD013](md013.md) | Line length | Line length |
| [MD027](md027.md) | Multiple spaces blockquote | Multiple spaces after blockquote symbol |
| [MD028](md028.md) | Blanks blockquote | Blank line inside blockquote |
| [MD031](md031.md) | Blanks around fences | Fenced code blocks should be surrounded by blank lines |
| [MD047](md047.md) | File end newline | Files should end with a single newline character |

## Formatting Rules

| Rule ID | Rule Name | Description |
|---------|-----------|-------------|
| [MD026](md026.md) | No trailing punctuation | Trailing punctuation in heading |
| [MD033](md033.md) | No inline HTML | Inline HTML |
| [MD035](md035.md) | HR style | Horizontal rule style |
| [MD037](md037.md) | Spaces around emphasis | Spaces inside emphasis markers |
| [MD038](md038.md) | No space in code | Spaces inside code span elements |
| [MD039](md039.md) | No space in links | Spaces inside link text |
| [MD044](md044.md) | Proper names | Proper names should have consistent capitalization |
| [MD049](md049.md) | Emphasis style | Emphasis style should be consistent |
| [MD050](md050.md) | Strong style | Strong style should be consistent |

## Code Block Rules

| Rule ID | Rule Name | Description |
|---------|-----------|-------------|
| [MD014](md014.md) | Commands show output | Code blocks should show output when appropriate |
| [MD040](md040.md) | Fenced code language | Fenced code blocks should have a language specified |
| [MD046](md046.md) | Code block style | Code block style |
| [MD048](md048.md) | Code fence style | Code fence style |

## Link and Image Rules

| Rule ID | Rule Name | Description |
|---------|-----------|-------------|
| [MD011](md011.md) | Reversed link | Reversed link syntax |
| [MD034](md034.md) | No bare URLs | Bare URL used |
| [MD042](md042.md) | No empty links | No empty links |
| [MD045](md045.md) | No alt text | Images should have alternate text |
| [MD052](md052.md) | Reference links images | References should be defined |
| [MD053](md053.md) | Link image definitions | Link and image reference definitions should be needed |
| [MD054](md054.md) | Link image style | Link and image style |

## Table Rules

| Rule ID | Rule Name | Description |
|---------|-----------|-------------|
| [MD055](md055.md) | Table pipe style | Table pipe style should be consistent |
| [MD056](md056.md) | Table column count | Table column count should be consistent |
| [MD058](md058.md) | Table spacing | Tables should be surrounded by blank lines |

## Other Rules

| Rule ID | Rule Name | Description |
|---------|-----------|-------------|
| [MD057](md057.md) | Relative links | Relative links should exist |

## Using Rules

Rules can be enabled, disabled, or configured in your rumdl configuration file:

```toml
# Global configuration options
[global]
# List of rules to disable
disable = ["MD013", "MD033"]

# Rule-specific configurations
[MD003]
style = "atx"  # Heading style (atx, atx_closed, setext)

[MD004]
style = "consistent"  # List style (asterisk, plus, dash, consistent)
```

For more information on configuring rumdl, see the [Configuration](#configuration) section.

## Rule Severities

Each rule has a default severity level:

- **error**: Rule violations will cause the linter to exit with a non-zero code
- **warning**: Rule violations will be reported but won't affect the exit code
- **info**: Rule violations are reported for informational purposes only

You can customize rule severities in your configuration file:

```toml
[MD013]
severity = "warning"  # Downgrade from error to warning
```

## Configuration

You can configure rumdl using a TOML configuration file. Create a default configuration file using:

```bash
rumdl init
```

This generates a `.rumdl.toml` file with default settings that you can customize.
