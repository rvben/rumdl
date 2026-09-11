/// Rule MD034: No unformatted URLs
///
/// See [docs/md034.md](../../docs/md034.md) for full documentation, configuration, and examples.
use std::sync::LazyLock;

use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::rule_config_serde::{RuleConfig, load_rule_config};
use crate::utils::range_utils::calculate_url_range;
use crate::utils::regex_cache::{
    EMAIL_PATTERN, URL_IPV6_REGEX, URL_QUICK_CHECK_REGEX, URL_STANDARD_REGEX, URL_WWW_REGEX, XMPP_URI_REGEX,
};

use crate::filtered_lines::FilteredLinesExt;
use crate::lint_context::LintContext;

// MD034-specific pre-compiled regex patterns for markdown constructs
static CUSTOM_PROTOCOL_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?:grpc|ws|wss|ssh|git|svn|file|data|javascript|vscode|chrome|about|slack|discord|matrix|irc|redis|mongodb|postgresql|mysql|kafka|nats|amqp|mqtt|custom|app|api|service)://"#).unwrap()
});
static MARKDOWN_LINK_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"\[(?:[^\[\]]|\[[^\]]*\])*\]\(([^)\s]+)(?:\s+(?:\"[^\"]*\"|\'[^\']*\'))?\)"#).unwrap()
});
static MARKDOWN_EMPTY_LINK_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"\[(?:[^\[\]]|\[[^\]]*\])*\]\(\)"#).unwrap());
static MARKDOWN_EMPTY_REF_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"\[(?:[^\[\]]|\[[^\]]*\])*\]\[\]"#).unwrap());
static ANGLE_LINK_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r#"<((?:https?|ftps?)://(?:\[[0-9a-fA-F:]+(?:%[a-zA-Z0-9]+)?\]|[^>]+)|xmpp:[^>]+|[^@\s]+@[^@\s]+\.[^@\s>]+)>"#,
    )
    .unwrap()
});
static BADGE_LINK_LINE_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"^\s*\[!\[[^\]]*\]\([^)]*\)\]\([^)]*\)\s*$"#).unwrap());
static MARKDOWN_IMAGE_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"!\s*\[([^\]]*)\]\s*\(([^)\s]+)(?:\s+(?:\"[^\"]*\"|\'[^\']*\'))?\)"#).unwrap());
static MULTILINE_LINK_CONTINUATION_REGEX: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#"^[^\[]*\]\(.*\)"#).unwrap());
static SHORTCUT_REF_REGEX: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#"\[([^\[\]]+)\]"#).unwrap());

/// Characters that are active inside a Markdown link text and must be escaped so
/// the text renders as the literal URL.
///
/// `{` and `}` are the MDX-specific members and the ones that matter most: an
/// unescaped `{b}` is evaluated as a JSX expression instead of shown, and a lone
/// brace is a hard MDX compile error, so omitting them would let the fix turn a
/// building document into one that no longer compiles. `*` and `&` are the other
/// two verified against the `@mdx-js/mdx` compiler: a `*` pair renders as emphasis
/// and `&amp;` decodes to `&`.
///
/// The remainder are escaped to keep the helper total rather than because a URL can
/// reach them today. ``< > [ ] \ ` `` all terminate the match in `URL_STANDARD_STR`
/// and `EMAIL_PATTERN`, so they cannot appear in captured text; `_` and `~` can, and
/// are inert under CommonMark alone but active under `remark-gfm`. Escaping is
/// always safe here (an escaped punctuation character renders as itself), so the
/// superset costs nothing and survives those patterns widening.
const MDX_LINK_TEXT_ESCAPES: [char; 12] = ['\\', '`', '*', '_', '{', '}', '[', ']', '<', '>', '~', '&'];

/// Escape a URL or email so it renders literally as the text of a Markdown link.
fn escape_mdx_link_text(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for ch in text.chars() {
        if MDX_LINK_TEXT_ESCAPES.contains(&ch) {
            escaped.push('\\');
        }
        escaped.push(ch);
    }
    escaped
}

/// Whether every parenthesis in the URL is matched.
///
/// A bare link destination may only contain balanced parentheses; an unmatched
/// `(` makes CommonMark reject the link entirely and emit no anchor at all.
fn has_balanced_parens(url: &str) -> bool {
    let mut depth: i32 = 0;
    for ch in url.chars() {
        match ch {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth < 0 {
                    return false;
                }
            }
            _ => {}
        }
    }
    depth == 0
}

/// Build the replacement for a bare URL or email under a JSX-carrying flavor.
///
/// `<` opens JSX in MDX, so the autolink form `<https://example.com>` is a parse
/// error and the "fixed" document stops compiling. The link form is what MDX's own
/// error message recommends and it renders identically to the autolink it replaces.
///
/// `trim_trailing_punctuation` already removes unmatched closing parens, so only
/// unmatched openers reach the destination here; those go in angle brackets, which
/// carry no balancing requirement. No URL this rule captures can contain `<`, `>`
/// or whitespace, so that wrapping is always safe.
fn jsx_safe_link(text: &str, destination: &str) -> String {
    let escaped = escape_mdx_link_text(text);
    if has_balanced_parens(destination) {
        format!("[{escaped}]({destination})")
    } else {
        format!("[{escaped}](<{destination}>)")
    }
}

/// What the source text immediately before a span would bind to if the span became a
/// `[text](destination)` link.
///
/// A link is self-contained only in isolation: its opening `[` binds leftwards. The
/// autolink form opens with `<` and binds to nothing, so both hazards below belong to
/// the link form alone and have to be handled where it is emitted.
enum LinkPrefix {
    /// Nothing before the span can bind to a `[`.
    Free,
    /// An active `!`, which would read the emitted link as an image instead.
    ActiveBang,
    /// An active `]`, which would read the emitted link text as that span's reference
    /// label, resolving the anchor against an unrelated definition and leaving the
    /// real destination behind as literal text.
    ActiveCloseBracket,
}

/// Classify the character immediately before `start` in `line`.
fn classify_link_prefix(line: &str, start: usize) -> LinkPrefix {
    let before = &line[..start];
    let Some(last) = before.chars().next_back() else {
        return LinkPrefix::Free;
    };
    if last != '!' && last != ']' {
        return LinkPrefix::Free;
    }

    // An odd run of backslashes escapes the character, leaving it literal text that
    // binds to nothing. An even run escapes only itself, so the character stays active.
    let preceding = &before[..before.len() - last.len_utf8()];
    if preceding.bytes().rev().take_while(|&b| b == b'\\').count() % 2 == 1 {
        return LinkPrefix::Free;
    }

    if last == '!' {
        LinkPrefix::ActiveBang
    } else {
        LinkPrefix::ActiveCloseBracket
    }
}

/// Build the JSX-flavor fix for the span at `start`, guarded against what precedes it.
///
/// Returns the byte offset within `line` that the replacement starts at, which is not
/// always `start`, together with the replacement. `None` means no replacement is safe
/// and the finding is reported without one.
fn jsx_fix(line: &str, start: usize, text: &str, destination: &str) -> Option<(usize, String)> {
    let link = jsx_safe_link(text, destination);
    match classify_link_prefix(line, start) {
        LinkPrefix::Free => Some((start, link)),
        // Absorb the `!` into the replacement and escape it, so it renders as itself
        // rather than opening an image. It is one byte, so the span grows by one.
        LinkPrefix::ActiveBang => Some((start - 1, format!("\\!{link}"))),
        // Escaping the `]` would break the span it closes, and no other spelling of the
        // link avoids the reference-label reading, so leave the text to the author.
        LinkPrefix::ActiveCloseBracket => None,
    }
}

/// Whether the address at `start` is already carrying a URI scheme, as in
/// `mailto:user@example.com` or `xmpp:user@example.com`.
///
/// `EMAIL_PATTERN` matches only the address part, so a schemed URI presents its tail as a
/// bare email. Wrapping that tail alone would produce `mailto:[user@example.com](...)`,
/// splitting the URI. GFM's autolink extension also declines to link an address whose
/// preceding character is anything but whitespace or one of `*_~(`, so an address behind a
/// scheme is not a link waiting to happen either way.
///
/// The scheme grammar is RFC 3986's: `ALPHA *( ALPHA / DIGIT / "+" / "-" / "." )`.
fn follows_uri_scheme(line: &str, start: usize) -> bool {
    let Some(before) = line[..start].strip_suffix(':') else {
        return false;
    };
    let scheme: &str = {
        let tail = before.len() - before.bytes().rev().take_while(|b| is_scheme_byte(*b)).count();
        &before[tail..]
    };
    scheme.bytes().next().is_some_and(|b| b.is_ascii_alphabetic())
}

/// Whether a byte may appear in a URI scheme.
fn is_scheme_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'+' | b'-' | b'.')
}

/// Reusable buffers for check_line to reduce allocations
#[derive(Default)]
struct LineCheckBuffers {
    markdown_link_ranges: Vec<(usize, usize)>,
    image_ranges: Vec<(usize, usize)>,
    urls_found: Vec<(usize, usize, String)>,
}

/// Configuration for MD034 (No bare URLs).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub struct MD034Config {
    /// Emit automatic fixes for bare URLs and email addresses.
    #[serde(default = "default_fix")]
    pub fix: bool,
}

const fn default_fix() -> bool {
    true
}

impl Default for MD034Config {
    fn default() -> Self {
        Self { fix: default_fix() }
    }
}

impl RuleConfig for MD034Config {
    const RULE_NAME: &'static str = "MD034";
}

#[derive(Default, Clone)]
pub struct MD034NoBareUrls {
    config: MD034Config,
}

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

    fn check_line(
        &self,
        line: &str,
        ctx: &LintContext,
        line_number: usize,
        code_spans: &[crate::lint_context::CodeSpan],
        buffers: &mut LineCheckBuffers,
    ) -> Vec<LintWarning> {
        let mut warnings = Vec::new();

        // Skip lines inside HTML blocks - URLs in HTML attributes should not be linted
        if ctx.line_info(line_number).is_some_and(|info| info.in_html_block) {
            return warnings;
        }

        // Skip lines that are continuations of multiline markdown links
        // Pattern: text](url) without a leading [
        if MULTILINE_LINK_CONTINUATION_REGEX.is_match(line) {
            return warnings;
        }

        // Quick check - does this line potentially have a URL or email?
        let has_quick_check = URL_QUICK_CHECK_REGEX.is_match(line);
        let has_www = line.contains("www.");
        let has_at = line.contains('@');

        if !has_quick_check && !has_at && !has_www {
            return warnings;
        }

        // Clear and reuse buffers instead of allocating new ones
        buffers.markdown_link_ranges.clear();
        buffers.image_ranges.clear();

        let has_bracket = line.contains('[');
        let has_angle = line.contains('<');
        let has_bang = line.contains('!');

        if has_bracket {
            for mat in MARKDOWN_LINK_REGEX.find_iter(line) {
                buffers.markdown_link_ranges.push((mat.start(), mat.end()));
            }

            // Also include empty link patterns like [text]() and [text][]
            for mat in MARKDOWN_EMPTY_LINK_REGEX.find_iter(line) {
                buffers.markdown_link_ranges.push((mat.start(), mat.end()));
            }

            for mat in MARKDOWN_EMPTY_REF_REGEX.find_iter(line) {
                buffers.markdown_link_ranges.push((mat.start(), mat.end()));
            }

            // Also exclude shortcut reference links like [URL]
            for mat in SHORTCUT_REF_REGEX.find_iter(line) {
                let end = mat.end();
                let next_non_ws = line[end..].bytes().find(|b| !b.is_ascii_whitespace());
                if next_non_ws == Some(b'(') || next_non_ws == Some(b'[') {
                    continue;
                }
                buffers.markdown_link_ranges.push((mat.start(), mat.end()));
            }

            // Check if this line contains only a badge link (common pattern)
            if has_bang && BADGE_LINK_LINE_REGEX.is_match(line) {
                return warnings;
            }
        }

        if has_angle {
            for mat in ANGLE_LINK_REGEX.find_iter(line) {
                buffers.markdown_link_ranges.push((mat.start(), mat.end()));
            }
        }

        // Find all markdown images for exclusion
        if has_bang && has_bracket {
            for mat in MARKDOWN_IMAGE_REGEX.find_iter(line) {
                buffers.image_ranges.push((mat.start(), mat.end()));
            }
        }

        // Find bare URLs
        buffers.urls_found.clear();

        // First, find IPv6 URLs (they need special handling)
        for mat in URL_IPV6_REGEX.find_iter(line) {
            let url_str = mat.as_str();
            buffers.urls_found.push((mat.start(), mat.end(), url_str.to_string()));
        }

        // Then find regular URLs
        for mat in URL_STANDARD_REGEX.find_iter(line) {
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
                    // Check if the next byte after our match is ] (ASCII, so byte check is safe)
                    if line.as_bytes().get(mat.end()) == Some(&b']') {
                        // This is likely a malformed IPv6 URL like "https://::1]:8080"
                        continue;
                    }
                }
            }

            buffers.urls_found.push((mat.start(), mat.end(), url_str.to_string()));
        }

        // Find www URLs without protocol (e.g., www.example.com)
        for mat in URL_WWW_REGEX.find_iter(line) {
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

        // Find XMPP URIs (GFM extended autolinks: xmpp:user@domain/resource)
        for mat in XMPP_URI_REGEX.find_iter(line) {
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

        // Process found URLs
        for &(start, _end, ref url_str) in &buffers.urls_found {
            // Skip custom protocols
            if CUSTOM_PROTOCOL_REGEX.is_match(url_str) {
                continue;
            }

            // Check if this URL is inside a markdown link, angle bracket, or image
            // We check if the URL starts within a construct, not if it's entirely contained.
            // This handles cases where URL detection may include trailing characters
            // that extend past the construct boundary (e.g., parentheses).
            // Linear scan is correct here because ranges can overlap/nest (e.g., [[1]](url))
            let is_inside_construct = buffers
                .markdown_link_ranges
                .iter()
                .any(|&(s, e)| start >= s && start < e)
                || buffers.image_ranges.iter().any(|&(s, e)| start >= s && start < e);

            if is_inside_construct {
                continue;
            }

            // Calculate absolute byte position for context-aware checks
            let line_start_byte = ctx.line_start_byte(line_number).unwrap_or(0);
            let absolute_pos = line_start_byte + start;

            // Check if URL is inside an HTML tag (handles multiline tags correctly)
            if ctx.is_in_html_tag(absolute_pos) {
                continue;
            }

            // Check if URL is a JSX component attribute value (e.g. `<Card href="..."/>`).
            // These are string props, not bare prose; wrapping them in angle brackets
            // would produce invalid JSX. No-op for non-JSX flavors.
            if ctx.is_in_jsx_component_tag(absolute_pos) {
                continue;
            }

            // Check if we're inside an HTML comment
            if ctx.is_in_html_comment(absolute_pos) || ctx.is_in_mdx_comment(absolute_pos) {
                continue;
            }

            // Check if we're inside a Hugo/Quarto shortcode
            if ctx.is_in_shortcode(absolute_pos) {
                continue;
            }

            // Skip URLs inside Pandoc line blocks (`| text`) or YAML metadata blocks.
            // Both constructs treat their content as literal/structured text where bare
            // URLs are intentional and should not be reformatted.
            if ctx.flavor.is_pandoc_compatible()
                && (ctx.is_in_line_block(absolute_pos) || ctx.is_in_pandoc_metadata(absolute_pos))
            {
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
                let destination = if trimmed_url.starts_with("www.") {
                    format!("https://{trimmed_url}")
                } else {
                    trimmed_url.to_string()
                };
                let line_start_byte = ctx.line_start_byte(line_number).unwrap_or(0);
                let span_end = line_start_byte + start + trimmed_len;
                let fix = self
                    .config
                    .fix
                    .then(|| {
                        if ctx.flavor.supports_jsx() {
                            jsx_fix(line, start, trimmed_url, &destination).map(|(fix_start, replacement)| {
                                Fix::new((line_start_byte + fix_start)..span_end, replacement)
                            })
                        } else {
                            Some(Fix::new(
                                (line_start_byte + start)..span_end,
                                format!("<{destination}>"),
                            ))
                        }
                    })
                    .flatten();

                warnings.push(LintWarning {
                    rule_name: Some("MD034".to_string()),
                    line: start_line,
                    column: start_col,
                    end_line,
                    end_column: end_col,
                    message: if ctx.flavor == crate::config::MarkdownFlavor::MDG {
                        format!(
                            "URL without link formatting: '{trimmed_url}' (angle brackets are Gherkin placeholder syntax; use explicit link formatting where appropriate, or disable MD034)"
                        )
                    } else {
                        format!("URL without angle brackets or link formatting: '{trimmed_url}'")
                    },
                    severity: Severity::Warning,
                    fix,
                });
            }
        }

        // Check for bare email addresses
        for cap in EMAIL_PATTERN.captures_iter(line) {
            if let Some(mat) = cap.get(0) {
                let email = mat.as_str();
                let start = mat.start();
                let end = mat.end();

                // Skip an address that is the tail of a schemed URI (xmpp:, mailto:, ...);
                // the scheme is outside the match, so wrapping the tail would split the URI.
                if follows_uri_scheme(line, start) {
                    continue;
                }

                // Check if email is inside angle brackets or markdown link
                let mut is_inside_construct = false;
                for &(link_start, link_end) in &buffers.markdown_link_ranges {
                    if start >= link_start && end <= link_end {
                        is_inside_construct = true;
                        break;
                    }
                }

                if !is_inside_construct {
                    // Calculate absolute byte position for context-aware checks
                    let line_start_byte = ctx.line_start_byte(line_number).unwrap_or(0);
                    let absolute_pos = line_start_byte + start;

                    // Check if email is inside an HTML tag (handles multiline tags)
                    if ctx.is_in_html_tag(absolute_pos) {
                        continue;
                    }

                    // Check if email is a JSX component attribute value (e.g.
                    // `<Contact email="..."/>`). No-op for non-JSX flavors.
                    if ctx.is_in_jsx_component_tag(absolute_pos) {
                        continue;
                    }

                    // Skip emails inside Pandoc line blocks or YAML metadata blocks.
                    if ctx.flavor.is_pandoc_compatible()
                        && (ctx.is_in_line_block(absolute_pos) || ctx.is_in_pandoc_metadata(absolute_pos))
                    {
                        continue;
                    }

                    // Check if email is inside a code span (byte offsets handle multi-line spans)
                    let is_in_code_span = code_spans
                        .iter()
                        .any(|span| absolute_pos >= span.byte_offset && absolute_pos < span.byte_end);

                    if !is_in_code_span {
                        let email_len = end - start;
                        let (start_line, start_col, end_line, end_col) =
                            calculate_url_range(line_number, line, start, email_len);

                        let fix = self
                            .config
                            .fix
                            .then(|| {
                                if ctx.flavor.supports_jsx() {
                                    jsx_fix(line, start, email, &format!("mailto:{email}")).map(
                                        |(fix_start, replacement)| {
                                            Fix::new(
                                                (line_start_byte + fix_start)..(line_start_byte + end),
                                                replacement,
                                            )
                                        },
                                    )
                                } else {
                                    Some(Fix::new(
                                        (line_start_byte + start)..(line_start_byte + end),
                                        format!("<{email}>"),
                                    ))
                                }
                            })
                            .flatten();

                        warnings.push(LintWarning {
                            rule_name: Some("MD034".to_string()),
                            line: start_line,
                            column: start_col,
                            end_line,
                            end_column: end_col,
                            message: if ctx.flavor == crate::config::MarkdownFlavor::MDG {
                                format!(
                                    "Email address without link formatting: '{email}' (angle brackets are Gherkin placeholder syntax; use explicit link formatting where appropriate, or disable MD034)"
                                )
                            } else {
                                format!("Email address without angle brackets or link formatting: '{email}'")
                            },
                            severity: Severity::Warning,
                            fix,
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

    fn from_config(config: &crate::config::Config) -> Box<dyn Rule>
    where
        Self: Sized,
    {
        Box::new(MD034NoBareUrls {
            config: load_rule_config(config),
        })
    }

    #[inline]
    fn category(&self) -> RuleCategory {
        RuleCategory::Link
    }

    fn skippable_by_category(&self) -> bool {
        // Bare email addresses and `xmpp:` URIs are MD034 findings, but the
        // document-wide Link prefilter deliberately recognizes only Markdown
        // links and common URL forms. Let MD034's own cheap `should_skip`
        // predicate decide so email/XMPP-only documents are still checked.
        false
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

        // Get code spans for exclusion
        let code_spans = ctx.code_spans();

        // Reference-definition lines are detected by rumdl's shared parser (which
        // understands blockquote-prefixed definitions and the full CommonMark
        // grammar), so their destination URLs are not flagged as bare URLs.
        let ref_def_lines: std::collections::HashSet<usize> =
            ctx.reference_definitions().iter().map(|def| def.line).collect();

        // Allocate reusable buffers once instead of per-line to reduce allocations
        let mut buffers = LineCheckBuffers::default();

        // Iterate over content lines, automatically skipping front matter, code blocks,
        // and Obsidian comments (when in Obsidian flavor)
        // This uses the filtered iterator API which centralizes the skip logic
        for line in ctx
            .filtered_lines()
            .skip_front_matter()
            .skip_code_blocks()
            .skip_jsx_expressions()
            .skip_mdx_comments()
            .skip_obsidian_comments()
        {
            // A gh-aw control directive is template syntax, not Markdown prose.
            // In particular, wrapping a runtime-import URL would corrupt the
            // directive by making the closing braces part of the autolink.
            if ctx.flavor == crate::config::MarkdownFlavor::GhAw && crate::utils::gh_aw::is_control_line(line.content) {
                continue;
            }

            // Skip MyST colon-fence directive openers (`:::{name} <arg>`). The text
            // after the directive name is an opaque argument (a URL, path, or label),
            // not markdown prose, so a bare URL there must not be wrapped in angle
            // brackets. Directive body lines are not openers, so they fall through to
            // `check_line` and are linted as usual.
            if ctx.is_myst_colon_directive_opener_line(line.line_num) {
                continue;
            }

            // Skip reference-definition lines (`[id]: url`, including inside blockquotes).
            if ref_def_lines.contains(&line.line_num) {
                continue;
            }

            let mut line_warnings = self.check_line(line.content, ctx, line.line_num, &code_spans, &mut buffers);

            // Filter out warnings that are inside code spans (handles multi-line spans via byte offsets)
            line_warnings.retain(|warning| {
                !code_spans.iter().any(|span| {
                    if let Some(fix) = &warning.fix {
                        // Byte-offset check handles both single-line and multi-line code spans
                        fix.range.start >= span.byte_offset && fix.range.start < span.byte_end
                    } else {
                        span.line == warning.line
                            && span.end_line == warning.line
                            && warning.column > 0
                            && (warning.column - 1) >= span.start_col
                            && (warning.column - 1) < span.end_col
                    }
                })
            });

            line_warnings.retain(|warning| {
                if let Some(fix) = &warning.fix {
                    // Check if the fix range falls inside any parsed link's byte range
                    !ctx.links().iter().any(|link| {
                        !(link.is_reference && link.url.is_empty())
                            && fix.range.start >= link.byte_offset
                            && fix.range.end <= link.byte_end
                    })
                } else {
                    true
                }
            });

            // Filter out warnings where the URL is inside an Obsidian comment (%%...%%)
            // This handles inline comments like: text %%https://hidden.com%% text
            line_warnings.retain(|warning| !ctx.is_position_in_obsidian_comment(warning.line, warning.column));

            warnings.extend(line_warnings);
        }

        // The generated ranges are needed by the filters above. Strip fixes only
        // after filtering: under MDG `<...>` is placeholder syntax, so the
        // standard automatic correction is not semantics-preserving.
        if ctx.flavor == crate::config::MarkdownFlavor::MDG {
            for warning in &mut warnings {
                warning.fix = None;
            }
        }

        Ok(warnings)
    }

    fn fix(&self, ctx: &LintContext) -> Result<String, LintError> {
        if !self.config.fix {
            return Ok(ctx.content.to_string());
        }
        let mut content = ctx.content.to_string();
        let warnings = self.check(ctx)?;
        let mut warnings =
            crate::utils::fix_utils::filter_warnings_by_inline_config(warnings, ctx.inline_config(), self.name());

        // Sort warnings by position to ensure consistent fix application
        warnings.sort_by_key(|w| w.fix.as_ref().map_or(0, |f| f.range.start));

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fix_can_be_disabled_without_suppressing_warnings() {
        let rule = MD034NoBareUrls {
            config: MD034Config { fix: false },
        };
        let content = "Visit https://example.com and email user@example.com.";
        let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);

        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 2);
        assert!(warnings.iter().all(|warning| warning.fix.is_none()));
        assert_eq!(rule.fix(&ctx).unwrap(), content);
    }

    #[test]
    fn test_shortcut_ref_at_end_of_line_no_trailing_chars() {
        let rule = MD034NoBareUrls::default();
        let content = "See [https://example.com]";
        let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "[URL] at end of line should be treated as shortcut ref: {result:?}"
        );
    }

    #[test]
    fn test_shortcut_ref_multiple_spaces_before_paren() {
        let rule = MD034NoBareUrls::default();
        let content = "[text]  (https://example.com)";
        let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        // [text]  (url) — the spaces between ] and ( mean this should be treated
        // as shortcut ref then bare parens, NOT a markdown link. URL may still be bare.
        // This test verifies consistent behavior with the FancyRegex that had (?!\s*[\[(])
        let _ = result; // Just verify no panic; the exact warning count depends on other rules
    }

    #[test]
    fn test_shortcut_ref_tab_before_bracket() {
        let rule = MD034NoBareUrls::default();
        let content = "[https://example.com]\t[other]";
        let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        // Tab between ] and [ does not form a full reference link in Markdown.
        // The first [URL] is a shortcut ref containing a bare URL, so MD034 warns.
        // This test verifies consistent behavior and no panic with tab characters.
        assert_eq!(
            result.len(),
            1,
            "Bare URL inside shortcut ref should be detected: {result:?}"
        );
    }

    #[test]
    fn test_shortcut_ref_followed_by_punctuation() {
        let rule = MD034NoBareUrls::default();
        let content = "[https://example.com], see also other things.";
        let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "[URL] followed by comma should be treated as shortcut ref: {result:?}"
        );
    }

    #[test]
    fn test_url_in_backticks_inside_mdx_component_not_flagged() {
        // Exact reproduction from issue #572: URL inside inline code within an MDX
        // component body must not be flagged. The same URL in backticks outside the
        // component is already handled correctly and serves as a control.
        let rule = MD034NoBareUrls::default();
        let content = "# Test\n\nControl: `https://rumdl.example.com/` is fine here.\n\n<ParamField path=\"--stuff\">\n  This URL `https://rumdl.example.com/` must not be flagged.\n</ParamField>\n";
        let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::MDX, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "URL in backticks inside MDX component must not be flagged: {result:?}"
        );
    }

    #[test]
    fn test_bare_url_inside_mdx_component_still_flagged() {
        // A bare URL (not in backticks) inside an MDX component body must still be flagged.
        // This ensures the fix for issue #572 only suppresses properly code-spanned URLs.
        let rule = MD034NoBareUrls::default();
        let content =
            "# Test\n\n<ParamField path=\"--stuff\">\n  Visit https://rumdl.example.com/ for details.\n</ParamField>\n";
        let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::MDX, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(
            result.len(),
            1,
            "Bare URL in MDX component body must still be flagged: {result:?}"
        );
    }

    #[test]
    fn test_url_in_backticks_inside_nested_mdx_component_not_flagged() {
        // Nested MDX components must also respect code spans.
        let rule = MD034NoBareUrls::default();
        let content = "<Outer>\n  <Inner>\n    Check `https://example.com/` here.\n  </Inner>\n</Outer>\n";
        let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::MDX, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "URL in backticks inside nested MDX component must not be flagged: {result:?}"
        );
    }

    /// Issue #678: a URL inside a fenced code block that is nested within a JSX/MDX
    /// component (e.g. `<Steps><Step>`) is code, not bare prose. It must not be
    /// flagged, and `fix` must not rewrite it (which would corrupt the command).
    #[test]
    fn test_url_in_fenced_code_block_inside_jsx_not_flagged() {
        let rule = MD034NoBareUrls::default();
        let content = "# Title\n\n<Steps>\n  <Step title=\"Send a request\">\n```bash\ncurl https://example.com/api\n```\n  </Step>\n</Steps>\n";
        let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::MDX, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "URL in a fenced code block nested in a JSX component must not be flagged: {result:?}"
        );
    }

    /// The same code block must be left byte-for-byte intact by `fix` (no
    /// `<https://...>` rewrite that breaks a copy-pasteable command).
    #[test]
    fn test_fix_does_not_rewrite_url_in_fenced_code_block_inside_jsx() {
        let rule = MD034NoBareUrls::default();
        let content = "# Title\n\n<Steps>\n  <Step title=\"Send a request\">\n```bash\ncurl https://example.com/api\n```\n  </Step>\n</Steps>\n";
        let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::MDX, None);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(
            fixed, content,
            "fix must not rewrite a URL inside a JSX-nested fenced code block"
        );
    }

    /// Control: a bare URL in the JSX *body* (outside any fence) is genuine prose
    /// and must still be flagged, so the fence exemption is not over-broad.
    #[test]
    fn test_bare_url_in_jsx_body_outside_fence_still_flagged() {
        let rule = MD034NoBareUrls::default();
        let content = "# Title\n\n<Steps>\n  <Step title=\"Send a request\">\n  Visit https://example.com/api now.\n  </Step>\n</Steps>\n";
        let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::MDX, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(
            result.len(),
            1,
            "A bare URL in the JSX body (not in a fence) must still be flagged: {result:?}"
        );
    }

    /// A `<!--` inside a fenced code block is literal, not a comment opener, so it
    /// must not pair with a later `-->` to form a comment range that masks a real
    /// bare URL between them (the code-block counterpart to the code-span fix).
    #[test]
    fn test_bare_url_not_masked_by_comment_delimiter_in_code_block() {
        let rule = MD034NoBareUrls::default();
        let content =
            "# T\n\n```text\n<!-- literal opener, not a comment\n```\n\nhttps://example.com should be flagged\n\n-->\n";
        let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1, "the bare URL must still be flagged: {result:?}");
        assert!(
            result[0].message.contains("example.com"),
            "the flagged URL must be the bare one: {result:?}"
        );
    }

    /// Only *fenced* code blocks suppress `<!--`/`-->` as literal. A real HTML
    /// comment indented inside a MkDocs admonition (which pulldown-cmark
    /// misclassifies as an indented code block) must still be recognized as a
    /// comment, so its bare URL stays skipped.
    #[test]
    fn test_bare_url_in_indented_comment_in_admonition_still_skipped() {
        let rule = MD034NoBareUrls::default();
        let content = "# T\n\n!!! note\n    Some text.\n\n    <!--\n    https://example.com\n    -->\n";
        let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::MkDocs, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "URL inside an indented HTML comment in an admonition must not be flagged: {result:?}"
        );
    }

    /// Issue #649: a URL that is a JSX component attribute value (e.g. `href="..."`)
    /// is a string prop, not bare prose. Wrapping it in angle brackets produces
    /// invalid JSX, so MD034 must not flag it under the MDX flavor.
    #[test]
    fn test_url_in_jsx_component_attribute_not_flagged() {
        let rule = MD034NoBareUrls::default();
        let content = "<Card title=\"Docs\" href=\"https://example.com/docs\" />\n";
        let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::MDX, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "URL in a JSX component attribute must not be flagged: {result:?}"
        );
    }

    /// The same exemption must apply when the JSX opening tag spans multiple lines.
    #[test]
    fn test_url_in_multiline_jsx_component_attribute_not_flagged() {
        let rule = MD034NoBareUrls::default();
        let content = "<Card\n  title=\"Docs\"\n  href=\"https://example.com/docs\"\n/>\n";
        let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::MDX, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "URL in a multi-line JSX component attribute must not be flagged: {result:?}"
        );
    }

    /// The exemption is surgical: a URL in the component's *attributes* is skipped,
    /// but a bare URL in the component's *body* is genuine prose and still flagged.
    #[test]
    fn test_jsx_attribute_url_skipped_but_body_url_flagged() {
        let rule = MD034NoBareUrls::default();
        let content = "<Card href=\"https://attr.example.com\">\n  Visit https://body.example.com now.\n</Card>\n";
        let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::MDX, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(
            result.len(),
            1,
            "Only the body URL must be flagged, not the attribute URL: {result:?}"
        );
        assert!(
            result[0].message.contains("body.example.com"),
            "The flagged URL must be the body one: {result:?}"
        );
    }

    /// The email path has the same JSX-attribute blind spot; an email used as a
    /// JSX component attribute value must not be flagged either.
    #[test]
    fn test_email_in_jsx_component_attribute_not_flagged() {
        let rule = MD034NoBareUrls::default();
        let content = "<Contact email=\"hello@example.com\" />\n";
        let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::MDX, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Email in a JSX component attribute must not be flagged: {result:?}"
        );
    }

    /// Control: under the Standard flavor `<Card .../>` is parsed as an HTML tag,
    /// so the attribute URL is already covered by the existing HTML-tag guard.
    /// This locks in that the two flavors agree.
    #[test]
    fn test_jsx_attribute_url_not_flagged_in_standard_flavor() {
        let rule = MD034NoBareUrls::default();
        let content = "<Card href=\"https://example.com/docs\" />\n";
        let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "URL in a tag attribute must not be flagged under Standard flavor either: {result:?}"
        );
    }

    /// URLs inside Pandoc line blocks (`| text`) must not be flagged as bare URLs.
    #[test]
    fn test_pandoc_skips_urls_in_line_blocks() {
        use crate::config::MarkdownFlavor;
        use crate::lint_context::LintContext;
        let rule = MD034NoBareUrls::default();
        let content = "| See https://example.com\n| For details\n";
        let ctx = LintContext::new(content, MarkdownFlavor::Pandoc, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "MD034 should skip URLs in Pandoc line blocks: {result:?}"
        );
    }

    /// URLs inside Pandoc YAML metadata blocks must not be flagged.
    #[test]
    fn test_pandoc_skips_urls_in_metadata() {
        use crate::config::MarkdownFlavor;
        use crate::lint_context::LintContext;
        let rule = MD034NoBareUrls::default();
        let content = "---\nhomepage: https://example.com\n---\n\nBody.\n";
        let ctx = LintContext::new(content, MarkdownFlavor::Pandoc, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "MD034 should skip URLs in Pandoc YAML metadata: {result:?}"
        );
    }

    /// Standard flavor must still flag bare URLs in lines starting with `|`
    /// (which are not interpreted as line blocks).
    #[test]
    fn test_standard_still_flags_urls_in_pipe_prefixed_lines() {
        use crate::config::MarkdownFlavor;
        use crate::lint_context::LintContext;
        let rule = MD034NoBareUrls::default();
        let content = "| See https://example.com\n";
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            !result.is_empty(),
            "MD034 should still flag URLs in pipe-prefixed lines under Standard flavor"
        );
    }

    #[test]
    fn test_url_in_backticks_after_fenced_code_block_inside_mdx_not_flagged() {
        // A fenced code block inside a JSX component must not misalign the code-span
        // offset map. The URL in backticks that appears *after* the code block must
        // still be recognised as being inside a code span.
        let rule = MD034NoBareUrls::default();
        let content = "\
<Component>
Some intro text.

```
example code here
```

Check `https://example.com/` here.
</Component>
";
        let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::MDX, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "URL in backticks after a fenced code block inside MDX must not be flagged: {result:?}"
        );
    }

    /// Issue #642: a URL given as the argument of a MyST colon-fence directive
    /// (`:::{name} <url>`) is the directive's opaque argument, not markdown prose,
    /// and must not be wrapped in angle brackets.
    #[test]
    fn test_myst_colon_directive_argument_url_not_flagged() {
        use crate::config::MarkdownFlavor;
        use crate::lint_context::LintContext;
        let rule = MD034NoBareUrls::default();
        let content = "\
:::{anywidget} https://cdn.jsdelivr.net/npm/repo-review-webapp@1.1.3/dist/repo-review-anywidget.mjs
{
  \"deps\": [\"repo-review~=1.1.0\"]
}
:::
";
        let ctx = LintContext::new(content, MarkdownFlavor::MyST, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "URL argument on a MyST colon directive opener must not be flagged: {result:?}"
        );
    }

    /// A nested MyST colon directive opener also carries an opaque argument.
    #[test]
    fn test_myst_nested_colon_directive_argument_url_not_flagged() {
        use crate::config::MarkdownFlavor;
        use crate::lint_context::LintContext;
        let rule = MD034NoBareUrls::default();
        let content = "\
::::{grid}
:::{card} https://example.com/card-target
Some caption.
:::
::::
";
        let ctx = LintContext::new(content, MarkdownFlavor::MyST, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "URL argument on a nested MyST colon directive opener must not be flagged: {result:?}"
        );
    }

    /// A bare URL in the *body* of a content directive (e.g. `{note}`) is genuine
    /// prose and must still be flagged. The opener exemption must not leak to the body.
    #[test]
    fn test_myst_directive_body_url_still_flagged() {
        use crate::config::MarkdownFlavor;
        use crate::lint_context::LintContext;
        let rule = MD034NoBareUrls::default();
        let content = "\
:::{note}
See https://example.com/docs for more details.
:::
";
        let ctx = LintContext::new(content, MarkdownFlavor::MyST, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(
            result.len(),
            1,
            "Bare URL in a MyST directive body must still be flagged: {result:?}"
        );
    }

    /// An unclosed colon directive (no terminating `:::`) still has its opener
    /// argument treated as opaque: the URL must not be flagged.
    #[test]
    fn test_myst_unclosed_colon_directive_argument_url_not_flagged() {
        use crate::config::MarkdownFlavor;
        use crate::lint_context::LintContext;
        let rule = MD034NoBareUrls::default();
        let content = "\
:::{anywidget} https://example.com/widget.mjs
Some trailing content with no closing fence.
";
        let ctx = LintContext::new(content, MarkdownFlavor::MyST, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "URL argument on an unclosed MyST colon directive opener must not be flagged: {result:?}"
        );
    }

    /// The colon-directive exemption is MyST-specific: under the Standard flavor a
    /// `:::{...}` line is ordinary text and a bare URL on it must still be flagged.
    #[test]
    fn test_colon_directive_url_flagged_in_standard_flavor() {
        use crate::config::MarkdownFlavor;
        use crate::lint_context::LintContext;
        let rule = MD034NoBareUrls::default();
        let content = ":::{anywidget} https://example.com/widget.mjs\n";
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(
            result.len(),
            1,
            "Under Standard flavor a bare URL on a `:::` line must still be flagged: {result:?}"
        );
    }

    #[test]
    fn test_md034_complex_link() {
        let rule = MD034NoBareUrls::default();

        // Case 1: Balanced brackets in code span.
        // We should flag the bare URL at the end, but NOT the one inside the link.
        let content = "Check [link `code [with brackets]` text](http://example.com) and see http://bare.com.\n";
        let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1, "Should flag exactly 1 URL (the bare one): {result:?}");
        assert!(result[0].message.contains("bare.com"));

        // Case 2: Unbalanced brackets in code span.
        // We should flag the bare URL at the end, but NOT the one inside the link.
        let content2 = "Check [link `code [` text](http://example.com) and see http://bare.com.\n";
        let ctx2 = crate::lint_context::LintContext::new(content2, crate::config::MarkdownFlavor::Standard, None);
        let result2 = rule.check(&ctx2).unwrap();
        assert_eq!(
            result2.len(),
            1,
            "Should flag exactly 1 URL (the bare one): {result2:?}"
        );
        assert!(result2[0].message.contains("bare.com"));
    }

    /// `<...>` is Gherkin placeholder syntax, substituted from `Examples` data. MD034
    /// still reports bare URLs under MDG, but withholds that unsafe automatic fix.
    #[test]
    fn test_mdg_reports_bare_urls_without_fixing_them() {
        let rule = MD034NoBareUrls::default();
        let content = "\
# Feature: Visit https://feature.example.com

Prose about https://prose.example.com for background.

## Scenario Outline: Open https://outline.example.com

* Given I go to https://step.example.com
  | site                          |
  | https://datatable.example.com |

> * Given I go to https://blockquoted.example.com

1. Given I go to https://ordered.example.com

| url                            |
| ------------------------------ |
| https://unindented.example.com |

### Examples:

  | url                          |
  | ---------------------------- |
  | https://examples.example.com |
";

        let standard_ctx =
            crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let standard_lines: Vec<usize> = rule.check(&standard_ctx).unwrap().iter().map(|w| w.line).collect();
        assert_eq!(
            standard_lines,
            vec![1, 3, 5, 7, 9, 11, 13, 17, 23],
            "Standard flavor flags every bare URL"
        );

        let mdg_ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::MDG, None);
        assert!(!rule.should_skip(&mdg_ctx), "MDG must still run the diagnostic");
        let mdg = rule.check(&mdg_ctx).unwrap();
        assert_eq!(mdg.iter().map(|w| w.line).collect::<Vec<_>>(), standard_lines);
        assert!(mdg.iter().all(|warning| warning.fix.is_none()));
        assert!(
            mdg.iter()
                .all(|warning| warning.message.contains("Gherkin placeholder"))
        );
        assert!(mdg.iter().all(|warning| warning.message.contains("disable MD034")));
        assert_eq!(rule.fix(&mdg_ctx).unwrap(), content, "MDG must rewrite nothing");
    }

    #[test]
    fn test_mdg_reports_bare_email_without_fixing_it() {
        let rule = MD034NoBareUrls::default();
        let content = "# Feature: Contact\n\n* Given I email user@example.com\n";
        let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::MDG, None);

        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].message.contains("Gherkin placeholder"));
        assert!(warnings[0].message.contains("disable MD034"));
        assert!(warnings[0].fix.is_none());
        assert_eq!(rule.fix(&ctx).unwrap(), content);
    }

    /// The exemption is confined to MDG: every other flavor still rewrites the same
    /// document, at every position.
    #[test]
    fn test_mdg_exemption_does_not_affect_other_flavors() {
        let rule = MD034NoBareUrls::default();
        let content = "\
# Feature: Visit https://feature.example.com

Prose about https://prose.example.com for background.

## Scenario Outline: Open https://outline.example.com

* Given I go to https://step.example.com
  | site                          |
  | https://datatable.example.com |

### Examples:

  | url                          |
  | ---------------------------- |
  | https://examples.example.com |
";
        let expected = "\
# Feature: Visit <https://feature.example.com>

Prose about <https://prose.example.com> for background.

## Scenario Outline: Open <https://outline.example.com>

* Given I go to <https://step.example.com>
  | site                          |
  | <https://datatable.example.com> |

### Examples:

  | url                          |
  | ---------------------------- |
  | <https://examples.example.com> |
";

        for flavor in [
            crate::config::MarkdownFlavor::Standard,
            crate::config::MarkdownFlavor::MkDocs,
            crate::config::MarkdownFlavor::MyST,
        ] {
            let ctx = crate::lint_context::LintContext::new(content, flavor, None);
            assert!(!rule.should_skip(&ctx), "{flavor:?} must still run the rule");
            assert_eq!(rule.check(&ctx).unwrap().len(), 6, "{flavor:?} must flag all six URLs");

            let fixed = rule.fix(&ctx).unwrap();
            assert_eq!(fixed, expected, "{flavor:?} must wrap all six URLs");

            let fixed_ctx = crate::lint_context::LintContext::new(&fixed, flavor, None);
            assert!(rule.check(&fixed_ctx).unwrap().is_empty());
            assert_eq!(
                rule.fix(&fixed_ctx).unwrap(),
                fixed,
                "{flavor:?} fix must be idempotent"
            );
        }
    }

    /// `<` opens JSX, so the autolink form the other flavors use is a parse error in
    /// MDX and the "fixed" document stops compiling. Each MDX expectation below was
    /// verified to compile under `@mdx-js/mdx` 3.x and to render an anchor whose
    /// href is the URL and whose text is the URL, literally.
    ///
    /// The Standard rows are the control: a change that merely stopped emitting the
    /// angle-bracket form everywhere would pass a one-sided test.
    #[test]
    fn test_mdx_fixes_bare_urls_to_links_instead_of_autolinks() {
        let rule = MD034NoBareUrls::default();
        let cases = [
            (
                "Bare link: http://localhost/\n",
                "Bare link: [http://localhost/](http://localhost/)\n",
                "Bare link: <http://localhost/>\n",
            ),
            (
                "Visit www.example.com today\n",
                "Visit [www.example.com](https://www.example.com) today\n",
                "Visit <https://www.example.com> today\n",
            ),
            (
                "Mail user@example.com now\n",
                "Mail [user@example.com](mailto:user@example.com) now\n",
                "Mail <user@example.com> now\n",
            ),
            (
                "Chat xmpp:foo@bar.baz please\n",
                "Chat [xmpp:foo@bar.baz](xmpp:foo@bar.baz) please\n",
                "Chat <xmpp:foo@bar.baz> please\n",
            ),
        ];

        for (content, expected_mdx, expected_standard) in cases {
            let mdx_ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::MDX, None);
            assert_eq!(
                rule.check(&mdx_ctx).unwrap().len(),
                1,
                "MDX must still report the bare URL in {content:?}"
            );
            assert_eq!(rule.fix(&mdx_ctx).unwrap(), expected_mdx, "MDX fix for {content:?}");

            let standard_ctx =
                crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
            assert_eq!(
                rule.fix(&standard_ctx).unwrap(),
                expected_standard,
                "Standard fix for {content:?} must be unchanged"
            );
        }
    }

    /// `EMAIL_PATTERN` matches the address alone, so every schemed URI ending in one
    /// presents its tail as a bare email. `xmpp:` was recognized by name; the others are
    /// the same construct and were reported, which would have split the URI at the colon.
    #[test]
    fn test_an_address_behind_a_uri_scheme_is_not_a_bare_email() {
        let rule = MD034NoBareUrls::default();
        for content in [
            "Mail mailto:user@example.com now\n",
            "Chat xmpp:foo@bar.baz please\n",
            "Call sip:user@example.com now\n",
            "Key openpgp4fpr:user@example.com here\n",
            "Ping xmpp+tls:user@example.com now\n",
        ] {
            for flavor in [
                crate::config::MarkdownFlavor::Standard,
                crate::config::MarkdownFlavor::MDX,
            ] {
                let ctx = crate::lint_context::LintContext::new(content, flavor, None);
                let emails: Vec<_> = rule
                    .check(&ctx)
                    .unwrap()
                    .into_iter()
                    .filter(|w| w.message.starts_with("Email address"))
                    .collect();
                assert!(
                    emails.is_empty(),
                    "{flavor:?} reported the tail of a schemed URI in {content:?} as a bare email: {emails:?}"
                );
            }
        }
    }

    /// The control for the guard above: it must not swallow an address that merely has a
    /// colon somewhere before it, which is ordinary prose and a real finding.
    #[test]
    fn test_a_colon_before_an_address_is_still_a_bare_email() {
        let rule = MD034NoBareUrls::default();
        for content in [
            "Contact: user@example.com\n",
            "Note (see 3:1): user@example.com\n",
            "Mail 2user@example.com now\n",
            // The colon is adjacent, so only the scheme grammar separates these from a
            // schemed URI: a scheme cannot start with a digit, and cannot be empty.
            "Ratio 3:user@example.com now\n",
            "Mail :user@example.com now\n",
        ] {
            let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
            assert_eq!(
                rule.check(&ctx).unwrap().len(),
                1,
                "{content:?} must still report a bare email"
            );
        }
    }

    #[test]
    fn test_follows_uri_scheme() {
        // The address starts right after the colon in each of these.
        assert!(follows_uri_scheme("mailto:a@b.co", 7));
        assert!(follows_uri_scheme("Mail mailto:a@b.co", 12));
        assert!(follows_uri_scheme("xmpp+tls:a@b.co", 9));
        assert!(follows_uri_scheme("a:a@b.co", 2));

        assert!(!follows_uri_scheme("a@b.co", 0));
        assert!(!follows_uri_scheme("Contact: a@b.co", 9), "a space separates the colon");
        // A scheme must begin with a letter, so neither of these is one.
        assert!(!follows_uri_scheme("2mailto:a@b.co", 8));
        assert!(!follows_uri_scheme(":a@b.co", 1), "empty scheme");
        // Multi-byte text before the colon must not panic or be misread. `é` is not a
        // scheme character, so the run ends on it and the slice must land on its boundary.
        assert!(follows_uri_scheme("Schrijf mailto:a@b.co", 15));
        assert!(!follows_uri_scheme("Schrijf é:a@b.co", 11));
    }

    /// A `[` binds to the character before it, which the `<url>` form this replaces
    /// never had to care about. Every expectation below was rendered through a
    /// spec-exact CommonMark+GFM implementation: unguarded, the first two produce an
    /// `<img>` instead of an `<a>`, silently and permanently, since the result is
    /// valid Markdown that MD034 does not report again.
    #[test]
    fn test_mdx_escapes_an_active_bang_before_the_link() {
        let rule = MD034NoBareUrls::default();
        let cases = [
            (
                "Download now!https://example.com/f today\n",
                "Download now\\![https://example.com/f](https://example.com/f) today\n",
            ),
            (
                "Contact us!user@example.com now\n",
                "Contact us\\![user@example.com](mailto:user@example.com) now\n",
            ),
            // Already escaped: the `!` is literal text and binds to nothing, so a second
            // backslash would escape the backslash instead and reinstate the image.
            (
                "Escaped already\\!https://example.com/e today\n",
                "Escaped already\\![https://example.com/e](https://example.com/e) today\n",
            ),
            // An even run leaves the `!` active again.
            (
                "Two slashes\\\\!https://example.com/t today\n",
                "Two slashes\\\\\\![https://example.com/t](https://example.com/t) today\n",
            ),
            // Separated by a space, the `!` cannot bind to the link at all.
            (
                "Normal! https://example.com/s today\n",
                "Normal! [https://example.com/s](https://example.com/s) today\n",
            ),
        ];

        for (content, expected) in cases {
            let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::MDX, None);
            assert_eq!(rule.fix(&ctx).unwrap(), expected, "MDX fix for {content:?}");
        }
    }

    /// The bang guard belongs to the link form, so the autolink flavors must not grow
    /// an escape they never needed: `!<url>` is not image syntax.
    #[test]
    fn test_a_preceding_bang_is_untouched_outside_jsx_flavors() {
        let rule = MD034NoBareUrls::default();
        let ctx = crate::lint_context::LintContext::new(
            "Download now!https://example.com/f today\n",
            crate::config::MarkdownFlavor::Standard,
            None,
        );
        assert_eq!(rule.fix(&ctx).unwrap(), "Download now!<https://example.com/f> today\n");
    }

    /// After a `]`, the emitted link text is read as that span's reference label, so the
    /// anchor resolves against an unrelated definition and the real destination is left
    /// behind as literal text. Escaping the `]` would break the span it closes, so the
    /// finding is reported with no fix rather than with a corrupting one.
    #[test]
    fn test_mdx_reports_but_does_not_fix_a_url_after_an_active_close_bracket() {
        let rule = MD034NoBareUrls::default();
        let content =
            "[See more]https://example.com/x here\n\n[https://example.com/x]: https://elsewhere.example.com/\n";
        let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::MDX, None);

        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 1, "the bare URL is still a finding");
        assert!(warnings[0].fix.is_none(), "no replacement is safe here");
        assert_eq!(rule.fix(&ctx).unwrap(), content, "fmt must leave the line alone");
    }

    /// An escaped `]` closes no span, so the link is safe and keeps its fix.
    #[test]
    fn test_mdx_fixes_after_an_escaped_close_bracket() {
        let rule = MD034NoBareUrls::default();
        let ctx = crate::lint_context::LintContext::new(
            "Text \\]https://example.com/x here\n",
            crate::config::MarkdownFlavor::MDX,
            None,
        );
        assert_eq!(
            rule.fix(&ctx).unwrap(),
            "Text \\][https://example.com/x](https://example.com/x) here\n"
        );
    }

    #[test]
    fn test_classify_link_prefix() {
        let free = |s: &str| matches!(classify_link_prefix(s, s.len()), LinkPrefix::Free);
        let bang = |s: &str| matches!(classify_link_prefix(s, s.len()), LinkPrefix::ActiveBang);
        let bracket = |s: &str| matches!(classify_link_prefix(s, s.len()), LinkPrefix::ActiveCloseBracket);

        assert!(free(""), "start of line binds to nothing");
        assert!(free("plain "));
        assert!(free("plain"));
        assert!(bang("hi!"));
        assert!(free("hi\\!"), "one backslash escapes the bang");
        assert!(bang("hi\\\\!"), "two backslashes escape each other, not the bang");
        assert!(free("hi\\\\\\!"), "three escape the bang again");
        assert!(bracket("[a]"));
        assert!(free("[a\\]"), "an escaped bracket closes no span");
        // A multi-byte character before the span must not be mistaken for either.
        assert!(free("café"));
        assert!(bang("café!"));
    }

    /// The link text must display the URL literally, so every character that is
    /// active there is escaped. Each case below was checked against the `@mdx-js/mdx`
    /// 3.x compiler: unescaped, a `*` pair becomes emphasis and `&amp;` decodes to a
    /// bare `&`, both dropping characters from the URL the reader sees. `_` and `~`
    /// happen to render literally today (intraword `_` is not emphasis, and MDX
    /// enables no strikethrough by default), but a `remark-gfm` pipeline is the norm
    /// in MDX projects, so they are escaped rather than left to the plugin set.
    #[test]
    fn test_mdx_link_text_escapes_characters_that_would_not_render_literally() {
        let rule = MD034NoBareUrls::default();
        let cases = [
            ("https://ex.com/a*b*c", "https://ex.com/a\\*b\\*c"),
            ("https://ex.com/a&amp;b", "https://ex.com/a\\&amp;b"),
            ("https://ex.com/a~b~c", "https://ex.com/a\\~b\\~c"),
            ("https://ex.com/a_b_c", "https://ex.com/a\\_b\\_c"),
        ];

        for (url, escaped_text) in cases {
            let content = format!("See {url} here\n");
            let ctx = crate::lint_context::LintContext::new(&content, crate::config::MarkdownFlavor::MDX, None);
            assert_eq!(
                rule.fix(&ctx).unwrap(),
                format!("See [{escaped_text}]({url}) here\n"),
                "MDX must escape the link text for {url}"
            );

            let standard_ctx =
                crate::lint_context::LintContext::new(&content, crate::config::MarkdownFlavor::Standard, None);
            assert_eq!(
                rule.fix(&standard_ctx).unwrap(),
                format!("See <{url}> here\n"),
                "Standard emits the autolink, which needs no escaping"
            );
        }
    }

    /// A `{...}` pair inside a URL makes MDX treat the span as a JSX expression, so
    /// the rule never reports it and no fix is offered. An UNMATCHED brace is not a
    /// complete expression, so it reaches the fix and must be escaped: left alone it
    /// is a hard MDX compile error ("expected a corresponding closing brace"), which
    /// would make the fix produce a document that no longer builds.
    #[test]
    fn test_mdx_braces_are_skipped_when_paired_and_escaped_when_not() {
        let rule = MD034NoBareUrls::default();

        let paired = "See https://ex.com/a{b}c here\n";
        let paired_ctx = crate::lint_context::LintContext::new(paired, crate::config::MarkdownFlavor::MDX, None);
        assert!(
            rule.check(&paired_ctx).unwrap().is_empty(),
            "a balanced brace pair is a JSX expression, which MD034 leaves alone"
        );

        let standard_ctx = crate::lint_context::LintContext::new(paired, crate::config::MarkdownFlavor::Standard, None);
        assert_eq!(
            rule.fix(&standard_ctx).unwrap(),
            "See <https://ex.com/a{b}c> here\n",
            "outside MDX the braces carry no meaning, so the URL is still reported"
        );

        for (url, escaped_text) in [
            ("https://ex.com/a{b", "https://ex.com/a\\{b"),
            ("https://ex.com/a}b", "https://ex.com/a\\}b"),
        ] {
            let content = format!("See {url} here\n");
            let ctx = crate::lint_context::LintContext::new(&content, crate::config::MarkdownFlavor::MDX, None);
            assert_eq!(
                rule.fix(&ctx).unwrap(),
                format!("See [{escaped_text}]({url}) here\n"),
                "an unmatched brace reaches the fix and must be escaped"
            );
        }
    }

    /// A bare link destination may only hold balanced parentheses: with an unmatched
    /// `(`, CommonMark rejects the link and renders no anchor at all. Unmatched
    /// closers never reach here (`trim_trailing_punctuation` strips those), so the
    /// angle-bracket destination is what covers the remaining case.
    #[test]
    fn test_mdx_unbalanced_open_paren_uses_an_angle_bracket_destination() {
        let rule = MD034NoBareUrls::default();

        let unbalanced = "Go to https://ex.com/a(b now\n";
        let ctx = crate::lint_context::LintContext::new(unbalanced, crate::config::MarkdownFlavor::MDX, None);
        assert_eq!(
            rule.fix(&ctx).unwrap(),
            "Go to [https://ex.com/a(b](<https://ex.com/a(b>) now\n"
        );

        let balanced = "Go to https://en.wikipedia.org/wiki/Foo_(bar) now\n";
        let ctx = crate::lint_context::LintContext::new(balanced, crate::config::MarkdownFlavor::MDX, None);
        assert_eq!(
            rule.fix(&ctx).unwrap(),
            "Go to [https://en.wikipedia.org/wiki/Foo\\_(bar)](https://en.wikipedia.org/wiki/Foo_(bar)) now\n",
            "balanced parens need no angle brackets"
        );
    }

    /// Everything the MDX branch emits must survive a second pass untouched,
    /// including the shapes whose escaping or angle brackets are unusual.
    #[test]
    fn test_mdx_fix_is_idempotent_and_stops_reporting() {
        let rule = MD034NoBareUrls::default();
        let content = "\
Plain http://localhost/ and www.example.com.

Mail user@example.com or see https://ex.com/a*b_c{d}e.

Parens https://ex.com/a(b and https://en.wikipedia.org/wiki/Foo_(bar).
";
        let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::MDX, None);
        let fixed = rule.fix(&ctx).unwrap();
        assert_ne!(fixed, content, "the fix must actually rewrite this document");

        let fixed_ctx = crate::lint_context::LintContext::new(&fixed, crate::config::MarkdownFlavor::MDX, None);
        assert!(
            rule.check(&fixed_ctx).unwrap().is_empty(),
            "MDX must not re-report its own output: {:?}",
            rule.check(&fixed_ctx).unwrap()
        );
        assert_eq!(rule.fix(&fixed_ctx).unwrap(), fixed, "MDX fix must be idempotent");
    }

    /// The carve-out is confined to flavors that carry JSX. MDG in particular reaches
    /// its own fix-stripping branch unchanged.
    #[test]
    fn test_link_form_is_confined_to_jsx_flavors() {
        let rule = MD034NoBareUrls::default();
        let content = "Visit https://example.com today\n";

        for flavor in [
            crate::config::MarkdownFlavor::Standard,
            crate::config::MarkdownFlavor::MkDocs,
            crate::config::MarkdownFlavor::MyST,
            crate::config::MarkdownFlavor::Quarto,
            crate::config::MarkdownFlavor::Obsidian,
        ] {
            assert!(!flavor.supports_jsx(), "{flavor:?} is not a JSX flavor");
            let ctx = crate::lint_context::LintContext::new(content, flavor, None);
            assert_eq!(
                rule.fix(&ctx).unwrap(),
                "Visit <https://example.com> today\n",
                "{flavor:?} must keep the autolink form"
            );
        }

        let mdg_ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::MDG, None);
        assert_eq!(rule.check(&mdg_ctx).unwrap().len(), 1);
        assert!(rule.check(&mdg_ctx).unwrap()[0].fix.is_none());
        assert_eq!(rule.fix(&mdg_ctx).unwrap(), content);
    }

    #[test]
    fn test_escape_mdx_link_text_covers_every_active_character() {
        assert_eq!(escape_mdx_link_text("plain"), "plain");
        for ch in MDX_LINK_TEXT_ESCAPES {
            assert_eq!(escape_mdx_link_text(&ch.to_string()), format!("\\{ch}"));
        }
    }

    #[test]
    fn test_has_balanced_parens() {
        assert!(has_balanced_parens("https://ex.com/a"));
        assert!(has_balanced_parens("https://ex.com/(a)"));
        assert!(has_balanced_parens("https://ex.com/(a)(b)"));
        assert!(has_balanced_parens("https://ex.com/((a))"));
        assert!(!has_balanced_parens("https://ex.com/(a"));
        assert!(!has_balanced_parens("https://ex.com/a)"));
        assert!(
            !has_balanced_parens("https://ex.com/)a("),
            "equal counts are not balance"
        );
    }
}
