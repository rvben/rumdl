//! Utilities for handling Kramdown-specific syntax
//!
//! Kramdown is a superset of Markdown that adds additional features like
//! Inline Attribute Lists (IAL) for adding attributes to elements.

use lazy_static::lazy_static;
use regex::Regex;

lazy_static! {
    /// Pattern for Kramdown span IAL: text{:.class #id key="value"}
    static ref SPAN_IAL_PATTERN: Regex = Regex::new(r"\{[:\.#][^}]*\}$").unwrap();

    /// Pattern for Kramdown extensions opening: {::comment}, {::nomarkdown}, etc.
    static ref EXTENSION_OPEN_PATTERN: Regex = Regex::new(r"^\s*\{::([a-z]+)(?:\s+[^}]*)?\}\s*$").unwrap();

    /// Pattern for Kramdown extensions closing: {:/comment}, {:/}, etc.
    static ref EXTENSION_CLOSE_PATTERN: Regex = Regex::new(r"^\s*\{:/([a-z]+)?\}\s*$").unwrap();

    /// Pattern for Kramdown options: {::options key="value" /}
    static ref OPTIONS_PATTERN: Regex = Regex::new(r"^\s*\{::options\s+[^}]+/\}\s*$").unwrap();

    /// Pattern for footnote references: [^footnote]
    static ref FOOTNOTE_REF_PATTERN: Regex = Regex::new(r"\[\^[a-zA-Z0-9_\-]+\]").unwrap();

    /// Pattern for footnote definitions: [^footnote]: definition
    static ref FOOTNOTE_DEF_PATTERN: Regex = Regex::new(r"^\[\^[a-zA-Z0-9_\-]+\]:").unwrap();

    /// Pattern for abbreviations: *[HTML]: HyperText Markup Language
    static ref ABBREVIATION_PATTERN: Regex = Regex::new(r"^\*\[[^\]]+\]:").unwrap();

    /// Pattern for math blocks: $$ or $
    static ref MATH_BLOCK_PATTERN: Regex = Regex::new(r"^\$\$").unwrap();
    static ref MATH_INLINE_PATTERN: Regex = Regex::new(r"\$[^$]+\$").unwrap();
}

/// Check if a line is a Kramdown block attribute (IAL - Inline Attribute List)
///
/// Kramdown IAL syntax allows adding attributes to block elements:
/// - `{:.class}` - CSS class
/// - `{:#id}` - Element ID
/// - `{:attribute="value"}` - Generic attributes
/// - `{:.class #id attribute="value"}` - Combinations
///
/// # Examples
///
/// ```
/// use rumdl_lib::utils::kramdown_utils::is_kramdown_block_attribute;
///
/// assert!(is_kramdown_block_attribute("{:.wrap}"));
/// assert!(is_kramdown_block_attribute("{:#my-id}"));
/// assert!(is_kramdown_block_attribute("{:.class #id}"));
/// assert!(is_kramdown_block_attribute("{:style=\"color: red\"}"));
///
/// assert!(!is_kramdown_block_attribute("{just text}"));
/// assert!(!is_kramdown_block_attribute("{}"));
/// assert!(!is_kramdown_block_attribute("{"));
/// ```
pub fn is_kramdown_block_attribute(line: &str) -> bool {
    let trimmed = line.trim();

    // Must start with { and end with }
    if !trimmed.starts_with('{') || !trimmed.ends_with('}') || trimmed.len() < 3 {
        return false;
    }

    // Check if it matches Kramdown IAL patterns
    // Valid patterns start with {: or {# or {.
    let second_char = trimmed.chars().nth(1);
    matches!(second_char, Some(':') | Some('#') | Some('.'))
}

/// Check if text ends with a Kramdown span IAL (inline attribute)
///
/// # Examples
/// ```
/// use rumdl_lib::utils::kramdown_utils::has_span_ial;
///
/// assert!(has_span_ial("*emphasized*{:.highlight}"));
/// assert!(has_span_ial("[link](url){:target=\"_blank\"}"));
/// assert!(!has_span_ial("regular text"));
/// ```
pub fn has_span_ial(text: &str) -> bool {
    SPAN_IAL_PATTERN.is_match(text.trim())
}

/// Remove span IAL from text if present
pub fn remove_span_ial(text: &str) -> &str {
    if let Some(captures) = SPAN_IAL_PATTERN.find(text) {
        &text[..captures.start()]
    } else {
        text
    }
}

/// Check if a line is a Kramdown extension opening tag
///
/// Extensions include: comment, nomarkdown, options
pub fn is_kramdown_extension_open(line: &str) -> bool {
    EXTENSION_OPEN_PATTERN.is_match(line)
}

/// Check if a line is a Kramdown extension closing tag
pub fn is_kramdown_extension_close(line: &str) -> bool {
    EXTENSION_CLOSE_PATTERN.is_match(line)
}

/// Check if a line is a Kramdown options directive
pub fn is_kramdown_options(line: &str) -> bool {
    OPTIONS_PATTERN.is_match(line)
}

/// Check if a line is a Kramdown extension (any type)
pub fn is_kramdown_extension(line: &str) -> bool {
    is_kramdown_extension_open(line) || is_kramdown_extension_close(line) || is_kramdown_options(line)
}

/// Check if a line is an End-of-Block (EOB) marker
///
/// In Kramdown, a line containing only `^` ends the current block
pub fn is_eob_marker(line: &str) -> bool {
    line.trim() == "^"
}

/// Check if text contains a footnote reference
pub fn has_footnote_reference(text: &str) -> bool {
    FOOTNOTE_REF_PATTERN.is_match(text)
}

/// Check if a line is a footnote definition
pub fn is_footnote_definition(line: &str) -> bool {
    FOOTNOTE_DEF_PATTERN.is_match(line.trim_start())
}

/// Check if a line is an abbreviation definition
pub fn is_abbreviation_definition(line: &str) -> bool {
    ABBREVIATION_PATTERN.is_match(line.trim_start())
}

/// Check if a line starts a math block
pub fn is_math_block_delimiter(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed == "$$" || MATH_BLOCK_PATTERN.is_match(trimmed)
}

/// Check if text contains inline math
pub fn has_inline_math(text: &str) -> bool {
    MATH_INLINE_PATTERN.is_match(text)
}

/// Check if a line is a definition list item
///
/// Definition lists in Kramdown use the pattern:
/// ```text
/// Term
/// : Definition
/// ```
pub fn is_definition_list_item(line: &str) -> bool {
    let trimmed = line.trim_start();
    trimmed.starts_with(": ")
        || (trimmed.starts_with(':') && trimmed.len() > 1 && trimmed.chars().nth(1).is_some_and(|c| c.is_whitespace()))
}

/// Check if a line contains any Kramdown-specific syntax
pub fn has_kramdown_syntax(line: &str) -> bool {
    is_kramdown_block_attribute(line)
        || has_span_ial(line)
        || is_kramdown_extension(line)
        || is_eob_marker(line)
        || is_footnote_definition(line)
        || is_abbreviation_definition(line)
        || is_math_block_delimiter(line)
        || is_definition_list_item(line)
        || has_footnote_reference(line)
        || has_inline_math(line)
}

/// Generate header ID following kramdown's algorithm
///
/// Based on the official kramdown 2.5.1 Ruby gem behavior (verified through testing):
/// 1. Special symbol replacements (space & space -> --, space > space -> --, --> -> --)
/// 2. Remove all characters except letters, numbers, spaces and dashes
/// 3. Remove characters from start until first letter
/// 4. Convert spaces to dashes, letters to lowercase, preserve numbers
/// 5. Apply kramdown's hyphen consolidation ONLY to pre-existing hyphens
/// 6. Remove leading dashes, preserve trailing dashes
/// 7. If nothing remains, use "section"
///
/// This function is verified against the official kramdown Ruby implementation.
pub fn heading_to_fragment(heading: &str) -> String {
    if heading.is_empty() {
        return "section".to_string();
    }

    let text = heading.trim();
    if text.is_empty() {
        return "section".to_string();
    }

    // Step 1: Remove all characters except letters, numbers, spaces and dashes FIRST
    // This is crucial - kramdown removes colons and other chars before symbol replacement
    let mut step1 = String::new();
    for c in text.chars() {
        if c.is_ascii_alphabetic() || c.is_ascii_digit() || c == ' ' || c == '-' {
            step1.push(c);
        }
        // All other characters (::, _, accented chars, etc.) are REMOVED entirely
    }

    // Step 2: Apply special symbol replacements AFTER character filtering
    let mut processed = step1;

    // Handle special arrow sequences first
    processed = processed.replace("-->", "--");

    // Handle spaced symbols (& and > with spaces become double hyphens)
    processed = processed.replace(" & ", "--");
    processed = processed.replace(" > ", "--");

    // Step 3: Apply hyphen consolidation to existing hyphens
    let chars: Vec<char> = processed.chars().collect();
    let mut hyphen_consolidated = String::new();
    let mut i = 0;

    while i < chars.len() {
        let c = chars[i];

        if c == '-' {
            // Count consecutive hyphens
            let mut hyphen_count = 0;
            let mut j = i;
            while j < chars.len() && chars[j] == '-' {
                hyphen_count += 1;
                j += 1;
            }

            // Apply kramdown consolidation rules to existing hyphens:
            match hyphen_count {
                1 => hyphen_consolidated.push('-'),
                2 => {}                                  // 2 existing hyphens -> removed
                3 => {}                                  // 3 existing hyphens -> removed
                4 => hyphen_consolidated.push('-'),      // 4 -> 1
                5 => {}                                  // 5 -> removed
                6 => hyphen_consolidated.push_str("--"), // 6 -> 2
                _ => {
                    if hyphen_count % 2 == 0 && hyphen_count >= 6 {
                        hyphen_consolidated.push_str("--");
                    } else if hyphen_count % 4 == 0 {
                        hyphen_consolidated.push('-');
                    }
                }
            }

            i = j;
        } else {
            hyphen_consolidated.push(c);
            i += 1;
        }
    }

    processed = hyphen_consolidated;

    // Step 4: Remove characters from start until first letter
    let mut start_pos = 0;
    let mut found_letter = false;
    for (i, c) in processed.char_indices() {
        if c.is_ascii_alphabetic() {
            start_pos = i;
            found_letter = true;
            break;
        }
    }

    if !found_letter {
        return "section".to_string();
    }

    let step2 = &processed[start_pos..];

    // Step 5: Convert characters to final form (spaces become hyphens, no consolidation)
    let mut result = String::new();
    for c in step2.chars() {
        if c.is_ascii_alphabetic() {
            result.push(c.to_ascii_lowercase());
        } else if c.is_ascii_digit() {
            result.push(c);
        } else {
            // Spaces and remaining hyphens become hyphens (no further consolidation)
            result.push('-');
        }
    }

    // Step 6: Remove leading dashes only, preserve trailing dashes
    let trimmed = result.trim_start_matches('-').to_string();

    if trimmed.is_empty() {
        "section".to_string()
    } else {
        trimmed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_kramdown_class_attributes() {
        assert!(is_kramdown_block_attribute("{:.wrap}"));
        assert!(is_kramdown_block_attribute("{:.class-name}"));
        assert!(is_kramdown_block_attribute("{:.multiple .classes}"));
    }

    #[test]
    fn test_kramdown_id_attributes() {
        assert!(is_kramdown_block_attribute("{:#my-id}"));
        assert!(is_kramdown_block_attribute("{:#section-1}"));
    }

    #[test]
    fn test_kramdown_generic_attributes() {
        assert!(is_kramdown_block_attribute("{:style=\"color: red\"}"));
        assert!(is_kramdown_block_attribute("{:data-value=\"123\"}"));
    }

    #[test]
    fn test_kramdown_combined_attributes() {
        assert!(is_kramdown_block_attribute("{:.class #id}"));
        assert!(is_kramdown_block_attribute("{:#id .class style=\"color: blue\"}"));
        assert!(is_kramdown_block_attribute("{:.wrap #my-code .highlight}"));
    }

    #[test]
    fn test_non_kramdown_braces() {
        assert!(!is_kramdown_block_attribute("{just some text}"));
        assert!(!is_kramdown_block_attribute("{not kramdown}"));
        assert!(!is_kramdown_block_attribute("{ spaces }"));
    }

    #[test]
    fn test_edge_cases() {
        assert!(!is_kramdown_block_attribute("{}"));
        assert!(!is_kramdown_block_attribute("{"));
        assert!(!is_kramdown_block_attribute("}"));
        assert!(!is_kramdown_block_attribute(""));
        assert!(!is_kramdown_block_attribute("not braces"));
    }

    #[test]
    fn test_whitespace_handling() {
        assert!(is_kramdown_block_attribute("  {:.wrap}  "));
        assert!(is_kramdown_block_attribute("\t{:#id}\t"));
        assert!(is_kramdown_block_attribute(" {:.class #id} "));
    }
}
