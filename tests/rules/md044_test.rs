use rumdl::rules::MD044ProperNames;
use rumdl::rule::Rule;

#[test]
fn test_correct_names() {
    let names = vec!["JavaScript".to_string(), "TypeScript".to_string()];
    let rule = MD044ProperNames::new(names, true);
    let content = "# Guide to JavaScript and TypeScript\n\nJavaScript is awesome!";
    let result = rule.check(content).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_incorrect_names() {
    let names = vec!["JavaScript".to_string(), "TypeScript".to_string()];
    let rule = MD044ProperNames::new(names, true);
    let content = "# Guide to javascript and typescript\n\njavascript is awesome!";
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 3);
    let fixed = rule.fix(content).unwrap();
    assert_eq!(fixed, "# Guide to JavaScript and TypeScript\n\nJavaScript is awesome!");
}

#[test]
fn test_code_block_excluded() {
    let names = vec!["JavaScript".to_string()];
    let rule = MD044ProperNames::new(names, true);
    let content = "# JavaScript Guide\n\n```javascript\nconst x = 'javascript';\n```";
    let result = rule.check(content).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_code_block_included() {
    let names = vec!["JavaScript".to_string()];
    let rule = MD044ProperNames::new(names, false);
    let content = "# JavaScript Guide\n\n```javascript\nconst x = 'javascript';\n```";
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 1);
    let fixed = rule.fix(content).unwrap();
    assert_eq!(fixed, "# JavaScript Guide\n\n```JavaScript\nconst x = 'JavaScript';\n```");
}

#[test]
fn test_indented_code_block() {
    let names = vec!["JavaScript".to_string()];
    let rule = MD044ProperNames::new(names, true);
    let content = "# JavaScript Guide\n\n    const x = 'javascript';\n    console.log(x);";
    let result = rule.check(content).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_multiple_occurrences() {
    let names = vec!["JavaScript".to_string(), "Node.js".to_string()];
    let rule = MD044ProperNames::new(names, true);
    let content = "javascript with nodejs\njavascript and nodejs again";
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 4);
    let fixed = rule.fix(content).unwrap();
    assert_eq!(fixed, "JavaScript with Node.js\nJavaScript and Node.js again");
}

#[test]
fn test_word_boundaries() {
    let names = vec!["Git".to_string()];
    let rule = MD044ProperNames::new(names, true);
    let content = "Using git and github with gitflow";
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 1); // Only "git" should be flagged, not "github" or "gitflow"
    let fixed = rule.fix(content).unwrap();
    assert_eq!(fixed, "Using Git and github with gitflow");
} 