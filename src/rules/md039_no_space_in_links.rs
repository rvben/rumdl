use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};

/// Rule MD039: No space inside link text
///
/// See [docs/md039.md](../../docs/md039.md) for full documentation, configuration, and examples.
///
/// This rule is triggered when link text has leading or trailing spaces which can cause
/// unexpected rendering in some Markdown parsers.
#[derive(Debug, Default, Clone)]
pub struct MD039NoSpaceInLinks;

// Static definition for the warning message
const WARNING_MESSAGE: &str = "Remove spaces inside link text";

impl MD039NoSpaceInLinks {
    pub fn new() -> Self {
        Self
    }

    /// Fast check to see if content has any potential links or images
    #[inline]
    fn has_links_or_images(&self, content: &str) -> bool {
        (content.contains('[') && content.contains("]("))
            || (content.contains("![") && content.contains("]("))
    }

    /// Shared parser: yields (is_image, text, url, link_start, link_end, text_start, text_end)
    fn parse_links_and_images(
        content: &str,
    ) -> Vec<(bool, &str, &str, usize, usize, usize, usize)> {
        let mut results = Vec::new();

        // Early return if no potential links
        if !content.contains('[') || !content.contains("](") {
            return results;
        }

        // Pre-compute code block ranges once for efficiency
        let code_block_ranges =
            crate::utils::code_block_utils::CodeBlockUtils::detect_code_blocks(content);

        // Convert to char indices for efficient processing
        let chars: Vec<(usize, char)> = content.char_indices().collect();
        let mut idx = 0;

        while idx < chars.len() {
            let (byte_i, c) = chars[idx];

            // Skip if in code block (optimized check)
            if code_block_ranges
                .iter()
                .any(|&(start, end)| byte_i >= start && byte_i < end)
            {
                idx += 1;
                continue;
            }

            let is_image = c == '!' && idx + 1 < chars.len() && chars[idx + 1].1 == '[';
            let start_bracket = if is_image {
                if idx + 1 < chars.len() && chars[idx + 1].1 == '[' {
                    idx + 1
                } else {
                    usize::MAX
                }
            } else if c == '[' {
                idx
            } else {
                usize::MAX
            };

            if start_bracket == usize::MAX {
                idx += 1;
                continue;
            }

            // Find matching closing bracket (not escaped), allowing multi-line
            let mut j = start_bracket + 1;
            while j < chars.len() {
                if chars[j].1 == ']' {
                    let mut backslashes = 0;
                    let mut k = j;
                    while k > start_bracket && chars[k - 1].1 == '\\' {
                        backslashes += 1;
                        k -= 1;
                    }
                    if backslashes % 2 == 0 {
                        break;
                    }
                }
                j += 1;
            }

            if j >= chars.len() {
                idx = j + 1;
                continue;
            }

            // Check for ( after ] (allow whitespace)
            let mut k = j + 1;
            while k < chars.len() && chars[k].1.is_whitespace() {
                k += 1;
            }
            if k >= chars.len() || chars[k].1 != '(' {
                idx = j + 1;
                continue;
            }

            // Find matching ) for url
            let mut l = k + 1;
            let mut paren_count = 1;
            while l < chars.len() {
                if chars[l].1 == '(' {
                    paren_count += 1;
                } else if chars[l].1 == ')' {
                    paren_count -= 1;
                    if paren_count == 0 {
                        break;
                    }
                }
                l += 1;
            }

            if paren_count != 0 || l >= chars.len() {
                idx = l + 1;
                continue;
            }

            let text_start = chars[start_bracket].0 + chars[start_bracket].1.len_utf8();
            let text_end = chars[j].0;
            let url_start = chars[k].0 + chars[k].1.len_utf8();
            let url_end = chars[l].0;

            if text_end < text_start
                || url_end < url_start
                || text_end > content.len()
                || url_end > content.len()
            {
                idx = l + 1;
                continue;
            }

            let text = &content[text_start..text_end];
            let url = &content[url_start..url_end];
            let link_start = if is_image {
                chars[idx].0
            } else {
                chars[start_bracket].0
            };
            let link_end = chars[l].0 + chars[l].1.len_utf8();

            results.push((
                is_image, text, url, link_start, link_end, text_start, text_end,
            ));
            idx = l + 1;
        }
        results
    }

    fn trim_link_text_preserve_escapes(text: &str) -> &str {
        // Find first non-whitespace
        let start = text
            .char_indices()
            .find(|&(_, c)| !c.is_whitespace())
            .map(|(i, _)| i)
            .unwrap_or(text.len());
        // Find last non-whitespace
        let end = text
            .char_indices()
            .rev()
            .find(|&(_, c)| !c.is_whitespace())
            .map(|(i, c)| i + c.len_utf8())
            .unwrap_or(0);
        if start >= end {
            ""
        } else {
            &text[start..end]
        }
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

    fn as_maybe_document_structure(&self) -> Option<&dyn crate::rule::MaybeDocumentStructure> {
        Some(self)
    }

    fn should_skip(&self, ctx: &crate::lint_context::LintContext) -> bool {
        let content = ctx.content;
        content.is_empty() || !self.has_links_or_images(content)
    }

    fn check(&self, ctx: &crate::lint_context::LintContext) -> LintResult {
        if self.should_skip(ctx) {
            return Ok(Vec::new());
        }

        let content = ctx.content;
        let mut warnings = Vec::new();

        // Parse links and images once
        let links_and_images = Self::parse_links_and_images(content);

        // Early return if no links found
        if links_and_images.is_empty() {
            return Ok(Vec::new());
        }

        for (is_image, text, url, link_start, _link_end, _text_start, _text_end) in links_and_images
        {
            // Unescape for whitespace check
            let mut unesc = String::with_capacity(text.len());
            let mut tchars = text.chars().peekable();
            while let Some(c) = tchars.next() {
                if c == '\\' {
                    if let Some(&next) = tchars.peek() {
                        unesc.push(next);
                        tchars.next();
                    }
                } else {
                    unesc.push(c);
                }
            }

            let needs_warning = if unesc.chars().all(|c| c.is_whitespace()) {
                true
            } else {
                let trimmed = text.trim_matches(|c: char| c.is_whitespace());
                text != trimmed
            };

            if needs_warning {
                let original = if is_image {
                    format!("![{}]({})", text, url)
                } else {
                    format!("[{}]({})", text, url)
                };
                let fixed = if unesc.chars().all(|c| c.is_whitespace()) {
                    if is_image {
                        format!("![]({})", url)
                    } else {
                        format!("[]({})", url)
                    }
                } else {
                    let trimmed = Self::trim_link_text_preserve_escapes(text);
                    if is_image {
                        format!("![{}]({})", trimmed, url)
                    } else {
                        format!("[{}]({})", trimmed, url)
                    }
                };
                let (line, column) = ctx.offset_to_line_col(link_start);
                warnings.push(LintWarning {
                    rule_name: Some(self.name()),
                    line,
                    column,
                    end_line: line,
                    end_column: column + original.len(),
                    message: WARNING_MESSAGE.to_string(),
                    severity: Severity::Warning,
                    fix: Some(Fix {
                        range: link_start..link_start + original.len(),
                        replacement: fixed,
                    }),
                });
            }
        }
        Ok(warnings)
    }

    fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, LintError> {
        if self.should_skip(ctx) {
            return Ok(ctx.content.to_string());
        }
        let content = ctx.content;
        let mut fixes = Vec::new();
        for (is_image, text, url, link_start, link_end, _text_start, _text_end) in
            Self::parse_links_and_images(content)
        {
            // Unescape for whitespace check
            let mut unesc = String::with_capacity(text.len());
            let mut tchars = text.chars().peekable();
            while let Some(c) = tchars.next() {
                if c == '\\' {
                    if let Some(&next) = tchars.peek() {
                        unesc.push(next);
                        tchars.next();
                    }
                } else {
                    unesc.push(c);
                }
            }
            let replacement = if unesc.chars().all(|c| c.is_whitespace()) {
                if is_image {
                    format!("![]({})", url)
                } else {
                    format!("[]({})", url)
                }
            } else {
                let trimmed = Self::trim_link_text_preserve_escapes(text);
                if is_image {
                    format!("![{}]({})", trimmed, url)
                } else {
                    format!("[{}]({})", trimmed, url)
                }
            };
            fixes.push((link_start, link_end, replacement));
        }
        if fixes.is_empty() {
            return Ok(content.to_string());
        }
        let mut fixed = String::with_capacity(content.len());
        let mut last = 0;
        for (start, end, replacement) in fixes {
            fixed.push_str(&content[last..start]);
            fixed.push_str(&replacement);
            last = end;
        }
        fixed.push_str(&content[last..]);
        Ok(fixed)
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
        doc_structure: &crate::utils::document_structure::DocumentStructure,
    ) -> bool {
        !doc_structure.links.is_empty() || !doc_structure.images.is_empty()
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

    #[test]
    fn test_parity_only_whitespace_and_newlines_minimal() {
        let rule = MD039NoSpaceInLinks::new();
        let content = "[   \n  ](url) and [\t\n\t](url)";
        let ctx = crate::lint_context::LintContext::new(content);
        let fixed = rule.fix(&ctx).unwrap();
        // markdownlint removes all whitespace, resulting in empty link text
        assert_eq!(fixed, "[](url) and [](url)");
    }

    #[test]
    fn test_parity_internal_newlines_minimal() {
        let rule = MD039NoSpaceInLinks::new();
        let content = "[link\ntext](url) and [ another\nlink ](url)";
        let ctx = crate::lint_context::LintContext::new(content);
        let fixed = rule.fix(&ctx).unwrap();
        // markdownlint trims only leading/trailing whitespace, preserves internal newlines
        assert_eq!(fixed, "[link\ntext](url) and [another\nlink](url)");
    }

    #[test]
    fn test_parity_escaped_brackets_minimal() {
        let rule = MD039NoSpaceInLinks::new();
        let content = "[link\\]](url) and [link\\[]](url)";
        let ctx = crate::lint_context::LintContext::new(content);
        let fixed = rule.fix(&ctx).unwrap();
        // markdownlint does not trim or remove escapes, so output should be unchanged
        assert_eq!(fixed, "[link\\]](url) and [link\\[]](url)");
    }
}
