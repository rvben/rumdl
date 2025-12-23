/// Rule MD011: No reversed link syntax
///
/// See [docs/md011.md](../../docs/md011.md) for full documentation, configuration, and examples.
use crate::filtered_lines::FilteredLinesExt;
use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, Severity};
use crate::utils::range_utils::calculate_match_range;
use crate::utils::regex_cache::get_cached_regex;
use crate::utils::skip_context::is_in_math_context;

// Reversed link detection pattern
const REVERSED_LINK_REGEX_STR: &str = r"(^|[^\\])\(([^()]+)\)\[([^\]]+)\]";

/// Classification of a link component
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LinkComponent {
    /// Clear URL: has protocol, www., mailto:, or path prefix
    ClearUrl,
    /// Multiple words or sentence-like (likely link text, not URL)
    MultiWord,
    /// Single word - could be either URL or text
    Ambiguous,
}

/// Information about a detected reversed link pattern
#[derive(Debug, Clone)]
struct ReversedLinkInfo {
    line_num: usize,
    column: usize,
    /// Content found in parentheses
    paren_content: String,
    /// Content found in square brackets
    bracket_content: String,
    /// Classification of parentheses content
    paren_type: LinkComponent,
    /// Classification of bracket content
    bracket_type: LinkComponent,
}

impl ReversedLinkInfo {
    /// Determine the correct order: returns (text, url)
    fn correct_order(&self) -> (&str, &str) {
        use LinkComponent::*;

        match (self.paren_type, self.bracket_type) {
            // One side is clearly a URL - that's the URL
            (ClearUrl, _) => (&self.bracket_content, &self.paren_content),
            (_, ClearUrl) => (&self.paren_content, &self.bracket_content),

            // One side is multi-word - that's the text, other is URL
            (MultiWord, _) => (&self.paren_content, &self.bracket_content),
            (_, MultiWord) => (&self.bracket_content, &self.paren_content),

            // Both ambiguous: assume standard reversed pattern (url)[text]
            (Ambiguous, Ambiguous) => (&self.bracket_content, &self.paren_content),
        }
    }

    /// Get the original pattern as it appears in the source
    fn original_pattern(&self) -> String {
        format!("({})[{}]", self.paren_content, self.bracket_content)
    }

    /// Get the corrected pattern
    fn corrected_pattern(&self) -> String {
        let (text, url) = self.correct_order();
        format!("[{text}]({url})")
    }
}

#[derive(Clone)]
pub struct MD011NoReversedLinks;

impl MD011NoReversedLinks {
    /// Classify a link component as URL, multi-word text, or ambiguous
    fn classify_component(s: &str) -> LinkComponent {
        let trimmed = s.trim();

        // Check for clear URL indicators
        if trimmed.starts_with("http://")
            || trimmed.starts_with("https://")
            || trimmed.starts_with("ftp://")
            || trimmed.starts_with("www.")
            || (trimmed.starts_with("mailto:") && trimmed.contains('@'))
            || (trimmed.starts_with('/') && trimmed.len() > 1)
            || (trimmed.starts_with("./") || trimmed.starts_with("../"))
            || (trimmed.starts_with('#') && trimmed.len() > 1 && !trimmed[1..].contains(' '))
        {
            return LinkComponent::ClearUrl;
        }

        // Multi-word text is likely a description, not a URL
        if trimmed.contains(' ') {
            return LinkComponent::MultiWord;
        }

        // Single word - could be either
        LinkComponent::Ambiguous
    }

    fn find_reversed_links(content: &str) -> Vec<ReversedLinkInfo> {
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
                let paren_content = cap[2].to_string();
                let bracket_content = cap[3].to_string();

                // Skip wiki-link patterns: if bracket content starts with [ or ends with ]
                // This handles cases like (url)[[wiki-link]] being misdetected
                if bracket_content.starts_with('[') || bracket_content.ends_with(']') {
                    last_end += match_obj.end();
                    continue;
                }

                // Skip footnote references: [^footnote]
                // This prevents false positives like [link](url)[^footnote]
                if bracket_content.starts_with('^') {
                    last_end += match_obj.end();
                    continue;
                }

                // Check if the brackets at the end are escaped
                if bracket_content.ends_with('\\') {
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

                // Classify both components
                let paren_type = Self::classify_component(&paren_content);
                let bracket_type = Self::classify_component(&bracket_content);

                // Calculate the actual column (accounting for any prefix character)
                let column = last_end + match_obj.start() + prechar.len() + 1;

                results.push(ReversedLinkInfo {
                    line_num,
                    column,
                    paren_content,
                    bracket_content,
                    paren_type,
                    bracket_type,
                });

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
        let mut warnings = Vec::new();

        let line_index = &ctx.line_index;

        // Use filtered_lines() to automatically skip front-matter
        for filtered_line in ctx.filtered_lines().skip_front_matter() {
            let line_num = filtered_line.line_num;
            let line = filtered_line.content;

            let byte_pos = line_index.get_line_start_byte(line_num).unwrap_or(0);

            let mut last_end = 0;

            while let Some(cap) = get_cached_regex(REVERSED_LINK_REGEX_STR)
                .ok()
                .and_then(|re| re.captures(&line[last_end..]))
            {
                let match_obj = cap.get(0).unwrap();
                let prechar = &cap[1];
                let paren_content = cap[2].to_string();
                let bracket_content = cap[3].to_string();

                // Skip wiki-link patterns: if bracket content starts with [ or ends with ]
                // This handles cases like (url)[[wiki-link]] being misdetected
                if bracket_content.starts_with('[') || bracket_content.ends_with(']') {
                    last_end += match_obj.end();
                    continue;
                }

                // Skip footnote references: [^footnote]
                // This prevents false positives like [link](url)[^footnote]
                if bracket_content.starts_with('^') {
                    last_end += match_obj.end();
                    continue;
                }

                // Check if the brackets at the end are escaped
                if bracket_content.ends_with('\\') {
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
                    || ctx.is_in_html_comment(match_byte_pos)
                    || is_in_math_context(ctx, match_byte_pos)
                    || ctx.is_in_jinja_range(match_byte_pos)
                {
                    last_end += match_obj.end();
                    continue;
                }

                // Classify both components and determine correct order
                let paren_type = Self::classify_component(&paren_content);
                let bracket_type = Self::classify_component(&bracket_content);

                let info = ReversedLinkInfo {
                    line_num,
                    column: match_start + 1,
                    paren_content,
                    bracket_content,
                    paren_type,
                    bracket_type,
                };

                let (text, url) = info.correct_order();

                // Calculate the range for the actual reversed link (excluding prechar)
                let actual_length = match_obj.len() - prechar.len();
                let (start_line, start_col, end_line, end_col) =
                    calculate_match_range(line_num, line, match_start, actual_length);

                warnings.push(LintWarning {
                    rule_name: Some(self.name().to_string()),
                    message: format!("Reversed link syntax: use [{text}]({url}) instead"),
                    line: start_line,
                    column: start_col,
                    end_line,
                    end_column: end_col,
                    severity: Severity::Error,
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

        let line_index = &ctx.line_index;

        for info in Self::find_reversed_links(content) {
            // Calculate absolute position in original content using LineIndex
            let line_start = line_index.get_line_start_byte(info.line_num).unwrap_or(0);
            let pos = line_start + (info.column - 1);

            // Skip if in front matter using centralized utility
            if ctx.is_in_front_matter(pos) {
                continue;
            }

            // Skip if in any skip context
            if !ctx.is_in_code_block_or_span(pos)
                && !ctx.is_in_html_comment(pos)
                && !is_in_math_context(ctx, pos)
                && !ctx.is_in_jinja_range(pos)
            {
                let adjusted_pos = (pos as isize + offset) as usize;

                // Use the info struct to get both original and corrected patterns
                let original = info.original_pattern();
                let replacement = info.corrected_pattern();

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
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].line, 1);

        // Should not detect correct links
        let content = "[Example](http://example.com)\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 0);
    }

    #[test]
    fn test_md011_with_escaped_brackets() {
        let rule = MD011NoReversedLinks;

        // Should not detect if brackets are escaped
        let content = "(url)[text\\]\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 0);
    }

    #[test]
    fn test_md011_no_false_positive_with_reference_link() {
        let rule = MD011NoReversedLinks;

        // Should not detect (text)[ref](url) as reversed
        let content = "(text)[ref](url)\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 0);
    }

    #[test]
    fn test_md011_fix() {
        let rule = MD011NoReversedLinks;

        let content = "(http://example.com)[Example]\n(another/url)[text]\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, "[Example](http://example.com)\n[text](another/url)\n");
    }

    #[test]
    fn test_md011_in_code_block() {
        let rule = MD011NoReversedLinks;

        let content = "```\n(url)[text]\n```\n(url)[text]\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].line, 4);
    }

    #[test]
    fn test_md011_inline_code() {
        let rule = MD011NoReversedLinks;

        let content = "`(url)[text]` and (url)[text]\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].column, 19);
    }

    #[test]
    fn test_md011_no_false_positive_with_footnote() {
        let rule = MD011NoReversedLinks;

        // Should not detect [link](url)[^footnote] as reversed - this is valid markdown
        // The [^footnote] is a footnote reference, not part of a reversed link
        let content = "Some text with [a link](https://example.com/)[^ft].\n\n[^ft]: Note.\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 0);

        // Also test with multiple footnotes
        let content = "[link1](url1)[^1] and [link2](url2)[^2]\n\n[^1]: First\n[^2]: Second\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 0);

        // But should still detect actual reversed links
        let content = "(url)[text] and [link](url)[^footnote]\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].line, 1);
        assert_eq!(warnings[0].column, 1);
    }
}
