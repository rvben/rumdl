use rumdl::rules::MD027MultipleSpacesBlockquote;
use rumdl::rule::Rule;

#[test]
fn test_md027_valid() {
    let rule = MD027MultipleSpacesBlockquote::default();
    let content = "> Quote\n> Another line\n> Third line\n";
    let result = rule.check(content).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_md027_invalid() {
    let rule = MD027MultipleSpacesBlockquote::default();
    let content = ">  Quote\n>   Another line\n>    Third line\n";
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 3);
    assert_eq!(result[0].line, 1);
    assert_eq!(result[1].line, 2);
    assert_eq!(result[2].line, 3);
}

#[test]
fn test_md027_mixed() {
    let rule = MD027MultipleSpacesBlockquote::default();
    let content = "> Quote\n>  Another line\n> Third line\n";
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].line, 2);
}

#[test]
fn test_md027_fix() {
    let rule = MD027MultipleSpacesBlockquote::default();
    let content = ">  Quote\n>   Another line\n>    Third line\n";
    let result = rule.fix(content).unwrap();
    assert_eq!(result, "> Quote\n> Another line\n> Third line\n");
} 