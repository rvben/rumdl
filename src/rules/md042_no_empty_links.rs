use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::utils::document_structure::{DocumentStructure, DocumentStructureExtensions};
use crate::utils::range_utils::LineIndex;
use lazy_static::lazy_static;
use regex::Regex;

/// Rule MD042: No empty links
///
/// See [docs/md042.md](../../docs/md042.md) for full documentation, configuration, and examples.
///
/// This rule is triggered when a link has no content (text) or destination (URL).
#[derive(Clone)]
pub struct MD042NoEmptyLinks;

impl Default for MD042NoEmptyLinks {
    fn default() -> Self {
        Self::new()
    }
}

impl MD042NoEmptyLinks {
    pub fn new() -> Self {
        Self
    }
}

impl Rule for MD042NoEmptyLinks {
    fn name(&self) -> &'static str {
        "MD042"
    }

    fn description(&self) -> &'static str {
        "No empty links"
    }

    fn check(&self, ctx: &crate::lint_context::LintContext) -> LintResult {
        let content = ctx.content;
        let line_index = LineIndex::new(content.to_string());
        let mut warnings = Vec::new();

        lazy_static! {
            static ref EMPTY_LINK_REGEX: Regex = Regex::new(r"\[([^\]]*)\]\(([^\)]*)\)").unwrap();
        }

        for (line_num, line) in content.lines().enumerate() {
            for cap in EMPTY_LINK_REGEX.captures_iter(line) {
                let text = cap.get(1).map_or("", |m| m.as_str());
                let url = cap.get(2).map_or("", |m| m.as_str());

                if text.trim().is_empty() || url.trim().is_empty() {
                    let full_match = cap.get(0).unwrap();
                    warnings.push(LintWarning {
                        rule_name: Some(self.name()),
                        message: format!("Empty link found: [{}]({})", text, url),
                        line: line_num + 1,
                        column: full_match.start() + 1,
                        severity: Severity::Warning,
                        fix: Some(Fix {
                            range: line_index
                                .line_col_to_byte_range(line_num + 1, full_match.start() + 1),
                            replacement: String::new(), // Remove empty link
                        }),
                    });
                }
            }
        }

        Ok(warnings)
    }

    /// Optimized check using document structure
    fn check_with_structure(
        &self,
        _ctx: &crate::lint_context::LintContext,
        structure: &DocumentStructure,
    ) -> LintResult {
        let content = _ctx.content;
        // Early return if there are no links
        if structure.links.is_empty() {
            return Ok(Vec::new());
        }

        let line_index = LineIndex::new(content.to_string());
        let mut warnings = Vec::new();

        // Get pre-computed empty links
        let empty_links = structure.get_empty_links();

        for link in empty_links {
            warnings.push(LintWarning {
                rule_name: Some(self.name()),
                message: format!("Empty link found: [{}]({})", link.text, link.url),
                line: link.line,
                column: link.start_col,
                severity: Severity::Warning,
                fix: Some(Fix {
                    range: line_index.line_col_to_byte_range(link.line, link.start_col),
                    replacement: String::new(), // Remove empty link
                }),
            });
        }

        Ok(warnings)
    }

    fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, LintError> {
        let content = ctx.content;
        let _line_index = LineIndex::new(content.to_string());

        lazy_static! {
            static ref EMPTY_LINK_REGEX: Regex = Regex::new(r"\[([^\]]*)\]\(([^\)]*)\)").unwrap();
        }

        let result = EMPTY_LINK_REGEX.replace_all(content, |caps: &regex::Captures| {
            let text = caps.get(1).map_or("", |m| m.as_str());
            let url = caps.get(2).map_or("", |m| m.as_str());

            if text.trim().is_empty() || url.trim().is_empty() {
                String::new()
            } else {
                caps[0].to_string()
            }
        });

        Ok(result.to_string())
    }

    /// Get the category of this rule for selective processing
    fn category(&self) -> RuleCategory {
        RuleCategory::Link
    }

    /// Check if this rule should be skipped
    fn should_skip(&self, ctx: &crate::lint_context::LintContext) -> bool {
        let content = ctx.content;
        content.is_empty() || !content.contains('[')
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn from_config(_config: &crate::config::Config) -> Box<dyn Rule>
    where
        Self: Sized,
    {
        Box::new(MD042NoEmptyLinks)
    }
}

impl DocumentStructureExtensions for MD042NoEmptyLinks {
    fn has_relevant_elements(
        &self,
        _ctx: &crate::lint_context::LintContext,
        doc_structure: &DocumentStructure,
    ) -> bool {
        !doc_structure.links.is_empty()
    }
}
