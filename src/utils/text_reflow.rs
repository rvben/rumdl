//! Text reflow utilities for MD013
//!
//! This module implements text wrapping/reflow functionality that preserves
//! Markdown elements like links, emphasis, code spans, etc.

use crate::utils::regex_cache::{
    DISPLAY_MATH_REGEX, EMOJI_SHORTCODE_REGEX, FOOTNOTE_REF_REGEX, HTML_ENTITY_REGEX, HTML_TAG_PATTERN,
    INLINE_IMAGE_FANCY_REGEX, INLINE_LINK_FANCY_REGEX, INLINE_MATH_REGEX, REF_IMAGE_REGEX, REF_LINK_REGEX,
    SHORTCUT_REF_REGEX, STRIKETHROUGH_FANCY_REGEX, WIKI_LINK_REGEX,
};
/// Options for reflowing text
#[derive(Clone)]
pub struct ReflowOptions {
    /// Target line length
    pub line_length: usize,
    /// Whether to break on sentence boundaries when possible
    pub break_on_sentences: bool,
    /// Whether to preserve existing line breaks in paragraphs
    pub preserve_breaks: bool,
}

impl Default for ReflowOptions {
    fn default() -> Self {
        Self {
            line_length: 80,
            break_on_sentences: true,
            preserve_breaks: false,
        }
    }
}

/// Reflow a single line of markdown text to fit within the specified line length
pub fn reflow_line(line: &str, options: &ReflowOptions) -> Vec<String> {
    // Quick check: if line is already short enough, return as-is
    if line.chars().count() <= options.line_length {
        return vec![line.to_string()];
    }

    // Parse the markdown to identify elements
    let elements = parse_markdown_elements(line);

    // Reflow the elements into lines
    reflow_elements(&elements, options)
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
/// 1. Inline links [text](url) - must be detected first to avoid conflicts
/// 2. Reference links [text][ref] - detected before shortcut references
/// 3. Empty reference links [text][] - a special case of reference links
/// 4. Shortcut reference links [ref] - detected last to avoid false positives
/// 5. Other elements (code, bold, italic) - processed normally
fn parse_markdown_elements(text: &str) -> Vec<Element> {
    let mut elements = Vec::new();
    let mut remaining = text;

    while !remaining.is_empty() {
        // Find the earliest occurrence of any markdown pattern
        let mut earliest_match: Option<(usize, &str, fancy_regex::Match)> = None;

        // Check for images first (they start with ! so should be detected before links)
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
        if let Ok(Some(m)) = HTML_TAG_PATTERN.find(remaining)
            && earliest_match.as_ref().is_none_or(|(start, _, _)| m.start() < *start)
        {
            earliest_match = Some((m.start(), "html_tag", m));
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
            // If this is a text element, always process it word by word
            let words: Vec<&str> = text.split_whitespace().collect();

            for word in words {
                let word_len = word.chars().count();
                if current_length > 0 && current_length + 1 + word_len > options.line_length {
                    // Start a new line
                    lines.push(current_line.trim().to_string());
                    current_line = word.to_string();
                    current_length = word_len;
                } else {
                    // Add word to current line
                    if current_length > 0 {
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
                if current_length > 0 {
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

        // Preserve code blocks
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

        // Preserve lists
        if trimmed.starts_with('-')
            || trimmed.starts_with('*')
            || trimmed.starts_with('+')
            || trimmed.chars().next().is_some_and(|c| c.is_numeric())
        {
            // Find the list marker and preserve indentation
            let indent = line.len() - line.trim_start().len();
            let indent_str = " ".repeat(indent);

            // Find where the content starts after the marker
            let content_start = line
                .find(|c: char| !(c.is_whitespace() || c == '-' || c == '*' || c == '+' || c == '.' || c.is_numeric()))
                .unwrap_or(line.len());

            let marker = &line[indent..content_start];
            let content = &line[content_start..].trim_start();

            // Calculate the proper indentation for continuation lines
            // We need to align with the text after the marker
            let trimmed_marker = marker.trim_end();
            let continuation_spaces = indent + trimmed_marker.len() + 1; // +1 for the space after marker

            let reflowed = reflow_line(content, options);
            for (j, reflowed_line) in reflowed.iter().enumerate() {
                if j == 0 {
                    result.push(format!("{indent_str}{trimmed_marker} {reflowed_line}"));
                } else {
                    // Continuation lines aligned with text after marker
                    let continuation_indent = " ".repeat(continuation_spaces);
                    result.push(format!("{continuation_indent}{reflowed_line}"));
                }
            }
            i += 1;
            continue;
        }

        // Preserve tables
        if trimmed.contains('|') {
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
                && !next_trimmed.starts_with('-')
                && !next_trimmed.starts_with('*')
                && !next_trimmed.starts_with('+')
                && !next_trimmed.chars().next().is_some_and(|c| c.is_numeric())
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
                || next_trimmed.starts_with('-')
                || next_trimmed.starts_with('*')
                || next_trimmed.starts_with('+')
                || next_trimmed.chars().next().is_some_and(|c| c.is_numeric())
            {
                break;
            }

            // Check if previous line ends with hard break (two spaces)
            if prev_line.ends_with("  ") {
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

            // Preserve hard break by ensuring last line of part ends with two spaces
            if j < paragraph_parts.len() - 1 && !result.is_empty() {
                let last_idx = result.len() - 1;
                if !result[last_idx].ends_with("  ") {
                    result[last_idx].push_str("  ");
                }
            }
        }
    }

    result.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_reflow_simple_text() {
        let options = ReflowOptions {
            line_length: 20,
            ..Default::default()
        };

        let input = "This is a very long line that needs to be wrapped";
        let result = reflow_line(input, &options);

        assert_eq!(result.len(), 3);
        assert!(result[0].chars().count() <= 20);
        assert!(result[1].chars().count() <= 20);
        assert!(result[2].chars().count() <= 20);
    }

    #[test]
    fn test_preserve_inline_code() {
        let options = ReflowOptions {
            line_length: 30,
            ..Default::default()
        };

        let result = reflow_line("This line has `inline code` that should be preserved", &options);
        // Verify inline code is not broken
        let joined = result.join(" ");
        assert!(joined.contains("`inline code`"));
    }

    #[test]
    fn test_preserve_links() {
        let options = ReflowOptions {
            line_length: 40,
            ..Default::default()
        };

        let text = "Check out [this link](https://example.com/very/long/url) for more info";
        let result = reflow_line(text, &options);

        // Verify link is preserved intact
        let joined = result.join(" ");
        assert!(joined.contains("[this link](https://example.com/very/long/url)"));
    }

    #[test]
    fn test_reference_link_patterns_fixed() {
        let options = ReflowOptions {
            line_length: 30,
            break_on_sentences: true,
            preserve_breaks: false,
        };

        // Test cases that verify reference links are preserved as atomic units
        let test_cases = vec![
            // Reference link: [text][ref] - should be preserved intact
            ("Check out [text][ref] for details", vec!["[text][ref]"]),
            // Empty reference: [text][] - should be preserved intact
            ("See [text][] for info", vec!["[text][]"]),
            // Shortcut reference: [homepage] - should be preserved intact
            ("Visit [homepage] today", vec!["[homepage]"]),
            // Multiple reference links in one line
            (
                "Links: [first][ref1] and [second][ref2] here",
                vec!["[first][ref1]", "[second][ref2]"],
            ),
            // Mixed inline and reference links
            (
                "See [inline](url) and [reference][ref] links",
                vec!["[inline](url)", "[reference][ref]"],
            ),
        ];

        for (input, expected_patterns) in test_cases {
            println!("\nTesting: {input}");
            let result = reflow_line(input, &options);
            let joined = result.join(" ");
            println!("Result:  {joined}");

            // Verify all expected patterns are preserved
            for expected_pattern in expected_patterns {
                assert!(
                    joined.contains(expected_pattern),
                    "Expected '{expected_pattern}' to be preserved in '{input}', but got '{joined}'"
                );
            }

            // Verify no broken patterns exist (spaces inside brackets)
            assert!(
                !joined.contains("[ ") || !joined.contains("] ["),
                "Detected broken reference link pattern with spaces inside brackets in '{joined}'"
            );
        }
    }

    #[test]
    fn test_reference_link_edge_cases() {
        let options = ReflowOptions {
            line_length: 40,
            break_on_sentences: true,
            preserve_breaks: false,
        };

        // Test cases for edge cases and potential conflicts
        let test_cases = vec![
            // Escaped brackets should be treated as regular text
            ("Text with \\[escaped\\] brackets", vec!["\\[escaped\\]"]),
            // Nested brackets in reference links
            (
                "Link [text with [nested] content][ref]",
                vec!["[text with [nested] content][ref]"],
            ),
            // Reference link followed by inline link
            (
                "First [ref][link] then [inline](url)",
                vec!["[ref][link]", "[inline](url)"],
            ),
            // Shortcut reference that might conflict with other patterns
            ("Array [0] and reference [link] here", vec!["[0]", "[link]"]),
            // Empty reference with complex text
            (
                "Complex [text with *emphasis*][] reference",
                vec!["[text with *emphasis*][]"],
            ),
        ];

        for (input, expected_patterns) in test_cases {
            println!("\nTesting edge case: {input}");
            let result = reflow_line(input, &options);
            let joined = result.join(" ");
            println!("Result: {joined}");

            // Verify all expected patterns are preserved
            for expected_pattern in expected_patterns {
                assert!(
                    joined.contains(expected_pattern),
                    "Expected '{expected_pattern}' to be preserved in '{input}', but got '{joined}'"
                );
            }
        }
    }

    #[test]
    fn test_reflow_with_emphasis() {
        let options = ReflowOptions {
            line_length: 25,
            ..Default::default()
        };

        let result = reflow_line("This is *emphasized* and **strong** text that needs wrapping", &options);

        // Verify emphasis markers are preserved
        let joined = result.join(" ");
        assert!(joined.contains("*emphasized*"));
        assert!(joined.contains("**strong**"));
    }

    #[test]
    fn test_image_patterns_preserved() {
        let options = ReflowOptions {
            line_length: 30,
            ..Default::default()
        };

        // Test cases for image patterns
        let test_cases = vec![
            // Inline image
            (
                "Check out ![alt text](image.png) for details",
                vec!["![alt text](image.png)"],
            ),
            // Reference image
            ("See ![image][ref] for info", vec!["![image][ref]"]),
            // Empty reference image
            ("Visit ![homepage][] today", vec!["![homepage][]"]),
            // Multiple images
            (
                "Images: ![first](a.png) and ![second][ref2]",
                vec!["![first](a.png)", "![second][ref2]"],
            ),
        ];

        for (input, expected_patterns) in test_cases {
            println!("\nTesting: {input}");
            let result = reflow_line(input, &options);
            let joined = result.join(" ");
            println!("Result:  {joined}");

            for expected_pattern in expected_patterns {
                assert!(
                    joined.contains(expected_pattern),
                    "Expected '{expected_pattern}' to be preserved in '{input}', but got '{joined}'"
                );
            }
        }
    }

    #[test]
    fn test_extended_markdown_patterns() {
        let options = ReflowOptions {
            line_length: 40,
            ..Default::default()
        };

        let test_cases = vec![
            // Strikethrough
            ("Text with ~~strikethrough~~ preserved", vec!["~~strikethrough~~"]),
            // Wiki links
            (
                "Check [[wiki link]] and [[page|display]]",
                vec!["[[wiki link]]", "[[page|display]]"],
            ),
            // Math
            (
                "Inline $x^2 + y^2$ and display $$\\int f(x) dx$$",
                vec!["$x^2 + y^2$", "$$\\int f(x) dx$$"],
            ),
            // Emoji
            ("Use :smile: and :heart: emojis", vec![":smile:", ":heart:"]),
            // HTML tags
            (
                "Text with <span>tag</span> and <br/>",
                vec!["<span>", "</span>", "<br/>"],
            ),
            // HTML entities
            ("Non-breaking&nbsp;space and em&mdash;dash", vec!["&nbsp;", "&mdash;"]),
        ];

        for (input, expected_patterns) in test_cases {
            let result = reflow_line(input, &options);
            let joined = result.join(" ");

            for pattern in expected_patterns {
                assert!(
                    joined.contains(pattern),
                    "Expected '{pattern}' to be preserved in '{input}', but got '{joined}'"
                );
            }
        }
    }

    #[test]
    fn test_complex_mixed_patterns() {
        let options = ReflowOptions {
            line_length: 50,
            ..Default::default()
        };

        // Test that multiple pattern types work together
        let input = "Line with **bold**, `code`, [link](url), ![image](img), ~~strike~~, $math$, :emoji:, and <tag> all together";
        let result = reflow_line(input, &options);
        let joined = result.join(" ");

        // All patterns should be preserved
        assert!(joined.contains("**bold**"));
        assert!(joined.contains("`code`"));
        assert!(joined.contains("[link](url)"));
        assert!(joined.contains("![image](img)"));
        assert!(joined.contains("~~strike~~"));
        assert!(joined.contains("$math$"));
        assert!(joined.contains(":emoji:"));
        assert!(joined.contains("<tag>"));
    }

    #[test]
    fn test_footnote_patterns_preserved() {
        let options = ReflowOptions {
            line_length: 40,
            ..Default::default()
        };

        let test_cases = vec![
            // Single footnote
            ("This has a footnote[^1] reference", vec!["[^1]"]),
            // Multiple footnotes
            ("Text with [^first] and [^second] notes", vec!["[^first]", "[^second]"]),
            // Long footnote name
            ("Reference to [^long-footnote-name] here", vec!["[^long-footnote-name]"]),
        ];

        for (input, expected_patterns) in test_cases {
            let result = reflow_line(input, &options);
            let joined = result.join(" ");

            for expected_pattern in expected_patterns {
                assert!(
                    joined.contains(expected_pattern),
                    "Expected '{expected_pattern}' to be preserved in '{input}', but got '{joined}'"
                );
            }
        }
    }
}
