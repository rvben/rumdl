use rumdl_lib::lint_context::LintContext;
use rumdl_lib::rule::Rule;
use rumdl_lib::rules::MD012NoMultipleBlanks;

#[test]
fn test_md012_valid() {
    let rule = MD012NoMultipleBlanks::default();
    let content = "Line 1\n\nLine 2\n\nLine 3\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_md012_invalid() {
    let rule = MD012NoMultipleBlanks::default();
    let content = "Line 1\n\n\nLine 2\n\n\n\nLine 3\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 3);
    assert_eq!(result[0].line, 3);
    assert_eq!(result[0].message, "Multiple consecutive blank lines between content");
    assert_eq!(result[1].line, 6);
    assert_eq!(result[1].message, "Multiple consecutive blank lines between content");
    assert_eq!(result[2].line, 7);
    assert_eq!(result[2].message, "Multiple consecutive blank lines between content");
}

#[test]
fn test_md012_start_end() {
    let rule = MD012NoMultipleBlanks::default();
    let content = "\n\nLine 1\nLine 2\n\n\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 2);
    assert_eq!(result[0].line, 2);
    assert_eq!(result[0].message, "Multiple consecutive blank lines at start of file");
    assert_eq!(result[1].line, 6);
    assert_eq!(result[1].message, "Multiple consecutive blank lines at end of file");
}

#[test]
fn test_md012_code_blocks() {
    let rule = MD012NoMultipleBlanks::default();
    let content = "Line 1\n\n```\n\n\nCode\n\n\n```\nLine 2\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty()); // Multiple blank lines in code blocks are allowed
}

#[test]
fn test_md012_front_matter() {
    let rule = MD012NoMultipleBlanks::default();
    let content = "---\ntitle: Test\n\n\ndescription: Test\n---\n\nContent\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty()); // Multiple blank lines in front matter are allowed
}

#[test]
fn test_md012_custom_maximum() {
    let rule = MD012NoMultipleBlanks::new(2);
    let content = "Line 1\n\n\nLine 2\n\n\n\nLine 3\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    // Only the second group (3 blanks > maximum 2) is invalid, reporting 1 excess line
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].line, 7); // The third blank line in the second sequence
    assert_eq!(result[0].message, "Multiple consecutive blank lines between content");
}

#[test]
fn test_md012_fix() {
    let rule = MD012NoMultipleBlanks::default();
    let content = "Line 1\n\n\nLine 2\n\n\n\nLine 3\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.fix(&ctx).unwrap();
    assert_eq!(result, "Line 1\n\nLine 2\n\nLine 3\n");
}

#[test]
fn test_md012_fix_with_code_blocks() {
    let rule = MD012NoMultipleBlanks::default();
    let content = "Line 1\n\n\n```\n\n\nCode\n\n\n```\nLine 2\n\n\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.fix(&ctx).unwrap();
    // Trailing blanks at EOF are removed (matching markdownlint-cli)
    assert_eq!(result, "Line 1\n\n```\n\n\nCode\n\n\n```\nLine 2\n");
}

#[test]
fn test_md012_fix_with_front_matter() {
    let rule = MD012NoMultipleBlanks::default();
    let content = "---\ntitle: Test\n\n\ndescription: Test\n---\n\n\n\nContent\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.fix(&ctx).unwrap();
    assert_eq!(result, "---\ntitle: Test\n\n\ndescription: Test\n---\n\nContent\n");
}

#[test]
fn test_md012_whitespace_lines() {
    let rule = MD012NoMultipleBlanks::default();
    let content = "Line 1\n  \n \t \nLine 2\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    // Expects 1 warning for the excess whitespace line
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].line, 3); // The second whitespace line is excess
    assert_eq!(result[0].message, "Multiple consecutive blank lines between content");
}

#[test]
fn test_md012_indented_code_blocks() {
    let rule = MD012NoMultipleBlanks::default();
    let content = "Line 1\n\n    code block\n\n    more code\n\nLine 2\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty()); // Multiple blank lines in indented code blocks are allowed
}

#[test]
fn test_md012_indented_fenced_code_blocks() {
    let rule = MD012NoMultipleBlanks::default();
    let content = "Text\n\n    ```bash\n    code\n    ```\n\nMore text\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty()); // Should not flag blank lines around indented fenced code blocks
}

#[test]
fn test_md012_debug_indented_fenced() {
    let content = "Text\n\n    ```bash\n    code\n    ```\n\nMore text\n";
    let lines: Vec<&str> = content.lines().collect();

    // Debug the regions
    println!("Lines:");
    for (i, line) in lines.iter().enumerate() {
        println!("  {i}: {line:?}");
    }

    // Test the rule
    let rule = MD012NoMultipleBlanks::default();
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    println!("Warnings: {result:?}");
    for warning in &result {
        println!("  Line {}: {}", warning.line, warning.message);
    }

    // This should pass but currently fails
    assert!(result.is_empty(), "Expected no warnings, got: {result:?}");
}

#[test]
fn test_md012_contributing_pattern() {
    // This reproduces the exact pattern from the CONTRIBUTING file that's causing false positives
    let content = "To set up the MLflow repository, run the following commands:\n\n    ```bash\n    # Clone the repository\n    git clone --recurse-submodules git@github.com:<username>/mlflow.git\n    # The alternative way of cloning through https may cause permission error during branch push\n";

    let rule = MD012NoMultipleBlanks::default();
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    println!("Content lines:");
    for (i, line) in content.lines().enumerate() {
        println!("  {i}: {line:?}");
    }

    println!("Warnings: {result:?}");
    for warning in &result {
        println!("  Line {}: {}", warning.line, warning.message);
    }

    // This should pass - there's only 1 blank line before the indented fenced code block
    assert!(result.is_empty(), "Expected no warnings, got: {result:?}");
}

#[test]
fn test_md012_region_calculation() {
    // Test with a simple fenced code block to debug region calculation
    let content = "Text\n\n```bash\ncode\n```\n\nMore text\n";
    let lines: Vec<&str> = content.lines().collect();

    println!("Lines:");
    for (i, line) in lines.iter().enumerate() {
        println!("  {i}: {line:?}");
    }

    // We need to access the compute_code_block_regions function somehow
    // For now, let's just test the rule behavior
    let rule = MD012NoMultipleBlanks::default();
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    println!("Warnings: {result:?}");
    for warning in &result {
        println!("  Line {}: {}", warning.line, warning.message);
    }

    // This should pass - there's only 1 blank line before and after the code block
    assert!(result.is_empty(), "Expected no warnings, got: {result:?}");
}
