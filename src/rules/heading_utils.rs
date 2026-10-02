use regex::Regex;
use std::fmt;
use std::str::FromStr;
use std::sync::LazyLock;

static ATX_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(\s*)(#{1,6})(\s*)([^#\n]*?)(?:\s+(#{1,6}))?\s*$").unwrap());
static SETEXT_HEADING_1: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^(\s*)(=+)(\s*)$").unwrap());
static SETEXT_HEADING_2: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^(\s*)(-+)(\s*)$").unwrap());
static HTML_TAG_REGEX: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"<[^>]*>").unwrap());

/// Represents different styles of Markdown headings
#[derive(Debug, Clone, PartialEq, Eq, Hash, Copy)]
pub enum HeadingStyle {
    Atx,       // # Heading
    AtxClosed, // # Heading #
    Setext1,   // Heading
    // =======
    Setext2, // Heading
    // -------
    Consistent,          // For maintaining consistency with the first found header style
    SetextWithAtx,       // Setext for h1/h2, ATX for h3-h6
    SetextWithAtxClosed, // Setext for h1/h2, ATX closed for h3-h6
}

impl fmt::Display for HeadingStyle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            HeadingStyle::Atx => "atx",
            HeadingStyle::AtxClosed => "atx-closed",
            HeadingStyle::Setext1 => "setext1",
            HeadingStyle::Setext2 => "setext2",
            HeadingStyle::Consistent => "consistent",
            HeadingStyle::SetextWithAtx => "setext-with-atx",
            HeadingStyle::SetextWithAtxClosed => "setext-with-atx-closed",
        };
        write!(f, "{s}")
    }
}

impl FromStr for HeadingStyle {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let normalized = s.trim().to_ascii_lowercase().replace('-', "_");
        match normalized.as_str() {
            "atx" => Ok(HeadingStyle::Atx),
            "atx_closed" => Ok(HeadingStyle::AtxClosed),
            "setext1" | "setext" => Ok(HeadingStyle::Setext1),
            "setext2" => Ok(HeadingStyle::Setext2),
            "consistent" => Ok(HeadingStyle::Consistent),
            "setext_with_atx" => Ok(HeadingStyle::SetextWithAtx),
            "setext_with_atx_closed" => Ok(HeadingStyle::SetextWithAtxClosed),
            _ => Err(()),
        }
    }
}

/// Protect a literal terminal hash run when serializing plain ATX text.
/// Closed ATX adds its own marker and does not need this escape.
pub(crate) fn escape_atx_closing_hashes(text: &str) -> std::borrow::Cow<'_, str> {
    let first_hash = text.len() - text.bytes().rev().take_while(|&byte| byte == b'#').count();
    if first_hash == text.len() || (first_hash > 0 && !matches!(text.as_bytes()[first_hash - 1], b' ' | b'\t')) {
        return std::borrow::Cow::Borrowed(text);
    }
    let mut escaped = String::with_capacity(text.len() + 1);
    escaped.push_str(&text[..first_hash]);
    escaped.push('\\');
    escaped.push_str(&text[first_hash..]);
    std::borrow::Cow::Owned(escaped)
}

/// Utility functions for working with Markdown headings
pub struct HeadingUtils;

impl HeadingUtils {
    /// Convert a heading to a different style
    pub fn convert_heading_style(text_content: &str, level: u32, style: HeadingStyle) -> String {
        // Validate heading level
        let level = level.clamp(1, 6);

        if text_content.trim().is_empty() {
            // Empty headings: ATX can be just `##`, Setext requires text so return empty
            return match style {
                HeadingStyle::Atx => "#".repeat(level as usize),
                HeadingStyle::AtxClosed => {
                    let hashes = "#".repeat(level as usize);
                    format!("{hashes} {hashes}")
                }
                HeadingStyle::Setext1 | HeadingStyle::Setext2 => String::new(),
                // These are meta-styles resolved before calling this function
                HeadingStyle::Consistent | HeadingStyle::SetextWithAtx | HeadingStyle::SetextWithAtxClosed => {
                    "#".repeat(level as usize)
                }
            };
        }

        let indentation = text_content
            .chars()
            .take_while(|c| c.is_whitespace())
            .collect::<String>();
        let text_content = text_content.trim();

        match style {
            HeadingStyle::Atx => {
                format!(
                    "{}{} {}",
                    indentation,
                    "#".repeat(level as usize),
                    escape_atx_closing_hashes(text_content)
                )
            }
            HeadingStyle::AtxClosed => {
                format!(
                    "{}{} {} {}",
                    indentation,
                    "#".repeat(level as usize),
                    text_content,
                    "#".repeat(level as usize)
                )
            }
            HeadingStyle::Setext1 | HeadingStyle::Setext2 => {
                if level > 2 {
                    // Fall back to ATX style for levels > 2
                    format!(
                        "{}{} {}",
                        indentation,
                        "#".repeat(level as usize),
                        escape_atx_closing_hashes(text_content)
                    )
                } else {
                    let underline_char = if level == 1 || style == HeadingStyle::Setext1 {
                        '='
                    } else {
                        '-'
                    };
                    let visible_length = text_content.chars().count();
                    let underline_length = visible_length.max(1); // Ensure at least 1 underline char
                    format!(
                        "{}{}\n{}{}",
                        indentation,
                        text_content,
                        indentation,
                        underline_char.to_string().repeat(underline_length)
                    )
                }
            }
            HeadingStyle::Consistent => {
                // For Consistent style, default to ATX as it's the most commonly used
                format!(
                    "{}{} {}",
                    indentation,
                    "#".repeat(level as usize),
                    escape_atx_closing_hashes(text_content)
                )
            }
            HeadingStyle::SetextWithAtx => {
                if level <= 2 {
                    // Use Setext for h1/h2
                    let underline_char = if level == 1 { '=' } else { '-' };
                    let visible_length = text_content.chars().count();
                    let underline_length = visible_length.max(1);
                    format!(
                        "{}{}\n{}{}",
                        indentation,
                        text_content,
                        indentation,
                        underline_char.to_string().repeat(underline_length)
                    )
                } else {
                    // Use ATX for h3-h6
                    format!(
                        "{}{} {}",
                        indentation,
                        "#".repeat(level as usize),
                        escape_atx_closing_hashes(text_content)
                    )
                }
            }
            HeadingStyle::SetextWithAtxClosed => {
                if level <= 2 {
                    // Use Setext for h1/h2
                    let underline_char = if level == 1 { '=' } else { '-' };
                    let visible_length = text_content.chars().count();
                    let underline_length = visible_length.max(1);
                    format!(
                        "{}{}\n{}{}",
                        indentation,
                        text_content,
                        indentation,
                        underline_char.to_string().repeat(underline_length)
                    )
                } else {
                    // Use ATX closed for h3-h6
                    format!(
                        "{}{} {} {}",
                        indentation,
                        "#".repeat(level as usize),
                        text_content,
                        "#".repeat(level as usize)
                    )
                }
            }
        }
    }

    /// Convert a heading text to a valid ID for fragment links
    pub fn heading_to_fragment(text: &str) -> String {
        // Remove any HTML tags
        let text_no_html = HTML_TAG_REGEX.replace_all(text, "");

        // Convert to lowercase and trim
        let text_lower = text_no_html.trim().to_lowercase();

        // Replace spaces and punctuation with hyphens
        let text_with_hyphens = text_lower
            .chars()
            .map(|c| if c.is_alphanumeric() { c } else { '-' })
            .collect::<String>();

        // Replace multiple consecutive hyphens with a single hyphen
        let text_clean = text_with_hyphens
            .split('-')
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join("-");

        // Remove leading and trailing hyphens
        text_clean.trim_matches('-').to_string()
    }
}

/// Setext-to-ATX conversion joins text lines. That must not alter literal
/// values or discard a hard break that belongs to the rendered heading.
pub(crate) fn setext_to_atx_is_safe(
    ctx: &crate::lint_context::LintContext,
    heading_idx: usize,
    heading: &crate::lint_context::HeadingInfo,
) -> bool {
    if !matches!(
        heading.style,
        crate::lint_context::HeadingStyle::Setext1 | crate::lint_context::HeadingStyle::Setext2
    ) {
        return true;
    }
    let first_idx = heading_idx + 1 - heading.text_lines;
    let tags = ctx.html_tags();
    let inside_literal = |pos| {
        let idx = tags.partition_point(|tag| tag.byte_offset < pos);
        ctx.is_inside_template_code(pos) || ctx.is_inside_mdx_code(pos) || (idx > 0 && tags[idx - 1].byte_end > pos)
    };
    let first = &ctx.lines[first_idx];
    if inside_literal(first.byte_offset + first.indent) {
        return false;
    }
    let joined_lines = &ctx.lines[first_idx..heading_idx];
    let title_crosses_join = |start: usize, end: usize, title: Option<&str>| {
        if !title.is_some_and(|title| title.contains(['\n', '\r'])) {
            return false;
        }
        let idx = joined_lines.partition_point(|line| line.byte_offset + line.byte_len <= start);
        joined_lines
            .get(idx)
            .is_some_and(|line| line.byte_offset + line.byte_len < end)
    };
    // Inline title newlines are attribute data. Reference titles live in an
    // untouched definition, and ordinary label soft breaks remain joinable.
    if !joined_lines.is_empty()
        && (ctx.links().iter().any(|link| {
            link.link_type == pulldown_cmark::LinkType::Inline
                && title_crosses_join(link.byte_offset, link.byte_end, link.title.as_deref())
        }) || ctx.images().iter().any(|image| {
            image.link_type == pulldown_cmark::LinkType::Inline
                && title_crosses_join(image.byte_offset, image.byte_end, image.title.as_deref())
        }))
    {
        // Decoded newlines can also come from entities in an unchanged title.
        // Reparse only these rare candidates with the same Markdown flavor.
        let converted = HeadingUtils::convert_heading_style(&heading.raw_text, heading.level as u32, HeadingStyle::Atx);
        let converted_ctx = crate::lint_context::LintContext::new(&converted, ctx.flavor, None);
        if converted_ctx.valid_headings().count() != 1 {
            return false;
        }
        let text_start = first.byte_offset;
        let text_end = ctx.lines[heading_idx].byte_offset + ctx.lines[heading_idx].byte_len;
        let expected_links = ctx
            .links()
            .iter()
            .filter(|link| {
                link.link_type == pulldown_cmark::LinkType::Inline
                    && link.byte_offset >= text_start
                    && link.byte_end <= text_end
            })
            .map(|link| link.title.as_deref().unwrap_or(""));
        let actual_links = converted_ctx
            .links()
            .iter()
            .filter(|link| link.link_type == pulldown_cmark::LinkType::Inline)
            .map(|link| link.title.as_deref().unwrap_or(""));
        let expected_images = ctx
            .images()
            .iter()
            .filter(|image| {
                image.link_type == pulldown_cmark::LinkType::Inline
                    && image.byte_offset >= text_start
                    && image.byte_end <= text_end
            })
            .map(|image| image.title.as_deref().unwrap_or(""));
        let actual_images = converted_ctx
            .images()
            .iter()
            .filter(|image| image.link_type == pulldown_cmark::LinkType::Inline)
            .map(|image| image.title.as_deref().unwrap_or(""));
        if !expected_links.eq(actual_links) || !expected_images.eq(actual_images) {
            return false;
        }
    }
    (first_idx..heading_idx).all(|idx| {
        let line = &ctx.lines[idx];
        let content = line.content(ctx.content);
        let boundary = line.byte_offset + line.byte_len;
        !inside_literal(boundary)
            && !ctx.line_ends_with_hard_break(idx + 1)
            && !content.ends_with("  ")
            && !(ctx.is_in_code_span_byte(boundary)
                && (content.ends_with([' ', '\t']) || ctx.lines[idx + 1].content(ctx.content).starts_with([' ', '\t'])))
    })
}

/// Checks if a line is a heading
#[inline]
pub fn is_heading(line: &str) -> bool {
    // Fast path checks first
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return false;
    }

    if trimmed.starts_with('#') {
        // Check for ATX heading
        ATX_PATTERN.is_match(line)
    } else {
        // We can't tell for setext headings without looking at the next line
        false
    }
}

/// Checks if a line is a setext heading marker
#[inline]
pub fn is_setext_heading_marker(line: &str) -> bool {
    SETEXT_HEADING_1.is_match(line) || SETEXT_HEADING_2.is_match(line)
}

/// Get the heading level for a line
#[inline]
pub fn get_heading_level(lines: &[&str], index: usize) -> u32 {
    if index >= lines.len() {
        return 0;
    }

    let line = lines[index];

    // Check for ATX style heading
    if let Some(captures) = ATX_PATTERN.captures(line) {
        let hashes = captures.get(2).map_or("", |m| m.as_str());
        return hashes.len() as u32;
    }

    // Check for setext style heading
    if index < lines.len() - 1 {
        let next_line = lines[index + 1];

        if SETEXT_HEADING_1.is_match(next_line) {
            return 1;
        }

        if SETEXT_HEADING_2.is_match(next_line) {
            return 2;
        }
    }

    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_heading_style_conversion() {
        assert_eq!(
            HeadingUtils::convert_heading_style("Heading 1", 1, HeadingStyle::Atx),
            "# Heading 1"
        );
        assert_eq!(
            HeadingUtils::convert_heading_style("Heading 2", 2, HeadingStyle::AtxClosed),
            "## Heading 2 ##"
        );
        assert_eq!(
            HeadingUtils::convert_heading_style("Heading 1", 1, HeadingStyle::Setext1),
            "Heading 1\n========="
        );
        assert_eq!(
            HeadingUtils::convert_heading_style("Heading 2", 2, HeadingStyle::Setext2),
            "Heading 2\n---------"
        );
    }

    #[test]
    fn test_convert_heading_style_edge_cases() {
        // Empty text: ATX headings produce just the hash marks (valid markdown)
        assert_eq!(HeadingUtils::convert_heading_style("", 1, HeadingStyle::Atx), "#");
        assert_eq!(HeadingUtils::convert_heading_style("   ", 1, HeadingStyle::Atx), "#");
        assert_eq!(HeadingUtils::convert_heading_style("", 2, HeadingStyle::Atx), "##");
        assert_eq!(
            HeadingUtils::convert_heading_style("", 1, HeadingStyle::AtxClosed),
            "# #"
        );
        // Setext cannot represent empty headings, returns empty
        assert_eq!(HeadingUtils::convert_heading_style("", 1, HeadingStyle::Setext1), "");

        // Level clamping
        assert_eq!(
            HeadingUtils::convert_heading_style("Text", 0, HeadingStyle::Atx),
            "# Text"
        );
        assert_eq!(
            HeadingUtils::convert_heading_style("Text", 10, HeadingStyle::Atx),
            "###### Text"
        );

        // Setext with level > 2 falls back to ATX
        assert_eq!(
            HeadingUtils::convert_heading_style("Text", 3, HeadingStyle::Setext1),
            "### Text"
        );

        // Preserve indentation
        assert_eq!(
            HeadingUtils::convert_heading_style("  Text", 1, HeadingStyle::Atx),
            "  # Text"
        );

        // Very short text for setext
        assert_eq!(
            HeadingUtils::convert_heading_style("Hi", 1, HeadingStyle::Setext1),
            "Hi\n=="
        );
    }

    #[test]
    fn test_heading_to_fragment() {
        assert_eq!(HeadingUtils::heading_to_fragment("Simple Heading"), "simple-heading");
        assert_eq!(
            HeadingUtils::heading_to_fragment("Heading with Numbers 123"),
            "heading-with-numbers-123"
        );
        assert_eq!(
            HeadingUtils::heading_to_fragment("Special!@#$%Characters"),
            "special-characters"
        );
        assert_eq!(HeadingUtils::heading_to_fragment("  Trimmed  "), "trimmed");
        assert_eq!(
            HeadingUtils::heading_to_fragment("Multiple   Spaces"),
            "multiple-spaces"
        );
        assert_eq!(
            HeadingUtils::heading_to_fragment("Heading <em>with HTML</em>"),
            "heading-with-html"
        );
        assert_eq!(
            HeadingUtils::heading_to_fragment("---Leading-Dashes---"),
            "leading-dashes"
        );
        assert_eq!(HeadingUtils::heading_to_fragment(""), "");
    }

    #[test]
    fn test_module_level_functions() {
        // Test is_heading
        assert!(is_heading("# Heading"));
        assert!(is_heading("  ## Indented"));
        assert!(!is_heading("Not a heading"));
        assert!(!is_heading(""));

        // Test is_setext_heading_marker
        assert!(is_setext_heading_marker("========"));
        assert!(is_setext_heading_marker("--------"));
        assert!(is_setext_heading_marker("  ======"));
        assert!(!is_setext_heading_marker("# Heading"));
        assert!(is_setext_heading_marker("---")); // Three dashes is valid

        // Test get_heading_level
        let lines = vec!["# H1", "## H2", "### H3"];
        assert_eq!(get_heading_level(&lines, 0), 1);
        assert_eq!(get_heading_level(&lines, 1), 2);
        assert_eq!(get_heading_level(&lines, 2), 3);
        assert_eq!(get_heading_level(&lines, 10), 0);
    }

    #[test]
    fn test_heading_style_from_str() {
        assert_eq!(HeadingStyle::from_str("atx"), Ok(HeadingStyle::Atx));
        assert_eq!(HeadingStyle::from_str("ATX"), Ok(HeadingStyle::Atx));
        assert_eq!(HeadingStyle::from_str("atx_closed"), Ok(HeadingStyle::AtxClosed));
        assert_eq!(HeadingStyle::from_str("atx-closed"), Ok(HeadingStyle::AtxClosed));
        assert_eq!(HeadingStyle::from_str("ATX-CLOSED"), Ok(HeadingStyle::AtxClosed));
        assert_eq!(HeadingStyle::from_str("setext1"), Ok(HeadingStyle::Setext1));
        assert_eq!(HeadingStyle::from_str("setext"), Ok(HeadingStyle::Setext1));
        assert_eq!(HeadingStyle::from_str("setext2"), Ok(HeadingStyle::Setext2));
        assert_eq!(HeadingStyle::from_str("consistent"), Ok(HeadingStyle::Consistent));
        assert_eq!(
            HeadingStyle::from_str("setext_with_atx"),
            Ok(HeadingStyle::SetextWithAtx)
        );
        assert_eq!(
            HeadingStyle::from_str("setext-with-atx"),
            Ok(HeadingStyle::SetextWithAtx)
        );
        assert_eq!(
            HeadingStyle::from_str("setext_with_atx_closed"),
            Ok(HeadingStyle::SetextWithAtxClosed)
        );
        assert_eq!(
            HeadingStyle::from_str("setext-with-atx-closed"),
            Ok(HeadingStyle::SetextWithAtxClosed)
        );
        assert_eq!(HeadingStyle::from_str("invalid"), Err(()));
    }

    #[test]
    fn test_heading_style_display() {
        assert_eq!(HeadingStyle::Atx.to_string(), "atx");
        assert_eq!(HeadingStyle::AtxClosed.to_string(), "atx-closed");
        assert_eq!(HeadingStyle::Setext1.to_string(), "setext1");
        assert_eq!(HeadingStyle::Setext2.to_string(), "setext2");
        assert_eq!(HeadingStyle::Consistent.to_string(), "consistent");
    }

    #[test]
    fn test_unicode_heading_fragments() {
        assert_eq!(HeadingUtils::heading_to_fragment("你好世界"), "你好世界");
        assert_eq!(HeadingUtils::heading_to_fragment("Café René"), "café-rené");
    }
    #[test]
    fn plain_atx_serialization_preserves_literal_terminal_hashes() {
        for style in [
            HeadingStyle::Atx,
            HeadingStyle::Consistent,
            HeadingStyle::Setext1,
            HeadingStyle::Setext2,
            HeadingStyle::SetextWithAtx,
        ] {
            for (title, expected) in [
                ("Title ###", "Title \\###"),
                ("Title\t#", "Title\t\\#"),
                ("#######", "\\#######"),
                ("Title###", "Title###"),
                ("Title \\###", "Title \\###"),
                ("Title ### {#id}", "Title ### {#id}"),
            ] {
                assert_eq!(
                    HeadingUtils::convert_heading_style(title, 3, style),
                    format!("### {expected}"),
                    "{style:?}: {title}"
                );
            }
        }
        assert_eq!(
            HeadingUtils::convert_heading_style("Title ###", 1, HeadingStyle::AtxClosed),
            "# Title ### #"
        );
        assert_eq!(
            HeadingUtils::convert_heading_style("Title ###", 1, HeadingStyle::Setext1),
            "Title ###\n========="
        );
    }
}
