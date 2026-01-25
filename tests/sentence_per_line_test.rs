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
        abbreviations: vec![],
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
        abbreviations: vec![],
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
        abbreviations: vec!["Assn".to_string()],
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
        abbreviations: vec!["Assn".to_string()],
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
        abbreviations: vec!["Univ".to_string()],
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
        abbreviations: vec!["Univ.".to_string()],
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

// =============================================================================
// Issue #335: Abbreviations config not recognized
// =============================================================================

#[test]
fn test_issue_335_abbreviations_config_empty_vec_uses_defaults() {
    // Issue #335: When abbreviations was Option<Vec<String>>, None and Some(vec![])
    // behaved differently. Now with Vec<String>, empty vec means "use defaults only"
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
        abbreviations: vec![], // Empty = use built-in defaults
    });

    // "Dr." is a built-in abbreviation - should NOT split after it
    // This single-sentence text should produce no warnings
    let content = "Dr. Smith is here today.";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // Single sentence with abbreviation - no warning expected
    assert!(
        result.is_empty(),
        "Single sentence with built-in abbreviation should not trigger warning: {result:?}"
    );

    // Now test that "Dr." doesn't cause incorrect split in multi-sentence
    let content2 = "Dr. Smith is here. He arrived today.";
    let ctx2 = LintContext::new(content2, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result2 = rule.check(&ctx2).unwrap();

    // Should detect 2 sentences (split at "here.", not at "Dr.")
    assert!(
        !result2.is_empty() && result2[0].message.contains("2 sentences"),
        "Should detect 2 sentences, not 3 (Dr. is abbreviation): {result2:?}"
    );

    // Verify the fix splits correctly (after "here.", not after "Dr.")
    if let Some(fix) = &result2[0].fix {
        assert!(
            fix.replacement.starts_with("Dr. Smith"),
            "Fix should keep 'Dr. Smith' together: {:?}",
            fix.replacement
        );
    }
}

#[test]
fn test_issue_335_custom_abbreviations_extend_defaults() {
    // Custom abbreviations should be ADDED to defaults, not replace them
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
        abbreviations: vec!["Corp".to_string(), "Inc".to_string()],
    });

    // Single sentence with multiple abbreviations - no warning expected
    let content = "Dr. Smith works at Corp. headquarters today.";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    assert!(
        result.is_empty(),
        "Single sentence with built-in and custom abbreviations should not trigger warning: {result:?}"
    );

    // Verify both built-in (Dr.) and custom (Corp., Inc.) are recognized
    let content2 = "Dr. Smith at Corp. arrived. He contacted Inc. today.";
    let ctx2 = LintContext::new(content2, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result2 = rule.check(&ctx2).unwrap();

    // Should detect 2 sentences (split at "arrived." and end)
    assert!(
        !result2.is_empty() && result2[0].message.contains("2 sentences"),
        "Should detect exactly 2 sentences (abbreviations recognized): {result2:?}"
    );

    // Verify the fix keeps abbreviations intact
    if let Some(fix) = &result2[0].fix {
        assert!(
            fix.replacement.contains("Dr. Smith") && fix.replacement.contains("Corp."),
            "Fix should keep abbreviations intact: {:?}",
            fix.replacement
        );
    }
}

// =============================================================================
// Issue #336: Year at end of sentence breaks reflow
// =============================================================================

#[test]
fn test_issue_336_year_at_end_of_sentence_not_list_item() {
    // Issue #336: "2019." was incorrectly identified as a list item because
    // is_numbered_list_item() didn't require space after the period
    let rule = create_sentence_per_line_rule();

    let content = "The event happened in 2019. It was a great year.";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // Should detect 2 sentences and offer to fix
    assert!(!result.is_empty(), "Should detect multiple sentences: {result:?}");
    assert!(
        result[0].message.contains("2 sentences"),
        "Should detect exactly 2 sentences: {result:?}"
    );

    // Verify a fix is available (means the reflow didn't fail/loop)
    assert!(
        result[0].fix.is_some(),
        "Should have a fix available (no infinite loop): {result:?}"
    );
}

#[test]
fn test_issue_336_various_years_at_sentence_end() {
    let rule = create_sentence_per_line_rule();

    let test_cases = [
        "Founded in 1999. Still going strong.",
        "Released in 2023. Users love it.",
        "Since 1776. A long history.",
        "Updated 2024. Now with new features.",
    ];

    for content in test_cases {
        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Should detect 2 sentences
        assert!(
            !result.is_empty() && result[0].message.contains("2 sentences"),
            "Should detect 2 sentences for: {content}, got: {result:?}"
        );

        // Should have a fix available (no convergence failure)
        assert!(result[0].fix.is_some(), "Should have fix for: {content}");
    }
}

#[test]
fn test_issue_336_actual_numbered_list_still_works() {
    // Make sure we didn't break actual numbered lists
    let rule = create_sentence_per_line_rule();

    let content = "1. First item\n2. Second item\n3. Third item";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // Numbered lists should not trigger sentence-per-line warnings
    assert!(
        result.is_empty(),
        "Numbered list items should not trigger warnings: {result:?}"
    );
}

// =============================================================================
// Issue #337: attr_list syntax being reflowed incorrectly
// =============================================================================

#[test]
fn test_issue_337_standalone_attr_list_preserved() {
    // Issue #337: Standalone attr_list like `{ .class-name }` should be preserved
    // and not merged into the previous paragraph during reflow
    let rule = create_sentence_per_line_rule();

    // Single sentence followed by attr_list - should not trigger warning
    let content = "This is a single sentence.\n{ .special-class }";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // The attr_list should be treated as a separate block, not merged with paragraph
    // If there's a warning, it should not be about multiple sentences
    for warning in &result {
        assert!(
            !warning.message.contains("sentences"),
            "attr_list should not cause sentence count issues: {result:?}"
        );
    }
}

#[test]
fn test_issue_337_various_attr_list_formats() {
    let rule = create_sentence_per_line_rule();

    let test_cases = [
        "Single sentence.\n{ .class }",
        "Single sentence.\n{: .class }",
        "Single sentence.\n{#custom-id}",
        "Single sentence.\n{: #id .class }",
    ];

    for content in test_cases {
        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Should not incorrectly count attr_list as part of paragraph
        for warning in &result {
            assert!(
                !warning.message.contains("2 sentences"),
                "attr_list should not be counted as sentence in: {content}, got: {result:?}"
            );
        }
    }
}

#[test]
fn test_issue_337_inline_attr_list_in_heading() {
    // Inline attr_lists (part of heading) should not cause issues
    let rule = create_sentence_per_line_rule();

    let content = "# Heading {#custom-id}\n\nThis is one sentence.";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // Should not trigger any warnings for this valid content
    assert!(
        result.is_empty(),
        "Heading with inline attr_list should not cause warnings: {result:?}"
    );
}

// =============================================================================
// Issue #338: MkDocs Snippets notation being reflowed
// =============================================================================

#[test]
fn test_issue_338_snippets_delimiter_not_merged() {
    // Issue #338: MkDocs Snippets notation like `;--8<--` should be preserved
    let rule = create_sentence_per_line_rule();

    // Snippets on their own line should not be merged with surrounding text
    let content = "Some text here.\n\n;--8<-- \"path/to/file.md\"\n\nMore text after.";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // Snippets line should be treated as its own block
    for warning in &result {
        // Should not try to merge snippets with other text
        assert!(
            !warning.message.contains("--8<--"),
            "Snippets notation should not appear in warning message: {result:?}"
        );
    }
}

#[test]
fn test_issue_338_snippets_block_style() {
    let rule = create_sentence_per_line_rule();

    // Block-style snippets (without semicolon)
    let content = "Introduction here.\n\n--8<-- \"includes/header.md\"\n\nConclusion here.";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // Snippets should be treated as block element
    for warning in &result {
        assert!(
            !warning.message.contains("--8<--"),
            "Block snippets should be preserved: {result:?}"
        );
    }
}

#[test]
fn test_issue_338_snippets_with_line_range() {
    let rule = create_sentence_per_line_rule();

    // Snippets with line range specifier
    let content = "Text before.\n\n--8<-- \"file.md:5:10\"\n\nText after.";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // Should handle snippets with options
    for warning in &result {
        assert!(
            !warning.message.contains("--8<--"),
            "Snippets with line range should be preserved: {result:?}"
        );
    }
}

#[test]
fn test_issue_338_multiple_snippets_in_document() {
    let rule = create_sentence_per_line_rule();

    let content = "Header text.\n\n--8<-- \"file1.md\"\n\n--8<-- \"file2.md\"\n\nFooter text.";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // Multiple snippets should all be preserved
    for warning in &result {
        assert!(
            !warning.message.contains("--8<--"),
            "Multiple snippets should all be preserved: {result:?}"
        );
    }
}

#[test]
fn test_issue_338_bare_snippet_delimiter_as_paragraph_boundary() {
    let rule = create_sentence_per_line_rule();

    // Bare snippet delimiters (multi-file block format) should act as paragraph boundaries
    // This is the format:
    // --8<--
    // file1.md
    // file2.md
    // --8<--
    let content = "First sentence. Second sentence.\n--8<--\nfile.md\n--8<--\nThird sentence.";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // Only the first line should have a warning (2 sentences)
    // The snippet block should NOT be merged with the text
    assert!(!result.is_empty(), "Should detect multiple sentences on first line");
    assert!(
        result[0].message.contains("2 sentences"),
        "First line has 2 sentences: {:?}",
        result[0].message
    );

    // The snippet delimiter should not appear in any fix
    for warning in &result {
        if let Some(fix) = &warning.fix {
            assert!(
                !fix.replacement.contains("--8<--"),
                "Snippet delimiter should not be in fix: {:?}",
                fix.replacement
            );
        }
    }
}
