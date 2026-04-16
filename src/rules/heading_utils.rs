use crate::utils::regex_cache::get_cached_regex;
use std::fmt;
use std::str::FromStr;

const ATX_PATTERN_STR: &str = r"^(\s*)(#{1,6})(\s*)([^#\n]*?)(?:\s+(#{1,6}))?\s*$";
const SETEXT_HEADING_1_STR: &str = r"^(\s*)(=+)(\s*)$";
const SETEXT_HEADING_2_STR: &str = r"^(\s*)(-+)(\s*)$";
const HTML_TAG_REGEX_STR: &str = r"<[^>]*>";

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
                format!("{}{} {}", indentation, "#".repeat(level as usize), text_content)
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
                    format!("{}{} {}", indentation, "#".repeat(level as usize), text_content)
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
                format!("{}{} {}", indentation, "#".repeat(level as usize), text_content)
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
                    format!("{}{} {}", indentation, "#".repeat(level as usize), text_content)
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
        let text_no_html =
            get_cached_regex(HTML_TAG_REGEX_STR).map_or_else(|_| text.into(), |re| re.replace_all(text, ""));

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
        get_cached_regex(ATX_PATTERN_STR)
            .map(|re| re.is_match(line))
            .unwrap_or(false)
    } else {
        // We can't tell for setext headings without looking at the next line
        false
    }
}

/// Checks if a line is a setext heading marker
#[inline]
pub fn is_setext_heading_marker(line: &str) -> bool {
    get_cached_regex(SETEXT_HEADING_1_STR)
        .map(|re| re.is_match(line))
        .unwrap_or(false)
        || get_cached_regex(SETEXT_HEADING_2_STR)
            .map(|re| re.is_match(line))
            .unwrap_or(false)
}

/// Get the heading level for a line
#[inline]
pub fn get_heading_level(lines: &[&str], index: usize) -> u32 {
    if index >= lines.len() {
        return 0;
    }

    let line = lines[index];

    // Check for ATX style heading
    if let Some(captures) = get_cached_regex(ATX_PATTERN_STR).ok().and_then(|re| re.captures(line)) {
        let hashes = captures.get(2).map_or("", |m| m.as_str());
        return hashes.len() as u32;
    }

    // Check for setext style heading
    if index < lines.len() - 1 {
        let next_line = lines[index + 1];

        if get_cached_regex(SETEXT_HEADING_1_STR)
            .map(|re| re.is_match(next_line))
            .unwrap_or(false)
        {
            return 1;
        }

        if get_cached_regex(SETEXT_HEADING_2_STR)
            .map(|re| re.is_match(next_line))
            .unwrap_or(false)
        {
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
}
