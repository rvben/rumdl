//! Text reflow utilities for MD013
//!
//! This module implements text wrapping/reflow functionality that preserves
//! Markdown elements like links, emphasis, code spans, etc.

use crate::utils::calculate_indentation_width_default;
use crate::utils::is_definition_list_item;
use crate::utils::mkdocs_attr_list::{ATTR_LIST_PATTERN, is_standalone_attr_list};
use crate::utils::mkdocs_snippets::is_snippet_block_delimiter;
use crate::utils::regex_cache::{
    DISPLAY_MATH_REGEX, EMAIL_PATTERN, EMOJI_SHORTCODE_REGEX, HTML_ENTITY_REGEX, HTML_TAG_PATTERN,
    HUGO_SHORTCODE_REGEX, INLINE_MATH_REGEX, WIKI_LINK_REGEX,
};
use crate::utils::sentence_utils::{
    get_abbreviations, is_cjk_char, is_cjk_sentence_ending, is_closing_quote, is_opening_quote,
    text_ends_with_abbreviation,
};
use pulldown_cmark::{BrokenLink, CowStr, Event, LinkType, Options, Parser, Tag, TagEnd};
use std::collections::HashSet;
use unicode_width::UnicodeWidthStr;

/// Length calculation mode for reflow
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum ReflowLengthMode {
    /// Count Unicode characters (grapheme clusters)
    Chars,
    /// Count visual display width (CJK = 2 columns, emoji = 2, etc.)
    #[default]
    Visual,
    /// Count raw bytes
    Bytes,
}

/// Calculate the display length of a string based on the length mode
fn display_len(s: &str, mode: ReflowLengthMode) -> usize {
    match mode {
        ReflowLengthMode::Chars => s.chars().count(),
        ReflowLengthMode::Visual => s.width(),
        ReflowLengthMode::Bytes => s.len(),
    }
}

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
    /// Whether to use semantic line breaks (cascading split strategy)
    pub semantic_line_breaks: bool,
    /// Custom abbreviations for sentence detection
    /// Periods are optional - both "Dr" and "Dr." work the same
    /// Custom abbreviations are always added to the built-in defaults
    pub abbreviations: Option<Vec<String>>,
    /// How to measure string length for line-length comparisons
    pub length_mode: ReflowLengthMode,
    /// Whether to treat {#id .class key="value"} as atomic (unsplittable) elements.
    /// Enabled for MkDocs and Kramdown flavors.
    pub attr_lists: bool,
    /// Whether to treat MyST inline roles (`` {role}`content` ``) as atomic
    /// (unsplittable) elements. Enabled for the MyST flavor so the colon inside
    /// `{domain:role}` is never used as a clause-break point.
    pub myst_roles: bool,
    /// Whether to require uppercase after periods for sentence detection.
    /// When true (default), only "word. Capital" is a sentence boundary.
    /// When false, "word. lowercase" is also treated as a sentence boundary.
    /// Does not affect ! and ? which are always treated as sentence boundaries.
    pub require_sentence_capital: bool,
    /// Cap list continuation indent to this value when set.
    /// Used by mkdocs flavor where continuation is always 4 spaces
    /// regardless of checkbox markers.
    pub max_list_continuation_indent: Option<usize>,
    /// Defined reference labels for the surrounding document, used to decide
    /// whether a bare shortcut reference (`[text]`) is a real link (kept atomic
    /// during reflow) or literal bracketed prose (wrapped like normal text).
    ///
    /// `None` means no reference information is available: every shortcut is
    /// treated as atomic. This is the safe default - it never splits a real
    /// link, at the cost of also not wrapping literal bracketed prose.
    ///
    /// `Some(set)` enables definition-aware behavior: a shortcut is atomic only
    /// when its normalized label (see [`normalize_reference_label`]) is in the
    /// set. Full and collapsed reference links and reference images are always
    /// atomic regardless, because their `][ref]` / `[]` syntax is an explicit
    /// link signal that does not depend on a definition being in scope.
    pub defined_references: Option<HashSet<String>>,
}

impl Default for ReflowOptions {
    fn default() -> Self {
        Self {
            line_length: 80,
            break_on_sentences: true,
            preserve_breaks: false,
            sentence_per_line: false,
            semantic_line_breaks: false,
            abbreviations: None,
            length_mode: ReflowLengthMode::default(),
            attr_lists: false,
            myst_roles: false,
            require_sentence_capital: true,
            max_list_continuation_indent: None,
            defined_references: None,
        }
    }
}

/// Normalize a reference label for definition matching: collapse internal
/// whitespace runs to a single space, trim, and lowercase (CommonMark-style
/// label matching). Both the defined labels and the shortcut references checked
/// against them are run through this function, so matching is case- and
/// whitespace-insensitive. Biasing toward matching keeps a real shortcut link
/// atomic even when its use and definition differ only in case or whitespace.
pub fn normalize_reference_label(label: &str) -> String {
    label.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase()
}

/// Build a boolean mask indicating which character positions are inside inline code spans.
/// Handles single, double, and triple backtick delimiters.
fn compute_inline_code_mask(text: &str) -> Vec<bool> {
    let code_spans = extract_code_spans(text);
    let chars: Vec<char> = text.chars().collect();
    let mut mask = vec![false; chars.len()];
    let mut span_it = code_spans.iter().peekable();
    let mut byte_idx = 0;
    // Map character indices to byte-offset based code spans in a single pass.
    // Since code spans are sorted by start offset, we advance the span iterator
    // as our character byte index passes the end of the current span.
    for (char_idx, ch) in chars.iter().enumerate() {
        let next_byte_idx = byte_idx + ch.len_utf8();
        while let Some(span) = span_it.peek() {
            if span.end <= byte_idx {
                span_it.next();
            } else {
                break;
            }
        }
        if let Some(span) = span_it.peek()
            && byte_idx >= span.start
            && byte_idx < span.end
        {
            mask[char_idx] = true;
        }
        byte_idx = next_byte_idx;
    }
    mask
}

/// If `chars` starts at `start` with one or more consecutive footnote
/// references (`[^label]`, matching the same `[a-zA-Z0-9_-]+` label grammar as
/// `FOOTNOTE_REF` in `mkdocs_footnotes.rs`), return the position just past the
/// last one. Returns `None` if `start` is not the beginning of a footnote
/// reference, so a bare `[1]` or `[text]` never matches.
fn footnote_refs_end(chars: &[char], start: usize) -> Option<usize> {
    let mut pos = start;
    let mut found = false;

    loop {
        if chars.get(pos) != Some(&'[') || chars.get(pos + 1) != Some(&'^') {
            break;
        }
        let label_start = pos + 2;
        let mut label_end = label_start;
        while matches!(chars.get(label_end), Some(c) if c.is_ascii_alphanumeric() || *c == '_' || *c == '-') {
            label_end += 1;
        }
        if label_end == label_start || chars.get(label_end) != Some(&']') {
            break;
        }
        pos = label_end + 1;
        found = true;
    }

    found.then_some(pos)
}

/// Detect if a character position is a sentence boundary
/// Based on the approach from github.com/JoshuaKGoldberg/sentences-per-line
/// Supports both ASCII punctuation (. ! ?) and CJK punctuation (。 ！ ？)
fn is_sentence_boundary(
    text: &str,
    chars: &[char],
    pos: usize,
    abbreviations: &HashSet<String>,
    require_sentence_capital: bool,
) -> bool {
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
    } else if next_char == '[' {
        // Sentence ends with one or more footnote references glued directly to
        // the punctuation, e.g. "sentence.[^1]" or "sentence.[^1][^2]". A bare
        // `[1]` or `[text]` doesn't match `footnote_refs_end` and falls through
        // to `return false` below, since that's link/citation-like text, not
        // footnote syntax.
        match footnote_refs_end(chars, pos + 1) {
            Some(end_pos) if chars.get(end_pos) == Some(&' ') => (end_pos, end_pos + 1),
            _ => return false,
        }
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

    let first_char = chars[first_letter_pos];

    // For ! and ?, sentence boundaries are unambiguous — no uppercase requirement
    if c == '!' || c == '?' {
        return true;
    }

    // Period-specific checks: periods are ambiguous (abbreviations, decimals, initials)
    // so we apply additional guards before accepting a sentence boundary.

    if pos > 0 {
        // Check for common abbreviations
        let byte_offset: usize = chars[..=pos].iter().map(|ch| ch.len_utf8()).sum();
        if text_ends_with_abbreviation(&text[..byte_offset], abbreviations) {
            return false;
        }

        // Check for decimal numbers (e.g., "3.14 is pi")
        if chars[pos - 1].is_numeric() && first_char.is_ascii_digit() {
            return false;
        }

        // Check for single-letter initials (e.g., "J. K. Rowling")
        // A single uppercase letter before the period preceded by whitespace or start
        // is likely an initial, not a sentence ending.
        if chars[pos - 1].is_ascii_uppercase() && (pos == 1 || (pos >= 2 && chars[pos - 2].is_whitespace())) {
            return false;
        }
    }

    // In strict mode, require uppercase or CJK to start the next sentence after a period.
    // In relaxed mode, accept any alphanumeric character.
    if require_sentence_capital && !first_char.is_uppercase() && !is_cjk_char(first_char) {
        return false;
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
    split_into_sentences_with_set(text, &abbreviations, true)
}

/// Internal function to split text into sentences with a pre-computed abbreviations set
/// Use this when calling multiple times in a loop to avoid repeatedly computing the set
fn split_into_sentences_with_set(
    text: &str,
    abbreviations: &HashSet<String>,
    require_sentence_capital: bool,
) -> Vec<String> {
    // Pre-compute which character positions are inside inline code spans
    let in_code = compute_inline_code_mask(text);
    // Collect chars once and share the slice with is_sentence_boundary, which
    // would otherwise re-collect the whole text on every position it checks.
    let char_vec: Vec<char> = text.chars().collect();

    let mut sentences = Vec::new();
    let mut current_sentence = String::new();
    let mut chars = text.chars().peekable();
    let mut pos = 0;

    while let Some(c) = chars.next() {
        current_sentence.push(c);

        if !in_code[pos] && is_sentence_boundary(text, &char_vec, pos, abbreviations, require_sentence_capital) {
            // Consume any trailing footnote references glued to the punctuation
            // (they belong to the current sentence, e.g. "sentence.[^1]" keeps
            // the marker attached to the sentence it annotates rather than
            // leaking onto the next one).
            if let Some(end_pos) = footnote_refs_end(&char_vec, pos + 1) {
                while pos + 1 < end_pos {
                    current_sentence.push(chars.next().unwrap());
                    pos += 1;
                }
            }

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

    // Line must consist only of a single marker char (-, _, or *) plus spaces,
    // with at least 3 markers. Scan chars directly to avoid allocating a Vec.
    let mut chars = line.chars();
    let Some(first_char) = chars.next() else {
        return false;
    };
    if first_char != '-' && first_char != '_' && first_char != '*' {
        return false;
    }

    let mut non_space_count = 1usize; // first_char is a marker
    for c in chars {
        if c == ' ' {
            continue;
        }
        if c != first_char {
            return false;
        }
        non_space_count += 1;
    }
    non_space_count >= 3
}

/// Check if a line is a numbered list item (e.g., "1. ", "10. ")
fn is_numbered_list_item(line: &str) -> bool {
    let mut chars = line.chars();

    // Must start with a digit
    if !chars.next().is_some_and(char::is_numeric) {
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

/// Check if a trimmed line is an unordered list item (-, *, + followed by space)
fn is_unordered_list_marker(s: &str) -> bool {
    matches!(s.as_bytes().first(), Some(b'-' | b'*' | b'+'))
        && !is_horizontal_rule(s)
        && (s.len() == 1 || s.as_bytes().get(1) == Some(&b' '))
}

/// Shared structural checks for block boundary detection.
/// Checks elements that only depend on the trimmed line content.
fn is_block_boundary_core(trimmed: &str) -> bool {
    trimmed.is_empty()
        || trimmed.starts_with('#')
        || trimmed.starts_with("```")
        || trimmed.starts_with("~~~")
        || trimmed.starts_with('>')
        || (trimmed.starts_with('[') && trimmed.contains("]:"))
        || is_horizontal_rule(trimmed)
        || is_unordered_list_marker(trimmed)
        || is_numbered_list_item(trimmed)
        || is_definition_list_item(trimmed)
        || trimmed.starts_with(":::")
}

/// Check if a trimmed line starts a new structural block element.
/// Used for paragraph boundary detection in `reflow_markdown()`.
fn is_block_boundary(trimmed: &str) -> bool {
    is_block_boundary_core(trimmed) || trimmed.starts_with('|')
}

/// Check if a line starts a new structural block for paragraph boundary detection
/// in `reflow_paragraph_at_line()`. Extends the core checks with indented code blocks
/// (≥4 spaces) and table row detection via `is_potential_table_row`.
fn is_paragraph_boundary(trimmed: &str, line: &str) -> bool {
    is_block_boundary_core(trimmed)
        || calculate_indentation_width_default(line) >= 4
        || crate::utils::table_utils::TableUtils::is_potential_table_row(line)
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

/// Check if text ends with sentence-terminating punctuation (. ! ?)
fn ends_with_sentence_punct(text: &str) -> bool {
    text.ends_with('.') || text.ends_with('!') || text.ends_with('?')
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

/// Parse markdown elements using the appropriate parser based on options.
fn parse_elements(text: &str, options: &ReflowOptions) -> Vec<Element> {
    parse_markdown_elements_inner(
        text,
        options.attr_lists,
        options.myst_roles,
        options.defined_references.as_ref(),
    )
}

pub fn reflow_line(line: &str, options: &ReflowOptions) -> Vec<String> {
    // For sentence-per-line mode, always process regardless of length
    if options.sentence_per_line {
        let elements = parse_elements(line, options);
        return merge_block_construct_continuations(reflow_elements_sentence_per_line(
            &elements,
            &options.abbreviations,
            options.require_sentence_capital,
        ));
    }

    // For semantic line breaks mode, use cascading split strategy
    if options.semantic_line_breaks {
        let elements = parse_elements(line, options);
        return merge_block_construct_continuations(reflow_elements_semantic(&elements, options));
    }

    // Quick check: if line is already short enough or no wrapping requested, return as-is
    // line_length = 0 means no wrapping (unlimited line length)
    if options.line_length == 0 || display_len(line, options.length_mode) <= options.line_length {
        return vec![line.to_string()];
    }

    // Parse the markdown to identify elements
    let elements = parse_elements(line, options);

    // Reflow the elements into lines
    merge_block_construct_continuations(reflow_elements(&elements, options))
}

/// Represents a piece of content in the markdown
#[derive(Debug, Clone)]
enum Element {
    /// Plain text that can be wrapped
    Text(String),
    /// A complete markdown inline link [text](url)
    Link(String),
    /// A complete markdown reference link [text][ref]
    ReferenceLink(String),
    /// A complete markdown empty reference link [text][]
    EmptyReferenceLink(String),
    /// A complete markdown shortcut reference link [ref]
    ShortcutReference(String),
    /// A complete markdown inline image ![alt](url)
    InlineImage(String),
    /// A complete markdown reference image ![alt][ref]
    ReferenceImage(String),
    /// A complete markdown empty reference image ![alt][]
    EmptyReferenceImage(String),
    /// A clickable image badge
    LinkedImage(String),
    /// Footnote reference [^note]
    FootnoteReference(String),
    /// Strikethrough text ~~text~~ or ~text~ (GFM allows one or two tildes)
    Strikethrough {
        content: String,
        /// True if the original used a double-tilde (~~) marker, false for a single tilde (~)
        double: bool,
    },
    /// Wiki-style link [[wiki]] or [[wiki|text]]
    WikiLink(String),
    /// Inline math $math$
    InlineMath(String),
    /// Display math $$math$$
    DisplayMath(String),
    /// Emoji shortcode :emoji:
    EmojiShortcode(String),
    /// Autolink <https://...> or <mailto:...> or <user@domain.com>
    Autolink(String),
    /// HTML tag <tag> or </tag> or <tag/>
    HtmlTag(String),
    /// HTML entity &nbsp; or &#123;
    HtmlEntity(String),
    /// Hugo/Go template shortcode {{< ... >}} or {{% ... %}}
    HugoShortcode(String),
    /// MkDocs/kramdown attribute list {#id .class key="value"}
    AttrList(String),
    /// MyST inline role `` {role}`content` `` (or `` {domain:role}`content` ``).
    /// Stored as the raw matched text and rendered verbatim so it round-trips
    /// exactly; treated as atomic so it is never split mid-role.
    MystRole(String),
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
            Element::Link(s) => write!(f, "{s}"),
            Element::ReferenceLink(s) => write!(f, "{s}"),
            Element::EmptyReferenceLink(s) => write!(f, "{s}"),
            Element::ShortcutReference(s) => write!(f, "{s}"),
            Element::InlineImage(s) => write!(f, "{s}"),
            Element::ReferenceImage(s) => write!(f, "{s}"),
            Element::EmptyReferenceImage(s) => write!(f, "{s}"),
            Element::LinkedImage(s) => write!(f, "{s}"),
            Element::FootnoteReference(s) => write!(f, "{s}"),
            Element::Strikethrough { content, double } => {
                let marker = if *double { "~~" } else { "~" };
                write!(f, "{marker}{content}{marker}")
            }
            Element::WikiLink(s) => write!(f, "[[{s}]]"),
            Element::InlineMath(s) => write!(f, "${s}$"),
            Element::DisplayMath(s) => write!(f, "$${s}$$"),
            Element::EmojiShortcode(s) => write!(f, ":{s}:"),
            Element::Autolink(s) => write!(f, "{s}"),
            Element::HtmlTag(s) => write!(f, "{s}"),
            Element::HtmlEntity(s) => write!(f, "{s}"),
            Element::HugoShortcode(s) => write!(f, "{s}"),
            Element::AttrList(s) => write!(f, "{s}"),
            Element::MystRole(s) => write!(f, "{s}"),
            Element::Code(s) => write!(f, "{s}"),
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
    /// For strikethrough spans, whether the original used a double-tilde (~~)
    /// marker rather than a single tilde (~). Meaningless for other spans.
    strikethrough_double: bool,
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
    // Every emphasis, strong, or strikethrough marker starts with one of these
    // bytes, so their absence rules out any span without running the parser.
    if !text.contains(['*', '_', '~']) {
        return Vec::new();
    }

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
                            strikethrough_double: false,
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
                            strikethrough_double: false,
                        });
                    }
                }
            }
            Event::Start(Tag::Strikethrough) => {
                strikethrough_stack.push(range.start);
            }
            Event::End(TagEnd::Strikethrough) => {
                if let Some(start_byte) = strikethrough_stack.pop() {
                    // pulldown-cmark's GFM strikethrough accepts both ~~text~~ and
                    // ~text~. Detect the actual marker width so single-tilde spans
                    // keep their first and last content character.
                    let double = text.get(start_byte..start_byte + 2) == Some("~~");
                    let marker_len = if double { 2 } else { 1 };
                    let content_start = start_byte + marker_len;
                    let content_end = range.end - marker_len;
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
                            strikethrough_double: double,
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

#[derive(Debug, Clone)]
struct CodeSpan {
    start: usize,
    end: usize,
}

fn extract_code_spans(text: &str) -> Vec<CodeSpan> {
    // A code span always needs a backtick; skip the parser entirely without one.
    if !text.contains('`') {
        return Vec::new();
    }

    let mut spans = Vec::new();
    let parser = Parser::new(text).into_offset_iter();
    for (event, range) in parser {
        if let Event::Code(_) = event {
            spans.push(CodeSpan {
                start: range.start,
                end: range.end,
            });
        }
    }
    spans
}

#[derive(Debug, Clone)]
struct LinkSpan {
    start: usize,
    end: usize,
    link_type: Option<LinkType>,
    is_image: bool,
    is_footnote: bool,
}

fn extract_link_spans(text: &str, defined_references: Option<&HashSet<String>>) -> Vec<LinkSpan> {
    // Links, images, and footnote references all open with `[`; skip the
    // parser entirely without one.
    if !text.contains('[') {
        return Vec::new();
    }

    let mut spans = Vec::new();
    let mut options = Options::empty();
    options.insert(Options::ENABLE_FOOTNOTES);

    // Reflow parses each paragraph in isolation, so the document's reference
    // definitions are never in scope. Without a broken-link callback,
    // pulldown-cmark would emit reference-style links (`[text][ref]`,
    // `[text][]`, `[text]`, `![alt][ref]`) as plain text, and reflow would wrap
    // their text mid-link. Resolving an unresolved reference to a dummy
    // destination makes pulldown emit the full link span so reflow treats it as
    // an atomic unit; the destination is unused because the element is rebuilt
    // verbatim from the source bytes.
    //
    // Full and collapsed references and reference images carry explicit
    // `][ref]` / `[]` syntax, so they are always resolved (atomic). A bare
    // shortcut `[text]` is ambiguous: it is only a real link when its label is
    // actually defined. With `Some(defined_references)` an undefined shortcut is
    // left unresolved (returns `None`) so it reflows as literal prose; with
    // `None` (no reference info) every shortcut stays atomic, which never splits
    // a real link.
    let resolve = move |link: BrokenLink<'_>| -> Option<(CowStr<'_>, CowStr<'_>)> {
        // The callback reports the syntactic reference type (`Shortcut` for a
        // bare `[text]`); the eventual emitted tag carries the `*Unknown`
        // variant. Only a bare shortcut is ambiguous - full and collapsed
        // references fall through and stay atomic.
        let atomic = match link.link_type {
            LinkType::Shortcut | LinkType::ShortcutUnknown => match defined_references {
                Some(defs) => defs.contains(&normalize_reference_label(link.reference.as_ref())),
                None => true,
            },
            _ => true,
        };
        atomic.then_some((CowStr::Borrowed(""), CowStr::Borrowed("")))
    };
    let parser = Parser::new_with_broken_link_callback(text, options, Some(resolve)).into_offset_iter();
    let mut stack = Vec::new();

    for (event, range) in parser {
        match event {
            Event::Start(Tag::Link { link_type, .. }) => {
                stack.push((range.start, Some(link_type), false));
            }
            Event::Start(Tag::Image { link_type, .. }) => {
                stack.push((range.start, Some(link_type), true));
            }
            Event::End(TagEnd::Link) => {
                if let Some((start_byte, link_type, is_image)) = stack.pop()
                    && stack.is_empty()
                {
                    spans.push(LinkSpan {
                        start: start_byte,
                        end: range.end,
                        link_type,
                        is_image,
                        is_footnote: false,
                    });
                }
            }
            Event::End(TagEnd::Image) => {
                if let Some((start_byte, link_type, is_image)) = stack.pop()
                    && stack.is_empty()
                {
                    spans.push(LinkSpan {
                        start: start_byte,
                        end: range.end,
                        link_type,
                        is_image,
                        is_footnote: false,
                    });
                }
            }
            Event::FootnoteReference(_) if stack.is_empty() => {
                spans.push(LinkSpan {
                    start: range.start,
                    end: range.end,
                    link_type: None,
                    is_image: false,
                    is_footnote: true,
                });
            }
            _ => {}
        }
    }

    spans.sort_by_key(|s| s.start);
    spans
}

/// If `text` starts with a MyST inline role (`` {name}`content` `` or
/// `` {domain:role}`content` ``), return the byte length of the whole role unit.
///
/// Mirrors the grammar in `lint_context::flavor_detection::detect_myst_role_ranges`:
/// a `{`, a name starting with an ASCII letter or `_` and continuing with
/// alphanumerics / `-` / `_` / `:` / `.`, a closing `}`, then a balanced inline
/// code span using one or more backticks. Returns `None` when any part is missing.
fn myst_role_len_at(text: &str) -> Option<usize> {
    let bytes = text.as_bytes();
    if bytes.first() != Some(&b'{') {
        return None;
    }

    // Role name.
    let mut j = 1;
    match bytes.get(j) {
        Some(&b) if b.is_ascii_alphabetic() || b == b'_' => {}
        _ => return None,
    }
    while let Some(&b) = bytes.get(j) {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b':' | b'.') {
            j += 1;
        } else {
            break;
        }
    }
    if bytes.get(j) != Some(&b'}') {
        return None;
    }
    j += 1; // past '}'

    // Must be immediately followed by an inline code span.
    if bytes.get(j) != Some(&b'`') {
        return None;
    }
    let backtick_start = j;
    while bytes.get(j) == Some(&b'`') {
        j += 1;
    }
    let backtick_count = j - backtick_start;

    // Find the matching run of `backtick_count` backticks.
    while j + backtick_count <= bytes.len() {
        if bytes[j] == b'`' {
            let close_count = bytes[j..].iter().take_while(|&&b| b == b'`').count();
            if close_count == backtick_count {
                return Some(j + close_count);
            }
            j += close_count;
        } else {
            j += 1;
        }
    }

    None
}

/// Parse markdown elements from text preserving the raw syntax.
///
/// Detection order is critical:
/// 1. Linked images [![alt](img)](link) - must be detected first as atomic units
/// 2. Inline images ![alt](url) - before links to handle ! prefix
/// 3. Reference images ![alt][ref] - before reference links
/// 4. Inline links [text](url) - before reference links
/// 5. Reference links [text][ref] - before shortcut references
/// 6. Shortcut reference links [ref] - detected last to avoid false positives
/// 7. Other elements (code, bold, italic, MyST roles, etc.) - processed normally
#[derive(Clone, Copy, Debug)]
struct PatternMatch {
    start: usize,
    end: usize,
}

fn parse_markdown_elements_inner(
    text: &str,
    attr_lists: bool,
    myst_roles: bool,
    defined_references: Option<&HashSet<String>>,
) -> Vec<Element> {
    let mut elements = Vec::new();
    let mut remaining = text;

    // Pre-extract emphasis spans, link spans, and code spans using pulldown-cmark.
    // These run as separate parses rather than one shared parser: link resolution
    // (the broken-link callback, needed to keep undefined shortcuts/references
    // atomic) changes which bracket runs collapse into link nodes, which shifts
    // where nearby emphasis delimiters pair up in edge cases (e.g. an unresolved
    // shortcut containing `**`). Sharing a parser would leak that shift into
    // emphasis_spans; each parse keeps its own event stream self-consistent.
    let emphasis_spans = extract_emphasis_spans(text);
    let link_spans = extract_link_spans(text, defined_references);
    let code_spans = extract_code_spans(text);

    // Caching state to avoid O(N^2) worst case on long inputs
    let mut cached_wiki_link: Option<Option<PatternMatch>> = None;
    let mut cached_display_math: Option<Option<PatternMatch>> = None;
    let mut cached_inline_math: Option<Option<PatternMatch>> = None;
    let mut cached_emoji: Option<Option<PatternMatch>> = None;
    let mut cached_html_entity: Option<Option<PatternMatch>> = None;
    let mut cached_hugo_shortcode: Option<Option<PatternMatch>> = None;
    let mut cached_html_tag: Option<Option<PatternMatch>> = None;
    let mut cached_next_curly: Option<Option<usize>> = None;

    while !remaining.is_empty() {
        let current_offset = text.len() - remaining.len();
        let mut earliest_match: Option<(usize, usize, &str)> = None;

        // Find the earliest link span
        let mut next_link: Option<&LinkSpan> = None;
        for span in &link_spans {
            if span.start >= current_offset {
                next_link = Some(span);
                break;
            }
        }

        if let Some(span) = next_link {
            let pos_in_remaining = span.start - current_offset;
            if earliest_match
                .as_ref()
                .is_none_or(|(start, _, _)| pos_in_remaining < *start)
            {
                let match_end = span.end - current_offset;
                earliest_match = Some((pos_in_remaining, match_end, "link_span"));
            }
        }

        macro_rules! get_or_update_match {
            ($cache:expr, $regex:expr) => {{
                let need_search = match &$cache {
                    Some(Some(pm)) => pm.start < current_offset,
                    Some(None) => false,
                    None => true,
                };
                if need_search {
                    if let Some(m) = $regex.find(&text[current_offset..]) {
                        $cache = Some(Some(PatternMatch {
                            start: current_offset + m.start(),
                            end: current_offset + m.end(),
                        }));
                    } else {
                        $cache = Some(None);
                    }
                }
                match &$cache {
                    Some(Some(pm)) => Some(*pm),
                    _ => None,
                }
            }};
        }

        macro_rules! get_or_update_fancy_match {
            ($cache:expr, $regex:expr) => {{
                let need_search = match &$cache {
                    Some(Some(pm)) => pm.start < current_offset,
                    Some(None) => false,
                    None => true,
                };
                if need_search {
                    if let Ok(Some(m)) = $regex.find(&text[current_offset..]) {
                        $cache = Some(Some(PatternMatch {
                            start: current_offset + m.start(),
                            end: current_offset + m.end(),
                        }));
                    } else {
                        $cache = Some(None);
                    }
                }
                match &$cache {
                    Some(Some(pm)) => Some(*pm),
                    _ => None,
                }
            }};
        }

        if let Some(pm) = get_or_update_match!(cached_wiki_link, WIKI_LINK_REGEX) {
            let pos_in_remaining = pm.start - current_offset;
            if earliest_match
                .as_ref()
                .is_none_or(|(start, _, _)| pos_in_remaining < *start)
            {
                let match_end = pm.end - current_offset;
                earliest_match = Some((pos_in_remaining, match_end, "wiki_link"));
            }
        }

        if let Some(pm) = get_or_update_match!(cached_display_math, DISPLAY_MATH_REGEX) {
            let pos_in_remaining = pm.start - current_offset;
            if earliest_match
                .as_ref()
                .is_none_or(|(start, _, _)| pos_in_remaining < *start)
            {
                let match_end = pm.end - current_offset;
                earliest_match = Some((pos_in_remaining, match_end, "display_math"));
            }
        }

        if let Some(pm) = get_or_update_fancy_match!(cached_inline_math, INLINE_MATH_REGEX) {
            let pos_in_remaining = pm.start - current_offset;
            if earliest_match
                .as_ref()
                .is_none_or(|(start, _, _)| pos_in_remaining < *start)
            {
                let match_end = pm.end - current_offset;
                earliest_match = Some((pos_in_remaining, match_end, "inline_math"));
            }
        }

        if let Some(pm) = get_or_update_match!(cached_emoji, EMOJI_SHORTCODE_REGEX) {
            let pos_in_remaining = pm.start - current_offset;
            if earliest_match
                .as_ref()
                .is_none_or(|(start, _, _)| pos_in_remaining < *start)
            {
                let match_end = pm.end - current_offset;
                earliest_match = Some((pos_in_remaining, match_end, "emoji"));
            }
        }

        if let Some(pm) = get_or_update_match!(cached_html_entity, HTML_ENTITY_REGEX) {
            let pos_in_remaining = pm.start - current_offset;
            if earliest_match
                .as_ref()
                .is_none_or(|(start, _, _)| pos_in_remaining < *start)
            {
                let match_end = pm.end - current_offset;
                earliest_match = Some((pos_in_remaining, match_end, "html_entity"));
            }
        }

        if let Some(pm) = get_or_update_match!(cached_hugo_shortcode, HUGO_SHORTCODE_REGEX) {
            let pos_in_remaining = pm.start - current_offset;
            if earliest_match
                .as_ref()
                .is_none_or(|(start, _, _)| pos_in_remaining < *start)
            {
                let match_end = pm.end - current_offset;
                earliest_match = Some((pos_in_remaining, match_end, "hugo_shortcode"));
            }
        }

        // Check for HTML tags - <tag> </tag> <tag/>
        // But exclude autolinks like <https://...> or <mailto:...> or email autolinks <user@domain.com>
        let need_html_tag_search = match &cached_html_tag {
            Some(Some(pm)) => pm.start < current_offset,
            Some(None) => false,
            None => true,
        };
        if need_html_tag_search {
            let mut search_offset = current_offset;
            loop {
                if let Some(m) = HTML_TAG_PATTERN.find(&text[search_offset..]) {
                    let absolute_start = search_offset + m.start();
                    let absolute_end = search_offset + m.end();
                    let matched_text = &text[absolute_start..absolute_end];
                    let is_url_autolink = matched_text.starts_with("<http://")
                        || matched_text.starts_with("<https://")
                        || matched_text.starts_with("<mailto:")
                        || matched_text.starts_with("<ftp://")
                        || matched_text.starts_with("<ftps://");
                    let is_email_autolink = {
                        // Use centralized EMAIL_PATTERN for consistency with MD034 and other rules
                        let content = matched_text.trim_start_matches('<').trim_end_matches('>');
                        EMAIL_PATTERN.is_match(content)
                    };
                    if is_url_autolink || is_email_autolink {
                        search_offset = absolute_end;
                    } else {
                        cached_html_tag = Some(Some(PatternMatch {
                            start: absolute_start,
                            end: absolute_end,
                        }));
                        break;
                    }
                } else {
                    cached_html_tag = Some(None);
                    break;
                }
            }
        }
        if let Some(Some(pm)) = &cached_html_tag {
            let pos_in_remaining = pm.start - current_offset;
            if earliest_match
                .as_ref()
                .is_none_or(|(start, _, _)| pos_in_remaining < *start)
            {
                let match_end = pm.end - current_offset;
                earliest_match = Some((pos_in_remaining, match_end, "html_tag"));
            }
        }

        // Find earliest non-link special characters
        let mut next_special = remaining.len();
        let mut special_type = "";
        let mut pulldown_emphasis: Option<&EmphasisSpan> = None;
        let mut attr_list_len: usize = 0;
        let mut myst_role_len: usize = 0;

        let mut next_code_span: Option<&CodeSpan> = None;
        // Check for code spans using pulldown-cmark pre-extracted spans
        for span in &code_spans {
            if span.start >= current_offset {
                next_code_span = Some(span);
                break;
            }
        }
        if let Some(span) = next_code_span {
            let pos_in_remaining = span.start - current_offset;
            if pos_in_remaining < next_special {
                next_special = pos_in_remaining;
                special_type = "pulldown_code";
            }
        }

        let need_curly_search = match &cached_next_curly {
            Some(Some(idx)) => *idx < current_offset,
            Some(None) => false,
            None => true,
        };
        if need_curly_search {
            if let Some(pos) = remaining.find('{') {
                cached_next_curly = Some(Some(current_offset + pos));
            } else {
                cached_next_curly = Some(None);
            }
        }
        let next_curly_pos = match &cached_next_curly {
            Some(Some(idx)) => Some(*idx - current_offset),
            _ => None,
        };

        // Check for MyST inline roles - {role}`content` (e.g. {cite:p}`ref`).
        // Checked before the bare code-span handling so the role's trailing code
        // span is absorbed into the atomic role rather than split off, and before
        // attr lists since a role's `{` would otherwise be probed as an attr list.
        if myst_roles
            && let Some(pos) = next_curly_pos
            && pos < next_special
            && let Some(role_len) = myst_role_len_at(&remaining[pos..])
        {
            next_special = pos;
            special_type = "myst_role";
            myst_role_len = role_len;
        }

        // Check for MkDocs/kramdown attr lists - {#id .class key="value"}
        if attr_lists
            && let Some(pos) = next_curly_pos
            && pos < next_special
            && let Some(m) = ATTR_LIST_PATTERN.find(&remaining[pos..])
            && m.start() == 0
        {
            next_special = pos;
            special_type = "attr_list";
            attr_list_len = m.end();
        }

        // Check for emphasis using pulldown-cmark's pre-extracted spans
        for span in &emphasis_spans {
            if span.start >= current_offset && span.start < current_offset + remaining.len() {
                let pos_in_remaining = span.start - current_offset;
                if pos_in_remaining < next_special {
                    next_special = pos_in_remaining;
                    special_type = "pulldown_emphasis";
                    pulldown_emphasis = Some(span);
                }
                break;
            }
        }

        // Determine which pattern to process first
        let should_process_markdown_link = if let Some((pos, _, _)) = earliest_match {
            pos < next_special
        } else {
            false
        };

        if should_process_markdown_link {
            let (pos, match_end, pattern_type) = earliest_match.unwrap();

            // Add any text before the match
            if pos > 0 {
                elements.push(Element::Text(remaining[..pos].to_string()));
            }

            // Process the matched pattern
            match pattern_type {
                "link_span" => {
                    let span = next_link.unwrap();
                    let raw_text = remaining[pos..match_end].to_string();
                    if span.is_footnote {
                        elements.push(Element::FootnoteReference(raw_text));
                    } else if span.is_image {
                        match span.link_type {
                            Some(LinkType::Inline) => elements.push(Element::InlineImage(raw_text)),
                            // `*Unknown` variants are produced when reflow's broken-link
                            // callback resolves a reference whose definition is out of scope.
                            Some(LinkType::Reference)
                            | Some(LinkType::ReferenceUnknown)
                            | Some(LinkType::Shortcut)
                            | Some(LinkType::ShortcutUnknown) => elements.push(Element::ReferenceImage(raw_text)),
                            Some(LinkType::Collapsed) | Some(LinkType::CollapsedUnknown) => {
                                elements.push(Element::EmptyReferenceImage(raw_text))
                            }
                            _ => elements.push(Element::InlineImage(raw_text)),
                        }
                    } else {
                        match span.link_type {
                            Some(LinkType::Inline) => {
                                if raw_text.starts_with('[') && raw_text.contains("![") {
                                    elements.push(Element::LinkedImage(raw_text));
                                } else {
                                    elements.push(Element::Link(raw_text));
                                }
                            }
                            // `*Unknown` variants are produced when reflow's broken-link
                            // callback resolves a reference whose definition is out of scope.
                            Some(LinkType::Reference) | Some(LinkType::ReferenceUnknown) => {
                                elements.push(Element::ReferenceLink(raw_text))
                            }
                            Some(LinkType::Collapsed) | Some(LinkType::CollapsedUnknown) => {
                                elements.push(Element::EmptyReferenceLink(raw_text))
                            }
                            Some(LinkType::Shortcut) | Some(LinkType::ShortcutUnknown) => {
                                elements.push(Element::ShortcutReference(raw_text))
                            }
                            Some(LinkType::Autolink) | Some(LinkType::Email) => {
                                elements.push(Element::Autolink(raw_text))
                            }
                            _ => elements.push(Element::Link(raw_text)),
                        }
                    }
                    remaining = &remaining[match_end..];
                }
                "wiki_link" => {
                    if let Some(caps) = WIKI_LINK_REGEX.captures(remaining) {
                        let content = caps.get(1).map_or("", |m| m.as_str());
                        elements.push(Element::WikiLink(content.to_string()));
                        remaining = &remaining[match_end..];
                    } else {
                        elements.push(Element::Text("[[".to_string()));
                        remaining = &remaining[2..];
                    }
                }
                "display_math" => {
                    if let Some(caps) = DISPLAY_MATH_REGEX.captures(remaining) {
                        let math = caps.get(1).map_or("", |m| m.as_str());
                        elements.push(Element::DisplayMath(math.to_string()));
                        remaining = &remaining[match_end..];
                    } else {
                        elements.push(Element::Text("$$".to_string()));
                        remaining = &remaining[2..];
                    }
                }
                "inline_math" => {
                    if let Ok(Some(caps)) = INLINE_MATH_REGEX.captures(remaining) {
                        let math = caps.get(1).map_or("", |m| m.as_str());
                        elements.push(Element::InlineMath(math.to_string()));
                        remaining = &remaining[match_end..];
                    } else {
                        elements.push(Element::Text("$".to_string()));
                        remaining = &remaining[1..];
                    }
                }
                "emoji" => {
                    if let Some(caps) = EMOJI_SHORTCODE_REGEX.captures(remaining) {
                        let emoji = caps.get(1).map_or("", |m| m.as_str());
                        elements.push(Element::EmojiShortcode(emoji.to_string()));
                        remaining = &remaining[match_end..];
                    } else {
                        elements.push(Element::Text(":".to_string()));
                        remaining = &remaining[1..];
                    }
                }
                "html_entity" => {
                    // HTML entities are captured whole
                    elements.push(Element::HtmlEntity(remaining[pos..match_end].to_string()));
                    remaining = &remaining[match_end..];
                }
                "hugo_shortcode" => {
                    // Hugo shortcodes are atomic elements - preserve them exactly
                    elements.push(Element::HugoShortcode(remaining[pos..match_end].to_string()));
                    remaining = &remaining[match_end..];
                }
                "html_tag" => {
                    // HTML tags are captured whole
                    elements.push(Element::HtmlTag(remaining[pos..match_end].to_string()));
                    remaining = &remaining[match_end..];
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
                "pulldown_code" => {
                    let span = next_code_span.unwrap();
                    let span_len = span.end - span.start;
                    let code = &remaining[..span_len];
                    elements.push(Element::Code(code.to_string()));
                    remaining = &remaining[span_len..];
                }
                "attr_list" => {
                    elements.push(Element::AttrList(remaining[..attr_list_len].to_string()));
                    remaining = &remaining[attr_list_len..];
                }
                "myst_role" => {
                    elements.push(Element::MystRole(remaining[..myst_role_len].to_string()));
                    remaining = &remaining[myst_role_len..];
                }
                "pulldown_emphasis" => {
                    // Use pre-extracted emphasis/strikethrough span from pulldown-cmark
                    if let Some(span) = pulldown_emphasis {
                        let span_len = span.end - span.start;
                        if span.is_strikethrough {
                            elements.push(Element::Strikethrough {
                                content: span.content.clone(),
                                double: span.strikethrough_double,
                            });
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

fn should_insert_space_before_join(current: &str) -> bool {
    !current.is_empty()
        && !current.ends_with(' ')
        && !current.ends_with('(')
        && !current.ends_with('[')
        && !current.ends_with('-')
}

/// True when `text` consists solely of setext-underline or thematic-break
/// characters: a run of `=` or `-` (setext underline, any count, no internal
/// spaces) or 3+ `-`/`*`/`_` optionally separated by spaces (thematic break).
/// A paragraph-continuation line like this converts the previous line into a
/// heading or inserts a horizontal rule.
fn is_setext_or_thematic(text: &str) -> bool {
    let mut marker = '\0';
    let mut count = 0usize;
    let mut has_space = false;
    for c in text.chars() {
        match c {
            ' ' | '\t' => has_space = true,
            '-' | '=' | '*' | '_' => {
                if marker == '\0' {
                    marker = c;
                } else if c != marker {
                    return false;
                }
                count += 1;
            }
            _ => return false,
        }
    }
    match marker {
        '=' => !has_space,
        '-' => !has_space || count >= 3,
        '*' | '_' => count >= 3,
        _ => false,
    }
}

/// True when `text`, placed at the start of a paragraph-continuation line,
/// would be re-parsed as opening a block construct - a list item (`- `, `* `,
/// `+ `, `1. `, `1) `), blockquote (`>`), ATX heading (`# `), code fence
/// (3+ backticks or tildes), thematic break, setext underline, footnote or
/// link-reference definition (`[^note]:`, `[label]: url`), or HTML block
/// (`<div>` and the other block-level tags rumdl's parser recognizes).
/// Reflow must never start a wrapped line with such content: prose that was
/// harmless mid-line becomes real block syntax at line start, silently
/// changing the document's structure (a `- ` clause becomes a nested list
/// item, a `# ` becomes a heading, a `[ref]: url` turns a dangling reference
/// elsewhere in the document into a live link, and so on).
fn starts_block_construct(text: &str) -> bool {
    let text = text.trim_start();
    let bytes = text.as_bytes();
    let Some(&first) = bytes.first() else {
        return false;
    };
    let marker_then_boundary = |len: usize| bytes.len() == len || bytes[len] == b' ' || bytes[len] == b'\t';
    match first {
        // A blockquote marker needs no following space
        b'>' => true,
        b'-' | b'*' | b'+' => marker_then_boundary(1) || is_setext_or_thematic(text),
        b'_' | b'=' => is_setext_or_thematic(text),
        b'#' => {
            let hashes = bytes.iter().take_while(|&&b| b == b'#').count();
            hashes <= 6 && marker_then_boundary(hashes)
        }
        b'`' => bytes.iter().take_while(|&&b| b == b'`').count() >= 3,
        b'~' => bytes.iter().take_while(|&&b| b == b'~').count() >= 3,
        b'0'..=b'9' => {
            let digits = bytes.iter().take_while(|b| b.is_ascii_digit()).count();
            digits <= 9
                && bytes.len() > digits
                && (bytes[digits] == b'.' || bytes[digits] == b')')
                && marker_then_boundary(digits + 1)
        }
        // Footnote/link-reference definition: `[label]:` anchored at line
        // start, meaning the label's own closing bracket is immediately
        // followed by a colon ("[ref]: url", "[^1]: note" - but not
        // "[a](b) [ref]:", whose first bracket is an inline link). rumdl's
        // parser recognizes definitions even on paragraph-continuation lines,
        // so hoisting one to line start reclassifies it (and can resolve
        // dangling references elsewhere in the document).
        b'[' => {
            let mut escaped = false;
            let mut label_close = None;
            for (i, &b) in bytes.iter().enumerate().skip(1) {
                if escaped {
                    escaped = false;
                } else if b == b'\\' {
                    escaped = true;
                } else if b == b']' {
                    label_close = Some(i);
                    break;
                }
            }
            label_close.is_some_and(|i| bytes.get(i + 1) == Some(&b':'))
        }
        // Block-level HTML tag per rumdl's parser (shared predicate, so the
        // guard cannot drift from what lint_context classifies as a block).
        b'<' => crate::utils::html_block::parse_html_block_start(text).is_some(),
        _ => false,
    }
}

/// Merge any reflowed continuation line that would open a block construct back
/// into the previous line. This is the safety net behind the per-break-site
/// guards: no matter which emitter produced the lines, a wrapped continuation
/// must never turn prose into a list item, heading, blockquote, code fence, or
/// horizontal rule. The first line keeps its position - it replaces the
/// paragraph's original start, where the source already established the
/// context. The merged line may exceed the configured width; a long line is
/// the correct failure direction, corrupted structure is not.
fn merge_block_construct_continuations(lines: Vec<String>) -> Vec<String> {
    let mut merged: Vec<String> = Vec::with_capacity(lines.len());
    for line in lines {
        match merged.last_mut() {
            Some(prev) if starts_block_construct(&line) => {
                prev.push(' ');
                prev.push_str(line.trim_start());
            }
            _ => merged.push(line),
        }
    }
    merged
}

/// Reflow elements for sentence-per-line mode
fn reflow_elements_sentence_per_line(
    elements: &[Element],
    custom_abbreviations: &Option<Vec<String>>,
    require_sentence_capital: bool,
) -> Vec<String> {
    let abbreviations = get_abbreviations(custom_abbreviations);
    let mut lines = Vec::new();
    let mut current_line = String::new();

    for (idx, element) in elements.iter().enumerate() {
        let element_str = format!("{element}");

        // For text elements, split into sentences
        if let Element::Text(text) = element {
            // Simply append text - it already has correct spacing from tokenization
            let combined = format!("{current_line}{text}");
            // Use the pre-computed abbreviations set to avoid redundant computation
            let sentences = split_into_sentences_with_set(&combined, &abbreviations, require_sentence_capital);

            if sentences.len() > 1 {
                // We found sentence boundaries
                for (i, sentence) in sentences.iter().enumerate() {
                    if i == 0 {
                        // First sentence might continue from previous elements
                        // But check if it ends with an abbreviation
                        let trimmed = sentence.trim();

                        if text_ends_with_abbreviation(trimmed, &abbreviations) {
                            // Don't emit yet - this sentence ends with abbreviation, continue accumulating
                            current_line.clone_from(sentence);
                        } else {
                            // Normal case - emit the first sentence
                            lines.push(sentence.clone());
                            current_line.clear();
                        }
                    } else if i == sentences.len() - 1 {
                        // Last sentence: check if it's complete or incomplete
                        let trimmed = sentence.trim();
                        let ends_with_sentence_punct = ends_with_sentence_punct(trimmed);

                        if ends_with_sentence_punct && !text_ends_with_abbreviation(trimmed, &abbreviations) {
                            // Complete sentence - emit it immediately
                            lines.push(sentence.clone());
                            current_line.clear();
                        } else {
                            // Incomplete sentence - save for next iteration
                            current_line.clone_from(sentence);
                        }
                    } else {
                        // Complete sentences in the middle
                        lines.push(sentence.clone());
                    }
                }
            } else {
                // Single sentence - check if it's complete
                let trimmed = combined.trim();

                // If the combined result is only whitespace, don't accumulate it.
                // This prevents leading spaces on subsequent elements when lines
                // are joined with spaces during reflow iteration.
                if trimmed.is_empty() {
                    continue;
                }

                let ends_with_sentence_punct = ends_with_sentence_punct(trimmed);

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
            handle_emphasis_sentence_split(
                content,
                marker,
                &abbreviations,
                require_sentence_capital,
                &mut current_line,
                &mut lines,
            );
        } else if let Element::Bold { content, underscore } = element {
            // Handle bold elements - may contain multiple sentences that need continuation
            let marker = if *underscore { "__" } else { "**" };
            handle_emphasis_sentence_split(
                content,
                marker,
                &abbreviations,
                require_sentence_capital,
                &mut current_line,
                &mut lines,
            );
        } else if let Element::Strikethrough { content, double } = element {
            // Handle strikethrough elements - may contain multiple sentences that need continuation
            handle_emphasis_sentence_split(
                content,
                if *double { "~~" } else { "~" },
                &abbreviations,
                require_sentence_capital,
                &mut current_line,
                &mut lines,
            );
        } else {
            // Non-text, non-emphasis elements (Code, Links, etc.)
            // Check if this element is adjacent to the preceding text (no space between)
            let is_adjacent = if idx > 0 {
                match &elements[idx - 1] {
                    Element::Text(t) => !t.is_empty() && !t.ends_with(char::is_whitespace),
                    _ => true,
                }
            } else {
                false
            };

            // Add space before element if needed, but not for adjacent elements
            if !is_adjacent && should_insert_space_before_join(&current_line) {
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
    require_sentence_capital: bool,
    current_line: &mut String,
    lines: &mut Vec<String>,
) {
    // Split the emphasis content into sentences
    let sentences = split_into_sentences_with_set(content, abbreviations, require_sentence_capital);

    if sentences.len() <= 1 {
        // Single sentence or no boundaries - treat as atomic
        if should_insert_space_before_join(current_line) {
            current_line.push(' ');
        }
        current_line.push_str(marker);
        current_line.push_str(content);
        current_line.push_str(marker);

        // Check if the emphasis content ends with sentence punctuation - if so, emit
        let trimmed = content.trim();
        let ends_with_punct = ends_with_sentence_punct(trimmed);
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
                if should_insert_space_before_join(current_line) {
                    current_line.push(' ');
                }
                current_line.push_str(marker);
                current_line.push_str(trimmed);
                current_line.push_str(marker);

                // Check if this is a complete sentence
                let ends_with_punct = ends_with_sentence_punct(trimmed);
                if ends_with_punct && !text_ends_with_abbreviation(trimmed, abbreviations) {
                    lines.push(current_line.clone());
                    current_line.clear();
                }
            } else if i == sentences.len() - 1 {
                // Last sentence: check if complete
                let ends_with_punct = ends_with_sentence_punct(trimmed);

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

/// English break-words used for semantic line break splitting.
/// These are conjunctions and relative pronouns where a line break
/// reads naturally.
const BREAK_WORDS: &[&str] = &[
    "and",
    "or",
    "but",
    "nor",
    "yet",
    "so",
    "for",
    "which",
    "that",
    "because",
    "when",
    "if",
    "while",
    "where",
    "although",
    "though",
    "unless",
    "since",
    "after",
    "before",
    "until",
    "as",
    "once",
    "whether",
    "however",
    "therefore",
    "moreover",
    "furthermore",
    "nevertheless",
    "whereas",
];

/// Check if a character is clause punctuation for semantic line breaks
fn is_clause_punctuation(c: char) -> bool {
    matches!(c, ',' | ';' | ':' | '\u{2014}') // comma, semicolon, colon, em dash
}

/// Whether a clause-punctuation char at `chars[i]` is a legitimate break point.
///
/// A real clause boundary is followed by whitespace (or ends the text): `,;:`
/// with no following space sit *inside* a token (`16:9`, `key:value`, a MyST role
/// like `{cite:p}`) and must not be split there. The em dash (`—`) is exempt:
/// it commonly joins words with no surrounding spaces and breaking after it reads
/// naturally.
fn clause_break_allowed_after(chars: &[char], i: usize) -> bool {
    if chars[i] == '\u{2014}' {
        return true;
    }
    match chars.get(i + 1) {
        None => true,
        Some(next) => next.is_whitespace(),
    }
}

/// Find the closing `)` that balances the `(` at the start of `slice`.
///
/// `offset` is the byte position of the `(` in the original full-line string;
/// it is used to translate local byte positions into global positions for
/// element-span lookups.  Parens inside markdown element spans are skipped so
/// that, e.g., the closing `)` of an inline link does not prematurely end the
/// scan.  The char's *start* byte (not byte-after) is used for the span check
/// so that closing element delimiters — which sit exactly at the span's
/// exclusive-end boundary — are correctly excluded.
///
/// Returns `(end_local, inner)` where `end_local` is the byte offset within
/// `slice` just past the closing `)`, and `inner` is the content between the
/// outermost `(` and `)`.
fn paren_group_end<'a>(slice: &'a str, element_spans: &[(usize, usize)], offset: usize) -> Option<(usize, &'a str)> {
    debug_assert!(slice.starts_with('('));
    let mut depth: i32 = 0;
    for (local_byte, c) in slice.char_indices() {
        let global_byte = offset + local_byte;
        // When depth > 0, skip parens that belong to a markdown element.
        // Use the char's start byte so that a closing element delimiter
        // (whose byte_after equals the span's exclusive end) is treated as
        // inside the element rather than outside it.
        if depth > 0 && is_inside_element(global_byte, element_spans) {
            continue;
        }
        match c {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    let end = local_byte + 1;
                    let inner = &slice[1..local_byte];
                    return Some((end, inner));
                }
            }
            _ => {}
        }
    }
    None
}

/// Split a line at a parenthetical boundary for semantic line breaks.
///
/// Two strategies are tried in order:
///
/// 1. **Leading parenthetical** — if the line begins with `(`, isolate the
///    entire balanced group on this line and start the rest on the next.
///    This handles lines produced by a prior split that placed a `(` at the
///    very beginning.
///
/// 2. **Mid-line parenthetical** — find the rightmost balanced `(…)` whose
///    content spans multiple words and whose preceding text fits within
///    `[min_first_len, line_length]`.  Split just before the `(` so the
///    parenthetical begins the following line.
///
/// Parentheses that fall inside markdown element spans (links, code, etc.)
/// are ignored in both strategies.
fn split_at_parenthetical(
    text: &str,
    line_length: usize,
    element_spans: &[(usize, usize)],
    length_mode: ReflowLengthMode,
) -> Option<(String, String)> {
    let min_first_len = ((line_length as f64) * MIN_SPLIT_RATIO) as usize;

    // Strategy 1: text starts with '(' — isolate the parenthetical as its own line.
    if text.starts_with('(')
        && let Some((end_local, inner)) = paren_group_end(text, element_spans, 0)
        && inner.contains(' ')
    {
        // If closing quotes or clause punctuation immediately follow the closing
        // ')', attach them to the parenthetical so the continuation line does
        // not start with a bare quote, comma, or semicolon.
        let tail = &text[end_local..];
        let attached_len = tail
            .char_indices()
            .take_while(|(_, c)| is_closing_quote(*c) || is_clause_punctuation(*c))
            .last()
            .map_or(0, |(idx, c)| idx + c.len_utf8());
        let first_end = end_local + attached_len;
        let rest_start = first_end;
        let first = &text[..first_end];
        let first_len = display_len(first, length_mode);
        // No MIN_SPLIT_RATIO check: a parenthetical unit is always a valid
        // semantic line regardless of its length.
        if first_len <= line_length {
            let rest = text[rest_start..].trim_start();
            if !rest.is_empty() {
                return Some((first.to_string(), rest.to_string()));
            }
        }
    }

    // Strategy 2: find the rightmost multi-word '(' whose preceding text fits.
    let mut best_open_byte: Option<usize> = None;
    let mut pos = 0usize;
    while pos < text.len() {
        // '(' is ASCII so a single-byte comparison is safe in UTF-8.
        if text.as_bytes()[pos] != b'(' {
            let c = text[pos..].chars().next().unwrap();
            pos += c.len_utf8();
            continue;
        }
        // Skip '(' that are part of a markdown element (use start byte).
        if is_inside_element(pos, element_spans) {
            pos += 1;
            continue;
        }
        if let Some((end_local, inner)) = paren_group_end(&text[pos..], element_spans, pos) {
            let first = text[..pos].trim_end();
            let first_len = display_len(first, length_mode);
            if !first.is_empty()
                && first_len >= min_first_len
                && first_len <= line_length
                && inner.contains(' ')
                && best_open_byte.is_none_or(|prev| pos > prev)
            {
                best_open_byte = Some(pos);
            }
            pos += end_local;
        } else {
            pos += 1;
        }
    }

    let open_byte = best_open_byte?;
    let first = text[..open_byte].trim_end().to_string();
    let rest = text[open_byte..].to_string();
    if first.is_empty() || rest.trim().is_empty() {
        return None;
    }
    Some((first, rest))
}

/// Compute element spans for a flat text representation of elements.
/// Returns Vec of (start, end) byte offsets for non-Text elements,
/// so we can check that a split position doesn't fall inside them.
fn compute_element_spans(elements: &[Element]) -> Vec<(usize, usize)> {
    let mut spans = Vec::new();
    let mut offset = 0;
    for element in elements {
        let rendered = format!("{element}");
        let len = rendered.len();
        if !matches!(element, Element::Text(_)) {
            spans.push((offset, offset + len));
        }
        offset += len;
    }
    spans
}

/// Check if a byte position falls inside any non-Text element span
fn is_inside_element(pos: usize, spans: &[(usize, usize)]) -> bool {
    spans.iter().any(|(start, end)| pos > *start && pos < *end)
}

/// Minimum fraction of line_length that the first part of a split must occupy.
/// Prevents awkwardly short first lines like "A," or "Note:" on their own.
const MIN_SPLIT_RATIO: f64 = 0.3;

/// Split a line at the latest clause punctuation that keeps the first part
/// within `line_length`. Returns None if no valid split point exists or if
/// the split would create an unreasonably short first line.
fn split_at_clause_punctuation(
    text: &str,
    line_length: usize,
    element_spans: &[(usize, usize)],
    length_mode: ReflowLengthMode,
) -> Option<(String, String)> {
    let chars: Vec<char> = text.chars().collect();
    let min_first_len = ((line_length as f64) * MIN_SPLIT_RATIO) as usize;

    // Find the char index where accumulated display width exceeds line_length
    let mut width_acc = 0;
    let mut search_end_char = 0;
    for (idx, &c) in chars.iter().enumerate() {
        let c_width = display_len(&c.to_string(), length_mode);
        if width_acc + c_width > line_length {
            break;
        }
        width_acc += c_width;
        search_end_char = idx + 1;
    }

    // Scan backwards tracking parenthesis depth to skip clause punctuation
    // inside plain-text parenthetical groups.  Scanning right-to-left means
    // ')' opens a depth level and '(' closes it.  Parens that belong to a
    // markdown element are excluded using the char's start byte (not byte-after)
    // so that closing element delimiters at the span boundary are correctly
    // treated as part of the element.
    let mut paren_depth: i32 = 0;
    let mut best_pos = None;
    for i in (0..search_end_char).rev() {
        // Start byte of char i (for paren element check)
        let byte_start: usize = chars[..i].iter().map(|c| c.len_utf8()).sum();
        // Byte just after char i (for clause punctuation element check — existing convention)
        let byte_after: usize = byte_start + chars[i].len_utf8();

        if !is_inside_element(byte_start, element_spans) {
            match chars[i] {
                ')' => paren_depth += 1,
                '(' => paren_depth = paren_depth.saturating_sub(1),
                _ => {}
            }
        }

        if paren_depth == 0
            && is_clause_punctuation(chars[i])
            && clause_break_allowed_after(&chars, i)
            && !is_inside_element(byte_after, element_spans)
        {
            best_pos = Some(i);
            break;
        }
    }

    let pos = best_pos?;

    // Reject splits that create very short first lines
    let first: String = chars[..=pos].iter().collect();
    let first_display_len = display_len(&first, length_mode);
    if first_display_len < min_first_len {
        return None;
    }

    // Split after the punctuation character
    let rest: String = chars[pos + 1..].iter().collect();
    let rest = rest.trim_start().to_string();

    if rest.is_empty() {
        return None;
    }

    Some((first, rest))
}

/// Compute plain-text paren-depth at each byte offset in `text`.
///
/// Returns a `Vec<i32>` of length `text.len()` where entry `i` is the
/// nesting depth at byte `i` — counting only `(` and `)` that fall
/// outside markdown element spans.  This lets callers quickly check
/// whether a byte position lies inside a plain-text parenthetical group.
fn paren_depth_map(text: &str, element_spans: &[(usize, usize)]) -> Vec<i32> {
    let mut map = vec![0i32; text.len()];
    let mut depth = 0i32;
    for (byte, c) in text.char_indices() {
        if !is_inside_element(byte, element_spans) {
            match c {
                '(' => depth += 1,
                ')' => depth = depth.saturating_sub(1),
                _ => {}
            }
        }
        // Fill the depth value for every byte of this (possibly multi-byte) char.
        let end = (byte + c.len_utf8()).min(map.len());
        for slot in &mut map[byte..end] {
            *slot = depth;
        }
    }
    map
}

/// Return `true` if `line` is a complete, balanced, multi-word parenthetical
/// group — i.e. it starts with `(`, ends with `)` (possibly followed by
/// clause punctuation), has balanced parens throughout, and the inner content
/// contains at least one space (matching the ≥2-word threshold used by
/// `split_at_parenthetical` when deciding to split).
///
/// Used to prevent the short-line merge step from collapsing intentional
/// parenthetical splits back into the previous line.
fn is_standalone_parenthetical(line: &str) -> bool {
    let trimmed = line.trim();
    if !trimmed.starts_with('(') {
        return false;
    }
    // Strip optional trailing clause punctuation to find the real end.
    let core = trimmed.trim_end_matches(|c: char| is_clause_punctuation(c));
    if !core.ends_with(')') {
        return false;
    }
    // Inner content must span multiple words (same threshold as split_at_parenthetical).
    let inner = &core[1..core.len() - 1];
    if !inner.contains(' ') {
        return false;
    }
    // Verify the parens are balanced (depth returns to 0 at the last ')').
    let mut depth = 0i32;
    for c in core.chars() {
        match c {
            '(' => depth += 1,
            ')' => depth -= 1,
            _ => {}
        }
        if depth < 0 {
            return false;
        }
    }
    depth == 0
}

/// Split a line before the latest break-word that keeps the first part
/// within `line_length`. Returns None if no valid split point exists or if
/// the split would create an unreasonably short first line.
fn split_at_break_word(
    text: &str,
    line_length: usize,
    element_spans: &[(usize, usize)],
    length_mode: ReflowLengthMode,
) -> Option<(String, String)> {
    let lower = text.to_lowercase();
    let min_first_len = ((line_length as f64) * MIN_SPLIT_RATIO) as usize;
    let mut best_split: Option<(usize, usize)> = None; // (byte_start, word_len_bytes)

    // Build a paren-depth map so we can skip break-words inside plain-text
    // parenthetical groups (matching the protection added to split_at_clause_punctuation).
    let depth_map = paren_depth_map(text, element_spans);

    for &word in BREAK_WORDS {
        let mut search_start = 0;
        while let Some(pos) = lower[search_start..].find(word) {
            let abs_pos = search_start + pos;

            // Verify it's a word boundary: preceded by space, followed by space
            let preceded_by_space = abs_pos == 0 || text.as_bytes().get(abs_pos - 1) == Some(&b' ');
            let followed_by_space = text.as_bytes().get(abs_pos + word.len()) == Some(&b' ');

            if preceded_by_space && followed_by_space {
                // The break goes BEFORE the word, so first part ends at abs_pos - 1
                let first_part = text[..abs_pos].trim_end();
                let first_part_len = display_len(first_part, length_mode);

                // Skip break-words inside plain-text parenthetical groups.
                let inside_paren = depth_map.get(abs_pos).is_some_and(|&d| d > 0);

                if first_part_len >= min_first_len
                    && first_part_len <= line_length
                    && !is_inside_element(abs_pos, element_spans)
                    && !inside_paren
                {
                    // Prefer the latest valid split point
                    if best_split.is_none_or(|(prev_pos, _)| abs_pos > prev_pos) {
                        best_split = Some((abs_pos, word.len()));
                    }
                }
            }

            search_start = abs_pos + word.len();
        }
    }

    let (byte_start, _word_len) = best_split?;

    let first = text[..byte_start].trim_end().to_string();
    let rest = text[byte_start..].to_string();

    if first.is_empty() || rest.trim().is_empty() {
        return None;
    }

    Some((first, rest))
}

/// Cascade-split a line that exceeds line_length.
/// Tries parenthetical boundaries, then clause punctuation, then break-words,
/// then word wrap.
///
/// This is iterative rather than recursive so a single very long line (tens of
/// thousands of words) cannot overflow the stack. Each accepted split shrinks
/// the remaining text by a non-empty prefix, so the loop always makes progress.
/// The whole line is parsed into markdown elements once up front; every
/// remaining suffix reuses those element spans (re-based to the suffix offset)
/// instead of re-parsing, which keeps repeated element parsing out of the loop.
fn cascade_split_line(
    text: &str,
    line_length: usize,
    abbreviations: &Option<Vec<String>>,
    length_mode: ReflowLengthMode,
    attr_lists: bool,
    myst_roles: bool,
    defined_references: Option<&HashSet<String>>,
) -> Vec<String> {
    if line_length == 0 || display_len(text, length_mode) <= line_length {
        return vec![text.to_string()];
    }

    let elements = parse_markdown_elements_inner(text, attr_lists, myst_roles, defined_references);
    let element_spans = compute_element_spans(&elements);

    // Element spans of the remaining suffix `text[start..]`, re-based so their
    // offsets are relative to the suffix. Split points never fall inside an
    // element, so every span lies wholly before or wholly at/after `start`.
    let rebased_spans = |start: usize| -> Vec<(usize, usize)> {
        if start == 0 {
            return element_spans.clone();
        }
        element_spans
            .iter()
            .filter(|&&(_, end)| end > start)
            .map(|&(s, e)| (s.saturating_sub(start), e.saturating_sub(start)))
            .collect()
    };

    let mut result = Vec::new();
    let mut start = 0usize;

    loop {
        let remaining = &text[start..];
        if display_len(remaining, length_mode) <= line_length {
            result.push(remaining.to_string());
            return result;
        }

        let spans = rebased_spans(start);

        // `rest` is always a suffix of `remaining` (the splitters only trim its
        // leading whitespace), so `remaining.len() - rest.len()` is the number of
        // bytes consumed, and the new absolute offset is `start + consumed`.
        let split = split_at_parenthetical(remaining, line_length, &spans, length_mode)
            .or_else(|| split_at_clause_punctuation(remaining, line_length, &spans, length_mode))
            .or_else(|| split_at_break_word(remaining, line_length, &spans, length_mode));

        if let Some((first, rest)) = split {
            let consumed = remaining.len().saturating_sub(rest.len());
            // Defensive: a zero-length advance would loop forever. Splitters only
            // return a non-empty `first`, so this never triggers, but guard anyway.
            if consumed == 0 {
                break;
            }
            result.push(first);
            start += consumed;
            continue;
        }

        // No semantic split point: word-wrap the remaining suffix and finish.
        break;
    }

    // Fallback: word wrap the still-oversized suffix using reflow_elements.
    let options = ReflowOptions {
        line_length,
        break_on_sentences: false,
        preserve_breaks: false,
        sentence_per_line: false,
        semantic_line_breaks: false,
        abbreviations: abbreviations.clone(),
        length_mode,
        attr_lists,
        myst_roles,
        require_sentence_capital: true,
        max_list_continuation_indent: None,
        // Unused here: this fallback arranges already-parsed elements and never
        // re-parses links, so shortcut definedness is never consulted.
        defined_references: None,
    };
    let remaining = &text[start..];
    let tail_elements = if start == 0 {
        elements
    } else {
        parse_markdown_elements_inner(remaining, attr_lists, myst_roles, defined_references)
    };
    result.extend(reflow_elements(&tail_elements, &options));
    result
}

/// Reflow elements using semantic line breaks strategy:
/// 1. Split at sentence boundaries (always)
/// 2. For lines exceeding line_length, cascade through clause punct → break-words → word wrap
fn reflow_elements_semantic(elements: &[Element], options: &ReflowOptions) -> Vec<String> {
    // Step 1: Split into sentences using existing sentence-per-line logic
    let sentence_lines =
        reflow_elements_sentence_per_line(elements, &options.abbreviations, options.require_sentence_capital);

    // Step 2: For each sentence line, apply cascading splits if it exceeds line_length
    // When line_length is 0 (unlimited), skip cascading — sentence splits only
    if options.line_length == 0 {
        return sentence_lines;
    }

    let length_mode = options.length_mode;
    let mut result = Vec::new();
    for line in sentence_lines {
        if display_len(&line, length_mode) <= options.line_length {
            result.push(line);
        } else {
            result.extend(cascade_split_line(
                &line,
                options.line_length,
                &options.abbreviations,
                length_mode,
                options.attr_lists,
                options.myst_roles,
                options.defined_references.as_ref(),
            ));
        }
    }

    // Step 3: Merge very short trailing lines back into the previous line.
    // Word wrap can produce lines like "was" or "see" on their own, which reads poorly.
    let min_line_len = ((options.line_length as f64) * MIN_SPLIT_RATIO) as usize;
    let mut merged: Vec<String> = Vec::with_capacity(result.len());
    for line in result {
        if !merged.is_empty() && display_len(&line, length_mode) < min_line_len && !line.trim().is_empty() {
            // Don't merge a line that is itself a standalone parenthetical group —
            // it was placed on its own line intentionally by split_at_parenthetical.
            if is_standalone_parenthetical(&line) {
                merged.push(line);
                continue;
            }

            // Don't merge across sentence boundaries — sentence splits are intentional
            let prev_ends_at_sentence = {
                let trimmed = merged.last().unwrap().trim_end();
                trimmed
                    .chars()
                    .rev()
                    .find(|c| !matches!(c, '"' | '\'' | '\u{201D}' | '\u{2019}' | ')' | ']'))
                    .is_some_and(|c| matches!(c, '.' | '!' | '?'))
            };

            if !prev_ends_at_sentence {
                let prev = merged.last_mut().unwrap();
                let combined = format!("{prev} {line}");
                // Only merge if the combined line fits within the limit
                if display_len(&combined, length_mode) <= options.line_length {
                    *prev = combined;
                    continue;
                }
            }
        }
        merged.push(line);
    }
    merged
}

/// Find the last space in `line` that is safe to split at.
/// Safe spaces are those NOT inside rendered non-Text elements and whose
/// suffix would not open a block construct when placed at line start.
/// `element_spans` contains (start, end) byte ranges of non-Text elements in
/// the line. Spans use exclusive bounds (pos > start && pos < end) because
/// element delimiters (e.g., `[`, `]`, `(`, `)`, `<`, `>`, `` ` ``) are never
/// spaces, so only interior positions need protection. The scan keeps looking
/// left past construct-leading suffixes (e.g. a trailing `- `), so a usable
/// earlier break point is found instead of forcing an overlong line.
fn rfind_safe_space(line: &str, element_spans: &[(usize, usize)]) -> Option<usize> {
    line.char_indices().rev().map(|(pos, _)| pos).find(|&pos| {
        line.as_bytes()[pos] == b' '
            && !element_spans.iter().any(|(s, e)| pos > *s && pos < *e)
            && !starts_block_construct(&line[pos + 1..])
    })
}

/// Reflow elements into lines that fit within the line length
fn reflow_elements(elements: &[Element], options: &ReflowOptions) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current_line = String::new();
    let mut current_length = 0;
    // Track byte spans of non-Text elements in current_line for safe splitting
    let mut current_line_element_spans: Vec<(usize, usize)> = Vec::new();
    let length_mode = options.length_mode;

    for (idx, element) in elements.iter().enumerate() {
        // Derive the display width from the already-formatted string rather than
        // formatting the element a second time just to measure it.
        let element_str = format!("{element}");
        let element_len = display_len(&element_str, length_mode);

        // Determine adjacency from the original elements, not from current_line.
        // Elements are adjacent when there's no whitespace between them in the source:
        // - Text("v") → HugoShortcode("{{<...>}}") = adjacent (text has no trailing space)
        // - Text(" and ") → InlineLink("[a](url)") = NOT adjacent (text has trailing space)
        // - HugoShortcode("{{<...>}}") → Text(",") = adjacent (text has no leading space)
        let is_adjacent_to_prev = if idx > 0 {
            match (&elements[idx - 1], element) {
                (Element::Text(t), _) => !t.is_empty() && !t.ends_with(char::is_whitespace),
                (_, Element::Text(t)) => !t.is_empty() && !t.starts_with(char::is_whitespace),
                _ => true,
            }
        } else {
            false
        };

        // For text elements that might need breaking
        if let Element::Text(text) = element {
            // Check if original text had leading whitespace
            let has_leading_space = text.starts_with(char::is_whitespace);
            // If this is a text element, always process it word by word
            let words: Vec<&str> = text.split_whitespace().collect();

            for (i, word) in words.iter().enumerate() {
                let word_len = display_len(word, length_mode);
                // Check if this "word" is just punctuation that should stay attached
                let is_trailing_punct = word
                    .chars()
                    .all(|c| matches!(c, ',' | '.' | ':' | ';' | '!' | '?' | ')' | ']' | '}'));

                // First word of text adjacent to preceding non-text element
                // must stay attached (e.g., shortcode followed by punctuation or text)
                let is_first_adjacent = i == 0 && is_adjacent_to_prev;

                if is_first_adjacent {
                    // Attach directly without space, preventing line break
                    if current_length + word_len > options.line_length && current_length > 0 {
                        // Would exceed — break before the adjacent group
                        // Use element-aware space search to avoid splitting inside links/code/etc.
                        // Never hoist text that would open a block construct to line start.
                        if let Some(last_space) = rfind_safe_space(&current_line, &current_line_element_spans) {
                            let before = current_line[..last_space].trim_end().to_string();
                            let after = current_line[last_space + 1..].to_string();
                            lines.push(before);
                            current_line = format!("{after}{word}");
                            current_length = display_len(&current_line, length_mode);
                            current_line_element_spans.clear();
                        } else {
                            current_line.push_str(word);
                            current_length += word_len;
                        }
                    } else {
                        current_line.push_str(word);
                        current_length += word_len;
                    }
                } else if current_length > 0
                    && current_length + 1 + word_len > options.line_length
                    && !is_trailing_punct
                {
                    if !starts_block_construct(word) {
                        // Start a new line (but never for trailing punctuation)
                        lines.push(current_line.trim().to_string());
                        current_line = word.to_string();
                        current_length = word_len;
                        current_line_element_spans.clear();
                    } else if let Some(last_space) = rfind_safe_space(&current_line, &current_line_element_spans) {
                        // The overflowing word would open a block construct at line
                        // start. Break one word earlier instead so the marker stays
                        // mid-line: "... and then" + "- clause" becomes "... and" +
                        // "then - clause".
                        let before = current_line[..last_space].trim_end().to_string();
                        let after = current_line[last_space + 1..].to_string();
                        lines.push(before);
                        current_line = format!("{after} {word}");
                        current_length = display_len(&current_line, length_mode);
                        current_line_element_spans.clear();
                    } else {
                        // No safe earlier break point — keep the marker attached and
                        // accept the long line rather than corrupt the structure.
                        if i > 0 || has_leading_space {
                            current_line.push(' ');
                            current_length += 1;
                        }
                        current_line.push_str(word);
                        current_length += word_len;
                    }
                } else {
                    // Add a space only where the source had whitespace at this position.
                    // For the first word of a text run (i == 0) that means the source had a
                    // leading space — and reaching this branch already implies the word is
                    // not adjacent to the previous element, so the space is real and must be
                    // kept even for punctuation. Suppressing it here would delete the space
                    // after an inline element, e.g. `` `code` } `` -> `` `code`} ``. The
                    // no-space (adjacent) case is handled above by `is_first_adjacent`.
                    // Within a text run (i > 0) trailing punctuation still attaches to the
                    // preceding word.
                    let add_space = current_length > 0 && if i == 0 { has_leading_space } else { !is_trailing_punct };
                    if add_space {
                        current_line.push(' ');
                        current_length += 1;
                    }
                    current_line.push_str(word);
                    current_length += word_len;
                }
            }
        } else if matches!(
            element,
            Element::Italic { .. } | Element::Bold { .. } | Element::Strikethrough { .. }
        ) && element_len > options.line_length
        {
            // Italic, bold, and strikethrough with content longer than line_length need word wrapping.
            // Split content word-by-word, attach the opening marker to the first word
            // and the closing marker to the last word.
            let (content, marker): (&str, &str) = match element {
                Element::Italic { content, underscore } => (content.as_str(), if *underscore { "_" } else { "*" }),
                Element::Bold { content, underscore } => (content.as_str(), if *underscore { "__" } else { "**" }),
                Element::Strikethrough { content, double } => (content.as_str(), if *double { "~~" } else { "~" }),
                _ => unreachable!(),
            };

            let words: Vec<&str> = content.split_whitespace().collect();
            let n = words.len();

            if n == 0 {
                // Empty span — treat as atomic
                let full = format!("{marker}{marker}");
                let full_len = display_len(&full, length_mode);
                if !is_adjacent_to_prev && current_length > 0 {
                    current_line.push(' ');
                    current_length += 1;
                }
                current_line.push_str(&full);
                current_length += full_len;
            } else {
                for (i, word) in words.iter().enumerate() {
                    let is_first = i == 0;
                    let is_last = i == n - 1;
                    let word_str: String = match (is_first, is_last) {
                        (true, true) => format!("{marker}{word}{marker}"),
                        (true, false) => format!("{marker}{word}"),
                        (false, true) => format!("{word}{marker}"),
                        (false, false) => word.to_string(),
                    };
                    let word_len = display_len(&word_str, length_mode);

                    let needs_space = if is_first {
                        !is_adjacent_to_prev && current_length > 0
                    } else {
                        current_length > 0
                    };

                    if needs_space
                        && current_length + 1 + word_len > options.line_length
                        && !starts_block_construct(&word_str)
                    {
                        lines.push(current_line.trim_end().to_string());
                        current_line = word_str;
                        current_length = word_len;
                        current_line_element_spans.clear();
                    } else {
                        if needs_space {
                            current_line.push(' ');
                            current_length += 1;
                        }
                        current_line.push_str(&word_str);
                        current_length += word_len;
                    }
                }
            }
        } else {
            // For non-text elements (code, links, references), treat as atomic units
            // These should never be broken across lines

            if is_adjacent_to_prev {
                // Adjacent to preceding text — attach directly without space
                if current_length + element_len > options.line_length {
                    // Would exceed limit — break before the adjacent word group
                    // Use element-aware space search to avoid splitting inside links/code/etc.
                    // Never hoist text that would open a block construct to line start.
                    if let Some(last_space) = rfind_safe_space(&current_line, &current_line_element_spans) {
                        let before = current_line[..last_space].trim_end().to_string();
                        let after = current_line[last_space + 1..].to_string();
                        lines.push(before);
                        current_line = format!("{after}{element_str}");
                        current_length = display_len(&current_line, length_mode);
                        current_line_element_spans.clear();
                        // Record the element span in the new current_line
                        let start = after.len();
                        current_line_element_spans.push((start, start + element_str.len()));
                    } else {
                        // No safe space to break at — accept the long line
                        let start = current_line.len();
                        current_line.push_str(&element_str);
                        current_length += element_len;
                        current_line_element_spans.push((start, current_line.len()));
                    }
                } else {
                    let start = current_line.len();
                    current_line.push_str(&element_str);
                    current_length += element_len;
                    current_line_element_spans.push((start, current_line.len()));
                }
            } else if current_length > 0 && current_length + 1 + element_len > options.line_length {
                if !starts_block_construct(&element_str) {
                    // Not adjacent, would exceed — start new line
                    lines.push(current_line.trim().to_string());
                    current_line.clone_from(&element_str);
                    current_length = element_len;
                    current_line_element_spans.clear();
                    current_line_element_spans.push((0, element_str.len()));
                } else if let Some(last_space) = rfind_safe_space(&current_line, &current_line_element_spans) {
                    // The overflowing element would open a block construct at
                    // line start (e.g. an HtmlTag like `<div>`). Break one word
                    // earlier instead so the element stays mid-line.
                    let before = current_line[..last_space].trim_end().to_string();
                    let after = current_line[last_space + 1..].to_string();
                    lines.push(before);
                    current_line = format!("{after} {element_str}");
                    current_length = display_len(&current_line, length_mode);
                    current_line_element_spans.clear();
                    let start = after.len() + 1;
                    current_line_element_spans.push((start, start + element_str.len()));
                } else {
                    // No safe earlier break point — keep the element attached
                    // and accept the long line rather than corrupt the structure.
                    let ends_with_opener =
                        current_line.ends_with('(') || current_line.ends_with('[') || current_line.ends_with('{');
                    if !ends_with_opener {
                        current_line.push(' ');
                        current_length += 1;
                    }
                    let start = current_line.len();
                    current_line.push_str(&element_str);
                    current_length += element_len;
                    current_line_element_spans.push((start, current_line.len()));
                }
            } else {
                // Not adjacent, fits — add with space
                let ends_with_opener =
                    current_line.ends_with('(') || current_line.ends_with('[') || current_line.ends_with('{');
                if current_length > 0 && !ends_with_opener {
                    current_line.push(' ');
                    current_length += 1;
                }
                let start = current_line.len();
                current_line.push_str(&element_str);
                current_length += element_len;
                current_line_element_spans.push((start, current_line.len()));
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

        // Preserve Quarto/Pandoc div markers (:::) as-is
        if trimmed.starts_with(":::") {
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
        if calculate_indentation_width_default(line) >= 4 {
            // Collect all consecutive indented lines
            result.push(line.to_string());
            i += 1;
            while i < lines.len() {
                let next_line = lines[i];
                // Continue if next line is also indented or empty (empty lines in code blocks are ok)
                if calculate_indentation_width_default(next_line) >= 4 || next_line.trim().is_empty() {
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
            let quote_prefix = line[0..=gt_pos].to_string();
            let quote_content = &line[quote_prefix.len()..].trim_start();

            let reflowed = reflow_line(quote_content, options);
            for reflowed_line in &reflowed {
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
        if is_unordered_list_marker(trimmed) || is_numbered_list_item(trimmed) {
            // Find the list marker and preserve indentation
            let indent = line.len() - line.trim_start().len();
            let indent_str = " ".repeat(indent);

            // For numbered lists, find the period and the space after it
            // For bullet lists, find the marker and the space after it
            let mut marker_end = indent;
            let mut content_start = indent;

            if trimmed.chars().next().is_some_and(char::is_numeric) {
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

            // Minimum indent for continuation lines (based on list marker, before checkbox)
            let min_continuation_indent = content_start;

            // Detect checkbox/task list markers: [ ], [x], [X]
            // GFM task lists work with both unordered and ordered lists
            let rest = &line[content_start..];
            if rest.starts_with("[ ] ") || rest.starts_with("[x] ") || rest.starts_with("[X] ") {
                marker_end = content_start + 3; // Include the checkbox `[ ]`
                content_start += 4; // Skip past `[ ] `
            }

            let marker = &line[indent..marker_end];

            // Collect all content for this list item (including continuation lines)
            // Preserve hard breaks (2 trailing spaces) while trimming excessive whitespace
            let mut list_content = vec![trim_preserving_hard_break(&line[content_start..])];
            i += 1;

            // Collect continuation lines (indented lines that are part of this list item)
            // Use the base marker indent (not checkbox-extended) for collection,
            // since users may indent continuations to the bullet level, not the checkbox level
            while i < lines.len() {
                let next_line = lines[i];
                let next_trimmed = next_line.trim();

                // Stop if we hit an empty line or another list item or special block
                if is_block_boundary(next_trimmed) {
                    break;
                }

                // Check if this line is indented (continuation of list item)
                let next_indent = next_line.len() - next_line.trim_start().len();
                if next_indent >= min_continuation_indent {
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
            let continuation_spaces = if let Some(max_indent) = options.max_list_continuation_indent {
                // Cap the relative indent (past the nesting level) to max_indent,
                // then add back the nesting indent so nested items stay correct
                indent + (content_start - indent).min(max_indent)
            } else {
                content_start
            };

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
            let next_trimmed = lines[i + 1].trim();
            // Check if next line continues this paragraph
            if !is_block_boundary(next_trimmed) {
                is_single_line_paragraph = false;
            }
        }

        // If it's a single line that fits, just add it as-is
        if is_single_line_paragraph && display_len(line, options.length_mode) <= options.line_length {
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
                if is_block_boundary(next_trimmed) {
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

/// A collected blockquote line used for style-preserving reflow.
///
/// The invariant `is_explicit == true` iff `prefix.is_some()` is enforced by the
/// constructors. Use [`BlockquoteLineData::explicit`] or [`BlockquoteLineData::lazy`]
/// rather than constructing the struct directly.
#[derive(Debug, Clone)]
pub struct BlockquoteLineData {
    /// Trimmed content without the `> ` prefix.
    pub(crate) content: String,
    /// Whether this line carries an explicit blockquote marker.
    pub(crate) is_explicit: bool,
    /// Full blockquote prefix (e.g. `"> "`, `"> > "`). `None` for lazy continuation lines.
    pub(crate) prefix: Option<String>,
}

impl BlockquoteLineData {
    /// Create an explicit (marker-bearing) blockquote line.
    pub fn explicit(content: String, prefix: String) -> Self {
        Self {
            content,
            is_explicit: true,
            prefix: Some(prefix),
        }
    }

    /// Create a lazy continuation line (no blockquote marker).
    pub fn lazy(content: String) -> Self {
        Self {
            content,
            is_explicit: false,
            prefix: None,
        }
    }
}

/// Style for blockquote continuation lines after reflow.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockquoteContinuationStyle {
    Explicit,
    Lazy,
}

/// Determine the continuation style for a blockquote paragraph from its collected lines.
///
/// The first line is always explicit (it carries the marker), so only continuation
/// lines (index 1+) are counted. Ties resolve to `Explicit`.
///
/// When the slice has only one element (no continuation lines to inspect), both
/// counts are zero and the tie-breaking rule returns `Explicit`.
pub fn blockquote_continuation_style(lines: &[BlockquoteLineData]) -> BlockquoteContinuationStyle {
    let mut explicit_count = 0usize;
    let mut lazy_count = 0usize;

    for line in lines.iter().skip(1) {
        if line.is_explicit {
            explicit_count += 1;
        } else {
            lazy_count += 1;
        }
    }

    if explicit_count > 0 && lazy_count == 0 {
        BlockquoteContinuationStyle::Explicit
    } else if lazy_count > 0 && explicit_count == 0 {
        BlockquoteContinuationStyle::Lazy
    } else if explicit_count >= lazy_count {
        BlockquoteContinuationStyle::Explicit
    } else {
        BlockquoteContinuationStyle::Lazy
    }
}

/// Determine the dominant blockquote prefix for a paragraph.
///
/// The most frequently occurring explicit prefix wins. Ties are broken by earliest
/// first appearance. Falls back to `fallback` when no explicit lines are present.
pub fn dominant_blockquote_prefix(lines: &[BlockquoteLineData], fallback: &str) -> String {
    let mut counts: std::collections::HashMap<String, (usize, usize)> = std::collections::HashMap::new();

    for (idx, line) in lines.iter().enumerate() {
        let Some(prefix) = line.prefix.as_ref() else {
            continue;
        };
        counts
            .entry(prefix.clone())
            .and_modify(|entry| entry.0 += 1)
            .or_insert((1, idx));
    }

    counts
        .into_iter()
        .max_by(|(_, (count_a, first_idx_a)), (_, (count_b, first_idx_b))| {
            count_a.cmp(count_b).then_with(|| first_idx_b.cmp(first_idx_a))
        })
        .map_or_else(|| fallback.to_string(), |(prefix, _)| prefix)
}

/// Whether a reflowed blockquote content line must carry an explicit prefix.
///
/// Lines that would start a new block structure (headings, fences, lists, etc.)
/// cannot safely use lazy continuation syntax.
pub(crate) fn should_force_explicit_blockquote_line(content_line: &str) -> bool {
    let trimmed = content_line.trim_start();
    trimmed.starts_with('>')
        || trimmed.starts_with('#')
        || trimmed.starts_with("```")
        || trimmed.starts_with("~~~")
        || is_unordered_list_marker(trimmed)
        || is_numbered_list_item(trimmed)
        || is_horizontal_rule(trimmed)
        || is_definition_list_item(trimmed)
        || (trimmed.starts_with('[') && trimmed.contains("]:"))
        || trimmed.starts_with(":::")
        || (trimmed.starts_with('<')
            && !trimmed.starts_with("<http")
            && !trimmed.starts_with("<https")
            && !trimmed.starts_with("<mailto:"))
}

/// Reflow blockquote content lines and apply continuation style.
///
/// Segments separated by hard breaks are reflowed independently. The output lines
/// receive blockquote prefixes according to `continuation_style`: the first line and
/// any line that would start a new block structure always get an explicit prefix;
/// other lines follow the detected style.
///
/// Returns the styled, reflowed lines (without a trailing newline).
pub fn reflow_blockquote_content(
    lines: &[BlockquoteLineData],
    explicit_prefix: &str,
    continuation_style: BlockquoteContinuationStyle,
    options: &ReflowOptions,
) -> Vec<String> {
    let content_strs: Vec<&str> = lines.iter().map(|l| l.content.as_str()).collect();
    let segments = split_into_segments_strs(&content_strs);
    let mut reflowed_content_lines: Vec<String> = Vec::new();

    for segment in segments {
        let hard_break_type = segment.last().and_then(|&line| {
            let line = line.strip_suffix('\r').unwrap_or(line);
            if line.ends_with('\\') {
                Some("\\")
            } else if line.ends_with("  ") {
                Some("  ")
            } else {
                None
            }
        });

        let pieces: Vec<&str> = segment
            .iter()
            .map(|&line| {
                if let Some(l) = line.strip_suffix('\\') {
                    l.trim_end()
                } else if let Some(l) = line.strip_suffix("  ") {
                    l.trim_end()
                } else {
                    line.trim_end()
                }
            })
            .collect();

        let segment_text = pieces.join(" ");
        let segment_text = segment_text.trim();
        if segment_text.is_empty() {
            continue;
        }

        let mut reflowed = reflow_line(segment_text, options);
        if let Some(break_marker) = hard_break_type
            && !reflowed.is_empty()
        {
            let last_idx = reflowed.len() - 1;
            if !has_hard_break(&reflowed[last_idx]) {
                reflowed[last_idx].push_str(break_marker);
            }
        }
        reflowed_content_lines.extend(reflowed);
    }

    let mut styled_lines: Vec<String> = Vec::new();
    for (idx, line) in reflowed_content_lines.iter().enumerate() {
        let force_explicit = idx == 0
            || continuation_style == BlockquoteContinuationStyle::Explicit
            || should_force_explicit_blockquote_line(line);
        if force_explicit {
            styled_lines.push(format!("{explicit_prefix}{line}"));
        } else {
            styled_lines.push(line.clone());
        }
    }

    styled_lines
}

fn is_blockquote_content_boundary(content: &str) -> bool {
    let trimmed = content.trim();
    trimmed.is_empty()
        || is_block_boundary(trimmed)
        || crate::utils::table_utils::TableUtils::is_potential_table_row(content)
        || trimmed.starts_with(":::")
        || crate::utils::is_template_directive_only(content)
        || is_standalone_attr_list(content)
        || is_snippet_block_delimiter(content)
}

fn split_into_segments_strs<'a>(lines: &[&'a str]) -> Vec<Vec<&'a str>> {
    let mut segments = Vec::new();
    let mut current = Vec::new();

    for &line in lines {
        current.push(line);
        if has_hard_break(line) {
            segments.push(current);
            current = Vec::new();
        }
    }

    if !current.is_empty() {
        segments.push(current);
    }

    segments
}

fn reflow_blockquote_paragraph_at_line(
    content: &str,
    lines: &[&str],
    target_idx: usize,
    options: &ReflowOptions,
) -> Option<ParagraphReflow> {
    let mut anchor_idx = target_idx;
    let mut target_level = if let Some(parsed) = crate::utils::blockquote::parse_blockquote_prefix(lines[target_idx]) {
        parsed.nesting_level
    } else {
        let mut found = None;
        let mut idx = target_idx;
        loop {
            if lines[idx].trim().is_empty() {
                break;
            }
            if let Some(parsed) = crate::utils::blockquote::parse_blockquote_prefix(lines[idx]) {
                found = Some((idx, parsed.nesting_level));
                break;
            }
            if idx == 0 {
                break;
            }
            idx -= 1;
        }
        let (idx, level) = found?;
        anchor_idx = idx;
        level
    };

    // Expand backward to capture prior quote content at the same nesting level.
    let mut para_start = anchor_idx;
    while para_start > 0 {
        let prev_idx = para_start - 1;
        let prev_line = lines[prev_idx];

        if prev_line.trim().is_empty() {
            break;
        }

        if let Some(parsed) = crate::utils::blockquote::parse_blockquote_prefix(prev_line) {
            if parsed.nesting_level != target_level || is_blockquote_content_boundary(parsed.content) {
                break;
            }
            para_start = prev_idx;
            continue;
        }

        let prev_lazy = prev_line.trim_start();
        if is_blockquote_content_boundary(prev_lazy) {
            break;
        }
        para_start = prev_idx;
    }

    // Lazy continuation cannot precede the first explicit marker.
    while para_start < lines.len() {
        let Some(parsed) = crate::utils::blockquote::parse_blockquote_prefix(lines[para_start]) else {
            para_start += 1;
            continue;
        };
        target_level = parsed.nesting_level;
        break;
    }

    if para_start >= lines.len() || para_start > target_idx {
        return None;
    }

    // Collect explicit lines at target level and lazy continuation lines.
    // Each entry is (original_line_idx, BlockquoteLineData).
    let mut collected: Vec<(usize, BlockquoteLineData)> = Vec::new();
    let mut idx = para_start;
    while idx < lines.len() {
        if !collected.is_empty() && has_hard_break(&collected[collected.len() - 1].1.content) {
            break;
        }

        let line = lines[idx];
        if line.trim().is_empty() {
            break;
        }

        if let Some(parsed) = crate::utils::blockquote::parse_blockquote_prefix(line) {
            if parsed.nesting_level != target_level || is_blockquote_content_boundary(parsed.content) {
                break;
            }
            collected.push((
                idx,
                BlockquoteLineData::explicit(trim_preserving_hard_break(parsed.content), parsed.prefix.to_string()),
            ));
            idx += 1;
            continue;
        }

        let lazy_content = line.trim_start();
        if is_blockquote_content_boundary(lazy_content) {
            break;
        }

        collected.push((idx, BlockquoteLineData::lazy(trim_preserving_hard_break(lazy_content))));
        idx += 1;
    }

    if collected.is_empty() {
        return None;
    }

    let para_end = collected[collected.len() - 1].0;
    if target_idx < para_start || target_idx > para_end {
        return None;
    }

    let line_data: Vec<BlockquoteLineData> = collected.iter().map(|(_, d)| d.clone()).collect();

    let fallback_prefix = line_data
        .iter()
        .find_map(|d| d.prefix.clone())
        .unwrap_or_else(|| "> ".to_string());
    let explicit_prefix = dominant_blockquote_prefix(&line_data, &fallback_prefix);
    let continuation_style = blockquote_continuation_style(&line_data);

    let adjusted_line_length = options
        .line_length
        .saturating_sub(display_len(&explicit_prefix, options.length_mode))
        .max(1);

    let adjusted_options = ReflowOptions {
        line_length: adjusted_line_length,
        ..options.clone()
    };

    let styled_lines = reflow_blockquote_content(&line_data, &explicit_prefix, continuation_style, &adjusted_options);

    if styled_lines.is_empty() {
        return None;
    }

    // Calculate byte offsets.
    let mut start_byte = 0;
    for line in lines.iter().take(para_start) {
        start_byte += line.len() + 1;
    }

    let mut end_byte = start_byte;
    for line in lines.iter().take(para_end + 1).skip(para_start) {
        end_byte += line.len() + 1;
    }

    let includes_trailing_newline = para_end != lines.len() - 1 || content.ends_with('\n');
    if !includes_trailing_newline {
        end_byte -= 1;
    }

    let reflowed_joined = styled_lines.join("\n");
    let reflowed_text = if includes_trailing_newline {
        if reflowed_joined.ends_with('\n') {
            reflowed_joined
        } else {
            format!("{reflowed_joined}\n")
        }
    } else if reflowed_joined.ends_with('\n') {
        reflowed_joined.trim_end_matches('\n').to_string()
    } else {
        reflowed_joined
    };

    Some(ParagraphReflow {
        start_byte,
        end_byte,
        reflowed_text,
    })
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
    reflow_paragraph_at_line_with_mode(content, line_number, line_length, ReflowLengthMode::default())
}

/// Reflow a paragraph at the given line with a specific length mode.
pub fn reflow_paragraph_at_line_with_mode(
    content: &str,
    line_number: usize,
    line_length: usize,
    length_mode: ReflowLengthMode,
) -> Option<ParagraphReflow> {
    let options = ReflowOptions {
        line_length,
        length_mode,
        ..Default::default()
    };
    reflow_paragraph_at_line_with_options(content, line_number, &options)
}

/// Reflow a paragraph at the given line using the provided options.
///
/// This is the canonical implementation used by both the rule's fix mode and the
/// LSP "Reflow paragraph" action. Passing a fully configured `ReflowOptions` allows
/// the LSP action to respect user-configured reflow mode, abbreviations, etc.
///
/// # Returns
///
/// Returns `Some(ParagraphReflow)` with byte offsets and reflowed text, or `None`
/// if the line is out of bounds or sits inside a non-reflow-able construct.
pub fn reflow_paragraph_at_line_with_options(
    content: &str,
    line_number: usize,
    options: &ReflowOptions,
) -> Option<ParagraphReflow> {
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

    // Handle blockquote paragraphs (including lazy continuation lines) with
    // style-preserving output.
    if let Some(blockquote_reflow) = reflow_blockquote_paragraph_at_line(content, &lines, target_idx, options) {
        return Some(blockquote_reflow);
    }

    // Don't reflow special blocks
    if is_paragraph_boundary(trimmed, target_line) {
        return None;
    }

    // Find paragraph start - scan backward until blank line or special block
    let mut para_start = target_idx;
    while para_start > 0 {
        let prev_idx = para_start - 1;
        let prev_line = lines[prev_idx];
        let prev_trimmed = prev_line.trim();

        // Stop at blank line or special blocks
        if is_paragraph_boundary(prev_trimmed, prev_line) {
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
        if is_paragraph_boundary(next_trimmed, next_line) {
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
    for line in paragraph_lines {
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

    // Reflow the paragraph using reflow_markdown to handle it properly
    let reflowed = reflow_markdown(&paragraph_text, options);

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

    #[test]
    fn cascade_split_line_handles_a_very_long_line_without_overflowing() {
        // A single line of thousands of words once drove `cascade_split_line`
        // into deep recursion (stack overflow / hang). The iterative version
        // must complete and split it into many lines that each fit the width and
        // that together preserve every word. The test finishing at all is the
        // core assertion (no stack overflow); the content checks guard behavior.
        let words: Vec<String> = (0..4000).map(|i| format!("word{i}")).collect();
        let line = words.join(" ");

        let out = cascade_split_line(&line, 80, &None, ReflowLengthMode::Chars, false, false, None);

        assert!(out.len() > 1, "a very long line should split into many lines");
        for segment in &out {
            assert!(
                display_len(segment, ReflowLengthMode::Chars) <= 80 || !segment.contains(' '),
                "each wrapped line should fit the width (or be a single unbreakable token)"
            );
        }
        // Every original word survives, in order.
        let rejoined = out.join(" ");
        let original_words: Vec<&str> = line.split(' ').collect();
        let result_words: Vec<&str> = rejoined.split_whitespace().collect();
        assert_eq!(original_words, result_words, "reflow must preserve all words in order");
    }

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

    #[test]
    fn test_footnote_after_period_splits_sentence() {
        // A footnote reference glued to the period (no space) must not swallow
        // the sentence boundary; the reference stays attached to the sentence
        // it annotates.
        let text = "First sentence.[^1] Second sentence.";
        let sentences = split_into_sentences(text);
        assert_eq!(
            sentences,
            vec!["First sentence.[^1]".to_string(), "Second sentence.".to_string()],
            "footnote glued to the period should keep the boundary and stay attached to the first sentence"
        );
    }

    #[test]
    fn test_multiple_consecutive_footnotes_after_period_splits_sentence() {
        // Multiple footnote references glued back-to-back after the period.
        let text = "Notes here.[^1][^2] Second sentence.";
        let sentences = split_into_sentences(text);
        assert_eq!(
            sentences,
            vec!["Notes here.[^1][^2]".to_string(), "Second sentence.".to_string()]
        );
    }

    #[test]
    fn test_footnote_before_period_still_splits_sentence() {
        // Control: a footnote reference before the period was already followed
        // by a space, so this boundary worked before this fix and must keep
        // working.
        let text = "Annotation here[^1]. Second sentence.";
        let sentences = split_into_sentences(text);
        assert_eq!(
            sentences,
            vec!["Annotation here[^1].".to_string(), "Second sentence.".to_string()]
        );
    }

    #[test]
    fn test_mid_sentence_footnote_does_not_split() {
        // A footnote reference not glued to sentence-ending punctuation must not
        // introduce a spurious boundary at the bracket itself.
        let text = "The system word[^1] more words. Next sentence.";
        let sentences = split_into_sentences(text);
        assert_eq!(
            sentences,
            vec![
                "The system word[^1] more words.".to_string(),
                "Next sentence.".to_string()
            ]
        );
    }

    #[test]
    fn test_bare_numeric_bracket_after_period_does_not_split() {
        // A bare `[1]` is link/citation-like text, not footnote syntax; the fix
        // is scoped to `[^label]` only.
        let text = "Citation here.[1] Second sentence.";
        let sentences = split_into_sentences(text);
        assert_eq!(
            sentences,
            vec![text.to_string()],
            "a bare numeric bracket must not be treated as a sentence boundary"
        );
    }

    #[test]
    fn test_footnote_glued_to_following_word_does_not_split() {
        // No whitespace after the footnote reference means there is nowhere a
        // next sentence can start, so this must not be treated as a boundary.
        let text = "First sentence.[^1]Continued glued text.";
        let sentences = split_into_sentences(text);
        assert_eq!(sentences, vec![text.to_string()]);
    }

    #[test]
    fn test_footnote_at_end_of_text_is_preserved() {
        // A footnote reference at the very end of the text has nothing after it
        // to split off; it is preserved as part of the single trailing sentence.
        let text = "Sentence.[^1]";
        let sentences = split_into_sentences(text);
        assert_eq!(sentences, vec![text.to_string()]);
    }

    #[test]
    fn test_abbreviation_before_footnote_does_not_split() {
        // The existing abbreviation guard must still apply when a footnote
        // reference immediately follows the abbreviation's period.
        let text = "See the notes, e.g.[^1] this one.";
        let sentences = split_into_sentences(text);
        assert_eq!(
            sentences,
            vec![text.to_string()],
            "e.g. is an abbreviation, not a sentence boundary"
        );
    }

    #[test]
    fn test_is_unordered_list_marker() {
        // Valid unordered list markers
        assert!(is_unordered_list_marker("- item"));
        assert!(is_unordered_list_marker("* item"));
        assert!(is_unordered_list_marker("+ item"));
        assert!(is_unordered_list_marker("-")); // lone marker
        assert!(is_unordered_list_marker("*"));
        assert!(is_unordered_list_marker("+"));

        // Not list markers
        assert!(!is_unordered_list_marker("---")); // horizontal rule
        assert!(!is_unordered_list_marker("***")); // horizontal rule
        assert!(!is_unordered_list_marker("- - -")); // horizontal rule
        assert!(!is_unordered_list_marker("* * *")); // horizontal rule
        assert!(!is_unordered_list_marker("*emphasis*")); // emphasis, not list
        assert!(!is_unordered_list_marker("-word")); // no space after marker
        assert!(!is_unordered_list_marker("")); // empty
        assert!(!is_unordered_list_marker("text")); // plain text
        assert!(!is_unordered_list_marker("# heading")); // heading
    }

    #[test]
    fn test_is_block_boundary() {
        // Block boundaries
        assert!(is_block_boundary("")); // empty line
        assert!(is_block_boundary("# Heading")); // ATX heading
        assert!(is_block_boundary("## Level 2")); // ATX heading
        assert!(is_block_boundary("```rust")); // code fence
        assert!(is_block_boundary("~~~")); // tilde code fence
        assert!(is_block_boundary("> quote")); // blockquote
        assert!(is_block_boundary("| cell |")); // table
        assert!(is_block_boundary("[link]: http://example.com")); // reference def
        assert!(is_block_boundary("---")); // horizontal rule
        assert!(is_block_boundary("***")); // horizontal rule
        assert!(is_block_boundary("- item")); // unordered list
        assert!(is_block_boundary("* item")); // unordered list
        assert!(is_block_boundary("+ item")); // unordered list
        assert!(is_block_boundary("1. item")); // ordered list
        assert!(is_block_boundary("10. item")); // ordered list
        assert!(is_block_boundary(": definition")); // definition list
        assert!(is_block_boundary(":::")); // div marker
        assert!(is_block_boundary("::::: {.callout-note}")); // div marker with attrs

        // NOT block boundaries (paragraph continuation)
        assert!(!is_block_boundary("regular text"));
        assert!(!is_block_boundary("*emphasis*")); // emphasis, not list
        assert!(!is_block_boundary("[link](url)")); // inline link, not reference def
        assert!(!is_block_boundary("some words here"));
    }

    #[test]
    fn test_definition_list_boundary_in_single_line_paragraph() {
        // Verifies that a definition list item after a single-line paragraph
        // is treated as a block boundary, not merged into the paragraph
        let options = ReflowOptions {
            line_length: 80,
            ..Default::default()
        };
        let input = "Term\n: Definition of the term";
        let result = reflow_markdown(input, &options);
        // The definition list marker should remain on its own line
        assert!(
            result.contains(": Definition"),
            "Definition list item should not be merged into previous line. Got: {result:?}"
        );
        let lines: Vec<&str> = result.lines().collect();
        assert_eq!(lines.len(), 2, "Should remain two separate lines. Got: {lines:?}");
        assert_eq!(lines[0], "Term");
        assert_eq!(lines[1], ": Definition of the term");
    }

    #[test]
    fn test_is_paragraph_boundary() {
        // Core block boundary checks are inherited
        assert!(is_paragraph_boundary("# Heading", "# Heading"));
        assert!(is_paragraph_boundary("- item", "- item"));
        assert!(is_paragraph_boundary(":::", ":::"));
        assert!(is_paragraph_boundary(": definition", ": definition"));

        // Indented code blocks (≥4 spaces or tab)
        assert!(is_paragraph_boundary("code", "    code"));
        assert!(is_paragraph_boundary("code", "\tcode"));

        // Table rows via is_potential_table_row
        assert!(is_paragraph_boundary("| a | b |", "| a | b |"));
        assert!(is_paragraph_boundary("a | b", "a | b")); // pipe-delimited without leading pipe

        // Not paragraph boundaries
        assert!(!is_paragraph_boundary("regular text", "regular text"));
        assert!(!is_paragraph_boundary("text", "  text")); // 2-space indent is not code
    }

    #[test]
    fn test_div_marker_boundary_in_reflow_paragraph_at_line() {
        // Verifies that div markers (:::) are treated as paragraph boundaries
        // in reflow_paragraph_at_line, preventing reflow across div boundaries
        let content = "Some paragraph text here.\n\n::: {.callout-note}\nThis is a callout.\n:::\n";
        // Line 3 is the div marker — should not be reflowed
        let result = reflow_paragraph_at_line(content, 3, 80);
        assert!(result.is_none(), "Div marker line should not be reflowed");
    }

    #[test]
    fn starts_block_construct_detects_block_openers() {
        // Bullet list markers: marker char followed by space or end
        for case in ["- item", "-", "* item", "*", "+ item", "+", "-\titem"] {
            assert!(starts_block_construct(case), "bullet: {case:?}");
        }
        // Ordered list markers: up to 9 digits, `.` or `)`, then space or end
        for case in ["1. item", "1) item", "9. x", "123456789. x", "1.", "42) x"] {
            assert!(starts_block_construct(case), "ordered: {case:?}");
        }
        // Blockquote: `>` needs no following space
        for case in ["> quote", ">quote", ">"] {
            assert!(starts_block_construct(case), "blockquote: {case:?}");
        }
        // ATX headings: 1-6 hashes then space or end
        for case in ["# heading", "###### h6", "#", "##"] {
            assert!(starts_block_construct(case), "heading: {case:?}");
        }
        // Code fences: 3+ backticks or tildes
        for case in ["```", "```rust", "````", "~~~", "~~~text"] {
            assert!(starts_block_construct(case), "fence: {case:?}");
        }
        // Setext underlines and thematic breaks
        for case in ["---", "--", "===", "=", "***", "___", "_ _ _", "- - -"] {
            assert!(starts_block_construct(case), "setext/thematic: {case:?}");
        }
        // Footnote and link-reference definitions: hoisting one to line start
        // reclassifies it and can resolve dangling references elsewhere
        for case in [
            "[^1]: text",
            "[^note]:",
            "[ref]: http://example.com",
            "[wat]: url follows",
        ] {
            assert!(starts_block_construct(case), "definition: {case:?}");
        }
        // Block-level HTML tags (rumdl parser's HTML block classification)
        for case in ["<div>content", "</div>", "<p>text", "<table>", "<pre>code", "<h1>x"] {
            assert!(starts_block_construct(case), "html block: {case:?}");
        }
    }

    #[test]
    fn starts_block_construct_allows_ordinary_prose() {
        for case in [
            "",
            "word",
            "-5 degrees",
            "--flag",
            "-item",
            "#hashtag",
            "####### seven hashes is not a heading",
            "1.5 million",
            "1234567890. ten digits is not a list marker",
            "1:30 pm",
            "*emphasis*",
            "**bold** text",
            "__bold__ text",
            "_emphasis_ text",
            "`code` span",
            "`` double backtick span ``",
            "~~strikethrough~~",
            "=x",
            "== ==",
            "(parenthetical)",
            "[link](url)",
            "[text][ref] more",
            "[bracketed] aside",
            "[a](b) [ref]: first bracket is a link, not a label",
            "[esc\\]: not a close] text",
            "<span>inline</span>",
            "<b>bold</b>",
            "<https://example.com> autolink",
            "<mailto:a@b.com>",
            "<notarealtag>",
        ] {
            assert!(!starts_block_construct(case), "prose: {case:?}");
        }
    }

    #[test]
    fn merge_block_construct_continuations_merges_marker_led_lines() {
        let lines = vec![
            "First sentence?".to_string(),
            "- looks like a list item".to_string(),
            "Second sentence.".to_string(),
        ];
        assert_eq!(
            merge_block_construct_continuations(lines),
            vec![
                "First sentence? - looks like a list item".to_string(),
                "Second sentence.".to_string(),
            ]
        );

        // The first line keeps its position: it replaces the paragraph's
        // original start, where the source already established the context.
        let lines = vec!["- real list content".to_string(), "continuation".to_string()];
        assert_eq!(
            merge_block_construct_continuations(lines.clone()),
            lines,
            "first line must never be merged"
        );
    }

    #[test]
    fn wrap_never_starts_a_line_with_a_block_marker() {
        let options = ReflowOptions {
            line_length: 25,
            ..Default::default()
        };
        // The dash lands exactly at the wrap point; the wrapper must break one
        // word earlier so the dash stays mid-line.
        let lines = reflow_line(
            "Some words here and then - a dash clause that wraps around the limit.",
            &options,
        );
        assert_eq!(
            lines,
            vec![
                "Some words here and",
                "then - a dash clause that",
                "wraps around the limit."
            ]
        );

        // Every marker category must stay mid-line in wrap mode, whatever the width.
        for input in [
            "Alpha beta gamma delta epsilon - dash clause here to wrap",
            "Alpha beta gamma delta epsilon > quote lookalike here to wrap",
            "Alpha beta gamma delta epsilon # heading lookalike here to wrap",
            "Alpha beta gamma delta epsilon 1. ordered lookalike here to wrap",
            "Alpha beta gamma delta epsilon * star clause here to wrap",
            "Alpha beta gamma delta epsilon + plus clause here to wrap",
        ] {
            for width in 10..40 {
                let options = ReflowOptions {
                    line_length: width,
                    ..Default::default()
                };
                for line in reflow_line(input, &options) {
                    assert!(
                        !starts_block_construct(&line),
                        "width {width}: wrapped line opens a block construct: {line:?} (input {input:?})"
                    );
                }
            }
        }
    }

    #[test]
    fn sentence_per_line_keeps_block_markers_mid_line() {
        let options = ReflowOptions {
            line_length: 80,
            sentence_per_line: true,
            ..Default::default()
        };
        // A sentence "starting" with a dash must stay attached to the previous
        // sentence instead of becoming a list item (issue #728).
        let lines = reflow_line(
            "Google Calendar (Can't we get rid of this dependency? - I don't really see the need)",
            &options,
        );
        assert_eq!(
            lines,
            vec!["Google Calendar (Can't we get rid of this dependency? - I don't really see the need)".to_string()]
        );

        // Same for heading, blockquote, and ordered-list lookalikes.
        let lines = reflow_line("See section 4? # is the marker we use. Fine.", &options);
        assert_eq!(lines, vec!["See section 4? # is the marker we use.", "Fine."]);

        let lines = reflow_line("Is this a problem? > I quote someone here.", &options);
        assert_eq!(lines, vec!["Is this a problem? > I quote someone here."]);

        let lines = reflow_line("Another case! 1. Not a list. More text follows here.", &options);
        for line in &lines {
            assert!(
                !starts_block_construct(line),
                "sentence-per-line output opens a block construct: {line:?}"
            );
        }
    }

    #[test]
    fn test_code_span_parsing() {
        // 1. Single backtick
        let elements = parse_markdown_elements_inner("`code`", false, false, None);
        assert_eq!(elements.len(), 1);
        assert!(matches!(&elements[0], Element::Code(s) if s == "`code`"));

        // 2. Double backtick
        let elements = parse_markdown_elements_inner("``code``", false, false, None);
        assert_eq!(elements.len(), 1);
        assert!(matches!(&elements[0], Element::Code(s) if s == "``code``"));

        // 3. Double backtick with single backtick inside
        let elements = parse_markdown_elements_inner("``code`inside``", false, false, None);
        assert_eq!(elements.len(), 1);
        assert!(matches!(&elements[0], Element::Code(s) if s == "``code`inside``"));

        // 4. Spaces inside
        let elements = parse_markdown_elements_inner("`` code ``", false, false, None);
        assert_eq!(elements.len(), 1);
        assert!(matches!(&elements[0], Element::Code(s) if s == "`` code ``"));

        // 5. Unclosed backtick (should be parsed as Text)
        let elements = parse_markdown_elements_inner("`unclosed", false, false, None);
        assert_eq!(elements.len(), 1);
        assert!(matches!(&elements[0], Element::Text(s) if s == "`unclosed"));

        // 6. Unclosed backtick followed by a link (the link should be parsed as Link, not Text)
        let elements = parse_markdown_elements_inner("`unclosed [link](url)", false, false, None);
        // We expect: Text("`unclosed "), Link("[link](url)")
        assert_eq!(elements.len(), 2);
        assert!(matches!(&elements[0], Element::Text(s) if s == "`unclosed "));
        assert!(matches!(&elements[1], Element::Link(s) if s == "[link](url)"));
    }

    #[test]
    fn test_reflow_performance_long_input() {
        // Generate a string with many distinct unclosed backtick runs to test worst-case performance.
        // E.g., "` `` ` `` ` ...`"
        let mut text = String::new();
        for i in 1..400 {
            let backticks = "`".repeat(i);
            text.push_str(&backticks);
            text.push(' ');
        }

        let start = std::time::Instant::now();
        let elements = parse_markdown_elements_inner(&text, false, false, None);
        let duration = start.elapsed();

        // Ensure it completes in under 100ms.
        assert!(duration.as_millis() < 100, "Parsing took too long: {duration:?}");
        assert!(!elements.is_empty());
    }
}
