use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::utils::document_structure::{DocumentStructure, DocumentStructureExtensions};
use crate::utils::range_utils::LineIndex;
use fancy_regex::Regex;
use lazy_static::lazy_static;

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

        // Early return for empty content or content without links
        if content.is_empty() || !content.contains('[') {
            return Ok(Vec::new());
        }

        // Use document structure for proper code block and code span detection
        let structure = DocumentStructure::new(content);
        self.check_with_structure(ctx, &structure)
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
            let replacement = if link.text.trim().is_empty() && link.url.trim().is_empty() {
                "[Link text](https://example.com)".to_string()
            } else if link.text.trim().is_empty() {
                format!("[Link text]({})", link.url)
            } else {
                format!("[{}](https://example.com)", link.text)
            };

            warnings.push(LintWarning {
                rule_name: Some(self.name()),
                message: format!(
                    "Empty link found: [{
            }]({})",
                    link.text, link.url
                ),
                line: link.line,
                column: link.start_col,
                end_line: link.line,
                end_column: link.end_col + 1,
                severity: Severity::Warning,
                fix: Some(Fix {
                    range: line_index.line_col_to_byte_range(link.line, link.start_col),
                    replacement,
                }),
            });
        }

        Ok(warnings)
    }

    fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, LintError> {
        let content = ctx.content;

        // Get all warnings first - only fix links that are actually flagged
        let warnings = self.check(ctx)?;
        if warnings.is_empty() {
            return Ok(content.to_string());
        }

        let mut result = content.to_string();

        lazy_static! {
            static ref EMPTY_LINK_REGEX: Regex =
                Regex::new(r"(?<!\!)\[([^\]]*)\]\(([^\)]*)\)").unwrap();
        }

        // Apply fixes by replacing each empty link with the appropriate replacement
        for warning in &warnings {
            if let Some(fix) = &warning.fix {
                // Find the specific link that matches this warning
                for cap_result in EMPTY_LINK_REGEX.captures_iter(&result) {
                    let cap = match cap_result {
                        Ok(cap) => cap,
                        Err(_) => continue,
                    };

                    let full_match = cap.get(0).unwrap();
                    let text = cap.get(1).map_or("", |m| m.as_str());
                    let url = cap.get(2).map_or("", |m| m.as_str());

                    // Check if this is an empty link that needs fixing
                    if text.trim().is_empty() || url.trim().is_empty() {
                        result = result.replacen(full_match.as_str(), &fix.replacement, 1);
                        break; // Only replace one at a time to avoid issues
                    }
                }
            }
        }

        Ok(result)
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
