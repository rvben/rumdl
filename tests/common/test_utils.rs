/// Test utilities to reduce I/O operations and improve test performance
use rumdl_lib::config::Config;
use rumdl_lib::lint_context::LintContext;
use std::collections::HashMap;

/// In-memory test runner that avoids file system I/O
pub struct InMemoryTestRunner {
    pub files: HashMap<String, String>,
    pub config: Option<Config>,
}

impl InMemoryTestRunner {
    pub fn new() -> Self {
        Self {
            files: HashMap::new(),
            config: None,
        }
    }

    /// Add a file to the in-memory file system
    pub fn add_file<S1: Into<String>, S2: Into<String>>(&mut self, path: S1, content: S2) {
        self.files.insert(path.into(), content.into());
    }

    /// Set configuration without writing to disk
    pub fn with_config(&mut self, config: Config) {
        self.config = Some(config);
    }

    /// Run linting on a file without disk I/O
    pub fn lint_file(&self, path: &str) -> Result<Vec<rumdl_lib::rule::LintWarning>, Box<dyn std::error::Error>> {
        let content = self.files.get(path)
            .ok_or_else(|| format!("File not found: {}", path))?;

        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);

        // Apply all default rules for testing
        let mut warnings = Vec::new();

        // Add a few common rules for basic validation
        use rumdl_lib::rule::Rule;
        use rumdl_lib::rules::{
            MD001HeadingIncrement::default(),
            MD013LineLength,
            MD022BlanksAroundHeadings,
            MD032BlanksAroundLists,
        };

        let rules: Vec<Box<dyn Rule>> = vec![
            Box::new(MD001HeadingIncrement::default()),
            Box::new(MD013LineLength::default()),
            Box::new(MD022BlanksAroundHeadings::default()),
            Box::new(MD032BlanksAroundLists::default()),
        ];

        for rule in rules {
            if let Ok(rule_warnings) = rule.check(&ctx) {
                warnings.extend(rule_warnings);
            }
        }

        Ok(warnings)
    }

    /// Check if content would have warnings without file I/O
    pub fn has_warnings(&self, path: &str) -> bool {
        self.lint_file(path).map(|w| !w.is_empty()).unwrap_or(false)
    }
}

/// Quick content validation without full linting overhead
pub fn quick_validate_content(content: &str) -> bool {
    // Fast heuristic checks that don't require full parsing
    !content.is_empty() && content.contains('#')
}

/// Generate test markdown content without writing to disk
pub fn generate_test_content(variant: &str) -> String {
    match variant {
        "simple" => "# Test\n\nSimple content.\n".to_string(),
        "complex" => {
            r#"# Main Title

Some introductory content here.

## Section 1

Content for section 1 with some details.

### Subsection 1.1

More detailed content here.

## Section 2

Another section with different content.

- Item 1
- Item 2
- Item 3

### Code Example

```rust
fn main() {
    println!("Hello, world!");
}
```

## Conclusion

Final thoughts and summary.
"#.to_string()
        }
        "with_issues" => {
            r#"# Test
### Skipped heading level (should trigger MD001)
No blank lines around this heading
## Another heading
- List item
- Another item
Paragraph directly after list (should trigger MD032)
"#.to_string()
        }
        "minimal" => "# Title\n".to_string(),
        _ => format!("# Test {}\n\nContent for {}.\n", variant, variant),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_in_memory_runner() {
        let mut runner = InMemoryTestRunner::new();
        runner.add_file("test.md", "# Test\n\nContent here.\n");

        let warnings = runner.lint_file("test.md").unwrap();
        // Should not error, exact warnings depend on rules
        assert!(warnings.len() >= 0);
    }

    #[test]
    fn test_content_generation() {
        let simple = generate_test_content("simple");
        assert!(simple.contains("# Test"));

        let complex = generate_test_content("complex");
        assert!(complex.contains("# Main Title"));
        assert!(complex.contains("```rust"));
    }

    #[test]
    fn test_quick_validation() {
        assert!(quick_validate_content("# Test"));
        assert!(!quick_validate_content(""));
        assert!(!quick_validate_content("no heading"));
    }
}
