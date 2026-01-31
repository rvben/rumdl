//! Comprehensive regression tests for MkDocs flavor extension support
//!
//! These tests verify that rumdl correctly handles the syntax from:
//! - Python-Markdown extensions
//! - PyMdown Extensions
//! - mkdocstrings
//!
//! Test categories:
//! 1. Basic recognition - Extensions don't produce false positives
//! 2. Edge cases - Malformed syntax, boundary conditions, unusual nesting
//! 3. Negative tests - Violations ARE still detected in extension contexts
//! 4. Rule interactions - Specific rules that might conflict with extensions
//! 5. Fix preservation - Auto-fix doesn't break extension syntax
//! 6. Stress tests - Complex documents with multiple nested extensions

use rumdl_lib::config::{Config, MarkdownFlavor};
use rumdl_lib::lint;
use rumdl_lib::lint_context::LintContext;
use rumdl_lib::rule::Rule;
use rumdl_lib::rules::*;

fn create_mkdocs_config() -> Config {
    let mut config = Config::default();
    config.global.flavor = MarkdownFlavor::MkDocs;
    config
}

fn lint_mkdocs(content: &str) -> Vec<rumdl_lib::rule::LintWarning> {
    let config = create_mkdocs_config();
    let rules = all_rules(&config);
    lint(content, &rules, false, MarkdownFlavor::MkDocs, None).unwrap()
}

fn lint_standard(content: &str) -> Vec<rumdl_lib::rule::LintWarning> {
    let config = Config::default();
    let rules = all_rules(&config);
    lint(content, &rules, false, MarkdownFlavor::Standard, None).unwrap()
}

// =============================================================================
// PART 1: BASIC EXTENSION RECOGNITION
// These tests verify that each extension syntax is recognized without false positives
// =============================================================================

mod basic_recognition {
    use super::*;

    #[test]
    fn test_admonitions_all_types() {
        // Test all admonition types recognized by Material for MkDocs
        let content = r#"# Admonitions

!!! note
    Note content.

!!! abstract
    Abstract content.

!!! info
    Info content.

!!! tip
    Tip content.

!!! success
    Success content.

!!! question
    Question content.

!!! warning
    Warning content.

!!! failure
    Failure content.

!!! danger
    Danger content.

!!! bug
    Bug content.

!!! example
    Example content.

!!! quote
    Quote content.
"#;
        let warnings = lint_mkdocs(content);
        assert!(
            warnings.is_empty(),
            "All admonition types should be recognized: {warnings:?}"
        );
    }

    #[test]
    fn test_collapsible_admonitions() {
        let content = r#"# Collapsible

??? note "Collapsed"
    Hidden by default.

???+ warning "Expanded"
    Visible by default.

??? abstract
    No title, collapsed.

???+ tip
    No title, expanded.
"#;
        let warnings = lint_mkdocs(content);
        assert!(warnings.is_empty(), "Collapsible admonitions should work: {warnings:?}");
    }

    #[test]
    fn test_content_tabs_variations() {
        let content = r#"# Tabs

=== "Tab 1"
    Content 1.

=== "Tab 2"
    Content 2.

=== "Tab with 'quotes'"
    Quoted title.

=== "Tab with \"double quotes\""
    Double quoted.
"#;
        let warnings = lint_mkdocs(content);
        assert!(warnings.is_empty(), "Content tabs should work: {warnings:?}");
    }

    #[test]
    fn test_mkdocstrings_variations() {
        let content = r#"# API Docs

::: module
    handler: python

::: package.module.Class
    options:
        show_source: true
        heading_level: 2

::: function

:::module.without.space
"#;
        let warnings = lint_mkdocs(content);
        assert!(warnings.is_empty(), "mkdocstrings blocks should work: {warnings:?}");
    }

    #[test]
    fn test_pymdown_inline_extensions() {
        let content = r#"# Inline Extensions

Keys: ++ctrl+c++ and ++ctrl+alt+del++

Caret: ^superscript^ and ^^insert^^

Tilde: ~subscript~ and ~~strikethrough~~

Mark: ==highlighted==

Critic: {++added++} {--deleted--} {~~old~>new~~} {==marked==} {>>comment<<}
"#;
        let warnings = lint_mkdocs(content);
        assert!(
            warnings.is_empty(),
            "PyMdown inline extensions should work: {warnings:?}"
        );
    }

    #[test]
    fn test_snippets_syntax() {
        let content = r#"# Snippets

--8<-- "file.md"

--8<-- "path/to/file.py"

--8<-- "file.md:10:20"

;--8<--
"#;
        let warnings = lint_mkdocs(content);
        assert!(warnings.is_empty(), "Snippets should work: {warnings:?}");
    }

    #[test]
    fn test_abbreviations() {
        let content = r#"# Abbreviations

The HTML and CSS specifications.

*[HTML]: Hypertext Markup Language
*[CSS]: Cascading Style Sheets
"#;
        let warnings = lint_mkdocs(content);
        assert!(warnings.is_empty(), "Abbreviations should work: {warnings:?}");
    }

    #[test]
    fn test_definition_lists() {
        let content = r#"# Definitions

Term 1
:   Definition 1

Term 2
:   Definition 2a
:   Definition 2b

Term 3
:   Multi-line
    definition.
"#;
        let warnings = lint_mkdocs(content);
        assert!(warnings.is_empty(), "Definition lists should work: {warnings:?}");
    }

    #[test]
    fn test_footnotes() {
        let content = r#"# Footnotes

Text with footnote.[^1] Another.[^named]

[^1]: Simple footnote.

[^named]:
    Multi-paragraph
    footnote content.

    Second paragraph.
"#;
        let warnings = lint_mkdocs(content);
        assert!(warnings.is_empty(), "Footnotes should work: {warnings:?}");
    }

    #[test]
    fn test_attribute_lists() {
        let content = r#"# Attributes

## Heading {#custom-id}

## Another {.class-name}

## Multiple {#id .class data-value="test"}

Paragraph.
{.centered}
"#;
        let warnings = lint_mkdocs(content);
        assert!(warnings.is_empty(), "Attribute lists should work: {warnings:?}");
    }

    #[test]
    fn test_superfences_with_attributes() {
        let content = r#"# Code Blocks

```python title="example.py"
print("hello")
```

```javascript linenums="1" hl_lines="2-3"
const a = 1;
const b = 2;
const c = 3;
```

```mermaid
graph TD
    A --> B
```
"#;
        let warnings = lint_mkdocs(content);
        assert!(warnings.is_empty(), "Superfences should work: {warnings:?}");
    }

    #[test]
    fn test_math_blocks() {
        let content = r#"# Math

Inline: $E = mc^2$ and $\int_0^1 x dx$

Block:

$$
\frac{n!}{k!(n-k)!} = \binom{n}{k}
$$

$$
\sum_{i=1}^{n} i = \frac{n(n+1)}{2}
$$
"#;
        let warnings = lint_mkdocs(content);
        assert!(warnings.is_empty(), "Math blocks should work: {warnings:?}");
    }

    #[test]
    fn test_emoji_shortcodes() {
        let content = r#"# Emoji

Material: :material-check: :material-close: :material-github:

FontAwesome: :fontawesome-brands-github: :fontawesome-solid-heart:

Octicons: :octicons-mark-github-16: :octicons-alert-24:

Twemoji: :smile: :heart: :rocket:
"#;
        let warnings = lint_mkdocs(content);
        assert!(warnings.is_empty(), "Emoji shortcodes should work: {warnings:?}");
    }

    #[test]
    fn test_inline_code_highlighting() {
        let content = r#"# InlineHilite

Use `#!python print("hello")` for Python.

Or `#!javascript console.log("hi")` for JS.

Generic: `#!bash echo $PATH`
"#;
        let warnings = lint_mkdocs(content);
        assert!(warnings.is_empty(), "InlineHilite should work: {warnings:?}");
    }

    #[test]
    fn test_md_in_html() {
        let content = r#"# MD in HTML

<div markdown="1">

**Bold** and *italic* work here.

- List item
- Another

</div>

<div markdown="block">
More content.
</div>
"#;
        let warnings = lint_mkdocs(content);
        // Should not flag the div tags in MkDocs flavor
        let md033 = warnings
            .iter()
            .filter(|w| w.rule_name.as_deref() == Some("MD033"))
            .count();
        assert_eq!(md033, 0, "md_in_html should not trigger MD033");
    }

    #[test]
    fn test_toc_marker() {
        let content = r#"# Document

[TOC]

## Section 1

## Section 2
"#;
        let warnings = lint_mkdocs(content);
        assert!(warnings.is_empty(), "TOC marker should work: {warnings:?}");
    }

    #[test]
    fn test_tasklists() {
        let content = r#"# Tasks

- [x] Completed
- [ ] Incomplete
- [X] Also completed
"#;
        let warnings = lint_mkdocs(content);
        assert!(warnings.is_empty(), "Tasklists should work: {warnings:?}");
    }

    #[test]
    fn test_smartsymbols() {
        let content = r#"# Symbols

Copyright (c) and trademark (tm) and registered (r).

Arrows: --> <-- <-->

Fractions: 1/4 1/2 3/4

Dashes: -- and ---
"#;
        let warnings = lint_mkdocs(content);
        assert!(warnings.is_empty(), "SmartSymbols should work: {warnings:?}");
    }
}

// =============================================================================
// PART 2: EDGE CASES
// Test malformed syntax, boundary conditions, and unusual nesting
// =============================================================================

mod edge_cases {
    use super::*;

    #[test]
    fn test_admonition_without_content() {
        let content = r#"# Empty Admonitions

!!! note

!!! warning "Title Only"

Text after.
"#;
        let warnings = lint_mkdocs(content);
        // Should handle gracefully, even if unusual
        assert!(warnings.is_empty(), "Empty admonitions should be handled: {warnings:?}");
    }

    #[test]
    fn test_admonition_single_line() {
        let content = r#"# Single Line

!!! note "Title" Content on same line is not standard but should not crash.

Regular text.
"#;
        // This is non-standard but should not cause false positives
        let _ = lint_mkdocs(content);
    }

    #[test]
    fn test_deeply_nested_admonitions() {
        let content = r#"# Deep Nesting

!!! note "Level 1"
    Content level 1.

    !!! warning "Level 2"
        Content level 2.

        !!! danger "Level 3"
            Content level 3.

            !!! tip "Level 4"
                Maximum reasonable nesting.
"#;
        let warnings = lint_mkdocs(content);
        assert!(
            warnings.is_empty(),
            "Deeply nested admonitions should work: {warnings:?}"
        );
    }

    #[test]
    fn test_tabs_with_code_blocks() {
        let content = r#"# Tabs with Code

=== "Python"

    ```python
    def hello():
        print("Hello")
    ```

=== "Rust"

    ```rust
    fn main() {
        println!("Hello");
    }
    ```
"#;
        let warnings = lint_mkdocs(content);
        assert!(warnings.is_empty(), "Tabs with code should work: {warnings:?}");
    }

    #[test]
    fn test_tabs_inside_admonition() {
        let content = r#"# Nested Tabs

!!! example "Code Examples"

    === "Python"

        ```python
        print("hello")
        ```

    === "JavaScript"

        ```javascript
        console.log("hello");
        ```
"#;
        let warnings = lint_mkdocs(content);
        assert!(warnings.is_empty(), "Tabs inside admonitions should work: {warnings:?}");
    }

    #[test]
    fn test_mkdocstrings_with_complex_paths() {
        let content = r#"# Complex Paths

::: package.subpackage.module.Class.method

::: _private_module._PrivateClass

::: module.Class.__init__

::: package.module.CONSTANT
"#;
        let warnings = lint_mkdocs(content);
        assert!(
            warnings.is_empty(),
            "Complex mkdocstrings paths should work: {warnings:?}"
        );
    }

    #[test]
    fn test_keys_with_special_characters() {
        let content = r#"# Special Keys

Simple: ++enter++ ++escape++ ++space++

Modifiers: ++ctrl+shift+alt+del++

Function keys: ++f1++ ++f12++

Arrows: ++arrow-up++ ++arrow-down++ ++arrow-left++ ++arrow-right++

Numpad: ++num0++ ++num-lock++
"#;
        let warnings = lint_mkdocs(content);
        assert!(warnings.is_empty(), "Special keys should work: {warnings:?}");
    }

    #[test]
    fn test_inline_extensions_adjacent() {
        // Multiple inline extensions next to each other
        let content = r#"# Adjacent

Text ==highlight==^super^~sub~**bold** end.

Keys ++ctrl+c++++ctrl+v++ adjacent.

Mixed {++add++}{--del--} together.
"#;
        let warnings = lint_mkdocs(content);
        assert!(
            warnings.is_empty(),
            "Adjacent inline extensions should work: {warnings:?}"
        );
    }

    #[test]
    fn test_inline_extensions_in_emphasis() {
        // Use inline emphasis within paragraphs to avoid MD036
        let content = r#"# In Emphasis

This is **bold with ==highlight== inside** text.

This is *italic with ^super^ inside* text.

This is ***bold italic with ~sub~ inside*** text.
"#;
        let warnings = lint_mkdocs(content);
        assert!(warnings.is_empty(), "Extensions in emphasis should work: {warnings:?}");
    }

    #[test]
    fn test_math_with_special_characters() {
        let content = r#"# Complex Math

Inline: $\alpha + \beta = \gamma$ and $x_{i,j}^{2}$

Block with alignment:

$$
\begin{aligned}
a &= b + c \\
d &= e + f
\end{aligned}
$$

Matrices:

$$
\begin{pmatrix}
a & b \\
c & d
\end{pmatrix}
$$
"#;
        let warnings = lint_mkdocs(content);
        assert!(warnings.is_empty(), "Complex math should work: {warnings:?}");
    }

    #[test]
    fn test_footnotes_complex_content() {
        let content = r#"# Complex Footnotes

Text[^complex] here.

[^complex]:
    This footnote has:

    - A list
    - With items

    ```python
    # And code
    print("hello")
    ```

    And more text.
"#;
        let warnings = lint_mkdocs(content);
        assert!(warnings.is_empty(), "Complex footnotes should work: {warnings:?}");
    }

    #[test]
    fn test_abbreviation_with_special_chars() {
        let content = r#"# Special Abbreviations

Using HTML5 and CSS3 and ES6+ features.

*[HTML5]: Hypertext Markup Language version 5
*[CSS3]: Cascading Style Sheets version 3
*[ES6+]: ECMAScript 6 and later
"#;
        let warnings = lint_mkdocs(content);
        assert!(
            warnings.is_empty(),
            "Abbreviations with special chars should work: {warnings:?}"
        );
    }

    #[test]
    fn test_snippet_paths_with_special_chars() {
        let content = r#"# Special Paths

--8<-- "path/to/file-name.md"

--8<-- "path/to/file_name.py"

--8<-- "../relative/path.txt"

--8<-- "./same-dir/file.md"
"#;
        let warnings = lint_mkdocs(content);
        assert!(warnings.is_empty(), "Snippet paths should work: {warnings:?}");
    }

    #[test]
    fn test_attribute_list_complex() {
        let content = r#"# Complex Attributes

## Heading {#my-id .class1 .class2 data-foo="bar" data-baz='qux'}

Paragraph with many attributes.
{#para-id .styled .centered style="color: red" data-toggle="tooltip"}
"#;
        let warnings = lint_mkdocs(content);
        assert!(warnings.is_empty(), "Complex attributes should work: {warnings:?}");
    }

    #[test]
    fn test_definition_list_with_markdown() {
        let content = r#"# Rich Definitions

Term with **bold**
:   Definition with *italic* and `code`.

    Second paragraph with [link](url).

Another Term
:   - List item 1
    - List item 2
"#;
        let warnings = lint_mkdocs(content);
        assert!(warnings.is_empty(), "Rich definitions should work: {warnings:?}");
    }

    #[test]
    fn test_critic_markup_edge_cases() {
        let content = r#"# Critic Edge Cases

Empty: {++++} {----}

Nested braces: {++text with {braces}++}

Multi-word: {++multiple words added here++}

With punctuation: {--removed, with punctuation!--}

Complex substitution: {~~old text with *emphasis*~>new text with **bold**~~}
"#;
        let warnings = lint_mkdocs(content);
        assert!(warnings.is_empty(), "Critic edge cases should work: {warnings:?}");
    }

    #[test]
    fn test_unclosed_inline_extensions() {
        // Malformed syntax should not crash
        let content = r#"# Unclosed

Unclosed key ++ctrl

Unclosed mark ==highlight

Unclosed caret ^super

Regular text after.
"#;
        // Should not panic, may or may not produce warnings
        let _ = lint_mkdocs(content);
    }

    #[test]
    fn test_empty_content() {
        let content = "";
        let warnings = lint_mkdocs(content);
        assert!(warnings.is_empty(), "Empty content should work: {warnings:?}");
    }

    #[test]
    fn test_only_extension_markers() {
        let content = r#"!!! note
    Only admonition.
"#;
        let warnings = lint_mkdocs(content);
        // May trigger MD041 (first line should be heading) but extension should be recognized
        let non_md041 = warnings
            .iter()
            .filter(|w| w.rule_name.as_deref() != Some("MD041"))
            .count();
        assert_eq!(
            non_md041, 0,
            "Only extension markers should work except MD041: {warnings:?}"
        );
    }
}

// =============================================================================
// PART 3: NEGATIVE TESTS
// Verify that actual violations ARE still detected in extension contexts
// =============================================================================

mod negative_tests {
    use super::*;

    #[test]
    fn test_violations_in_admonition_content() {
        // Test that regular markdown violations are still detected in admonitions
        // Use multiple blank lines which MD012 should detect
        let content = "# Test\n\n!!! note\n    Content here.\n\n\n    More content.\n";
        let warnings = lint_mkdocs(content);
        // MD012 should still detect multiple blank lines
        let md012 = warnings
            .iter()
            .filter(|w| w.rule_name.as_deref() == Some("MD012"))
            .count();
        assert!(
            md012 > 0,
            "MD012 should detect multiple blanks in admonition: {warnings:?}"
        );
    }

    #[test]
    fn test_trailing_spaces_in_admonition() {
        let content = "# Test\n\n!!! note\n    Line with trailing spaces   \n";
        let warnings = lint_mkdocs(content);
        let md009 = warnings
            .iter()
            .filter(|w| w.rule_name.as_deref() == Some("MD009"))
            .count();
        assert!(
            md009 > 0,
            "MD009 should detect trailing spaces in admonition: {warnings:?}"
        );
    }

    #[test]
    fn test_long_lines_detected() {
        let content = "# Test\n\nThis is a very long line that exceeds the default 80 character limit and should definitely trigger MD013 in the linter.\n";
        let warnings = lint_mkdocs(content);
        let md013 = warnings
            .iter()
            .filter(|w| w.rule_name.as_deref() == Some("MD013"))
            .count();
        assert!(md013 > 0, "MD013 should detect long lines: {warnings:?}");
    }

    #[test]
    fn test_multiple_blank_lines_detected() {
        let content = "# Test\n\n\n\nText after multiple blanks.\n";
        let warnings = lint_mkdocs(content);
        let md012 = warnings
            .iter()
            .filter(|w| w.rule_name.as_deref() == Some("MD012"))
            .count();
        assert!(md012 > 0, "MD012 should detect multiple blank lines: {warnings:?}");
    }

    #[test]
    fn test_heading_increment_detected() {
        let content = "# H1\n\n### H3 skipping H2\n";
        let warnings = lint_mkdocs(content);
        let md001 = warnings
            .iter()
            .filter(|w| w.rule_name.as_deref() == Some("MD001"))
            .count();
        assert!(md001 > 0, "MD001 should detect heading increment: {warnings:?}");
    }

    #[test]
    fn test_bare_url_detected() {
        let content = "# Test\n\nVisit https://example.com for more.\n";
        let warnings = lint_mkdocs(content);
        let md034 = warnings
            .iter()
            .filter(|w| w.rule_name.as_deref() == Some("MD034"))
            .count();
        assert!(md034 > 0, "MD034 should detect bare URLs: {warnings:?}");
    }

    #[test]
    fn test_hard_tabs_detected() {
        let content = "# Test\n\n\tIndented with tab.\n";
        let warnings = lint_mkdocs(content);
        let md010 = warnings
            .iter()
            .filter(|w| w.rule_name.as_deref() == Some("MD010"))
            .count();
        assert!(md010 > 0, "MD010 should detect hard tabs: {warnings:?}");
    }

    #[test]
    fn test_emphasis_used_as_heading_detected() {
        let content = "# Test\n\n**This looks like a heading**\n\nBut it's just bold text.\n";
        let warnings = lint_mkdocs(content);
        let md036 = warnings
            .iter()
            .filter(|w| w.rule_name.as_deref() == Some("MD036"))
            .count();
        assert!(md036 > 0, "MD036 should detect emphasis as heading: {warnings:?}");
    }

    #[test]
    fn test_standard_flavor_flags_mkdocs_syntax() {
        // In standard flavor, MkDocs-specific syntax should be flagged
        let content = r#"# Test

!!! note
    This is not an admonition in standard markdown.
"#;
        let mkdocs_warnings = lint_mkdocs(content);
        let standard_warnings = lint_standard(content);

        // MkDocs should have fewer or different warnings
        // The exact behavior depends on what rules flag the !!! syntax
        // At minimum, standard should not treat it as a special block
        assert!(
            mkdocs_warnings.len() <= standard_warnings.len()
                || mkdocs_warnings.iter().map(|w| &w.rule_name).collect::<Vec<_>>()
                    != standard_warnings.iter().map(|w| &w.rule_name).collect::<Vec<_>>(),
            "Standard and MkDocs should handle admonitions differently"
        );
    }

    #[test]
    fn test_unreferenced_footnote_detected() {
        let content = "# Test\n\n[^orphan]: This footnote is never used.\n";
        let warnings = lint_mkdocs(content);
        let md066 = warnings
            .iter()
            .filter(|w| w.rule_name.as_deref() == Some("MD066"))
            .count();
        assert!(md066 > 0, "MD066 should detect unreferenced footnotes: {warnings:?}");
    }

    #[test]
    fn test_undefined_reference_detected() {
        let content = "# Test\n\nSee [undefined reference][nowhere] here.\n";
        let warnings = lint_mkdocs(content);
        let md052 = warnings
            .iter()
            .filter(|w| w.rule_name.as_deref() == Some("MD052"))
            .count();
        assert!(md052 > 0, "MD052 should detect undefined references: {warnings:?}");
    }
}

// =============================================================================
// PART 4: RULE INTERACTION TESTS
// Test specific rules that might conflict with extension syntax
// =============================================================================

mod rule_interactions {
    use super::*;

    #[test]
    fn test_md031_blanks_around_admonitions() {
        // MD031 should recognize admonitions as fence-like blocks
        let content = r#"# Test

Text before.

!!! note
    Admonition content.

Text after.
"#;
        let warnings = lint_mkdocs(content);
        let md031 = warnings
            .iter()
            .filter(|w| w.rule_name.as_deref() == Some("MD031"))
            .count();
        assert_eq!(
            md031, 0,
            "MD031 should not flag properly spaced admonitions: {warnings:?}"
        );
    }

    #[test]
    fn test_md038_with_inlinehilite() {
        // MD038 checks for spaces in code spans
        // InlineHilite uses `#!lang code` which should not be flagged
        let content = "# Test\n\nUse `#!python print()` here.\n";
        let warnings = lint_mkdocs(content);
        let md038 = warnings
            .iter()
            .filter(|w| w.rule_name.as_deref() == Some("MD038"))
            .count();
        assert_eq!(md038, 0, "MD038 should not flag InlineHilite: {warnings:?}");
    }

    #[test]
    fn test_md040_with_superfences() {
        // MD040 requires language on fenced code blocks
        // Superfences with mermaid/other should be recognized
        let content = "# Test\n\n```mermaid\ngraph TD\n    A --> B\n```\n";
        let warnings = lint_mkdocs(content);
        let md040 = warnings
            .iter()
            .filter(|w| w.rule_name.as_deref() == Some("MD040"))
            .count();
        assert_eq!(md040, 0, "MD040 should recognize mermaid as language: {warnings:?}");
    }

    #[test]
    fn test_md042_with_auto_references() {
        // MD042 flags empty links []()
        // mkdocstrings uses [Class][] for auto-references
        let content = "# Test\n\nSee [module.Class][] for details.\n";
        let warnings = lint_mkdocs(content);
        let md042 = warnings
            .iter()
            .filter(|w| w.rule_name.as_deref() == Some("MD042"))
            .count();
        assert_eq!(md042, 0, "MD042 should allow auto-references: {warnings:?}");
    }

    #[test]
    fn test_md046_with_tabs_and_admonitions() {
        // MD046 checks code block style consistency
        let content = r#"# Test

=== "Tab 1"

    ```python
    code()
    ```

=== "Tab 2"

    ```python
    more_code()
    ```
"#;
        let warnings = lint_mkdocs(content);
        let md046 = warnings
            .iter()
            .filter(|w| w.rule_name.as_deref() == Some("MD046"))
            .count();
        assert_eq!(md046, 0, "MD046 should handle code in tabs: {warnings:?}");
    }

    #[test]
    fn test_md033_with_md_in_html() {
        // MD033 flags inline HTML
        // But markdown="1" attribute should be allowed in MkDocs
        let content = "# Test\n\n<div markdown=\"1\">\nContent.\n</div>\n";
        let warnings = lint_mkdocs(content);
        let md033 = warnings
            .iter()
            .filter(|w| w.rule_name.as_deref() == Some("MD033"))
            .count();
        assert_eq!(md033, 0, "MD033 should allow markdown attribute: {warnings:?}");
    }

    #[test]
    fn test_md049_md050_with_pymdown() {
        // MD049/MD050 check emphasis style consistency
        // PyMdown extensions like ==mark== use similar syntax
        let content = "# Test\n\nThis is ==marked== and *italic* and **bold**.\n";
        let warnings = lint_mkdocs(content);
        let md049 = warnings
            .iter()
            .filter(|w| w.rule_name.as_deref() == Some("MD049"))
            .count();
        let md050 = warnings
            .iter()
            .filter(|w| w.rule_name.as_deref() == Some("MD050"))
            .count();
        assert_eq!(
            md049 + md050,
            0,
            "MD049/MD050 should not flag mark syntax: {warnings:?}"
        );
    }

    #[test]
    fn test_md032_with_lists_in_admonitions() {
        // MD032 requires blanks around lists
        // Lists inside admonitions might have different rules
        let content = r#"# Test

!!! note
    Text before list.

    - Item 1
    - Item 2

    Text after list.
"#;
        let warnings = lint_mkdocs(content);
        let md032 = warnings
            .iter()
            .filter(|w| w.rule_name.as_deref() == Some("MD032"))
            .count();
        assert_eq!(md032, 0, "MD032 should handle lists in admonitions: {warnings:?}");
    }

    #[test]
    fn test_md022_md023_with_attr_list() {
        // MD022/MD023 check blanks around headings
        // Headings with attribute lists like {#id} should work
        let content = "# Test\n\n## Heading {#custom-id}\n\nText.\n";
        let warnings = lint_mkdocs(content);
        let md022 = warnings
            .iter()
            .filter(|w| w.rule_name.as_deref() == Some("MD022"))
            .count();
        let md023 = warnings
            .iter()
            .filter(|w| w.rule_name.as_deref() == Some("MD023"))
            .count();
        assert_eq!(md022 + md023, 0, "MD022/MD023 should handle attr_list: {warnings:?}");
    }

    #[test]
    fn test_md024_with_tabs() {
        // MD024 flags duplicate headings
        // Same heading in different tabs should be allowed
        let content = r#"# Guide

=== "Python"

    ## Installation

    Install with pip.

=== "JavaScript"

    ## Installation

    Install with npm.
"#;
        let warnings = lint_mkdocs(content);
        // The duplicate "Installation" headings are in different tab contexts
        // This is a nuanced case - behavior may vary
        let _ = warnings; // Just verify no panic
    }
}

// =============================================================================
// PART 5: FIX PRESERVATION TESTS
// Verify that auto-fix doesn't break extension syntax
// =============================================================================

mod fix_preservation {
    use super::*;

    fn assert_fix_preserves(content: &str, rule: &dyn Rule, rule_name: &str) {
        let ctx = LintContext::new(content, MarkdownFlavor::MkDocs, None);
        let fixed = rule.fix(&ctx).unwrap();

        // Key extension markers should be preserved
        if content.contains("!!!") {
            assert!(fixed.contains("!!!"), "{rule_name} should preserve admonitions");
        }
        if content.contains("===") {
            assert!(fixed.contains("==="), "{rule_name} should preserve tabs");
        }
        if content.contains(":::") {
            assert!(fixed.contains(":::"), "{rule_name} should preserve mkdocstrings");
        }
        if content.contains("++") && content.contains("++") {
            // Keys extension
            assert!(fixed.matches("++").count() >= 2, "{rule_name} should preserve keys");
        }
        if content.contains("--8<--") {
            assert!(fixed.contains("--8<--"), "{rule_name} should preserve snippets");
        }
    }

    #[test]
    fn test_md009_preserves_extensions() {
        let content = "# Test\n\n!!! note\n    Content here.   \n\n=== \"Tab\"\n\n    Tab content.\n";
        let rule = MD009TrailingSpaces::default();
        assert_fix_preserves(content, &rule, "MD009");
    }

    #[test]
    fn test_md010_preserves_extensions() {
        let content = "# Test\n\n!!! note\n\tTabbed content.\n";
        let rule = MD010NoHardTabs::default();
        assert_fix_preserves(content, &rule, "MD010");
    }

    #[test]
    fn test_md012_preserves_extensions() {
        let content = "# Test\n\n\n!!! note\n    Content.\n\n\n=== \"Tab\"\n\n    More.\n";
        let rule = MD012NoMultipleBlanks::default();
        assert_fix_preserves(content, &rule, "MD012");
    }

    #[test]
    fn test_md013_preserves_extensions() {
        let content = r#"# Test

!!! note "A very long admonition title that might exceed line length limits"
    Content inside the admonition that is also quite long and might be wrapped.

=== "Tab with a somewhat long title"

    Tab content here.

::: module.path.to.a.deeply.nested.Class
    options:
      show_source: true
"#;
        let config = create_mkdocs_config();
        let rule = MD013LineLength::from_config(&config);
        assert_fix_preserves(content, rule.as_ref(), "MD013");
    }

    #[test]
    fn test_md022_preserves_extensions() {
        let content = "# Test\n## Heading {#custom-id}\nText.\n";
        let config = create_mkdocs_config();
        let rule = MD022BlanksAroundHeadings::from_config(&config);
        let ctx = LintContext::new(content, MarkdownFlavor::MkDocs, None);
        let fixed = rule.fix(&ctx).unwrap();
        assert!(fixed.contains("{#custom-id}"), "MD022 should preserve attribute lists");
    }

    #[test]
    fn test_md023_preserves_extensions() {
        let content = "# Test\n\n  ## Indented {.class}\n\nText.\n";
        let config = create_mkdocs_config();
        let rule = MD023HeadingStartLeft::from_config(&config);
        let ctx = LintContext::new(content, MarkdownFlavor::MkDocs, None);
        let fixed = rule.fix(&ctx).unwrap();
        assert!(fixed.contains("{.class}"), "MD023 should preserve attribute lists");
    }

    #[test]
    fn test_md031_preserves_extensions() {
        let content = "# Test\n!!! note\n    Content.\nText after.\n";
        let rule = MD031BlanksAroundFences::default();
        assert_fix_preserves(content, &rule, "MD031");
    }

    #[test]
    fn test_md032_preserves_extensions() {
        let content = "# Test\n!!! note\n    - Item 1\n    - Item 2\nText.\n";
        let rule = MD032BlanksAroundLists::default();
        assert_fix_preserves(content, &rule, "MD032");
    }

    #[test]
    fn test_md047_preserves_extensions() {
        let content = "# Test\n\n!!! note\n    Content.\n";
        let rule = MD047SingleTrailingNewline;
        let ctx = LintContext::new(content, MarkdownFlavor::MkDocs, None);
        let fixed = rule.fix(&ctx).unwrap();
        assert!(fixed.contains("!!!"), "MD047 should preserve admonitions");
    }

    #[test]
    fn test_multiple_fixes_preserve_extensions() {
        // Apply multiple fixes in sequence
        let content = r#"# Test

!!! note
    Content with trailing spaces.

=== "Tab"

    Tab content.


Extra blank lines above.

::: module.Class
    options:
      show: true
"#;
        let ctx = LintContext::new(content, MarkdownFlavor::MkDocs, None);

        // Apply fixes in sequence
        let md009 = MD009TrailingSpaces::default();
        let fixed1 = md009.fix(&ctx).unwrap();

        let ctx2 = LintContext::new(&fixed1, MarkdownFlavor::MkDocs, None);
        let md012 = MD012NoMultipleBlanks::default();
        let fixed2 = md012.fix(&ctx2).unwrap();

        assert!(fixed2.contains("!!!"), "Multiple fixes should preserve admonitions");
        assert!(fixed2.contains("==="), "Multiple fixes should preserve tabs");
        assert!(fixed2.contains(":::"), "Multiple fixes should preserve mkdocstrings");
    }
}

// =============================================================================
// PART 6: STRESS TESTS
// Complex documents with multiple nested extensions and edge cases
// =============================================================================

mod stress_tests {
    use super::*;

    #[test]
    fn test_comprehensive_document() {
        let content = r#"# Comprehensive MkDocs Document

[TOC]

## Introduction

This document tests all MkDocs extensions. The HTML[^1] works.

*[HTML]: Hypertext Markup Language
*[CSS]: Cascading Style Sheets

## Admonitions with Everything

!!! note "Complex Admonition"
    This admonition contains:

    - Task lists:
        - [x] Completed
        - [ ] Pending

    - Keys: Press ++ctrl+c++ to copy.

    - Code:

        ```python title="example.py"
        print("Hello")
        ```

    - Math: $E = mc^2$

    - Formatting: ==highlighted== and ^^inserted^^

??? tip "Collapsible with Tabs"

    === "Python"

        ```python
        def greet():
            print("Hello")
        ```

    === "Rust"

        ```rust
        fn greet() {
            println!("Hello");
        }
        ```

## API Reference

::: mypackage.core.MainClass
    handler: python
    options:
        show_source: true
        heading_level: 3
        members:
            - __init__
            - process
            - cleanup

See [mypackage.core.MainClass][] for details.

## Definition Lists

API
:   Application Programming Interface.

    Used for:

    - Communication
    - Integration

SDK
:   Software Development Kit.

## Complex Math

Inline: $\sum_{i=1}^{n} x_i$ and $\int_0^\infty e^{-x} dx$

Block:

$$
\mathbf{V}_1 \times \mathbf{V}_2 =
\begin{vmatrix}
\mathbf{i} & \mathbf{j} & \mathbf{k} \\
\frac{\partial X}{\partial u} & \frac{\partial Y}{\partial u} & 0 \\
\frac{\partial X}{\partial v} & \frac{\partial Y}{\partial v} & 0
\end{vmatrix}
$$

## Critic Markup

Has {++additions++}, {--deletions--}, and {~~old~>new~~}.

It also has {==highlights==} and {>>author comments<<}.

## Snippets

--8<-- "examples/header.md"

## Formatting Summary

| Feature | Syntax | Example |
|---------|--------|---------|
| Keys | `++key++` | ++enter++ |
| Mark | `==text==` | ==marked== |
| Super | `^text^` | ^super^ |
| Sub | `~text~` | ~sub~ |

---

[^1]: Footnote with complex content:

    Including code:

    ```python
    print("footnote code")
    ```

    And lists:

    - Item 1
    - Item 2
"#;

        let warnings = lint_mkdocs(content);
        assert!(
            warnings.is_empty(),
            "Comprehensive document should have no warnings: {warnings:?}"
        );
    }

    #[test]
    fn test_maximum_nesting_depth() {
        let content = r#"# Maximum Nesting

!!! note "Level 1"

    !!! warning "Level 2"

        !!! danger "Level 3"

            === "Tab A"

                ```python title="nested.py"
                def deeply_nested():
                    """
                    Docstring with math: $x^2$
                    """
                    pass
                ```

            === "Tab B"

                ::: module.Class
                    options:
                        show: true
"#;
        let warnings = lint_mkdocs(content);
        assert!(warnings.is_empty(), "Maximum nesting should work: {warnings:?}");
    }

    #[test]
    fn test_all_inline_extensions_together() {
        let content = r#"# All Inline Extensions

This has ==highlights==, ^super^, ~sub~, ~~strike~~, ^^insert^^.

Also ++keys++ and `#!python code()`.

Math inline: $\alpha + \beta = \gamma$

Emoji: :material-check: :fontawesome-solid-heart:

Critic: {++add++} {--del--} {~~old~>new~~} {==mark==} {>>note<<}

Combined: ==mark== ^sup^ ~sub~ ^^ins^^ ++key++ `#!py x` $x$ :smile:
"#;
        let warnings = lint_mkdocs(content);
        assert!(warnings.is_empty(), "All inline extensions should work: {warnings:?}");
    }

    #[test]
    fn test_rapid_context_switching() {
        // Rapidly switch between different extension contexts
        let content = r#"# Rapid Switching

!!! note
    Note.

=== "Tab"
    Tab.

::: mod
    opt: val

!!! warning
    Warning.

=== "Another"
    Another.

::: other
    more: opts

Regular paragraph.

!!! tip
    Final.
"#;
        let warnings = lint_mkdocs(content);
        assert!(warnings.is_empty(), "Rapid context switching should work: {warnings:?}");
    }

    #[test]
    fn test_large_document_performance() {
        // Generate a large document with many extensions
        let mut content = String::from("# Large Document\n\n");

        for i in 0..50 {
            content.push_str(&format!("## Section {i}\n\n"));
            content.push_str(&format!("!!! note \"Note {i}\"\n    Content for note {i}.\n\n"));
            content.push_str(&format!("=== \"Tab A{i}\"\n\n    Tab A content.\n\n"));
            content.push_str(&format!("=== \"Tab B{i}\"\n\n    Tab B content.\n\n"));
            content.push_str(&format!("::: module{i}.Class\n\n"));
            content.push_str(&format!("Text with ==hi{i}== and ++key{i}++.\n\n"));
        }

        // Trim trailing newlines to avoid MD012 (multiple blank lines at end)
        let content = content.trim_end().to_string() + "\n";

        let start = std::time::Instant::now();
        let warnings = lint_mkdocs(&content);
        let duration = start.elapsed();

        assert!(
            warnings.is_empty(),
            "Large document should have no warnings: {warnings:?}"
        );
        assert!(
            duration.as_secs() < 10,
            "Large document should lint in reasonable time: {duration:?}"
        );
    }

    #[test]
    fn test_unicode_in_extensions() {
        let content = r#"# Unicode Extensions

!!! note "日本語タイトル"
    Japanese content: こんにちは

=== "Français"
    Contenu français avec accents: é, è, ê, ë

=== "中文"
    中文内容

::: module.Ελληνικά

Text with ==强调== and ^^挿入^^ and ~下付き~.

*[API]: アプリケーションプログラミングインターフェース

Press ++ctrl+日++ for Japanese.
"#;
        let warnings = lint_mkdocs(content);
        assert!(warnings.is_empty(), "Unicode in extensions should work: {warnings:?}");
    }

    #[test]
    fn test_special_characters_in_extensions() {
        let content = r#"# Special Characters

!!! note "Title with <angle> & 'quotes' \"doubles\""
    Content with special chars: < > & ' " ` ~

=== "Tab with `backticks`"
    Content.

=== "Tab with *asterisks*"
    More content.

::: module.Class_with_underscores

*[C++]: C Plus Plus
*[C#]: C Sharp

Keys: ++ctrl+<++ and ++>+shift++
"#;
        let warnings = lint_mkdocs(content);
        // May have some warnings but should not crash
        let _ = warnings;
    }

    #[test]
    fn test_empty_extension_blocks() {
        let content = r#"# Empty Blocks

!!! note

!!! warning ""

=== ""

=== "Empty Tab"

:::

::: module

Text after empty blocks.
"#;
        // Should handle gracefully
        let _ = lint_mkdocs(content);
    }

    #[test]
    fn test_extensions_at_document_boundaries() {
        // Extension at very start
        let content1 = "!!! note\n    Start with admonition.\n";
        let _ = lint_mkdocs(content1);

        // Extension at very end
        let content2 = "# Title\n\n!!! note\n    End with admonition.";
        let _ = lint_mkdocs(content2);

        // Only extensions, no regular content
        let content3 = "!!! note\n    Only.\n\n=== \"Tab\"\n    Tab.\n\n::: mod\n";
        let _ = lint_mkdocs(content3);
    }
}

// =============================================================================
// PART 7: TABLE EXTENSION TESTS
// Verify table handling with MkDocs extensions (MD056)
// =============================================================================

mod table_extensions {
    use super::*;

    #[test]
    fn test_basic_table_in_mkdocs() {
        let content = r#"# Tables

| Header 1 | Header 2 | Header 3 |
|----------|----------|----------|
| Cell 1   | Cell 2   | Cell 3   |
| Cell 4   | Cell 5   | Cell 6   |
"#;
        let warnings = lint_mkdocs(content);
        assert!(warnings.is_empty(), "Basic table should work: {warnings:?}");
    }

    #[test]
    fn test_table_in_admonition() {
        let content = r#"# Table in Admonition

!!! note "Table Example"

    | Header | Value |
    |--------|-------|
    | Key    | Val   |
"#;
        let warnings = lint_mkdocs(content);
        assert!(warnings.is_empty(), "Table in admonition should work: {warnings:?}");
    }

    #[test]
    fn test_table_in_content_tab() {
        let content = r#"# Table in Tab

=== "Data Table"

    | ID | Name    | Status |
    |----|---------|--------|
    | 1  | Alice   | Active |
    | 2  | Bob     | Active |

=== "Summary Table"

    | Metric | Value |
    |--------|-------|
    | Total  | 100   |
"#;
        let warnings = lint_mkdocs(content);
        assert!(warnings.is_empty(), "Table in content tab should work: {warnings:?}");
    }

    #[test]
    fn test_table_with_inline_extensions() {
        let content = r#"# Tables with Extensions

| Feature | Syntax | Result |
|---------|--------|--------|
| Keys | `++ctrl++` | ++ctrl++ |
| Mark | `==text==` | ==text== |
| Math | `$x^2$` | $x^2$ |
"#;
        let warnings = lint_mkdocs(content);
        assert!(
            warnings.is_empty(),
            "Table with inline extensions should work: {warnings:?}"
        );
    }

    #[test]
    fn test_table_alignment_variations() {
        let content = r#"# Aligned Tables

| Left | Center | Right |
|:-----|:------:|------:|
| L    | C      | R     |
| L    | C      | R     |
"#;
        let warnings = lint_mkdocs(content);
        assert!(warnings.is_empty(), "Table alignment should work: {warnings:?}");
    }

    #[test]
    fn test_table_after_admonition() {
        let content = r#"# Table After Admonition

!!! info
    Some info here.

| After | Admonition |
|-------|------------|
| Data  | Here       |
"#;
        let warnings = lint_mkdocs(content);
        assert!(warnings.is_empty(), "Table after admonition should work: {warnings:?}");
    }

    #[test]
    fn test_multiple_tables_with_extensions() {
        let content = r#"# Multiple Tables

!!! note "First Table"

    | A | B |
    |---|---|
    | 1 | 2 |

!!! warning "Second Table"

    | C | D |
    |---|---|
    | 3 | 4 |

Regular table:

| E | F |
|---|---|
| 5 | 6 |
"#;
        let warnings = lint_mkdocs(content);
        assert!(
            warnings.is_empty(),
            "Multiple tables with extensions should work: {warnings:?}"
        );
    }
}

// =============================================================================
// PART 8: FRONTMATTER/META EXTENSION TESTS
// Verify YAML frontmatter handling with MkDocs
// =============================================================================

mod frontmatter_tests {
    use super::*;

    #[test]
    fn test_yaml_frontmatter_basic() {
        let content = r#"---
title: Test Document
description: A test document for MkDocs
---

# Test Document

Content here.
"#;
        let warnings = lint_mkdocs(content);
        assert!(warnings.is_empty(), "YAML frontmatter should work: {warnings:?}");
    }

    #[test]
    fn test_frontmatter_with_extensions() {
        let content = r#"---
title: Extensions Test
tags:
  - mkdocs
  - testing
---

# Extensions Test

!!! note
    Content after frontmatter.

=== "Tab"
    Tab content.
"#;
        let warnings = lint_mkdocs(content);
        assert!(
            warnings.is_empty(),
            "Frontmatter with extensions should work: {warnings:?}"
        );
    }

    #[test]
    fn test_frontmatter_with_special_yaml() {
        let content = r#"---
title: "Title with: colon"
description: |
  Multi-line
  description
list:
  - item1
  - item2
---

# Content

Text here.
"#;
        let warnings = lint_mkdocs(content);
        assert!(
            warnings.is_empty(),
            "Complex YAML frontmatter should work: {warnings:?}"
        );
    }

    #[test]
    fn test_frontmatter_not_confused_with_hr() {
        let content = r#"---
title: Test
---

# Heading

Content.

---

More content after horizontal rule.
"#;
        let warnings = lint_mkdocs(content);
        assert!(warnings.is_empty(), "HR after frontmatter should work: {warnings:?}");
    }

    #[test]
    fn test_frontmatter_with_mkdocstrings_config() {
        let content = r#"---
title: API Reference
plugins:
  - mkdocstrings:
      handlers:
        python:
          options:
            show_source: true
---

# API Reference

::: mymodule.MyClass
"#;
        let warnings = lint_mkdocs(content);
        assert!(
            warnings.is_empty(),
            "Frontmatter with mkdocstrings config should work: {warnings:?}"
        );
    }

    #[test]
    fn test_toml_frontmatter() {
        // TOML frontmatter uses +++ delimiters (Hugo-style)
        // Note: rumdl may or may not support this - test for graceful handling
        let content = r#"+++
title = "TOML Frontmatter"
date = 2024-01-01
+++

# TOML Test

Content.
"#;
        // Should not panic, warnings may vary based on implementation
        let _ = lint_mkdocs(content);
    }
}

// =============================================================================
// PART 9: EXTENSION-INSIDE-EXTENSION INTERACTION TESTS
// Verify deeply nested and interacting extensions
// =============================================================================

mod extension_interactions {
    use super::*;

    #[test]
    fn test_tabs_inside_admonition() {
        let content = r#"# Nested Extensions

!!! example "Code Examples"

    === "Python"

        ```python
        def hello():
            print("Hello")
        ```

    === "Rust"

        ```rust
        fn hello() {
            println!("Hello");
        }
        ```

    === "Go"

        ```go
        func hello() {
            fmt.Println("Hello")
        }
        ```
"#;
        let warnings = lint_mkdocs(content);
        assert!(warnings.is_empty(), "Tabs inside admonition should work: {warnings:?}");
    }

    #[test]
    fn test_admonition_inside_tabs() {
        let content = r#"# Admonitions in Tabs

=== "Notes"

    !!! note
        A note inside a tab.

    !!! warning
        A warning inside the same tab.

=== "Tips"

    !!! tip
        A tip in another tab.
"#;
        let warnings = lint_mkdocs(content);
        assert!(warnings.is_empty(), "Admonitions inside tabs should work: {warnings:?}");
    }

    #[test]
    fn test_code_blocks_inside_nested_extensions() {
        let content = r#"# Deep Code Nesting

!!! example

    === "With Highlighting"

        ```python title="example.py" hl_lines="2 3"
        def process():
            data = load()
            result = transform(data)
            return result
        ```

    === "With Line Numbers"

        ```python linenums="1"
        def other():
            pass
        ```
"#;
        let warnings = lint_mkdocs(content);
        assert!(
            warnings.is_empty(),
            "Code blocks in nested extensions should work: {warnings:?}"
        );
    }

    #[test]
    fn test_mkdocstrings_inside_admonition() {
        let content = r#"# API in Admonition

!!! info "Quick Reference"

    ::: mymodule.quick_function
        options:
            show_source: false

!!! example "Full Reference"

    ::: mymodule.detailed_function
        options:
            show_source: true
"#;
        let warnings = lint_mkdocs(content);
        assert!(
            warnings.is_empty(),
            "mkdocstrings inside admonition should work: {warnings:?}"
        );
    }

    #[test]
    fn test_definition_list_inside_admonition() {
        let content = r#"# Definitions in Admonition

!!! note "Terminology"

    Term 1
    :   Definition of term 1.

    Term 2
    :   Definition of term 2.
        With additional detail.
"#;
        let warnings = lint_mkdocs(content);
        assert!(
            warnings.is_empty(),
            "Definition list in admonition should work: {warnings:?}"
        );
    }

    #[test]
    fn test_footnotes_with_extensions() {
        let content = r#"# Footnotes and Extensions

Text with footnote[^1] and ==highlighting==.

!!! note
    Content with another footnote[^2].

[^1]: Regular footnote.

[^2]: Footnote from admonition.

    With code:

    ```python
    print("footnote code")
    ```
"#;
        let warnings = lint_mkdocs(content);
        assert!(
            warnings.is_empty(),
            "Footnotes with extensions should work: {warnings:?}"
        );
    }

    #[test]
    fn test_math_inside_all_contexts() {
        let content = r#"# Math Everywhere

Inline: $E = mc^2$

!!! note "Math Note"
    Block math in admonition:

    $$
    \int_0^\infty e^{-x} dx = 1
    $$

=== "Equations"

    More math: $\sum_{i=1}^n x_i$

    $$
    \frac{d}{dx} \sin(x) = \cos(x)
    $$
"#;
        let warnings = lint_mkdocs(content);
        assert!(
            warnings.is_empty(),
            "Math inside all contexts should work: {warnings:?}"
        );
    }

    #[test]
    fn test_inline_extensions_inside_all_block_extensions() {
        let content = r#"# Inline in Blocks

!!! note
    Keys: ++ctrl+s++ to save.
    Mark: ==important== text.
    Super: E=mc^2^

=== "Inline Tab"
    Subscript: H~2~O
    Emoji: :material-check:
    Critic: {++added++}

::: module.Class
    options:
        show_root_heading: true
"#;
        let warnings = lint_mkdocs(content);
        assert!(
            warnings.is_empty(),
            "Inline extensions in block extensions should work: {warnings:?}"
        );
    }

    #[test]
    fn test_collapsible_with_everything() {
        let content = r#"# Collapsible Complex

??? example "Click to expand"

    === "Overview"

        !!! tip
            Nested tip.

        Regular paragraph with ==marks==.

    === "Details"

        ```python title="code.py"
        print("nested code")
        ```

        Math: $x^2 + y^2 = z^2$

???+ warning "Open by default"

    - List with ++keys++
    - And ^superscript^
"#;
        let warnings = lint_mkdocs(content);
        assert!(
            warnings.is_empty(),
            "Collapsible with everything should work: {warnings:?}"
        );
    }

    #[test]
    fn test_triple_nesting_depth() {
        let content = r#"# Triple Nesting

!!! note "Level 1"

    === "Level 2A"

        !!! warning "Level 3"
            Deep content with ==mark== and ++key++.

    === "Level 2B"

        !!! tip "Level 3 Alt"
            Alternative deep content.
"#;
        let warnings = lint_mkdocs(content);
        assert!(warnings.is_empty(), "Triple nesting should work: {warnings:?}");
    }
}

// =============================================================================
// PART 10: FIX OUTPUT VALIDATION TESTS
// Verify that fixed output is still valid MkDocs markdown
// =============================================================================

mod fix_validation {
    use super::*;

    fn fix_and_validate(content: &str, rule: &dyn Rule, rule_name: &str) {
        let ctx = LintContext::new(content, MarkdownFlavor::MkDocs, None);
        let fixed = rule.fix(&ctx).unwrap();

        // The fixed content should have no warnings from ANY rule
        let post_fix_warnings = lint_mkdocs(&fixed);

        // Filter to warnings that indicate broken extension syntax
        let extension_breaks: Vec<_> = post_fix_warnings
            .iter()
            .filter(|w| {
                // These would indicate the fix broke extension syntax
                let msg = &w.message;
                msg.contains("!!!") || msg.contains("===") || msg.contains(":::")
            })
            .collect();

        assert!(
            extension_breaks.is_empty(),
            "{rule_name} fix should not break extension syntax. \
             Original:\n{content}\nFixed:\n{fixed}\nBroken: {extension_breaks:?}"
        );
    }

    #[test]
    fn test_md009_fix_validates() {
        let content = "# Test\n\n!!! note\n    Content here.   \n\n=== \"Tab\"  \n    Tab.   \n";
        let rule = MD009TrailingSpaces::default();
        fix_and_validate(content, &rule, "MD009");

        // Also verify the fix removed trailing spaces
        let ctx = LintContext::new(content, MarkdownFlavor::MkDocs, None);
        let fixed = rule.fix(&ctx).unwrap();
        assert!(!fixed.contains("   \n"), "MD009 should remove trailing spaces");
    }

    #[test]
    fn test_md010_fix_validates() {
        let content = "# Test\n\n!!! note\n\tTabbed content.\n";
        let rule = MD010NoHardTabs::default();
        fix_and_validate(content, &rule, "MD010");

        let ctx = LintContext::new(content, MarkdownFlavor::MkDocs, None);
        let fixed = rule.fix(&ctx).unwrap();
        assert!(!fixed.contains('\t'), "MD010 should remove hard tabs");
    }

    #[test]
    fn test_md012_fix_validates() {
        let content = "# Test\n\n\n\n!!! note\n    Content.\n\n\n\n=== \"Tab\"\n    More.\n";
        let rule = MD012NoMultipleBlanks::default();
        fix_and_validate(content, &rule, "MD012");

        let ctx = LintContext::new(content, MarkdownFlavor::MkDocs, None);
        let fixed = rule.fix(&ctx).unwrap();
        assert!(!fixed.contains("\n\n\n"), "MD012 should reduce multiple blanks");
    }

    #[test]
    fn test_md047_fix_validates() {
        let content = "# Test\n\n!!! note\n    Content.\n\n=== \"Tab\"\n    Tab.";
        let rule = MD047SingleTrailingNewline;
        fix_and_validate(content, &rule, "MD047");

        let ctx = LintContext::new(content, MarkdownFlavor::MkDocs, None);
        let fixed = rule.fix(&ctx).unwrap();
        assert!(fixed.ends_with('\n'), "MD047 should add trailing newline");
        assert!(!fixed.ends_with("\n\n"), "MD047 should not add multiple newlines");
    }

    #[test]
    fn test_fix_preserves_extension_markers_precisely() {
        let content = "# Test\n\n!!! note \"Title\"   \n    Content.   \n\n???+ tip\n    Tip.   \n";
        let rule = MD009TrailingSpaces::default();
        let ctx = LintContext::new(content, MarkdownFlavor::MkDocs, None);
        let fixed = rule.fix(&ctx).unwrap();

        // Verify specific markers are preserved exactly
        assert!(fixed.contains("!!! note \"Title\""), "Admonition marker preserved");
        assert!(fixed.contains("???+ tip"), "Collapsible marker preserved");
    }

    #[test]
    fn test_fix_chain_validates() {
        // Apply multiple fixes in sequence and validate result
        let content = "# Test   \n\n\n!!! note\n    Content.   \n\n\n=== \"Tab\"\n    Tab.   ";

        let mut current = content.to_string();

        // Fix trailing spaces
        let ctx = LintContext::new(&current, MarkdownFlavor::MkDocs, None);
        current = MD009TrailingSpaces::default().fix(&ctx).unwrap();

        // Fix multiple blanks
        let ctx = LintContext::new(&current, MarkdownFlavor::MkDocs, None);
        current = MD012NoMultipleBlanks::default().fix(&ctx).unwrap();

        // Fix trailing newline
        let ctx = LintContext::new(&current, MarkdownFlavor::MkDocs, None);
        current = MD047SingleTrailingNewline.fix(&ctx).unwrap();

        // Final result should be clean
        let final_warnings = lint_mkdocs(&current);
        let critical: Vec<_> = final_warnings
            .iter()
            .filter(|w| matches!(w.rule_name.as_deref(), Some("MD009") | Some("MD012") | Some("MD047")))
            .collect();

        assert!(
            critical.is_empty(),
            "Fix chain should resolve all targeted issues: {critical:?}"
        );

        // Extensions still intact
        assert!(current.contains("!!! note"), "Admonition preserved");
        assert!(current.contains("=== \"Tab\""), "Tab preserved");
    }

    #[test]
    fn test_fix_with_deeply_nested_content() {
        let content = r#"# Deep Nesting

!!! note "Outer"

    === "Tab A"

        !!! warning "Inner"
            Content.

    === "Tab B"

        More content.
"#;
        let rule = MD009TrailingSpaces::default();
        let ctx = LintContext::new(content, MarkdownFlavor::MkDocs, None);
        let fixed = rule.fix(&ctx).unwrap();

        // All nesting levels preserved
        assert!(fixed.contains("!!! note \"Outer\""), "Outer admonition");
        assert!(fixed.contains("=== \"Tab A\""), "Tab A");
        assert!(fixed.contains("!!! warning \"Inner\""), "Inner admonition");
        assert!(fixed.contains("=== \"Tab B\""), "Tab B");

        // No trailing spaces remain
        for (i, line) in fixed.lines().enumerate() {
            assert!(
                !line.ends_with(' '),
                "Line {} should not have trailing space: {:?}",
                i + 1,
                line
            );
        }
    }
}

// =============================================================================
// PART 11: SYSTEMATIC BOUNDARY TESTS
// Property-based style tests for edge cases
// =============================================================================

mod boundary_tests {
    use super::*;

    #[test]
    fn test_extension_marker_variations() {
        // Test all valid admonition marker formats
        let markers = [
            "!!! note",
            "!!! note \"Title\"",
            "!!! note \"\"",
            "??? note",
            "??? note \"Title\"",
            "???+ note",
            "???+ note \"Title\"",
        ];

        for marker in markers {
            let content = format!("# Test\n\n{marker}\n    Content.\n");
            let warnings = lint_mkdocs(&content);
            assert!(warnings.is_empty(), "Marker '{marker}' should work: {warnings:?}");
        }
    }

    #[test]
    fn test_tab_marker_variations() {
        let markers = [
            "=== \"Tab\"",
            "=== \"\"",
            "=== \"Tab with spaces\"",
            "=== \"Tab's apostrophe\"",
        ];

        for marker in markers {
            let content = format!("# Test\n\n{marker}\n    Content.\n");
            let warnings = lint_mkdocs(&content);
            // May have warnings but should not panic
            let _ = warnings;
        }
    }

    #[test]
    fn test_mkdocstrings_marker_variations() {
        let markers = [
            "::: module",
            "::: module.submodule",
            "::: module.submodule.Class",
            "::: module.Class.method",
            "::: package.module:function",
        ];

        for marker in markers {
            let content = format!("# Test\n\n{marker}\n");
            let warnings = lint_mkdocs(&content);
            assert!(warnings.is_empty(), "mkdocstrings '{marker}' should work: {warnings:?}");
        }
    }

    #[test]
    fn test_inline_extension_boundary_positions() {
        // Extensions at various positions in text
        let cases = [
            "==start== of line",
            "at ==middle== of line",
            "at end ==here==",
            "==only==",
            "a==tight==b",
            "==one== and ==two==",
        ];

        for case in cases {
            let content = format!("# Test\n\n{case}\n");
            let warnings = lint_mkdocs(&content);
            assert!(
                warnings.is_empty(),
                "Inline extension '{case}' should work: {warnings:?}"
            );
        }
    }

    #[test]
    fn test_keys_extension_variations() {
        let keys = [
            "++ctrl++",
            "++ctrl+c++",
            "++ctrl+alt+del++",
            "++ctrl+shift+alt+f12++",
            "++enter++",
            "++backspace++",
            "++arrow-up++",
            "++arrow-down++",
            "++arrow-left++",
            "++arrow-right++",
        ];

        for key in keys {
            let content = format!("# Test\n\nPress {key} to continue.\n");
            let warnings = lint_mkdocs(&content);
            assert!(warnings.is_empty(), "Key '{key}' should work: {warnings:?}");
        }
    }

    #[test]
    fn test_snippet_syntax_variations() {
        let snippets = [
            "--8<-- \"file.md\"",
            "--8<-- \"path/to/file.md\"",
            "--8<-- \"../relative/file.md\"",
            ";--8<--",
        ];

        for snippet in snippets {
            let content = format!("# Test\n\n{snippet}\n");
            let warnings = lint_mkdocs(&content);
            assert!(warnings.is_empty(), "Snippet '{snippet}' should work: {warnings:?}");
        }
    }

    #[test]
    fn test_math_boundary_positions() {
        let cases = [
            "$x$ at start",
            "at end $x$",
            "in $middle$ here",
            "$a$ and $b$ and $c$",
            "tight$x$bound",
        ];

        for case in cases {
            let content = format!("# Test\n\n{case}\n");
            let warnings = lint_mkdocs(&content);
            assert!(warnings.is_empty(), "Math '{case}' should work: {warnings:?}");
        }
    }

    #[test]
    fn test_critic_markup_variations() {
        let critics = [
            "{++addition++}",
            "{--deletion--}",
            "{~~old~>new~~}",
            "{==highlight==}",
            "{>>comment<<}",
        ];

        for critic in critics {
            let content = format!("# Test\n\nText with {critic} here.\n");
            let warnings = lint_mkdocs(&content);
            assert!(warnings.is_empty(), "Critic '{critic}' should work: {warnings:?}");
        }
    }

    #[test]
    fn test_abbreviation_variations() {
        let abbrs = [
            "*[HTML]: Hypertext Markup Language",
            "*[CSS]: Cascading Style Sheets",
            "*[API]: Application Programming Interface",
            "*[URL]: Uniform Resource Locator",
        ];

        for abbr in abbrs {
            let content = format!("# Test\n\nThe HTML spec.\n\n{abbr}\n");
            let warnings = lint_mkdocs(&content);
            assert!(warnings.is_empty(), "Abbreviation '{abbr}' should work: {warnings:?}");
        }
    }

    #[test]
    fn test_attribute_list_variations() {
        let attrs = [
            "{ #id }",
            "{ .class }",
            "{ #id .class }",
            "{ .class1 .class2 }",
            "{ data-attr=value }",
            "{ #id .class data-x=y }",
        ];

        for attr in attrs {
            let content = format!("# Heading {attr}\n\nText.\n");
            let warnings = lint_mkdocs(&content);
            assert!(warnings.is_empty(), "Attribute '{attr}' should work: {warnings:?}");
        }
    }

    #[test]
    fn test_empty_and_whitespace_only_content() {
        let cases = ["", "\n", "\n\n", "   ", "   \n", "\t", "\t\n"];

        for case in cases {
            // Should not panic
            let _ = lint_mkdocs(case);
        }
    }

    #[test]
    fn test_maximum_line_lengths_with_extensions() {
        // 79 chars (within limit)
        let short = format!(
            "# Test\n\nPress {} to continue.\n",
            "++ctrl+alt+shift+".to_string() + &"x".repeat(40) + "++"
        );
        // Should work or fail gracefully
        let _ = lint_mkdocs(&short);
    }
}

// =============================================================================
// PART 12: REGRESSION PREVENTION TESTS
// Tests for specific bugs that could occur
// =============================================================================

mod regression_tests {
    use super::*;

    #[test]
    fn test_admonition_not_confused_with_emphasis() {
        // The "!!!" should not trigger emphasis-related rules
        let content = "# Test\n\n!!! note\n    Content.\n";
        let warnings = lint_mkdocs(content);
        let emphasis_warnings: Vec<_> = warnings
            .iter()
            .filter(|w| {
                matches!(
                    w.rule_name.as_deref(),
                    Some("MD036") | Some("MD037") | Some("MD049") | Some("MD050")
                )
            })
            .collect();
        assert!(
            emphasis_warnings.is_empty(),
            "Admonition should not trigger emphasis rules: {emphasis_warnings:?}"
        );
    }

    #[test]
    fn test_tabs_not_confused_with_code_fence() {
        // The "===" should not trigger code fence rules
        let content = "# Test\n\n=== \"Tab\"\n    Content.\n";
        let warnings = lint_mkdocs(content);
        let fence_warnings: Vec<_> = warnings
            .iter()
            .filter(|w| matches!(w.rule_name.as_deref(), Some("MD031") | Some("MD040") | Some("MD046")))
            .collect();
        assert!(
            fence_warnings.is_empty(),
            "Tab should not trigger fence rules: {fence_warnings:?}"
        );
    }

    #[test]
    fn test_mkdocstrings_not_confused_with_definition_list() {
        // The ":::" should not trigger definition list warnings
        let content = "# Test\n\n::: module.Class\n";
        let warnings = lint_mkdocs(content);
        // Should have no warnings about definition lists
        assert!(
            warnings.is_empty(),
            "mkdocstrings should not trigger warnings: {warnings:?}"
        );
    }

    #[test]
    fn test_keys_not_confused_with_code_span() {
        // The "++...++" should not trigger MD038 (spaces in code spans)
        let content = "# Test\n\nPress ++ctrl+c++ to copy.\n";
        let warnings = lint_mkdocs(content);
        let code_span_warnings: Vec<_> = warnings
            .iter()
            .filter(|w| w.rule_name.as_deref() == Some("MD038"))
            .collect();
        assert!(
            code_span_warnings.is_empty(),
            "Keys should not trigger MD038: {code_span_warnings:?}"
        );
    }

    #[test]
    fn test_mark_not_confused_with_emphasis() {
        // The "==...==" should not trigger emphasis rules
        let content = "# Test\n\nThis is ==highlighted== text.\n";
        let warnings = lint_mkdocs(content);
        let emphasis_warnings: Vec<_> = warnings
            .iter()
            .filter(|w| matches!(w.rule_name.as_deref(), Some("MD049") | Some("MD050")))
            .collect();
        assert!(
            emphasis_warnings.is_empty(),
            "Mark should not trigger emphasis rules: {emphasis_warnings:?}"
        );
    }

    #[test]
    fn test_snippet_not_confused_with_html_comment() {
        // The "--8<--" should not trigger HTML comment handling
        let content = "# Test\n\n--8<-- \"file.md\"\n";
        let warnings = lint_mkdocs(content);
        let html_warnings: Vec<_> = warnings
            .iter()
            .filter(|w| w.rule_name.as_deref() == Some("MD033"))
            .collect();
        assert!(
            html_warnings.is_empty(),
            "Snippet should not trigger MD033: {html_warnings:?}"
        );
    }

    #[test]
    fn test_indented_content_in_extensions_not_code_block() {
        // Indented content inside admonitions should not be treated as code
        let content = r#"# Test

!!! note
    This is indented but not code.

    Still not code.
"#;
        let warnings = lint_mkdocs(content);
        let code_warnings: Vec<_> = warnings
            .iter()
            .filter(|w| matches!(w.rule_name.as_deref(), Some("MD040") | Some("MD046")))
            .collect();
        assert!(
            code_warnings.is_empty(),
            "Admonition content should not be code: {code_warnings:?}"
        );
    }

    #[test]
    fn test_auto_reference_handling() {
        // [Class][] is auto-reference syntax in MkDocs with mkdocstrings
        // This tests current behavior - may warn until auto-ref support is added
        let content = "# Test\n\nSee [MyClass][] for details.\n";
        let warnings = lint_mkdocs(content);
        // Document current behavior: MD052 may or may not fire
        // The important thing is it doesn't crash and handles gracefully
        let _ = warnings;
    }

    #[test]
    fn test_smartsymbols_not_flagged() {
        // Smart symbols like (c), (tm), --> should work
        let content = "# Test\n\nCopyright (c) 2024. Trademark (tm). Arrow -->.\n";
        let warnings = lint_mkdocs(content);
        assert!(warnings.is_empty(), "Smart symbols should work: {warnings:?}");
    }

    #[test]
    fn test_emoji_shortcodes_not_flagged() {
        // Emoji shortcodes :name: should work
        let content = "# Test\n\nCheck :material-check: and :fontawesome-solid-star:.\n";
        let warnings = lint_mkdocs(content);
        assert!(warnings.is_empty(), "Emoji shortcodes should work: {warnings:?}");
    }
}

// =============================================================================
// PART 13: CROSS-FLAVOR COMPARISON TESTS
// Verify MkDocs flavor behaves differently from Standard where expected
// =============================================================================

mod cross_flavor_tests {
    use super::*;

    #[test]
    fn test_admonition_standard_vs_mkdocs() {
        let content = "# Test\n\n!!! note\n    Content.\n";

        let mkdocs_warnings = lint_mkdocs(content);
        let standard_warnings = lint_standard(content);

        // MkDocs should recognize this, Standard may flag it
        assert!(
            mkdocs_warnings.len() <= standard_warnings.len(),
            "MkDocs should be more lenient with admonitions.\n\
             MkDocs: {mkdocs_warnings:?}\nStandard: {standard_warnings:?}"
        );
    }

    #[test]
    fn test_content_tabs_standard_vs_mkdocs() {
        let content = "# Test\n\n=== \"Tab\"\n    Content.\n";

        let mkdocs_warnings = lint_mkdocs(content);
        let standard_warnings = lint_standard(content);

        // MkDocs should recognize tabs, Standard may see issues
        assert!(
            mkdocs_warnings.len() <= standard_warnings.len(),
            "MkDocs should be more lenient with content tabs.\n\
             MkDocs: {mkdocs_warnings:?}\nStandard: {standard_warnings:?}"
        );
    }

    #[test]
    fn test_mkdocstrings_standard_vs_mkdocs() {
        let content = "# Test\n\n::: module.Class\n";

        let mkdocs_warnings = lint_mkdocs(content);
        let standard_warnings = lint_standard(content);

        // MkDocs should recognize mkdocstrings
        assert!(
            mkdocs_warnings.len() <= standard_warnings.len(),
            "MkDocs should be more lenient with mkdocstrings.\n\
             MkDocs: {mkdocs_warnings:?}\nStandard: {standard_warnings:?}"
        );
    }

    #[test]
    fn test_keys_extension_standard_vs_mkdocs() {
        let content = "# Test\n\nPress ++ctrl+c++ to copy.\n";

        let mkdocs_warnings = lint_mkdocs(content);
        let standard_warnings = lint_standard(content);

        // Both should handle this gracefully
        // Document any difference in behavior
        let _ = (mkdocs_warnings, standard_warnings);
    }

    #[test]
    fn test_mark_extension_standard_vs_mkdocs() {
        let content = "# Test\n\nThis is ==highlighted== text.\n";

        let mkdocs_warnings = lint_mkdocs(content);
        let standard_warnings = lint_standard(content);

        // Document behavior difference
        let _ = (mkdocs_warnings, standard_warnings);
    }

    #[test]
    fn test_math_standard_vs_mkdocs() {
        let content = "# Test\n\nInline $x^2$ and block:\n\n$$\ny = mx + b\n$$\n";

        let mkdocs_warnings = lint_mkdocs(content);
        let standard_warnings = lint_standard(content);

        // Math should work in both, but MkDocs may be more permissive
        let _ = (mkdocs_warnings, standard_warnings);
    }

    #[test]
    fn test_snippet_standard_vs_mkdocs() {
        let content = "# Test\n\n--8<-- \"file.md\"\n";

        let mkdocs_warnings = lint_mkdocs(content);
        let standard_warnings = lint_standard(content);

        // MkDocs should recognize snippets
        assert!(
            mkdocs_warnings.len() <= standard_warnings.len(),
            "MkDocs should be more lenient with snippets.\n\
             MkDocs: {mkdocs_warnings:?}\nStandard: {standard_warnings:?}"
        );
    }

    #[test]
    fn test_complex_document_both_flavors() {
        let content = r#"# Complex Document

!!! note "Admonition"
    Content here.

=== "Tab 1"
    Tab content.

::: module.Class

Regular paragraph.
"#;

        let mkdocs_warnings = lint_mkdocs(content);
        let standard_warnings = lint_standard(content);

        // MkDocs should have fewer warnings for extension syntax
        assert!(
            mkdocs_warnings.len() <= standard_warnings.len(),
            "MkDocs should handle extensions better.\n\
             MkDocs ({} warnings): {mkdocs_warnings:?}\n\
             Standard ({} warnings): {standard_warnings:?}",
            mkdocs_warnings.len(),
            standard_warnings.len()
        );
    }
}

// =============================================================================
// PART 14: LINE ENDING TESTS
// Verify extensions work with different line ending styles
// =============================================================================

mod line_ending_tests {
    use super::*;

    #[test]
    fn test_crlf_admonitions() {
        let content = "# Test\r\n\r\n!!! note\r\n    Content.\r\n";
        let warnings = lint_mkdocs(content);
        // Should handle CRLF gracefully
        assert!(warnings.is_empty(), "CRLF admonitions should work: {warnings:?}");
    }

    #[test]
    fn test_crlf_content_tabs() {
        let content = "# Test\r\n\r\n=== \"Tab\"\r\n\r\n    Content.\r\n";
        let warnings = lint_mkdocs(content);
        assert!(warnings.is_empty(), "CRLF content tabs should work: {warnings:?}");
    }

    #[test]
    fn test_crlf_mkdocstrings() {
        let content = "# Test\r\n\r\n::: module.Class\r\n";
        let warnings = lint_mkdocs(content);
        assert!(warnings.is_empty(), "CRLF mkdocstrings should work: {warnings:?}");
    }

    #[test]
    fn test_crlf_inline_extensions() {
        let content = "# Test\r\n\r\nText with ==mark== and ++key++.\r\n";
        let warnings = lint_mkdocs(content);
        assert!(warnings.is_empty(), "CRLF inline extensions should work: {warnings:?}");
    }

    #[test]
    fn test_crlf_nested_extensions() {
        let content = "# Test\r\n\r\n!!! note\r\n\r\n    === \"Tab\"\r\n\r\n        Content.\r\n";
        let warnings = lint_mkdocs(content);
        assert!(warnings.is_empty(), "CRLF nested extensions should work: {warnings:?}");
    }

    #[test]
    fn test_mixed_line_endings() {
        // Mix of LF and CRLF (common in cross-platform projects)
        let content = "# Test\n\n!!! note\r\n    Content.\n\n=== \"Tab\"\r\n    Tab.\n";
        // Should handle gracefully without panic
        let _ = lint_mkdocs(content);
    }

    #[test]
    fn test_cr_only_line_endings() {
        // Classic Mac line endings (rare but possible)
        let content = "# Test\r\r!!! note\r    Content.\r";
        // Should handle gracefully without panic
        let _ = lint_mkdocs(content);
    }

    #[test]
    fn test_crlf_complex_document() {
        let content = "# Document\r\n\r\n!!! note \"Title\"\r\n    Content with ==mark==.\r\n\r\n\
                       === \"Tab A\"\r\n\r\n    Tab content.\r\n\r\n\
                       === \"Tab B\"\r\n\r\n    More content.\r\n\r\n\
                       ::: module.Class\r\n";
        let warnings = lint_mkdocs(content);
        assert!(warnings.is_empty(), "CRLF complex document should work: {warnings:?}");
    }
}

// =============================================================================
// PART 15: MIXED INDENTATION TESTS
// Verify extensions handle mixed tabs and spaces
// =============================================================================

mod mixed_indentation_tests {
    use super::*;

    #[test]
    fn test_admonition_with_tab_indent() {
        let content = "# Test\n\n!!! note\n\tContent with tab.\n";
        // Tab indentation may trigger MD010, but should not crash
        let warnings = lint_mkdocs(content);
        // Filter out MD010 (hard tabs) to check extension handling
        let non_tab_warnings: Vec<_> = warnings
            .iter()
            .filter(|w| w.rule_name.as_deref() != Some("MD010"))
            .collect();
        assert!(
            non_tab_warnings.is_empty(),
            "Tab-indented admonition should work (except MD010): {non_tab_warnings:?}"
        );
    }

    #[test]
    fn test_content_tab_with_tab_indent() {
        let content = "# Test\n\n=== \"Tab\"\n\n\tContent with tab.\n";
        let warnings = lint_mkdocs(content);
        let non_tab_warnings: Vec<_> = warnings
            .iter()
            .filter(|w| w.rule_name.as_deref() != Some("MD010"))
            .collect();
        assert!(
            non_tab_warnings.is_empty(),
            "Tab-indented content tab should work: {non_tab_warnings:?}"
        );
    }

    #[test]
    fn test_mixed_spaces_and_tabs_in_admonition() {
        // 2 spaces + tab + 2 spaces (weird but possible)
        let content = "# Test\n\n!!! note\n  \t  Mixed indent.\n";
        // Should handle gracefully
        let _ = lint_mkdocs(content);
    }

    #[test]
    fn test_2_space_indent_admonition() {
        let content = "# Test\n\n!!! note\n  Two space indent.\n";
        // May or may not be recognized as admonition content
        let _ = lint_mkdocs(content);
    }

    #[test]
    fn test_8_space_indent_admonition() {
        let content = "# Test\n\n!!! note\n        Eight space indent.\n";
        // Extra indentation should still work
        let _ = lint_mkdocs(content);
    }

    #[test]
    fn test_nested_mixed_indentation() {
        let content = "# Test\n\n!!! note\n    === \"Tab\"\n\t    \tMixed deep.\n";
        // Complex mixed indentation
        let _ = lint_mkdocs(content);
    }
}

// =============================================================================
// PART 16: EXTENSIONS INSIDE BLOCKQUOTES
// Verify extensions work inside blockquote contexts
// =============================================================================

mod blockquote_tests {
    use super::*;

    #[test]
    fn test_inline_extensions_in_blockquote() {
        let content = "# Test\n\n> Quote with ==highlighted== text.\n";
        let warnings = lint_mkdocs(content);
        assert!(
            warnings.is_empty(),
            "Inline extensions in blockquote should work: {warnings:?}"
        );
    }

    #[test]
    fn test_keys_in_blockquote() {
        let content = "# Test\n\n> Press ++ctrl+c++ to copy.\n";
        let warnings = lint_mkdocs(content);
        assert!(warnings.is_empty(), "Keys in blockquote should work: {warnings:?}");
    }

    #[test]
    fn test_math_in_blockquote() {
        let content = "# Test\n\n> The equation $E = mc^2$ is famous.\n";
        let warnings = lint_mkdocs(content);
        assert!(warnings.is_empty(), "Math in blockquote should work: {warnings:?}");
    }

    #[test]
    fn test_critic_in_blockquote() {
        let content = "# Test\n\n> Text with {++addition++} here.\n";
        let warnings = lint_mkdocs(content);
        assert!(
            warnings.is_empty(),
            "Critic markup in blockquote should work: {warnings:?}"
        );
    }

    #[test]
    fn test_nested_blockquote_with_extensions() {
        let content = "# Test\n\n> Level 1\n> > Level 2 with ==mark==.\n";
        let warnings = lint_mkdocs(content);
        assert!(
            warnings.is_empty(),
            "Nested blockquote with extensions should work: {warnings:?}"
        );
    }

    #[test]
    fn test_multiline_blockquote_with_extensions() {
        let content = r#"# Test

> This is a blockquote.
> It has ==highlighted== text.
> And ++keyboard++ keys.
> Plus $math$ expressions.
"#;
        let warnings = lint_mkdocs(content);
        assert!(
            warnings.is_empty(),
            "Multiline blockquote with extensions should work: {warnings:?}"
        );
    }

    #[test]
    fn test_blockquote_admonition_interaction() {
        // Admonition syntax inside blockquote (unusual but possible)
        let content = "# Test\n\n> !!! note\n>     This is unusual.\n";
        // Should handle gracefully without panic
        let _ = lint_mkdocs(content);
    }
}

// =============================================================================
// PART 17: ESCAPE CHARACTER TESTS
// Verify extensions handle escaped characters correctly
// =============================================================================

mod escape_tests {
    use super::*;

    #[test]
    fn test_escaped_mark_syntax() {
        // \== should not be treated as mark start
        let content = "# Test\n\nThis is \\==not marked\\== text.\n";
        let warnings = lint_mkdocs(content);
        // Should handle gracefully
        let _ = warnings;
    }

    #[test]
    fn test_escaped_keys_syntax() {
        // \++ should not be treated as keys
        let content = "# Test\n\nThis is \\++not a key\\++ combo.\n";
        let _ = lint_mkdocs(content);
    }

    #[test]
    fn test_escaped_math_syntax() {
        // \$ should not start math
        let content = "# Test\n\nPrice is \\$100 dollars.\n";
        let warnings = lint_mkdocs(content);
        assert!(warnings.is_empty(), "Escaped dollar should work: {warnings:?}");
    }

    #[test]
    fn test_escaped_caret_syntax() {
        // \^ should not start superscript
        let content = "# Test\n\nUse \\^text\\^ for literal carets.\n";
        let _ = lint_mkdocs(content);
    }

    #[test]
    fn test_escaped_tilde_syntax() {
        // \~ should not start subscript
        let content = "# Test\n\nUse \\~text\\~ for literal tildes.\n";
        let _ = lint_mkdocs(content);
    }

    #[test]
    fn test_backslash_in_admonition_title() {
        let content = "# Test\n\n!!! note \"Path: C:\\\\Users\"\n    Content.\n";
        let warnings = lint_mkdocs(content);
        assert!(
            warnings.is_empty(),
            "Backslash in admonition title should work: {warnings:?}"
        );
    }

    #[test]
    fn test_backslash_in_tab_title() {
        let content = "# Test\n\n=== \"C:\\\\Path\"\n\n    Content.\n";
        let warnings = lint_mkdocs(content);
        assert!(warnings.is_empty(), "Backslash in tab title should work: {warnings:?}");
    }

    #[test]
    fn test_special_chars_in_code_spans() {
        let content = "# Test\n\nUse `==` for mark and `++` for keys.\n";
        let warnings = lint_mkdocs(content);
        assert!(
            warnings.is_empty(),
            "Special chars in code spans should work: {warnings:?}"
        );
    }

    #[test]
    fn test_html_entities_with_extensions() {
        let content = "# Test\n\nText with &amp; ==marked== &lt;content&gt;.\n";
        let warnings = lint_mkdocs(content);
        assert!(
            warnings.is_empty(),
            "HTML entities with extensions should work: {warnings:?}"
        );
    }
}

// =============================================================================
// PART 18: MALFORMED EXTENSION RECOVERY TESTS
// Verify graceful handling of broken/invalid extension syntax
// =============================================================================

mod malformed_tests {
    use super::*;

    #[test]
    fn test_unclosed_admonition() {
        // Admonition without content
        let content = "# Test\n\n!!! note\n\nNext paragraph.\n";
        // Should handle gracefully
        let _ = lint_mkdocs(content);
    }

    #[test]
    fn test_admonition_wrong_indent() {
        // Content not indented properly
        let content = "# Test\n\n!!! note\nNot indented.\n";
        // Should handle gracefully
        let _ = lint_mkdocs(content);
    }

    #[test]
    fn test_tab_missing_quotes() {
        // Tab without quotes around title
        let content = "# Test\n\n=== Tab\n    Content.\n";
        // Should handle gracefully
        let _ = lint_mkdocs(content);
    }

    #[test]
    fn test_tab_unclosed_quote() {
        // Tab with unclosed quote
        let content = "# Test\n\n=== \"Tab\n    Content.\n";
        // Should not panic
        let _ = lint_mkdocs(content);
    }

    #[test]
    fn test_mkdocstrings_no_path() {
        // ::: without path
        let content = "# Test\n\n:::\n";
        // Should handle gracefully
        let _ = lint_mkdocs(content);
    }

    #[test]
    fn test_unclosed_mark() {
        // == without closing
        let content = "# Test\n\nThis is ==unclosed mark.\n";
        // Should not panic
        let _ = lint_mkdocs(content);
    }

    #[test]
    fn test_unclosed_keys() {
        // ++ without closing
        let content = "# Test\n\nPress ++ctrl+c to copy.\n";
        // Should not panic
        let _ = lint_mkdocs(content);
    }

    #[test]
    fn test_unclosed_math() {
        // $ without closing
        let content = "# Test\n\nEquation $x + y without closing.\n";
        // Should not panic
        let _ = lint_mkdocs(content);
    }

    #[test]
    fn test_unclosed_superscript() {
        // ^ without closing
        let content = "# Test\n\nText with ^unclosed super.\n";
        // Should not panic
        let _ = lint_mkdocs(content);
    }

    #[test]
    fn test_unclosed_subscript() {
        // ~ without closing
        let content = "# Test\n\nH~2 without closing O.\n";
        // Should not panic
        let _ = lint_mkdocs(content);
    }

    #[test]
    fn test_nested_unclosed_extensions() {
        // Multiple unclosed
        let content = "# Test\n\n==mark with ^super and ~sub all unclosed.\n";
        // Should not panic
        let _ = lint_mkdocs(content);
    }

    #[test]
    fn test_mismatched_extension_markers() {
        // Different opening and closing
        let content = "# Test\n\nThis ==opens but ^^ closes wrong.\n";
        // Should not panic
        let _ = lint_mkdocs(content);
    }

    #[test]
    fn test_empty_extension_markers() {
        // Empty content between markers
        let content = "# Test\n\nEmpty ==== mark and ++++ keys.\n";
        // Should not panic
        let _ = lint_mkdocs(content);
    }

    #[test]
    fn test_only_markers_no_content() {
        // Just markers
        let content = "# Test\n\n== ++ ^^ ~~ ::\n";
        // Should not panic
        let _ = lint_mkdocs(content);
    }

    #[test]
    fn test_deeply_broken_nesting() {
        let content = r#"# Test

!!! note
    === "Tab
        !!! warning
            Content without proper closing.

    === "Another
        More broken.
"#;
        // Should not panic even with severely broken nesting
        let _ = lint_mkdocs(content);
    }

    #[test]
    fn test_interleaved_broken_extensions() {
        let content = "# Test\n\nStart ==mark then ^super then ~sub all ==cross^crossed~.\n";
        // Should not panic
        let _ = lint_mkdocs(content);
    }
}

// =============================================================================
// PART 19: EDGE POSITION TESTS
// Verify extensions at unusual document positions
// =============================================================================

mod position_tests {
    use super::*;

    #[test]
    fn test_extension_as_first_content() {
        // Extension as very first thing (no heading)
        let content = "!!! note\n    First thing in document.\n";
        // May have warnings but should not panic
        let _ = lint_mkdocs(content);
    }

    #[test]
    fn test_extension_at_eof_no_newline() {
        let content = "# Test\n\n!!! note\n    No trailing newline";
        let _ = lint_mkdocs(content);
    }

    #[test]
    fn test_inline_extension_at_line_start() {
        let content = "# Test\n\n==highlighted== starts the line.\n";
        let warnings = lint_mkdocs(content);
        assert!(warnings.is_empty(), "Extension at line start should work: {warnings:?}");
    }

    #[test]
    fn test_inline_extension_at_line_end() {
        let content = "# Test\n\nLine ends with ==highlighted==\n";
        let warnings = lint_mkdocs(content);
        assert!(warnings.is_empty(), "Extension at line end should work: {warnings:?}");
    }

    #[test]
    fn test_extension_only_line() {
        let content = "# Test\n\n==only content==\n";
        let warnings = lint_mkdocs(content);
        assert!(warnings.is_empty(), "Extension-only line should work: {warnings:?}");
    }

    #[test]
    fn test_extension_after_many_blank_lines() {
        let content = "# Test\n\n\n\n\n\n!!! note\n    After many blanks.\n";
        // MD012 may fire, but extension should still work
        let warnings = lint_mkdocs(content);
        let extension_issues: Vec<_> = warnings
            .iter()
            .filter(|w| w.rule_name.as_deref() != Some("MD012"))
            .collect();
        assert!(
            extension_issues.is_empty(),
            "Extension after blanks should work: {extension_issues:?}"
        );
    }

    #[test]
    fn test_extension_between_headings() {
        let content = "# Heading 1\n\n!!! note\n    Between headings.\n\n## Heading 2\n";
        let warnings = lint_mkdocs(content);
        assert!(
            warnings.is_empty(),
            "Extension between headings should work: {warnings:?}"
        );
    }

    #[test]
    fn test_extension_in_list_item() {
        let content = "# Test\n\n- Item with ==mark== inside.\n- Another with ++key++.\n";
        let warnings = lint_mkdocs(content);
        assert!(
            warnings.is_empty(),
            "Extensions in list items should work: {warnings:?}"
        );
    }

    #[test]
    fn test_extension_after_code_block() {
        let content = "# Test\n\n```python\ncode()\n```\n\n!!! note\n    After code.\n";
        let warnings = lint_mkdocs(content);
        assert!(
            warnings.is_empty(),
            "Extension after code block should work: {warnings:?}"
        );
    }

    #[test]
    fn test_extension_before_code_block() {
        let content = "# Test\n\n!!! note\n    Before code.\n\n```python\ncode()\n```\n";
        let warnings = lint_mkdocs(content);
        assert!(
            warnings.is_empty(),
            "Extension before code block should work: {warnings:?}"
        );
    }
}

// =============================================================================
// PART 20: COMPREHENSIVE INTEGRATION TESTS
// Full document tests combining multiple aspects
// =============================================================================

mod integration_tests {
    use super::*;

    #[test]
    fn test_real_world_api_docs() {
        let content = r#"---
title: API Reference
---

# API Reference

[TOC]

## Overview

This module provides the core functionality.

!!! warning "Deprecation Notice"
    The old API is deprecated. Use the new one.

## Classes

::: mypackage.core.Client
    options:
        show_source: true
        members:
            - connect
            - disconnect

### Usage

=== "Basic"

    ```python
    client = Client()
    client.connect()
    ```

=== "Advanced"

    ```python
    client = Client(timeout=30)
    client.connect(retry=True)
    ```

## Keyboard Shortcuts

| Action | Shortcut |
|--------|----------|
| Copy   | ++ctrl+c++ |
| Paste  | ++ctrl+v++ |
| Save   | ++ctrl+s++ |

## Math Examples

The quadratic formula: $x = \frac{-b \pm \sqrt{b^2-4ac}}{2a}$

## See Also

- [OtherClass][] for related functionality.

*[API]: Application Programming Interface

[^1]: Additional reference.
"#;
        let warnings = lint_mkdocs(content);
        // Real-world doc should have minimal warnings
        assert!(
            warnings.len() <= 2,
            "Real-world API docs should work well: {warnings:?}"
        );
    }

    #[test]
    fn test_real_world_tutorial() {
        let content = r#"# Getting Started Tutorial

!!! info "Prerequisites"
    - Python 3.8+
    - pip installed

## Installation

=== "pip"

    ```bash
    pip install mypackage
    ```

=== "poetry"

    ```bash
    poetry add mypackage
    ```

=== "conda"

    ```bash
    conda install mypackage
    ```

## First Steps

1. Import the module
2. Create a client
3. Connect to the server

??? example "Complete Example"

    ```python
    from mypackage import Client

    client = Client()
    client.connect()
    print("Connected!")
    ```

## Tips

!!! tip
    Use ++ctrl+c++ to interrupt long-running operations.

!!! warning
    Always call `disconnect()` when done.

## Next Steps

See the [API Reference](api.md) for details.
"#;
        let warnings = lint_mkdocs(content);
        assert!(warnings.is_empty(), "Tutorial should have no warnings: {warnings:?}");
    }

    #[test]
    fn test_real_world_changelog() {
        let content = r#"# Changelog

## [2.0.0] - 2024-01-15

!!! danger "Breaking Changes"
    - Removed deprecated `old_function()`
    - Changed signature of `process()`

### Added

- New ==highlighted== feature
- Support for ^superscript^ text

### Fixed

- Bug in {~~old~>new~~} handling

## [1.5.0] - 2024-01-01

??? note "Migration Guide"
    Follow these steps to upgrade:

    1. Update dependencies
    2. Run migrations
    3. Test thoroughly
"#;
        let warnings = lint_mkdocs(content);
        assert!(warnings.is_empty(), "Changelog should have no warnings: {warnings:?}");
    }

    #[test]
    fn test_stress_all_extensions_combined() {
        let content = r#"---
title: Complete Test
---

# Complete Extension Test

[TOC]

*[HTML]: Hypertext Markup Language

Text with ==mark==, ^super^, ~sub~, ++key++, $math$, and :emoji:.

!!! note "Admonition"

    === "Tab 1"

        ```python title="code.py"
        print("Hello")
        ```

    === "Tab 2"

        ::: module.Class

??? tip "Collapsible"

    Content with {++critic++} markup.

> Blockquote with ==extensions==.

Term
:   Definition with ++keys++.

| Header | Value |
|--------|-------|
| Key    | ++v++ |

$$
E = mc^2
$$

--8<-- "snippet.md"

[Reference][] link.

[^1]: Footnote.
"#;
        let warnings = lint_mkdocs(content);
        // With all extensions, expect very few warnings
        assert!(
            warnings.len() <= 3,
            "Complete test should have minimal warnings: {warnings:?}"
        );
    }
}
