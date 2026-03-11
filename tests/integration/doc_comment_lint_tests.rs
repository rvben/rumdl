/// Integration tests for Rust doc comment linting.
///
/// Tests extraction, checking, and fix restoration of markdown
/// embedded in `///` and `//!` doc comments.
use rumdl_lib::config::Config;
use rumdl_lib::doc_comment_lint::{DocCommentKind, check_doc_comment_blocks, extract_doc_comment_blocks};
use rumdl_lib::rule::Rule;
use rumdl_lib::rules;

/// Helper to create a default set of rules for testing.
fn default_rules() -> Vec<Box<dyn Rule>> {
    let config = Config::default();
    rules::all_rules(&config)
}

// ─── Extraction tests ───────────────────────────────────────────

#[test]
fn test_extract_basic_outer_doc_comment() {
    let content = "/// A simple function.\n/// It does things.\nfn foo() {}\n";
    let blocks = extract_doc_comment_blocks(content);

    assert_eq!(blocks.len(), 1);
    assert_eq!(blocks[0].kind, DocCommentKind::Outer);
    assert_eq!(blocks[0].start_line, 0);
    assert_eq!(blocks[0].end_line, 1);
    assert_eq!(blocks[0].markdown, "A simple function.\nIt does things.");
}

#[test]
fn test_extract_basic_inner_doc_comment() {
    let content = "//! Crate-level documentation.\n//! Second line.\n\nuse std::io;\n";
    let blocks = extract_doc_comment_blocks(content);

    assert_eq!(blocks.len(), 1);
    assert_eq!(blocks[0].kind, DocCommentKind::Inner);
    assert_eq!(blocks[0].markdown, "Crate-level documentation.\nSecond line.");
}

#[test]
fn test_extract_multiple_separate_blocks() {
    let content = "\
/// Block one.
fn foo() {}

/// Block two.
/// Second line of block two.
fn bar() {}
";
    let blocks = extract_doc_comment_blocks(content);

    assert_eq!(blocks.len(), 2);
    assert_eq!(blocks[0].markdown, "Block one.");
    assert_eq!(blocks[0].start_line, 0);
    assert_eq!(blocks[1].markdown, "Block two.\nSecond line of block two.");
    assert_eq!(blocks[1].start_line, 3);
}

#[test]
fn test_extract_mixed_outer_inner_separate_blocks() {
    let content = "//! Module doc\n/// Struct doc\nstruct Foo;\n";
    let blocks = extract_doc_comment_blocks(content);

    assert_eq!(blocks.len(), 2);
    assert_eq!(blocks[0].kind, DocCommentKind::Inner);
    assert_eq!(blocks[0].markdown, "Module doc");
    assert_eq!(blocks[1].kind, DocCommentKind::Outer);
    assert_eq!(blocks[1].markdown, "Struct doc");
}

#[test]
fn test_extract_empty_doc_comment_lines() {
    let content = "/// First paragraph.\n///\n/// Second paragraph.\n";
    let blocks = extract_doc_comment_blocks(content);

    assert_eq!(blocks.len(), 1);
    assert_eq!(blocks[0].markdown, "First paragraph.\n\nSecond paragraph.");
}

#[test]
fn test_extract_indented_doc_comments() {
    let content = "\
impl Foo {
    /// Method documentation.
    /// More details.
    fn bar(&self) {}
}
";
    let blocks = extract_doc_comment_blocks(content);

    assert_eq!(blocks.len(), 1);
    assert_eq!(blocks[0].markdown, "Method documentation.\nMore details.");
    assert_eq!(blocks[0].line_metadata[0].leading_whitespace, "    ");
}

#[test]
fn test_extract_preserves_extra_space() {
    let content = "///  Two leading spaces preserved.\n";
    let blocks = extract_doc_comment_blocks(content);

    assert_eq!(blocks.len(), 1);
    assert_eq!(blocks[0].markdown, " Two leading spaces preserved.");
}

#[test]
fn test_four_slashes_not_doc_comment() {
    let content = "//// This is not a doc comment\nfn foo() {}\n";
    let blocks = extract_doc_comment_blocks(content);

    assert!(blocks.is_empty());
}

#[test]
fn test_regular_comment_not_doc_comment() {
    let content = "// Regular comment\nfn foo() {}\n";
    let blocks = extract_doc_comment_blocks(content);

    assert!(blocks.is_empty());
}

#[test]
fn test_blank_line_ends_block() {
    let content = "/// Block 1\n\n/// Block 2\n";
    let blocks = extract_doc_comment_blocks(content);

    assert_eq!(blocks.len(), 2);
    assert_eq!(blocks[0].markdown, "Block 1");
    assert_eq!(blocks[1].markdown, "Block 2");
}

#[test]
fn test_code_between_blocks_separates_them() {
    let content = "/// First\nlet x = 1;\n/// Second\n";
    let blocks = extract_doc_comment_blocks(content);

    assert_eq!(blocks.len(), 2);
}

#[test]
fn test_no_doc_comments_returns_empty() {
    let content = "fn main() {\n    println!(\"hello\");\n}\n";
    let blocks = extract_doc_comment_blocks(content);

    assert!(blocks.is_empty());
}

#[test]
fn test_empty_file() {
    let blocks = extract_doc_comment_blocks("");
    assert!(blocks.is_empty());
}

// ─── Check (linting) tests ─────────────────────────────────────

#[test]
fn test_check_no_warnings_for_clean_doc() {
    let content = "\
/// # Example
///
/// This is clean markdown.
fn foo() {}
";
    let rules = default_rules();
    let config = Config::default();
    let warnings = check_doc_comment_blocks(content, &rules, &config);

    // Clean doc comments should produce no warnings (MD041/MD047 are already
    // skipped by the check function, so we just check for unexpected violations)
    assert!(
        warnings.is_empty(),
        "Clean doc comments should produce no warnings, but got: {warnings:?}"
    );
}

#[test]
fn test_check_skips_md041() {
    // MD041 requires first line to be a heading, which doesn't apply to doc comments
    let content = "/// Some text without a heading.\nfn foo() {}\n";
    let rules = default_rules();
    let config = Config::default();
    let warnings = check_doc_comment_blocks(content, &rules, &config);

    let md041_warnings: Vec<_> = warnings
        .iter()
        .filter(|w| w.rule_name.as_deref() == Some("MD041"))
        .collect();
    assert!(md041_warnings.is_empty(), "MD041 should be skipped for doc comments");
}

#[test]
fn test_check_skips_md047() {
    // MD047 requires file to end with newline, which doesn't apply to doc comments
    let content = "/// No trailing newline\nfn foo() {}\n";
    let rules = default_rules();
    let config = Config::default();
    let warnings = check_doc_comment_blocks(content, &rules, &config);

    let md047_warnings: Vec<_> = warnings
        .iter()
        .filter(|w| w.rule_name.as_deref() == Some("MD047"))
        .collect();
    assert!(md047_warnings.is_empty(), "MD047 should be skipped for doc comments");
}

#[test]
fn test_check_detects_trailing_spaces() {
    // MD009: trailing spaces
    let content = "/// Line with trailing spaces   \nfn foo() {}\n";
    let rules = default_rules();
    let config = Config::default();
    let warnings = check_doc_comment_blocks(content, &rules, &config);

    let md009_warnings: Vec<_> = warnings
        .iter()
        .filter(|w| w.rule_name.as_deref() == Some("MD009"))
        .collect();
    assert!(
        !md009_warnings.is_empty(),
        "MD009 should detect trailing spaces in doc comments"
    );
    // Line 1 in the file (0-indexed start_line=0, warning line=1 in block → file line=1)
    assert_eq!(md009_warnings[0].line, 1);
}

#[test]
fn test_check_line_numbers_remapped_correctly() {
    // Put the doc comment after some code so start_line > 0
    // Use explicit trailing spaces that won't be trimmed by the editor
    let content = "use std::io;\n\n/// Line with trailing spaces   \nfn foo() {}\n";
    let rules = default_rules();
    let config = Config::default();
    let warnings = check_doc_comment_blocks(content, &rules, &config);

    let md009_warnings: Vec<_> = warnings
        .iter()
        .filter(|w| w.rule_name.as_deref() == Some("MD009"))
        .collect();
    assert!(
        !md009_warnings.is_empty(),
        "Expected MD009 warnings but found none. All warnings: {:?}",
        warnings.iter().map(|w| &w.rule_name).collect::<Vec<_>>()
    );
    // The doc comment is on line index 2 (0-indexed), so the warning should be on line 3 (1-indexed)
    assert_eq!(md009_warnings[0].line, 3);
}

#[test]
fn test_check_multiple_blocks_independent() {
    let content = "\
/// # Block One
///
/// Clean block.
fn foo() {}

/// # Block Two
///
/// Also clean.
fn bar() {}
";
    let rules = default_rules();
    let config = Config::default();
    let warnings = check_doc_comment_blocks(content, &rules, &config);

    // Both blocks are clean — should produce no warnings
    assert!(
        warnings.is_empty(),
        "Clean doc comment blocks should produce no warnings, but got: {warnings:?}"
    );
}

#[test]
fn test_check_heading_increment_violation() {
    // MD001: heading levels should increment by one
    let content = "/// # Heading 1\n///\n/// ### Heading 3\nfn foo() {}\n";
    let rules = default_rules();
    let config = Config::default();
    let warnings = check_doc_comment_blocks(content, &rules, &config);

    let md001_warnings: Vec<_> = warnings
        .iter()
        .filter(|w| w.rule_name.as_deref() == Some("MD001"))
        .collect();
    assert!(
        !md001_warnings.is_empty(),
        "MD001 should detect heading level skip in doc comments"
    );
    // The heading is on the 3rd line of the block (index 2), file line index 2 (0-indexed)
    // So warning should be at file line 3 (1-indexed)
    assert_eq!(md001_warnings[0].line, 3);
}

#[test]
fn test_check_fixes_are_stripped() {
    // Fixes should be None in check mode (only used in fix mode path)
    let content = "/// trailing spaces   \nfn foo() {}\n";
    let rules = default_rules();
    let config = Config::default();
    let warnings = check_doc_comment_blocks(content, &rules, &config);

    for warning in &warnings {
        assert!(
            warning.fix.is_none(),
            "Fixes should be stripped in check mode, but found fix for {:?}",
            warning.rule_name
        );
    }
}

#[test]
fn test_check_inner_doc_comment_linting() {
    // Inner doc comments should also be linted
    let content = "//! trailing spaces   \n\nuse std::io;\n";
    let rules = default_rules();
    let config = Config::default();
    let warnings = check_doc_comment_blocks(content, &rules, &config);

    let md009_warnings: Vec<_> = warnings
        .iter()
        .filter(|w| w.rule_name.as_deref() == Some("MD009"))
        .collect();
    assert!(
        !md009_warnings.is_empty(),
        "MD009 should detect trailing spaces in inner doc comments"
    );
    assert_eq!(md009_warnings[0].line, 1);
}

// ─── Rustdoc-specific rule skipping ─────────────────────────────

#[test]
fn test_check_skips_md025_multiple_h1_headings() {
    // Rustdoc conventionally uses multiple H1 headings: # Examples, # Errors, # Safety, # Panics
    let content = "\
/// # Examples
///
/// ```
/// let x = 1;
/// ```
///
/// # Errors
///
/// Returns an error if the input is invalid.
///
/// # Panics
///
/// Panics if the lock is poisoned.
pub fn example() {}
";
    let rules = default_rules();
    let config = Config::default();
    let warnings = check_doc_comment_blocks(content, &rules, &config);

    let md025_warnings: Vec<_> = warnings
        .iter()
        .filter(|w| w.rule_name.as_deref() == Some("MD025"))
        .collect();
    assert!(
        md025_warnings.is_empty(),
        "MD025 should be skipped for doc comments (multiple H1s are standard in rustdoc)"
    );
}

#[test]
fn test_check_skips_md033_html_warning_block() {
    // Rustdoc requires <div class="warning"> for warning blocks
    let content = "\
/// # Safety
///
/// <div class=\"warning\">
///
/// This function is unsafe because it dereferences a raw pointer.
///
/// </div>
pub unsafe fn deref_ptr() {}
";
    let rules = default_rules();
    let config = Config::default();
    let warnings = check_doc_comment_blocks(content, &rules, &config);

    let md033_warnings: Vec<_> = warnings
        .iter()
        .filter(|w| w.rule_name.as_deref() == Some("MD033"))
        .collect();
    assert!(
        md033_warnings.is_empty(),
        "MD033 should be skipped for doc comments (HTML tags are required for rustdoc warning blocks)"
    );
}

#[test]
fn test_check_skips_md040_unlabeled_code_blocks() {
    // Rustdoc assumes unlabeled code blocks are Rust code
    let content = "\
/// # Examples
///
/// ```
/// let x = 42;
/// assert_eq!(x, 42);
/// ```
pub fn example() {}
";
    let rules = default_rules();
    let config = Config::default();
    let warnings = check_doc_comment_blocks(content, &rules, &config);

    let md040_warnings: Vec<_> = warnings
        .iter()
        .filter(|w| w.rule_name.as_deref() == Some("MD040"))
        .collect();
    assert!(
        md040_warnings.is_empty(),
        "MD040 should be skipped for doc comments (rustdoc defaults unlabeled code blocks to Rust)"
    );
}

#[test]
fn test_check_skips_md051_rustdoc_anchors() {
    // Rustdoc generates anchors like #method.bar, #structfield.name that aren't headings
    let content = "\
/// See [`Foo`](#method.bar) for details.
///
/// Also check [`field`](#structfield.name).
pub fn example() {}
";
    let rules = default_rules();
    let config = Config::default();
    let warnings = check_doc_comment_blocks(content, &rules, &config);

    let md051_warnings: Vec<_> = warnings
        .iter()
        .filter(|w| w.rule_name.as_deref() == Some("MD051"))
        .collect();
    assert!(
        md051_warnings.is_empty(),
        "MD051 should be skipped for doc comments (rustdoc anchors aren't document headings)"
    );
}

#[test]
fn test_check_skips_md052_intra_doc_links() {
    // Intra-doc links like [crate::io] are valid rustdoc syntax, not broken references
    let content = "\
/// See [crate::io::Read] for the trait definition.
///
/// Also see [super::parent_module].
pub fn example() {}
";
    let rules = default_rules();
    let config = Config::default();
    let warnings = check_doc_comment_blocks(content, &rules, &config);

    let md052_warnings: Vec<_> = warnings
        .iter()
        .filter(|w| w.rule_name.as_deref() == Some("MD052"))
        .collect();
    assert!(
        md052_warnings.is_empty(),
        "MD052 should be skipped for doc comments (intra-doc links are rustdoc syntax)"
    );
}

#[test]
fn test_check_skips_md054_shortcut_intra_doc_links() {
    // Shortcut reference links like [crate::module] are the canonical intra-doc link syntax
    let content = "\
/// See [crate::io::Read] for details.
///
/// Also uses [std::fmt::Display].
pub fn example() {}
";
    let rules = default_rules();
    let config = Config::default();
    let warnings = check_doc_comment_blocks(content, &rules, &config);

    let md054_warnings: Vec<_> = warnings
        .iter()
        .filter(|w| w.rule_name.as_deref() == Some("MD054"))
        .collect();
    assert!(
        md054_warnings.is_empty(),
        "MD054 should be skipped for doc comments (shortcut style is canonical for intra-doc links)"
    );
}

#[test]
fn test_check_non_skipped_rules_still_fire() {
    // Ensure rules that aren't in SKIPPED_RULES still detect issues
    // MD009: trailing spaces should still be caught
    let content = "/// trailing spaces   \npub fn example() {}\n";
    let rules = default_rules();
    let config = Config::default();
    let warnings = check_doc_comment_blocks(content, &rules, &config);

    let md009_warnings: Vec<_> = warnings
        .iter()
        .filter(|w| w.rule_name.as_deref() == Some("MD009"))
        .collect();
    assert!(
        !md009_warnings.is_empty(),
        "Non-skipped rules like MD009 should still detect issues in doc comments"
    );
}

// ─── Edge cases ─────────────────────────────────────────────────

#[test]
fn test_extract_doc_comment_with_code_block() {
    let content = "\
/// # Examples
///
/// ```rust
/// let x = 42;
/// ```
fn foo() {}
";
    let blocks = extract_doc_comment_blocks(content);

    assert_eq!(blocks.len(), 1);
    assert!(blocks[0].markdown.contains("```rust"));
    assert!(blocks[0].markdown.contains("let x = 42;"));
}

#[test]
fn test_extract_tab_indentation() {
    let content = "\t/// Tab indented\n\t/// More\n";
    let blocks = extract_doc_comment_blocks(content);

    assert_eq!(blocks.len(), 1);
    assert_eq!(blocks[0].markdown, "Tab indented\nMore");
    assert_eq!(blocks[0].line_metadata[0].leading_whitespace, "\t");
}

#[test]
fn test_extract_only_bare_prefix() {
    // A line with just `///` and nothing else
    let content = "///\n";
    let blocks = extract_doc_comment_blocks(content);

    assert_eq!(blocks.len(), 1);
    assert_eq!(blocks[0].markdown, "");
}

#[test]
fn test_extract_deeply_nested_indentation() {
    let content = "\
mod outer {
    mod inner {
        /// Deeply nested.
        fn deep() {}
    }
}
";
    let blocks = extract_doc_comment_blocks(content);

    assert_eq!(blocks.len(), 1);
    assert_eq!(blocks[0].markdown, "Deeply nested.");
    assert_eq!(blocks[0].line_metadata[0].leading_whitespace, "        ");
}

#[test]
fn test_extract_consecutive_different_kinds() {
    // Inner followed immediately by outer → two separate blocks
    let content = "//! Inner line 1\n//! Inner line 2\n/// Outer line 1\n/// Outer line 2\n";
    let blocks = extract_doc_comment_blocks(content);

    assert_eq!(blocks.len(), 2);
    assert_eq!(blocks[0].kind, DocCommentKind::Inner);
    assert_eq!(blocks[0].markdown, "Inner line 1\nInner line 2");
    assert_eq!(blocks[1].kind, DocCommentKind::Outer);
    assert_eq!(blocks[1].markdown, "Outer line 1\nOuter line 2");
}

#[test]
fn test_regular_comment_between_doc_comments() {
    let content = "/// Block 1\n// regular comment\n/// Block 2\n";
    let blocks = extract_doc_comment_blocks(content);

    assert_eq!(blocks.len(), 2, "Regular comment should end a block");
}

#[test]
fn test_extract_byte_offsets_accurate() {
    let content = "/// Hello\nfn foo() {}\n/// World\n";
    let blocks = extract_doc_comment_blocks(content);

    assert_eq!(blocks.len(), 2);

    // First block "/// Hello\n" is 10 bytes
    assert_eq!(blocks[0].byte_start, 0);
    assert_eq!(blocks[0].byte_end, 10);

    // "fn foo() {}\n" is 12 bytes, so second block starts at 22
    assert_eq!(blocks[1].byte_start, 22);
    assert_eq!(blocks[1].byte_end, 32); // "/// World\n" = 10 bytes
}

#[test]
fn test_check_column_numbers_remapped() {
    // MD009 reports trailing spaces at the column where trailing spaces start.
    // For "/// Line with trailing spaces   ", the prefix "/// " is 4 bytes,
    // so the column should be offset by 4.
    let content = "/// trailing spaces   \nfn foo() {}\n";
    let rules = default_rules();
    let config = Config::default();
    let warnings = check_doc_comment_blocks(content, &rules, &config);

    let md009_warnings: Vec<_> = warnings
        .iter()
        .filter(|w| w.rule_name.as_deref() == Some("MD009"))
        .collect();
    assert!(!md009_warnings.is_empty(), "Expected MD009 for trailing spaces");
    // Column should be > 1 (offset by the prefix length)
    assert!(
        md009_warnings[0].column > 1,
        "Column should be remapped to account for prefix, got {}",
        md009_warnings[0].column
    );
}

#[test]
fn test_check_column_with_indentation() {
    // Indented doc comment: "    /// trailing   "
    // prefix_byte_length = 4 (indent) + 4 (prefix) = 8
    let content = "    /// trailing   \n    fn method() {}\n";
    let rules = default_rules();
    let config = Config::default();
    let warnings = check_doc_comment_blocks(content, &rules, &config);

    let md009_warnings: Vec<_> = warnings
        .iter()
        .filter(|w| w.rule_name.as_deref() == Some("MD009"))
        .collect();
    assert!(
        !md009_warnings.is_empty(),
        "Expected MD009 for trailing spaces in indented doc comment"
    );
    // Column should account for both indentation and prefix
    assert!(
        md009_warnings[0].column > 4,
        "Column should account for indentation + prefix, got {}",
        md009_warnings[0].column
    );
}

/// MD013 should not flag long lines inside code blocks in doc comments.
/// Code blocks contain Rust code formatted by rustfmt, not prose.
#[test]
fn test_md013_skips_code_blocks_in_doc_comments() {
    let content = r#"/// # Examples
///
/// ```
/// let very_long_variable_name_that_exceeds_eighty_characters = some_function_with_a_long_name(argument_one, argument_two);
/// ```
fn foo() {}
"#;

    let rules = default_rules();
    let config = Config::default();
    let warnings = check_doc_comment_blocks(content, &rules, &config);

    let md013_warnings: Vec<_> = warnings
        .iter()
        .filter(|w| w.rule_name.as_deref() == Some("MD013"))
        .collect();
    assert!(
        md013_warnings.is_empty(),
        "MD013 should not flag code blocks in doc comments, got: {md013_warnings:?}"
    );
}

/// MD013 should not flag long lines inside indented code blocks in doc comments.
#[test]
fn test_md013_skips_indented_code_blocks_in_doc_comments() {
    let content = "/// # Examples\n///\n///     let very_long_variable_name_that_exceeds_eighty_characters = some_function_with_a_long_name(argument_one, argument_two);\n///\nfn foo() {}\n";

    let rules = default_rules();
    let config = Config::default();
    let warnings = check_doc_comment_blocks(content, &rules, &config);

    let md013_warnings: Vec<_> = warnings
        .iter()
        .filter(|w| w.rule_name.as_deref() == Some("MD013"))
        .collect();
    assert!(
        md013_warnings.is_empty(),
        "MD013 should not flag indented code blocks in doc comments, got: {md013_warnings:?}"
    );
}

/// MD013 should still flag long prose lines in doc comments.
#[test]
fn test_md013_still_flags_long_prose_in_doc_comments() {
    let content = "/// This is a very long documentation line that definitely exceeds the default eighty character limit and should be flagged by MD013.\nfn foo() {}\n";

    let rules = default_rules();
    let config = Config::default();
    let warnings = check_doc_comment_blocks(content, &rules, &config);

    let md013_warnings: Vec<_> = warnings
        .iter()
        .filter(|w| w.rule_name.as_deref() == Some("MD013"))
        .collect();
    assert!(
        !md013_warnings.is_empty(),
        "MD013 should still flag long prose lines in doc comments"
    );
}
