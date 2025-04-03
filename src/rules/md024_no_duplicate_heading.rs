use crate::utils::range_utils::LineIndex;

use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, Severity, RuleCategory};
use crate::rules::heading_utils::HeadingUtils;
use crate::utils::document_structure::{DocumentStructure, DocumentStructureExtensions};
use std::collections::HashSet;

#[derive(Debug)]
pub struct MD024NoDuplicateHeading {
    pub allow_different_nesting: bool,
    pub siblings_only: bool,
}

impl Default for MD024NoDuplicateHeading {
    fn default() -> Self {
        Self {
            allow_different_nesting: false,
            siblings_only: false,
        }
    }
}

impl MD024NoDuplicateHeading {
    pub fn new(allow_different_nesting: bool, siblings_only: bool) -> Self {
        Self {
            allow_different_nesting,
            siblings_only,
        }
    }
}

impl Rule for MD024NoDuplicateHeading {
    fn name(&self) -> &'static str {
        "MD024"
    }

    fn description(&self) -> &'static str {
        "Multiple headings with the same content"
    }

    fn check(&self, content: &str) -> LintResult {
        let line_index = LineIndex::new(content.to_string());

        let mut warnings = Vec::new();

        let mut seen_headings = HashSet::new();

        let mut current_level = 0;

        let mut current_siblings = HashSet::new();

        let mut level_siblings = Vec::new();

        for (line_num, line) in content.lines().enumerate() {
            if let Some(heading) = HeadingUtils::parse_heading(line, line_num + 1) {
                let indentation = HeadingUtils::get_indentation(line);
                let text = heading.text.to_lowercase();

                if self.siblings_only {
                    if heading.level > current_level {
                        level_siblings.push(current_siblings.clone());
                        current_siblings = HashSet::new();
                    } else if heading.level < current_level {
                        while level_siblings.len() > heading.level {
                            level_siblings.pop();
                        }
                        if let Some(siblings) = level_siblings.last() {
                            current_siblings = siblings.clone();
                        }
                    }

                    if current_siblings.contains(&text) {
                        warnings.push(LintWarning {
                            rule_name: Some(self.name()),
                            line: line_num + 1,
                            column: indentation + 1,
                            message: "Multiple headings with the same content at the same nesting level".to_string(),
                            severity: Severity::Warning,
                            fix: Some(Fix {
                                range: line_index.line_col_to_byte_range(line_num + 1, indentation + 1),
                                replacement: format!("{}{} {} ({})",
                                    " ".repeat(indentation),
                                    "#".repeat(heading.level),
                                    heading.text,
                                    current_siblings.len() + 1),
                            }),
                        });
                    }
                    current_siblings.insert(text);
                    current_level = heading.level;
                } else if !self.allow_different_nesting || heading.level == current_level {
                    if seen_headings.contains(&text) {
                        warnings.push(LintWarning {
                            rule_name: Some(self.name()),
                            line: line_num + 1,
                            column: indentation + 1,
                            message: "Multiple headings with the same content".to_string(),
                            severity: Severity::Warning,
                            fix: Some(Fix {
                                range: line_index.line_col_to_byte_range(line_num + 1, indentation + 1),
                                replacement: format!("{}{} {} ({})",
                                    " ".repeat(indentation),
                                    "#".repeat(heading.level),
                                    heading.text,
                                    seen_headings.iter().filter(|&h| h == &text).count() + 1),
                            }),
                        });
                    }
                    seen_headings.insert(text);
                }
            }
        }

        Ok(warnings)
    }

    fn fix(&self, content: &str) -> Result<String, LintError> {
        let line_index = LineIndex::new(content.to_string());

        let mut result = String::new();

        let mut seen_headings = HashSet::new();

        let mut current_level = 0;

        let mut current_siblings = HashSet::new();

        let mut level_siblings = Vec::new();

        for line in content.lines() {
            if let Some(heading) = HeadingUtils::parse_heading(line, 0) {
                let indentation = HeadingUtils::get_indentation(line);
                let text = heading.text.to_lowercase();

                if self.siblings_only {
                    if heading.level > current_level {
                        level_siblings.push(current_siblings.clone());
                        current_siblings = HashSet::new();
                    } else if heading.level < current_level {
                        while level_siblings.len() > heading.level {
                            level_siblings.pop();
                        }
                        if let Some(siblings) = level_siblings.last() {
                            current_siblings = siblings.clone();
                        }
                    }

                    if current_siblings.contains(&text) {
                        result.push_str(&format!("{}{} {} ({})\n",
                            " ".repeat(indentation),
                            "#".repeat(heading.level),
                            heading.text,
                            current_siblings.len() + 1));
                    } else {
                        result.push_str(line);
                        result.push('\n');
                    }
                    current_siblings.insert(text);
                    current_level = heading.level;
                } else if !self.allow_different_nesting || heading.level == current_level {
                    if seen_headings.contains(&text) {
                        result.push_str(&format!("{}{} {} ({})\n",
                            " ".repeat(indentation),
                            "#".repeat(heading.level),
                            heading.text,
                            seen_headings.iter().filter(|&h| h == &text).count() + 1));
                    } else {
                        result.push_str(line);
                        result.push('\n');
                    }
                    seen_headings.insert(text);
                } else {
                    result.push_str(line);
                    result.push('\n');
                }
            } else {
                result.push_str(line);
                result.push('\n');
            }
        }

        if !content.ends_with('\n') {
            result.pop();
        }

        Ok(result)
    }