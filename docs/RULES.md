# rumdl Rules Reference

## A comprehensive reference of all Markdown linting rules

## Introduction

rumdl implements 67 rules for checking Markdown files. This document provides a comprehensive reference of all available rules, organized by category.
Each rule has a brief description and a link to its detailed documentation.

For information on global configuration settings (file selection, rule enablement, etc.), see the [Global Settings Reference](global-settings.md).

For flavor-specific behavior (MkDocs, MDX, Quarto), see the [Markdown Flavors Reference](flavors.md).

## Rule Categories

- [Severity Levels](#severity-levels) - Understanding Error vs Warning severities
- [Heading Rules](#heading-rules) - Rules related to heading structure and formatting
- [List Rules](#list-rules) - Rules for list formatting and structure
- [Whitespace Rules](#whitespace-rules) - Rules for spacing, indentation, and line length
- [Formatting Rules](#formatting-rules) - Rules for general Markdown formatting
- [Code Block Rules](#code-block-rules) - Rules specific to code blocks and fences
- [Link and Image Rules](#link-and-image-rules) - Rules for links, references, and images
- [Table Rules](#table-rules) - Rules for table formatting and structure
- [Footnote Rules](#footnote-rules) - Rules for footnote validation and formatting
- [Frontmatter Rules](#frontmatter-rules) - Rules for YAML/TOML/JSON frontmatter
- [Other Rules](#other-rules) - Miscellaneous rules that don't fit the other categories
- [Opt-in Rules](#opt-in-rules) - Rules disabled by default

## Opt-in Rules

The following rules are **disabled by default** because they enforce opinionated style choices that may not suit all projects. Enable them explicitly if your project requires these checks.

| Rule              | Description            | Why opt-in                                               |
| ----------------- | ---------------------- | -------------------------------------------------------- |
| [MD060](md060.md) | Table formatting       | Makes significant formatting changes to existing tables  |
| [MD063](md063.md) | Heading capitalization | Style varies by guide (AP, Chicago, APA)                 |
| [MD072](md072.md) | Frontmatter key sort   | Many projects prefer semantic ordering over alphabetical |
| [MD073](md073.md) | TOC validation         | Requires specific TOC markers in document                |

### Enabling Opt-in Rules

**.rumdl.toml:**

```toml
[MD060]
enabled = true

[MD063]
enabled = true
style = "title_case"  # Optional: configure the style

[MD072]
enabled = true
```

**pyproject.toml:**

```toml
[tool.rumdl.MD060]
enabled = true

[tool.rumdl.MD063]
enabled = true
style = "title_case"

[tool.rumdl.MD072]
enabled = true
```

See each rule's documentation for available configuration options.

## Note on Missing Rule Numbers

Some rule numbers are not implemented in rumdl:

- **MD002** - Deprecated and removed from markdownlint v0.13.0 (replaced by MD041); removed from rumdl for compatibility
- **MD006** - Not implemented in DavidAnson/markdownlint (JavaScript version); removed from rumdl for compatibility
- **MD008** - Originally intended for "Unordered list spacing" but not implemented in modern markdownlint
- **MD015, MD016, MD017** - These rule numbers were never assigned in either the Ruby or Node.js versions of markdownlint

These gaps in numbering are maintained for compatibility with markdownlint rule numbering.

## Severity Levels

Rules are categorized into three severity levels based on their impact on document functionality:

### Error Severity

Rules with Error severity flag issues that break document functionality:

- **MD001** - Broken heading hierarchy prevents screen reader navigation
- **MD011** - Reversed link syntax makes links non-functional
- **MD024** - Duplicate headings create ID collisions, anchors point to wrong sections
- **MD025** - Multiple H1 elements break document outline structure
- **MD042** - Empty links have no destination and don't work
- **MD045** - Missing alt text violates WCAG accessibility requirements
- **MD051** - Invalid link fragments point to non-existent sections
- **MD057** - Links to non-existent files are broken
- **MD066** - Undefined footnote references are broken
- **MD068** - Empty footnote definitions have no content

### Warning Severity

Most rules use Warning severity by default. These flag style, formatting, and convention issues that don't break document functionality but affect readability, consistency, or best practices.

### Info Severity

Info severity is available for rules you want to track but not treat as warnings. Useful for:

- Style issues that automatic formatting will fix
- Low-priority suggestions
- Rules you're gradually adopting

### Configuring Severity

You can override default severities for any rule in your configuration file:

**.rumdl.toml:**

```toml
[MD013]
severity = "info"     # Downgrade to info (formatting will fix this)

[MD004]
severity = "error"    # Upgrade from warning to error
```

**pyproject.toml:**

```toml
[tool.rumdl.MD013]
severity = "info"

[tool.rumdl.MD004]
severity = "error"
```

Valid severity values: `"error"`, `"warning"`, `"info"` (case-insensitive)

Severity affects:

- Exit codes: Use `--fail-on` to control which severities cause exit code 1
- Output formatting: Different severities are visually distinct in console output
- LSP: Error → Error, Warning → Warning, Info → Information in your editor
- CI/CD: severity controls whether linting failures block builds

## Heading Rules

| Rule ID           | Rule Name                 | Description                                               |
| ----------------- | ------------------------- | --------------------------------------------------------- |
| [MD001](md001.md) | Heading increment         | Headings should only increment by one level at a time     |
| [MD003](md003.md) | Heading style             | Heading style should be consistent                        |
| [MD018](md018.md) | No space atx              | No space after hash on atx style heading                  |
| [MD019](md019.md) | Multiple space atx        | Multiple spaces after hash on atx style heading           |
| [MD020](md020.md) | No space closed atx       | No space inside hashes on closed atx style heading        |
| [MD021](md021.md) | Multiple space closed atx | Multiple spaces inside hashes on closed atx style heading |
| [MD022](md022.md) | Blanks around headings    | Headings should be surrounded by blank lines              |
| [MD023](md023.md) | Heading start left        | Headings must start at the beginning of the line          |
| [MD024](md024.md) | Multiple headings         | Multiple headings with the same content                   |
| [MD025](md025.md) | Single title              | Multiple top-level headings in the same document          |
| [MD036](md036.md) | No emphasis as heading    | Emphasis used instead of a heading                        |
| [MD041](md041.md) | First line h1             | First line in a file should be a top-level heading        |
| [MD043](md043.md) | Required headings         | Required heading structure                                |
| [MD063](md063.md) | Heading capitalization    | Heading text capitalization style                         |

## List Rules

| Rule ID           | Rule Name                 | Description                                               |
| ----------------- | ------------------------- | --------------------------------------------------------- |
| [MD004](md004.md) | UL style                  | Unordered list style                                      |
| [MD005](md005.md) | List indent               | Inconsistent indentation for list items at the same level |
| [MD007](md007.md) | UL indent                 | Unordered list indentation                                |
| [MD029](md029.md) | OL prefix                 | Ordered list item prefix                                  |
| [MD030](md030.md) | List marker space         | Spaces after list markers                                 |
| [MD032](md032.md) | Blanks around lists       | Lists should be surrounded by blank lines                 |
| [MD069](md069.md) | No duplicate list markers | Duplicate markers like `- - text` from copy-paste         |

## Whitespace Rules

| Rule ID           | Rule Name                      | Description                                            |
| ----------------- | ------------------------------ | ------------------------------------------------------ |
| [MD009](md009.md) | No trailing spaces             | No trailing spaces                                     |
| [MD010](md010.md) | No hard tabs                   | No hard tabs                                           |
| [MD012](md012.md) | No multiple blanks             | No multiple consecutive blank lines                    |
| [MD013](md013.md) | Line length                    | Line length                                            |
| [MD027](md027.md) | Multiple spaces blockquote     | Multiple spaces after blockquote symbol                |
| [MD028](md028.md) | Blanks blockquote              | Blank line inside blockquote                           |
| [MD031](md031.md) | Blanks around fences           | Fenced code blocks should be surrounded by blank lines |
| [MD047](md047.md) | File end newline               | Files should end with a single newline character       |
| [MD064](md064.md) | No multiple consecutive spaces | Multiple consecutive spaces in content                 |

## Formatting Rules

| Rule ID           | Rule Name               | Description                                        |
| ----------------- | ----------------------- | -------------------------------------------------- |
| [MD026](md026.md) | No trailing punctuation | Trailing punctuation in heading                    |
| [MD033](md033.md) | No inline HTML          | Inline HTML                                        |
| [MD035](md035.md) | HR style                | Horizontal rule style                              |
| [MD037](md037.md) | Spaces around emphasis  | Spaces inside emphasis markers                     |
| [MD038](md038.md) | No space in code        | Spaces inside code span elements                   |
| [MD039](md039.md) | No space in links       | Spaces inside link text                            |
| [MD044](md044.md) | Proper names            | Proper names should have consistent capitalization |
| [MD049](md049.md) | Emphasis style          | Emphasis style should be consistent                |
| [MD050](md050.md) | Strong style            | Strong style should be consistent                  |

## Code Block Rules

| Rule ID           | Rule Name            | Description                                         |
| ----------------- | -------------------- | --------------------------------------------------- |
| [MD014](md014.md) | Commands show output | Code blocks should show output when appropriate     |
| [MD040](md040.md) | Fenced code language | Fenced code blocks should have a language specified |
| [MD046](md046.md) | Code block style     | Code block style                                    |
| [MD048](md048.md) | Code fence style     | Code fence style                                    |
| [MD070](md070.md) | Nested code fence    | Nested fence collision detection                    |

## Link and Image Rules

| Rule ID           | Rule Name              | Description                                           |
| ----------------- | ---------------------- | ----------------------------------------------------- |
| [MD011](md011.md) | Reversed link          | Reversed link syntax                                  |
| [MD034](md034.md) | No bare URLs           | Bare URL used                                         |
| [MD042](md042.md) | No empty links         | No empty links                                        |
| [MD045](md045.md) | No alt text            | Images should have alternate text                     |
| [MD051](md051.md) | Link fragments         | Link fragments should be valid heading IDs            |
| [MD052](md052.md) | Reference links images | References should be defined                          |
| [MD053](md053.md) | Link image definitions | Link and image reference definitions should be needed |
| [MD054](md054.md) | Link image style       | Link and image style                                  |
| [MD059](md059.md) | Link text              | Link text should be descriptive                       |

## Table Rules

| Rule ID           | Rule Name          | Description                                |
| ----------------- | ------------------ | ------------------------------------------ |
| [MD055](md055.md) | Table pipe style   | Table pipe style should be consistent      |
| [MD056](md056.md) | Table column count | Table column count should be consistent    |
| [MD058](md058.md) | Table spacing      | Tables should be surrounded by blank lines |

## Footnote Rules

| Rule ID           | Rule Name                  | Description                                                |
| ----------------- | -------------------------- | ---------------------------------------------------------- |
| [MD066](md066.md) | Footnote validation        | Footnote references should have definitions and vice versa |
| [MD067](md067.md) | Footnote definition order  | Footnote definitions should appear in order of reference   |
| [MD068](md068.md) | Empty footnote definitions | Footnote definitions should not be empty                   |

## Frontmatter Rules

| Rule ID           | Rule Name                    | Description                                        |
| ----------------- | ---------------------------- | -------------------------------------------------- |
| [MD071](md071.md) | Blank line after frontmatter | Frontmatter should be followed by a blank line     |
| [MD072](md072.md) | Frontmatter key sort         | Frontmatter keys should be sorted (YAML/TOML/JSON) |

## Other Rules

| Rule ID           | Rule Name              | Description                             |
| ----------------- | ---------------------- | --------------------------------------- |
| [MD057](md057.md) | Relative links         | Relative links should exist             |
| [MD060](md060.md) | Table format           | Table formatting should be consistent   |
| [MD061](md061.md) | Forbidden terms        | Certain terms should not be used        |
| [MD062](md062.md) | Link destination space | No whitespace in link destinations      |
| [MD073](md073.md) | TOC validation         | Table of Contents should match headings |

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

- **error**: Critical issues (broken links, accessibility violations)
- **warning**: Style and convention issues (default for most rules)
- **info**: Low-priority suggestions or issues that formatting will fix

You can customize rule severities in your configuration file:

```toml
[MD013]
severity = "info"  # Downgrade to info (formatting will fix this)
```

Use `--fail-on` to control which severities cause exit code 1:

- `--fail-on any` (default): Exit 1 on any violation
- `--fail-on warning`: Exit 1 on warning or error only
- `--fail-on error`: Exit 1 only on errors
- `--fail-on never`: Always exit 0

## Configuration

You can configure rumdl using a TOML configuration file. Create a default configuration file using:

```bash
rumdl init
```

This generates a `.rumdl.toml` file with default settings that you can customize.
