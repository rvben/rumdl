use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::utils::document_structure::{DocumentStructure, DocumentStructureExtensions};
use crate::utils::range_utils::LineIndex;

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
        let mut warnings = Vec::new();

        // Use centralized link parsing from LintContext
        for link in &ctx.links {
            // For reference links, resolve the URL
            let effective_url = if link.is_reference {
                if let Some(ref_id) = &link.reference_id {
                    ctx.get_reference_url(ref_id).unwrap_or("").to_string()
                } else {
                    String::new()
                }
            } else {
                link.url.clone()
            };
            
            // Check for empty links
            if link.text.trim().is_empty() || effective_url.trim().is_empty() {
                let replacement = if link.text.trim().is_empty() && effective_url.trim().is_empty() {
                    "[Link text](https://example.com)".to_string()
                } else if link.text.trim().is_empty() {
                    if link.is_reference {
                        format!("[Link text]{}", &ctx.content[link.byte_offset + 1..link.byte_end])
                    } else {
                        format!("[Link text]({})", effective_url)
                    }
                } else {
                    if link.is_reference {
                        // Keep the reference format
                        let ref_part = &ctx.content[link.byte_offset + link.text.len() + 2..link.byte_end];
                        format!("[{}]{}", link.text, ref_part)
                    } else {
                        format!("[{}](https://example.com)", link.text)
                    }
                };

                warnings.push(LintWarning {
                    rule_name: Some(self.name()),
                    message: format!("Empty link found: [{}]({})", link.text, effective_url),
                    line: link.line,
                    column: link.start_col + 1, // Convert to 1-indexed
                    end_line: link.line,
                    end_column: link.end_col + 1, // Convert to 1-indexed
                    severity: Severity::Warning,
                    fix: Some(Fix {
                        range: link.byte_offset..link.byte_end,
                        replacement,
                    }),
                });
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
            let replacement = if link.text.trim().is_empty() && link.url.trim().is_empty() {
                "[Link text](https://example.com)".to_string()
            } else if link.text.trim().is_empty() {
                format!("[Link text]({})", link.url)
            } else {
                format!("[{}](https://example.com)", link.text)
            };

            warnings.push(LintWarning {
                rule_name: Some(self.name()),
                message: format!("Empty link found: [{}]({})", link.text, link.url),
                line: link.line,
                column: link.start_col,
                end_line: link.line,
                end_column: link.end_col + 1,
                severity: Severity::Warning,
                fix: Some(Fix {
                    range: line_index.line_col_to_byte_range_with_length(
                        link.line, 
                        link.start_col, 
                        (link.end_col + 1).saturating_sub(link.start_col)
                    ),
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

        // Collect all fixes with their ranges
        let mut fixes: Vec<(std::ops::Range<usize>, String)> = warnings
            .iter()
            .filter_map(|w| w.fix.as_ref().map(|f| (f.range.clone(), f.replacement.clone())))
            .collect();

        // Sort fixes by position (descending) to apply from end to start
        fixes.sort_by(|a, b| b.0.start.cmp(&a.0.start));

        let mut result = content.to_string();
        
        // Apply fixes from end to start to maintain correct positions
        for (range, replacement) in fixes {
            result.replace_range(range, &replacement);
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
