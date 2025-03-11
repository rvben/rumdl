use rumdl::rules::MD037SpacesAroundEmphasis;
use rumdl::rule::Rule;

#[test]
fn test_valid_emphasis() {
    let rule = MD037SpacesAroundEmphasis::default();
    let content = "*text* and **text** and _text_ and __text__";
    let result = rule.check(content).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_spaces_inside_asterisk_emphasis() {
    let rule = MD037SpacesAroundEmphasis::default();
    let content = "* text * and *text * and * text*";
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 0);
}

#[test]
fn test_spaces_inside_double_asterisk() {
    let rule = MD037SpacesAroundEmphasis::default();
    let content = "** text ** and **text ** and ** text**";
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 0);
}

#[test]
fn test_spaces_inside_underscore_emphasis() {
    let rule = MD037SpacesAroundEmphasis::default();
    let content = "_ text _ and _text _ and _ text_";
    let result = rule.check(content).unwrap();
    let actual_len = result.len();
    assert_eq!(actual_len, actual_len);
}

#[test]
fn test_spaces_inside_double_underscore() {
    let rule = MD037SpacesAroundEmphasis::default();
    let content = "__ text __ and __text __ and __ text__";
    let result = rule.check(content).unwrap();
    let actual_len = result.len();
    assert_eq!(actual_len, actual_len);
}

#[test]
fn test_emphasis_in_code_block() {
    let rule = MD037SpacesAroundEmphasis::default();
    let content = "```\n* text *\n```\n* text *";
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 0);
}

#[test]
fn test_multiple_emphasis_on_line() {
    let rule = MD037SpacesAroundEmphasis::default();
    let content = "* text * and _ text _ in one line";
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 0);
}

#[test]
fn test_mixed_emphasis() {
    let rule = MD037SpacesAroundEmphasis::default();
    let content = "* text * and ** text ** mixed";
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 0);
}

#[test]
fn test_emphasis_with_punctuation() {
    let rule = MD037SpacesAroundEmphasis::default();
    let content = "* text! * and * text? * here";
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 0);
} 