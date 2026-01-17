/// Rule MD034: No unformatted URLs
///
/// See [docs/md034.md](../../docs/md034.md) for full documentation, configuration, and examples.
use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::utils::range_utils::{LineIndex, calculate_url_range};
use crate::utils::regex_cache::{
    EMAIL_PATTERN, URL_IPV6_STR, URL_QUICK_CHECK_STR, URL_STANDARD_STR, URL_WWW_STR, XMPP_URI_STR,
    get_cached_fancy_regex, get_cached_regex,
};

use crate::filtered_lines::FilteredLinesExt;
use crate::lint_context::LintContext;

// MD034-specific patterns for markdown constructs
// Core URL patterns (URL_QUICK_CHECK_STR, URL_STANDARD_STR, etc.) are imported from regex_cache
const CUSTOM_PROTOCOL_PATTERN_STR: &str = r#"(?:grpc|ws|wss|ssh|git|svn|file|data|javascript|vscode|chrome|about|slack|discord|matrix|irc|redis|mongodb|postgresql|mysql|kafka|nats|amqp|mqtt|custom|app|api|service)://"#;
const MARKDOWN_LINK_PATTERN_STR: &str = r#"\[(?:[^\[\]]|\[[^\]]*\])*\]\(([^)\s]+)(?:\s+(?:\"[^\"]*\"|\'[^\']*\'))?\)"#;
const MARKDOWN_EMPTY_LINK_PATTERN_STR: &str = r#"\[(?:[^\[\]]|\[[^\]]*\])*\]\(\)"#;
const MARKDOWN_EMPTY_REF_PATTERN_STR: &str = r#"\[(?:[^\[\]]|\[[^\]]*\])*\]\[\]"#;
// Pattern for links in angle brackets - excludes HTTP(S), FTP(S), XMPP URIs, and emails
const ANGLE_LINK_PATTERN_STR: &str =
    r#"<((?:https?|ftps?)://(?:\[[0-9a-fA-F:]+(?:%[a-zA-Z0-9]+)?\]|[^>]+)|xmpp:[^>]+|[^@\s]+@[^@\s]+\.[^@\s>]+)>"#;
const BADGE_LINK_LINE_STR: &str = r#"^\s*\[!\[[^\]]*\]\([^)]*\)\]\([^)]*\)\s*$"#;
const MARKDOWN_IMAGE_PATTERN_STR: &str = r#"!\s*\[([^\]]*)\]\s*\(([^)\s]+)(?:\s+(?:\"[^\"]*\"|\'[^\']*\'))?\)"#;
// Reference definition pattern - matches [label]: URL with optional title
const REFERENCE_DEF_RE_STR: &str = r"^\s*\[[^\]]+\]:\s*(?:<|(?:https?|ftps?)://)";
const MULTILINE_LINK_CONTINUATION_STR: &str = r#"^[^\[]*\]\(.*\)"#;
// Pattern to match shortcut/collapsed reference links: [text] or [text][]
const SHORTCUT_REF_PATTERN_STR: &str = r#"\[([^\[\]]+)\](?!\s*[\[(])"#;

/// Reusable buffers for check_line to reduce allocations
#[derive(Default)]
struct LineCheckBuffers {
    markdown_link_ranges: Vec<(usize, usize)>,
    image_ranges: Vec<(usize, usize)>,
    urls_found: Vec<(usize, usize, String)>,
}

#[derive(Default, Clone)]
pub struct MD034NoBareUrls;

impl MD034NoBareUrls {
    #[inline]
    pub fn should_skip_content(&self, content: &str) -> bool {
        // Skip if content has no URLs, XMPP URIs, or email addresses
        // Fast byte scanning for common URL/email/xmpp indicators
        let bytes = content.as_bytes();
        let has_colon = bytes.contains(&b':');
        let has_at = bytes.contains(&b'@');
        let has_www = content.contains("www.");
        !has_colon && !has_at && !has_www
    }

    /// Remove trailing punctuation that is likely sentence punctuation, not part of the URL
    fn trim_trailing_punctuation<'a>(&self, url: &'a str) -> &'a str {
        let mut trimmed = url;

        // Check for balanced parentheses - if we have unmatched closing parens, they're likely punctuation
        let open_parens = url.chars().filter(|&c| c == '(').count();
        let close_parens = url.chars().filter(|&c| c == ')').count();

        if close_parens > open_parens {
            // Find the last balanced closing paren position
            let mut balance = 0;
            let mut last_balanced_pos = url.len();

            for (byte_idx, c) in url.char_indices() {
                if c == '(' {
                    balance += 1;
                } else if c == ')' {
                    balance -= 1;
                    if balance < 0 {
                        // Found an unmatched closing paren
                        last_balanced_pos = byte_idx;
                        break;
                    }
                }
            }

            trimmed = &trimmed[..last_balanced_pos];
        }

        // Trim specific punctuation only if not followed by more URL-like chars
        while let Some(last_char) = trimmed.chars().last() {
            if matches!(last_char, '.' | ',' | ';' | ':' | '!' | '?') {
                // Check if this looks like it could be part of the URL
                // For ':' specifically, keep it if followed by digits (port number)
                if last_char == ':' && trimmed.len() > 1 {
                    // Don't trim
                    break;
                }
                trimmed = &trimmed[..trimmed.len() - 1];
            } else {
                break;
            }
        }

        trimmed
    }

    /// Check if line is inside a reference definition
    fn is_reference_definition(&self, line: &str) -> bool {
        get_cached_regex(REFERENCE_DEF_RE_STR)
            .map(|re| re.is_match(line))
            .unwrap_or(false)
    }

    fn check_line(
        &self,
        line: &str,
        ctx: &LintContext,
        line_number: usize,
        code_spans: &[crate::lint_context::CodeSpan],
        buffers: &mut LineCheckBuffers,
        line_index: &LineIndex,
    ) -> Vec<LintWarning> {
        let mut warnings = Vec::new();

        // Skip reference definitions
        if self.is_reference_definition(line) {
            return warnings;
        }

        // Skip lines inside HTML blocks - URLs in HTML attributes should not be linted
        if ctx.line_info(line_number).is_some_and(|info| info.in_html_block) {
            return warnings;
        }

        // Skip lines that are continuations of multiline markdown links
        // Pattern: text](url) without a leading [
        if let Ok(re) = get_cached_regex(MULTILINE_LINK_CONTINUATION_STR)
            && re.is_match(line)
        {
            return warnings;
        }

        // Quick check - does this line potentially have a URL or email?
        let has_quick_check = get_cached_regex(URL_QUICK_CHECK_STR)
            .map(|re| re.is_match(line))
            .unwrap_or(false);
        let has_www = line.contains("www.");
        let has_at = line.contains('@');

        if !has_quick_check && !has_at && !has_www {
            return warnings;
        }

        // Clear and reuse buffers instead of allocating new ones
        buffers.markdown_link_ranges.clear();
        if let Ok(re) = get_cached_regex(MARKDOWN_LINK_PATTERN_STR) {
            for cap in re.captures_iter(line) {
                if let Some(mat) = cap.get(0) {
                    buffers.markdown_link_ranges.push((mat.start(), mat.end()));
                }
            }
        }

        // Also include empty link patterns like [text]() and [text][]
        if let Ok(re) = get_cached_regex(MARKDOWN_EMPTY_LINK_PATTERN_STR) {
            for mat in re.find_iter(line) {
                buffers.markdown_link_ranges.push((mat.start(), mat.end()));
            }
        }

        if let Ok(re) = get_cached_regex(MARKDOWN_EMPTY_REF_PATTERN_STR) {
            for mat in re.find_iter(line) {
                buffers.markdown_link_ranges.push((mat.start(), mat.end()));
            }
        }

        // Also exclude shortcut reference links like [URL] - even if no definition exists,
        // the brackets indicate user intent to use markdown formatting
        // Uses fancy_regex for negative lookahead support
        if let Ok(re) = get_cached_fancy_regex(SHORTCUT_REF_PATTERN_STR) {
            for mat in re.find_iter(line).flatten() {
                buffers.markdown_link_ranges.push((mat.start(), mat.end()));
            }
        }

        if let Ok(re) = get_cached_regex(ANGLE_LINK_PATTERN_STR) {
            for cap in re.captures_iter(line) {
                if let Some(mat) = cap.get(0) {
                    buffers.markdown_link_ranges.push((mat.start(), mat.end()));
                }
            }
        }

        // Find all markdown images for exclusion
        buffers.image_ranges.clear();
        if let Ok(re) = get_cached_regex(MARKDOWN_IMAGE_PATTERN_STR) {
            for cap in re.captures_iter(line) {
                if let Some(mat) = cap.get(0) {
                    buffers.image_ranges.push((mat.start(), mat.end()));
                }
            }
        }

        // Check if this line contains only a badge link (common pattern)
        let is_badge_line = get_cached_regex(BADGE_LINK_LINE_STR)
            .map(|re| re.is_match(line))
            .unwrap_or(false);

        if is_badge_line {
            return warnings;
        }

        // Find bare URLs
        buffers.urls_found.clear();

        // First, find IPv6 URLs (they need special handling)
        if let Ok(re) = get_cached_regex(URL_IPV6_STR) {
            for mat in re.find_iter(line) {
                let url_str = mat.as_str();
                buffers.urls_found.push((mat.start(), mat.end(), url_str.to_string()));
            }
        }

        // Then find regular URLs
        if let Ok(re) = get_cached_regex(URL_STANDARD_STR) {
            for mat in re.find_iter(line) {
                let url_str = mat.as_str();

                // Skip if it's an IPv6 URL (already handled)
                if url_str.contains("://[") {
                    continue;
                }

                // Skip malformed IPv6-like URLs
                // Check for IPv6-like patterns that are malformed
                if let Some(host_start) = url_str.find("://") {
                    let after_protocol = &url_str[host_start + 3..];
                    // If it looks like IPv6 (has :: or multiple :) but no brackets, skip if followed by ]
                    if after_protocol.contains("::") || after_protocol.chars().filter(|&c| c == ':').count() > 1 {
                        // Check if the next character after our match is ]
                        if let Some(char_after) = line.chars().nth(mat.end())
                            && char_after == ']'
                        {
                            // This is likely a malformed IPv6 URL like "https://::1]:8080"
                            continue;
                        }
                    }
                }

                buffers.urls_found.push((mat.start(), mat.end(), url_str.to_string()));
            }
        }

        // Find www URLs without protocol (e.g., www.example.com)
        if let Ok(re) = get_cached_regex(URL_WWW_STR) {
            for mat in re.find_iter(line) {
                let url_str = mat.as_str();
                let start_pos = mat.start();
                let end_pos = mat.end();

                // Skip if preceded by / or @ (likely part of a full URL)
                if start_pos > 0 {
                    let prev_char = line.as_bytes().get(start_pos - 1).copied();
                    if prev_char == Some(b'/') || prev_char == Some(b'@') {
                        continue;
                    }
                }

                // Skip if inside angle brackets (autolink syntax like <www.example.com>)
                if start_pos > 0 && end_pos < line.len() {
                    let prev_char = line.as_bytes().get(start_pos - 1).copied();
                    let next_char = line.as_bytes().get(end_pos).copied();
                    if prev_char == Some(b'<') && next_char == Some(b'>') {
                        continue;
                    }
                }

                buffers.urls_found.push((start_pos, end_pos, url_str.to_string()));
            }
        }

        // Find XMPP URIs (GFM extended autolinks: xmpp:user@domain/resource)
        if let Ok(re) = get_cached_regex(XMPP_URI_STR) {
            for mat in re.find_iter(line) {
                let uri_str = mat.as_str();
                let start_pos = mat.start();
                let end_pos = mat.end();

                // Skip if inside angle brackets (already properly formatted: <xmpp:user@domain>)
                if start_pos > 0 && end_pos < line.len() {
                    let prev_char = line.as_bytes().get(start_pos - 1).copied();
                    let next_char = line.as_bytes().get(end_pos).copied();
                    if prev_char == Some(b'<') && next_char == Some(b'>') {
                        continue;
                    }
                }

                buffers.urls_found.push((start_pos, end_pos, uri_str.to_string()));
            }
        }

        // Process found URLs
        for &(start, _end, ref url_str) in buffers.urls_found.iter() {
            // Skip custom protocols
            if get_cached_regex(CUSTOM_PROTOCOL_PATTERN_STR)
                .map(|re| re.is_match(url_str))
                .unwrap_or(false)
            {
                continue;
            }

            // Check if this URL is inside a markdown link, angle bracket, or image
            // We check if the URL starts within a construct, not if it's entirely contained.
            // This handles cases where URL detection may include trailing characters
            // that extend past the construct boundary (e.g., parentheses).
            let mut is_inside_construct = false;
            for &(link_start, link_end) in buffers.markdown_link_ranges.iter() {
                if start >= link_start && start < link_end {
                    is_inside_construct = true;
                    break;
                }
            }

            for &(img_start, img_end) in buffers.image_ranges.iter() {
                if start >= img_start && start < img_end {
                    is_inside_construct = true;
                    break;
                }
            }

            if is_inside_construct {
                continue;
            }

            // Calculate absolute byte position for context-aware checks
            let line_start_byte = line_index.get_line_start_byte(line_number).unwrap_or(0);
            let absolute_pos = line_start_byte + start;

            // Check if URL is inside an HTML tag (handles multiline tags correctly)
            if ctx.is_in_html_tag(absolute_pos) {
                continue;
            }

            // Check if we're inside an HTML comment
            if ctx.is_in_html_comment(absolute_pos) {
                continue;
            }

            // Check if we're inside a Hugo/Quarto shortcode
            if ctx.is_in_shortcode(absolute_pos) {
                continue;
            }

            // Clean up the URL by removing trailing punctuation
            let trimmed_url = self.trim_trailing_punctuation(url_str);

            // Only report if we have a valid URL after trimming
            if !trimmed_url.is_empty() && trimmed_url != "//" {
                let trimmed_len = trimmed_url.len();
                let (start_line, start_col, end_line, end_col) =
                    calculate_url_range(line_number, line, start, trimmed_len);

                // For www URLs without protocol, add https:// prefix in the fix
                let replacement = if trimmed_url.starts_with("www.") {
                    format!("<https://{trimmed_url}>")
                } else {
                    format!("<{trimmed_url}>")
                };

                warnings.push(LintWarning {
                    rule_name: Some("MD034".to_string()),
                    line: start_line,
                    column: start_col,
                    end_line,
                    end_column: end_col,
                    message: format!("URL without angle brackets or link formatting: '{trimmed_url}'"),
                    severity: Severity::Warning,
                    fix: Some(Fix {
                        range: {
                            let line_start_byte = line_index.get_line_start_byte(line_number).unwrap_or(0);
                            (line_start_byte + start)..(line_start_byte + start + trimmed_len)
                        },
                        replacement,
                    }),
                });
            }
        }

        // Check for bare email addresses
        for cap in EMAIL_PATTERN.captures_iter(line) {
            if let Some(mat) = cap.get(0) {
                let email = mat.as_str();
                let start = mat.start();
                let end = mat.end();

                // Skip if email is part of an XMPP URI (xmpp:user@domain)
                // Check character boundary to avoid panics with multi-byte UTF-8
                if start >= 5 && line.is_char_boundary(start - 5) && &line[start - 5..start] == "xmpp:" {
                    continue;
                }

                // Check if email is inside angle brackets or markdown link
                let mut is_inside_construct = false;
                for &(link_start, link_end) in buffers.markdown_link_ranges.iter() {
                    if start >= link_start && end <= link_end {
                        is_inside_construct = true;
                        break;
                    }
                }

                if !is_inside_construct {
                    // Calculate absolute byte position for context-aware checks
                    let line_start_byte = line_index.get_line_start_byte(line_number).unwrap_or(0);
                    let absolute_pos = line_start_byte + start;

                    // Check if email is inside an HTML tag (handles multiline tags)
                    if ctx.is_in_html_tag(absolute_pos) {
                        continue;
                    }

                    // Check if email is inside a code span
                    let is_in_code_span = code_spans
                        .iter()
                        .any(|span| span.line == line_number && start >= span.start_col && start < span.end_col);

                    if !is_in_code_span {
                        let email_len = end - start;
                        let (start_line, start_col, end_line, end_col) =
                            calculate_url_range(line_number, line, start, email_len);

                        warnings.push(LintWarning {
                            rule_name: Some("MD034".to_string()),
                            line: start_line,
                            column: start_col,
                            end_line,
                            end_column: end_col,
                            message: format!("Email address without angle brackets or link formatting: '{email}'"),
                            severity: Severity::Warning,
                            fix: Some(Fix {
                                range: (line_start_byte + start)..(line_start_byte + end),
                                replacement: format!("<{email}>"),
                            }),
                        });
                    }
                }
            }
        }

        warnings
    }
}

impl Rule for MD034NoBareUrls {
    #[inline]
    fn name(&self) -> &'static str {
        "MD034"
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn from_config(_config: &crate::config::Config) -> Box<dyn Rule>
    where
        Self: Sized,
    {
        Box::new(MD034NoBareUrls)
    }

    #[inline]
    fn category(&self) -> RuleCategory {
        RuleCategory::Link
    }

    fn should_skip(&self, ctx: &crate::lint_context::LintContext) -> bool {
        !ctx.likely_has_links_or_images() && self.should_skip_content(ctx.content)
    }

    #[inline]
    fn description(&self) -> &'static str {
        "No bare URLs - wrap URLs in angle brackets"
    }

    fn check(&self, ctx: &LintContext) -> LintResult {
        let mut warnings = Vec::new();
        let content = ctx.content;

        // Quick skip for content without URLs
        if self.should_skip_content(content) {
            return Ok(warnings);
        }

        // Create LineIndex for correct byte position calculations across all line ending types
        let line_index = &ctx.line_index;

        // Get code spans for exclusion
        let code_spans = ctx.code_spans();

        // Allocate reusable buffers once instead of per-line to reduce allocations
        let mut buffers = LineCheckBuffers::default();

        // Iterate over content lines, automatically skipping front matter and code blocks
        // This uses the filtered iterator API which centralizes the skip logic
        for line in ctx.filtered_lines().skip_front_matter().skip_code_blocks() {
            let mut line_warnings =
                self.check_line(line.content, ctx, line.line_num, &code_spans, &mut buffers, line_index);

            // Filter out warnings that are inside code spans
            line_warnings.retain(|warning| {
                // Check if the URL is inside a code span
                !code_spans.iter().any(|span| {
                    span.line == warning.line &&
                    warning.column > 0 && // column is 1-indexed
                    (warning.column - 1) >= span.start_col &&
                    (warning.column - 1) < span.end_col
                })
            });

            // Filter out warnings where the URL is inside a parsed link
            // This handles cases like [text]( https://url ) where the URL has leading whitespace
            // pulldown-cmark correctly parses these as valid links even though our regex misses them
            line_warnings.retain(|warning| {
                if let Some(fix) = &warning.fix {
                    // Check if the fix range falls inside any parsed link's byte range
                    !ctx.links
                        .iter()
                        .any(|link| fix.range.start >= link.byte_offset && fix.range.end <= link.byte_end)
                } else {
                    true
                }
            });

            warnings.extend(line_warnings);
        }

        Ok(warnings)
    }

    fn fix(&self, ctx: &LintContext) -> Result<String, LintError> {
        let mut content = ctx.content.to_string();
        let mut warnings = self.check(ctx)?;

        // Sort warnings by position to ensure consistent fix application
        warnings.sort_by_key(|w| w.fix.as_ref().map(|f| f.range.start).unwrap_or(0));

        // Apply fixes in reverse order to maintain positions
        for warning in warnings.iter().rev() {
            if let Some(fix) = &warning.fix {
                let start = fix.range.start;
                let end = fix.range.end;
                content.replace_range(start..end, &fix.replacement);
            }
        }

        Ok(content)
    }
}
