use crate::lint_context::LintContext;
use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use pulldown_cmark::LinkType;

/// Describes what type of whitespace issue was found
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WhitespaceIssue {
    Leading,
    Trailing,
    Both,
}

impl WhitespaceIssue {
    fn message(self, is_image: bool) -> String {
        let element = if is_image { "Image" } else { "Link" };
        match self {
            WhitespaceIssue::Leading => {
                format!("{element} destination has leading whitespace")
            }
            WhitespaceIssue::Trailing => {
                format!("{element} destination has trailing whitespace")
            }
            WhitespaceIssue::Both => {
                format!("{element} destination has leading and trailing whitespace")
            }
        }
    }
}

/// Rule MD062: No whitespace in link destinations
///
/// See [docs/md062.md](../../docs/md062.md) for full documentation, configuration, and examples.
///
/// This rule is triggered when link destinations have leading or trailing whitespace
/// inside the parentheses, which is a common copy-paste error.
///
/// Examples that trigger this rule:
/// - `[text]( url)` - leading space
/// - `[text](url )` - trailing space
/// - `[text]( url )` - both
///
/// The fix trims the whitespace: `[text](url)`
#[derive(Debug, Default, Clone)]
pub struct MD062LinkDestinationWhitespace;

impl MD062LinkDestinationWhitespace {
    pub fn new() -> Self {
        Self
    }

    /// Extract the destination portion from a link's raw text
    /// Returns (dest_start_offset, dest_end_offset, raw_dest) relative to link start
    fn extract_destination_info<'a>(&self, raw_link: &'a str) -> Option<(usize, usize, &'a str)> {
        // Find the opening parenthesis for the destination
        // Handle nested brackets in link text: [text [nested]](url)
        let mut bracket_depth = 0;
        let mut paren_start = None;

        for (i, c) in raw_link.char_indices() {
            match c {
                '[' => bracket_depth += 1,
                ']' => {
                    bracket_depth -= 1;
                    if bracket_depth == 0 {
                        // Next char should be '(' for inline links
                        let rest = &raw_link[i + 1..];
                        if rest.starts_with('(') {
                            paren_start = Some(i + 1);
                        }
                        break;
                    }
                }
                _ => {}
            }
        }

        let paren_start = paren_start?;

        // Find matching closing parenthesis
        let dest_content_start = paren_start + 1; // After '('
        let rest = &raw_link[dest_content_start..];

        // Find the closing paren, handling nested parens and titles
        let mut depth = 1;
        let mut in_angle_brackets = false;
        let mut dest_content_end = rest.len();

        for (i, c) in rest.char_indices() {
            match c {
                '<' if !in_angle_brackets => in_angle_brackets = true,
                '>' if in_angle_brackets => in_angle_brackets = false,
                '(' if !in_angle_brackets => depth += 1,
                ')' if !in_angle_brackets => {
                    depth -= 1;
                    if depth == 0 {
                        dest_content_end = i;
                        break;
                    }
                }
                _ => {}
            }
        }

        let dest_content = &rest[..dest_content_end];

        Some((dest_content_start, dest_content_start + dest_content_end, dest_content))
    }

    /// Check if destination has leading/trailing whitespace
    /// Returns the type of whitespace issue found, if any
    fn check_destination_whitespace(&self, full_dest: &str) -> Option<WhitespaceIssue> {
        if full_dest.is_empty() {
            return None;
        }

        let first_char = full_dest.chars().next();
        let last_char = full_dest.chars().last();

        let has_leading = first_char.is_some_and(|c| c.is_whitespace());

        // Check for trailing whitespace - either at the end or before title
        let has_trailing = if last_char.is_some_and(|c| c.is_whitespace()) {
            true
        } else if let Some(title_start) = full_dest.find(['"', '\'']) {
            let url_portion = &full_dest[..title_start];
            url_portion.ends_with(char::is_whitespace)
        } else {
            false
        };

        match (has_leading, has_trailing) {
            (true, true) => Some(WhitespaceIssue::Both),
            (true, false) => Some(WhitespaceIssue::Leading),
            (false, true) => Some(WhitespaceIssue::Trailing),
            (false, false) => None,
        }
    }

    /// Create the fixed link text
    fn create_fix(&self, raw_link: &str) -> Option<String> {
        let (dest_start, dest_end, _) = self.extract_destination_info(raw_link)?;

        // Get the full destination content (may include title)
        let full_dest_content = &raw_link[dest_start..dest_end];

        // Split into URL and optional title
        let (url_part, title_part) = if let Some(title_start) = full_dest_content.find(['"', '\'']) {
            let url = full_dest_content[..title_start].trim();
            let title = &full_dest_content[title_start..];
            (url, Some(title.trim()))
        } else {
            (full_dest_content.trim(), None)
        };

        // Reconstruct: text part + ( + trimmed_url + optional_title + )
        let text_part = &raw_link[..dest_start]; // Includes '[text]('

        let mut fixed = String::with_capacity(raw_link.len());
        fixed.push_str(text_part);
        fixed.push_str(url_part);
        if let Some(title) = title_part {
            fixed.push(' ');
            fixed.push_str(title);
        }
        fixed.push(')');

        // Only return fix if it actually changed something
        if fixed != raw_link { Some(fixed) } else { None }
    }
}

impl Rule for MD062LinkDestinationWhitespace {
    fn name(&self) -> &'static str {
        "MD062"
    }

    fn description(&self) -> &'static str {
        "Link destination should not have leading or trailing whitespace"
    }

    fn category(&self) -> RuleCategory {
        RuleCategory::Link
    }

    fn should_skip(&self, ctx: &LintContext) -> bool {
        ctx.content.is_empty() || !ctx.likely_has_links_or_images()
    }

    fn check(&self, ctx: &LintContext) -> LintResult {
        let mut warnings = Vec::new();

        // Process links
        for link in &ctx.links {
            // Only check inline links, not reference links
            if link.is_reference || !matches!(link.link_type, LinkType::Inline) {
                continue;
            }

            // Skip links inside Jinja templates
            if ctx.is_in_jinja_range(link.byte_offset) {
                continue;
            }

            // Get raw link text from content
            let raw_link = &ctx.content[link.byte_offset..link.byte_end];

            // Extract destination info and check for whitespace issues
            if let Some((_, _, raw_dest)) = self.extract_destination_info(raw_link)
                && let Some(issue) = self.check_destination_whitespace(raw_dest)
                && let Some(fixed) = self.create_fix(raw_link)
            {
                warnings.push(LintWarning {
                    rule_name: Some(self.name().to_string()),
                    line: link.line,
                    column: link.start_col + 1,
                    end_line: link.line,
                    end_column: link.end_col + 1,
                    message: issue.message(false),
                    severity: Severity::Warning,
                    fix: Some(Fix {
                        range: link.byte_offset..link.byte_end,
                        replacement: fixed,
                    }),
                });
            }
        }

        // Process images
        for image in &ctx.images {
            // Only check inline images, not reference images
            if image.is_reference || !matches!(image.link_type, LinkType::Inline) {
                continue;
            }

            // Skip images inside Jinja templates
            if ctx.is_in_jinja_range(image.byte_offset) {
                continue;
            }

            // Get raw image text from content
            let raw_image = &ctx.content[image.byte_offset..image.byte_end];

            // For images, skip the leading '!'
            let link_portion = raw_image.strip_prefix('!').unwrap_or(raw_image);

            // Extract destination info and check for whitespace issues
            if let Some((_, _, raw_dest)) = self.extract_destination_info(link_portion)
                && let Some(issue) = self.check_destination_whitespace(raw_dest)
                && let Some(fixed_link) = self.create_fix(link_portion)
            {
                let fixed = format!("!{fixed_link}");
                warnings.push(LintWarning {
                    rule_name: Some(self.name().to_string()),
                    line: image.line,
                    column: image.start_col + 1,
                    end_line: image.line,
                    end_column: image.end_col + 1,
                    message: issue.message(true),
                    severity: Severity::Warning,
                    fix: Some(Fix {
                        range: image.byte_offset..image.byte_end,
                        replacement: fixed,
                    }),
                });
            }
        }

        Ok(warnings)
    }

    fn fix(&self, ctx: &LintContext) -> Result<String, LintError> {
        let warnings = self.check(ctx)?;

        if warnings.is_empty() {
            return Ok(ctx.content.to_string());
        }

        let mut content = ctx.content.to_string();
        let mut fixes: Vec<_> = warnings
            .into_iter()
            .filter_map(|w| w.fix.map(|f| (f.range.start, f.range.end, f.replacement)))
            .collect();

        // Sort by position and apply in reverse order
        fixes.sort_by_key(|(start, _, _)| *start);

        for (start, end, replacement) in fixes.into_iter().rev() {
            content.replace_range(start..end, &replacement);
        }

        Ok(content)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::MarkdownFlavor;

    #[test]
    fn test_no_whitespace() {
        let rule = MD062LinkDestinationWhitespace::new();
        let content = "[link](https://example.com)";
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_leading_whitespace() {
        let rule = MD062LinkDestinationWhitespace::new();
        let content = "[link]( https://example.com)";
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 1);
        assert_eq!(
            warnings[0].fix.as_ref().unwrap().replacement,
            "[link](https://example.com)"
        );
    }

    #[test]
    fn test_trailing_whitespace() {
        let rule = MD062LinkDestinationWhitespace::new();
        let content = "[link](https://example.com )";
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 1);
        assert_eq!(
            warnings[0].fix.as_ref().unwrap().replacement,
            "[link](https://example.com)"
        );
    }

    #[test]
    fn test_both_whitespace() {
        let rule = MD062LinkDestinationWhitespace::new();
        let content = "[link]( https://example.com )";
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 1);
        assert_eq!(
            warnings[0].fix.as_ref().unwrap().replacement,
            "[link](https://example.com)"
        );
    }

    #[test]
    fn test_multiple_spaces() {
        let rule = MD062LinkDestinationWhitespace::new();
        let content = "[link](   https://example.com   )";
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 1);
        assert_eq!(
            warnings[0].fix.as_ref().unwrap().replacement,
            "[link](https://example.com)"
        );
    }

    #[test]
    fn test_with_title() {
        let rule = MD062LinkDestinationWhitespace::new();
        let content = "[link]( https://example.com \"title\")";
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 1);
        assert_eq!(
            warnings[0].fix.as_ref().unwrap().replacement,
            "[link](https://example.com \"title\")"
        );
    }

    #[test]
    fn test_image_leading_whitespace() {
        let rule = MD062LinkDestinationWhitespace::new();
        let content = "![alt]( https://example.com/image.png)";
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 1);
        assert_eq!(
            warnings[0].fix.as_ref().unwrap().replacement,
            "![alt](https://example.com/image.png)"
        );
    }

    #[test]
    fn test_multiple_links() {
        let rule = MD062LinkDestinationWhitespace::new();
        let content = "[a]( url1) and [b](url2 ) here";
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 2);
    }

    #[test]
    fn test_fix() {
        let rule = MD062LinkDestinationWhitespace::new();
        let content = "[link]( https://example.com ) and ![img]( /path/to/img.png )";
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, "[link](https://example.com) and ![img](/path/to/img.png)");
    }

    #[test]
    fn test_reference_links_skipped() {
        let rule = MD062LinkDestinationWhitespace::new();
        let content = "[link][ref]\n\n[ref]: https://example.com";
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_nested_brackets() {
        let rule = MD062LinkDestinationWhitespace::new();
        let content = "[text [nested]]( https://example.com)";
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 1);
    }

    #[test]
    fn test_empty_destination() {
        let rule = MD062LinkDestinationWhitespace::new();
        let content = "[link]()";
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_tabs_and_newlines() {
        let rule = MD062LinkDestinationWhitespace::new();
        let content = "[link](\thttps://example.com\t)";
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 1);
        assert_eq!(
            warnings[0].fix.as_ref().unwrap().replacement,
            "[link](https://example.com)"
        );
    }

    // Edge case tests for comprehensive coverage

    #[test]
    fn test_trailing_whitespace_after_title() {
        let rule = MD062LinkDestinationWhitespace::new();
        let content = "[link](https://example.com \"title\" )";
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 1);
        assert_eq!(
            warnings[0].fix.as_ref().unwrap().replacement,
            "[link](https://example.com \"title\")"
        );
    }

    #[test]
    fn test_leading_and_trailing_with_title() {
        let rule = MD062LinkDestinationWhitespace::new();
        let content = "[link]( https://example.com \"title\" )";
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 1);
        assert_eq!(
            warnings[0].fix.as_ref().unwrap().replacement,
            "[link](https://example.com \"title\")"
        );
    }

    #[test]
    fn test_multiple_spaces_before_title() {
        let rule = MD062LinkDestinationWhitespace::new();
        let content = "[link](https://example.com  \"title\")";
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 1);
        assert_eq!(
            warnings[0].fix.as_ref().unwrap().replacement,
            "[link](https://example.com \"title\")"
        );
    }

    #[test]
    fn test_single_quote_title() {
        let rule = MD062LinkDestinationWhitespace::new();
        let content = "[link]( https://example.com 'title')";
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 1);
        assert_eq!(
            warnings[0].fix.as_ref().unwrap().replacement,
            "[link](https://example.com 'title')"
        );
    }

    #[test]
    fn test_single_quote_title_trailing_space() {
        let rule = MD062LinkDestinationWhitespace::new();
        let content = "[link](https://example.com 'title' )";
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 1);
        assert_eq!(
            warnings[0].fix.as_ref().unwrap().replacement,
            "[link](https://example.com 'title')"
        );
    }

    #[test]
    fn test_wikipedia_style_url() {
        // Wikipedia URLs with parentheses should work correctly
        let rule = MD062LinkDestinationWhitespace::new();
        let content = "[wiki]( https://en.wikipedia.org/wiki/Rust_(programming_language) )";
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 1);
        assert_eq!(
            warnings[0].fix.as_ref().unwrap().replacement,
            "[wiki](https://en.wikipedia.org/wiki/Rust_(programming_language))"
        );
    }

    #[test]
    fn test_angle_bracket_url_no_warning() {
        // Angle bracket URLs can contain spaces per CommonMark, so we should skip them
        let rule = MD062LinkDestinationWhitespace::new();
        let content = "[link](<https://example.com/path with spaces>)";
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();
        // Angle bracket URLs are allowed to have spaces, no warning expected
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_image_with_title() {
        let rule = MD062LinkDestinationWhitespace::new();
        let content = "![alt]( https://example.com/img.png \"Image title\" )";
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 1);
        assert_eq!(
            warnings[0].fix.as_ref().unwrap().replacement,
            "![alt](https://example.com/img.png \"Image title\")"
        );
    }

    #[test]
    fn test_only_whitespace_in_destination() {
        let rule = MD062LinkDestinationWhitespace::new();
        let content = "[link](   )";
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].fix.as_ref().unwrap().replacement, "[link]()");
    }

    #[test]
    fn test_code_block_skipped() {
        let rule = MD062LinkDestinationWhitespace::new();
        let content = "```\n[link]( https://example.com )\n```";
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_inline_code_not_skipped() {
        // Links in inline code are not valid markdown anyway
        let rule = MD062LinkDestinationWhitespace::new();
        let content = "text `[link]( url )` more text";
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();
        // pulldown-cmark doesn't parse this as a link since it's in code
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_valid_link_with_title_no_warning() {
        let rule = MD062LinkDestinationWhitespace::new();
        let content = "[link](https://example.com \"Title\")";
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_mixed_links_on_same_line() {
        let rule = MD062LinkDestinationWhitespace::new();
        let content = "[good](https://example.com) and [bad]( https://example.com ) here";
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 1);
        assert_eq!(
            warnings[0].fix.as_ref().unwrap().replacement,
            "[bad](https://example.com)"
        );
    }

    #[test]
    fn test_fix_multiple_on_same_line() {
        let rule = MD062LinkDestinationWhitespace::new();
        let content = "[a]( url1 ) and [b]( url2 )";
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, "[a](url1) and [b](url2)");
    }

    #[test]
    fn test_complex_nested_brackets() {
        let rule = MD062LinkDestinationWhitespace::new();
        let content = "[text [with [deeply] nested] brackets]( https://example.com )";
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 1);
    }

    #[test]
    fn test_url_with_query_params() {
        let rule = MD062LinkDestinationWhitespace::new();
        let content = "[link]( https://example.com?foo=bar&baz=qux )";
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 1);
        assert_eq!(
            warnings[0].fix.as_ref().unwrap().replacement,
            "[link](https://example.com?foo=bar&baz=qux)"
        );
    }

    #[test]
    fn test_url_with_fragment() {
        let rule = MD062LinkDestinationWhitespace::new();
        let content = "[link]( https://example.com#section )";
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 1);
        assert_eq!(
            warnings[0].fix.as_ref().unwrap().replacement,
            "[link](https://example.com#section)"
        );
    }

    #[test]
    fn test_relative_path() {
        let rule = MD062LinkDestinationWhitespace::new();
        let content = "[link]( ./path/to/file.md )";
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 1);
        assert_eq!(
            warnings[0].fix.as_ref().unwrap().replacement,
            "[link](./path/to/file.md)"
        );
    }

    #[test]
    fn test_autolink_not_affected() {
        // Autolinks use <> syntax and are different from inline links
        let rule = MD062LinkDestinationWhitespace::new();
        let content = "<https://example.com>";
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();
        assert!(warnings.is_empty());
    }
}
