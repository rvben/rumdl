/// Rule MD011: No reversed link syntax
///
/// See [docs/md011.md](../../docs/md011.md) for full documentation, configuration, and examples.
use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, Severity};
use crate::utils::jinja_utils::is_in_jinja_template;
use crate::utils::range_utils::{LineIndex, calculate_match_range};
use crate::utils::regex_cache::get_cached_regex;
use crate::utils::skip_context::{is_in_html_comment, is_in_math_context};

// Reversed link detection pattern
const REVERSED_LINK_REGEX_STR: &str = r"(^|[^\\])\(([^()]+)\)\[([^\]]+)\]";

#[derive(Clone)]
pub struct MD011NoReversedLinks;

impl MD011NoReversedLinks {
    fn find_reversed_links(content: &str) -> Vec<(usize, usize, String, String)> {
        let mut results = Vec::new();
        let mut line_num = 1;

        for line in content.lines() {
            let mut last_end = 0;

            while let Some(cap) = get_cached_regex(REVERSED_LINK_REGEX_STR)
                .ok()
                .and_then(|re| re.captures(&line[last_end..]))
            {
                let match_obj = cap.get(0).unwrap();
                let prechar = &cap[1];
                let url = &cap[2];
                let text = &cap[3];

                // Check if the brackets at the end are escaped
                if text.ends_with('\\') {
                    last_end += match_obj.end();
                    continue;
                }

                // Manual negative lookahead: skip if followed by (
                // This prevents matching (text)[ref](url) patterns
                let end_pos = last_end + match_obj.end();
                if end_pos < line.len() && line[end_pos..].starts_with('(') {
                    last_end += match_obj.end();
                    continue;
                }

                // Calculate the actual column (accounting for any prefix character)
                let column = last_end + match_obj.start() + prechar.len() + 1;

                results.push((line_num, column, text.to_string(), url.to_string()));
                last_end += match_obj.end();
            }

            line_num += 1;
        }

        results
    }
}

impl Rule for MD011NoReversedLinks {
    fn name(&self) -> &'static str {
        "MD011"
    }

    fn description(&self) -> &'static str {
        "Reversed link syntax"
    }

    fn check(&self, ctx: &crate::lint_context::LintContext) -> LintResult {
        let content = ctx.content;
        let mut warnings = Vec::new();

        // Create LineIndex for correct byte position calculations across all line ending types
        let line_index = LineIndex::new(content.to_string());

        for (line_num, line) in content.lines().enumerate() {
            // Skip lines that are in front matter (use pre-computed info from LintContext)
            if ctx.line_info(line_num).is_some_and(|info| info.in_front_matter) {
                continue;
            }

            let byte_pos = line_index.get_line_start_byte(line_num + 1).unwrap_or(0);

            let mut last_end = 0;

            while let Some(cap) = get_cached_regex(REVERSED_LINK_REGEX_STR)
                .ok()
                .and_then(|re| re.captures(&line[last_end..]))
            {
                let match_obj = cap.get(0).unwrap();
                let prechar = &cap[1];
                let url = &cap[2];
                let text = &cap[3];

                // Check if the brackets at the end are escaped
                if text.ends_with('\\') {
                    last_end += match_obj.end();
                    continue;
                }

                // Manual negative lookahead: skip if followed by (
                // This prevents matching (text)[ref](url) patterns
                let end_pos = last_end + match_obj.end();
                if end_pos < line.len() && line[end_pos..].starts_with('(') {
                    last_end += match_obj.end();
                    continue;
                }

                // Calculate the actual position
                let match_start = last_end + match_obj.start() + prechar.len();
                let match_byte_pos = byte_pos + match_start;

                // Skip if in code block, inline code, HTML comments, math contexts, or Jinja templates
                if ctx.is_in_code_block_or_span(match_byte_pos)
                    || is_in_html_comment(content, match_byte_pos)
                    || is_in_math_context(ctx, match_byte_pos)
                    || is_in_jinja_template(content, match_byte_pos)
                {
                    last_end += match_obj.end();
                    continue;
                }

                // Calculate the range for the actual reversed link (excluding prechar)
                let actual_length = match_obj.len() - prechar.len();
                let (start_line, start_col, end_line, end_col) =
                    calculate_match_range(line_num + 1, line, match_start, actual_length);

                warnings.push(LintWarning {
                    rule_name: Some(self.name()),
                    message: format!("Reversed link syntax: use [{text}]({url}) instead"),
                    line: start_line,
                    column: start_col,
                    end_line,
                    end_column: end_col,
                    severity: Severity::Warning,
                    fix: Some(Fix {
                        range: {
                            let match_start_byte = byte_pos + match_start;
                            let match_end_byte = match_start_byte + actual_length;
                            match_start_byte..match_end_byte
                        },
                        replacement: format!("[{text}]({url})"),
                    }),
                });

                last_end += match_obj.end();
            }
        }

        Ok(warnings)
    }

    fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, LintError> {
        let content = ctx.content;
        let mut result = content.to_string();
        let mut offset: isize = 0;

        // Create LineIndex for correct byte position calculations across all line ending types
        let line_index = LineIndex::new(content.to_string());

        for (line_num, column, text, url) in Self::find_reversed_links(content) {
            // Skip if in front matter (line_num is 1-based from find_reversed_links)
            if line_num > 0 && ctx.line_info(line_num - 1).is_some_and(|info| info.in_front_matter) {
                continue;
            }

            // Calculate absolute position in original content using LineIndex
            let line_start = line_index.get_line_start_byte(line_num).unwrap_or(0);
            let pos = line_start + (column - 1);

            // Skip if in any skip context
            if !ctx.is_in_code_block_or_span(pos)
                && !is_in_html_comment(content, pos)
                && !is_in_math_context(ctx, pos)
                && !is_in_jinja_template(content, pos)
            {
                let adjusted_pos = (pos as isize + offset) as usize;
                let original = format!("({url})[{text}]");
                let replacement = format!("[{text}]({url})");

                // Make sure we have the right substring before replacing
                let end_pos = adjusted_pos + original.len();
                if end_pos <= result.len() && adjusted_pos < result.len() {
                    result.replace_range(adjusted_pos..end_pos, &replacement);
                    // Update offset based on the difference in lengths
                    offset += replacement.len() as isize - original.len() as isize;
                }
            }
        }

        Ok(result)
    }

    fn should_skip(&self, ctx: &crate::lint_context::LintContext) -> bool {
        ctx.content.is_empty() || !ctx.likely_has_links_or_images()
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn from_config(_config: &crate::config::Config) -> Box<dyn Rule>
    where
        Self: Sized,
    {
        Box::new(MD011NoReversedLinks)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lint_context::LintContext;

    #[test]
    fn test_md011_basic() {
        let rule = MD011NoReversedLinks;

        // Should detect reversed links
        let content = "(http://example.com)[Example]\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].line, 1);

        // Should not detect correct links
        let content = "[Example](http://example.com)\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 0);
    }

    #[test]
    fn test_md011_with_escaped_brackets() {
        let rule = MD011NoReversedLinks;

        // Should not detect if brackets are escaped
        let content = "(url)[text\\]\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 0);
    }

    #[test]
    fn test_md011_no_false_positive_with_reference_link() {
        let rule = MD011NoReversedLinks;

        // Should not detect (text)[ref](url) as reversed
        let content = "(text)[ref](url)\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 0);
    }

    #[test]
    fn test_md011_fix() {
        let rule = MD011NoReversedLinks;

        let content = "(http://example.com)[Example]\n(another/url)[text]\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, "[Example](http://example.com)\n[text](another/url)\n");
    }

    #[test]
    fn test_md011_in_code_block() {
        let rule = MD011NoReversedLinks;

        let content = "```\n(url)[text]\n```\n(url)[text]\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].line, 4);
    }

    #[test]
    fn test_md011_inline_code() {
        let rule = MD011NoReversedLinks;

        let content = "`(url)[text]` and (url)[text]\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].column, 19);
    }
}
