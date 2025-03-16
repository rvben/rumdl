use rumdl::rule::Rule;
use rumdl::rules::MD041FirstLineHeading;

#[test]
fn test_valid_first_line_heading() {
    let rule = MD041FirstLineHeading::new(1, false);
    let content = "# First heading\nSome text";
    let result = rule.check(content).unwrap();
    println!("Valid test result: {:?}", result);
    assert!(result.is_empty());
}

#[test]
fn test_missing_first_line_heading() {
    let rule = MD041FirstLineHeading::new(1, false);
    let content = "Some text\n# Not first heading";
    let result = rule.check(content).unwrap();
    assert!(!result.is_empty());
}

#[test]
fn test_wrong_level_heading() {
    let rule = MD041FirstLineHeading::new(1, false);
    let content = "## Second level heading\nSome text";
    let result = rule.check(content).unwrap();
    assert!(!result.is_empty());
}

#[test]
fn test_with_front_matter() {
    let rule = MD041FirstLineHeading::new(1, true);
    let content = "---\ntitle: Test\n---\n# First heading\nSome text";
    let result = rule.check(content).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_with_front_matter_no_heading() {
    let rule = MD041FirstLineHeading::new(1, false);
    let content = "---\ntitle: Test\n---\nSome text";
    let result = rule.check(content).unwrap();
    assert!(!result.is_empty());
}

#[test]
fn test_fix_missing_heading() {
    let rule = MD041FirstLineHeading::new(1, false);
    let content = "Some text\nMore text";
    let result = rule.fix(content).unwrap();
    assert!(result.starts_with("# "));
}

#[test]
fn test_custom_level() {
    let rule = MD041FirstLineHeading::new(2, false);
    let content = "## Second level heading\nSome text";
    let result = rule.check(content).unwrap();
    println!("Custom level test result: {:?}", result);
    assert!(result.is_empty());
}
