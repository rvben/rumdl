//! Text reflow utilities for MD013
//!
//! This module implements text wrapping/reflow functionality that preserves
//! Markdown elements like links, emphasis, code spans, etc.

use crate::utils::element_cache::ElementCache;
use crate::utils::is_definition_list_item;
use crate::utils::regex_cache::{
    DISPLAY_MATH_REGEX, EMAIL_PATTERN, EMOJI_SHORTCODE_REGEX, FOOTNOTE_REF_REGEX, HTML_ENTITY_REGEX, HTML_TAG_PATTERN,
    HUGO_SHORTCODE_REGEX, INLINE_IMAGE_FANCY_REGEX, INLINE_LINK_FANCY_REGEX, INLINE_MATH_REGEX,
    LINKED_IMAGE_INLINE_INLINE, LINKED_IMAGE_INLINE_REF, LINKED_IMAGE_REF_INLINE, LINKED_IMAGE_REF_REF,
    REF_IMAGE_REGEX, REF_LINK_REGEX, SHORTCUT_REF_REGEX, WIKI_LINK_REGEX,
};
use crate::utils::sentence_utils::{
    get_abbreviations, is_cjk_char, is_cjk_sentence_ending, is_closing_quote, is_opening_quote,
    text_ends_with_abbreviation,
};
use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};
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

/// Detect if a character position is a sentence boundary
/// Based on the approach from github.com/JoshuaKGoldberg/sentences-per-line
/// Supports both ASCII punctuation (. ! ?) and CJK punctuation (。 ！ ？)
fn is_sentence_boundary(text: &str, pos: usize, abbreviations: &HashSet<String>) -> bool {
    let chars: Vec<char> = text.chars().collect();

    if pos + 1 >= chars.len() {
        return false;
    }

    let c = chars[pos];
    let next_char = chars[pos + 1];

    // Check for CJK sentence-ending punctuation (。, ！, ？)
    // CJK punctuation doesn't require space or uppercase after it
    if is_cjk_sentence_ending(c) {
        // Skip any trailing emphasis/strikethrough markers
        let mut after_punct_pos = pos + 1;
        while after_punct_pos < chars.len()
            && (chars[after_punct_pos] == '*' || chars[after_punct_pos] == '_' || chars[after_punct_pos] == '~')
        {
            after_punct_pos += 1;
        }

        // Skip whitespace
        while after_punct_pos < chars.len() && chars[after_punct_pos].is_whitespace() {
            after_punct_pos += 1;
        }

        // Check if we have more content (any non-whitespace)
        if after_punct_pos >= chars.len() {
            return false;
        }

        // Skip leading emphasis/strikethrough markers
        while after_punct_pos < chars.len()
            && (chars[after_punct_pos] == '*' || chars[after_punct_pos] == '_' || chars[after_punct_pos] == '~')
        {
            after_punct_pos += 1;
        }

        if after_punct_pos >= chars.len() {
            return false;
        }

        // For CJK, we accept any character as the start of the next sentence
        // (no uppercase requirement, since CJK doesn't have case)
        return true;
    }

    // Check for ASCII sentence-ending punctuation
    if c != '.' && c != '!' && c != '?' {
        return false;
    }

    // Must be followed by space, closing quote, or emphasis/strikethrough marker followed by space
    let (_space_pos, after_space_pos) = if next_char == ' ' {
        // Normal case: punctuation followed by space
        (pos + 1, pos + 2)
    } else if is_closing_quote(next_char) && pos + 2 < chars.len() {
        // Sentence ends with quote - check what follows the quote
        if chars[pos + 2] == ' ' {
            // Just quote followed by space: 'sentence." '
            (pos + 2, pos + 3)
        } else if (chars[pos + 2] == '*' || chars[pos + 2] == '_') && pos + 3 < chars.len() && chars[pos + 3] == ' ' {
            // Quote followed by emphasis: 'sentence."* '
            (pos + 3, pos + 4)
        } else if (chars[pos + 2] == '*' || chars[pos + 2] == '_')
            && pos + 4 < chars.len()
            && chars[pos + 3] == chars[pos + 2]
            && chars[pos + 4] == ' '
        {
            // Quote followed by bold: 'sentence."** '
            (pos + 4, pos + 5)
        } else {
            return false;
        }
    } else if (next_char == '*' || next_char == '_') && pos + 2 < chars.len() && chars[pos + 2] == ' ' {
        // Sentence ends with emphasis: "sentence.* " or "sentence._ "
        (pos + 2, pos + 3)
    } else if (next_char == '*' || next_char == '_')
        && pos + 3 < chars.len()
        && chars[pos + 2] == next_char
        && chars[pos + 3] == ' '
    {
        // Sentence ends with bold: "sentence.** " or "sentence.__ "
        (pos + 3, pos + 4)
    } else if next_char == '~' && pos + 3 < chars.len() && chars[pos + 2] == '~' && chars[pos + 3] == ' ' {
        // Sentence ends with strikethrough: "sentence.~~ "
        (pos + 3, pos + 4)
    } else {
        return false;
    };

    // Skip all whitespace after the space to find the start of the next sentence
    let mut next_char_pos = after_space_pos;
    while next_char_pos < chars.len() && chars[next_char_pos].is_whitespace() {
        next_char_pos += 1;
    }

    // Check if we reached the end of the string
    if next_char_pos >= chars.len() {
        return false;
    }

    // Skip leading emphasis/strikethrough markers and opening quotes to find the actual first letter
    let mut first_letter_pos = next_char_pos;
    while first_letter_pos < chars.len()
        && (chars[first_letter_pos] == '*'
            || chars[first_letter_pos] == '_'
            || chars[first_letter_pos] == '~'
            || is_opening_quote(chars[first_letter_pos]))
    {
        first_letter_pos += 1;
    }

    // Check if we reached the end after skipping emphasis
    if first_letter_pos >= chars.len() {
        return false;
    }

    // First character of next sentence must be uppercase or CJK
    let first_char = chars[first_letter_pos];
    if !first_char.is_uppercase() && !is_cjk_char(first_char) {
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
        // Make sure to check if first_letter_pos is within bounds
        if chars[pos - 1].is_numeric() && first_letter_pos < chars.len() && chars[first_letter_pos].is_numeric() {
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
            // Consume any trailing emphasis/strikethrough markers and quotes (they belong to the current sentence)
            while let Some(&next) = chars.peek() {
                if next == '*' || next == '_' || next == '~' || is_closing_quote(next) {
                    current_sentence.push(chars.next().unwrap());
                    pos += 1;
                } else {
                    break;
                }
            }

            // Consume the space after the sentence
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
            // After period, must have a space (consistent with list marker extraction)
            // "2019." alone is NOT treated as a list item to avoid false positives
            return chars.next() == Some(' ');
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

    // Quick check: if line is already short enough or no wrapping requested, return as-is
    // line_length = 0 means no wrapping (unlimited line length)
    if options.line_length == 0 || line.chars().count() <= options.line_length {
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
    /// Hugo/Go template shortcode {{< ... >}} or {{% ... %}}
    HugoShortcode(String),
    /// Inline code `code`
    Code(String),
    /// Bold text **text** or __text__
    Bold {
        content: String,
        /// True if underscore markers (__), false for asterisks (**)
        underscore: bool,
    },
    /// Italic text *text* or _text_
    Italic {
        content: String,
        /// True if underscore marker (_), false for asterisk (*)
        underscore: bool,
    },
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
            Element::HugoShortcode(s) => write!(f, "{s}"),
            Element::Code(s) => write!(f, "`{s}`"),
            Element::Bold { content, underscore } => {
                if *underscore {
                    write!(f, "__{content}__")
                } else {
                    write!(f, "**{content}**")
                }
            }
            Element::Italic { content, underscore } => {
                if *underscore {
                    write!(f, "_{content}_")
                } else {
                    write!(f, "*{content}*")
                }
            }
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
            Element::HugoShortcode(s) => s.chars().count(),                  // {{< ... >}} - already complete
            Element::Code(s) => s.chars().count() + 2,                       // `code`
            Element::Bold { content, .. } => content.chars().count() + 4,    // **text** or __text__
            Element::Italic { content, .. } => content.chars().count() + 2,  // *text* or _text_
        }
    }
}

/// An emphasis or formatting span parsed by pulldown-cmark
#[derive(Debug, Clone)]
struct EmphasisSpan {
    /// Byte offset where the emphasis starts (including markers)
    start: usize,
    /// Byte offset where the emphasis ends (after closing markers)
    end: usize,
    /// The content inside the emphasis markers
    content: String,
    /// Whether this is strong (bold) emphasis
    is_strong: bool,
    /// Whether this is strikethrough (~~text~~)
    is_strikethrough: bool,
    /// Whether the original used underscore markers (for emphasis only)
    uses_underscore: bool,
}

/// Extract emphasis and strikethrough spans from text using pulldown-cmark
///
/// This provides CommonMark-compliant emphasis parsing, correctly handling:
/// - Nested emphasis like `*text **bold** more*`
/// - Left/right flanking delimiter rules
/// - Underscore vs asterisk markers
/// - GFM strikethrough (~~text~~)
///
/// Returns spans sorted by start position.
fn extract_emphasis_spans(text: &str) -> Vec<EmphasisSpan> {
    let mut spans = Vec::new();
    let mut options = Options::empty();
    options.insert(Options::ENABLE_STRIKETHROUGH);

    // Stacks to track nested formatting with their start positions
    let mut emphasis_stack: Vec<(usize, bool)> = Vec::new(); // (start_byte, uses_underscore)
    let mut strong_stack: Vec<(usize, bool)> = Vec::new();
    let mut strikethrough_stack: Vec<usize> = Vec::new();

    let parser = Parser::new_ext(text, options).into_offset_iter();

    for (event, range) in parser {
        match event {
            Event::Start(Tag::Emphasis) => {
                // Check if this uses underscore by looking at the original text
                let uses_underscore = text.get(range.start..range.start + 1) == Some("_");
                emphasis_stack.push((range.start, uses_underscore));
            }
            Event::End(TagEnd::Emphasis) => {
                if let Some((start_byte, uses_underscore)) = emphasis_stack.pop() {
                    // Extract content between the markers (1 char marker on each side)
                    let content_start = start_byte + 1;
                    let content_end = range.end - 1;
                    if content_end > content_start
                        && let Some(content) = text.get(content_start..content_end)
                    {
                        spans.push(EmphasisSpan {
                            start: start_byte,
                            end: range.end,
                            content: content.to_string(),
                            is_strong: false,
                            is_strikethrough: false,
                            uses_underscore,
                        });
                    }
                }
            }
            Event::Start(Tag::Strong) => {
                // Check if this uses underscore by looking at the original text
                let uses_underscore = text.get(range.start..range.start + 2) == Some("__");
                strong_stack.push((range.start, uses_underscore));
            }
            Event::End(TagEnd::Strong) => {
                if let Some((start_byte, uses_underscore)) = strong_stack.pop() {
                    // Extract content between the markers (2 char marker on each side)
                    let content_start = start_byte + 2;
                    let content_end = range.end - 2;
                    if content_end > content_start
                        && let Some(content) = text.get(content_start..content_end)
                    {
                        spans.push(EmphasisSpan {
                            start: start_byte,
                            end: range.end,
                            content: content.to_string(),
                            is_strong: true,
                            is_strikethrough: false,
                            uses_underscore,
                        });
                    }
                }
            }
            Event::Start(Tag::Strikethrough) => {
                strikethrough_stack.push(range.start);
            }
            Event::End(TagEnd::Strikethrough) => {
                if let Some(start_byte) = strikethrough_stack.pop() {
                    // Extract content between the ~~ markers (2 char marker on each side)
                    let content_start = start_byte + 2;
                    let content_end = range.end - 2;
                    if content_end > content_start
                        && let Some(content) = text.get(content_start..content_end)
                    {
                        spans.push(EmphasisSpan {
                            start: start_byte,
                            end: range.end,
                            content: content.to_string(),
                            is_strong: false,
                            is_strikethrough: true,
                            uses_underscore: false,
                        });
                    }
                }
            }
            _ => {}
        }
    }

    // Sort by start position
    spans.sort_by_key(|s| s.start);
    spans
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

    // Pre-extract emphasis spans using pulldown-cmark for CommonMark-compliant parsing
    let emphasis_spans = extract_emphasis_spans(text);

    while !remaining.is_empty() {
        // Calculate current byte offset in original text
        let current_offset = text.len() - remaining.len();
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

        // Note: Strikethrough is now handled by pulldown-cmark in extract_emphasis_spans

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

        // Check for Hugo shortcodes - {{< ... >}} or {{% ... %}}
        // Must be checked before other patterns to avoid false sentence breaks
        if let Ok(Some(m)) = HUGO_SHORTCODE_REGEX.find(remaining)
            && earliest_match.as_ref().is_none_or(|(start, _, _)| m.start() < *start)
        {
            earliest_match = Some((m.start(), "hugo_shortcode", m));
        }

        // Check for HTML tags - <tag> </tag> <tag/>
        // But exclude autolinks like <https://...> or <mailto:...> or email autolinks <user@domain.com>
        if let Ok(Some(m)) = HTML_TAG_PATTERN.find(remaining)
            && earliest_match.as_ref().is_none_or(|(start, _, _)| m.start() < *start)
        {
            // Check if this is an autolink (starts with protocol or mailto:)
            let matched_text = &remaining[m.start()..m.end()];
            let is_url_autolink = matched_text.starts_with("<http://")
                || matched_text.starts_with("<https://")
                || matched_text.starts_with("<mailto:")
                || matched_text.starts_with("<ftp://")
                || matched_text.starts_with("<ftps://");

            // Check if this is an email autolink (per CommonMark spec: <local@domain.tld>)
            // Use centralized EMAIL_PATTERN for consistency with MD034 and other rules
            let is_email_autolink = {
                let content = matched_text.trim_start_matches('<').trim_end_matches('>');
                EMAIL_PATTERN.is_match(content)
            };

            if !is_url_autolink && !is_email_autolink {
                earliest_match = Some((m.start(), "html_tag", m));
            }
        }

        // Find earliest non-link special characters
        let mut next_special = remaining.len();
        let mut special_type = "";
        let mut pulldown_emphasis: Option<&EmphasisSpan> = None;

        // Check for code spans (not handled by pulldown-cmark in this context)
        if let Some(pos) = remaining.find('`')
            && pos < next_special
        {
            next_special = pos;
            special_type = "code";
        }

        // Check for emphasis using pulldown-cmark's pre-extracted spans
        // Find the earliest emphasis span that starts within remaining text
        for span in &emphasis_spans {
            if span.start >= current_offset && span.start < current_offset + remaining.len() {
                let pos_in_remaining = span.start - current_offset;
                if pos_in_remaining < next_special {
                    next_special = pos_in_remaining;
                    special_type = "pulldown_emphasis";
                    pulldown_emphasis = Some(span);
                }
                break; // Spans are sorted by start position, so first match is earliest
            }
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
                // Note: "strikethrough" case removed - now handled by pulldown-cmark
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
                    // HTML entities are captured whole - use as_str() to get just the matched content
                    elements.push(Element::HtmlEntity(match_obj.as_str().to_string()));
                    remaining = &remaining[match_obj.end()..];
                }
                "hugo_shortcode" => {
                    // Hugo shortcodes are atomic elements - preserve them exactly
                    elements.push(Element::HugoShortcode(match_obj.as_str().to_string()));
                    remaining = &remaining[match_obj.end()..];
                }
                "html_tag" => {
                    // HTML tags are captured whole - use as_str() to get just the matched content
                    elements.push(Element::HtmlTag(match_obj.as_str().to_string()));
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
                "pulldown_emphasis" => {
                    // Use pre-extracted emphasis/strikethrough span from pulldown-cmark
                    if let Some(span) = pulldown_emphasis {
                        let span_len = span.end - span.start;
                        if span.is_strikethrough {
                            elements.push(Element::Strikethrough(span.content.clone()));
                        } else if span.is_strong {
                            elements.push(Element::Bold {
                                content: span.content.clone(),
                                underscore: span.uses_underscore,
                            });
                        } else {
                            elements.push(Element::Italic {
                                content: span.content.clone(),
                                underscore: span.uses_underscore,
                            });
                        }
                        remaining = &remaining[span_len..];
                    } else {
                        // Fallback - shouldn't happen
                        elements.push(Element::Text(remaining[..1].to_string()));
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
                // Single sentence - check if it's complete
                let trimmed = combined.trim();
                let ends_with_sentence_punct =
                    trimmed.ends_with('.') || trimmed.ends_with('!') || trimmed.ends_with('?');

                if ends_with_sentence_punct && !text_ends_with_abbreviation(trimmed, &abbreviations) {
                    // Complete single sentence - emit it
                    lines.push(trimmed.to_string());
                    current_line.clear();
                } else {
                    // Incomplete sentence - continue accumulating
                    current_line = combined;
                }
            }
        } else if let Element::Italic { content, underscore } = element {
            // Handle italic elements - may contain multiple sentences that need continuation
            let marker = if *underscore { "_" } else { "*" };
            handle_emphasis_sentence_split(content, marker, &abbreviations, &mut current_line, &mut lines);
        } else if let Element::Bold { content, underscore } = element {
            // Handle bold elements - may contain multiple sentences that need continuation
            let marker = if *underscore { "__" } else { "**" };
            handle_emphasis_sentence_split(content, marker, &abbreviations, &mut current_line, &mut lines);
        } else if let Element::Strikethrough(content) = element {
            // Handle strikethrough elements - may contain multiple sentences that need continuation
            handle_emphasis_sentence_split(content, "~~", &abbreviations, &mut current_line, &mut lines);
        } else {
            // Non-text, non-emphasis elements (Code, Links, etc.)
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

/// Handle splitting emphasis content at sentence boundaries while preserving markers
fn handle_emphasis_sentence_split(
    content: &str,
    marker: &str,
    abbreviations: &HashSet<String>,
    current_line: &mut String,
    lines: &mut Vec<String>,
) {
    // Split the emphasis content into sentences
    let sentences = split_into_sentences_with_set(content, abbreviations);

    if sentences.len() <= 1 {
        // Single sentence or no boundaries - treat as atomic
        if !current_line.is_empty()
            && !current_line.ends_with(' ')
            && !current_line.ends_with('(')
            && !current_line.ends_with('[')
        {
            current_line.push(' ');
        }
        current_line.push_str(marker);
        current_line.push_str(content);
        current_line.push_str(marker);

        // Check if the emphasis content ends with sentence punctuation - if so, emit
        let trimmed = content.trim();
        let ends_with_punct = trimmed.ends_with('.') || trimmed.ends_with('!') || trimmed.ends_with('?');
        if ends_with_punct && !text_ends_with_abbreviation(trimmed, abbreviations) {
            lines.push(current_line.clone());
            current_line.clear();
        }
    } else {
        // Multiple sentences - each gets its own emphasis markers
        for (i, sentence) in sentences.iter().enumerate() {
            let trimmed = sentence.trim();
            if trimmed.is_empty() {
                continue;
            }

            if i == 0 {
                // First sentence: combine with current_line and emit
                if !current_line.is_empty()
                    && !current_line.ends_with(' ')
                    && !current_line.ends_with('(')
                    && !current_line.ends_with('[')
                {
                    current_line.push(' ');
                }
                current_line.push_str(marker);
                current_line.push_str(trimmed);
                current_line.push_str(marker);

                // Check if this is a complete sentence
                let ends_with_punct = trimmed.ends_with('.') || trimmed.ends_with('!') || trimmed.ends_with('?');
                if ends_with_punct && !text_ends_with_abbreviation(trimmed, abbreviations) {
                    lines.push(current_line.clone());
                    current_line.clear();
                }
            } else if i == sentences.len() - 1 {
                // Last sentence: check if complete
                let ends_with_punct = trimmed.ends_with('.') || trimmed.ends_with('!') || trimmed.ends_with('?');

                let mut line = String::new();
                line.push_str(marker);
                line.push_str(trimmed);
                line.push_str(marker);

                if ends_with_punct && !text_ends_with_abbreviation(trimmed, abbreviations) {
                    lines.push(line);
                } else {
                    // Incomplete - keep in current_line for potential continuation
                    *current_line = line;
                }
            } else {
                // Middle sentences: emit with markers
                let mut line = String::new();
                line.push_str(marker);
                line.push_str(trimmed);
                line.push_str(marker);
                lines.push(line);
            }
        }
    }
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
            // find() returns byte position which is correct for str slicing
            // The unwrap is safe because we already verified trimmed starts with '>'
            let gt_pos = line.find('>').expect("'>' must exist since trimmed.starts_with('>')");
            let quote_prefix = line[0..gt_pos + 1].to_string();
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
        // A valid unordered list marker must be followed by a space (or be alone on line)
        // This prevents emphasis markers like "*text*" from being parsed as list items
        let is_unordered_list = |s: &str, marker: char| -> bool {
            s.starts_with(marker) && !is_horizontal_rule(s) && (s.len() == 1 || s.chars().nth(1) == Some(' '))
        };
        if is_unordered_list(trimmed, '-')
            || is_unordered_list(trimmed, '*')
            || is_unordered_list(trimmed, '+')
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
                    // Use byte-based check since content_start is a byte index
                    // This is safe because space is ASCII (single byte)
                    while content_start < line.len() && line.as_bytes().get(content_start) == Some(&b' ') {
                        content_start += 1;
                    }
                }
            } else {
                // Bullet list: marker is single character
                marker_end = indent + 1; // Just the marker character
                content_start = marker_end;
                // Skip any spaces after the marker
                // Use byte-based check since content_start is a byte index
                // This is safe because space is ASCII (single byte)
                while content_start < line.len() && line.as_bytes().get(content_start) == Some(&b' ') {
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
                // or is a complete sentence in sentence_per_line mode
                let prev_trimmed = prev_line.trim();
                let abbreviations = get_abbreviations(&options.abbreviations);
                let ends_with_sentence = (prev_trimmed.ends_with('.')
                    || prev_trimmed.ends_with('!')
                    || prev_trimmed.ends_with('?')
                    || prev_trimmed.ends_with(".*")
                    || prev_trimmed.ends_with("!*")
                    || prev_trimmed.ends_with("?*")
                    || prev_trimmed.ends_with("._")
                    || prev_trimmed.ends_with("!_")
                    || prev_trimmed.ends_with("?_")
                    // Quote-terminated sentences (straight and curly quotes)
                    || prev_trimmed.ends_with(".\"")
                    || prev_trimmed.ends_with("!\"")
                    || prev_trimmed.ends_with("?\"")
                    || prev_trimmed.ends_with(".'")
                    || prev_trimmed.ends_with("!'")
                    || prev_trimmed.ends_with("?'")
                    || prev_trimmed.ends_with(".\u{201D}")
                    || prev_trimmed.ends_with("!\u{201D}")
                    || prev_trimmed.ends_with("?\u{201D}")
                    || prev_trimmed.ends_with(".\u{2019}")
                    || prev_trimmed.ends_with("!\u{2019}")
                    || prev_trimmed.ends_with("?\u{2019}"))
                    && !text_ends_with_abbreviation(
                        prev_trimmed.trim_end_matches(['*', '_', '"', '\'', '\u{201D}', '\u{2019}']),
                        &abbreviations,
                    );

                if has_hard_break(prev_line) || (options.sentence_per_line && ends_with_sentence) {
                    // Start a new part after hard break or complete sentence
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
                // But don't add hard breaks in sentence_per_line mode - lines are already separate
                if j < paragraph_parts.len() - 1 && !result.is_empty() && !options.sentence_per_line {
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
