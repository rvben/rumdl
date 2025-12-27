//! Text reflow utilities for MD013
//!
//! This module implements text wrapping/reflow functionality that preserves
//! Markdown elements like links, emphasis, code spans, etc.

use crate::utils::element_cache::ElementCache;
use crate::utils::is_definition_list_item;
use crate::utils::regex_cache::{
    DISPLAY_MATH_REGEX, EMOJI_SHORTCODE_REGEX, FOOTNOTE_REF_REGEX, HTML_ENTITY_REGEX, HTML_TAG_PATTERN,
    INLINE_IMAGE_FANCY_REGEX, INLINE_LINK_FANCY_REGEX, INLINE_MATH_REGEX, LINKED_IMAGE_INLINE_INLINE,
    LINKED_IMAGE_INLINE_REF, LINKED_IMAGE_REF_INLINE, LINKED_IMAGE_REF_REF, REF_IMAGE_REGEX, REF_LINK_REGEX,
    SHORTCUT_REF_REGEX, STRIKETHROUGH_FANCY_REGEX, WIKI_LINK_REGEX,
};
use std::collections::HashSet;

/// Options for reflowing text
#[derive(Clone)]
pub struct ReflowOptions {
    /// Target line length
    pub line_length: usize,
    /// Whether to break on sentence boundaries when possible
    pub break_on_sentences: bool,
    /// Whether to preserve existing line breaks in paragraphs
    pub preserve_breaks: bool,
    /// Whether to enforce one sentence per line
    pub sentence_per_line: bool,
    /// Custom abbreviations for sentence detection
    /// Periods are optional - both "Dr" and "Dr." work the same
    /// Custom abbreviations are always added to the built-in defaults
    pub abbreviations: Option<Vec<String>>,
}

impl Default for ReflowOptions {
    fn default() -> Self {
        Self {
            line_length: 80,
            break_on_sentences: true,
            preserve_breaks: false,
            sentence_per_line: false,
            abbreviations: None,
        }
    }
}

/// Get the effective abbreviations set based on options
/// All abbreviations are normalized to lowercase for case-insensitive matching
/// Custom abbreviations are always merged with built-in defaults
fn get_abbreviations(custom: &Option<Vec<String>>) -> HashSet<String> {
    // Only include abbreviations that:
    // 1. Conventionally ALWAYS have a period in standard writing
    // 2. Are followed by something (name, example), not sentence-final
    //
    // Do NOT include:
    // - Words that don't typically take periods (vs, etc)
    // - Abbreviations that can end sentences (Inc., Ph.D., U.S.)
    let mut abbreviations: HashSet<String> = [
        // Titles - always have period, always followed by a name
        "Mr", "Mrs", "Ms", "Dr", "Prof", "Sr", "Jr",
        // Latin - always written with periods, introduce examples/references
        "i.e", "e.g",
    ]
    .iter()
    .map(|s| s.to_lowercase())
    .collect();

    // Always extend defaults with custom abbreviations
    // Strip any trailing periods and normalize to lowercase for consistent matching
    if let Some(custom_list) = custom {
        for abbr in custom_list {
            let normalized = abbr.trim_end_matches('.').to_lowercase();
            if !normalized.is_empty() {
                abbreviations.insert(normalized);
            }
        }
    }

    abbreviations
}

/// Check if text ends with a common abbreviation followed by a period
///
/// Abbreviations only count when followed by a period, not ! or ?.
/// This prevents false positives where words ending in abbreviation-like
/// letter sequences (e.g., "paradigms" ending in "ms") are incorrectly
/// detected as abbreviations.
///
/// Examples:
///   - "Dr." -> true (abbreviation)
///   - "Dr?" -> false (question, not abbreviation)
///   - "paradigms." -> false (not in abbreviation list)
///   - "paradigms?" -> false (question mark, not abbreviation)
///
/// See: Issue #150
fn text_ends_with_abbreviation(text: &str, abbreviations: &HashSet<String>) -> bool {
    // Only check if text ends with a period (abbreviations require periods)
    if !text.ends_with('.') {
        return false;
    }

    // Remove the trailing period
    let without_period = text.trim_end_matches('.');

    // Get the last word by splitting on whitespace
    let last_word = without_period.split_whitespace().last().unwrap_or("");

    if last_word.is_empty() {
        return false;
    }

    // O(1) HashSet lookup (abbreviations are already lowercase)
    abbreviations.contains(&last_word.to_lowercase())
}

/// Detect if a character position is a sentence boundary
/// Based on the approach from github.com/JoshuaKGoldberg/sentences-per-line
fn is_sentence_boundary(text: &str, pos: usize, abbreviations: &HashSet<String>) -> bool {
    let chars: Vec<char> = text.chars().collect();

    if pos + 1 >= chars.len() {
        return false;
    }

    // Check for sentence-ending punctuation
    let c = chars[pos];
    if c != '.' && c != '!' && c != '?' {
        return false;
    }

    // Must be followed by at least one space
    if chars[pos + 1] != ' ' {
        return false;
    }

    // Skip all whitespace after the punctuation to find the start of the next sentence
    let mut next_char_pos = pos + 2;
    while next_char_pos < chars.len() && chars[next_char_pos].is_whitespace() {
        next_char_pos += 1;
    }

    // Check if we reached the end of the string
    if next_char_pos >= chars.len() {
        return false;
    }

    // Next character after space(s) must be uppercase (new sentence indicator)
    if !chars[next_char_pos].is_uppercase() {
        return false;
    }

    // Look back to check for common abbreviations (only applies to periods)
    if pos > 0 && c == '.' {
        // Check if the text up to and including this period ends with an abbreviation
        // Note: text[..=pos] includes the character at pos (the period)
        if text_ends_with_abbreviation(&text[..=pos], abbreviations) {
            return false;
        }

        // Check for decimal numbers (e.g., "3.14")
        // Make sure to check if next_char_pos is within bounds
        if chars[pos - 1].is_numeric() && next_char_pos < chars.len() && chars[next_char_pos].is_numeric() {
            return false;
        }
    }
    true
}

/// Split text into sentences
pub fn split_into_sentences(text: &str) -> Vec<String> {
    split_into_sentences_custom(text, &None)
}

/// Split text into sentences with custom abbreviations
pub fn split_into_sentences_custom(text: &str, custom_abbreviations: &Option<Vec<String>>) -> Vec<String> {
    let abbreviations = get_abbreviations(custom_abbreviations);
    split_into_sentences_with_set(text, &abbreviations)
}

/// Internal function to split text into sentences with a pre-computed abbreviations set
/// Use this when calling multiple times in a loop to avoid repeatedly computing the set
fn split_into_sentences_with_set(text: &str, abbreviations: &HashSet<String>) -> Vec<String> {
    let mut sentences = Vec::new();
    let mut current_sentence = String::new();
    let mut chars = text.chars().peekable();
    let mut pos = 0;

    while let Some(c) = chars.next() {
        current_sentence.push(c);

        if is_sentence_boundary(text, pos, abbreviations) {
            // Include the space after sentence if it exists
            if chars.peek() == Some(&' ') {
                chars.next();
                pos += 1;
            }
            sentences.push(current_sentence.trim().to_string());
            current_sentence.clear();
        }

        pos += 1;
    }

    // Add any remaining text as the last sentence
    if !current_sentence.trim().is_empty() {
        sentences.push(current_sentence.trim().to_string());
    }
    sentences
}

/// Check if a line is a horizontal rule (---, ___, ***)
fn is_horizontal_rule(line: &str) -> bool {
    if line.len() < 3 {
        return false;
    }

    // Check if line consists only of -, _, or * characters (at least 3)
    let chars: Vec<char> = line.chars().collect();
    if chars.is_empty() {
        return false;
    }

    let first_char = chars[0];
    if first_char != '-' && first_char != '_' && first_char != '*' {
        return false;
    }

    // All characters should be the same (allowing spaces between)
    for c in &chars {
        if *c != first_char && *c != ' ' {
            return false;
        }
    }

    // Count non-space characters
    let non_space_count = chars.iter().filter(|c| **c != ' ').count();
    non_space_count >= 3
}

/// Check if a line is a numbered list item (e.g., "1. ", "10. ")
fn is_numbered_list_item(line: &str) -> bool {
    let mut chars = line.chars();

    // Must start with a digit
    if !chars.next().is_some_and(|c| c.is_numeric()) {
        return false;
    }

    // Can have more digits
    while let Some(c) = chars.next() {
        if c == '.' {
            // After period, must have a space or be end of line
            return chars.next().is_none_or(|c| c == ' ');
        }
        if !c.is_numeric() {
            return false;
        }
    }

    false
}

/// Check if a line ends with a hard break (either two spaces or backslash)
///
/// CommonMark supports two formats for hard line breaks:
/// 1. Two or more trailing spaces
/// 2. A backslash at the end of the line
fn has_hard_break(line: &str) -> bool {
    let line = line.strip_suffix('\r').unwrap_or(line);
    line.ends_with("  ") || line.ends_with('\\')
}

/// Trim trailing whitespace while preserving hard breaks (two trailing spaces or backslash)
///
/// Hard breaks in Markdown can be indicated by:
/// 1. Two trailing spaces before a newline (traditional)
/// 2. A backslash at the end of the line (mdformat style)
fn trim_preserving_hard_break(s: &str) -> String {
    // Strip trailing \r from CRLF line endings first to handle Windows files
    let s = s.strip_suffix('\r').unwrap_or(s);

    // Check for backslash hard break (mdformat style)
    if s.ends_with('\\') {
        // Preserve the backslash exactly as-is
        return s.to_string();
    }

    // Check if there are at least 2 trailing spaces (traditional hard break)
    if s.ends_with("  ") {
        // Find the position where non-space content ends
        let content_end = s.trim_end().len();
        if content_end == 0 {
            // String is all whitespace
            return String::new();
        }
        // Preserve exactly 2 trailing spaces for hard break
        format!("{}  ", &s[..content_end])
    } else {
        // No hard break, just trim all trailing whitespace
        s.trim_end().to_string()
    }
}

pub fn reflow_line(line: &str, options: &ReflowOptions) -> Vec<String> {
    // For sentence-per-line mode, always process regardless of length
    if options.sentence_per_line {
        let elements = parse_markdown_elements(line);
        return reflow_elements_sentence_per_line(&elements, &options.abbreviations);
    }

    // Quick check: if line is already short enough, return as-is
    if line.chars().count() <= options.line_length {
        return vec![line.to_string()];
    }

    // Parse the markdown to identify elements
    let elements = parse_markdown_elements(line);

    // Reflow the elements into lines
    reflow_elements(&elements, options)
}

/// Image source in a linked image structure
#[derive(Debug, Clone)]
enum LinkedImageSource {
    /// Inline image URL: ![alt](url)
    Inline(String),
    /// Reference image: ![alt][ref]
    Reference(String),
}

/// Link target in a linked image structure
#[derive(Debug, Clone)]
enum LinkedImageTarget {
    /// Inline link URL: ](url)
    Inline(String),
    /// Reference link: ][ref]
    Reference(String),
}

/// Represents a piece of content in the markdown
#[derive(Debug, Clone)]
enum Element {
    /// Plain text that can be wrapped
    Text(String),
    /// A complete markdown inline link [text](url)
    Link { text: String, url: String },
    /// A complete markdown reference link [text][ref]
    ReferenceLink { text: String, reference: String },
    /// A complete markdown empty reference link [text][]
    EmptyReferenceLink { text: String },
    /// A complete markdown shortcut reference link [ref]
    ShortcutReference { reference: String },
    /// A complete markdown inline image ![alt](url)
    InlineImage { alt: String, url: String },
    /// A complete markdown reference image ![alt][ref]
    ReferenceImage { alt: String, reference: String },
    /// A complete markdown empty reference image ![alt][]
    EmptyReferenceImage { alt: String },
    /// A clickable image badge in any of 4 forms:
    /// - [![alt](img-url)](link-url)
    /// - [![alt][img-ref]](link-url)
    /// - [![alt](img-url)][link-ref]
    /// - [![alt][img-ref]][link-ref]
    LinkedImage {
        alt: String,
        img_source: LinkedImageSource,
        link_target: LinkedImageTarget,
    },
    /// Footnote reference [^note]
    FootnoteReference { note: String },
    /// Strikethrough text ~~text~~
    Strikethrough(String),
    /// Wiki-style link [[wiki]] or [[wiki|text]]
    WikiLink(String),
    /// Inline math $math$
    InlineMath(String),
    /// Display math $$math$$
    DisplayMath(String),
    /// Emoji shortcode :emoji:
    EmojiShortcode(String),
    /// HTML tag <tag> or </tag> or <tag/>
    HtmlTag(String),
    /// HTML entity &nbsp; or &#123;
    HtmlEntity(String),
    /// Inline code `code`
    Code(String),
    /// Bold text **text**
    Bold(String),
    /// Italic text *text*
    Italic(String),
}

impl std::fmt::Display for Element {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Element::Text(s) => write!(f, "{s}"),
            Element::Link { text, url } => write!(f, "[{text}]({url})"),
            Element::ReferenceLink { text, reference } => write!(f, "[{text}][{reference}]"),
            Element::EmptyReferenceLink { text } => write!(f, "[{text}][]"),
            Element::ShortcutReference { reference } => write!(f, "[{reference}]"),
            Element::InlineImage { alt, url } => write!(f, "![{alt}]({url})"),
            Element::ReferenceImage { alt, reference } => write!(f, "![{alt}][{reference}]"),
            Element::EmptyReferenceImage { alt } => write!(f, "![{alt}][]"),
            Element::LinkedImage {
                alt,
                img_source,
                link_target,
            } => {
                // Build the image part: ![alt](url) or ![alt][ref]
                let img_part = match img_source {
                    LinkedImageSource::Inline(url) => format!("![{alt}]({url})"),
                    LinkedImageSource::Reference(r) => format!("![{alt}][{r}]"),
                };
                // Build the link part: (url) or [ref]
                match link_target {
                    LinkedImageTarget::Inline(url) => write!(f, "[{img_part}]({url})"),
                    LinkedImageTarget::Reference(r) => write!(f, "[{img_part}][{r}]"),
                }
            }
            Element::FootnoteReference { note } => write!(f, "[^{note}]"),
            Element::Strikethrough(s) => write!(f, "~~{s}~~"),
            Element::WikiLink(s) => write!(f, "[[{s}]]"),
            Element::InlineMath(s) => write!(f, "${s}$"),
            Element::DisplayMath(s) => write!(f, "$${s}$$"),
            Element::EmojiShortcode(s) => write!(f, ":{s}:"),
            Element::HtmlTag(s) => write!(f, "{s}"),
            Element::HtmlEntity(s) => write!(f, "{s}"),
            Element::Code(s) => write!(f, "`{s}`"),
            Element::Bold(s) => write!(f, "**{s}**"),
            Element::Italic(s) => write!(f, "*{s}*"),
        }
    }
}

impl Element {
    fn len(&self) -> usize {
        match self {
            Element::Text(s) => s.chars().count(),
            Element::Link { text, url } => text.chars().count() + url.chars().count() + 4, // [text](url)
            Element::ReferenceLink { text, reference } => text.chars().count() + reference.chars().count() + 4, // [text][ref]
            Element::EmptyReferenceLink { text } => text.chars().count() + 4, // [text][]
            Element::ShortcutReference { reference } => reference.chars().count() + 2, // [ref]
            Element::InlineImage { alt, url } => alt.chars().count() + url.chars().count() + 5, // ![alt](url)
            Element::ReferenceImage { alt, reference } => alt.chars().count() + reference.chars().count() + 5, // ![alt][ref]
            Element::EmptyReferenceImage { alt } => alt.chars().count() + 5, // ![alt][]
            Element::LinkedImage {
                alt,
                img_source,
                link_target,
            } => {
                // Calculate length based on variant
                // Base: [ + ![alt] + ] = 4 chars for outer brackets and !
                let alt_len = alt.chars().count();
                let img_len = match img_source {
                    LinkedImageSource::Inline(url) => url.chars().count() + 2, // (url)
                    LinkedImageSource::Reference(r) => r.chars().count() + 2,  // [ref]
                };
                let link_len = match link_target {
                    LinkedImageTarget::Inline(url) => url.chars().count() + 2, // (url)
                    LinkedImageTarget::Reference(r) => r.chars().count() + 2,  // [ref]
                };
                // [![alt](img)](link) = [ + ! + [ + alt + ] + (img) + ] + (link)
                //                     = 1 + 1 + 1 + alt + 1 + img_len + 1 + link_len = 5 + alt + img + link
                5 + alt_len + img_len + link_len
            }
            Element::FootnoteReference { note } => note.chars().count() + 3, // [^note]
            Element::Strikethrough(s) => s.chars().count() + 4,              // ~~text~~
            Element::WikiLink(s) => s.chars().count() + 4,                   // [[wiki]]
            Element::InlineMath(s) => s.chars().count() + 2,                 // $math$
            Element::DisplayMath(s) => s.chars().count() + 4,                // $$math$$
            Element::EmojiShortcode(s) => s.chars().count() + 2,             // :emoji:
            Element::HtmlTag(s) => s.chars().count(),                        // <tag> - already includes brackets
            Element::HtmlEntity(s) => s.chars().count(),                     // &nbsp; - already complete
            Element::Code(s) => s.chars().count() + 2,                       // `code`
            Element::Bold(s) => s.chars().count() + 4,                       // **text**
            Element::Italic(s) => s.chars().count() + 2,                     // *text*
        }
    }
}

/// Parse markdown elements from text preserving the raw syntax
///
/// Detection order is critical:
/// 1. Linked images [![alt](img)](link) - must be detected first as atomic units
/// 2. Inline images ![alt](url) - before links to handle ! prefix
/// 3. Reference images ![alt][ref] - before reference links
/// 4. Inline links [text](url) - before reference links
/// 5. Reference links [text][ref] - before shortcut references
/// 6. Shortcut reference links [ref] - detected last to avoid false positives
/// 7. Other elements (code, bold, italic, etc.) - processed normally
fn parse_markdown_elements(text: &str) -> Vec<Element> {
    let mut elements = Vec::new();
    let mut remaining = text;

    while !remaining.is_empty() {
        // Find the earliest occurrence of any markdown pattern
        let mut earliest_match: Option<(usize, &str, fancy_regex::Match)> = None;

        // Check for linked images FIRST (all 4 variants)
        // Quick literal check: only run expensive regexes if we might have a linked image
        // Pattern starts with "[!" so check for that first
        if remaining.contains("[!") {
            // Pattern 1: [![alt](img)](link) - inline image in inline link
            if let Ok(Some(m)) = LINKED_IMAGE_INLINE_INLINE.find(remaining)
                && earliest_match.as_ref().is_none_or(|(start, _, _)| m.start() < *start)
            {
                earliest_match = Some((m.start(), "linked_image_ii", m));
            }

            // Pattern 2: [![alt][ref]](link) - reference image in inline link
            if let Ok(Some(m)) = LINKED_IMAGE_REF_INLINE.find(remaining)
                && earliest_match.as_ref().is_none_or(|(start, _, _)| m.start() < *start)
            {
                earliest_match = Some((m.start(), "linked_image_ri", m));
            }

            // Pattern 3: [![alt](img)][ref] - inline image in reference link
            if let Ok(Some(m)) = LINKED_IMAGE_INLINE_REF.find(remaining)
                && earliest_match.as_ref().is_none_or(|(start, _, _)| m.start() < *start)
            {
                earliest_match = Some((m.start(), "linked_image_ir", m));
            }

            // Pattern 4: [![alt][ref]][ref] - reference image in reference link
            if let Ok(Some(m)) = LINKED_IMAGE_REF_REF.find(remaining)
                && earliest_match.as_ref().is_none_or(|(start, _, _)| m.start() < *start)
            {
                earliest_match = Some((m.start(), "linked_image_rr", m));
            }
        }

        // Check for images (they start with ! so should be detected before links)
        // Inline images - ![alt](url)
        if let Ok(Some(m)) = INLINE_IMAGE_FANCY_REGEX.find(remaining)
            && earliest_match.as_ref().is_none_or(|(start, _, _)| m.start() < *start)
        {
            earliest_match = Some((m.start(), "inline_image", m));
        }

        // Reference images - ![alt][ref]
        if let Ok(Some(m)) = REF_IMAGE_REGEX.find(remaining)
            && earliest_match.as_ref().is_none_or(|(start, _, _)| m.start() < *start)
        {
            earliest_match = Some((m.start(), "ref_image", m));
        }

        // Check for footnote references - [^note]
        if let Ok(Some(m)) = FOOTNOTE_REF_REGEX.find(remaining)
            && earliest_match.as_ref().is_none_or(|(start, _, _)| m.start() < *start)
        {
            earliest_match = Some((m.start(), "footnote_ref", m));
        }

        // Check for inline links - [text](url)
        if let Ok(Some(m)) = INLINE_LINK_FANCY_REGEX.find(remaining)
            && earliest_match.as_ref().is_none_or(|(start, _, _)| m.start() < *start)
        {
            earliest_match = Some((m.start(), "inline_link", m));
        }

        // Check for reference links - [text][ref]
        if let Ok(Some(m)) = REF_LINK_REGEX.find(remaining)
            && earliest_match.as_ref().is_none_or(|(start, _, _)| m.start() < *start)
        {
            earliest_match = Some((m.start(), "ref_link", m));
        }

        // Check for shortcut reference links - [ref]
        // Only check if we haven't found an earlier pattern that would conflict
        if let Ok(Some(m)) = SHORTCUT_REF_REGEX.find(remaining)
            && earliest_match.as_ref().is_none_or(|(start, _, _)| m.start() < *start)
        {
            earliest_match = Some((m.start(), "shortcut_ref", m));
        }

        // Check for wiki-style links - [[wiki]]
        if let Ok(Some(m)) = WIKI_LINK_REGEX.find(remaining)
            && earliest_match.as_ref().is_none_or(|(start, _, _)| m.start() < *start)
        {
            earliest_match = Some((m.start(), "wiki_link", m));
        }

        // Check for display math first (before inline) - $$math$$
        if let Ok(Some(m)) = DISPLAY_MATH_REGEX.find(remaining)
            && earliest_match.as_ref().is_none_or(|(start, _, _)| m.start() < *start)
        {
            earliest_match = Some((m.start(), "display_math", m));
        }

        // Check for inline math - $math$
        if let Ok(Some(m)) = INLINE_MATH_REGEX.find(remaining)
            && earliest_match.as_ref().is_none_or(|(start, _, _)| m.start() < *start)
        {
            earliest_match = Some((m.start(), "inline_math", m));
        }

        // Check for strikethrough - ~~text~~
        if let Ok(Some(m)) = STRIKETHROUGH_FANCY_REGEX.find(remaining)
            && earliest_match.as_ref().is_none_or(|(start, _, _)| m.start() < *start)
        {
            earliest_match = Some((m.start(), "strikethrough", m));
        }

        // Check for emoji shortcodes - :emoji:
        if let Ok(Some(m)) = EMOJI_SHORTCODE_REGEX.find(remaining)
            && earliest_match.as_ref().is_none_or(|(start, _, _)| m.start() < *start)
        {
            earliest_match = Some((m.start(), "emoji", m));
        }

        // Check for HTML entities - &nbsp; etc
        if let Ok(Some(m)) = HTML_ENTITY_REGEX.find(remaining)
            && earliest_match.as_ref().is_none_or(|(start, _, _)| m.start() < *start)
        {
            earliest_match = Some((m.start(), "html_entity", m));
        }

        // Check for HTML tags - <tag> </tag> <tag/>
        // But exclude autolinks like <https://...> or <mailto:...>
        if let Ok(Some(m)) = HTML_TAG_PATTERN.find(remaining)
            && earliest_match.as_ref().is_none_or(|(start, _, _)| m.start() < *start)
        {
            // Check if this is an autolink (starts with protocol or mailto:)
            let matched_text = &remaining[m.start()..m.end()];
            let is_autolink = matched_text.starts_with("<http://")
                || matched_text.starts_with("<https://")
                || matched_text.starts_with("<mailto:")
                || matched_text.starts_with("<ftp://")
                || matched_text.starts_with("<ftps://");

            if !is_autolink {
                earliest_match = Some((m.start(), "html_tag", m));
            }
        }

        // Find earliest non-link special characters
        let mut next_special = remaining.len();
        let mut special_type = "";

        if let Some(pos) = remaining.find('`')
            && pos < next_special
        {
            next_special = pos;
            special_type = "code";
        }
        if let Some(pos) = remaining.find("**")
            && pos < next_special
        {
            next_special = pos;
            special_type = "bold";
        }
        if let Some(pos) = remaining.find('*')
            && pos < next_special
            && !remaining[pos..].starts_with("**")
        {
            next_special = pos;
            special_type = "italic";
        }

        // Determine which pattern to process first
        let should_process_markdown_link = if let Some((pos, _, _)) = earliest_match {
            pos < next_special
        } else {
            false
        };

        if should_process_markdown_link {
            let (pos, pattern_type, match_obj) = earliest_match.unwrap();

            // Add any text before the match
            if pos > 0 {
                elements.push(Element::Text(remaining[..pos].to_string()));
            }

            // Process the matched pattern
            match pattern_type {
                // Pattern 1: [![alt](img)](link) - inline image in inline link
                "linked_image_ii" => {
                    if let Ok(Some(caps)) = LINKED_IMAGE_INLINE_INLINE.captures(remaining) {
                        let alt = caps.get(1).map(|m| m.as_str()).unwrap_or("");
                        let img_url = caps.get(2).map(|m| m.as_str()).unwrap_or("");
                        let link_url = caps.get(3).map(|m| m.as_str()).unwrap_or("");
                        elements.push(Element::LinkedImage {
                            alt: alt.to_string(),
                            img_source: LinkedImageSource::Inline(img_url.to_string()),
                            link_target: LinkedImageTarget::Inline(link_url.to_string()),
                        });
                        remaining = &remaining[match_obj.end()..];
                    } else {
                        elements.push(Element::Text("[".to_string()));
                        remaining = &remaining[1..];
                    }
                }
                // Pattern 2: [![alt][ref]](link) - reference image in inline link
                "linked_image_ri" => {
                    if let Ok(Some(caps)) = LINKED_IMAGE_REF_INLINE.captures(remaining) {
                        let alt = caps.get(1).map(|m| m.as_str()).unwrap_or("");
                        let img_ref = caps.get(2).map(|m| m.as_str()).unwrap_or("");
                        let link_url = caps.get(3).map(|m| m.as_str()).unwrap_or("");
                        elements.push(Element::LinkedImage {
                            alt: alt.to_string(),
                            img_source: LinkedImageSource::Reference(img_ref.to_string()),
                            link_target: LinkedImageTarget::Inline(link_url.to_string()),
                        });
                        remaining = &remaining[match_obj.end()..];
                    } else {
                        elements.push(Element::Text("[".to_string()));
                        remaining = &remaining[1..];
                    }
                }
                // Pattern 3: [![alt](img)][ref] - inline image in reference link
                "linked_image_ir" => {
                    if let Ok(Some(caps)) = LINKED_IMAGE_INLINE_REF.captures(remaining) {
                        let alt = caps.get(1).map(|m| m.as_str()).unwrap_or("");
                        let img_url = caps.get(2).map(|m| m.as_str()).unwrap_or("");
                        let link_ref = caps.get(3).map(|m| m.as_str()).unwrap_or("");
                        elements.push(Element::LinkedImage {
                            alt: alt.to_string(),
                            img_source: LinkedImageSource::Inline(img_url.to_string()),
                            link_target: LinkedImageTarget::Reference(link_ref.to_string()),
                        });
                        remaining = &remaining[match_obj.end()..];
                    } else {
                        elements.push(Element::Text("[".to_string()));
                        remaining = &remaining[1..];
                    }
                }
                // Pattern 4: [![alt][ref]][ref] - reference image in reference link
                "linked_image_rr" => {
                    if let Ok(Some(caps)) = LINKED_IMAGE_REF_REF.captures(remaining) {
                        let alt = caps.get(1).map(|m| m.as_str()).unwrap_or("");
                        let img_ref = caps.get(2).map(|m| m.as_str()).unwrap_or("");
                        let link_ref = caps.get(3).map(|m| m.as_str()).unwrap_or("");
                        elements.push(Element::LinkedImage {
                            alt: alt.to_string(),
                            img_source: LinkedImageSource::Reference(img_ref.to_string()),
                            link_target: LinkedImageTarget::Reference(link_ref.to_string()),
                        });
                        remaining = &remaining[match_obj.end()..];
                    } else {
                        elements.push(Element::Text("[".to_string()));
                        remaining = &remaining[1..];
                    }
                }
                "inline_image" => {
                    if let Ok(Some(caps)) = INLINE_IMAGE_FANCY_REGEX.captures(remaining) {
                        let alt = caps.get(1).map(|m| m.as_str()).unwrap_or("");
                        let url = caps.get(2).map(|m| m.as_str()).unwrap_or("");
                        elements.push(Element::InlineImage {
                            alt: alt.to_string(),
                            url: url.to_string(),
                        });
                        remaining = &remaining[match_obj.end()..];
                    } else {
                        elements.push(Element::Text("!".to_string()));
                        remaining = &remaining[1..];
                    }
                }
                "ref_image" => {
                    if let Ok(Some(caps)) = REF_IMAGE_REGEX.captures(remaining) {
                        let alt = caps.get(1).map(|m| m.as_str()).unwrap_or("");
                        let reference = caps.get(2).map(|m| m.as_str()).unwrap_or("");

                        if reference.is_empty() {
                            elements.push(Element::EmptyReferenceImage { alt: alt.to_string() });
                        } else {
                            elements.push(Element::ReferenceImage {
                                alt: alt.to_string(),
                                reference: reference.to_string(),
                            });
                        }
                        remaining = &remaining[match_obj.end()..];
                    } else {
                        elements.push(Element::Text("!".to_string()));
                        remaining = &remaining[1..];
                    }
                }
                "footnote_ref" => {
                    if let Ok(Some(caps)) = FOOTNOTE_REF_REGEX.captures(remaining) {
                        let note = caps.get(1).map(|m| m.as_str()).unwrap_or("");
                        elements.push(Element::FootnoteReference { note: note.to_string() });
                        remaining = &remaining[match_obj.end()..];
                    } else {
                        elements.push(Element::Text("[".to_string()));
                        remaining = &remaining[1..];
                    }
                }
                "inline_link" => {
                    if let Ok(Some(caps)) = INLINE_LINK_FANCY_REGEX.captures(remaining) {
                        let text = caps.get(1).map(|m| m.as_str()).unwrap_or("");
                        let url = caps.get(2).map(|m| m.as_str()).unwrap_or("");
                        elements.push(Element::Link {
                            text: text.to_string(),
                            url: url.to_string(),
                        });
                        remaining = &remaining[match_obj.end()..];
                    } else {
                        // Fallback - shouldn't happen
                        elements.push(Element::Text("[".to_string()));
                        remaining = &remaining[1..];
                    }
                }
                "ref_link" => {
                    if let Ok(Some(caps)) = REF_LINK_REGEX.captures(remaining) {
                        let text = caps.get(1).map(|m| m.as_str()).unwrap_or("");
                        let reference = caps.get(2).map(|m| m.as_str()).unwrap_or("");

                        if reference.is_empty() {
                            // Empty reference link [text][]
                            elements.push(Element::EmptyReferenceLink { text: text.to_string() });
                        } else {
                            // Regular reference link [text][ref]
                            elements.push(Element::ReferenceLink {
                                text: text.to_string(),
                                reference: reference.to_string(),
                            });
                        }
                        remaining = &remaining[match_obj.end()..];
                    } else {
                        // Fallback - shouldn't happen
                        elements.push(Element::Text("[".to_string()));
                        remaining = &remaining[1..];
                    }
                }
                "shortcut_ref" => {
                    if let Ok(Some(caps)) = SHORTCUT_REF_REGEX.captures(remaining) {
                        let reference = caps.get(1).map(|m| m.as_str()).unwrap_or("");
                        elements.push(Element::ShortcutReference {
                            reference: reference.to_string(),
                        });
                        remaining = &remaining[match_obj.end()..];
                    } else {
                        // Fallback - shouldn't happen
                        elements.push(Element::Text("[".to_string()));
                        remaining = &remaining[1..];
                    }
                }
                "wiki_link" => {
                    if let Ok(Some(caps)) = WIKI_LINK_REGEX.captures(remaining) {
                        let content = caps.get(1).map(|m| m.as_str()).unwrap_or("");
                        elements.push(Element::WikiLink(content.to_string()));
                        remaining = &remaining[match_obj.end()..];
                    } else {
                        elements.push(Element::Text("[[".to_string()));
                        remaining = &remaining[2..];
                    }
                }
                "display_math" => {
                    if let Ok(Some(caps)) = DISPLAY_MATH_REGEX.captures(remaining) {
                        let math = caps.get(1).map(|m| m.as_str()).unwrap_or("");
                        elements.push(Element::DisplayMath(math.to_string()));
                        remaining = &remaining[match_obj.end()..];
                    } else {
                        elements.push(Element::Text("$$".to_string()));
                        remaining = &remaining[2..];
                    }
                }
                "inline_math" => {
                    if let Ok(Some(caps)) = INLINE_MATH_REGEX.captures(remaining) {
                        let math = caps.get(1).map(|m| m.as_str()).unwrap_or("");
                        elements.push(Element::InlineMath(math.to_string()));
                        remaining = &remaining[match_obj.end()..];
                    } else {
                        elements.push(Element::Text("$".to_string()));
                        remaining = &remaining[1..];
                    }
                }
                "strikethrough" => {
                    if let Ok(Some(caps)) = STRIKETHROUGH_FANCY_REGEX.captures(remaining) {
                        let text = caps.get(1).map(|m| m.as_str()).unwrap_or("");
                        elements.push(Element::Strikethrough(text.to_string()));
                        remaining = &remaining[match_obj.end()..];
                    } else {
                        elements.push(Element::Text("~~".to_string()));
                        remaining = &remaining[2..];
                    }
                }
                "emoji" => {
                    if let Ok(Some(caps)) = EMOJI_SHORTCODE_REGEX.captures(remaining) {
                        let emoji = caps.get(1).map(|m| m.as_str()).unwrap_or("");
                        elements.push(Element::EmojiShortcode(emoji.to_string()));
                        remaining = &remaining[match_obj.end()..];
                    } else {
                        elements.push(Element::Text(":".to_string()));
                        remaining = &remaining[1..];
                    }
                }
                "html_entity" => {
                    // HTML entities are captured whole
                    elements.push(Element::HtmlEntity(remaining[..match_obj.end()].to_string()));
                    remaining = &remaining[match_obj.end()..];
                }
                "html_tag" => {
                    // HTML tags are captured whole
                    elements.push(Element::HtmlTag(remaining[..match_obj.end()].to_string()));
                    remaining = &remaining[match_obj.end()..];
                }
                _ => {
                    // Unknown pattern, treat as text
                    elements.push(Element::Text("[".to_string()));
                    remaining = &remaining[1..];
                }
            }
        } else {
            // Process non-link special characters

            // Add any text before the special character
            if next_special > 0 && next_special < remaining.len() {
                elements.push(Element::Text(remaining[..next_special].to_string()));
                remaining = &remaining[next_special..];
            }

            // Process the special element
            match special_type {
                "code" => {
                    // Find end of code
                    if let Some(code_end) = remaining[1..].find('`') {
                        let code = &remaining[1..1 + code_end];
                        elements.push(Element::Code(code.to_string()));
                        remaining = &remaining[1 + code_end + 1..];
                    } else {
                        // No closing backtick, treat as text
                        elements.push(Element::Text(remaining.to_string()));
                        break;
                    }
                }
                "bold" => {
                    // Check for bold text
                    if let Some(bold_end) = remaining[2..].find("**") {
                        let bold_text = &remaining[2..2 + bold_end];
                        elements.push(Element::Bold(bold_text.to_string()));
                        remaining = &remaining[2 + bold_end + 2..];
                    } else {
                        // No closing **, treat as text
                        elements.push(Element::Text("**".to_string()));
                        remaining = &remaining[2..];
                    }
                }
                "italic" => {
                    // Check for italic text
                    if let Some(italic_end) = remaining[1..].find('*') {
                        let italic_text = &remaining[1..1 + italic_end];
                        elements.push(Element::Italic(italic_text.to_string()));
                        remaining = &remaining[1 + italic_end + 1..];
                    } else {
                        // No closing *, treat as text
                        elements.push(Element::Text("*".to_string()));
                        remaining = &remaining[1..];
                    }
                }
                _ => {
                    // No special elements found, add all remaining text
                    elements.push(Element::Text(remaining.to_string()));
                    break;
                }
            }
        }
    }

    elements
}

/// Reflow elements for sentence-per-line mode
fn reflow_elements_sentence_per_line(elements: &[Element], custom_abbreviations: &Option<Vec<String>>) -> Vec<String> {
    let abbreviations = get_abbreviations(custom_abbreviations);
    let mut lines = Vec::new();
    let mut current_line = String::new();

    for element in elements.iter() {
        let element_str = format!("{element}");

        // For text elements, split into sentences
        if let Element::Text(text) = element {
            // Simply append text - it already has correct spacing from tokenization
            let combined = format!("{current_line}{text}");
            // Use the pre-computed abbreviations set to avoid redundant computation
            let sentences = split_into_sentences_with_set(&combined, &abbreviations);

            if sentences.len() > 1 {
                // We found sentence boundaries
                for (i, sentence) in sentences.iter().enumerate() {
                    if i == 0 {
                        // First sentence might continue from previous elements
                        // But check if it ends with an abbreviation
                        let trimmed = sentence.trim();

                        if text_ends_with_abbreviation(trimmed, &abbreviations) {
                            // Don't emit yet - this sentence ends with abbreviation, continue accumulating
                            current_line = sentence.to_string();
                        } else {
                            // Normal case - emit the first sentence
                            lines.push(sentence.to_string());
                            current_line.clear();
                        }
                    } else if i == sentences.len() - 1 {
                        // Last sentence: check if it's complete or incomplete
                        let trimmed = sentence.trim();
                        let ends_with_sentence_punct =
                            trimmed.ends_with('.') || trimmed.ends_with('!') || trimmed.ends_with('?');

                        if ends_with_sentence_punct && !text_ends_with_abbreviation(trimmed, &abbreviations) {
                            // Complete sentence - emit it immediately
                            lines.push(sentence.to_string());
                            current_line.clear();
                        } else {
                            // Incomplete sentence - save for next iteration
                            current_line = sentence.to_string();
                        }
                    } else {
                        // Complete sentences in the middle
                        lines.push(sentence.to_string());
                    }
                }
            } else {
                // No sentence boundary found, continue accumulating
                current_line = combined;
            }
        } else {
            // Non-text elements (Code, Bold, Italic, etc.)
            // Add space before element if needed (unless it's after an opening paren/bracket)
            if !current_line.is_empty()
                && !current_line.ends_with(' ')
                && !current_line.ends_with('(')
                && !current_line.ends_with('[')
            {
                current_line.push(' ');
            }
            current_line.push_str(&element_str);
        }
    }

    // Add any remaining content
    if !current_line.is_empty() {
        lines.push(current_line.trim().to_string());
    }
    lines
}

/// Reflow elements into lines that fit within the line length
fn reflow_elements(elements: &[Element], options: &ReflowOptions) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current_line = String::new();
    let mut current_length = 0;

    for element in elements {
        let element_str = format!("{element}");
        let element_len = element.len();

        // For text elements that might need breaking
        if let Element::Text(text) = element {
            // Check if original text had leading whitespace
            let has_leading_space = text.starts_with(char::is_whitespace);
            // If this is a text element, always process it word by word
            let words: Vec<&str> = text.split_whitespace().collect();

            for (i, word) in words.iter().enumerate() {
                let word_len = word.chars().count();
                // Check if this "word" is just punctuation that should stay attached
                let is_trailing_punct = word
                    .chars()
                    .all(|c| matches!(c, ',' | '.' | ':' | ';' | '!' | '?' | ')' | ']' | '}'));

                if current_length > 0 && current_length + 1 + word_len > options.line_length && !is_trailing_punct {
                    // Start a new line (but never for trailing punctuation)
                    lines.push(current_line.trim().to_string());
                    current_line = word.to_string();
                    current_length = word_len;
                } else {
                    // Add word to current line
                    // Only add space if: we have content AND (this isn't the first word OR original had leading space)
                    // AND this isn't trailing punctuation (which attaches directly)
                    if current_length > 0 && (i > 0 || has_leading_space) && !is_trailing_punct {
                        current_line.push(' ');
                        current_length += 1;
                    }
                    current_line.push_str(word);
                    current_length += word_len;
                }
            }
        } else {
            // For non-text elements (code, links, references), treat as atomic units
            // These should never be broken across lines
            if current_length > 0 && current_length + 1 + element_len > options.line_length {
                // Start a new line
                lines.push(current_line.trim().to_string());
                current_line = element_str;
                current_length = element_len;
            } else {
                // Add element to current line
                // Don't add space if the current line ends with an opening bracket/paren
                let ends_with_opener =
                    current_line.ends_with('(') || current_line.ends_with('[') || current_line.ends_with('{');
                if current_length > 0 && !ends_with_opener {
                    current_line.push(' ');
                    current_length += 1;
                }
                current_line.push_str(&element_str);
                current_length += element_len;
            }
        }
    }

    // Don't forget the last line
    if !current_line.is_empty() {
        lines.push(current_line.trim_end().to_string());
    }

    lines
}

/// Reflow markdown content preserving structure
pub fn reflow_markdown(content: &str, options: &ReflowOptions) -> String {
    let lines: Vec<&str> = content.lines().collect();
    let mut result = Vec::new();
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i];
        let trimmed = line.trim();

        // Preserve empty lines
        if trimmed.is_empty() {
            result.push(String::new());
            i += 1;
            continue;
        }

        // Preserve headings as-is
        if trimmed.starts_with('#') {
            result.push(line.to_string());
            i += 1;
            continue;
        }

        // Preserve fenced code blocks
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            result.push(line.to_string());
            i += 1;
            // Copy lines until closing fence
            while i < lines.len() {
                result.push(lines[i].to_string());
                if lines[i].trim().starts_with("```") || lines[i].trim().starts_with("~~~") {
                    i += 1;
                    break;
                }
                i += 1;
            }
            continue;
        }

        // Preserve indented code blocks (4+ columns accounting for tab expansion)
        if ElementCache::calculate_indentation_width_default(line) >= 4 {
            // Collect all consecutive indented lines
            result.push(line.to_string());
            i += 1;
            while i < lines.len() {
                let next_line = lines[i];
                // Continue if next line is also indented or empty (empty lines in code blocks are ok)
                if ElementCache::calculate_indentation_width_default(next_line) >= 4 || next_line.trim().is_empty() {
                    result.push(next_line.to_string());
                    i += 1;
                } else {
                    break;
                }
            }
            continue;
        }

        // Preserve block quotes (but reflow their content)
        if trimmed.starts_with('>') {
            let quote_prefix = line[0..line.find('>').unwrap() + 1].to_string();
            let quote_content = &line[quote_prefix.len()..].trim_start();

            let reflowed = reflow_line(quote_content, options);
            for reflowed_line in reflowed.iter() {
                result.push(format!("{quote_prefix} {reflowed_line}"));
            }
            i += 1;
            continue;
        }

        // Preserve horizontal rules first (before checking for lists)
        if is_horizontal_rule(trimmed) {
            result.push(line.to_string());
            i += 1;
            continue;
        }

        // Preserve lists (but not horizontal rules)
        if (trimmed.starts_with('-') && !is_horizontal_rule(trimmed))
            || (trimmed.starts_with('*') && !is_horizontal_rule(trimmed))
            || trimmed.starts_with('+')
            || is_numbered_list_item(trimmed)
        {
            // Find the list marker and preserve indentation
            let indent = line.len() - line.trim_start().len();
            let indent_str = " ".repeat(indent);

            // For numbered lists, find the period and the space after it
            // For bullet lists, find the marker and the space after it
            let mut marker_end = indent;
            let mut content_start = indent;

            if trimmed.chars().next().is_some_and(|c| c.is_numeric()) {
                // Numbered list: find the period
                if let Some(period_pos) = line[indent..].find('.') {
                    marker_end = indent + period_pos + 1; // Include the period
                    content_start = marker_end;
                    // Skip any spaces after the period to find content start
                    while content_start < line.len() && line.chars().nth(content_start) == Some(' ') {
                        content_start += 1;
                    }
                }
            } else {
                // Bullet list: marker is single character
                marker_end = indent + 1; // Just the marker character
                content_start = marker_end;
                // Skip any spaces after the marker
                while content_start < line.len() && line.chars().nth(content_start) == Some(' ') {
                    content_start += 1;
                }
            }

            let marker = &line[indent..marker_end];

            // Collect all content for this list item (including continuation lines)
            // Preserve hard breaks (2 trailing spaces) while trimming excessive whitespace
            let mut list_content = vec![trim_preserving_hard_break(&line[content_start..])];
            i += 1;

            // Collect continuation lines (indented lines that are part of this list item)
            while i < lines.len() {
                let next_line = lines[i];
                let next_trimmed = next_line.trim();

                // Stop if we hit an empty line or another list item or special block
                if next_trimmed.is_empty()
                    || next_trimmed.starts_with('#')
                    || next_trimmed.starts_with("```")
                    || next_trimmed.starts_with("~~~")
                    || next_trimmed.starts_with('>')
                    || next_trimmed.starts_with('|')
                    || (next_trimmed.starts_with('[') && next_line.contains("]:"))
                    || is_horizontal_rule(next_trimmed)
                    || (next_trimmed.starts_with('-')
                        && (next_trimmed.len() == 1 || next_trimmed.chars().nth(1) == Some(' ')))
                    || (next_trimmed.starts_with('*')
                        && (next_trimmed.len() == 1 || next_trimmed.chars().nth(1) == Some(' ')))
                    || (next_trimmed.starts_with('+')
                        && (next_trimmed.len() == 1 || next_trimmed.chars().nth(1) == Some(' ')))
                    || is_numbered_list_item(next_trimmed)
                    || is_definition_list_item(next_trimmed)
                {
                    break;
                }

                // Check if this line is indented (continuation of list item)
                let next_indent = next_line.len() - next_line.trim_start().len();
                if next_indent >= content_start {
                    // This is a continuation line - add its content
                    // Preserve hard breaks while trimming excessive whitespace
                    let trimmed_start = next_line.trim_start();
                    list_content.push(trim_preserving_hard_break(trimmed_start));
                    i += 1;
                } else {
                    // Not indented enough, not part of this list item
                    break;
                }
            }

            // Join content, but respect hard breaks (lines ending with 2 spaces or backslash)
            // Hard breaks should prevent joining with the next line
            let combined_content = if options.preserve_breaks {
                list_content[0].clone()
            } else {
                // Check if any lines have hard breaks - if so, preserve the structure
                let has_hard_breaks = list_content.iter().any(|line| has_hard_break(line));
                if has_hard_breaks {
                    // Don't join lines with hard breaks - keep them separate with newlines
                    list_content.join("\n")
                } else {
                    // No hard breaks, safe to join with spaces
                    list_content.join(" ")
                }
            };

            // Calculate the proper indentation for continuation lines
            let trimmed_marker = marker;
            let continuation_spaces = content_start;

            // Adjust line length to account for list marker and space
            let prefix_length = indent + trimmed_marker.len() + 1;

            // Create adjusted options with reduced line length
            let adjusted_options = ReflowOptions {
                line_length: options.line_length.saturating_sub(prefix_length),
                ..options.clone()
            };

            let reflowed = reflow_line(&combined_content, &adjusted_options);
            for (j, reflowed_line) in reflowed.iter().enumerate() {
                if j == 0 {
                    result.push(format!("{indent_str}{trimmed_marker} {reflowed_line}"));
                } else {
                    // Continuation lines aligned with text after marker
                    let continuation_indent = " ".repeat(continuation_spaces);
                    result.push(format!("{continuation_indent}{reflowed_line}"));
                }
            }
            continue;
        }

        // Preserve tables
        if crate::utils::table_utils::TableUtils::is_potential_table_row(line) {
            result.push(line.to_string());
            i += 1;
            continue;
        }

        // Preserve reference definitions
        if trimmed.starts_with('[') && line.contains("]:") {
            result.push(line.to_string());
            i += 1;
            continue;
        }

        // Preserve definition list items (extended markdown)
        if is_definition_list_item(trimmed) {
            result.push(line.to_string());
            i += 1;
            continue;
        }

        // Check if this is a single line that doesn't need processing
        let mut is_single_line_paragraph = true;
        if i + 1 < lines.len() {
            let next_line = lines[i + 1];
            let next_trimmed = next_line.trim();
            // Check if next line starts a new block
            if !next_trimmed.is_empty()
                && !next_trimmed.starts_with('#')
                && !next_trimmed.starts_with("```")
                && !next_trimmed.starts_with("~~~")
                && !next_trimmed.starts_with('>')
                && !next_trimmed.starts_with('|')
                && !(next_trimmed.starts_with('[') && next_line.contains("]:"))
                && !is_horizontal_rule(next_trimmed)
                && !(next_trimmed.starts_with('-')
                    && !is_horizontal_rule(next_trimmed)
                    && (next_trimmed.len() == 1 || next_trimmed.chars().nth(1) == Some(' ')))
                && !(next_trimmed.starts_with('*')
                    && !is_horizontal_rule(next_trimmed)
                    && (next_trimmed.len() == 1 || next_trimmed.chars().nth(1) == Some(' ')))
                && !(next_trimmed.starts_with('+')
                    && (next_trimmed.len() == 1 || next_trimmed.chars().nth(1) == Some(' ')))
                && !is_numbered_list_item(next_trimmed)
            {
                is_single_line_paragraph = false;
            }
        }

        // If it's a single line that fits, just add it as-is
        if is_single_line_paragraph && line.chars().count() <= options.line_length {
            result.push(line.to_string());
            i += 1;
            continue;
        }

        // For regular paragraphs, collect consecutive lines
        let mut paragraph_parts = Vec::new();
        let mut current_part = vec![line];
        i += 1;

        // If preserve_breaks is true, treat each line separately
        if options.preserve_breaks {
            // Don't collect consecutive lines - just reflow this single line
            let hard_break_type = if line.strip_suffix('\r').unwrap_or(line).ends_with('\\') {
                Some("\\")
            } else if line.ends_with("  ") {
                Some("  ")
            } else {
                None
            };
            let reflowed = reflow_line(line, options);

            // Preserve hard breaks (two trailing spaces or backslash)
            if let Some(break_marker) = hard_break_type {
                if !reflowed.is_empty() {
                    let mut reflowed_with_break = reflowed;
                    let last_idx = reflowed_with_break.len() - 1;
                    if !has_hard_break(&reflowed_with_break[last_idx]) {
                        reflowed_with_break[last_idx].push_str(break_marker);
                    }
                    result.extend(reflowed_with_break);
                }
            } else {
                result.extend(reflowed);
            }
        } else {
            // Original behavior: collect consecutive lines into a paragraph
            while i < lines.len() {
                let prev_line = if !current_part.is_empty() {
                    current_part.last().unwrap()
                } else {
                    ""
                };
                let next_line = lines[i];
                let next_trimmed = next_line.trim();

                // Stop at empty lines or special blocks
                if next_trimmed.is_empty()
                    || next_trimmed.starts_with('#')
                    || next_trimmed.starts_with("```")
                    || next_trimmed.starts_with("~~~")
                    || next_trimmed.starts_with('>')
                    || next_trimmed.starts_with('|')
                    || (next_trimmed.starts_with('[') && next_line.contains("]:"))
                    || is_horizontal_rule(next_trimmed)
                    || (next_trimmed.starts_with('-')
                        && !is_horizontal_rule(next_trimmed)
                        && (next_trimmed.len() == 1 || next_trimmed.chars().nth(1) == Some(' ')))
                    || (next_trimmed.starts_with('*')
                        && !is_horizontal_rule(next_trimmed)
                        && (next_trimmed.len() == 1 || next_trimmed.chars().nth(1) == Some(' ')))
                    || (next_trimmed.starts_with('+')
                        && (next_trimmed.len() == 1 || next_trimmed.chars().nth(1) == Some(' ')))
                    || is_numbered_list_item(next_trimmed)
                    || is_definition_list_item(next_trimmed)
                {
                    break;
                }

                // Check if previous line ends with hard break (two spaces or backslash)
                if has_hard_break(prev_line) {
                    // Start a new part after hard break
                    paragraph_parts.push(current_part.join(" "));
                    current_part = vec![next_line];
                } else {
                    current_part.push(next_line);
                }
                i += 1;
            }

            // Add the last part
            if !current_part.is_empty() {
                if current_part.len() == 1 {
                    // Single line, don't add trailing space
                    paragraph_parts.push(current_part[0].to_string());
                } else {
                    paragraph_parts.push(current_part.join(" "));
                }
            }

            // Reflow each part separately, preserving hard breaks
            for (j, part) in paragraph_parts.iter().enumerate() {
                let reflowed = reflow_line(part, options);
                result.extend(reflowed);

                // Preserve hard break by ensuring last line of part ends with hard break marker
                // Use two spaces as the default hard break format for reflows
                if j < paragraph_parts.len() - 1 && !result.is_empty() {
                    let last_idx = result.len() - 1;
                    if !has_hard_break(&result[last_idx]) {
                        result[last_idx].push_str("  ");
                    }
                }
            }
        }
    }

    // Preserve trailing newline if the original content had one
    let result_text = result.join("\n");
    if content.ends_with('\n') && !result_text.ends_with('\n') {
        format!("{result_text}\n")
    } else {
        result_text
    }
}

/// Information about a reflowed paragraph
#[derive(Debug, Clone)]
pub struct ParagraphReflow {
    /// Starting byte offset of the paragraph in the original content
    pub start_byte: usize,
    /// Ending byte offset of the paragraph in the original content
    pub end_byte: usize,
    /// The reflowed text for this paragraph
    pub reflowed_text: String,
}

/// Reflow a single paragraph at the specified line number
///
/// This function finds the paragraph containing the given line number,
/// reflows it according to the specified line length, and returns
/// information about the paragraph location and its reflowed text.
///
/// # Arguments
///
/// * `content` - The full document content
/// * `line_number` - The 1-based line number within the paragraph to reflow
/// * `line_length` - The target line length for reflowing
///
/// # Returns
///
/// Returns `Some(ParagraphReflow)` if a paragraph was found and reflowed,
/// or `None` if the line number is out of bounds or the content at that
/// line shouldn't be reflowed (e.g., code blocks, headings, etc.)
pub fn reflow_paragraph_at_line(content: &str, line_number: usize, line_length: usize) -> Option<ParagraphReflow> {
    if line_number == 0 {
        return None;
    }

    let lines: Vec<&str> = content.lines().collect();

    // Check if line number is valid (1-based)
    if line_number > lines.len() {
        return None;
    }

    let target_idx = line_number - 1; // Convert to 0-based
    let target_line = lines[target_idx];
    let trimmed = target_line.trim();

    // Don't reflow special blocks
    if trimmed.is_empty()
        || trimmed.starts_with('#')
        || trimmed.starts_with("```")
        || trimmed.starts_with("~~~")
        || ElementCache::calculate_indentation_width_default(target_line) >= 4
        || trimmed.starts_with('>')
        || crate::utils::table_utils::TableUtils::is_potential_table_row(target_line) // Tables
        || (trimmed.starts_with('[') && target_line.contains("]:")) // Reference definitions
        || is_horizontal_rule(trimmed)
        || ((trimmed.starts_with('-') || trimmed.starts_with('*') || trimmed.starts_with('+'))
            && !is_horizontal_rule(trimmed)
            && (trimmed.len() == 1 || trimmed.chars().nth(1) == Some(' ')))
        || is_numbered_list_item(trimmed)
        || is_definition_list_item(trimmed)
    {
        return None;
    }

    // Find paragraph start - scan backward until blank line or special block
    let mut para_start = target_idx;
    while para_start > 0 {
        let prev_idx = para_start - 1;
        let prev_line = lines[prev_idx];
        let prev_trimmed = prev_line.trim();

        // Stop at blank line or special blocks
        if prev_trimmed.is_empty()
            || prev_trimmed.starts_with('#')
            || prev_trimmed.starts_with("```")
            || prev_trimmed.starts_with("~~~")
            || ElementCache::calculate_indentation_width_default(prev_line) >= 4
            || prev_trimmed.starts_with('>')
            || crate::utils::table_utils::TableUtils::is_potential_table_row(prev_line)
            || (prev_trimmed.starts_with('[') && prev_line.contains("]:"))
            || is_horizontal_rule(prev_trimmed)
            || ((prev_trimmed.starts_with('-') || prev_trimmed.starts_with('*') || prev_trimmed.starts_with('+'))
                && !is_horizontal_rule(prev_trimmed)
                && (prev_trimmed.len() == 1 || prev_trimmed.chars().nth(1) == Some(' ')))
            || is_numbered_list_item(prev_trimmed)
            || is_definition_list_item(prev_trimmed)
        {
            break;
        }

        para_start = prev_idx;
    }

    // Find paragraph end - scan forward until blank line or special block
    let mut para_end = target_idx;
    while para_end + 1 < lines.len() {
        let next_idx = para_end + 1;
        let next_line = lines[next_idx];
        let next_trimmed = next_line.trim();

        // Stop at blank line or special blocks
        if next_trimmed.is_empty()
            || next_trimmed.starts_with('#')
            || next_trimmed.starts_with("```")
            || next_trimmed.starts_with("~~~")
            || ElementCache::calculate_indentation_width_default(next_line) >= 4
            || next_trimmed.starts_with('>')
            || crate::utils::table_utils::TableUtils::is_potential_table_row(next_line)
            || (next_trimmed.starts_with('[') && next_line.contains("]:"))
            || is_horizontal_rule(next_trimmed)
            || ((next_trimmed.starts_with('-') || next_trimmed.starts_with('*') || next_trimmed.starts_with('+'))
                && !is_horizontal_rule(next_trimmed)
                && (next_trimmed.len() == 1 || next_trimmed.chars().nth(1) == Some(' ')))
            || is_numbered_list_item(next_trimmed)
            || is_definition_list_item(next_trimmed)
        {
            break;
        }

        para_end = next_idx;
    }

    // Extract paragraph lines
    let paragraph_lines = &lines[para_start..=para_end];

    // Calculate byte offsets
    let mut start_byte = 0;
    for line in lines.iter().take(para_start) {
        start_byte += line.len() + 1; // +1 for newline
    }

    let mut end_byte = start_byte;
    for line in paragraph_lines.iter() {
        end_byte += line.len() + 1; // +1 for newline
    }

    // Track whether the byte range includes a trailing newline
    // (it doesn't if this is the last line and the file doesn't end with newline)
    let includes_trailing_newline = para_end != lines.len() - 1 || content.ends_with('\n');

    // Adjust end_byte if the last line doesn't have a newline
    if !includes_trailing_newline {
        end_byte -= 1;
    }

    // Join paragraph lines and reflow
    let paragraph_text = paragraph_lines.join("\n");

    // Create reflow options
    let options = ReflowOptions {
        line_length,
        break_on_sentences: true,
        preserve_breaks: false,
        sentence_per_line: false,
        abbreviations: None,
    };

    // Reflow the paragraph using reflow_markdown to handle it properly
    let reflowed = reflow_markdown(&paragraph_text, &options);

    // Ensure reflowed text matches whether the byte range includes a trailing newline
    // This is critical: if the range includes a newline, the replacement must too,
    // otherwise the next line will get appended to the reflowed paragraph
    let reflowed_text = if includes_trailing_newline {
        // Range includes newline - ensure reflowed text has one
        if reflowed.ends_with('\n') {
            reflowed
        } else {
            format!("{reflowed}\n")
        }
    } else {
        // Range doesn't include newline - ensure reflowed text doesn't have one
        if reflowed.ends_with('\n') {
            reflowed.trim_end_matches('\n').to_string()
        } else {
            reflowed
        }
    };

    Some(ParagraphReflow {
        start_byte,
        end_byte,
        reflowed_text,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Unit test for private helper function text_ends_with_abbreviation()
    ///
    /// This test stays inline because it tests a private function.
    /// All other tests (public API, integration tests) are in tests/utils/text_reflow_test.rs
    #[test]
    fn test_helper_function_text_ends_with_abbreviation() {
        // Test the helper function directly
        let abbreviations = get_abbreviations(&None);

        // True cases - built-in abbreviations (titles and i.e./e.g.)
        assert!(text_ends_with_abbreviation("Dr.", &abbreviations));
        assert!(text_ends_with_abbreviation("word Dr.", &abbreviations));
        assert!(text_ends_with_abbreviation("e.g.", &abbreviations));
        assert!(text_ends_with_abbreviation("i.e.", &abbreviations));
        assert!(text_ends_with_abbreviation("Mr.", &abbreviations));
        assert!(text_ends_with_abbreviation("Mrs.", &abbreviations));
        assert!(text_ends_with_abbreviation("Ms.", &abbreviations));
        assert!(text_ends_with_abbreviation("Prof.", &abbreviations));

        // False cases - NOT in built-in list (etc doesn't always have period)
        assert!(!text_ends_with_abbreviation("etc.", &abbreviations));
        assert!(!text_ends_with_abbreviation("paradigms.", &abbreviations));
        assert!(!text_ends_with_abbreviation("programs.", &abbreviations));
        assert!(!text_ends_with_abbreviation("items.", &abbreviations));
        assert!(!text_ends_with_abbreviation("systems.", &abbreviations));
        assert!(!text_ends_with_abbreviation("Dr?", &abbreviations)); // question mark, not period
        assert!(!text_ends_with_abbreviation("Mr!", &abbreviations)); // exclamation, not period
        assert!(!text_ends_with_abbreviation("paradigms?", &abbreviations)); // question mark
        assert!(!text_ends_with_abbreviation("word", &abbreviations)); // no punctuation
        assert!(!text_ends_with_abbreviation("", &abbreviations)); // empty string
    }
}
