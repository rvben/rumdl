use rumdl_lib::config::{Config, MarkdownFlavor};
use rumdl_lib::rules;

#[test]
fn test_md032_multiple_backslash_continuations() {
    // Test multiple consecutive backslash continuations
    let content = r#"# Header

1. First\
   Second\
   Third\
   Fourth

2. Next item

Text"#;

    let config = Config::default();
    let all_rules = rules::all_rules(&config);
    let md032_rules: Vec<_> = all_rules.into_iter().filter(|r| r.name() == "MD032").collect();

    let warnings = rumdl_lib::lint(content, &md032_rules, false, MarkdownFlavor::Standard).unwrap();

    assert_eq!(
        warnings.len(),
        0,
        "MD032 should handle multiple backslash continuations. Found warnings: {:?}",
        warnings
            .iter()
            .map(|w| format!("Line {}: {}", w.line, w.message))
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_md032_indented_code_block_in_list() {
    // Test indented code block as part of list item
    let content = r#"# Header

1. Item with code

       indented code block
       more code

2. Next item

Text"#;

    let config = Config::default();
    let all_rules = rules::all_rules(&config);
    let md032_rules: Vec<_> = all_rules.into_iter().filter(|r| r.name() == "MD032").collect();

    let warnings = rumdl_lib::lint(content, &md032_rules, false, MarkdownFlavor::Standard).unwrap();

    assert_eq!(
        warnings.len(),
        0,
        "MD032 should handle indented code blocks in list items. Found warnings: {:?}",
        warnings
            .iter()
            .map(|w| format!("Line {}: {}", w.line, w.message))
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_md032_fenced_code_block_in_list() {
    // Test fenced code block properly indented in list
    let content = r#"# Header

1. Item with code

   ```bash
   echo "test"
   ```

2. Next item

Text"#;

    let config = Config::default();
    let all_rules = rules::all_rules(&config);
    let md032_rules: Vec<_> = all_rules.into_iter().filter(|r| r.name() == "MD032").collect();

    let warnings = rumdl_lib::lint(content, &md032_rules, false, MarkdownFlavor::Standard).unwrap();

    assert_eq!(
        warnings.len(),
        0,
        "MD032 should handle fenced code blocks in list items when properly indented. Found warnings: {:?}",
        warnings
            .iter()
            .map(|w| format!("Line {}: {}", w.line, w.message))
            .collect::<Vec<_>>()
    );
}