use crate::lint_context::LintContext;
use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use pulldown_cmark::LinkType;

/// Whether `c` is whitespace between a link's parentheses and its destination.
///
/// CommonMark allows spaces, tabs and line endings there. Every other Unicode
/// space (U+00A0, U+2003, ...) is part of the destination, so it is neither
/// reported nor stripped.
fn is_destination_space(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\n' | '\r')
}

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

    /// Returns true when the character at `byte_index` is preceded by an odd
    /// number of backslashes, which makes it escaped rather than structural.
    ///
    /// Callers scanning a whole string track the escape state as they go
    /// instead: the backwards walk here costs the length of the backslash run
    /// in front of the index, so calling it per character is quadratic.
    fn is_escaped(text: &str, byte_index: usize) -> bool {
        text.as_bytes()[..byte_index]
            .iter()
            .rev()
            .take_while(|&&byte| byte == b'\\')
            .count()
            % 2
            != 0
    }

    /// The link source with the blockquote markers of its continuation lines removed.
    ///
    /// The raw text of a link spanning lines carries the `>` markers of the lines
    /// after the first. They belong to the container, not to the destination, so
    /// they are dropped before the destination is read. Lines outside a blockquote
    /// (list item indentation, lazy continuation) are kept as they are; that
    /// indentation is whitespace and is trimmed with the rest.
    fn without_quote_markers<'a>(ctx: &LintContext, line: usize, raw: &'a str) -> std::borrow::Cow<'a, str> {
        use crate::utils::blockquote::parse_blockquote_prefix;
        use std::borrow::Cow;

        if !raw.contains('\n') {
            return Cow::Borrowed(raw);
        }
        let mut stripped = String::with_capacity(raw.len());
        let mut changed = false;
        for (index, segment) in raw.split_inclusive('\n').enumerate() {
            let in_quote = index > 0
                && ctx
                    .lines
                    .get(line - 1 + index)
                    .is_some_and(|info| info.blockquote.is_some());
            match parse_blockquote_prefix(segment).filter(|_| in_quote) {
                Some(parsed) => {
                    stripped.push_str(parsed.content);
                    changed = true;
                }
                None => stripped.push_str(segment),
            }
        }
        if changed {
            Cow::Owned(stripped)
        } else {
            Cow::Borrowed(raw)
        }
    }

    /// The fix for a link whose source is `raw`, when it has one.
    ///
    /// A fix computed from marker-free text replaces the whole link, so it can only
    /// be used when it is a single line: a destination or title that still spans
    /// lines would need its markers put back.
    ///
    /// `target` is the destination and title the parser read from `raw`. When quote
    /// markers were dropped, the fix is kept only if parsing it gives back exactly
    /// that target: a `>` that opens no quote (for instance one indented four
    /// spaces) is destination text, and dropping it would change the URL.
    fn fix_for(
        &self,
        ctx: &LintContext,
        line: usize,
        raw: &str,
        target: (&str, Option<&str>),
    ) -> Option<(WhitespaceIssue, String)> {
        // The label keeps its line breaks and markers; only what follows `](` is rewritten.
        let (dest_start, _, _) = self.extract_destination_info(raw)?;
        let (label, tail) = raw.split_at(dest_start);
        let label_lines = label.matches('\n').count();
        let tail = Self::without_quote_markers(ctx, line + label_lines, tail);
        let markers_dropped = matches!(tail, std::borrow::Cow::Owned(_));
        let source = format!("{label}{tail}");
        let (_, _, raw_dest) = self.extract_destination_info(&source)?;
        let issue = self.check_destination_whitespace(raw_dest)?;
        let fixed = self.create_fix(&source)?;
        if markers_dropped && fixed[dest_start..].contains(['\n', '\r']) {
            return None;
        }
        if markers_dropped && !Self::parses_to_target(&format!("[x]({}", &fixed[dest_start..]), target) {
            return None;
        }
        Some((issue, fixed))
    }

    /// Whether `link` (a label followed by the destination part) is a single inline link with the given destination and title.
    fn parses_to_target(link: &str, (url, title): (&str, Option<&str>)) -> bool {
        use pulldown_cmark::{Event, Parser, Tag};

        let mut found = Parser::new(link).filter_map(|event| match event {
            Event::Start(Tag::Link {
                dest_url, title: found, ..
            }) => Some((dest_url, found)),
            _ => None,
        });
        match (found.next(), found.next()) {
            (Some((dest, found)), None) => dest.as_ref() == url && (found.as_ref() == title.unwrap_or("")),
            _ => false,
        }
    }

    /// Extract the destination portion from a link's raw text
    /// Returns (dest_start_offset, dest_end_offset, raw_dest) relative to link start
    fn extract_destination_info<'a>(&self, raw_link: &'a str) -> Option<(usize, usize, &'a str)> {
        // Find the opening parenthesis for the destination
        // Handle nested brackets in link text: [text [nested]](url)
        let mut bracket_depth = 0;
        let mut paren_start = None;

        let mut escaped = false;
        for (i, c) in raw_link.char_indices() {
            if escaped {
                escaped = false;
                continue;
            }

            match c {
                '\\' => escaped = true,
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

        let dest_content_start = paren_start + 1; // After '('
        let dest_content_end = raw_link.len().checked_sub(1)?;
        if raw_link.as_bytes().get(dest_content_end) != Some(&b')')
            || Self::is_escaped(raw_link, dest_content_end)
            || dest_content_end < dest_content_start
        {
            return None;
        }

        // The parser already bounded raw_link at the matching closing
        // parenthesis. Only retain the conservative unmatched-angle check that
        // prevents fixes when the destination itself is not safe to interpret.
        let dest_content = &raw_link[dest_content_start..dest_content_end];
        let (url, _) = Self::split_url_and_title(dest_content);
        let mut in_angle_brackets = false;
        let mut escaped = false;
        for c in url.chars() {
            if escaped {
                escaped = false;
                continue;
            }
            match c {
                '\\' => escaped = true,
                '<' if !in_angle_brackets => in_angle_brackets = true,
                '>' if in_angle_brackets => in_angle_brackets = false,
                _ => {}
            }
        }
        if in_angle_brackets {
            return None;
        }

        Some((dest_content_start, dest_content_end, dest_content))
    }

    /// Split destination content into (url, optional title).
    ///
    /// A title is recognized only when it is a *balanced* quoted string
    /// (`"..."` or `'...'`) that forms the trailing token of the destination
    /// (only whitespace may follow the closing quote). A lone or unbalanced quote
    /// is treated as part of the URL. This prevents the fix from turning a stray
    /// quote into an unterminated title that, when the document is re-parsed,
    /// swallows the following line - which would make the fix non-idempotent and
    /// drop content.
    fn split_url_and_title(dest: &str) -> (&str, Option<&str>) {
        // Quote characters are ASCII, so byte and char indices coincide here.
        if let Some(q_start) = dest.find(['"', '\'']) {
            // A title is separated from the destination by whitespace, and a destination
            // has to come first. A quote glued to the URL (`x"t"`, or after a U+00A0 that
            // is not whitespace to CommonMark) is part of the URL.
            let before = &dest[..q_start];
            if !before.ends_with(is_destination_space) || before.trim_matches(is_destination_space).is_empty() {
                return (dest, None);
            }
            let quote = dest.as_bytes()[q_start] as char;
            if let Some(rel_close) = dest[q_start + 1..].find(quote) {
                let close = q_start + 1 + rel_close;
                if dest[close + 1..].trim_matches(is_destination_space).is_empty() {
                    return (&dest[..q_start], Some(&dest[q_start..=close]));
                }
            }
        }
        (dest, None)
    }

    /// Check if destination has leading/trailing whitespace
    /// Returns the type of whitespace issue found, if any
    fn check_destination_whitespace(&self, full_dest: &str) -> Option<WhitespaceIssue> {
        if full_dest.is_empty() {
            return None;
        }

        let first_char = full_dest.chars().next();
        let last_char = full_dest.chars().last();

        let has_leading = first_char.is_some_and(is_destination_space);

        // Check for trailing whitespace - either at the end or before a title
        let has_trailing = if last_char.is_some_and(is_destination_space) {
            true
        } else {
            let (url_portion, title) = Self::split_url_and_title(full_dest);
            title.is_some() && url_portion.ends_with(is_destination_space)
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

        // Split into URL and optional title. Only a balanced quoted title is
        // recognized; a stray quote stays part of the URL (see split_url_and_title).
        let (url_raw, title_raw) = Self::split_url_and_title(full_dest_content);
        let url_part = url_raw.trim_matches(is_destination_space);
        let title_part = title_raw.map(|title| title.trim_matches(is_destination_space));

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
    fn native_format_policy(&self) -> Option<crate::format_proof::NativeFormatPolicy> {
        Some(crate::format_proof::NativeFormatPolicy::for_rule::<Self>(62))
    }

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
        for link in ctx.links() {
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
            if let Some((issue, fixed)) = self.fix_for(ctx, link.line, raw_link, (&link.url, link.title.as_deref())) {
                warnings.push(LintWarning {
                    rule_name: Some(self.name().to_string()),
                    line: link.line,
                    column: link.start_col + 1,
                    end_line: link.end_line,
                    end_column: link.end_col + 1,
                    message: issue.message(false),
                    severity: Severity::Warning,
                    fix: Some(Fix::new(link.byte_offset..link.byte_end, fixed)),
                });
            }
        }

        // Process images
        for image in ctx.images() {
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
            if let Some((issue, fixed_link)) =
                self.fix_for(ctx, image.line, link_portion, (&image.url, image.title.as_deref()))
            {
                let fixed = format!("!{fixed_link}");
                warnings.push(LintWarning {
                    rule_name: Some(self.name().to_string()),
                    line: image.line,
                    column: image.start_col + 1,
                    end_line: image.end_line,
                    end_column: image.end_col + 1,
                    message: issue.message(true),
                    severity: Severity::Warning,
                    fix: Some(Fix::new(image.byte_offset..image.byte_end, fixed)),
                });
            }
        }

        Ok(warnings)
    }

    fn fix(&self, ctx: &LintContext) -> Result<String, LintError> {
        let warnings = self.check(ctx)?;
        let warnings =
            crate::utils::fix_utils::filter_warnings_by_inline_config(warnings, ctx.inline_config(), self.name());

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
    fn test_fix_preserves_parentheses_inside_destination_and_title() {
        let rule = MD062LinkDestinationWhitespace::new();
        let cases = [
            (r"[link]( path\)suffix )", r"[link](path\)suffix)"),
            (r"[link]( path\(suffix )", r"[link](path\(suffix)"),
            (r#"[link]( path "title) rest" )"#, r#"[link](path "title) rest")"#),
            (r"[link]( path 'title) rest' )", r"[link](path 'title) rest')"),
            (r"![alt]( path\)suffix )", r"![alt](path\)suffix)"),
            (r"[a \] b]( path )", r"[a \] b](path)"),
        ];

        for (content, expected) in cases {
            let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
            let fixed = rule.fix(&ctx).unwrap();
            assert_eq!(fixed, expected, "unexpected fix for {content:?}");

            let fixed_ctx = LintContext::new(&fixed, MarkdownFlavor::Standard, None);
            assert_eq!(rule.fix(&fixed_ctx).unwrap(), fixed, "second fix changed {content:?}");
        }
    }

    #[test]
    fn test_fix_idempotent_with_unbalanced_quote() {
        // Regression: a lone quote inside a destination must not be treated as a
        // title. Previously the fix inserted a space before the stray quote
        // (`![](X" )` -> `![](X ")`), which re-parsed as an unterminated title
        // that swallowed the following line, making the fix non-idempotent and
        // dropping content. The character is U+2A700 (4 bytes in UTF-8) to also
        // guard the byte/char offset handling.
        let rule = MD062LinkDestinationWhitespace::new();
        let content = "![](\u{2a700}\" )\n![](\")";

        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        let once = rule.fix(&ctx).unwrap();

        let ctx2 = LintContext::new(&once, MarkdownFlavor::Standard, None);
        let twice = rule.fix(&ctx2).unwrap();

        assert_eq!(once, twice, "MD062 fix must be idempotent for unbalanced quotes");
        assert_eq!(
            once.lines().count(),
            2,
            "fix must not drop the second image line, got: {once:?}"
        );
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
    fn test_unmatched_angle_bracket_in_destination() {
        // When `<` inside the destination masks the closing `)`, the rule
        // should not produce a warning or fix, since it cannot reliably
        // determine the destination boundaries.
        let rule = MD062LinkDestinationWhitespace::new();
        let content = "[](  \"<)";
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();
        assert!(
            warnings.is_empty(),
            "Should not warn when closing paren is masked by angle bracket"
        );

        // Verify idempotency: fix should not modify unparseable links
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, content);
    }

    #[test]
    fn test_unicode_whitespace_in_destination() {
        // Unicode whitespace (EN QUAD U+2000) in link destination
        let rule = MD062LinkDestinationWhitespace::new();
        let content = "[](\u{2000}\"<)";
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();
        assert!(
            warnings.is_empty(),
            "Should not warn when angle bracket masks closing paren"
        );

        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, content, "Fix must be idempotent for unparseable links");
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

    fn md062_fix(content: &str) -> (usize, String) {
        let rule = MD062LinkDestinationWhitespace::new();
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        (rule.check(&ctx).unwrap().len(), rule.fix(&ctx).unwrap())
    }

    #[test]
    fn test_non_commonmark_whitespace_is_destination_content() {
        // NBSP, EM SPACE and IDEOGRAPHIC SPACE are not CommonMark whitespace, so
        // each is part of the destination and must neither warn nor be stripped.
        for ws in ['\u{a0}', '\u{2003}', '\u{3000}'] {
            for source in [
                format!("[a]({ws}x)\n"),
                format!("[a](x{ws})\n"),
                format!("[a]({ws}x{ws})\n"),
                format!("[a](x{ws}\"t\")\n"),
                format!("![a]({ws}x)\n"),
            ] {
                let (warnings, fixed) = md062_fix(&source);
                assert_eq!(warnings, 0, "{source:?} must not warn");
                assert_eq!(fixed, source, "{source:?} must be unchanged");
            }
        }
    }

    #[test]
    fn test_ascii_whitespace_around_non_commonmark_whitespace() {
        // Only the ASCII space and tab are stripped, the NBSP inside the
        // destination stays.
        let (warnings, fixed) = md062_fix("[a]( \u{a0}x\t)\n");
        assert_eq!(warnings, 1);
        assert_eq!(fixed, "[a](\u{a0}x)\n");
    }

    #[test]
    fn test_blockquote_multiline_destination_is_fixed_without_the_marker() {
        // The `>` on a continuation line belongs to the blockquote. The fix joins the
        // destination onto one line and the marker never becomes the destination.
        for (source, expected) in [
            ("> [k](\n> )\n", "> [k]()\n"),
            ("> [k](\n> url)\n", "> [k](url)\n"),
            ("> [k](url\n> )\n", "> [k](url)\n"),
            ("> [k](\n> x\n> )\n", "> [k](x)\n"),
            ("> [k](\n> x\n> \"t\")\n", "> [k](x \"t\")\n"),
            (">> [k](\n>> )\n", ">> [k]()\n"),
            ("> > [k](\n> > url\n> > )\n", "> > [k](url)\n"),
            ("> - [k](\n>   )\n", "> - [k]()\n"),
            ("- > [k](\n  > )\n", "- > [k]()\n"),
            ("> ![k](\n> )\n", "> ![k]()\n"),
            ("> [k](\n)\n", "> [k]()\n"),
        ] {
            let (warnings, fixed) = md062_fix(source);
            assert_eq!(warnings, 1, "{source:?} must warn");
            assert_eq!(fixed, expected, "{source:?}");
            assert_eq!(md062_fix(expected).0, 0, "{expected:?} must be clean");
        }
    }

    #[test]
    fn test_blockquote_multiline_label_keeps_its_markers() {
        // Only the destination is rewritten: the label's line break and marker stay.
        for (source, expected) in [
            ("> [multi\n> line]( url )\n", "> [multi\n> line](url)\n"),
            ("> ![multi\n> line]( url )\n", "> ![multi\n> line](url)\n"),
            (
                "> > [multi\n> > line](\n> > url\n> > )\n",
                "> > [multi\n> > line](url)\n",
            ),
            ("- > [multi\n  > line]( url )\n", "- > [multi\n  > line](url)\n"),
        ] {
            let (warnings, fixed) = md062_fix(source);
            assert_eq!(warnings, 1, "{source:?} must warn");
            assert_eq!(fixed, expected, "{source:?}");
            assert_eq!(md062_fix(expected).0, 0, "{expected:?} must be clean");
        }
    }

    #[test]
    fn test_blockquote_title_spanning_lines_is_left_alone() {
        // Joining the title would need its continuation line's marker re-inserted, so
        // there is no warning the fix could resolve.
        let source = "> [k](x \"a\n> b\" )\n";
        let (warnings, fixed) = md062_fix(source);
        assert_eq!(warnings, 0);
        assert_eq!(fixed, source);
    }

    #[test]
    fn test_blockquote_single_line_destination_is_still_fixed() {
        let (warnings, fixed) = md062_fix("> [a]( x )\n>\n> [b](  y  \"t\" )\n");
        assert_eq!(warnings, 2);
        assert_eq!(fixed, "> [a](x)\n>\n> [b](y \"t\")\n");
    }

    #[test]
    fn test_list_item_multiline_destination_is_still_fixed() {
        // Indentation-only containers have no marker to leak into the destination.
        let (warnings, fixed) = md062_fix("- [k](\n  )\n");
        assert_eq!(warnings, 1);
        assert_eq!(fixed, "- [k]()\n");
        let (warnings, fixed) = md062_fix("- [k](\n  url\n  )\n");
        assert_eq!(warnings, 1);
        assert_eq!(fixed, "- [k](url)\n");
    }

    #[test]
    fn test_quote_glued_to_the_destination_is_not_a_title() {
        // A title needs whitespace before it. `x"t"` and `x<NBSP>"t"` are URLs, so
        // splitting a title off would change the destination.
        for source in ["[a](x\"t\" )\n", "[a](x\u{a0}\"t\" )\n", "[a]( x'a' )\n"] {
            let (_, fixed) = md062_fix(source);
            assert!(
                !fixed.contains(" \"t\")") && !fixed.contains(" 'a')"),
                "{source:?} gained a title: {fixed:?}"
            );
        }
        assert_eq!(md062_fix("[a](x\"t\" )\n").1, "[a](x\"t\")\n");
        // Positive control: a separated title is still a title.
        assert_eq!(md062_fix("[a](x  \"t\" )\n").1, "[a](x \"t\")\n");
    }

    #[test]
    fn test_indented_angle_bracket_is_destination_text_not_a_quote_marker() {
        // Four spaces of indentation keep `>` from opening a quote, so it is part of
        // the destination and must survive any fix.
        for source in ["[a](\n    >url)\n", "![a](\n    >url)\n", "[a](\n    >url\n    )\n"] {
            let (_, fixed) = md062_fix(source);
            assert!(
                fixed.contains(">url") || fixed == source,
                "{source:?} lost the > of its destination: {fixed:?}"
            );
        }
        // Positive control: a real marker is still dropped.
        assert_eq!(md062_fix("> [a](\n> url)\n").1, "> [a](url)\n");
    }
}
