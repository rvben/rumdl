# rumdl - A Markdown Linter written in Rust

rumdl is a fast and efficient Markdown linter that helps ensure consistency and best practices in your Markdown files.

## Features

- 50+ rules for Markdown linting
- Fast performance with Rust
- Configurable rule settings
- Detailed error reporting

## Installation 

```
cargo install rumdl
```

## Usage

```
rumdl [options] <file>...
```

### Options

- `-c, --config <file>`: Use custom configuration file
- `-f, --fix`: Automatically fix issues where possible
- `-l, --list-rules`: List all available rules
- `-d, --disable <rules>`: Disable specific rules (comma-separated)
- `-v, --verbose`: Show detailed output

## Rules

For a complete list of rules and their descriptions, run `rumdl --list-rules` or visit our [documentation](docs/RULES.md).

### Rules Overview

The following table provides an overview of all supported rules and indicates whether rumdl can detect violations and automatically fix them:

| Rule ID | Description | Detects | Fixes |
|---------|-------------|:-------:|:-----:|
| MD001   | Heading levels should only increment by one level at a time | ✅ | ✅ |
| MD002   | First heading should be a top-level heading | ✅ | ✅ |
| MD003   | Heading style should be consistent | ✅ | ✅ |
| MD004   | Unordered list style should be consistent | ✅ | ✅ |
| MD005   | Consistent indentation for list items at the same level | ✅ | ✅ |
| MD006   | Start bullets at the beginning of the line | ✅ | ✅ |
| MD007   | Unordered list indentation | ✅ | ✅ |
| MD008   | Unordered list style | ✅ | ✅ |
| MD009   | Trailing spaces | ✅ | ✅ |
| MD010   | Hard tabs | ✅ | ✅ |
| MD011   | Reversed link syntax | ✅ | ✅ |
| MD012   | Multiple consecutive blank lines | ✅ | ✅ |
| MD013   | Line length | ✅ | ❌ |
| MD014   | Dollar signs used before commands without showing output | ✅ | ✅ |
| MD015   | No space after list marker | ✅ | ✅ |
| MD016   | Multiple spaces after list marker | ✅ | ✅ |
| MD017   | No emphasis as heading | ✅ | ✅ |
| MD018   | No space after hash on atx style heading | ✅ | ✅ |
| MD019   | Multiple spaces after hash on atx style heading | ✅ | ✅ |
| MD020   | No space inside hashes on closed atx style heading | ✅ | ✅ |
| MD021   | Multiple spaces inside hashes on closed atx style heading | ✅ | ✅ |
| MD022   | Headings should be surrounded by blank lines | ✅ | ✅ |
| MD023   | Headings must start at the beginning of the line | ✅ | ✅ |
| MD024   | Multiple headings with the same content | ✅ | ❌ |
| MD025   | Multiple top-level headings in the same document | ✅ | ❌ |
| MD026   | Trailing punctuation in heading | ✅ | ✅ |
| MD027   | Multiple spaces after blockquote symbol | ✅ | ✅ |
| MD028   | Blank line inside blockquote | ✅ | ✅ |
| MD029   | Ordered list item prefix | ✅ | ✅ |
| MD030   | Spaces after list markers | ✅ | ✅ |
| MD031   | Fenced code blocks should be surrounded by blank lines | ✅ | ✅ |
| MD032   | Lists should be surrounded by blank lines | ✅ | ✅ |
| MD033   | Inline HTML | ✅ | ❌ |
| MD034   | Bare URL used | ✅ | ✅ |
| MD035   | Horizontal rule style | ✅ | ✅ |
| MD036   | Emphasis used instead of a heading | ✅ | ✅ |
| MD037   | Spaces inside emphasis markers | ✅ | ✅ |
| MD038   | Spaces inside code span elements | ✅ | ✅ |
| MD039   | Spaces inside link text | ✅ | ✅ |
| MD040   | Fenced code blocks should have a language specified | ✅ | ✅ |
| MD041   | First line in a file should be a top-level heading | ✅ | ✅ |
| MD042   | No empty links | ✅ | ✅ |
| MD043   | Required heading structure | ✅ | ❌ |
| MD044   | Proper names should have the correct capitalization | ✅ | ✅ |
| MD045   | Images should have alternate text | ✅ | ✅ |
| MD046   | Code block style | ✅ | ✅ |
| MD047   | Files should end with a single newline character | ✅ | ✅ |
| MD048   | Code fence style | ✅ | ✅ |
| MD049   | Emphasis style | ✅ | ✅ |
| MD050   | Strong style | ✅ | ✅ |
| MD051   | Link fragments should exist | ✅ | ❌ |
| MD052   | Reference links and images should use a reference that exists | ✅ | ❌ |
| MD053   | Link and image reference definitions should be needed | ✅ | ✅ |

## Development

### Prerequisites

- Rust 1.70 or higher
- Make (for development commands)

### Building

```
make build
```

### Testing

```
make test
```

## License

MIT License