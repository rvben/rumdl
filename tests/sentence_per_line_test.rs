use rumdl_lib::lint_context::LintContext;
use rumdl_lib::rule::Rule;
use rumdl_lib::rules::MD013LineLength;
use rumdl_lib::rules::md013_line_length::md013_config::{MD013Config, ReflowMode};
use rumdl_lib::types::LineLength;

fn create_sentence_per_line_rule() -> MD013LineLength {
    MD013LineLength::from_config_struct(MD013Config {
        line_length: LineLength::from_const(80),
        code_blocks: false,
        tables: false,
        headings: false,
        paragraphs: true, // Default: check paragraphs
        strict: false,
        reflow: true,
        reflow_mode: ReflowMode::SentencePerLine,
        length_mode: rumdl_lib::rules::md013_line_length::md013_config::LengthMode::default(),
        abbreviations: None,
    })
}

#[test]
fn test_sentence_per_line_detection() {
    let rule = create_sentence_per_line_rule();
    let content = "This is the first sentence. This is the second sentence. And this is the third.";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // Should detect violations on lines with multiple sentences
    assert!(!result.is_empty(), "Should detect multiple sentences on one line");
    assert_eq!(
        result[0].message,
        "Line contains 3 sentences (one sentence per line required)"
    );
}

#[test]
fn test_single_sentence_no_warning() {
    let rule = create_sentence_per_line_rule();
    let content = "This is a single sentence that should not trigger a warning.";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    assert!(result.is_empty(), "Single sentence should not trigger warning");
}

#[test]
fn test_abbreviations_not_split() {
    let rule = create_sentence_per_line_rule();
    let content = "Mr. Smith met Dr. Jones at 3.14 PM.";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // Should not break at abbreviations or decimal numbers
    assert!(
        result.is_empty(),
        "Abbreviations should not be treated as sentence boundaries"
    );
}

#[test]
fn test_titles_not_split() {
    let rule = create_sentence_per_line_rule();
    // Titles followed by names should NOT be treated as sentence boundaries
    let content = "Talk to Dr. Smith or Prof. Jones about the project.";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // Single sentence with titles - should not be split
    assert!(
        result.is_empty(),
        "Titles before names should not be treated as sentence boundaries"
    );
}

#[test]
fn test_question_and_exclamation_marks() {
    let rule = create_sentence_per_line_rule();
    let content = "Is this a question? Yes it is! And this is another statement.";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
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
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
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
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
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
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // Should detect violations in both paragraphs
    assert_eq!(result.len(), 2, "Should detect violations in both paragraphs");
}

#[test]
fn test_multi_sentence_paragraph_with_backticks() {
    // Paragraph with multiple sentences spanning multiple lines, containing inline code
    // Reported in issue #124
    let rule = create_sentence_per_line_rule();
    let content = "If you are sure that all data structures exposed in a `PyModule` are\n\
                   thread-safe, then pass `gil_used = false` as a parameter to the\n\
                   `pymodule` procedural macro declaring the module or call\n\
                   `PyModule::gil_used` on a `PyModule` instance.  For example:";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // This paragraph has at least two sentences - should be detected
    assert!(
        !result.is_empty(),
        "Should detect multiple sentences in paragraph with backticks"
    );
}

#[test]
fn test_single_sentence_exceeds_line_length() {
    // Single sentence spanning multiple lines that exceeds line-length constraint
    // This sentence is 85 chars when joined, so with line-length=80 it should NOT be reflowed
    // Reported in issue #124
    let rule = create_sentence_per_line_rule(); // Uses line_length: 80
    let content = "This document provides advice for porting Rust code using PyO3 to run under\n\
                   free-threaded Python.";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // Single sentence spanning multiple lines: should NOT be reflowed if it exceeds line-length
    assert!(
        result.is_empty(),
        "Single sentence exceeding line-length should not be reflowed"
    );
}

#[test]
fn test_single_sentence_with_no_line_length_constraint() {
    // Single sentence spanning multiple lines with line-length=0 (no constraint)
    // Should be joined into one line since there's no line-length limitation
    // Reported in issue #124
    let rule = MD013LineLength::from_config_struct(MD013Config {
        line_length: LineLength::from_const(0), // No line-length constraint
        code_blocks: false,
        tables: false,
        headings: false,
        paragraphs: true,
        strict: false,
        reflow: true,
        reflow_mode: ReflowMode::SentencePerLine,
        length_mode: rumdl_lib::rules::md013_line_length::md013_config::LengthMode::default(),
        abbreviations: None,
    });
    let content = "This document provides advice for porting Rust code using PyO3 to run under\n\
                   free-threaded Python.";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // With line-length=0, single sentences spanning multiple lines should be joined
    assert!(
        !result.is_empty(),
        "Single sentence should be joined when line-length=0"
    );
    assert_eq!(
        result[0].message,
        "Paragraph should have one sentence per line (found 1 sentences across 2 lines)"
    );

    // Verify the fix joins the sentence
    assert!(result[0].fix.is_some());
    let fix = result[0].fix.as_ref().unwrap();
    assert_eq!(
        fix.replacement.trim(),
        "This document provides advice for porting Rust code using PyO3 to run under free-threaded Python."
    );
}

#[test]
fn test_single_sentence_fits_within_line_length() {
    // Single sentence spanning multiple lines that DOES fit within line-length should be joined
    let rule = create_sentence_per_line_rule(); // Uses line_length: 80
    let content = "This is a short sentence that\nspans two lines.";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // This sentence is 46 chars, fits in 80, so should be joined
    assert!(
        !result.is_empty(),
        "Single sentence spanning multiple lines should be joined if it fits within line-length"
    );

    // Verify the fix joins the sentence
    assert!(result[0].fix.is_some());
    let fix = result[0].fix.as_ref().unwrap();
    assert_eq!(fix.replacement.trim(), "This is a short sentence that spans two lines.");
}

#[test]
fn test_custom_abbreviations_recognized() {
    // Test that custom abbreviations are recognized and don't split sentences
    // "Assn" is not a built-in abbreviation, so without configuration it would split
    let rule = MD013LineLength::from_config_struct(MD013Config {
        line_length: LineLength::from_const(0), // No line-length constraint
        code_blocks: false,
        tables: false,
        headings: false,
        paragraphs: true,
        strict: false,
        reflow: true,
        reflow_mode: ReflowMode::SentencePerLine,
        length_mode: rumdl_lib::rules::md013_line_length::md013_config::LengthMode::default(),
        abbreviations: Some(vec!["Assn".to_string()]),
    });

    // With custom "Assn" abbreviation, this should be ONE sentence
    let content = "Contact the Assn. Representative for details.";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // Should be empty because it's a single sentence (Assn. is recognized as abbreviation)
    assert!(
        result.is_empty(),
        "Custom abbreviation 'Assn' should prevent sentence split: {result:?}"
    );
}

#[test]
fn test_custom_abbreviations_merged_with_builtin() {
    // Test that custom abbreviations are ADDED to built-in ones, not replacing them
    let rule = MD013LineLength::from_config_struct(MD013Config {
        line_length: LineLength::from_const(0),
        code_blocks: false,
        tables: false,
        headings: false,
        paragraphs: true,
        strict: false,
        reflow: true,
        reflow_mode: ReflowMode::SentencePerLine,
        length_mode: rumdl_lib::rules::md013_line_length::md013_config::LengthMode::default(),
        abbreviations: Some(vec!["Assn".to_string()]),
    });

    // Both "Dr." (built-in) and "Assn." (custom) should be recognized
    let content = "Talk to Dr. Smith about the Assn. Meeting today.";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // Should be empty because both abbreviations are recognized
    assert!(
        result.is_empty(),
        "Both built-in 'Dr' and custom 'Assn' should be recognized: {result:?}"
    );
}

#[test]
fn test_custom_abbreviation_with_period_in_config() {
    // Test that abbreviations work whether configured with or without trailing period
    let rule_without_period = MD013LineLength::from_config_struct(MD013Config {
        line_length: LineLength::from_const(0),
        code_blocks: false,
        tables: false,
        headings: false,
        paragraphs: true,
        strict: false,
        reflow: true,
        reflow_mode: ReflowMode::SentencePerLine,
        length_mode: rumdl_lib::rules::md013_line_length::md013_config::LengthMode::default(),
        abbreviations: Some(vec!["Univ".to_string()]),
    });

    let rule_with_period = MD013LineLength::from_config_struct(MD013Config {
        line_length: LineLength::from_const(0),
        code_blocks: false,
        tables: false,
        headings: false,
        paragraphs: true,
        strict: false,
        reflow: true,
        reflow_mode: ReflowMode::SentencePerLine,
        length_mode: rumdl_lib::rules::md013_line_length::md013_config::LengthMode::default(),
        abbreviations: Some(vec!["Univ.".to_string()]),
    });

    let content = "Visit Univ. Campus for the tour.";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);

    let result_without = rule_without_period.check(&ctx).unwrap();
    let result_with = rule_with_period.check(&ctx).unwrap();

    // Both configurations should produce the same result
    assert_eq!(
        result_without.len(),
        result_with.len(),
        "Abbreviation config with/without period should behave the same"
    );
}
