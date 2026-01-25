use rumdl_lib::utils::text_reflow::*;
use std::time::Instant;

#[test]
fn test_list_item_trailing_whitespace_removal() {
    // Test for issue #76 - hard breaks (2 trailing spaces) should be preserved
    // and prevent reflowing
    let input = "1. First line with trailing spaces   \n    Second line with trailing spaces  \n    Third line\n";

    let options = ReflowOptions {
        line_length: 999999,
        break_on_sentences: true, // MD013 uses true by default
        preserve_breaks: false,
        sentence_per_line: false,
        abbreviations: None,
    };

    let result = reflow_markdown(input, &options);

    // Should not contain 3+ consecutive spaces (which would indicate
    // trailing whitespace became mid-line whitespace)
    assert!(
        !result.contains("   "),
        "Result should not contain 3+ consecutive spaces: {result:?}"
    );

    // Hard breaks should be preserved (exactly 2 trailing spaces)
    assert!(result.contains("  \n"), "Hard breaks should be preserved: {result:?}");

    // Should NOT be reflowed into a single line because hard breaks are present
    // The content should maintain its line structure
    assert!(
        result.lines().count() >= 2,
        "Should have multiple lines (not reflowed due to hard breaks), got: {}",
        result.lines().count()
    );
}

#[test]
fn test_reflow_simple_text() {
    let options = ReflowOptions {
        line_length: 20,
        ..Default::default()
    };

    let input = "This is a very long line that needs to be wrapped";
    let result = reflow_line(input, &options);

    assert_eq!(result.len(), 3);
    assert!(result[0].chars().count() <= 20);
}

#[test]
fn test_preserve_inline_code() {
    let options = ReflowOptions {
        line_length: 20,
        ..Default::default()
    };

    let input = "This line contains `some code` that should not be broken";
    let result = reflow_line(input, &options);

    // Code spans should not be broken
    assert!(result.iter().any(|line| line.contains("`some code`")));
}

#[test]
fn test_preserve_links() {
    let options = ReflowOptions {
        line_length: 30,
        ..Default::default()
    };

    let input = "Check out [this link](https://example.com) for more information on the topic";
    let result = reflow_line(input, &options);

    // Links should not be broken
    assert!(
        result
            .iter()
            .any(|line| line.contains("[this link](https://example.com)"))
    );
}

#[test]
fn test_reference_link_patterns_fixed() {
    let options = ReflowOptions {
        line_length: 80,
        ..Default::default()
    };

    // Test various reference link patterns
    let test_cases = vec![
        (
            "See [link][ref] for details",
            vec!["[link][ref]"],
            "reference link with label",
        ),
        (
            "Check [this][1] and [that][2] out",
            vec!["[this][1]", "[that][2]"],
            "multiple reference links",
        ),
        (
            "Visit [example.com][] today",
            vec!["[example.com][]"],
            "shortcut reference link",
        ),
        (
            "See [link] for more info [here][ref]",
            vec!["[link]", "[here][ref]"],
            "mixed reference styles",
        ),
    ];

    for (input, expected_patterns, description) in test_cases {
        let result = reflow_markdown(input, &options);

        for pattern in expected_patterns {
            assert!(
                result.contains(pattern),
                "Pattern '{pattern}' should be preserved in result for test: {description}\nInput: {input}\nResult: {result}"
            );
        }
    }
}

#[test]
fn test_sentence_detection_basic() {
    let text = "First sentence. Second sentence. Third sentence.";
    let sentences = split_into_sentences(text);

    assert_eq!(sentences.len(), 3);
    assert_eq!(sentences[0], "First sentence.");
    assert_eq!(sentences[1], "Second sentence.");
    assert_eq!(sentences[2], "Third sentence.");
}

#[test]
fn test_sentence_detection_abbreviations() {
    // Test that common abbreviations don't create false sentence boundaries
    let text = "Talk to Dr. Smith. He is helpful.";
    let sentences = split_into_sentences(text);

    assert_eq!(sentences.len(), 2);
    assert!(sentences[0].contains("Dr. Smith"));
}

#[test]
fn test_split_into_sentences() {
    let text = "This is the first sentence. And this is the second! Is this the third?";
    let sentences = split_into_sentences(text);

    assert_eq!(sentences.len(), 3);
    assert_eq!(sentences[0], "This is the first sentence.");
    assert_eq!(sentences[1], "And this is the second!");
    assert_eq!(sentences[2], "Is this the third?");

    // Test with no punctuation at end
    let text_no_punct = "This is a single sentence";
    let sentences = split_into_sentences(text_no_punct);
    assert_eq!(sentences.len(), 1);
    assert_eq!(sentences[0], "This is a single sentence");

    // Test empty string
    let sentences = split_into_sentences("");
    assert_eq!(sentences.len(), 0);
}

#[test]
fn test_sentence_per_line_reflow() {
    let options = ReflowOptions {
        line_length: 0, // Unlimited
        break_on_sentences: true,
        preserve_breaks: false,
        sentence_per_line: true,
        abbreviations: None,
    };

    let input = "First sentence. Second sentence. Third sentence.";
    let result = reflow_line(input, &options);

    assert_eq!(result.len(), 3);
    assert_eq!(result[0], "First sentence.");
    assert_eq!(result[1], "Second sentence.");
    assert_eq!(result[2], "Third sentence.");

    // Test with markdown
    let input_with_md = "This is `code`. And this is **bold**.";
    let result = reflow_line(input_with_md, &options);
    assert_eq!(result.len(), 2);
}

#[test]
fn test_sentence_per_line_with_backticks() {
    let options = ReflowOptions {
        line_length: 0,
        sentence_per_line: true,
        abbreviations: None,
        ..Default::default()
    };

    let input = "First sentence with `code`. Second sentence here.";
    let result = reflow_line(input, &options);

    assert_eq!(result.len(), 2);
    assert_eq!(result[0], "First sentence with `code`.");
    assert_eq!(result[1], "Second sentence here.");
}

#[test]
fn test_sentence_per_line_with_backticks_in_parens() {
    let options = ReflowOptions {
        line_length: 0,
        sentence_per_line: true,
        abbreviations: None,
        ..Default::default()
    };

    let input = "First sentence (with `code`). Second sentence here.";
    let result = reflow_line(input, &options);

    assert_eq!(result.len(), 2);
    assert_eq!(result[0], "First sentence (with `code`).");
    assert_eq!(result[1], "Second sentence here.");
}

#[test]
fn test_sentence_per_line_with_questions_exclamations() {
    let options = ReflowOptions {
        line_length: 0,
        sentence_per_line: true,
        abbreviations: None,
        ..Default::default()
    };

    let input = "Is this working? Yes it is! And a statement.";
    let result = reflow_line(input, &options);

    assert_eq!(result.len(), 3);
    let lines = result;
    assert_eq!(lines[0], "Is this working?");
    assert_eq!(lines[1], "Yes it is!");
    assert_eq!(lines[2], "And a statement.");
}

#[test]
fn test_split_sentences_issue_124() {
    // Test for issue #124 - Pydantic example
    let text = "If you are sure ... on a `PyModule` instance. For example:";

    let sentences = split_into_sentences(text);

    // This should detect 2 sentences:
    // 1. "If you are sure ... on a `PyModule` instance."
    // 2. "For example:"
    assert_eq!(sentences.len(), 2, "Should detect 2 sentences");
}

#[test]
fn test_reference_link_edge_cases() {
    let options = ReflowOptions {
        line_length: 80,
        ..Default::default()
    };

    // Test edge cases for reference link handling

    // 1. Reference link at start of line
    let input1 = "[link][ref] at the beginning of a line";
    let result1 = reflow_markdown(input1, &options);
    assert!(
        result1.contains("[link][ref]"),
        "Reference link at start should be preserved"
    );

    // 2. Reference link at end of line
    let input2 = "This is a reference to [link][ref]";
    let result2 = reflow_markdown(input2, &options);
    assert!(
        result2.contains("[link][ref]"),
        "Reference link at end should be preserved"
    );

    // 3. Multiple reference links on same line
    let input3 = "See [first][1] and [second][2] and [third][3] for details";
    let result3 = reflow_markdown(input3, &options);
    assert!(
        result3.contains("[first][1]"),
        "First reference link should be preserved"
    );
    assert!(
        result3.contains("[second][2]"),
        "Second reference link should be preserved"
    );
    assert!(
        result3.contains("[third][3]"),
        "Third reference link should be preserved"
    );

    // 4. Shortcut reference link (empty second brackets)
    let input4 = "Check out [example.com][] for more info";
    let result4 = reflow_markdown(input4, &options);
    assert!(
        result4.contains("[example.com][]"),
        "Shortcut reference link should be preserved"
    );

    // 5. Nested brackets (should not break the link)
    let input5 = "See [link with [nested] brackets][ref] here";
    let result5 = reflow_markdown(input5, &options);
    assert!(
        result5.contains("[link with [nested] brackets][ref]"),
        "Reference link with nested brackets should be preserved"
    );
}

#[test]
fn test_reflow_with_emphasis() {
    let options = ReflowOptions {
        line_length: 30,
        ..Default::default()
    };

    let input = "This line contains **bold text** and *italic text* that should be preserved";
    let result = reflow_markdown(input, &options);

    assert!(result.contains("**bold text**"));
    assert!(result.contains("*italic text*"));
}

#[test]
fn test_image_patterns_preserved() {
    let options = ReflowOptions {
        line_length: 50,
        ..Default::default()
    };

    // Test various image patterns
    let test_cases = vec![
        ("![alt text](image.png)", "![alt text](image.png)", "basic image"),
        (
            "![alt text](https://example.com/image.png)",
            "![alt text](https://example.com/image.png)",
            "image with URL",
        ),
        (
            "![alt text](image.png \"title\")",
            "![alt text](image.png \"title\")",
            "image with title",
        ),
        ("![](image.png)", "![](image.png)", "image without alt text"),
        ("![alt][ref]", "![alt][ref]", "reference-style image"),
    ];

    for (input, expected_pattern, description) in test_cases {
        let result = reflow_markdown(input, &options);
        assert!(
            result.contains(expected_pattern),
            "Image pattern should be preserved for test: {description}\nInput: {input}\nResult: {result}"
        );
    }
}

#[test]
fn test_extended_markdown_patterns() {
    let options = ReflowOptions {
        line_length: 80,
        ..Default::default()
    };

    // Strikethrough
    let input_strike = "This text has ~~strikethrough~~ formatting";
    let result_strike = reflow_markdown(input_strike, &options);
    assert!(result_strike.contains("~~strikethrough~~"));

    // Subscript
    let input_sub = "H~2~O is water";
    let result_sub = reflow_markdown(input_sub, &options);
    assert!(result_sub.contains("H~2~O"));

    // Superscript
    let input_sup = "E = mc^2^";
    let result_sup = reflow_markdown(input_sup, &options);
    assert!(result_sup.contains("mc^2^"));

    // Highlight
    let input_mark = "This is ==highlighted== text";
    let result_mark = reflow_markdown(input_mark, &options);
    assert!(result_mark.contains("==highlighted=="));
}

#[test]
fn test_complex_mixed_patterns() {
    let options = ReflowOptions {
        line_length: 100,
        ..Default::default()
    };

    let input = "This is a **bold link [example](https://example.com)** with `code` and an ![image](img.png).";
    let result = reflow_markdown(input, &options);

    // All patterns should be preserved
    assert!(result.contains("**bold link [example](https://example.com)**"));
    assert!(result.contains("`code`"));
    assert!(result.contains("![image](img.png)"));
}

#[test]
fn test_footnote_patterns_preserved() {
    let options = ReflowOptions {
        line_length: 80,
        ..Default::default()
    };

    // Inline footnote
    let input_inline = "This is a sentence with a footnote^[This is the footnote text] in it.";
    let result_inline = reflow_markdown(input_inline, &options);
    assert!(result_inline.contains("^[This is the footnote text]"));

    // Reference footnote
    let input_ref = "This is a sentence with a reference footnote[^1] in it.";
    let result_ref = reflow_markdown(input_ref, &options);
    assert!(result_ref.contains("[^1]"));

    // Named footnote
    let input_named = "This is a sentence with a named footnote[^note] in it.";
    let result_named = reflow_markdown(input_named, &options);
    assert!(result_named.contains("[^note]"));
}

#[test]
fn test_reflow_markdown_numbered_lists() {
    // Use shorter line length to force wrapping
    let options = ReflowOptions {
        line_length: 40,
        ..Default::default()
    };

    let input = "1. This is the first item in a numbered list\n2. This is the second item with a continuation that spans multiple lines\n3. Third item";
    let result = reflow_markdown(input, &options);

    // Lists should preserve their markers
    assert!(result.contains("1. "), "Should have first list marker");
    assert!(result.contains("2. "), "Should have second list marker");
    assert!(result.contains("3. "), "Should have third list marker");

    // Continuations should be indented with 3 spaces (marker + space = 3 chars)
    let lines: Vec<&str> = result.lines().collect();
    let continuation_lines: Vec<&&str> = lines
        .iter()
        .filter(|l| l.starts_with("   ") && !l.starts_with("   that"))
        .collect();

    // Should have at least one continuation line (wrapped content)
    assert!(
        !continuation_lines.is_empty(),
        "Numbered list continuations should be indented with 3 spaces. Got:\n{result}"
    );
}

#[test]
fn test_reflow_markdown_bullet_lists() {
    // Use shorter line length to force wrapping
    let options = ReflowOptions {
        line_length: 40,
        ..Default::default()
    };

    let input = "- This is the first bullet item\n- This is the second bullet with a continuation that spans multiple lines\n- Third item";
    let result = reflow_markdown(input, &options);

    // Bullet lists should preserve their markers
    assert!(result.contains("- This"), "Should have bullet markers");

    // Continuations should be indented with 2 spaces (marker + space = 2 chars)
    let lines: Vec<&str> = result.lines().collect();
    // Look for lines that start with 2 spaces but not a list marker
    let continuation_lines: Vec<&&str> = lines
        .iter()
        .filter(|l| l.starts_with("  ") && !l.starts_with("- ") && !l.starts_with("  that"))
        .collect();

    // Should have continuation lines (wrapped content)
    assert!(
        !continuation_lines.is_empty(),
        "Bullet lists should preserve markers and indent continuations with 2 spaces. Got:\n{result}"
    );
}

#[test]
fn test_ie_abbreviation_split_debug() {
    let input = "This results in extracting directly from the input object, i.e. `obj.extract()`, rather than trying to access an item or attribute.";

    let options = ReflowOptions {
        line_length: 80,
        break_on_sentences: true,
        preserve_breaks: false,
        sentence_per_line: true,
        abbreviations: None,
    };

    let result = reflow_line(input, &options);

    // Should be 1 sentence, not split after "i.e."
    assert_eq!(result.len(), 1, "Should not split after i.e. abbreviation");
}

#[test]
fn test_ie_abbreviation_paragraph() {
    // Test the full paragraph from the file that's causing the issue
    let input = "The `pyo3(transparent)` attribute can be used on structs with exactly one field.\nThis results in extracting directly from the input object, i.e. `obj.extract()`, rather than trying to access an item or attribute.\nThis behaviour is enabled per default for newtype structs and tuple-variants with a single field.";

    let options = ReflowOptions {
        line_length: 80,
        break_on_sentences: true,
        preserve_breaks: false,
        sentence_per_line: true,
        abbreviations: None,
    };

    let result = reflow_markdown(input, &options);

    // The "i.e." should NOT cause a line break
    assert!(
        !result.contains("i.e.\n"),
        "Should not break after i.e. abbreviation:\n{result}"
    );
}

#[test]
fn test_definition_list_preservation() {
    let options = ReflowOptions {
        line_length: 80,
        sentence_per_line: true,
        abbreviations: None,
        ..Default::default()
    };

    let content = "Term\n: Definition here";
    let result = reflow_markdown(content, &options);

    // Definition list format should be preserved
    assert!(
        result.contains(": Definition"),
        "Definition list marker should be preserved"
    );
}

#[test]
fn test_definition_list_multiline() {
    let options = ReflowOptions {
        line_length: 80,
        sentence_per_line: true,
        abbreviations: None,
        ..Default::default()
    };

    let content = "Term\n: First sentence of definition. Second sentence.";
    let result = reflow_markdown(content, &options);

    // Definition list should NOT be reflowed into sentence-per-line
    // We don't split sentences within definition list items
    assert!(result.contains("\n: First sentence of definition. Second sentence."));
}

#[test]
fn test_definition_list_multiple() {
    let options = ReflowOptions {
        line_length: 80,
        sentence_per_line: true,
        abbreviations: None,
        ..Default::default()
    };

    let content = "Term 1\n: Definition 1\n: Another definition for term 1\n\nTerm 2\n: Definition 2";
    let result = reflow_markdown(content, &options);

    // All definition lines should preserve ": " at start
    assert!(result.lines().filter(|l| l.trim_start().starts_with(": ")).count() >= 3);
}

#[test]
fn test_definition_list_with_paragraphs() {
    let options = ReflowOptions {
        line_length: 0, // No line length constraint
        break_on_sentences: true,
        preserve_breaks: false,
        sentence_per_line: true,
        abbreviations: None,
    };

    let content = "Regular paragraph. With multiple sentences.\n\nTerm\n: Definition.\n\nAnother paragraph.";
    let result = reflow_markdown(content, &options);

    // Paragraph should be reflowed (sentences on separate lines)
    assert!(result.contains("Regular paragraph."));
    assert!(result.contains("\nWith multiple sentences."));
    // Definition list should be preserved
    assert!(result.contains("Term\n: Definition."));
    // Another paragraph should be preserved (single sentence, stays as is)
    assert!(result.contains("Another paragraph."));
}

#[test]
fn test_definition_list_edge_cases() {
    let options = ReflowOptions::default();

    // Indented definition
    let content1 = "Term\n  : Indented definition";
    let result1 = reflow_markdown(content1, &options);
    assert!(result1.contains("\n  : Indented definition"));

    // Multiple spaces after colon
    let content2 = "Term\n:   Definition";
    let result2 = reflow_markdown(content2, &options);
    assert!(result2.contains("\n:   Definition"));

    // Tab after colon
    let content3 = "Term\n:\tDefinition";
    let result3 = reflow_markdown(content3, &options);
    assert!(result3.contains("\n:\tDefinition"));
}

// Tests for issue #150: Abbreviation detection bug
// https://github.com/rvben/rumdl/issues/150

#[test]
fn test_abbreviation_false_positives_word_boundary() {
    // Issue #150: Words ending in abbreviation letter sequences
    // should NOT be detected as abbreviations
    let options = ReflowOptions {
        line_length: 80,
        sentence_per_line: true,
        abbreviations: None,
        ..Default::default()
    };

    // False positives to prevent (word endings that look like abbreviations)
    let false_positive_cases = vec![
        ("Why doesn't `rumdl` like the word paradigms?", 1),
        ("There are many programs?", 1),
        ("We have multiple items?", 1),
        ("The systems?", 1),
        ("Complex regex?", 1),
        ("These teams!", 1),
        ("Multiple schemes.", 1), // ends with period but "schemes" != "Ms"
    ];

    for (input, expected_sentences) in false_positive_cases {
        let result = reflow_line(input, &options);
        assert_eq!(
            result.len(),
            expected_sentences,
            "Input '{input}' should be {expected_sentences} sentence(s), got {}: {:?}",
            result.len(),
            result
        );
    }
}

#[test]
fn test_abbreviation_period_vs_other_punctuation() {
    let options = ReflowOptions {
        line_length: 80,
        sentence_per_line: true,
        abbreviations: None,
        ..Default::default()
    };

    // Questions and exclamations are NOT abbreviations (only periods count)
    let not_abbreviations = vec![
        "Who is Dr?",  // ? means not abbreviation
        "See Mr!",     // ! means not abbreviation
        "What is Ms?", // ? means not abbreviation
    ];

    for input in not_abbreviations {
        let result = reflow_line(input, &options);
        assert_eq!(
            result.len(),
            1,
            "'{input}' should be 1 complete sentence (punctuation is not period)"
        );
    }

    // Only periods after abbreviations count
    let actual_abbreviations = vec![
        "See Dr. Smith today",   // Dr. is abbreviation
        "Use e.g. this example", // e.g. is abbreviation
        "Call Mr. Jones",        // Mr. is abbreviation
    ];

    for input in actual_abbreviations {
        let sentences = split_into_sentences(input);
        assert_eq!(
            sentences.len(),
            1,
            "'{input}' should be 1 sentence (contains abbreviation with period)"
        );
    }
}

#[test]
fn test_abbreviation_true_positives() {
    // Actual abbreviations should still be detected correctly
    let text = "Talk to Dr. Smith. He is helpful. See also Mr. Jones.";
    let sentences = split_into_sentences(text);

    // Should NOT split at "Dr." or "Mr."
    assert_eq!(sentences.len(), 3);
    assert!(sentences[0].contains("Dr. Smith"));
    assert!(sentences[2].contains("Mr. Jones"));
}

#[test]
fn test_issue_150_paradigms_with_question_mark() {
    // The actual issue: "paradigms?" should be a complete sentence
    let text = "Why doesn't `rumdl` like the word paradigms? Next sentence.";
    let sentences = split_into_sentences(text);

    assert_eq!(sentences.len(), 2, "Should split at '?' (not an abbreviation)");
    assert!(sentences[0].ends_with("paradigms?"));
    assert_eq!(sentences[1], "Next sentence.");
}

#[test]
fn test_issue_150_exact_reproduction() {
    // Exact test case from issue #150
    let options = ReflowOptions {
        line_length: 0, // unlimited
        sentence_per_line: true,
        abbreviations: None,
        ..Default::default()
    };

    let input = "Why doesn't `rumdl` like the word paradigms?\nIf I remove the \"s\" from \"paradigms\", or if I replace \"paradigms\" with another word that ends in \"s\", this passes!";

    // This should complete without hanging (use reflow_markdown for multi-line input)
    let result = reflow_markdown(input, &options);

    // Should have 2 lines (one sentence per line)
    let lines: Vec<&str> = result.lines().collect();
    assert_eq!(lines.len(), 2, "Should have 2 sentences on separate lines");
    assert!(
        lines[0].contains("paradigms?"),
        "First line should contain 'paradigms?'"
    );
    assert!(lines[1].contains("passes!"), "Second line should contain 'passes!'");
}

#[test]
fn test_all_abbreviations_comprehensive() {
    // Property-based test: ALL built-in abbreviations should be detected
    // Built-in list: titles (Mr, Mrs, Ms, Dr, Prof, Sr, Jr) and Latin (i.e, e.g)
    let all_abbreviations = ["i.e", "e.g", "Mr", "Mrs", "Dr", "Ms", "Prof", "Sr", "Jr"];

    for abbr in all_abbreviations {
        // Test standalone abbreviation with period - should be 1 sentence
        let with_period = format!("{abbr}.");
        let sentences = split_into_sentences(&with_period);
        assert_eq!(
            sentences.len(),
            1,
            "Should detect '{with_period}' as complete (ends with abbreviation)"
        );

        // Test abbreviation NOT splitting inline usage - should be 1 sentence
        // "word i.e. next" is ONE sentence because i.e. is an inline abbreviation
        let inline = format!("word {abbr}. next word");
        let sentences = split_into_sentences(&inline);
        assert_eq!(
            sentences.len(),
            1,
            "'{inline}' should be 1 sentence (abbreviation doesn't end sentence)"
        );

        // Test abbreviation with content AFTER it that ends the sentence
        // "See Dr. Smith. He" should be 2 sentences - split happens after "Smith."
        let with_content = format!("See {abbr}. Name here. Next sentence.");
        let sentences = split_into_sentences(&with_content);
        assert!(sentences.len() >= 2, "'{with_content}' should have multiple sentences");
    }
}

#[test]
fn test_abbreviation_case_insensitivity() {
    // All case variations should work
    let case_variations = vec![
        "Talk to dr. Smith. Next sentence.",
        "Talk to Dr. Smith. Next sentence.",
        "Talk to DR. Smith. Next sentence.",
        "Talk to dR. Smith. Next sentence.",
    ];

    for input in case_variations {
        let sentences = split_into_sentences(input);
        assert_eq!(sentences.len(), 2, "Case variation '{input}' should work correctly");
        assert!(sentences[0].contains("Smith"), "First sentence should include 'Smith'");
    }
}

#[test]
fn test_abbreviation_at_eof() {
    // Sentences ending with abbreviation at end of file (no following sentence)
    // Single sentence ending with abbreviation
    let inputs = vec!["Talk to Dr.", "Use e.g.", "See Mr. Smith", "Prof. Jones", "It's vs."];

    for input in inputs {
        let sentences = split_into_sentences(input);
        assert_eq!(
            sentences.len(),
            1,
            "'{input}' should be 1 sentence (ends with abbreviation at EOF)"
        );
    }
}

#[test]
fn test_abbreviation_followed_by_sentence() {
    // Abbreviation immediately followed by another sentence
    let text = "See Dr. Smith went home. Another sentence here.";
    let sentences = split_into_sentences(text);

    assert_eq!(sentences.len(), 2, "Should detect 2 sentences");
    assert!(
        sentences[0].contains("Dr. Smith went home"),
        "First sentence should include 'Dr. Smith went home'"
    );
    assert_eq!(sentences[1], "Another sentence here.");
}

#[test]
fn test_multiple_consecutive_spaces_with_abbreviations() {
    // Multiple spaces shouldn't break abbreviation detection
    let text = "Talk  to  Dr.  Smith went home.";
    let sentences = split_into_sentences(text);

    assert_eq!(sentences.len(), 1, "Should be 1 sentence despite multiple spaces");
}

#[test]
fn test_all_false_positive_word_endings() {
    // Property-based test: Common word endings that look like abbreviations
    // should NOT be detected as abbreviations
    let false_positive_words = vec![
        // Words ending in "ms"
        ("paradigms.", "ms"),
        ("programs.", "ms"),
        ("items.", "ms"),
        ("systems.", "ms"),
        ("teams.", "ms"),
        ("schemes.", "ms"),
        ("problems.", "ms"),
        ("algorithms.", "ms"),
        // Words ending in "vs"
        ("obviouslyvs.", "vs"), // contrived but tests the pattern
        // Words ending in "ex"
        ("complex.", "ex"),
        ("index.", "ex"),
        ("regex.", "ex"),
        ("vertex.", "ex"),
        ("cortex.", "ex"),
        // Words ending in "ie"
        ("cookie.", "ie"),
        ("movie.", "ie"),
        ("zombie.", "ie"),
        // Words ending in "eg"
        ("nutmeg.", "eg"),
        ("peg.", "eg"),
        // Words ending in "sr"
        ("usr.", "sr"), // like /usr/ directory
        // Words ending in "jr"
        ("mjr.", "jr"), // like major abbreviated differently
    ];

    for (word, _pattern) in false_positive_words {
        let text = format!("{word} Next sentence.");
        let sentences = split_into_sentences(&text);
        assert_eq!(
            sentences.len(),
            2,
            "'{word}' should NOT be detected as abbreviation (should split into 2 sentences)"
        );
    }
}

#[test]
fn test_abbreviations_in_sentence_per_line_integration() {
    // Integration test: Test all abbreviations in sentence-per-line mode
    // This verifies the complete flow works correctly
    let options = ReflowOptions {
        line_length: 0, // unlimited
        sentence_per_line: true,
        abbreviations: None,
        ..Default::default()
    };

    // Test with multiple abbreviations in different contexts
    let content = r#"Talk to Dr. Smith about the research. The experiment uses e.g. neural networks. Meet Prof. Jones and Mr. Wilson tomorrow. This is important, i.e. very critical. Compare apples vs. oranges in the study. See also Sr. Developer position. Contact Jr. Analyst for details. Use etc. for additional items. Check ex. references in appendix. Define ie. for clarity. Consider eg. alternative approaches."#;

    // Should complete without hanging
    let result = reflow_markdown(content, &options);

    // Verify each sentence is on its own line
    let lines: Vec<&str> = result.lines().collect();

    // Should have 11 sentences (one per line)
    assert_eq!(lines.len(), 11, "Should have 11 sentences on separate lines");

    // Verify abbreviations are preserved in output
    assert!(result.contains("Dr. Smith"));
    assert!(result.contains("e.g. neural"));
    assert!(result.contains("Prof. Jones"));
    assert!(result.contains("Mr. Wilson"));
    assert!(result.contains("i.e. very"));
    assert!(result.contains("vs. oranges"));
    assert!(result.contains("Sr. Developer"));
    assert!(result.contains("Jr. Analyst"));
    assert!(result.contains("etc. for"));
    assert!(result.contains("ex. references"));
}

#[test]
fn test_issue_150_all_reported_variations() {
    // Test all variations mentioned in issue #150
    let options = ReflowOptions {
        line_length: 0,
        sentence_per_line: true,
        abbreviations: None,
        ..Default::default()
    };

    // Original case: "paradigms"
    let paradigms = "Why doesn't `rumdl` like the word paradigms?\nNext sentence.";
    let result = reflow_markdown(paradigms, &options);
    assert!(result.contains("paradigms?"), "Should handle 'paradigms'");

    // Mentioned variation: removing "s" from "paradigms" = "paradigm"
    let paradigm = "Why doesn't `rumdl` like the word paradigm?\nNext sentence.";
    let result = reflow_markdown(paradigm, &options);
    assert!(result.contains("paradigm?"), "Should handle 'paradigm'");

    // Mentioned variation: "another word that ends in 's'"
    let programs = "Why doesn't `rumdl` like programs?\nNext sentence.";
    let result = reflow_markdown(programs, &options);
    assert!(result.contains("programs?"), "Should handle 'programs'");

    let items = "Why doesn't `rumdl` like items?\nNext sentence.";
    let result = reflow_markdown(items, &options);
    assert!(result.contains("items?"), "Should handle 'items'");
}

#[test]
fn test_performance_no_hang_on_false_positives() {
    // Performance regression test: Ensure processing completes quickly
    // Previously these would hang indefinitely
    let options = ReflowOptions {
        line_length: 0,
        sentence_per_line: true,
        abbreviations: None,
        ..Default::default()
    };

    let test_cases = vec![
        "paradigms?",
        "programs!",
        "items.",
        "systems?",
        "teams!",
        "complex.",
        "regex?",
        "cookie.",
        "vertex!",
    ];

    for case in test_cases {
        let start = Instant::now();
        let _result = reflow_line(case, &options);
        let elapsed = start.elapsed();

        assert!(
            elapsed.as_millis() < 100,
            "'{case}' took {elapsed:?} (should be <100ms)"
        );
    }
}

// Tests for spacing preservation during reflow
// These test cases verify that punctuation stays attached to adjacent elements

#[test]
fn test_reflow_preserves_colon_after_code() {
    // Bug: `code`: was becoming `code` : (spurious space before colon)
    let options = ReflowOptions {
        line_length: 20,
        ..Default::default()
    };

    let input = "This has `code`: followed by text";
    let result = reflow_line(input, &options);
    let joined = result.join("\n");

    // Colon should stay attached to backtick
    assert!(
        joined.contains("`code`:"),
        "Colon should stay attached to code span, got: {joined:?}"
    );
    assert!(
        !joined.contains("`code` :"),
        "Should not have space before colon, got: {joined:?}"
    );
}

#[test]
fn test_reflow_preserves_comma_after_code() {
    // Bug: `a`, was becoming `a` , (spurious space before comma)
    let options = ReflowOptions {
        line_length: 30,
        ..Default::default()
    };

    let input = "List: `a`, `b`, `c`.";
    let result = reflow_line(input, &options);
    let joined = result.join("\n");

    // Commas should stay attached
    assert!(
        joined.contains("`a`,"),
        "Comma should stay attached to code span, got: {joined:?}"
    );
    assert!(
        !joined.contains("`a` ,"),
        "Should not have space before comma, got: {joined:?}"
    );
}

#[test]
fn test_reflow_preserves_closing_paren_after_code() {
    // Bug: `parens`) was becoming `parens` ) (spurious space before paren)
    let options = ReflowOptions {
        line_length: 40,
        ..Default::default()
    };

    let input = "And (`parens`) here";
    let result = reflow_line(input, &options);
    let joined = result.join("\n");

    // Closing paren should stay attached
    assert!(
        joined.contains("`parens`)"),
        "Closing paren should stay attached, got: {joined:?}"
    );
    assert!(
        !joined.contains("`parens` )"),
        "Should not have space before closing paren, got: {joined:?}"
    );
}

#[test]
fn test_reflow_no_space_after_opening_paren() {
    // Bug: (`Mr` was becoming ( `Mr` (spurious space after open paren)
    let options = ReflowOptions {
        line_length: 80,
        ..Default::default()
    };

    let input = "titles (`Mr`, `Mrs`, `Ms`)";
    let result = reflow_line(input, &options);
    let joined = result.join("\n");

    // No space after opening paren
    assert!(
        joined.contains("(`Mr`"),
        "No space after opening paren, got: {joined:?}"
    );
    assert!(
        !joined.contains("( `Mr`"),
        "Should not have space after opening paren, got: {joined:?}"
    );
}

#[test]
fn test_reflow_punctuation_never_starts_line() {
    // Bug: punctuation like comma could end up at start of new line
    let options = ReflowOptions {
        line_length: 10,
        ..Default::default()
    };

    let input = "List: `a`, `b`, `c`.";
    let result = reflow_line(input, &options);

    // No line should start with punctuation
    for line in &result {
        let trimmed = line.trim_start();
        assert!(!trimmed.starts_with(','), "Line should not start with comma: {line:?}");
        assert!(!trimmed.starts_with('.'), "Line should not start with period: {line:?}");
        assert!(
            !trimmed.starts_with(')'),
            "Line should not start with closing paren: {line:?}"
        );
    }
}

#[test]
fn test_reflow_complex_punctuation_case() {
    // Combined test case from original bug report
    let options = ReflowOptions {
        line_length: 200,
        ..Default::default()
    };

    let input = "- `abbreviations`: Custom abbreviations for sentence-per-line mode (optional). Periods are optional - both `\"Dr\"` and `\"Dr.\"` work the same. Custom abbreviations are added to the built-in defaults: titles (`Mr`, `Mrs`, `Ms`, `Dr`, `Prof`, `Sr`, `Jr`) and Latin (`i.e`, `e.g`).";
    let result = reflow_markdown(input, &options);

    // Verify no spurious spaces around punctuation
    assert!(
        !result.contains("` :"),
        "No space before colon after backtick: {result:?}"
    );
    assert!(
        !result.contains("` ,"),
        "No space before comma after backtick: {result:?}"
    );
    assert!(
        !result.contains("` )"),
        "No space before paren after backtick: {result:?}"
    );
    assert!(
        !result.contains("( `"),
        "No space after opening paren before backtick: {result:?}"
    );
}

/// Issue #170: Comprehensive tests for all 4 linked image variants
/// These patterns represent clickable image badges that must be treated as atomic units.
/// Breaking between `]` and `(` or `]` and `[` produces invalid markdown.
mod issue_170_nested_link_image {
    use super::*;

    // ============================================================
    // Pattern 1: Inline image in inline link - [![alt](img)](link)
    // ============================================================

    #[test]
    fn test_pattern1_inline_inline_simple() {
        let options = ReflowOptions {
            line_length: 80,
            ..Default::default()
        };

        let input = "[![Badge](https://img.shields.io/badge)](https://example.com) some text here";
        let result = reflow_markdown(input, &options);

        assert!(
            !result.contains("]\n(") && !result.contains("]\n["),
            "Linked image should not be broken: {result:?}"
        );
        assert!(
            result.contains("[![Badge](https://img.shields.io/badge)](https://example.com)"),
            "Full structure should be preserved: {result:?}"
        );
    }

    #[test]
    fn test_pattern1_inline_inline_long_url() {
        let options = ReflowOptions {
            line_length: 80,
            ..Default::default()
        };

        let input = "[![GitHub Actions](https://img.shields.io/github/actions/workflow/status/user/repo/release.yaml)](https://github.com/user/repo/actions) text";
        let result = reflow_markdown(input, &options);

        assert!(
            !result.contains("]\n(") && !result.contains("]\n["),
            "Long linked image should not be broken: {result:?}"
        );
    }

    #[test]
    fn test_pattern1_inline_inline_with_text() {
        let options = ReflowOptions {
            line_length: 80,
            ..Default::default()
        };

        let input = "prefix: [![Crates.io](https://img.shields.io/crates/v/mypackage)](https://crates.io/crates/mypackage) This is descriptive text that continues after";
        let result = reflow_markdown(input, &options);

        assert!(!result.contains("]\n("), "Badge should not be broken: {result:?}");
        assert!(
            result.contains(
                "[![Crates.io](https://img.shields.io/crates/v/mypackage)](https://crates.io/crates/mypackage)"
            ),
            "Full badge structure should be preserved: {result:?}"
        );
    }

    #[test]
    fn test_pattern1_multiple_badges() {
        let options = ReflowOptions {
            line_length: 80,
            ..Default::default()
        };

        let input = "[![B1](https://img1.io)](https://l1.com) [![B2](https://img2.io)](https://l2.com) [![B3](https://img3.io)](https://l3.com)";
        let result = reflow_markdown(input, &options);

        assert!(
            !result.contains("]\n("),
            "Badge structures should not be broken: {result:?}"
        );
    }

    // ============================================================
    // Pattern 2: Reference image in inline link - [![alt][ref]](link)
    // ============================================================

    #[test]
    fn test_pattern2_ref_inline_simple() {
        let options = ReflowOptions {
            line_length: 80,
            ..Default::default()
        };

        let input = "[![Badge][badge-img]](https://example.com) some text here that might wrap";
        let result = reflow_markdown(input, &options);

        assert!(
            !result.contains("]\n(") && !result.contains("]\n["),
            "Linked image with ref should not be broken: {result:?}"
        );
        assert!(
            result.contains("[![Badge][badge-img]](https://example.com)"),
            "Full structure should be preserved: {result:?}"
        );
    }

    #[test]
    fn test_pattern2_ref_inline_long() {
        let options = ReflowOptions {
            line_length: 80,
            ..Default::default()
        };

        let input = "[![GitHub Actions Status][github-actions-badge]](https://github.com/user/repo/actions/workflows/ci.yml) text after";
        let result = reflow_markdown(input, &options);

        assert!(
            !result.contains("]\n(") && !result.contains("]\n["),
            "Long ref-inline linked image should not be broken: {result:?}"
        );
    }

    // ============================================================
    // Pattern 3: Inline image in reference link - [![alt](img)][ref]
    // ============================================================

    #[test]
    fn test_pattern3_inline_ref_simple() {
        let options = ReflowOptions {
            line_length: 80,
            ..Default::default()
        };

        let input = "[![Badge](https://img.shields.io/badge)][link-ref] some text here to wrap";
        let result = reflow_markdown(input, &options);

        assert!(
            !result.contains("]\n(") && !result.contains("]\n["),
            "Linked image with ref link should not be broken: {result:?}"
        );
        assert!(
            result.contains("[![Badge](https://img.shields.io/badge)][link-ref]"),
            "Full structure should be preserved: {result:?}"
        );
    }

    #[test]
    fn test_pattern3_inline_ref_long() {
        let options = ReflowOptions {
            line_length: 80,
            ..Default::default()
        };

        let input = "[![Build Status](https://github.com/user/repo/actions/workflows/ci.yml/badge.svg)][ci-link] text";
        let result = reflow_markdown(input, &options);

        assert!(
            !result.contains("]\n(") && !result.contains("]\n["),
            "Long inline-ref linked image should not be broken: {result:?}"
        );
    }

    // ============================================================
    // Pattern 4: Reference image in reference link - [![alt][ref]][ref]
    // ============================================================

    #[test]
    fn test_pattern4_ref_ref_simple() {
        let options = ReflowOptions {
            line_length: 80,
            ..Default::default()
        };

        let input = "[![Badge][badge-img]][badge-link] some text here that might need to wrap";
        let result = reflow_markdown(input, &options);

        assert!(
            !result.contains("]\n(") && !result.contains("]\n["),
            "Double-ref linked image should not be broken: {result:?}"
        );
        assert!(
            result.contains("[![Badge][badge-img]][badge-link]"),
            "Full structure should be preserved: {result:?}"
        );
    }

    #[test]
    fn test_pattern4_ref_ref_long() {
        let options = ReflowOptions {
            line_length: 80,
            ..Default::default()
        };

        let input = "[![GitHub Actions Badge][github-actions-img]][github-actions-link] text after the badge";
        let result = reflow_markdown(input, &options);

        assert!(
            !result.contains("]\n(") && !result.contains("]\n["),
            "Long double-ref linked image should not be broken: {result:?}"
        );
    }

    // ============================================================
    // Edge cases
    // ============================================================

    #[test]
    fn test_url_with_parentheses() {
        let options = ReflowOptions {
            line_length: 80,
            ..Default::default()
        };

        let input = "[![Wiki](https://img.io/badge)](https://en.wikipedia.org/wiki/Rust_(language)) text";
        let result = reflow_markdown(input, &options);

        assert!(
            !result.contains("]\n("),
            "URL with parentheses should not break badge: {result:?}"
        );
    }

    #[test]
    fn test_empty_alt_text() {
        let options = ReflowOptions {
            line_length: 80,
            ..Default::default()
        };

        let input = "[![](https://img.shields.io/badge)](https://example.com) text after";
        let result = reflow_markdown(input, &options);

        assert!(
            !result.contains("]\n("),
            "Empty alt text badge should not be broken: {result:?}"
        );
    }

    #[test]
    fn test_special_chars_in_alt() {
        let options = ReflowOptions {
            line_length: 80,
            ..Default::default()
        };

        let input = "[![Build: passing!](https://img.io/badge)](https://example.com) text";
        let result = reflow_markdown(input, &options);

        assert!(
            !result.contains("]\n("),
            "Special chars in alt should not break badge: {result:?}"
        );
    }

    #[test]
    fn test_mixed_patterns_on_line() {
        let options = ReflowOptions {
            line_length: 120,
            ..Default::default()
        };

        // Mix of pattern 1 and pattern 3
        let input = "[![A](https://img1.io)](https://l1.com) [![B](https://img2.io)][ref] more text here";
        let result = reflow_markdown(input, &options);

        assert!(
            !result.contains("]\n(") && !result.contains("]\n["),
            "Mixed patterns should all be preserved: {result:?}"
        );
    }

    // Issue #249: Hugo shortcodes should be preserved as atomic elements
    #[test]
    fn test_hugo_shortcode_preserved() {
        let options = ReflowOptions {
            line_length: 80,
            sentence_per_line: true,
            ..Default::default()
        };

        // Simple Hugo shortcode with periods in attributes
        let input = r#"{{< figure src="image.png" alt="Description. More text." >}}"#;
        let result = reflow_markdown(input, &options);

        // Shortcode should not be broken at the period
        assert!(
            result.contains(r#"{{< figure src="image.png" alt="Description. More text." >}}"#),
            "Hugo shortcode should be preserved as atomic unit: {result:?}"
        );
    }

    #[test]
    fn test_hugo_percent_shortcode_preserved() {
        let options = ReflowOptions {
            line_length: 80,
            sentence_per_line: true,
            ..Default::default()
        };

        // Hugo template shortcode with {{% %}} delimiters
        let input = r#"{{% notice tip %}}This is a tip. It has periods.{{% /notice %}}"#;
        let result = reflow_markdown(input, &options);

        // Content should be preserved without splitting on periods
        assert!(
            result.contains(r#"{{% notice tip %}}"#),
            "Hugo template shortcode should be preserved: {result:?}"
        );
    }

    #[test]
    fn test_hugo_shortcode_no_duplication() {
        // Issue #249: Content was doubling each time rumdl was run
        let options = ReflowOptions {
            line_length: 80,
            sentence_per_line: true,
            ..Default::default()
        };

        let input = r#"{{< figure src="images/test.png" alt="Grid of three rows. Each comparing." >}}"#;

        // Run reflow twice
        let result1 = reflow_markdown(input, &options);
        let result2 = reflow_markdown(&result1, &options);

        // Content should be idempotent (same size after multiple runs)
        assert_eq!(
            result1.len(),
            result2.len(),
            "Hugo shortcode reflow should be idempotent. Got: first={}, second={}",
            result1.len(),
            result2.len()
        );

        // Content should not duplicate
        let original_shortcode_count = input.matches("{{<").count();
        let result_shortcode_count = result2.matches("{{<").count();
        assert_eq!(
            original_shortcode_count, result_shortcode_count,
            "Number of shortcodes should not change: original={original_shortcode_count}, result={result_shortcode_count}"
        );
    }

    #[test]
    fn test_hugo_shortcode_multiline() {
        let options = ReflowOptions {
            line_length: 80,
            sentence_per_line: true,
            ..Default::default()
        };

        // Multi-line Hugo shortcode content (collapsed to single line for testing)
        let input =
            r#"{{< figure src="test.png" alt="Line one. Line two. Line three." caption="A caption. With periods." >}}"#;
        let result = reflow_markdown(input, &options);

        // The shortcode should remain intact
        assert!(
            result.contains("{{<") && result.contains(">}}"),
            "Hugo shortcode delimiters should be preserved: {result:?}"
        );

        // Should not duplicate content
        assert_eq!(
            result.matches("test.png").count(),
            1,
            "Image path should appear exactly once: {result:?}"
        );
    }

    #[test]
    fn test_hugo_shortcode_with_text_before_after() {
        let options = ReflowOptions {
            line_length: 80,
            sentence_per_line: true,
            ..Default::default()
        };

        let input = r#"Some text before. {{< shortcode param="value. with period." >}} And text after."#;
        let result = reflow_markdown(input, &options);

        // The shortcode should be preserved
        assert!(
            result.contains(r#"{{< shortcode param="value. with period." >}}"#),
            "Shortcode should be preserved: {result:?}"
        );
    }
}

/// Issue #251: Sentence reflow & formatting markers (bold, italic)
/// When reflowing multi-sentence emphasized text, emphasis markers should
/// continue across line breaks to maintain formatting on each line.
mod issue_251_emphasis_continuation {
    use super::*;

    // ============================================================
    // Part 1: Underscore emphasis parsing
    // ============================================================

    #[test]
    fn test_underscore_italic_parsing() {
        let options = ReflowOptions {
            line_length: 80,
            ..Default::default()
        };

        let input = "This has _italic text_ in it";
        let result = reflow_markdown(input, &options);

        // Underscore italic should be preserved
        assert!(
            result.contains("_italic text_"),
            "Underscore italic should be preserved: {result:?}"
        );
    }

    #[test]
    fn test_underscore_bold_parsing() {
        let options = ReflowOptions {
            line_length: 80,
            ..Default::default()
        };

        let input = "This has __bold text__ in it";
        let result = reflow_markdown(input, &options);

        // Underscore bold should be preserved
        assert!(
            result.contains("__bold text__"),
            "Underscore bold should be preserved: {result:?}"
        );
    }

    #[test]
    fn test_mixed_emphasis_markers() {
        let options = ReflowOptions {
            line_length: 80,
            ..Default::default()
        };

        let input = "Text with *asterisk italic* and _underscore italic_ mixed";
        let result = reflow_markdown(input, &options);

        assert!(
            result.contains("*asterisk italic*"),
            "Asterisk italic preserved: {result:?}"
        );
        assert!(
            result.contains("_underscore italic_"),
            "Underscore italic preserved: {result:?}"
        );
    }

    // ============================================================
    // Part 2: Emphasis continuation across sentence splits
    // ============================================================

    #[test]
    fn test_asterisk_italic_sentence_continuation() {
        let options = ReflowOptions {
            line_length: 0,
            sentence_per_line: true,
            ..Default::default()
        };

        let input = "*Sentence one. Sentence two. Sentence three.*";
        let result = reflow_line(input, &options);

        // Each sentence should have its own italic markers
        assert_eq!(result.len(), 3, "Should have 3 lines: {result:?}");
        assert!(
            result[0].starts_with('*') && result[0].ends_with('*'),
            "First line should have italic markers: {:?}",
            result[0]
        );
        assert!(
            result[1].starts_with('*') && result[1].ends_with('*'),
            "Second line should have italic markers: {:?}",
            result[1]
        );
        assert!(
            result[2].starts_with('*') && result[2].ends_with('*'),
            "Third line should have italic markers: {:?}",
            result[2]
        );
    }

    #[test]
    fn test_underscore_italic_sentence_continuation() {
        let options = ReflowOptions {
            line_length: 0,
            sentence_per_line: true,
            ..Default::default()
        };

        let input = "_Sentence one. Sentence two. Sentence three._";
        let result = reflow_line(input, &options);

        // Each sentence should have its own italic markers (underscore style)
        assert_eq!(result.len(), 3, "Should have 3 lines: {result:?}");
        assert!(
            result[0].starts_with('_') && result[0].ends_with('_'),
            "First line should have underscore markers: {:?}",
            result[0]
        );
        assert!(
            result[1].starts_with('_') && result[1].ends_with('_'),
            "Second line should have underscore markers: {:?}",
            result[1]
        );
        assert!(
            result[2].starts_with('_') && result[2].ends_with('_'),
            "Third line should have underscore markers: {:?}",
            result[2]
        );
    }

    #[test]
    fn test_bold_sentence_continuation() {
        let options = ReflowOptions {
            line_length: 0,
            sentence_per_line: true,
            ..Default::default()
        };

        let input = "**Sentence one. Sentence two.**";
        let result = reflow_line(input, &options);

        // Each sentence should have its own bold markers
        assert_eq!(result.len(), 2, "Should have 2 lines: {result:?}");
        assert!(
            result[0].starts_with("**") && result[0].ends_with("**"),
            "First line should have bold markers: {:?}",
            result[0]
        );
        assert!(
            result[1].starts_with("**") && result[1].ends_with("**"),
            "Second line should have bold markers: {:?}",
            result[1]
        );
    }

    #[test]
    fn test_underscore_bold_sentence_continuation() {
        let options = ReflowOptions {
            line_length: 0,
            sentence_per_line: true,
            ..Default::default()
        };

        let input = "__Sentence one. Sentence two.__";
        let result = reflow_line(input, &options);

        // Each sentence should have its own bold markers (underscore style)
        assert_eq!(result.len(), 2, "Should have 2 lines: {result:?}");
        assert!(
            result[0].starts_with("__") && result[0].ends_with("__"),
            "First line should have underscore bold markers: {:?}",
            result[0]
        );
        assert!(
            result[1].starts_with("__") && result[1].ends_with("__"),
            "Second line should have underscore bold markers: {:?}",
            result[1]
        );
    }

    // ============================================================
    // Part 3: Issue #251 exact reproduction - quoted citations
    // ============================================================

    #[test]
    fn test_issue_251_quoted_citation() {
        let options = ReflowOptions {
            line_length: 0,
            sentence_per_line: true,
            ..Default::default()
        };

        // The exact pattern from issue #251
        let input = r#"He said, _"There is this whole spectrum of crazy futures. But the one that I feel we're almost guaranteed to get. It's the same either way"_ [^ref]."#;
        let result = reflow_markdown(input, &options);

        let lines: Vec<&str> = result.lines().collect();

        // Should split into multiple sentences, each with emphasis markers
        assert!(
            lines.len() >= 3,
            "Should have at least 3 lines for 3 sentences: {result:?}"
        );

        // First line should start with context and have opening emphasis
        assert!(
            lines[0].contains("_\"There is this whole spectrum"),
            "First line should have opening quote with emphasis: {:?}",
            lines[0]
        );

        // Middle lines should have emphasis markers on both ends
        for line in &lines[1..lines.len() - 1] {
            if !line.trim().is_empty() && !line.starts_with("He said") {
                assert!(
                    line.trim().starts_with('_') || line.contains("_\""),
                    "Middle line should start with emphasis: {line:?}"
                );
            }
        }

        // Last line should have closing emphasis with quote and footnote
        let last_line = lines.last().unwrap();
        assert!(
            last_line.contains("\"_") || last_line.ends_with("_"),
            "Last line should have closing emphasis: {last_line:?}"
        );
    }

    #[test]
    fn test_issue_251_simplified() {
        let options = ReflowOptions {
            line_length: 0,
            sentence_per_line: true,
            ..Default::default()
        };

        // Simplified version of issue #251
        let input = r#"_"First sentence. Second sentence."_"#;
        let result = reflow_line(input, &options);

        assert_eq!(result.len(), 2, "Should have 2 lines: {result:?}");

        // First sentence should have opening quote inside emphasis
        assert!(
            result[0].starts_with("_\"") && result[0].ends_with('_'),
            "First line: {:?}",
            result[0]
        );

        // Second sentence should have closing quote inside emphasis
        assert!(
            result[1].starts_with('_') && result[1].ends_with("\"_"),
            "Second line: {:?}",
            result[1]
        );
    }

    // ============================================================
    // Part 4: Edge cases
    // ============================================================

    #[test]
    fn test_emphasis_with_trailing_text() {
        let options = ReflowOptions {
            line_length: 0,
            sentence_per_line: true,
            ..Default::default()
        };

        // Emphasis followed by non-emphasized text
        let input = "Intro: *Sentence one. Sentence two.* And then more text.";
        let result = reflow_markdown(input, &options);

        let lines: Vec<&str> = result.lines().collect();

        // The non-emphasized text should be on its own line
        assert!(
            lines.iter().any(|l| l.contains("And then more text")),
            "Non-emphasized text should be preserved: {result:?}"
        );
    }

    #[test]
    fn test_emphasis_single_sentence_no_change() {
        let options = ReflowOptions {
            line_length: 0,
            sentence_per_line: true,
            ..Default::default()
        };

        // Single sentence - should not be modified
        let input = "*Just one sentence here.*";
        let result = reflow_line(input, &options);

        assert_eq!(result.len(), 1, "Single sentence should stay one line");
        assert_eq!(result[0], "*Just one sentence here.*");
    }

    #[test]
    fn test_emphasis_with_abbreviations() {
        let options = ReflowOptions {
            line_length: 0,
            sentence_per_line: true,
            ..Default::default()
        };

        // Abbreviations should not trigger false sentence splits
        let input = "*Talk to Dr. Smith about the results. Then report back.*";
        let result = reflow_line(input, &options);

        // Should be 2 sentences (split after "results." not after "Dr.")
        assert_eq!(result.len(), 2, "Should have 2 lines: {result:?}");
        assert!(
            result[0].contains("Dr. Smith"),
            "First sentence should contain Dr. Smith"
        );
    }

    #[test]
    fn test_nested_emphasis_sentence_split() {
        let options = ReflowOptions {
            line_length: 0,
            sentence_per_line: true,
            ..Default::default()
        };

        // Bold text containing sentences - each should get markers
        let input = "**First bold sentence. Second bold sentence.**";
        let result = reflow_line(input, &options);

        assert_eq!(result.len(), 2, "Should have 2 lines: {result:?}");

        // Each line should have bold markers
        for (i, line) in result.iter().enumerate() {
            assert!(
                line.starts_with("**") && line.ends_with("**"),
                "Line {i} should have bold markers: {line:?}"
            );
        }
    }

    #[test]
    fn test_emphasis_idempotence() {
        let options = ReflowOptions {
            line_length: 0,
            sentence_per_line: true,
            ..Default::default()
        };

        // Running reflow twice should produce the same result
        let input = "*Sentence one. Sentence two.*";

        let result1 = reflow_markdown(input, &options);
        let result2 = reflow_markdown(&result1, &options);

        assert_eq!(
            result1, result2,
            "Reflow should be idempotent.\nFirst: {result1:?}\nSecond: {result2:?}"
        );
    }

    #[test]
    fn test_multiple_emphasis_spans_on_line() {
        let options = ReflowOptions {
            line_length: 0,
            sentence_per_line: true,
            ..Default::default()
        };

        // Multiple separate emphasis spans
        let input = "*First italic.* Normal text. *Second italic.*";
        let result = reflow_markdown(input, &options);

        let lines: Vec<&str> = result.lines().collect();

        // Should have 3 sentences on 3 lines
        assert_eq!(lines.len(), 3, "Should have 3 lines: {result:?}");
        assert!(lines[0].contains("*First italic.*"));
        assert!(lines[1].contains("Normal text."));
        assert!(lines[2].contains("*Second italic.*"));
    }

    // ============================================================
    // Part 5: Marker type preservation
    // ============================================================

    #[test]
    fn test_marker_type_preserved_asterisk() {
        let options = ReflowOptions {
            line_length: 0,
            sentence_per_line: true,
            ..Default::default()
        };

        let input = "*Sentence one. Sentence two.*";
        let result = reflow_line(input, &options);

        // All markers should be asterisks, not underscores
        for line in &result {
            assert!(
                !line.contains('_'),
                "Asterisk emphasis should not become underscore: {line:?}"
            );
            assert!(
                line.starts_with('*') && line.ends_with('*'),
                "Should use asterisk markers: {line:?}"
            );
        }
    }

    #[test]
    fn test_marker_type_preserved_underscore() {
        let options = ReflowOptions {
            line_length: 0,
            sentence_per_line: true,
            ..Default::default()
        };

        let input = "_Sentence one. Sentence two._";
        let result = reflow_line(input, &options);

        // All markers should be underscores, not asterisks
        for line in &result {
            // Check that we don't have asterisks acting as emphasis markers
            // (asterisks in content are OK, but the wrapper should be underscore)
            assert!(
                line.starts_with('_') && line.ends_with('_'),
                "Should use underscore markers: {line:?}"
            );
        }
    }

    // ============================================================
    // Part 6: Nested emphasis parsing
    // ============================================================

    #[test]
    fn test_nested_italic_containing_bold_asterisk() {
        let options = ReflowOptions {
            line_length: 0,
            sentence_per_line: true,
            ..Default::default()
        };

        // Italic with bold inside: *text **bold** more*
        let input = "*Sentence one. **Bold sentence.** Sentence three.*";
        let result = reflow_line(input, &options);

        // Should parse as a single italic element containing "Sentence one. **Bold sentence.** Sentence three."
        // Each sentence should get italic markers
        assert!(
            result.len() >= 2,
            "Should have at least 2 lines (bold is inside italic): {result:?}"
        );

        // First sentence should have italic marker
        assert!(result[0].starts_with('*'), "First line should start with *: {result:?}");

        // The bold content should be preserved somewhere in the output
        let all_text = result.join("\n");
        assert!(
            all_text.contains("**Bold sentence.**") || all_text.contains("**"),
            "Bold markers should be preserved: {result:?}"
        );
    }

    #[test]
    fn test_nested_italic_containing_bold_underscore() {
        let options = ReflowOptions {
            line_length: 0,
            sentence_per_line: true,
            ..Default::default()
        };

        // Underscore italic with underscore bold inside
        let input = "_Sentence one. __Bold sentence.__ Sentence three._";
        let result = reflow_line(input, &options);

        assert!(result.len() >= 2, "Should have at least 2 lines: {result:?}");

        // First line should use underscore markers
        assert!(result[0].starts_with('_'), "First line should start with _: {result:?}");
    }

    #[test]
    fn test_mixed_nested_emphasis() {
        let options = ReflowOptions {
            line_length: 0,
            sentence_per_line: true,
            ..Default::default()
        };

        // Asterisk italic with underscore bold inside (valid but unusual)
        let input = "*Text with __bold__ inside.*";
        let result = reflow_line(input, &options);

        assert_eq!(result.len(), 1, "Single sentence should be one line: {result:?}");
        assert!(
            result[0].contains("__bold__"),
            "Nested bold should be preserved: {result:?}"
        );
    }

    #[test]
    fn test_double_asterisk_not_confused_with_single() {
        let options = ReflowOptions {
            line_length: 0,
            sentence_per_line: false,
            ..Default::default()
        };

        // **bold** should be parsed as bold, not italic + something
        let input = "Text with **bold** content.";
        let result = reflow_line(input, &options);

        assert_eq!(result.len(), 1, "Should be single line");
        assert!(result[0].contains("**bold**"), "Bold should be preserved: {result:?}");
    }

    #[test]
    fn test_adjacent_emphasis_markers() {
        let options = ReflowOptions {
            line_length: 0,
            sentence_per_line: false,
            ..Default::default()
        };

        // Multiple adjacent emphasis: *italic* followed by **bold**
        let input = "Here is *italic* and **bold** text.";
        let result = reflow_line(input, &options);

        assert_eq!(result.len(), 1);
        assert!(
            result[0].contains("*italic*") && result[0].contains("**bold**"),
            "Both emphasis should be preserved: {result:?}"
        );
    }

    // ============================================================
    // Part 7: Sentence boundary detection with emphasis
    // ============================================================

    #[test]
    fn test_sentence_boundary_after_emphasis() {
        let options = ReflowOptions {
            line_length: 0,
            sentence_per_line: true,
            ..Default::default()
        };

        // Sentence ends inside emphasis, next sentence is plain text
        let input = "Normal text. *Italic sentence.* Another sentence.";
        let result = reflow_line(input, &options);

        assert_eq!(result.len(), 3, "Should have 3 sentences: {result:?}");
        assert_eq!(result[0], "Normal text.");
        assert_eq!(result[1], "*Italic sentence.*");
        assert_eq!(result[2], "Another sentence.");
    }

    #[test]
    fn test_sentence_boundary_before_emphasis() {
        let options = ReflowOptions {
            line_length: 0,
            sentence_per_line: true,
            ..Default::default()
        };

        // Sentence ends in plain text, next sentence starts with emphasis
        let input = "Plain sentence. *Italic sentence.* More text.";
        let result = reflow_line(input, &options);

        assert_eq!(result.len(), 3, "Should have 3 sentences: {result:?}");
        assert_eq!(result[0], "Plain sentence.");
        assert_eq!(result[1], "*Italic sentence.*");
        assert_eq!(result[2], "More text.");
    }

    #[test]
    fn test_sentence_boundary_bold_emphasis() {
        let options = ReflowOptions {
            line_length: 0,
            sentence_per_line: true,
            ..Default::default()
        };

        // Sentence with bold emphasis
        let input = "Before. **Bold sentence.** After.";
        let result = reflow_line(input, &options);

        assert_eq!(result.len(), 3, "Should have 3 sentences: {result:?}");
        assert_eq!(result[0], "Before.");
        assert_eq!(result[1], "**Bold sentence.**");
        assert_eq!(result[2], "After.");
    }

    #[test]
    fn test_sentence_boundary_underscore_emphasis() {
        let options = ReflowOptions {
            line_length: 0,
            sentence_per_line: true,
            ..Default::default()
        };

        // Sentence with underscore emphasis
        let input = "Before. _Underscore sentence._ After.";
        let result = reflow_line(input, &options);

        assert_eq!(result.len(), 3, "Should have 3 sentences: {result:?}");
        assert_eq!(result[0], "Before.");
        assert_eq!(result[1], "_Underscore sentence._");
        assert_eq!(result[2], "After.");
    }

    #[test]
    fn test_sentence_boundary_underscore_bold() {
        let options = ReflowOptions {
            line_length: 0,
            sentence_per_line: true,
            ..Default::default()
        };

        // Sentence with underscore bold
        let input = "Before. __Bold sentence.__ After.";
        let result = reflow_line(input, &options);

        assert_eq!(result.len(), 3, "Should have 3 sentences: {result:?}");
        assert_eq!(result[0], "Before.");
        assert_eq!(result[1], "__Bold sentence.__");
        assert_eq!(result[2], "After.");
    }

    #[test]
    fn test_sentence_boundary_exclamation() {
        let options = ReflowOptions {
            line_length: 0,
            sentence_per_line: true,
            ..Default::default()
        };

        // Sentences ending with exclamation inside emphasis
        let input = "Normal! *Excited!* More.";
        let result = reflow_line(input, &options);

        assert_eq!(result.len(), 3, "Should have 3 sentences: {result:?}");
        assert_eq!(result[0], "Normal!");
        assert_eq!(result[1], "*Excited!*");
        assert_eq!(result[2], "More.");
    }

    #[test]
    fn test_sentence_boundary_question() {
        let options = ReflowOptions {
            line_length: 0,
            sentence_per_line: true,
            ..Default::default()
        };

        // Sentences ending with question mark inside emphasis
        let input = "Really? *Is it?* Yes.";
        let result = reflow_line(input, &options);

        assert_eq!(result.len(), 3, "Should have 3 sentences: {result:?}");
        assert_eq!(result[0], "Really?");
        assert_eq!(result[1], "*Is it?*");
        assert_eq!(result[2], "Yes.");
    }

    // ============================================================
    // Part 8: CJK (Chinese/Japanese/Korean) punctuation
    // ============================================================

    #[test]
    fn test_cjk_chinese_ideographic_full_stop() {
        let options = ReflowOptions {
            line_length: 0,
            sentence_per_line: true,
            ..Default::default()
        };

        // Chinese text with ideographic full stop (。)
        let input = "这是第一句。这是第二句。";
        let result = reflow_line(input, &options);

        assert_eq!(result.len(), 2, "Should have 2 sentences: {result:?}");
        assert_eq!(result[0], "这是第一句。");
        assert_eq!(result[1], "这是第二句。");
    }

    #[test]
    fn test_cjk_fullwidth_exclamation() {
        let options = ReflowOptions {
            line_length: 0,
            sentence_per_line: true,
            ..Default::default()
        };

        // Chinese text with fullwidth exclamation mark (！)
        let input = "太棒了！继续努力！";
        let result = reflow_line(input, &options);

        assert_eq!(result.len(), 2, "Should have 2 sentences: {result:?}");
        assert_eq!(result[0], "太棒了！");
        assert_eq!(result[1], "继续努力！");
    }

    #[test]
    fn test_cjk_fullwidth_question() {
        let options = ReflowOptions {
            line_length: 0,
            sentence_per_line: true,
            ..Default::default()
        };

        // Chinese text with fullwidth question mark (？)
        let input = "你好吗？我很好。";
        let result = reflow_line(input, &options);

        assert_eq!(result.len(), 2, "Should have 2 sentences: {result:?}");
        assert_eq!(result[0], "你好吗？");
        assert_eq!(result[1], "我很好。");
    }

    #[test]
    fn test_cjk_japanese_mixed() {
        let options = ReflowOptions {
            line_length: 0,
            sentence_per_line: true,
            ..Default::default()
        };

        // Japanese text with hiragana and kanji
        let input = "これは日本語です。もう一文。";
        let result = reflow_line(input, &options);

        assert_eq!(result.len(), 2, "Should have 2 sentences: {result:?}");
        assert_eq!(result[0], "これは日本語です。");
        assert_eq!(result[1], "もう一文。");
    }

    #[test]
    fn test_mixed_cjk_and_english() {
        let options = ReflowOptions {
            line_length: 0,
            sentence_per_line: true,
            ..Default::default()
        };

        // Mixed Chinese and English
        let input = "Hello。你好。World.";
        let result = reflow_line(input, &options);

        assert_eq!(result.len(), 3, "Should have 3 sentences: {result:?}");
        assert_eq!(result[0], "Hello。");
        assert_eq!(result[1], "你好。");
        assert_eq!(result[2], "World.");
    }

    #[test]
    fn test_cjk_with_emphasis() {
        let options = ReflowOptions {
            line_length: 0,
            sentence_per_line: true,
            ..Default::default()
        };

        // Chinese text with emphasis markers
        let input = "普通文字。*强调文字。* 更多文字。";
        let result = reflow_line(input, &options);

        assert_eq!(result.len(), 3, "Should have 3 sentences: {result:?}");
        assert_eq!(result[0], "普通文字。");
        assert_eq!(result[1], "*强调文字。*");
        assert_eq!(result[2], "更多文字。");
    }

    // ============================================================
    // Part 9: Edge cases and stress tests
    // ============================================================

    #[test]
    fn test_url_inside_emphasis() {
        let options = ReflowOptions {
            line_length: 0,
            sentence_per_line: true,
            ..Default::default()
        };

        // URL inside emphasis should be preserved
        let input = "Check *https://example.com* for details. More text.";
        let result = reflow_line(input, &options);

        // URL should stay intact
        assert!(
            result[0].contains("https://example.com"),
            "URL should be preserved: {result:?}"
        );
    }

    #[test]
    fn test_code_span_inside_emphasis() {
        let options = ReflowOptions {
            line_length: 0,
            sentence_per_line: false,
            ..Default::default()
        };

        // Code span inside emphasis
        let input = "Use *the `code` function* to process.";
        let result = reflow_line(input, &options);

        assert_eq!(result.len(), 1);
        assert!(
            result[0].contains("`code`"),
            "Code span should be preserved: {result:?}"
        );
    }

    #[test]
    fn test_link_inside_emphasis() {
        let options = ReflowOptions {
            line_length: 0,
            sentence_per_line: false,
            ..Default::default()
        };

        // Link inside emphasis
        let input = "See *[the link](https://example.com)* for info.";
        let result = reflow_line(input, &options);

        assert_eq!(result.len(), 1);
        assert!(result[0].contains("[the link]"), "Link should be preserved: {result:?}");
    }

    #[test]
    fn test_very_long_emphasis_text() {
        let options = ReflowOptions {
            line_length: 0,
            sentence_per_line: true,
            ..Default::default()
        };

        // Very long emphasized text with multiple sentences
        // Note: Must NOT have trailing space before closing *, or CommonMark won't
        // recognize it as a right-flanking delimiter
        let long_sentence = "This is a sentence. ".repeat(49) + "This is a sentence.";
        let input = format!("*{long_sentence}*");
        let result = reflow_line(&input, &options);

        // Should split into 50 sentences
        assert_eq!(result.len(), 50, "Should have 50 sentences");

        // Each line should have emphasis markers
        for line in &result {
            assert!(
                line.starts_with('*') && line.ends_with('*'),
                "Each line should have emphasis: {line}"
            );
        }
    }

    #[test]
    fn test_consecutive_emphasis_markers() {
        let options = ReflowOptions {
            line_length: 0,
            sentence_per_line: false,
            ..Default::default()
        };

        // Multiple consecutive emphasis elements
        let input = "*italic* **bold** *more italic*";
        let result = reflow_line(input, &options);

        assert_eq!(result.len(), 1);
        assert!(
            result[0].contains("*italic*") && result[0].contains("**bold**") && result[0].contains("*more italic*"),
            "All emphasis should be preserved: {result:?}"
        );
    }

    #[test]
    fn test_emphasis_at_line_boundaries() {
        let options = ReflowOptions {
            line_length: 0,
            sentence_per_line: true,
            ..Default::default()
        };

        // Emphasis at start and end of content
        let input = "*Start sentence.* Middle. *End sentence.*";
        let result = reflow_line(input, &options);

        assert_eq!(result.len(), 3, "Should have 3 sentences: {result:?}");
        assert_eq!(result[0], "*Start sentence.*");
        assert_eq!(result[1], "Middle.");
        assert_eq!(result[2], "*End sentence.*");
    }

    #[test]
    fn test_single_character_emphasis() {
        let options = ReflowOptions {
            line_length: 0,
            sentence_per_line: false,
            ..Default::default()
        };

        // Single character in emphasis
        let input = "Press *x* to continue.";
        let result = reflow_line(input, &options);

        assert_eq!(result.len(), 1);
        assert!(
            result[0].contains("*x*"),
            "Single char emphasis should be preserved: {result:?}"
        );
    }

    #[test]
    fn test_empty_emphasis_handled() {
        let options = ReflowOptions {
            line_length: 0,
            sentence_per_line: false,
            ..Default::default()
        };

        // Empty emphasis (edge case)
        let input = "Text ** more text";
        let result = reflow_line(input, &options);

        // Should not crash, empty emphasis treated as text
        assert_eq!(result.len(), 1);
    }

    // ============================================================
    // Part 10: Known limitations (documented behavior)
    // ============================================================

    #[test]
    fn test_limitation_lowercase_after_period() {
        let options = ReflowOptions {
            line_length: 0,
            sentence_per_line: true,
            ..Default::default()
        };

        // Limitation: lowercase after period is not treated as sentence boundary
        // This is intentional to avoid false positives with abbreviations
        let input = "Use e.g. this method. And this.";
        let result = reflow_line(input, &options);

        // Should only split at "method. And" (uppercase A)
        // The "e.g. this" should not split because 't' is lowercase
        assert!(!result.is_empty(), "Should have at least 1 line: {result:?}");
    }

    #[test]
    fn test_limitation_triple_emphasis() {
        let options = ReflowOptions {
            line_length: 0,
            sentence_per_line: false,
            ..Default::default()
        };

        // Triple emphasis (bold + italic)
        // Current implementation treats this as separate elements
        let input = "This is ***bold italic*** text.";
        let result = reflow_line(input, &options);

        // Should preserve the content even if parsing isn't perfect
        assert_eq!(result.len(), 1);
        assert!(
            result[0].contains("bold italic"),
            "Content should be preserved: {result:?}"
        );
    }
}

// =============================================================================
// UTF-8 / Multi-byte Character Tests
// =============================================================================
// These tests verify that text reflow correctly handles multi-byte UTF-8
// characters without panicking due to byte/character index mismatches.

#[test]
fn test_utf8_numbered_list_with_chinese_characters() {
    // Regression test: numbered lists with multi-byte chars before content
    // Previously caused panic due to byte/char index mismatch
    let options = ReflowOptions {
        line_length: 80,
        ..Default::default()
    };

    let input = "1. 你好世界 - Hello World in Chinese\n2. 日本語 - Japanese text\n";
    let result = reflow_markdown(input, &options);

    // Should not panic and should preserve the content
    assert!(result.contains("你好世界"), "Chinese characters should be preserved");
    assert!(result.contains("日本語"), "Japanese characters should be preserved");
    assert!(result.contains("1."), "List numbering should be preserved");
    assert!(result.contains("2."), "List numbering should be preserved");
}

#[test]
fn test_utf8_bullet_list_with_emoji() {
    // Test bullet lists with emoji (multi-byte UTF-8)
    let options = ReflowOptions {
        line_length: 80,
        ..Default::default()
    };

    let input = "- 🎉 Party time!\n- 🚀 Rocket launch\n- 🌟 Starry night\n";
    let result = reflow_markdown(input, &options);

    assert!(result.contains("🎉"), "Emoji should be preserved");
    assert!(result.contains("🚀"), "Emoji should be preserved");
    assert!(result.contains("🌟"), "Emoji should be preserved");
}

#[test]
fn test_utf8_indented_list_with_cyrillic() {
    // Test indented lists with Cyrillic characters
    let options = ReflowOptions {
        line_length: 80,
        ..Default::default()
    };

    let input = "   - Привет мир (Hello World in Russian)\n   - Добрый день (Good day)\n";
    let result = reflow_markdown(input, &options);

    assert!(result.contains("Привет"), "Cyrillic should be preserved");
    assert!(result.contains("Добрый"), "Cyrillic should be preserved");
}

#[test]
fn test_utf8_blockquote_with_arabic() {
    // Test blockquotes with Arabic text (RTL, multi-byte)
    let options = ReflowOptions {
        line_length: 80,
        ..Default::default()
    };

    let input = "> مرحبا بالعالم - Hello World in Arabic\n";
    let result = reflow_markdown(input, &options);

    assert!(result.contains("مرحبا"), "Arabic text should be preserved");
    assert!(result.starts_with(">"), "Blockquote marker should be preserved");
}

#[test]
fn test_utf8_blockquote_with_leading_spaces_and_unicode() {
    // Test blockquotes with leading whitespace and unicode
    let options = ReflowOptions {
        line_length: 80,
        ..Default::default()
    };

    let input = "   > 日本語テキスト with some English\n";
    let result = reflow_markdown(input, &options);

    assert!(result.contains("日本語"), "Japanese should be preserved");
    assert!(result.contains(">"), "Blockquote marker should be preserved");
}

#[test]
fn test_utf8_mixed_scripts_in_numbered_list() {
    // Test numbered list with mixed scripts (Latin, Chinese, emoji)
    let options = ReflowOptions {
        line_length: 80,
        ..Default::default()
    };

    let input = "1. Hello 你好 🌍 World\n2. مرحبا Привет 🎉 Mixed\n3. Normal ASCII text\n";
    let result = reflow_markdown(input, &options);

    // All content should be preserved without panic
    assert!(result.contains("Hello"), "Latin preserved");
    assert!(result.contains("你好"), "Chinese preserved");
    assert!(result.contains("🌍"), "Emoji preserved");
    assert!(result.contains("مرحبا"), "Arabic preserved");
    assert!(result.contains("Привет"), "Cyrillic preserved");
}

#[test]
fn test_utf8_list_marker_after_multibyte_indent() {
    // Edge case: what if the indent itself somehow contains multi-byte chars?
    // This tests the boundary conditions of our byte-based space skipping
    let options = ReflowOptions {
        line_length: 80,
        ..Default::default()
    };

    // Standard indentation with multi-byte content
    let input = "    1. 日本語 text after marker\n";
    let result = reflow_markdown(input, &options);

    assert!(result.contains("日本語"), "Content after marker preserved");
    assert!(result.contains("1."), "List marker preserved");
}

#[test]
fn test_utf8_multiple_spaces_after_marker_with_unicode() {
    // Test that multiple spaces after list marker are handled correctly
    // even when followed by multi-byte characters
    let options = ReflowOptions {
        line_length: 80,
        ..Default::default()
    };

    // Multiple spaces after the marker
    let input = "-   🎉 Extra spaces before emoji\n1.   日本語 Extra spaces before Japanese\n";
    let result = reflow_markdown(input, &options);

    assert!(result.contains("🎉"), "Emoji preserved after extra spaces");
    assert!(result.contains("日本語"), "Japanese preserved after extra spaces");
}

#[test]
fn test_utf8_very_long_unicode_line_reflow() {
    // Test that long lines with unicode characters reflow correctly
    let options = ReflowOptions {
        line_length: 40,
        ..Default::default()
    };

    let input = "这是一个很长的中文句子，包含了很多汉字，需要被正确地换行处理。";
    let result = reflow_line(input, &options);

    // Should reflow without panic
    assert!(!result.is_empty(), "Should produce output");
    // All characters should be preserved across lines
    let joined = result.join("");
    assert!(joined.contains("中文"), "Chinese text preserved after reflow");
}

#[test]
fn test_utf8_combining_characters() {
    // Test with combining characters (e.g., accents that combine with base chars)
    let options = ReflowOptions {
        line_length: 80,
        ..Default::default()
    };

    // é can be represented as e + combining acute accent
    let input = "- Café résumé naïve\n";
    let result = reflow_markdown(input, &options);

    assert!(
        result.contains("Café") || result.contains("Cafe"),
        "Accented text preserved"
    );
}

#[test]
fn test_utf8_zero_width_characters() {
    // Test with zero-width characters (joiners, non-joiners)
    let options = ReflowOptions {
        line_length: 80,
        ..Default::default()
    };

    // Zero-width space (U+200B) and zero-width joiner (U+200D)
    let input = "1. Text\u{200B}with\u{200D}invisible\n";
    let result = reflow_markdown(input, &options);

    // Should not panic, content should be mostly preserved
    assert!(result.contains("Text"), "Base text preserved");
    assert!(result.contains("invisible"), "Text after zero-width preserved");
}

// ============================================================
// Sentence reflow with quotes
// ============================================================

#[test]
fn test_sentence_split_when_next_sentence_starts_with_quote() {
    // Sentence ends with period, next sentence starts with opening quote
    let options = ReflowOptions {
        line_length: 0,
        sentence_per_line: true,
        ..Default::default()
    };

    let input = r#"Builders create significant business value. "AI native" workers set the AI vision."#;
    let result = reflow_markdown(input, &options);

    let lines: Vec<&str> = result.lines().collect();
    assert_eq!(lines.len(), 2, "Should split into 2 sentences: {result:?}");
    assert!(lines[0].ends_with("value."), "First sentence ends with 'value.'");
    assert!(
        lines[1].starts_with("\"AI"),
        "Second sentence starts with opening quote"
    );
}

#[test]
fn test_sentence_split_when_period_inside_closing_quote() {
    // Sentence ends with period inside quote, next sentence follows
    let options = ReflowOptions {
        line_length: 0,
        sentence_per_line: true,
        ..Default::default()
    };

    let input = r#"Users electable "to make Gemini helpful." Personal context is provided."#;
    let result = reflow_markdown(input, &options);

    let lines: Vec<&str> = result.lines().collect();
    assert_eq!(lines.len(), 2, "Should split into 2 sentences: {result:?}");
    assert!(
        lines[0].ends_with("helpful.\""),
        "First sentence ends with closing quote after period: {:?}",
        lines[0]
    );
    assert!(
        lines[1].starts_with("Personal"),
        "Second sentence starts with 'Personal'"
    );
}

#[test]
fn test_curly_quotes_sentence_boundary() {
    // Curly/smart quotes should also be recognized
    let options = ReflowOptions {
        line_length: 0,
        sentence_per_line: true,
        ..Default::default()
    };

    // Using Unicode escape sequences for curly quotes
    // \u{201C} = left double quotation mark "
    // \u{201D} = right double quotation mark "
    let input = "First sentence.\u{201C}Second sentence.\u{201D} Third sentence.";
    let result = reflow_markdown(input, &options);

    // Note: The left curly quote after period is trickier because there's no space
    // But the right curly quote followed by space should work
    let lines: Vec<&str> = result.lines().collect();
    assert!(lines.len() >= 2, "Should split at sentence boundaries: {result:?}");
}

#[test]
fn test_exclamation_with_quotes() {
    let options = ReflowOptions {
        line_length: 0,
        sentence_per_line: true,
        ..Default::default()
    };

    let input = r#"She said "Amazing!" He replied "Incredible!""#;
    let result = reflow_markdown(input, &options);

    let lines: Vec<&str> = result.lines().collect();
    assert_eq!(lines.len(), 2, "Should split at exclamation: {result:?}");
    assert!(
        lines[0].ends_with("Amazing!\""),
        "First sentence should end with exclamation and quote"
    );
}

#[test]
fn test_question_with_quotes() {
    let options = ReflowOptions {
        line_length: 0,
        sentence_per_line: true,
        ..Default::default()
    };

    let input = r#"He asked "Really?" She answered yes."#;
    let result = reflow_markdown(input, &options);

    let lines: Vec<&str> = result.lines().collect();
    assert_eq!(lines.len(), 2, "Should split at question mark: {result:?}");
    assert!(
        lines[0].ends_with("Really?\""),
        "First sentence should end with question and quote"
    );
}

#[test]
fn test_single_quote_sentence_boundary() {
    let options = ReflowOptions {
        line_length: 0,
        sentence_per_line: true,
        ..Default::default()
    };

    let input = "The character said 'Done.' Next line follows.";
    let result = reflow_markdown(input, &options);

    let lines: Vec<&str> = result.lines().collect();
    assert_eq!(lines.len(), 2, "Should split at period with single quote: {result:?}");
    assert!(lines[0].ends_with("Done.'"), "First sentence ends with single quote");
}

#[test]
fn test_mixed_quotes_and_emphasis() {
    let options = ReflowOptions {
        line_length: 0,
        sentence_per_line: true,
        ..Default::default()
    };

    let input = r#"He wrote *"Important text."* Then continued."#;
    let result = reflow_markdown(input, &options);

    let lines: Vec<&str> = result.lines().collect();
    assert_eq!(
        lines.len(),
        2,
        "Should split with mixed emphasis and quotes: {result:?}"
    );
}

// =============================================================================
// Email autolink tests
// Regression tests for issue #339 where email autolinks caused infinite loops
// =============================================================================

#[test]
fn test_email_autolink_not_treated_as_html_tag() {
    // Issue #339: Email autolinks like <user@example.com> were being treated as HTML tags,
    // causing content duplication and infinite loops in sentence-per-line reflow
    let options = ReflowOptions {
        line_length: 0,
        sentence_per_line: true,
        ..Default::default()
    };

    let input = "First sentence here. Reach me at <test@example.com>.";
    let result = reflow_markdown(input, &options);

    let lines: Vec<&str> = result.lines().collect();
    assert_eq!(lines.len(), 2, "Should split into 2 sentences: {result:?}");
    assert_eq!(lines[0], "First sentence here.");
    assert_eq!(lines[1], "Reach me at <test@example.com>.");
}

#[test]
fn test_email_autolink_at_end_of_sentence() {
    let options = ReflowOptions {
        line_length: 0,
        sentence_per_line: true,
        ..Default::default()
    };

    let input = "Contact us at <support@company.com>. We respond within 24 hours.";
    let result = reflow_markdown(input, &options);

    let lines: Vec<&str> = result.lines().collect();
    assert_eq!(lines.len(), 2, "Should split into 2 sentences: {result:?}");
    assert_eq!(lines[0], "Contact us at <support@company.com>.");
    assert_eq!(lines[1], "We respond within 24 hours.");
}

#[test]
fn test_email_autolink_mid_sentence() {
    let options = ReflowOptions {
        line_length: 0,
        sentence_per_line: true,
        ..Default::default()
    };

    let input = "Email <admin@test.org> for more info. Thank you.";
    let result = reflow_markdown(input, &options);

    let lines: Vec<&str> = result.lines().collect();
    assert_eq!(lines.len(), 2, "Should split into 2 sentences: {result:?}");
    assert_eq!(lines[0], "Email <admin@test.org> for more info.");
    assert_eq!(lines[1], "Thank you.");
}

#[test]
fn test_email_autolink_complex_domain() {
    let options = ReflowOptions {
        line_length: 0,
        sentence_per_line: true,
        ..Default::default()
    };

    let input = "Reach me at <user.name+tag@sub.domain.example.com>. Thanks!";
    let result = reflow_markdown(input, &options);

    let lines: Vec<&str> = result.lines().collect();
    assert_eq!(lines.len(), 2, "Should split into 2 sentences: {result:?}");
    assert!(lines[0].contains("<user.name+tag@sub.domain.example.com>"));
}

#[test]
fn test_url_autolinks_still_work() {
    // Make sure URL autolinks still work correctly after the email autolink fix
    let options = ReflowOptions {
        line_length: 0,
        sentence_per_line: true,
        ..Default::default()
    };

    let input = "Visit <https://example.com> for details. See you there.";
    let result = reflow_markdown(input, &options);

    let lines: Vec<&str> = result.lines().collect();
    assert_eq!(lines.len(), 2, "Should split into 2 sentences: {result:?}");
    assert_eq!(lines[0], "Visit <https://example.com> for details.");
    assert_eq!(lines[1], "See you there.");
}

#[test]
fn test_html_tag_vs_email_autolink_distinction() {
    // Test that real HTML tags are still processed correctly
    let options = ReflowOptions {
        line_length: 0,
        sentence_per_line: true,
        ..Default::default()
    };

    // HTML tags should be kept as-is
    let input = "Use the <code>command</code> here. It's simple.";
    let result = reflow_markdown(input, &options);

    let lines: Vec<&str> = result.lines().collect();
    assert_eq!(lines.len(), 2, "Should split into 2 sentences: {result:?}");
    assert!(lines[0].contains("<code>"));
    assert!(lines[0].contains("</code>"));
}

#[test]
fn test_email_autolink_no_content_duplication() {
    // Regression test for the content extraction bug in issue #339
    // The bug caused text BEFORE the email to be duplicated in the HtmlTag element
    // e.g., "Reach me at <test@example.com>" would create:
    //   HtmlTag("Reach me at <test@example.com>") instead of just the email
    let options = ReflowOptions {
        line_length: 0,
        sentence_per_line: true,
        ..Default::default()
    };

    let input = "Prefix text <test@example.com> suffix text.";
    let result = reflow_markdown(input, &options);

    // Should NOT contain duplicated prefix
    assert_eq!(
        result.matches("Prefix text").count(),
        1,
        "Prefix should appear exactly once: {result:?}"
    );
    // Email should appear exactly once
    assert_eq!(
        result.matches("<test@example.com>").count(),
        1,
        "Email should appear exactly once: {result:?}"
    );
}

#[test]
fn test_multiple_emails_in_sentence() {
    let options = ReflowOptions {
        line_length: 0,
        sentence_per_line: true,
        ..Default::default()
    };

    let input = "Contact <sales@example.com> or <support@example.com> for help. Thanks!";
    let result = reflow_markdown(input, &options);

    let lines: Vec<&str> = result.lines().collect();
    assert_eq!(lines.len(), 2, "Should split into 2 sentences: {result:?}");
    assert!(lines[0].contains("<sales@example.com>"));
    assert!(lines[0].contains("<support@example.com>"));
}

#[test]
fn test_email_and_html_tags_mixed() {
    let options = ReflowOptions {
        line_length: 0,
        sentence_per_line: true,
        ..Default::default()
    };

    let input = "Use <code>git</code> or email <dev@example.com> for help. Done.";
    let result = reflow_markdown(input, &options);

    let lines: Vec<&str> = result.lines().collect();
    assert_eq!(lines.len(), 2, "Should split into 2 sentences: {result:?}");
    // Verify email is preserved correctly (the main focus of issue #339)
    assert!(lines[0].contains("<dev@example.com>"));
    // Verify HTML tags are present (opening and closing)
    assert!(lines[0].contains("<code>"));
    assert!(lines[0].contains("</code>"));
}

#[test]
fn test_email_and_url_autolinks_mixed() {
    let options = ReflowOptions {
        line_length: 0,
        sentence_per_line: true,
        ..Default::default()
    };

    let input = "Visit <https://example.com> or email <info@example.com> for details. Bye.";
    let result = reflow_markdown(input, &options);

    let lines: Vec<&str> = result.lines().collect();
    assert_eq!(lines.len(), 2, "Should split into 2 sentences: {result:?}");
    assert!(lines[0].contains("<https://example.com>"));
    assert!(lines[0].contains("<info@example.com>"));
}

#[test]
fn test_email_with_long_tld() {
    // TLDs like .museum, .photography exist
    let options = ReflowOptions {
        line_length: 0,
        sentence_per_line: true,
        ..Default::default()
    };

    let input = "Contact <curator@art.museum> for exhibitions. Welcome!";
    let result = reflow_markdown(input, &options);

    let lines: Vec<&str> = result.lines().collect();
    assert_eq!(lines.len(), 2, "Should split into 2 sentences: {result:?}");
    assert!(lines[0].contains("<curator@art.museum>"));
}

#[test]
fn test_email_with_numbers_in_local_part() {
    let options = ReflowOptions {
        line_length: 0,
        sentence_per_line: true,
        ..Default::default()
    };

    let input = "Email <user123@test99.example.com> for access. Thanks!";
    let result = reflow_markdown(input, &options);

    let lines: Vec<&str> = result.lines().collect();
    assert_eq!(lines.len(), 2, "Should split into 2 sentences: {result:?}");
    assert!(lines[0].contains("<user123@test99.example.com>"));
}

#[test]
fn test_email_with_percent_encoding_chars() {
    // EMAIL_PATTERN allows % in local part for percent-encoded chars
    let options = ReflowOptions {
        line_length: 0,
        sentence_per_line: true,
        ..Default::default()
    };

    let input = "Email <user%40special@example.com> if needed. Done!";
    let result = reflow_markdown(input, &options);

    let lines: Vec<&str> = result.lines().collect();
    assert_eq!(lines.len(), 2, "Should split into 2 sentences: {result:?}");
    assert!(lines[0].contains("<user%40special@example.com>"));
}

#[test]
fn test_invalid_email_single_char_tld_treated_as_html() {
    // <a@b.c> has single-char TLD which doesn't exist - treated as HTML tag
    // This should still work (preserved as-is) without causing issues
    let options = ReflowOptions {
        line_length: 0,
        sentence_per_line: true,
        ..Default::default()
    };

    let input = "Check <a@b.c> for testing. Done!";
    let result = reflow_markdown(input, &options);

    let lines: Vec<&str> = result.lines().collect();
    assert_eq!(lines.len(), 2, "Should split into 2 sentences: {result:?}");
    // Should be preserved regardless of classification
    assert!(lines[0].contains("<a@b.c>"));
}

#[test]
fn test_invalid_email_no_tld_treated_as_html() {
    // <user@localhost> has no TLD - treated as HTML tag
    let options = ReflowOptions {
        line_length: 0,
        sentence_per_line: true,
        ..Default::default()
    };

    let input = "Use <user@localhost> locally. Done!";
    let result = reflow_markdown(input, &options);

    let lines: Vec<&str> = result.lines().collect();
    assert_eq!(lines.len(), 2, "Should split into 2 sentences: {result:?}");
    assert!(lines[0].contains("<user@localhost>"));
}

#[test]
fn test_email_at_very_start_of_text() {
    let options = ReflowOptions {
        line_length: 0,
        sentence_per_line: true,
        ..Default::default()
    };

    let input = "<start@example.com> is the contact. Use it.";
    let result = reflow_markdown(input, &options);

    let lines: Vec<&str> = result.lines().collect();
    assert_eq!(lines.len(), 2, "Should split into 2 sentences: {result:?}");
    assert!(lines[0].starts_with("<start@example.com>"));
}

#[test]
fn test_email_as_only_content() {
    let options = ReflowOptions {
        line_length: 0,
        sentence_per_line: true,
        ..Default::default()
    };

    let input = "<only@example.com>";
    let result = reflow_markdown(input, &options);

    assert_eq!(result, "<only@example.com>");
}

#[test]
fn test_consecutive_emails() {
    let options = ReflowOptions {
        line_length: 0,
        sentence_per_line: true,
        ..Default::default()
    };

    let input = "<first@example.com><second@example.com> are contacts. Done.";
    let result = reflow_markdown(input, &options);

    let lines: Vec<&str> = result.lines().collect();
    assert_eq!(lines.len(), 2, "Should split into 2 sentences: {result:?}");
    assert!(lines[0].contains("<first@example.com>"));
    assert!(lines[0].contains("<second@example.com>"));
}

#[test]
fn test_email_idempotency() {
    // Applying reflow twice should produce the same result
    let options = ReflowOptions {
        line_length: 0,
        sentence_per_line: true,
        ..Default::default()
    };

    let input = "Contact <test@example.com> for help. Thank you for reading.";
    let first_pass = reflow_markdown(input, &options);
    let second_pass = reflow_markdown(&first_pass, &options);

    assert_eq!(first_pass, second_pass, "Reflow should be idempotent");
}

#[test]
fn test_email_with_hyphen_in_domain() {
    let options = ReflowOptions {
        line_length: 0,
        sentence_per_line: true,
        ..Default::default()
    };

    let input = "Email <contact@my-company.example.com> for info. Thanks!";
    let result = reflow_markdown(input, &options);

    let lines: Vec<&str> = result.lines().collect();
    assert_eq!(lines.len(), 2, "Should split into 2 sentences: {result:?}");
    assert!(lines[0].contains("<contact@my-company.example.com>"));
}

#[test]
fn test_html_entity_extraction_no_duplication() {
    // Regression test: html_entity extraction had the same bug
    let options = ReflowOptions {
        line_length: 0,
        sentence_per_line: true,
        ..Default::default()
    };

    let input = "Use &nbsp; for spacing. Done!";
    let result = reflow_markdown(input, &options);

    assert_eq!(
        result.matches("Use").count(),
        1,
        "Prefix should appear exactly once: {result:?}"
    );
    assert_eq!(
        result.matches("&nbsp;").count(),
        1,
        "Entity should appear exactly once: {result:?}"
    );
}

#[test]
fn test_hugo_shortcode_extraction_no_duplication() {
    // Regression test: hugo_shortcode extraction had the same bug
    let options = ReflowOptions {
        line_length: 0,
        sentence_per_line: true,
        ..Default::default()
    };

    let input = "Include {{< figure src=\"test.png\" >}} here. Done!";
    let result = reflow_markdown(input, &options);

    assert_eq!(
        result.matches("Include").count(),
        1,
        "Prefix should appear exactly once: {result:?}"
    );
    assert_eq!(
        result.matches("{{< figure").count(),
        1,
        "Shortcode should appear exactly once: {result:?}"
    );
}
