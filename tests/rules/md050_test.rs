use rumdl::rules::MD050StrongStyle;
use rumdl::rules::md050_strong_style::StrongStyle;
use rumdl::rule::Rule;

#[test]
fn test_consistent_asterisks() {
    let rule = MD050StrongStyle::new(StrongStyle::Asterisk);
    let content = "# Test\n\nThis is **strong** and this is also **strong**";
    let result = rule.check(content).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_consistent_underscores() {
    let rule = MD050StrongStyle::new(StrongStyle::Underscore);
    let content = "# Test\n\nThis is __strong__ and this is also __strong__";
    let result = rule.check(content).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_mixed_strong_prefer_asterisks() {
    let rule = MD050StrongStyle::new(StrongStyle::Asterisk);
    let content = "# Mixed strong\n\nThis is **asterisk** and this is __underscore__";
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 1);
    let fixed = rule.fix(content).unwrap();
    assert_eq!(fixed, "# Mixed strong\n\nThis is **asterisk** and this is **underscore**\n");
}

#[test]
fn test_mixed_strong_prefer_underscores() {
    let rule = MD050StrongStyle::new(StrongStyle::Underscore);
    let content = "# Mixed strong\n\nThis is **asterisk** and this is __underscore__";
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 1);
    let fixed = rule.fix(content).unwrap();
    assert_eq!(fixed, "# Mixed strong\n\nThis is __asterisk__ and this is __underscore__\n");
}

#[test]
fn test_consistent_style_first_asterisk() {
    let rule = MD050StrongStyle::new(StrongStyle::Consistent);
    let content = "# Mixed strong\n\nThis is **asterisk** and this is __underscore__";
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 1);
    let fixed = rule.fix(content).unwrap();
    assert_eq!(fixed, "# Mixed strong\n\nThis is **asterisk** and this is **underscore**\n");
}

#[test]
fn test_consistent_style_first_underscore() {
    let rule = MD050StrongStyle::new(StrongStyle::Consistent);
    let content = "# Mixed strong\n\nThis is __underscore__ and this is **asterisk**";
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 1);
    let fixed = rule.fix(content).unwrap();
    assert_eq!(fixed, "# Mixed strong\n\nThis is __underscore__ and this is __asterisk__\n");
}

#[test]
fn test_empty_content() {
    let rule = MD050StrongStyle::new(StrongStyle::Consistent);
    let content = "";
    let result = rule.check(content).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_no_strong() {
    let rule = MD050StrongStyle::new(StrongStyle::Consistent);
    let content = "# Just a heading\n\nSome regular text\n\n> A blockquote";
    let result = rule.check(content).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_ignore_emphasis() {
    let rule = MD050StrongStyle::new(StrongStyle::Asterisk);
    let content = "# Test\n\nThis is *emphasis* and this is **strong**";
    let result = rule.check(content).unwrap();
    assert!(result.is_empty());
} 