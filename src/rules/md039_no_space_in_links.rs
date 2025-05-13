use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::utils::document_structure::DocumentStructure;
use crate::utils::range_utils::LineIndex;

/// Rule MD039: No space inside link text
///
/// See [docs/md039.md](../../docs/md039.md) for full documentation, configuration, and examples.
///
/// This rule is triggered when link text has leading or trailing spaces which can cause
/// unexpected rendering in some Markdown parsers.
#[derive(Debug, Default, Clone)]
pub struct MD039NoSpaceInLinks;

// Static definition for the warning message
const WARNING_MESSAGE: &str = "Spaces inside link text should be removed";

impl MD039NoSpaceInLinks {
    pub fn new() -> Self {
        Self
    }

    /// Fast check to see if content has any potential links
    #[inline]
    fn has_links(&self, content: &str) -> bool {
        content.contains('[') && content.contains("](")
    }

    /// Check if the text has leading or trailing spaces, and return the fixed version if so
    #[inline]
    fn check_link_text<'a>(&self, text: &'a str) -> Option<&'a str> {
        if text.starts_with(' ') || text.ends_with(' ') {
            let trimmed = text.trim();
            if !trimmed.is_empty() {
                Some(trimmed)
            } else {
                None
            }
        } else {
            None
        }
    }

    fn check_line(&self, line: &str) -> Vec<(usize, String, String)> {
        let mut issues = Vec::new();
        let chars: Vec<char> = line.chars().collect();
        let mut i = 0;

        while i < chars.len() {
            if chars[i] == '[' {
                let text_start_idx = i + 1;
                let mut text_end_idx = None;
                let mut link_start_idx = None;
                let mut link_end_idx = None;
                let mut bracket_depth = 1;
                let mut j = i + 1;

                // Find matching closing bracket
                while j < chars.len() {
                    match chars[j] {
                        '[' => bracket_depth += 1,
                        ']' => {
                            bracket_depth -= 1;
                            if bracket_depth == 0 {
                                text_end_idx = Some(j);
                                // Look for opening parenthesis
                                if j + 1 < chars.len() && chars[j + 1] == '(' {
                                    link_start_idx = Some(j + 2);
                                    // Find closing parenthesis
                                    let mut paren_depth = 1;
                                    let mut k = j + 2;
                                    while k < chars.len() {
                                        match chars[k] {
                                            '(' => paren_depth += 1,
                                            ')' => {
                                                paren_depth -= 1;
                                                if paren_depth == 0 {
                                                    link_end_idx = Some(k);
                                                    break;
                                                }
                                            }
                                            _ => {}
                                        }
                                        k += 1;
                                    }
                                }
                                break;
                            }
                        }
                        _ => {}
                    }
                    j += 1;
                }

                // If we found a complete link pattern
                if let (Some(text_end_idx), Some(link_start_idx), Some(link_end_idx)) =
                    (text_end_idx, link_start_idx, link_end_idx)
                {
                    // Extract text and link using safe char-based operations
                    let text: String = chars[text_start_idx..text_end_idx].iter().collect();
                    let link: String = chars[link_start_idx..link_end_idx].iter().collect();

                    // Check for spaces at start or end of text
                    if text.starts_with(' ') || text.ends_with(' ') {
                        let trimmed_text = text.trim();
                        if !trimmed_text.is_empty() {
                            // Safely reconstruct the original text using char indices
                            let original: String = chars[i..=link_end_idx].iter().collect();
                            let fixed = format!("[{}]({})", trimmed_text, link);

                            // Calculate the byte position for the column
                            // This is the byte offset of the start of the link
                            let byte_position = chars[..i].iter().collect::<String>().len() + 1;

                            issues.push((byte_position, original, fixed));
                        }
                    }

                    i = link_end_idx + 1;
                    continue;
                }
            }
            i += 1;
        }

        issues
    }
}

impl Rule for MD039NoSpaceInLinks {
    fn name(&self) -> &'static str {
        "MD039"
    }

    fn description(&self) -> &'static str {
        "Spaces inside link text"
    }

    fn category(&self) -> RuleCategory {
        RuleCategory::Link
    }

    fn should_skip(&self, ctx: &crate::lint_context::LintContext) -> bool {
        let content = ctx.content;
        content.is_empty() || !self.has_links(content)
    }

    /// Optimized check using document structure
    fn check_with_structure(
        &self,
        ctx: &crate::lint_context::LintContext,
        structure: &DocumentStructure,
    ) -> LintResult {
        let content = ctx.content;
        if structure.links.is_empty() {
            return Ok(Vec::new());
        }
        let mut warnings = Vec::new();
        let line_index = LineIndex::new(content.to_string());
        for link in &structure.links {
            if let Some(fixed_text) = self.check_link_text(&link.text) {
                let start_col = link.start_col;
                let line_num = link.line;
                let start_pos = line_index.line_col_to_byte_range(line_num, start_col).start;
                let original = format!("[{}]({})", link.text, link.url);
                let fixed = format!("[{}]({})", fixed_text, link.url);
                warnings.push(LintWarning {
                    rule_name: Some(self.name()),
                    line: line_num,
                    column: start_col,
                    message: WARNING_MESSAGE.to_string(),
                    severity: Severity::Warning,
                    fix: Some(Fix {
                        range: start_pos..start_pos + original.len(),
                        replacement: fixed,
                    }),
                });
            }
        }
        Ok(warnings)
    }

    fn check(&self, ctx: &crate::lint_context::LintContext) -> LintResult {
        if self.should_skip(ctx) {
            return Ok(Vec::new());
        }

        // Create document structure once
        let doc_structure = DocumentStructure::new(ctx.content);

        // Call check_with_structure
        self.check_with_structure(ctx, &doc_structure)
    }

    fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, LintError> {
        if self.should_skip(ctx) {
            return Ok(ctx.content.to_string());
        }

        // Get warnings using the consolidated check method
        let warnings = self.check(ctx)?;
        if warnings.is_empty() {
            return Ok(ctx.content.to_string());
        }

        // Apply fixes using the information from warnings
        // BTreeMap ensures fixes are applied in reverse order by start position
        let mut fixes: std::collections::BTreeMap<usize, Fix> = std::collections::BTreeMap::new();
        for warning in warnings {
            if let Some(fix) = warning.fix {
                // Use start position as key for sorting
                fixes.insert(fix.range.start, fix);
            }
        }

        let mut fixed_content = ctx.content.to_string();
        for (_, fix) in fixes.iter().rev() { // Iterate in reverse order of start position
             // Ensure range is valid for the current state of fixed_content
             if fix.range.end <= fixed_content.len() {
                fixed_content.replace_range(fix.range.clone(), &fix.replacement);
             } else {
                 // Log or handle invalid range error - maybe return error?
                 // For now, let's skip invalid ranges to avoid panic
                 eprintln!("Warning: Skipping fix for rule {} due to invalid range {:?} in content of length {}.",
                          self.name(), fix.range, fixed_content.len());
             }
        }

        Ok(fixed_content)
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn from_config(_config: &crate::config::Config) -> Box<dyn Rule>
    where
        Self: Sized,
    {
        Box::new(Self)
    }
}

impl crate::utils::document_structure::DocumentStructureExtensions for MD039NoSpaceInLinks {
    fn has_relevant_elements(
        &self,
        _ctx: &crate::lint_context::LintContext,
        doc_structure: &DocumentStructure,
    ) -> bool {
        !doc_structure.links.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_links() {
        let rule = MD039NoSpaceInLinks::new();
        let content = "[link](url) and [another link](url) here";
        let ctx = crate::lint_context::LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_spaces_both_ends() {
        let rule = MD039NoSpaceInLinks::new();
        let content = "[ link ](url) and [ another link ](url) here";
        let ctx = crate::lint_context::LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 2);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, "[link](url) and [another link](url) here");
    }

    #[test]
    fn test_space_at_start() {
        let rule = MD039NoSpaceInLinks::new();
        let content = "[ link](url) and [ another link](url) here";
        let ctx = crate::lint_context::LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 2);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, "[link](url) and [another link](url) here");
    }

    #[test]
    fn test_space_at_end() {
        let rule = MD039NoSpaceInLinks::new();
        let content = "[link ](url) and [another link ](url) here";
        let ctx = crate::lint_context::LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 2);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, "[link](url) and [another link](url) here");
    }

    #[test]
    fn test_link_in_code_block() {
        let rule = MD039NoSpaceInLinks::new();
        let content = "```
[ link ](url)
```
[ link ](url)";
        let ctx = crate::lint_context::LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(
            fixed,
            "```
[ link ](url)
```
[link](url)"
        );
    }

    #[test]
    fn test_multiple_links() {
        let rule = MD039NoSpaceInLinks::new();
        let content = "[ link ](url) and [ another ](url) in one line";
        let ctx = crate::lint_context::LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 2);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, "[link](url) and [another](url) in one line");
    }

    #[test]
    fn test_link_with_internal_spaces() {
        let rule = MD039NoSpaceInLinks::new();
        let content = "[this is link](url) and [ this is also link ](url)";
        let ctx = crate::lint_context::LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, "[this is link](url) and [this is also link](url)");
    }

    #[test]
    fn test_link_with_punctuation() {
        let rule = MD039NoSpaceInLinks::new();
        let content = "[ link! ](url) and [ link? ](url) here";
        let ctx = crate::lint_context::LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 2);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, "[link!](url) and [link?](url) here");
    }
}
