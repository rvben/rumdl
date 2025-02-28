use rumdl::rules::MD049EmphasisStyle;
use rumdl::rules::md049_emphasis_style::EmphasisStyle;
use rumdl::rule::Rule;

#[test]
fn test_consistent_asterisks() {
    let rule = MD049EmphasisStyle::new(EmphasisStyle::Asterisk);
    let content = "# Test\n\nThis is *emphasized* and this is also *emphasized*";
    let result = rule.check(content).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_consistent_underscores() {
    let rule = MD049EmphasisStyle::new(EmphasisStyle::Underscore);
    let content = "# Test\n\nThis is _emphasized_ and this is also _emphasized_";
    let result = rule.check(content).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_mixed_emphasis_prefer_asterisks() {
    let rule = MD049EmphasisStyle::new(EmphasisStyle::Asterisk);
    let content = "# Mixed emphasis\n\nThis is *asterisk* and this is _underscore_";
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 1);
    let fixed = rule.fix(content).unwrap();
    assert_eq!(fixed, "# Mixed emphasis\n\nThis is *asterisk* and this is *underscore*\n");
}

#[test]
fn test_mixed_emphasis_prefer_underscores() {
    let rule = MD049EmphasisStyle::new(EmphasisStyle::Underscore);
    let content = "# Mixed emphasis\n\nThis is *asterisk* and this is _underscore_";
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 1);
    let fixed = rule.fix(content).unwrap();
    assert_eq!(fixed, "# Mixed emphasis\n\nThis is _asterisk_ and this is _underscore_\n");
}

#[test]
fn test_consistent_style_first_asterisk() {
    let rule = MD049EmphasisStyle::new(EmphasisStyle::Consistent);
    let content = "# Mixed emphasis\n\nThis is *asterisk* and this is _underscore_";
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 1);
    let fixed = rule.fix(content).unwrap();
    assert_eq!(fixed, "# Mixed emphasis\n\nThis is *asterisk* and this is *underscore*\n");
}

#[test]
fn test_consistent_style_first_underscore() {
    let rule = MD049EmphasisStyle::new(EmphasisStyle::Consistent);
    let content = "# Mixed emphasis\n\nThis is _underscore_ and this is *asterisk*";
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 1);
    let fixed = rule.fix(content).unwrap();
    assert_eq!(fixed, "# Mixed emphasis\n\nThis is _underscore_ and this is _asterisk_\n");
}

#[test]
fn test_empty_content() {
    let rule = MD049EmphasisStyle::new(EmphasisStyle::Consistent);
    let content = "";
    let result = rule.check(content).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_no_emphasis() {
    let rule = MD049EmphasisStyle::new(EmphasisStyle::Consistent);
    let content = "# Just a heading\n\nSome regular text\n\n> A blockquote";
    let result = rule.check(content).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_ignore_strong_emphasis() {
    let rule = MD049EmphasisStyle::new(EmphasisStyle::Asterisk);
    let content = "# Test\n\nThis is *emphasis* and this is **strong**";
    let result = rule.check(content).unwrap();
    assert!(result.is_empty());
} 