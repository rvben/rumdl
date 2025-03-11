# rumdl - An extremely fast Markdown linter, written in Rust

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](<https://opensource.org/licenses/MIT>)

rumdl is a fast Markdown linter and fixer that helps ensure consistency and best practices in your Markdown files. Built in Rust for exceptional performance.

## Features

- **Lightning Fast**: Built with Rust for exceptional performance
- **50+ lint rules**: Comprehensive rule set covering common Markdown issues
- **Automatic fixing**: Many rules support automatic fixing with `--fix`
- **Highly configurable**: Customize rules to match your project's style
- **Modern CLI**: User-friendly interface with detailed error reporting

## Installation

With Cargo:

```bash
cargo install rumdl
```

## Usage

Check Markdown files for issues:

```bash
# Check a single file
rumdl README.md

# Check multiple files
rumdl doc1.md doc2.md

# Check all Markdown files in a directory (recursive)
rumdl .

# Check and automatically fix issues
rumdl --fix README.md

# Create a default configuration file
rumdl init
```

## Command-line Interface

```bash
rumdl [options] [file or directory...]
rumdl <command> [options]
```

### Commands

- `init`: Create a default `.rumdl.toml` configuration file in the current directory

### Options

- `-c, --config <file>`: Use custom configuration file
- `-f, --fix`: Automatically fix issues where possible
- `-l, --list-rules`: List all available rules
- `-d, --disable <rules>`: Disable specific rules (comma-separated)
- `-e, --enable <rules>`: Enable only specific rules (comma-separated)
- `--exclude <patterns>`: Exclude specific files or directories (comma-separated glob patterns)
- `--respect-gitignore`: Respect .gitignore files when scanning directories
- `-v, --verbose`: Show detailed output

## Configuration

rumdl can be configured using a TOML configuration file. By default, it looks for `rumdl.toml` or `.rumdl.toml` in the current directory.

You can create a default configuration file using the `init` command:

```bash
rumdl init
```

This will create a `.rumdl.toml` file in the current directory with default settings that you can customize.

Example configuration file:

```toml
# Global configuration options
[global]
# List of rules to disable
disable = ["MD013", "MD033"]

# List of rules to enable exclusively (if provided, only these rules will run)
# enable = ["MD001", "MD003", "MD004"]

# List of file/directory patterns to exclude from linting
exclude = [
    # Common directories to exclude
    ".git",
    ".github",
    "node_modules",
    "vendor",
    "dist",
    "build",
    
    # Specific files or patterns
    "CHANGELOG.md",
    "LICENSE.md",
    "generated/*.md",
    "**/temp_*.md",
]

# Whether to respect .gitignore files when scanning directories
respect_gitignore = false

# Rule-specific configurations
[MD002]
level = 1  # Expected level for first heading

[MD003]
style = "atx"  # Heading style (atx, atx_closed, setext)
```

## Output Style

rumdl produces clean, colorized output similar to modern linting tools:

```text
README.md:12:1: [MD022] Headings should be surrounded by blank lines [*]
README.md:24:5: [MD037] Spaces inside emphasis markers: "* incorrect *" [*]
README.md:31:76: [MD013] Line length exceeds 80 characters
README.md:42:3: [MD010] Hard tabs found, use spaces instead [*]
```

When running with `--fix`, rumdl shows which issues were fixed:

```text
README.md:12:1: [MD022] Headings should be surrounded by blank lines [fixed]
README.md:24:5: [MD037] Spaces inside emphasis markers: "* incorrect *" [fixed]
README.md:42:3: [MD010] Hard tabs found, use spaces instead [fixed]

Fixed 3 issues in 1 file
```

For a more detailed view, use the `--verbose` option:

```text
✓ No issues found in CONTRIBUTING.md
README.md:12:1: [MD022] Headings should be surrounded by blank lines [*]
README.md:24:5: [MD037] Spaces inside emphasis markers: "* incorrect *" [*]
README.md:42:3: [MD010] Hard tabs found, use spaces instead [*]

Found 3 issues in 1 file (2 files checked)
Run with `--fix` to automatically fix issues
```

### Output Format

rumdl uses a consistent output format for all issues:

```text
{file}:{line}:{column}: [{rule*id}] {message} [{fix*indicator}]
```

The output is colorized by default:

- Filenames appear in blue and underlined
- Line and column numbers appear in cyan
- Rule IDs appear in yellow
- Error messages appear in white
- Fixable issues are marked with `[*]` in green
- Fixed issues are marked with `[fixed]` in green

## Rules

rumdl implements over 50 lint rules for Markdown files. By default, all rules are enabled except those specifically disabled in your configuration.

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
| MD054   | Link and image style | ✅ | ❌ |
| MD055   | Table pipe style | ✅ | ✅ |
| MD056   | Table column count | ✅ | ✅ |
| MD058   | Tables should be surrounded by blank lines | ✅ | ✅ |

## Development

### Prerequisites

- Rust 1.70 or higher
- Make (for development commands)

### Building

```bash
make build
```

### Testing

```bash
make test
```

## License

MIT License