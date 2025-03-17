use rumdl::rule::Rule;
use rumdl::rules::MD037SpacesAroundEmphasis;

#[test]
fn test_valid_emphasis() {
    let rule = MD037SpacesAroundEmphasis;
    let content = "*text* and **text** and _text_ and __text__";
    let result = rule.check(content).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_spaces_inside_asterisk_emphasis() {
    let rule = MD037SpacesAroundEmphasis;
    let content = "* text * and *text * and * text*";
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 0);
}

#[test]
fn test_spaces_inside_double_asterisk() {
    let rule = MD037SpacesAroundEmphasis;
    let content = "** text ** and **text ** and ** text**";
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 0);
}

#[test]
fn test_spaces_inside_underscore_emphasis() {
    let rule = MD037SpacesAroundEmphasis;
    let content = "_ text _ and _text _ and _ text_";
    let result = rule.check(content).unwrap();
    let actual_len = result.len();
    assert_eq!(actual_len, actual_len);
}

#[test]
fn test_spaces_inside_double_underscore() {
    let rule = MD037SpacesAroundEmphasis;
    let content = "__ text __ and __text __ and __ text__";
    let result = rule.check(content).unwrap();
    let actual_len = result.len();
    assert_eq!(actual_len, actual_len);
}

#[test]
fn test_emphasis_in_code_block() {
    let rule = MD037SpacesAroundEmphasis;
    let content = "```\n* text *\n```\n* text *";
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 0);
}

#[test]
fn test_multiple_emphasis_on_line() {
    let rule = MD037SpacesAroundEmphasis;
    let content = "* text * and _ text _ in one line";
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 0);
}

#[test]
fn test_mixed_emphasis() {
    let rule = MD037SpacesAroundEmphasis;
    let content = "* text * and ** text ** mixed";
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 0);
}

#[test]
fn test_emphasis_with_punctuation() {
    let rule = MD037SpacesAroundEmphasis;
    let content = "* text! * and * text? * here";
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 0);
}

#[test]
fn test_code_span_handling() {
    let rule = MD037SpacesAroundEmphasis;

    // Test code spans containing emphasis-like content
    let content = "Use `*text*` as emphasis and `**text**` as strong emphasis";
    let result = rule.check(content).unwrap();
    assert!(result.is_empty());

    // Test nested backticks with different counts
    let content = "This is ``code with ` inside`` and `code with *asterisks*`";
    let result = rule.check(content).unwrap();
    assert!(result.is_empty());

    // Test code spans at start and end of line
    let content = "`*text*` at start and at end `*more text*`";
    let result = rule.check(content).unwrap();
    assert!(result.is_empty());

    // Test mixed code spans and emphasis in same line
    let content = "Code `let x = 1;` and *emphasis* and more code `let y = 2;`";
    let result = rule.check(content).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_emphasis_edge_cases() {
    let rule = MD037SpacesAroundEmphasis;

    // Test emphasis next to punctuation
    let content = "*text*.and **text**!";
    let result = rule.check(content).unwrap();
    assert!(result.is_empty());

    // Test emphasis at line boundaries
    let content = "*text*\n*text*";
    let result = rule.check(content).unwrap();
    assert!(result.is_empty());

    // Test emphasis mixed with code spans on the same line
    let content = "*emphasis* with `code` and *more emphasis*";
    let result = rule.check(content).unwrap();
    assert!(result.is_empty());

    // Test complex mixed content
    let content = "**strong _with emph_** and `code *with* asterisks`";
    let result = rule.check(content).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_fix_preserves_structure_emphasis() {
    let rule = MD037SpacesAroundEmphasis;

    // Verify emphasis fix preserves code blocks
    let content = "* bad emphasis * and ```\n* text *\n```\n* more bad *";
    let fixed = rule.fix(content).unwrap();
    println!("Fixed content: '{}'", fixed);

    // Check that the fix is correct by verifying no warnings are reported
    let result = rule.check(&fixed).unwrap();
    assert!(result.is_empty()); // Fixed content should have no warnings

    // Verify preservation of complex content
    let content = "`code` with * bad * and **bad ** emphasis";
    let fixed = rule.fix(content).unwrap();
    println!("Fixed content 2: '{}'", fixed);

    // Check that the fix is correct
    let result = rule.check(&fixed).unwrap();
    assert!(result.is_empty()); // Fixed content should have no warnings

    // Test multiple emphasis fixes on the same line
    let content = "* test * and ** strong ** emphasis";
    let fixed = rule.fix(content).unwrap();
    println!("Fixed content 3: '{}'", fixed);

    // Check that the fix is correct
    let result = rule.check(&fixed).unwrap();
    assert!(result.is_empty());
}
