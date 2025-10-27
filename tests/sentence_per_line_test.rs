use rumdl_lib::lint_context::LintContext;
use rumdl_lib::rule::Rule;
use rumdl_lib::rules::MD013LineLength;
use rumdl_lib::rules::md013_line_length::md013_config::{MD013Config, ReflowMode};

fn create_sentence_per_line_rule() -> MD013LineLength {
    MD013LineLength::from_config_struct(MD013Config {
        line_length: 80,
        code_blocks: false,
        tables: false,
        headings: false,
        paragraphs: true, // Default: check paragraphs
        strict: false,
        reflow: true,
        reflow_mode: ReflowMode::SentencePerLine,
    })
}

#[test]
fn test_sentence_per_line_detection() {
    let rule = create_sentence_per_line_rule();
    let content = "This is the first sentence. This is the second sentence. And this is the third.";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard);
    let result = rule.check(&ctx).unwrap();

    // Should detect violations on lines with multiple sentences
    assert!(!result.is_empty(), "Should detect multiple sentences on one line");
    assert_eq!(result[0].message, "Line contains multiple sentences");
}

#[test]
fn test_single_sentence_no_warning() {
    let rule = create_sentence_per_line_rule();
    let content = "This is a single sentence that should not trigger a warning.";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard);
    let result = rule.check(&ctx).unwrap();

    assert!(result.is_empty(), "Single sentence should not trigger warning");
}

#[test]
fn test_abbreviations_not_split() {
    let rule = create_sentence_per_line_rule();
    let content = "Mr. Smith met Dr. Jones at 3.14 PM.";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard);
    let result = rule.check(&ctx).unwrap();

    // Should not break at abbreviations or decimal numbers
    assert!(
        result.is_empty(),
        "Abbreviations should not be treated as sentence boundaries"
    );
}

#[test]
fn test_question_and_exclamation_marks() {
    let rule = create_sentence_per_line_rule();
    let content = "Is this a question? Yes it is! And this is another statement.";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard);
    let result = rule.check(&ctx).unwrap();

    assert!(
        !result.is_empty(),
        "Should detect multiple sentences with ? and ! marks"
    );
    assert_eq!(result.len(), 1);
}

#[test]
fn test_sentence_per_line_fix() {
    let rule = create_sentence_per_line_rule();
    let content = "First sentence. Second sentence.";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard);
    let result = rule.check(&ctx).unwrap();

    assert!(!result.is_empty());
    assert!(result[0].fix.is_some());

    let fix = result[0].fix.as_ref().unwrap();
    assert_eq!(fix.replacement.trim(), "First sentence.\nSecond sentence.");
}

#[test]
fn test_markdown_elements_preserved_in_fix() {
    let rule = create_sentence_per_line_rule();
    let content = "This has **bold text**. And this has [a link](https://example.com).";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard);
    let result = rule.check(&ctx).unwrap();

    assert!(!result.is_empty());
    assert!(result[0].fix.is_some());

    let fix = result[0].fix.as_ref().unwrap();
    assert_eq!(
        fix.replacement.trim(),
        "This has **bold text**.\nAnd this has [a link](https://example.com)."
    );
}

#[test]
fn test_multiple_paragraphs() {
    let rule = create_sentence_per_line_rule();
    let content = "First paragraph. With two sentences.\n\nSecond paragraph. Also with two.";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard);
    let result = rule.check(&ctx).unwrap();

    // Should detect violations in both paragraphs
    assert_eq!(result.len(), 2, "Should detect violations in both paragraphs");
}

#[test]
fn test_issue_124_paragraph_with_backticks() {
    // Test case from issue #124: paragraph with backticks not being detected
    let rule = create_sentence_per_line_rule();
    let content = "If you are sure that all data structures exposed in a `PyModule` are\n\
                   thread-safe, then pass `gil_used = false` as a parameter to the\n\
                   `pymodule` procedural macro declaring the module or call\n\
                   `PyModule::gil_used` on a `PyModule` instance.  For example:";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard);
    let result = rule.check(&ctx).unwrap();

    // This paragraph has at least two sentences - should be detected
    assert!(
        !result.is_empty(),
        "Should detect multiple sentences in paragraph with backticks"
    );
}

#[test]
fn test_issue_124_second_paragraph() {
    // Test case from issue #124: second example paragraph
    let rule = create_sentence_per_line_rule();
    let content = "This document provides advice for porting Rust code using PyO3 to run under\n\
                   free-threaded Python.";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard);
    let result = rule.check(&ctx).unwrap();

    // This is a single sentence on multiple lines - should NOT be detected
    // (single long sentences don't violate sentence-per-line, only multiple sentences do)
    assert!(
        result.is_empty(),
        "Single sentence on multiple lines should not trigger sentence-per-line warning"
    );
}
