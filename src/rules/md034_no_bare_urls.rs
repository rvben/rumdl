/// Rule MD034: No unformatted URLs
///
/// See [docs/md034.md](../../docs/md034.md) for full documentation, configuration, and examples.
use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::utils::range_utils::{LineIndex, calculate_url_range};
use crate::utils::regex_cache::{EMAIL_PATTERN, get_cached_regex};

use crate::lint_context::LintContext;

// URL detection patterns
const URL_QUICK_CHECK_STR: &str = r#"(?:https?|ftps?)://|@"#;
const CUSTOM_PROTOCOL_PATTERN_STR: &str = r#"(?:grpc|ws|wss|ssh|git|svn|file|data|javascript|vscode|chrome|about|slack|discord|matrix|irc|redis|mongodb|postgresql|mysql|kafka|nats|amqp|mqtt|custom|app|api|service)://"#;
const MARKDOWN_LINK_PATTERN_STR: &str = r#"\[(?:[^\[\]]|\[[^\]]*\])*\]\(([^)\s]+)(?:\s+(?:\"[^\"]*\"|\'[^\']*\'))?\)"#;
const MARKDOWN_EMPTY_LINK_PATTERN_STR: &str = r#"\[(?:[^\[\]]|\[[^\]]*\])*\]\(\)"#;
const MARKDOWN_EMPTY_REF_PATTERN_STR: &str = r#"\[(?:[^\[\]]|\[[^\]]*\])*\]\[\]"#;
const ANGLE_LINK_PATTERN_STR: &str =
    r#"<((?:https?|ftps?)://(?:\[[0-9a-fA-F:]+(?:%[a-zA-Z0-9]+)?\]|[^>]+)|[^@\s]+@[^@\s]+\.[^@\s>]+)>"#;
const BADGE_LINK_LINE_STR: &str = r#"^\s*\[!\[[^\]]*\]\([^)]*\)\]\([^)]*\)\s*$"#;
const MARKDOWN_IMAGE_PATTERN_STR: &str = r#"!\s*\[([^\]]*)\]\s*\(([^)\s]+)(?:\s+(?:\"[^\"]*\"|\'[^\']*\'))?\)"#;
const SIMPLE_URL_REGEX_STR: &str = r#"(https?|ftps?)://(?:\[[0-9a-fA-F:%.]+\](?::\d+)?|[^\s<>\[\]()\\'\"`\]]+)(?:/[^\s<>\[\]()\\'\"`]*)?(?:\?[^\s<>\[\]()\\'\"`]*)?(?:#[^\s<>\[\]()\\'\"`]*)?"#;
const IPV6_URL_REGEX_STR: &str = r#"(https?|ftps?)://\[[0-9a-fA-F:%.\-a-zA-Z]+\](?::\d+)?(?:/[^\s<>\[\]()\\'\"`]*)?(?:\?[^\s<>\[\]()\\'\"`]*)?(?:#[^\s<>\[\]()\\'\"`]*)?"#;
const REFERENCE_DEF_RE_STR: &str = r"^\s*\[[^\]]+\]:\s*(?:https?|ftps?)://\S+$";
const HTML_COMMENT_PATTERN_STR: &str = r#"<!--[\s\S]*?-->"#;
const HTML_TAG_PATTERN_STR: &str = r#"<[^>]*>"#;
const MULTILINE_LINK_CONTINUATION_STR: &str = r#"^[^\[]*\]\(.*\)"#;

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
        // Skip if content has no URLs and no email addresses
        // Fast byte scanning for common URL/email indicators
        let bytes = content.as_bytes();
        !bytes.contains(&b':') && !bytes.contains(&b'@')
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

            for (i, c) in url.chars().enumerate() {
                if c == '(' {
                    balance += 1;
                } else if c == ')' {
                    balance -= 1;
                    if balance < 0 {
                        // Found an unmatched closing paren
                        last_balanced_pos = i;
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

    /// Check if we're inside an HTML comment
    fn is_in_html_comment(&self, content: &str, pos: usize) -> bool {
        // Find all HTML comments in the content
        if let Ok(re) = get_cached_regex(HTML_COMMENT_PATTERN_STR) {
            for mat in re.find_iter(content) {
                if pos >= mat.start() && pos < mat.end() {
                    return true;
                }
            }
        }
        false
    }

    /// Check if a position in a line is inside an HTML tag
    fn is_in_html_tag(&self, line: &str, pos: usize) -> bool {
        // Find all HTML tags in the line
        if let Ok(re) = get_cached_regex(HTML_TAG_PATTERN_STR) {
            for mat in re.find_iter(line) {
                if pos >= mat.start() && pos < mat.end() {
                    return true;
                }
            }
        }
        false
    }

    fn check_line(
        &self,
        line: &str,
        content: &str,
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

        // Skip lines that are continuations of multiline markdown links
        // Pattern: text](url) without a leading [
        if let Ok(re) = get_cached_regex(MULTILINE_LINK_CONTINUATION_STR)
            && re.is_match(line)
        {
            return warnings;
        }

        // Quick check - does this line potentially have a URL or email?
        if let Ok(re) = get_cached_regex(URL_QUICK_CHECK_STR)
            && !re.is_match(line)
            && !line.contains('@')
        {
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
        if let Ok(re) = get_cached_regex(IPV6_URL_REGEX_STR) {
            for mat in re.find_iter(line) {
                let url_str = mat.as_str();
                buffers.urls_found.push((mat.start(), mat.end(), url_str.to_string()));
            }
        }

        // Then find regular URLs
        if let Ok(re) = get_cached_regex(SIMPLE_URL_REGEX_STR) {
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

        // Process found URLs
        for &(start, end, ref url_str) in buffers.urls_found.iter() {
            // Skip custom protocols
            if get_cached_regex(CUSTOM_PROTOCOL_PATTERN_STR)
                .map(|re| re.is_match(url_str))
                .unwrap_or(false)
            {
                continue;
            }

            // Check if this URL is inside a markdown link, angle bracket, or image
            let mut is_inside_construct = false;
            for &(link_start, link_end) in buffers.markdown_link_ranges.iter() {
                if start >= link_start && end <= link_end {
                    is_inside_construct = true;
                    break;
                }
            }

            for &(img_start, img_end) in buffers.image_ranges.iter() {
                if start >= img_start && end <= img_end {
                    is_inside_construct = true;
                    break;
                }
            }

            if is_inside_construct {
                continue;
            }

            // Check if URL is inside an HTML tag
            if self.is_in_html_tag(line, start) {
                continue;
            }

            // Check if we're inside an HTML comment
            let line_start_byte = line_index.get_line_start_byte(line_number).unwrap_or(0);
            let absolute_pos = line_start_byte + start;
            if self.is_in_html_comment(content, absolute_pos) {
                continue;
            }

            // Clean up the URL by removing trailing punctuation
            let trimmed_url = self.trim_trailing_punctuation(url_str);

            // Only report if we have a valid URL after trimming
            if !trimmed_url.is_empty() && trimmed_url != "//" {
                let trimmed_len = trimmed_url.len();
                let (start_line, start_col, end_line, end_col) =
                    calculate_url_range(line_number, line, start, trimmed_len);

                warnings.push(LintWarning {
                    rule_name: Some("MD034"),
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
                        replacement: format!("<{trimmed_url}>"),
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

                // Check if email is inside angle brackets or markdown link
                let mut is_inside_construct = false;
                for &(link_start, link_end) in buffers.markdown_link_ranges.iter() {
                    if start >= link_start && end <= link_end {
                        is_inside_construct = true;
                        break;
                    }
                }

                if !is_inside_construct {
                    // Check if email is inside an HTML tag
                    if self.is_in_html_tag(line, start) {
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
                            rule_name: Some("MD034"),
                            line: start_line,
                            column: start_col,
                            end_line,
                            end_column: end_col,
                            message: format!("Email address without angle brackets or link formatting: '{email}'"),
                            severity: Severity::Warning,
                            fix: Some(Fix {
                                range: {
                                    let line_start_byte = line_index.get_line_start_byte(line_number).unwrap_or(0);
                                    (line_start_byte + start)..(line_start_byte + end)
                                },
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
        let line_index = LineIndex::new(content.to_string());

        // Get code spans for exclusion
        let code_spans = ctx.code_spans();

        // Allocate reusable buffers once instead of per-line to reduce allocations
        let mut buffers = LineCheckBuffers::default();

        // Check line by line
        for (line_num, line) in content.lines().enumerate() {
            // Skip lines inside code blocks
            if ctx.is_in_code_block(line_num + 1) {
                continue;
            }

            let mut line_warnings =
                self.check_line(line, content, line_num + 1, &code_spans, &mut buffers, &line_index);

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
