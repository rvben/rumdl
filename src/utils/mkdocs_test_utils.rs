/// Shared test utilities for MkDocs pattern testing
///
/// This module provides common test helpers to reduce duplication across
/// MkDocs feature test modules.
/// A test case for pattern detection
#[cfg(test)]
#[derive(Debug)]
pub struct PatternTestCase {
    pub input: &'static str,
    pub expected: bool,
    pub description: &'static str,
}

/// A test case for position detection within content
#[cfg(test)]
#[derive(Debug)]
pub struct PositionTestCase {
    pub content: &'static str,
    pub test_positions: Vec<(&'static str, bool)>, // (substring to find, expected in context)
    pub description: &'static str,
}

/// A test case for indentation detection
#[cfg(test)]
#[derive(Debug)]
pub struct IndentTestCase {
    pub input: &'static str,
    pub expected: Option<usize>,
    pub description: &'static str,
}

/// Run a batch of pattern detection tests
#[cfg(test)]
pub fn run_pattern_tests<F>(test_fn: F, cases: &[PatternTestCase])
where
    F: Fn(&str) -> bool,
{
    for case in cases {
        assert_eq!(
            test_fn(case.input),
            case.expected,
            "Failed: {} - Input: {:?}",
            case.description,
            case.input
        );
    }
}

/// Run a batch of position detection tests
#[cfg(test)]
pub fn run_position_tests<F>(test_fn: F, cases: &[PositionTestCase])
where
    F: Fn(&str, usize) -> bool,
{
    for case in cases {
        for (substring, expected) in &case.test_positions {
            let pos = case.content.find(substring).unwrap_or_else(|| {
                panic!(
                    "Substring '{}' not found in content for test: {}",
                    substring, case.description
                )
            });
            assert_eq!(
                test_fn(case.content, pos),
                *expected,
                "Failed: {} - Position test for substring '{}' at position {}",
                case.description,
                substring,
                pos
            );
        }
    }
}

/// Run a batch of indentation detection tests
#[cfg(test)]
pub fn run_indent_tests<F>(test_fn: F, cases: &[IndentTestCase])
where
    F: Fn(&str) -> Option<usize>,
{
    for case in cases {
        assert_eq!(
            test_fn(case.input),
            case.expected,
            "Failed: {} - Input: {:?}",
            case.description,
            case.input
        );
    }
}

/// Helper to create a document with various MkDocs features for integration testing
#[cfg(test)]
pub fn create_mkdocs_test_document() -> String {
    r#"# Test Document

Regular paragraph text.

!!! note "Test Note"
    This is an admonition with content.

    Multiple lines of content.

[^1]: This is a footnote definition
    with multiple lines
    of content.

=== "Tab 1"

    Content in tab 1.

    More content.

=== "Tab 2"

    Content in tab 2.

::: mymodule.MyClass
    handler: python
    options:
      show_source: true

--8<-- "included.md"

Regular text with [^1] footnote reference.

<!-- --8<-- [start:section] -->
Section content
<!-- --8<-- [end:section] -->

Final paragraph."#
        .to_string()
}

/// Helper to assert multiple positions in content
#[cfg(test)]
pub fn assert_positions<F>(content: &str, test_fn: F, positions: &[(&str, bool)])
where
    F: Fn(&str, usize) -> bool,
{
    for (substring, expected) in positions {
        if let Some(pos) = content.find(substring) {
            let actual = test_fn(content, pos);
            assert_eq!(
                actual, *expected,
                "Position test failed for '{substring}' at position {pos}. Expected: {expected}, Got: {actual}"
            );
        } else {
            panic!("Substring '{substring}' not found in content");
        }
    }
}

/// Helper to generate test content with specific patterns
#[cfg(test)]
pub struct TestContentBuilder {
    lines: Vec<String>,
}

#[cfg(test)]
impl TestContentBuilder {
    pub fn new() -> Self {
        Self { lines: Vec::new() }
    }

    pub fn add_line(mut self, line: &str) -> Self {
        self.lines.push(line.to_string());
        self
    }

    pub fn add_empty_line(mut self) -> Self {
        self.lines.push(String::new());
        self
    }

    pub fn add_indented(mut self, indent: usize, content: &str) -> Self {
        self.lines.push(format!("{}{}", " ".repeat(indent), content));
        self
    }

    pub fn add_admonition(mut self, admon_type: &str, title: Option<&str>) -> Self {
        let line = if let Some(t) = title {
            format!("!!! {admon_type} \"{t}\"")
        } else {
            format!("!!! {admon_type}")
        };
        self.lines.push(line);
        self
    }

    pub fn add_footnote_def(mut self, ref_name: &str, content: &str) -> Self {
        self.lines.push(format!("[^{ref_name}]: {content}"));
        self
    }

    pub fn add_tab(mut self, label: &str) -> Self {
        self.lines.push(format!("=== \"{label}\""));
        self
    }

    pub fn add_snippet(mut self, file: &str) -> Self {
        self.lines.push(format!("--8<-- \"{file}\""));
        self
    }

    pub fn build(self) -> String {
        self.lines.join("\n")
    }
}

#[cfg(test)]
impl Default for TestContentBuilder {
    fn default() -> Self {
        Self::new()
    }
}
