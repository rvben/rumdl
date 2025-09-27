/// Rule MD034: No unformatted URLs
///
/// See [docs/md034.md](../../docs/md034.md) for full documentation, configuration, and examples.
use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::utils::range_utils::calculate_url_range;
use crate::utils::regex_cache::{EMAIL_PATTERN, get_cached_regex};

use crate::lint_context::LintContext;

// URL detection patterns
const URL_QUICK_CHECK_STR: &str = r#"(?:https?|ftps?)://|@"#;
const CUSTOM_PROTOCOL_PATTERN_STR: &str = r#"(?:grpc|ws|wss|ssh|git|svn|file|data|javascript|vscode|chrome|about|slack|discord|matrix|irc|redis|mongodb|postgresql|mysql|kafka|nats|amqp|mqtt|custom|app|api|service)://"#;
const MARKDOWN_LINK_PATTERN_STR: &str = r#"\[(?:[^\[\]]|\[[^\]]*\])*\]\(([^)\s]+)(?:\s+(?:\"[^\"]*\"|\'[^\']*\'))?\)"#;
const ANGLE_LINK_PATTERN_STR: &str =
    r#"<((?:https?|ftps?)://(?:\[[0-9a-fA-F:]+(?:%[a-zA-Z0-9]+)?\]|[^>]+)|[^@\s]+@[^@\s]+\.[^@\s>]+)>"#;
const BADGE_LINK_LINE_STR: &str = r#"^\s*\[!\[[^\]]*\]\([^)]*\)\]\([^)]*\)\s*$"#;
const MARKDOWN_IMAGE_PATTERN_STR: &str = r#"!\s*\[([^\]]*)\]\s*\(([^)\s]+)(?:\s+(?:\"[^\"]*\"|\'[^\']*\'))?\)"#;
const SIMPLE_URL_REGEX_STR: &str = r#"(https?|ftps?)://(?:\[[0-9a-fA-F:%.]+\](?::\d+)?|[^\s<>\[\]()\\'\"`:\]]+(?::\d+)?)(?:/[^\s<>\[\]()\\'\"`]*)?(?:\?[^\s<>\[\]()\\'\"`]*)?(?:#[^\s<>\[\]()\\'\"`]*)?"#;
const IPV6_URL_REGEX_STR: &str = r#"(https?|ftps?)://\[[0-9a-fA-F:%.\-a-zA-Z]+\](?::\d+)?(?:/[^\s<>\[\]()\\'\"`]*)?(?:\?[^\s<>\[\]()\\'\"`]*)?(?:#[^\s<>\[\]()\\'\"`]*)?"#;
const REFERENCE_DEF_RE_STR: &str = r"^\s*\[[^\]]+\]:\s*(?:https?|ftps?)://\S+$";
const HTML_COMMENT_PATTERN_STR: &str = r#"<!--[\s\S]*?-->"#;

#[derive(Default, Clone)]
pub struct MD034NoBareUrls;

impl MD034NoBareUrls {
    #[inline]
    pub fn should_skip(&self, content: &str) -> bool {
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

    fn check_line(&self, line: &str, content: &str, line_number: usize) -> Vec<LintWarning> {
        let mut warnings = Vec::new();

        // Skip reference definitions
        if self.is_reference_definition(line) {
            return warnings;
        }

        // Quick check - does this line potentially have a URL or email?
        if let Ok(re) = get_cached_regex(URL_QUICK_CHECK_STR)
            && !re.is_match(line)
            && !line.contains('@')
        {
            return warnings;
        }

        // Find all markdown links and angle bracket links for exclusion
        let mut markdown_link_ranges = Vec::new();
        if let Ok(re) = get_cached_regex(MARKDOWN_LINK_PATTERN_STR) {
            for cap in re.captures_iter(line) {
                if let Some(mat) = cap.get(0) {
                    markdown_link_ranges.push((mat.start(), mat.end()));
                }
            }
        }

        if let Ok(re) = get_cached_regex(ANGLE_LINK_PATTERN_STR) {
            for cap in re.captures_iter(line) {
                if let Some(mat) = cap.get(0) {
                    markdown_link_ranges.push((mat.start(), mat.end()));
                }
            }
        }

        // Find all markdown images for exclusion
        let mut image_ranges = Vec::new();
        if let Ok(re) = get_cached_regex(MARKDOWN_IMAGE_PATTERN_STR) {
            for cap in re.captures_iter(line) {
                if let Some(mat) = cap.get(0) {
                    image_ranges.push((mat.start(), mat.end()));
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
        let mut urls_found = Vec::new();

        // First, find IPv6 URLs (they need special handling)
        if let Ok(re) = get_cached_regex(IPV6_URL_REGEX_STR) {
            for mat in re.find_iter(line) {
                let url_str = mat.as_str();
                urls_found.push((mat.start(), mat.end(), url_str.to_string()));
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

                urls_found.push((mat.start(), mat.end(), url_str.to_string()));
            }
        }

        // Process found URLs
        for (start, end, url_str) in urls_found {
            // Skip custom protocols
            if get_cached_regex(CUSTOM_PROTOCOL_PATTERN_STR)
                .map(|re| re.is_match(&url_str))
                .unwrap_or(false)
            {
                continue;
            }

            // Check if this URL is inside a markdown link, angle bracket, or image
            let mut is_inside_construct = false;
            for &(link_start, link_end) in &markdown_link_ranges {
                if start >= link_start && end <= link_end {
                    is_inside_construct = true;
                    break;
                }
            }

            for &(img_start, img_end) in &image_ranges {
                if start >= img_start && end <= img_end {
                    is_inside_construct = true;
                    break;
                }
            }

            if is_inside_construct {
                continue;
            }

            // Check if we're inside an HTML comment
            let absolute_pos = content
                .lines()
                .take(line_number - 1)
                .map(|l| l.len() + 1)
                .sum::<usize>()
                + start;
            if self.is_in_html_comment(content, absolute_pos) {
                continue;
            }

            // Clean up the URL by removing trailing punctuation
            let trimmed_url = self.trim_trailing_punctuation(&url_str);

            // Only report if we have a valid URL after trimming
            if !trimmed_url.is_empty() && trimmed_url != "//" {
                let trimmed_end = start + trimmed_url.len();
                let (start_line, start_col, end_line, end_col) =
                    calculate_url_range(line_number, line, start, trimmed_end);

                warnings.push(LintWarning {
                    rule_name: Some("MD034"),
                    line: start_line,
                    column: start_col,
                    end_line,
                    end_column: end_col,
                    message: format!("Bare URL '{trimmed_url}' should be formatted as a link"),
                    severity: Severity::Warning,
                    fix: Some(Fix {
                        range: {
                            let line_start_byte = content
                                .lines()
                                .take(line_number - 1)
                                .map(|l| l.len() + 1)
                                .sum::<usize>();
                            (line_start_byte + start)..(line_start_byte + trimmed_end)
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
                for &(link_start, link_end) in &markdown_link_ranges {
                    if start >= link_start && end <= link_end {
                        is_inside_construct = true;
                        break;
                    }
                }

                if !is_inside_construct {
                    let (start_line, start_col, end_line, end_col) = calculate_url_range(line_number, line, start, end);

                    warnings.push(LintWarning {
                        rule_name: Some("MD034"),
                        line: start_line,
                        column: start_col,
                        end_line,
                        end_column: end_col,
                        message: format!("Bare email address '{email}' should be formatted as a link"),
                        severity: Severity::Warning,
                        fix: Some(Fix {
                            range: {
                                let line_start_byte = content
                                    .lines()
                                    .take(line_number - 1)
                                    .map(|l| l.len() + 1)
                                    .sum::<usize>();
                                (line_start_byte + start)..(line_start_byte + end)
                            },
                            replacement: format!("<{email}>"),
                        }),
                    });
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

    #[inline]
    fn description(&self) -> &'static str {
        "No bare URLs - wrap URLs in angle brackets"
    }

    fn check(&self, ctx: &LintContext) -> LintResult {
        let mut warnings = Vec::new();
        let content = ctx.content;

        // Quick skip for content without URLs
        if self.should_skip(content) {
            return Ok(warnings);
        }

        // Check line by line
        for (line_num, line) in content.lines().enumerate() {
            let line_warnings = self.check_line(line, content, line_num + 1);
            warnings.extend(line_warnings);
        }

        Ok(warnings)
    }

    fn fix(&self, ctx: &LintContext) -> Result<String, LintError> {
        let mut content = ctx.content.to_string();
        let warnings = self.check(ctx)?;

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
