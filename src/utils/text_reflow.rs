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
pub(crate) fn display_len(s: &str, mode: ReflowLengthMode) -> usize {
    match mode {
        ReflowLengthMode::Chars => s.chars().count(),
        ReflowLengthMode::Visual => s.width(),
        ReflowLengthMode::Bytes => s.len(),
    }
}

/// Whitespace characters whose whole purpose is to forbid a line break:
/// no-break space (U+00A0), narrow no-break space (U+202F), and figure
/// space (U+2007).
fn is_non_breaking_space(c: char) -> bool {
    matches!(c, '\u{00A0}' | '\u{202F}' | '\u{2007}')
}

/// Whitespace on which reflow may break and rejoin lines. Non-breaking
/// spaces are excluded: they stay inside the surrounding token so they
/// survive reflow byte-for-byte and never become a wrap point (e.g. the
/// French `mot\u{00A0}:` pair or a `10\u{00A0}000` thousands separator).
fn is_breakable_whitespace(c: char) -> bool {
    c.is_whitespace() && !is_non_breaking_space(c)
}

/// Split text into wrappable tokens on breakable whitespace only.
fn split_breakable_words(text: &str) -> impl Iterator<Item = &str> {
    text.split(is_breakable_whitespace).filter(|word| !word.is_empty())
}

/// Whether an inline code span's content can be word-wrapped without altering it.
///
/// Interior whitespace in code spans is literal. Word-splitting collapses a run
/// of whitespace to a single space and cannot represent tabs, so wrapping would
/// corrupt content like `a    b` (four spaces) into `a b`. Only wrap when every
/// breakable-whitespace separator is already a single plain space; a lone
/// leading/trailing space still round-trips (CommonMark normalizes it and the
/// marker-padding path re-adds it for backtick-adjacent content).
fn code_span_wraps_losslessly(content: &str) -> bool {
    let mut prev_ws = false;
    for c in content.chars() {
        let ws = is_breakable_whitespace(c);
        if ws && (prev_ws || c != ' ') {
            return false;
        }
        prev_ws = ws;
    }
    true
}

/// How the inline structure nested inside a span's content constrains where a
/// line break may land.
struct NestedStructure {
    /// Ranges a break may never land inside, merged into outermost,
    /// non-overlapping ranges. The whitespace in a code span is literal, and
    /// the whitespace in a link destination or an HTML tag is structural, so
    /// replacing it with a newline rewrites the document. A link, image or
    /// attr list is held whole beyond that too, matching how the top level
    /// treats one.
    atomic: Vec<(usize, usize)>,
    /// The delimiter runs of nested emphasis, strong and strikethrough spans.
    /// These marker characters belong to a well-formed span, so they do not
    /// force the whole span to be kept whole, but they are not break points
    /// either: the prose between them breaks at whitespace as usual.
    markers: Vec<(usize, usize)>,
    /// Every link, image, wikilink and footnote reference the parse recognised,
    /// nested ones included, sorted by start. Where `atomic` folds a construct
    /// into the one enclosing it, this keeps each one's own start, so a sentence
    /// opener can be walked into a link whose text begins with an image.
    links: Vec<(usize, usize)>,
    /// Every code span the parse recognised, sorted by start. A backtick that
    /// opens none is ordinary text, so this is what tells the two apart.
    code_spans: Vec<(usize, usize)>,
}

/// An emphasis, strong or strikethrough span whose end has not been seen yet.
struct OpenSpan {
    /// The span's full range, delimiters included.
    span: (usize, usize),
    /// Bounds of the content found inside it so far. What falls outside these
    /// but inside `span` is the delimiter run.
    content: Option<(usize, usize)>,
}

/// Record an event as content of every enclosing span still open, widening the
/// bounds that separate a span's delimiters from what sits between them.
fn note_span_content(open: &mut [OpenSpan], start: usize, end: usize) {
    for open_span in open.iter_mut() {
        if start >= open_span.span.0 && end <= open_span.span.1 {
            open_span.content = Some(match open_span.content {
                Some((known_start, known_end)) => (known_start.min(start), known_end.max(end)),
                None => (start, end),
            });
        }
    }
}

fn merge_ranges(mut ranges: Vec<(usize, usize)>) -> Vec<(usize, usize)> {
    // A nested construct is reported alongside its parent, so any range
    // starting at or before the current end is already covered by it.
    ranges.sort_unstable();
    let mut merged: Vec<(usize, usize)> = Vec::with_capacity(ranges.len());
    for (start, end) in ranges {
        match merged.last_mut() {
            Some(last) if start <= last.1 => last.1 = last.1.max(end),
            _ => merged.push((start, end)),
        }
    }
    merged
}

/// Classify the inline constructs nested inside a span's content.
fn nested_structure(content: &str, defined_references: Option<&HashSet<String>>, attr_lists: bool) -> NestedStructure {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_STRIKETHROUGH);

    let mut atomic: Vec<(usize, usize)> = Vec::new();
    let mut markers: Vec<(usize, usize)> = Vec::new();
    let mut links: Vec<(usize, usize)> = Vec::new();
    let mut code_spans: Vec<(usize, usize)> = Vec::new();
    // Emphasis-like spans whose end has not been seen yet, each with the bounds
    // of the content found inside it so far.
    let mut open: Vec<OpenSpan> = Vec::new();

    for (event, range) in Parser::new_ext(content, options).into_offset_iter() {
        let (start, end) = (range.start, range.end);
        // An `End` repeats the range its `Start` already contributed, and the
        // one closing a span covers that span whole, which would swallow its
        // own delimiters.
        if !matches!(event, Event::End(_)) {
            note_span_content(&mut open, start, end);
        }
        match event {
            Event::Start(Tag::Link { .. } | Tag::Image { .. }) => {
                atomic.push((start, end));
                links.push((start, end));
            }
            Event::Code(_) => {
                atomic.push((start, end));
                code_spans.push((start, end));
            }
            Event::InlineHtml(_) => {
                atomic.push((start, end));
            }
            Event::Start(Tag::Emphasis | Tag::Strong | Tag::Strikethrough) => {
                open.push(OpenSpan {
                    span: (start, end),
                    content: None,
                });
            }
            Event::End(TagEnd::Emphasis | TagEnd::Strong | TagEnd::Strikethrough) => {
                if let Some(OpenSpan {
                    span: (span_start, span_end),
                    content,
                }) = open.pop()
                {
                    match content {
                        // Whatever sits outside the content is the delimiter run.
                        // Its length is read off the parse rather than assumed,
                        // since `~x~` and `~~x~~` are both strikethrough.
                        Some((content_start, content_end)) => {
                            markers.push((span_start, content_start));
                            markers.push((content_end, span_end));
                        }
                        // Nothing inside to anchor the delimiters against, so
                        // keep the span whole rather than guess where they end.
                        None => atomic.push((span_start, span_end)),
                    }
                }
            }
            _ => {}
        }
    }

    // Reference-style links and images (`[text][ref]`, `[text][]`, `[text]`) and
    // footnote references need the document's definitions to be recognised, which
    // the parse above has no access to. Reusing the walk the top level runs
    // keeps a link atomic under exactly the same conditions wherever it appears.
    // Nested constructs come along too: the reference image inside
    // `[![alt][img]](url)` is what that link's sentence opens with.
    for span in all_link_spans(content, defined_references) {
        atomic.push((span.start, span.end));
        links.push((span.start, span.end));
    }

    // Constructs pulldown does not model, but that `parse_elements` holds
    // atomic at the top level. Only those that can contain whitespace matter
    // here: an emoji shortcode or HTML entity has no break point inside it.
    for found in WIKI_LINK_REGEX.find_iter(content) {
        atomic.push((found.start(), found.end()));
        links.push((found.start(), found.end()));
    }
    for found in HUGO_SHORTCODE_REGEX
        .find_iter(content)
        .chain(DISPLAY_MATH_REGEX.find_iter(content))
    {
        atomic.push((found.start(), found.end()));
    }
    let mut from = 0;
    while let Ok(Some(found)) = INLINE_MATH_REGEX.find_from_pos(content, from) {
        atomic.push((found.start(), found.end()));
        from = found.end();
    }

    // A MkDocs/kramdown attr list (`{.class key="value"}`) holds interior
    // whitespace that is structural, so breaking inside one rewrites it. The
    // pattern anchors on `{`, so a non-overlapping sweep finds the same units
    // the top level does. Only when the flavor is enabled: otherwise `{a b}` is
    // literal prose and breaks like any other words, matching the top level.
    if attr_lists {
        for found in ATTR_LIST_PATTERN.find_iter(content) {
            atomic.push((found.start(), found.end()));
        }
    }

    // Both parses report an outermost inline link, so the same range can arrive
    // twice; a nested one arrives once, from the parse without definitions.
    links.sort_unstable();
    links.dedup();

    NestedStructure {
        atomic: merge_ranges(atomic),
        markers: merge_ranges(markers),
        links,
        code_spans,
    }
}

/// Split an emphasis, strong or strikethrough span's content into the units
/// that may be placed on separate lines, or `None` when the span has to stay
/// atomic.
///
/// Breaking such a span at whitespace is safe: emphasis carries across a soft
/// line break, and a newline is whitespace just like the space it replaces, so
/// every delimiter keeps its flanking classification. Two things are not safe,
/// and this rules them out:
///
/// - A break inside a code span, link, image or HTML tag. Each is one
///   unbreakable unit, matching how they are already held atomic at the top
///   level, because the whitespace in one is literal or structural.
/// - A marker character that belongs to no well-formed nested span: a stray or
///   backslash-escaped `` ` ``, `*`, `_` or `~`. The content's structure is then
///   not fully modelled, so the span is kept whole rather than broken on a guess.
///   This matters: breaking `**a * b**` at its spaces would put a literal `*` at
///   the start of a line, turning it into a list item.
///
/// A nested emphasis, strong or strikethrough span is not one of those. The
/// whitespace inside it is ordinary prose whitespace, so it breaks like any
/// other, and only its delimiter runs are held together with the words they
/// flank.
fn breakable_units<'a>(
    content: &'a str,
    defined_references: Option<&HashSet<String>>,
    attr_lists: bool,
) -> Option<Vec<&'a str>> {
    // Plain prose cannot hold a nested construct, so every whitespace run is a
    // break point and the parse below can be skipped.
    if !content.contains(['`', '*', '_', '~', '[', '<', '$', '{']) {
        return Some(split_breakable_words(content).collect());
    }

    let NestedStructure { atomic, markers, .. } = nested_structure(content, defined_references, attr_lists);

    let mut units = Vec::new();
    let mut unit_start = None;
    let mut next_atomic = 0;
    let mut next_marker = 0;
    for (offset, ch) in content.char_indices() {
        while atomic.get(next_atomic).is_some_and(|&(_, end)| end <= offset) {
            next_atomic += 1;
        }
        if atomic.get(next_atomic).is_some_and(|&(start, _)| offset >= start) {
            // Inside an atomic construct: never a break point, and its markers
            // are accounted for.
            if unit_start.is_none() {
                unit_start = Some(offset);
            }
            continue;
        }
        while markers.get(next_marker).is_some_and(|&(_, end)| end <= offset) {
            next_marker += 1;
        }
        if matches!(ch, '`' | '*' | '_' | '~') && markers.get(next_marker).is_none_or(|&(start, _)| offset < start) {
            return None;
        }
        if is_breakable_whitespace(ch) {
            if let Some(start) = unit_start.take() {
                units.push(&content[start..offset]);
            }
        } else if unit_start.is_none() {
            unit_start = Some(offset);
        }
    }
    if let Some(start) = unit_start {
        units.push(&content[start..]);
    }
    Some(units)
}

/// Split a link or image's text into wrappable units, or `None` when the
/// construct has to stay whole. Only consulted when
/// [`ReflowOptions::break_link_text`] is enabled.
///
/// `inner` is the bracketed text and `suffix` everything from the closing `]`
/// on (`](url)`, `][ref]`, `][]`, or a bare `]`). The suffix is never split:
/// its whitespace is structural (a destination or title), and the checker's
/// inline-URL exemption only ever applies to an intact link. That bounds what
/// breaking can achieve, and two checks keep reflow from splitting a link
/// into lines the checker would then report but reflow could never fix:
///
/// - When the suffix alone exceeds the budget but the bracketed text fits,
///   every split still leaves a line at least as wide as the suffix. Breaking
///   would trade one forgiven line (a standalone link is exempt, and an intact
///   inline link earns the URL exemption) for fragments that earn nothing, so
///   the link stays whole.
/// - The last line of a split link is the final unit plus the suffix. That
///   closing line must either fit or overflow only within its final
///   whitespace-delimited token, the one overflow the checker forgives.
pub(crate) fn link_text_break_units<'a>(
    inner: &'a str,
    suffix: &str,
    budget: usize,
    mode: ReflowLengthMode,
    defined_references: Option<&HashSet<String>>,
    attr_lists: bool,
) -> Option<Vec<&'a str>> {
    if display_len(suffix, mode) > budget && display_len(inner, mode) + 2 <= budget {
        return None;
    }
    let units = breakable_units(inner, defined_references, attr_lists)?;
    if units.len() < 2 {
        return None;
    }
    let tail = format!("{}{suffix}", units[units.len() - 1]);
    if display_len(&tail, mode) > budget && !last_token_overflow_only(&tail, budget, mode) {
        return None;
    }
    Some(units)
}

/// Whether `line` overflows `budget` only within its final
/// whitespace-delimited token. Mirrors the checker's trailing-token
/// forgiveness (markdownlint's `line.replace(/\S*$/u, "#")`): the width up to
/// and including the last whitespace, plus one for the replaced token, is what
/// the checker measures.
fn last_token_overflow_only(line: &str, budget: usize, mode: ReflowLengthMode) -> bool {
    match line.rfind(char::is_whitespace) {
        None => true,
        Some(pos) => {
            let ws_len = line[pos..].chars().next().map_or(1, char::len_utf8);
            display_len(&line[..pos + ws_len], mode) < budget
        }
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
    /// Whether to hold emphasis/strong/strikethrough and code spans atomic during reflow.
    /// When true (default), these spans are treated as atomic units.
    /// When false, they can be wrapped word-by-word like normal text.
    pub atomic_spans: bool,
    /// Whether the text of a link or image may wrap at its whitespace.
    /// When false (default), every link and image is one atomic token. When
    /// true, `[text](url)` and its reference, shortcut and image forms follow
    /// the same rules `atomic_spans` applies to emphasis spans: the text wraps
    /// when the construct alone can never fit a line (or always, when
    /// `atomic_spans` is off). The `](...)` tail is never split, and a link
    /// whose tail rules out a useful break stays whole; see
    /// [`link_text_break_units`].
    pub break_link_text: bool,
    /// Which of the checker's line-length exemptions reflow mirrors when it
    /// measures a line. Empty measures the markdown as written.
    pub length_exemptions: LengthExemptions,
}

/// The line-length exemptions MD013's check applies, as far as reflow can mirror
/// them.
///
/// The checker tests each one against the budget separately, so a line is
/// forgiven when either reduced length fits, never when the two savings together
/// would fit. Reflow therefore has to keep them apart too: see
/// [`LineWidth`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LengthExemptions {
    /// An inline `[text](url)` costs `[text]` and an inline `![alt](url)` costs
    /// `![alt]`. Reference, collapsed and shortcut forms are never exempt.
    pub link_urls: bool,
    /// An inline code span costs nothing, because it cannot be wrapped.
    pub code_spans: bool,
}

impl LengthExemptions {
    /// Whether any exemption is active. When none is, every width below reduces
    /// to the plain source width and the whole mechanism is inert.
    fn any(&self) -> bool {
        self.link_urls || self.code_spans
    }
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
            atomic_spans: true,
            break_link_text: false,
            length_exemptions: LengthExemptions::default(),
        }
    }
}

/// A line's width under each exemption the checker applies independently.
///
/// The checker forgives a line when the link-exempt length fits *or* the
/// code-exempt length fits, so the width that decides whether reflow may stop is
/// the smaller of the two, never one total with both savings taken out. Adding
/// widths is component-wise, which is what makes it usable as a running total.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct LineWidth {
    /// Width with inline link and image destinations discounted.
    link_exempt: usize,
    /// Width with inline code spans discounted.
    code_exempt: usize,
}

impl LineWidth {
    /// A span of text that no exemption touches, so both components are its
    /// full width.
    fn plain(width: usize) -> Self {
        Self {
            link_exempt: width,
            code_exempt: width,
        }
    }

    /// The width the checker measures this line at: whichever exemption helps
    /// more.
    fn effective(self) -> usize {
        self.link_exempt.min(self.code_exempt)
    }

    fn fits(self, line_length: usize) -> bool {
        self.effective() <= line_length
    }

    /// Whether nothing has been accumulated. Only an empty string measures zero
    /// under both exemptions: the cheapest an exempt link can be is `[]`, and a
    /// code span still costs its full width against the link exemption.
    fn is_empty(self) -> bool {
        self.link_exempt == 0 && self.code_exempt == 0
    }
}

impl std::ops::Add for LineWidth {
    type Output = Self;

    fn add(self, other: Self) -> Self {
        Self {
            link_exempt: self.link_exempt + other.link_exempt,
            code_exempt: self.code_exempt + other.code_exempt,
        }
    }
}

impl std::ops::AddAssign for LineWidth {
    fn add_assign(&mut self, other: Self) {
        *self = *self + other;
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

/// Byte offset of each char in the text `chars` was collected from, with the
/// text's byte length as a final entry.
fn char_byte_offsets(chars: &[char]) -> Vec<usize> {
    let mut offsets = Vec::with_capacity(chars.len() + 1);
    let mut offset = 0;
    for c in chars {
        offsets.push(offset);
        offset += c.len_utf8();
    }
    offsets.push(offset);
    offsets
}

/// A text being split into sentences, with the views the boundary check reads
/// alongside it: its chars, each char's byte offset (plus the text's length as
/// a final entry), and the `links` list of its [`NestedStructure`], the
/// link-like constructs a sentence may open with, nested ones included.
struct SentenceText<'a> {
    text: &'a str,
    chars: &'a [char],
    char_offsets: &'a [usize],
    links: &'a [(usize, usize)],
    code_spans: &'a [(usize, usize)],
}

impl SentenceText<'_> {
    /// Whether a code span the parse recognised opens at `chars[pos]`.
    ///
    /// An unmatched backtick opens nothing, and the sentence it sits in carries
    /// on through it, so the character alone cannot stand for a code span.
    fn opens_code_span(&self, pos: usize) -> bool {
        self.char_offsets
            .get(pos)
            .is_some_and(|&start| self.code_spans.binary_search_by_key(&start, |&(s, _)| s).is_ok())
    }

    /// Char index just past the link, image, wikilink or footnote reference
    /// that starts at `chars[pos]`, or `None` when no such construct starts
    /// there. The construct is one the parse behind `links` recognised, so a
    /// bracket the parse reads as text (`[Smith 2020]` with no such reference
    /// defined, `[text](unterminated`) opens nothing here either, and one
    /// nested in another (`[![alt](img)](url)`) is found at its own start. An
    /// Obsidian embed `![[note]]` is known to the parse from its `[[`, so the
    /// range starts one char after its `!`.
    fn link_end_at(&self, pos: usize) -> Option<usize> {
        let range_start = match self.chars.get(pos) {
            Some('[') => pos,
            Some('!') if self.chars.get(pos + 1) == Some(&'[') => match self.link_range_end_at(pos) {
                Some(end) => return Some(end),
                None => pos + 1,
            },
            _ => return None,
        };
        self.link_range_end_at(range_start)
    }

    /// Char index just past the link-like construct that starts at `chars[pos]`.
    fn link_range_end_at(&self, pos: usize) -> Option<usize> {
        let start = self.char_offsets[pos];
        let idx = self.links.binary_search_by_key(&start, |&(s, _)| s).ok()?;
        let end = self.links[idx].1;
        Some(self.char_offsets.binary_search(&end).unwrap_or_else(|i| i))
    }
}

/// Detect if a character position is a sentence boundary
/// Based on the approach from github.com/JoshuaKGoldberg/sentences-per-line
/// Supports both ASCII punctuation (. ! ?) and CJK punctuation (。 ！ ？)
fn is_sentence_boundary(
    st: &SentenceText<'_>,
    pos: usize,
    abbreviations: &HashSet<String>,
    require_sentence_capital: bool,
) -> bool {
    let SentenceText { text, chars, .. } = *st;
    if pos + 1 >= chars.len() {
        return false;
    }
    let byte_offset_after_punct = st.char_offsets[pos + 1];

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

        // Same rule as after ASCII punctuation below: no sentence opens with
        // an ordered-list marker.
        if opens_ordered_list_marker(&chars[after_punct_pos..]) {
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

    // A terminator immediately followed by a closing quote sits inside the
    // quotation, not after it.
    let inside_quotation = is_closing_quote(next_char);

    // Must be followed by space, closing quote, or emphasis/strikethrough marker followed by space
    let (space_pos, after_space_pos) = if next_char == ' ' {
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

    // A sentence is not allowed to open with an ordered-list marker. Every
    // line this splitter produces ends a sentence, and text shaped `2. Do that`
    // right after such a line is a list item: to CommonMark when the number is
    // 1, and to MD032 (which reports a list item missing its blank line, in
    // any document that has a list) for any number. So `Do this. 2. Do that.`
    // keeps its enumerator on the line of the sentence before it, and the
    // enumerated text opens the next line. The CJK path above applies the
    // same rule.
    if opens_ordered_list_marker(&chars[next_char_pos..]) {
        return false;
    }

    // Skip leading emphasis/strikethrough markers, opening quotes and the
    // opener of a link, image or wikilink to find the actual first letter: a
    // sentence that starts with `[Link text](url)` starts with its text. A
    // bracket the parse reads as text is not skipped, so a citation like
    // `[Smith 2020]` or a footnote label opens no sentence.
    let mut first_letter_pos = next_char_pos;
    while first_letter_pos < chars.len() {
        let ch = chars[first_letter_pos];
        if let Some(end) = st.link_end_at(first_letter_pos) {
            first_letter_pos += link_opener_len(chars, first_letter_pos, end);
        } else if matches!(ch, '*' | '_' | '~') || is_opening_quote(ch) {
            first_letter_pos += 1;
        } else {
            break;
        }
    }

    // Check if we reached the end after skipping emphasis
    if first_letter_pos >= chars.len() {
        return false;
    }

    let first_char = chars[first_letter_pos];

    // A bare ! or ? ends a sentence unambiguously, unlike a period, which also
    // ends abbreviations and initials. Inside a quotation it is ambiguous
    // again: the question can belong to the quoted phrase rather than to the
    // sentence carrying it, as in `A "Is this a test?" guide`. A lowercase
    // word after the closing quote means that sentence continues.
    if c == '!' || c == '?' {
        return !inside_quotation || !require_sentence_capital || opens_sentence_in_strict_mode(first_char);
    }

    // Period-specific checks: periods are ambiguous (abbreviations, initials)
    // so we apply additional guards before accepting a sentence boundary. A
    // decimal such as `3.14` never reaches this point: the period must be
    // followed by a space to get here.

    if pos > 0 {
        // Check for common abbreviations
        if text_ends_with_abbreviation(&text[..byte_offset_after_punct], abbreviations) {
            return false;
        }

        // Check for single-letter initials (e.g., "J. K. Rowling")
        // A single uppercase letter before the period preceded by whitespace or start
        // is likely an initial, not a sentence ending.
        if chars[pos - 1].is_ascii_uppercase() && (pos == 1 || (pos >= 2 && chars[pos - 2].is_whitespace())) {
            return false;
        }
    }

    // Both relaxations below end a sentence where the word after it does not
    // vouch for one, so both read the form of the period instead. Every other
    // discrimination `require_sentence_capital` was making here is already caught
    // by a guard above — abbreviations, initials, and a decimal, which needs a
    // digit after the period as well as in front of it.
    //
    // One mark of an elision is never a terminator. A period closing a digit run
    // is an enumerator or a version where it stands bare, as in `Steps: 1. ` and
    // `0.2.43. `; between a span's closing markers and the space it belongs to a
    // label instead, as in `**A2.**`, and a label ends what it labels.
    let elision = pos > 0 && chars[pos - 1] == '.';
    let digit_run = pos > 0 && chars[pos - 1].is_numeric();
    let bare = space_pos == pos + 1;

    // A code span opens a sentence on its own terms. It starts on a backtick
    // rather than on a letter, and the case of what it holds belongs to the code,
    // so `require_sentence_capital` has nothing to read there. `!` and `?` already
    // accept any following character above; a period was the outlier. Vouching for
    // itself is also what lets it act on a label's period.
    if st.opens_code_span(first_letter_pos) && !elision && !(digit_run && bare) {
        return true;
    }

    // In strict mode the next sentence must open with something a lowercase
    // continuation cannot. In relaxed mode, accept any character.
    if require_sentence_capital && !opens_sentence_in_strict_mode(first_char) {
        return false;
    }

    true
}

/// Whether `first_char` can open a sentence under `require-sentence-capital`.
///
/// The option exists to keep `word. lowercase` continuations such as
/// `etc. and` or `approx. ten` from being read as two sentences, so what it
/// requires is a first character that is not a lowercase letter: an uppercase
/// letter, a digit (`2nd try.`, `1976 was hot.`, `6:00 is early.`), or a CJK
/// character, none of which has a lowercase form.
fn opens_sentence_in_strict_mode(first_char: char) -> bool {
    first_char.is_uppercase() || first_char.is_numeric() || is_cjk_char(first_char)
}

/// Whether `chars` opens with an ordered-list marker: digits, `.` or `)`,
/// then a space or tab. Any number qualifies, and any number of digits:
/// CommonMark stops reading a marker at nine digits and only lets `1` open a
/// list mid-paragraph, but a line shaped this way reads as a list item to a
/// person and to MD032 alike, whatever the number.
fn opens_ordered_list_marker(chars: &[char]) -> bool {
    let digits = chars.iter().take_while(|c| c.is_ascii_digit()).count();
    digits > 0 && matches!(chars.get(digits), Some('.' | ')')) && matches!(chars.get(digits + 1), Some(' ' | '\t'))
}

/// Length in chars of the opener of the link, image, wikilink or footnote
/// reference occupying `chars[pos..end]`: the part before the text a reader
/// sees. `[` opens a link and `![` an image. A wikilink's `[[` opener runs
/// past its alias pipe, since `[[target|display]]` shows `display`; the pipe
/// is looked for inside the construct only, before its closing `]]`.
fn link_opener_len(chars: &[char], pos: usize, end: usize) -> usize {
    let open = if chars[pos] == '!' { pos + 1 } else { pos };
    let body = open + 1;
    if chars.get(body) != Some(&'[') {
        return body - pos;
    }
    let body = body + 1;
    let alias = chars[body..end.saturating_sub(2).max(body)]
        .iter()
        .position(|&c| c == '|')
        .map_or(body, |p| body + p + 1);
    alias - pos
}

/// Split text into sentences.
///
/// `defined_references` is the document's set of normalized reference labels,
/// which decides whether a bare `[text]` is a link (its label is defined) or
/// prose; `None` means the definitions are unknown and every shortcut is held
/// to be a link, so no real one is ever split. See
/// [`ReflowOptions::defined_references`].
pub fn split_into_sentences(text: &str, defined_references: Option<&HashSet<String>>) -> Vec<String> {
    let abbreviations = get_abbreviations(&None);
    split_into_sentences_with_set(text, &abbreviations, true, None, defined_references)
}

/// Internal function to split text into sentences with a pre-computed abbreviations set
/// Use this when calling multiple times in a loop to avoid repeatedly computing the set
///
/// `appended_span_start` is the byte offset at which an inline span the caller has
/// just appended begins, for callers that assemble a line one element at a time. A
/// sentence ending inside a span takes the closing marker with it, so a delimiter
/// run right after the punctuation joins the sentence that ends there. At that one
/// offset the run is the appended span's opening marker instead, and carrying it
/// back would leave the span with nothing to open it.
fn split_into_sentences_with_set(
    text: &str,
    abbreviations: &HashSet<String>,
    require_sentence_capital: bool,
    appended_span_start: Option<usize>,
    defined_references: Option<&HashSet<String>>,
) -> Vec<String> {
    let char_vec: Vec<char> = text.chars().collect();
    let char_offsets = char_byte_offsets(&char_vec);

    // The constructs a boundary must not fall inside, sorted and non-overlapping,
    // and the link-like ones a sentence may open with.
    let NestedStructure {
        atomic,
        links,
        code_spans,
        ..
    } = sentence_structure(text, defined_references);
    let mut atomic_it = atomic.iter().peekable();
    let st = SentenceText {
        text,
        chars: &char_vec,
        char_offsets: &char_offsets,
        links: &links,
        code_spans: &code_spans,
    };

    let mut sentences = Vec::new();
    let mut current_sentence = String::new();
    let mut pos = 0;

    while pos < char_vec.len() {
        let c = char_vec[pos];
        current_sentence.push(c);

        let byte_idx = char_offsets[pos];

        // Advance past every atomic range the current char start has left behind.
        while let Some(&&(_, end)) = atomic_it.peek() {
            if end <= byte_idx {
                atomic_it.next();
            } else {
                break;
            }
        }

        // True if the current character position falls inside an atomic construct.
        let in_atomic = atomic_it
            .peek()
            .is_some_and(|&&(start, end)| byte_idx >= start && byte_idx < end);

        if !in_atomic && is_sentence_boundary(&st, pos, abbreviations, require_sentence_capital) {
            // Consume any trailing footnote references glued to the punctuation
            if let Some(end_pos) = footnote_refs_end(&char_vec, pos + 1) {
                while pos + 1 < end_pos {
                    pos += 1;
                    current_sentence.push(char_vec[pos]);
                }
            }

            // Consume any trailing emphasis/strikethrough markers and quotes
            while pos + 1 < char_vec.len() {
                let next = char_vec[pos + 1];
                if matches!(next, '*' | '_' | '~') && Some(char_offsets[pos + 1]) == appended_span_start {
                    break;
                }
                if next == '*' || next == '_' || next == '~' || is_closing_quote(next) {
                    pos += 1;
                    current_sentence.push(char_vec[pos]);
                } else {
                    break;
                }
            }

            // Consume the space after the sentence
            if pos + 1 < char_vec.len() && char_vec[pos + 1] == ' ' {
                pos += 1; // skip space (not pushed to current_sentence)
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

/// The inline structure of a text being split into sentences: the byte ranges
/// a boundary must not fall inside, and the link-like constructs a sentence
/// may open with.
///
/// A link's text, destination and title, an image's alt text, a wikilink's
/// target, a math span, an HTML tag's attributes and a code span each hold text
/// that reads like prose to the boundary check (`[First. Second](url)`) but is
/// one construct to the renderer, so a line break inside it rewrites the
/// document rather than its layout. These are the ranges `parse_elements`
/// holds atomic, computed here from the raw text so that the check counting a
/// line's sentences and the reflow splitting them agree on where a sentence
/// can end. A bare `[text]` is a link only when `defined_references` holds
/// its label, prose otherwise; without the definitions (`None`) it is held
/// atomic wherever it appears, which keeps a real shortcut link whole at the
/// price of leaving a bracketed prose aside on one line.
fn sentence_structure(text: &str, defined_references: Option<&HashSet<String>>) -> NestedStructure {
    // Every construct that can hold whitespace, and every link-like construct,
    // opens with one of these; plain prose skips the parse entirely.
    if !text.contains(['`', '[', '<', '$']) {
        return NestedStructure {
            atomic: Vec::new(),
            markers: Vec::new(),
            links: Vec::new(),
            code_spans: Vec::new(),
        };
    }
    nested_structure(text, defined_references, false)
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

/// Reflow a line, falling back to the input when the result would not preserve it.
///
/// Reflow redistributes whitespace: it decides where lines break, never which
/// characters a paragraph contains. So the sequence of non-whitespace characters
/// is invariant across a correct reflow, and any difference means the reflow
/// dropped, duplicated, reordered, or invented content. Returning the input
/// unchanged in that case costs a paragraph that stays unwrapped; the alternative
/// is writing corrupted prose into the user's file. A caller comparing its
/// replacement against the original then sees no change and reports nothing.
pub fn reflow_line(line: &str, options: &ReflowOptions) -> Vec<String> {
    let reflowed = reflow_line_unchecked(line, options);
    if preserves_content(line, &reflowed) {
        reflowed
    } else {
        vec![line.to_string()]
    }
}

/// Whether `reflowed` still holds `original`'s text: the same non-whitespace
/// characters in the same order, with every word boundary intact.
///
/// Reflow may add a boundary the input did not have, since wrapping a script
/// that writes without spaces has to break somewhere. Removing one is different:
/// it glues two words into a word the author never wrote.
fn preserves_content(original: &str, reflowed: &[String]) -> bool {
    let (original_text, original_breaks) = visible_text_and_breaks(original.chars());
    let (reflowed_text, reflowed_breaks) =
        visible_text_and_breaks(reflowed.iter().flat_map(|line| line.chars().chain(['\n'])));

    original_text == reflowed_text && contains_all(&reflowed_breaks, &original_breaks)
}

/// The non-whitespace characters of `text`, and for each interior run of
/// whitespace, how many characters precede it.
fn visible_text_and_breaks(text: impl Iterator<Item = char>) -> (String, Vec<usize>) {
    let mut visible = String::new();
    let mut breaks = Vec::new();
    let mut count = 0usize;
    let mut pending_break = false;

    for c in text {
        if c.is_whitespace() {
            pending_break = count > 0;
        } else {
            if pending_break {
                breaks.push(count);
                pending_break = false;
            }
            visible.push(c);
            count += 1;
        }
    }

    (visible, breaks)
}

/// Whether every value in `subset` appears in `superset`. Both are ascending.
fn contains_all(superset: &[usize], subset: &[usize]) -> bool {
    let mut candidates = superset.iter();
    subset
        .iter()
        .all(|wanted| candidates.by_ref().any(|found| found == wanted))
}

fn reflow_line_unchecked(line: &str, options: &ReflowOptions) -> Vec<String> {
    // For sentence-per-line mode, always process regardless of length
    if options.sentence_per_line {
        let elements = parse_elements(line, options);
        return merge_block_construct_continuations(reflow_elements_sentence_per_line(&elements, options));
    }

    // For semantic line breaks mode, use cascading split strategy
    if options.semantic_line_breaks {
        let elements = parse_elements(line, options);
        return merge_block_construct_continuations(reflow_elements_semantic(&elements, options));
    }

    // Quick check: if line is already short enough or no wrapping requested, return as-is
    // line_length = 0 means no wrapping (unlimited line length)
    if options.line_length == 0 || line_fits(line, options) {
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
    Code { content: String, marker: String },
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

impl Element {
    /// Whether the element's source form opens with `[` or `![`: a link,
    /// image, wikilink, footnote or shortcut reference. What the sentence
    /// splitter makes of that bracket decides whether the element can start a
    /// sentence, so the reflow defers to the splitter for these.
    fn opens_with_bracket(&self) -> bool {
        matches!(
            self,
            Element::Link(_)
                | Element::ReferenceLink(_)
                | Element::EmptyReferenceLink(_)
                | Element::ShortcutReference(_)
                | Element::FootnoteReference(_)
                | Element::InlineImage(_)
                | Element::ReferenceImage(_)
                | Element::EmptyReferenceImage(_)
                | Element::LinkedImage(_)
                | Element::WikiLink(_)
        )
    }
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
            Element::Code { content, marker } => write!(f, "{marker}{content}{marker}"),
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
    fn display_len(&self, mode: ReflowLengthMode) -> usize {
        match self {
            Element::Text(s)
            | Element::Link(s)
            | Element::ReferenceLink(s)
            | Element::EmptyReferenceLink(s)
            | Element::ShortcutReference(s)
            | Element::InlineImage(s)
            | Element::ReferenceImage(s)
            | Element::EmptyReferenceImage(s)
            | Element::LinkedImage(s)
            | Element::FootnoteReference(s)
            | Element::Autolink(s)
            | Element::HtmlTag(s)
            | Element::HtmlEntity(s)
            | Element::HugoShortcode(s)
            | Element::AttrList(s)
            | Element::MystRole(s) => display_len(s, mode),
            Element::WikiLink(s) => display_len(s, mode) + 4,
            Element::InlineMath(s) => display_len(s, mode) + 2,
            Element::DisplayMath(s) => display_len(s, mode) + 4,
            Element::EmojiShortcode(s) => display_len(s, mode) + 2,
            Element::Strikethrough { content, double } => display_len(content, mode) + if *double { 4 } else { 2 },
            Element::Code { content, marker } => display_len(content, mode) + display_len(marker, mode) * 2,
            Element::Bold { content, .. } => display_len(content, mode) + 4,
            Element::Italic { content, .. } => display_len(content, mode) + 2,
        }
    }

    /// The width the checker measures this element at, under each exemption.
    ///
    /// An inline link costs `[text]` and an inline image `![alt]`, exactly as
    /// `MD013`'s check computes them; a code span costs nothing. Every other
    /// element, reference and shortcut link forms included, costs its full
    /// width, because the check does not exempt those either.
    ///
    /// An element whose text cannot be delimited is charged in full. Charging
    /// too much only makes reflow wrap a line the check would have forgiven;
    /// charging too little would leave a line the check reports.
    fn exempt_width(&self, mode: ReflowLengthMode, exemptions: LengthExemptions) -> LineWidth {
        let full = self.display_len(mode);
        let mut width = LineWidth::plain(full);
        match self {
            Element::Link(s) | Element::LinkedImage(s) if exemptions.link_urls => {
                if let Some(text) = bracketed_text(s, 0) {
                    width.link_exempt = (2 + display_len(text, mode)).min(full);
                }
            }
            Element::InlineImage(s) if exemptions.link_urls => {
                if let Some(alt) = bracketed_text(s, 1) {
                    width.link_exempt = (3 + display_len(alt, mode)).min(full);
                }
            }
            Element::Code { .. } if exemptions.code_spans => width.code_exempt = 0,
            _ => {}
        }
        width
    }
}

/// The source between the `[` at byte `open` and its matching `]`.
///
/// Bracket matching follows the document parser: a nested `[` raises the depth,
/// a backslash-escaped bracket is literal, and a bracket inside a code span is
/// literal too, so `[a \] b](url)` and ``[a `]` b](url)`` are each one link.
/// Returns `None` when `open` is not a `[` or the bracket never closes.
pub(crate) fn bracketed_text(s: &str, open: usize) -> Option<&str> {
    let bytes = s.as_bytes();
    if bytes.get(open) != Some(&b'[') {
        return None;
    }
    let mut depth = 0usize;
    let mut in_code_span = false;
    let mut escaped = false;
    for (i, &byte) in bytes.iter().enumerate().skip(open + 1) {
        if escaped {
            escaped = false;
            continue;
        }
        match byte {
            b'\\' => escaped = true,
            b'`' => in_code_span = !in_code_span,
            b'[' if !in_code_span => depth += 1,
            b']' if !in_code_span => match depth.checked_sub(1) {
                Some(next) => depth = next,
                None => return s.get(open + 1..i),
            },
            _ => {}
        }
    }
    None
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
fn extract_emphasis_and_code_spans(text: &str) -> (Vec<EmphasisSpan>, Vec<CodeSpan>) {
    // If neither marker is present, skip the parser entirely.
    let has_emphasis = text.contains(['*', '_', '~']);
    let has_code = text.contains('`');
    if !has_emphasis && !has_code {
        return (Vec::new(), Vec::new());
    }

    let mut emphasis_spans = Vec::new();
    let mut code_spans = Vec::new();

    let mut options = Options::empty();
    if has_emphasis {
        options.insert(Options::ENABLE_STRIKETHROUGH);
    }

    // Stacks to track nested formatting with their start positions
    let mut emphasis_stack: Vec<(usize, bool)> = Vec::new(); // (start_byte, uses_underscore)
    let mut strong_stack: Vec<(usize, bool)> = Vec::new();
    let mut strikethrough_stack: Vec<usize> = Vec::new();

    let parser = Parser::new_ext(text, options).into_offset_iter();

    for (event, range) in parser {
        match event {
            Event::Code(_) => {
                code_spans.push(CodeSpan {
                    start: range.start,
                    end: range.end,
                });
            }
            Event::Start(Tag::Emphasis) => {
                // Check if this uses underscore by looking at the original text
                let uses_underscore = text.get(range.start..range.start + 1) == Some("_");
                emphasis_stack.push((range.start, uses_underscore));
            }
            Event::End(TagEnd::Emphasis) => {
                if let Some((start_byte, uses_underscore)) = emphasis_stack.pop() {
                    let content_start = start_byte + 1;
                    let content_end = range.end - 1;
                    if content_end > content_start
                        && let Some(content) = text.get(content_start..content_end)
                    {
                        emphasis_spans.push(EmphasisSpan {
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
                let uses_underscore = text.get(range.start..range.start + 2) == Some("__");
                strong_stack.push((range.start, uses_underscore));
            }
            Event::End(TagEnd::Strong) => {
                if let Some((start_byte, uses_underscore)) = strong_stack.pop() {
                    let content_start = start_byte + 2;
                    let content_end = range.end - 2;
                    if content_end > content_start
                        && let Some(content) = text.get(content_start..content_end)
                    {
                        emphasis_spans.push(EmphasisSpan {
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
                    let double = text.get(start_byte..start_byte + 2) == Some("~~");
                    let marker_len = if double { 2 } else { 1 };
                    let content_start = start_byte + marker_len;
                    let content_end = range.end - marker_len;
                    if content_end > content_start
                        && let Some(content) = text.get(content_start..content_end)
                    {
                        emphasis_spans.push(EmphasisSpan {
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

    emphasis_spans.sort_by_key(|s| s.start);
    (emphasis_spans, code_spans)
}

#[derive(Debug, Clone)]
struct CodeSpan {
    start: usize,
    end: usize,
}

#[derive(Debug, Clone)]
struct LinkSpan {
    start: usize,
    end: usize,
    link_type: Option<LinkType>,
    is_image: bool,
    is_footnote: bool,
    /// How many links or images enclose this one. The image in
    /// `[![alt](img)](url)` sits at depth 1.
    depth: usize,
}

/// The outermost links, images and footnote references in `text`, sorted by
/// start. The top level holds each of these whole, so a construct nested in
/// another is covered by the one enclosing it.
fn extract_link_spans(text: &str, defined_references: Option<&HashSet<String>>) -> Vec<LinkSpan> {
    let mut spans = all_link_spans(text, defined_references);
    spans.retain(|span| span.depth == 0);
    spans
}

/// Every link, image and footnote reference in `text`, nested ones included,
/// sorted by start.
fn all_link_spans(text: &str, defined_references: Option<&HashSet<String>>) -> Vec<LinkSpan> {
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
            Event::End(TagEnd::Link | TagEnd::Image) => {
                if let Some((start_byte, link_type, is_image)) = stack.pop() {
                    let mut end = range.end;
                    if matches!(link_type, Some(LinkType::Collapsed) | Some(LinkType::CollapsedUnknown))
                        && text[end..].starts_with("[]")
                    {
                        end += 2;
                    }
                    spans.push(LinkSpan {
                        start: start_byte,
                        end,
                        link_type,
                        is_image,
                        is_footnote: false,
                        depth: stack.len(),
                    });
                }
            }
            Event::FootnoteReference(_) => {
                spans.push(LinkSpan {
                    start: range.start,
                    end: range.end,
                    link_type: None,
                    is_image: false,
                    is_footnote: true,
                    depth: stack.len(),
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
fn myst_role_len_at(text: &str, absolute_pos: usize, code_spans: &[CodeSpan]) -> Option<usize> {
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
    let code_span_start = absolute_pos + j;
    if let Ok(idx) = code_spans.binary_search_by_key(&code_span_start, |span| span.start) {
        let span = &code_spans[idx];
        let code_span_len = span.end - span.start;
        return Some(j + code_span_len);
    }

    None
}

/// Byte length of an inline-math span (`$math$`) starting at the very
/// beginning of `s`, if one starts there.
///
/// Mirrors INLINE_MATH_REGEX (`(?<!\$)\$(?!\$)([^\$]+)\$(?!\$)`) with the
/// leading lookbehind dropped: callers probe only at a slice start, where
/// the lookbehind passes vacuously.
fn inline_math_len_at_start(s: &str) -> Option<usize> {
    let bytes = s.as_bytes();
    // Opening `$` not followed by another `$` (that would be display math).
    if bytes.first() != Some(&b'$') || bytes.get(1) == Some(&b'$') {
        return None;
    }
    // Content is `[^$]+`: everything up to the closing `$`. It is non-empty
    // whenever a closing `$` exists, because the byte at index 1 is not `$`.
    let close = 1 + s[1..].find('$')?;
    // Closing `$` not followed by another `$`.
    if bytes.get(close + 1) == Some(&b'$') {
        return None;
    }
    Some(close + 1)
}

/// Absolute byte offsets of a cached pattern match within the full input text.
#[derive(Clone, Copy, Debug)]
struct PatternMatch {
    start: usize,
    end: usize,
}

/// Lazily-computed earliest match of one pattern within the unparsed suffix.
///
/// `parse_markdown_elements_inner` probes every pattern on every loop
/// iteration; re-running each search against the whole remaining suffix made
/// pathological inputs quadratic. The cache keeps the previous result as
/// absolute offsets: until the parse cursor moves past a cached match, that
/// match is still the earliest one, so the search is skipped.
///
/// This is sound only for patterns whose match at a given position does not
/// depend on where the searched slice starts (no `^`, no lookbehind): for
/// those, a cached miss stays a miss and a cached hit stays the earliest hit
/// as the cursor advances. A start-sensitive pattern needs a dedicated probe
/// at the cursor first (see the inline-math call site).
#[derive(Clone, Copy)]
enum PatternCache {
    Unsearched,
    NotFound,
    Found(PatternMatch),
}

impl PatternCache {
    /// Returns the earliest match at or after `cursor` as offsets relative to
    /// `remaining` (the unparsed suffix starting at `cursor`), re-running
    /// `find` on the suffix only when the cached result no longer applies.
    fn earliest_in(
        &mut self,
        remaining: &str,
        cursor: usize,
        find: impl FnOnce(&str) -> Option<(usize, usize)>,
    ) -> Option<(usize, usize)> {
        let stale = match self {
            PatternCache::Found(pm) => pm.start < cursor,
            PatternCache::NotFound => false,
            PatternCache::Unsearched => true,
        };
        if stale {
            *self = match find(remaining) {
                Some((start, end)) => PatternCache::Found(PatternMatch {
                    start: cursor + start,
                    end: cursor + end,
                }),
                None => PatternCache::NotFound,
            };
        }
        match self {
            PatternCache::Found(pm) => Some((pm.start - cursor, pm.end - cursor)),
            _ => None,
        }
    }
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
fn parse_markdown_elements_inner(
    text: &str,
    attr_lists: bool,
    myst_roles: bool,
    defined_references: Option<&HashSet<String>>,
) -> Vec<Element> {
    let mut elements = Vec::new();
    let mut remaining = text;

    // Pre-extract emphasis spans, link spans, and code spans using pulldown-cmark.
    // Emphasis and code spans are extracted in a single shared parse to reduce cmark overhead.
    // Link spans must run as a separate parse because link resolution (the broken-link
    // callback) changes bracket collapses, which shifts delimiter range boundaries.
    let (emphasis_spans, code_spans) = extract_emphasis_and_code_spans(text);
    let link_spans = extract_link_spans(text, defined_references);

    // One cache per probed pattern to avoid an O(N^2) worst case on long
    // inputs; see PatternCache for the validity rules.
    let mut cached_wiki_link = PatternCache::Unsearched;
    let mut cached_display_math = PatternCache::Unsearched;
    let mut cached_inline_math = PatternCache::Unsearched;
    let mut cached_emoji = PatternCache::Unsearched;
    let mut cached_html_entity = PatternCache::Unsearched;
    let mut cached_hugo_shortcode = PatternCache::Unsearched;
    let mut cached_html_tag = PatternCache::Unsearched;
    let mut cached_next_curly = PatternCache::Unsearched;

    // Cursor indices into the sorted span lists: spans behind the parse cursor
    // can never match again, so each list is advanced monotonically instead of
    // rescanned from the start on every iteration.
    let mut link_span_idx = 0usize;
    let mut emphasis_span_idx = 0usize;
    let mut code_span_idx = 0usize;

    while !remaining.is_empty() {
        // Calculate current byte offset in original text
        let current_offset = text.len() - remaining.len();
        // Find the earliest occurrence of any markdown pattern
        // Store (start, end, pattern_name) to unify regex and span-list results
        let mut earliest_match: Option<(usize, usize, &str)> = None;

        // Find the earliest link span
        while link_span_idx < link_spans.len() && link_spans[link_span_idx].start < current_offset {
            link_span_idx += 1;
        }
        let next_link: Option<&LinkSpan> = link_spans.get(link_span_idx);

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

        // Check for wiki-style links - [[wiki]]
        if let Some((start, end)) = cached_wiki_link.earliest_in(remaining, current_offset, |suffix| {
            WIKI_LINK_REGEX.find(suffix).map(|m| (m.start(), m.end()))
        }) && earliest_match.as_ref().is_none_or(|(s, _, _)| start < *s)
        {
            earliest_match = Some((start, end, "wiki_link"));
        }

        // Check for display math first (before inline) - $$math$$
        if let Some((start, end)) = cached_display_math.earliest_in(remaining, current_offset, |suffix| {
            DISPLAY_MATH_REGEX.find(suffix).map(|m| (m.start(), m.end()))
        }) && earliest_match.as_ref().is_none_or(|(s, _, _)| start < *s)
        {
            earliest_match = Some((start, end, "display_math"));
        }

        // Check for inline math - $math$
        // INLINE_MATH_REGEX opens with the lookbehind `(?<!\$)`, which is
        // slice-start-sensitive: at the start of the searched slice there is
        // no preceding character, so the lookbehind trivially passes, while
        // the cached search, anchored earlier, saw the real `$` predecessor
        // and can have rejected the same position. Positions past the cursor
        // are unaffected by where the slice starts, so the cache stays valid
        // for them; only a match beginning exactly at the cursor can be
        // missing from it. When the cursor sits directly after a `$`, probe
        // for that one match in place, leaving the cache untouched. (Either
        // rescanning the suffix here or storing the probe hit in the cache is
        // quadratic on math-heavy inputs: each consumed span would trigger a
        // fresh scan of everything that follows.)
        let inline_math_probe = if current_offset > 0 && text.as_bytes()[current_offset - 1] == b'$' {
            inline_math_len_at_start(remaining).map(|len| (0, len))
        } else {
            None
        };
        if let Some((start, end)) = inline_math_probe.or_else(|| {
            cached_inline_math.earliest_in(remaining, current_offset, |suffix| {
                INLINE_MATH_REGEX
                    .find(suffix)
                    .ok()
                    .flatten()
                    .map(|m| (m.start(), m.end()))
            })
        }) && earliest_match.as_ref().is_none_or(|(s, _, _)| start < *s)
        {
            earliest_match = Some((start, end, "inline_math"));
        }

        // Check for emoji shortcodes - :emoji:
        if let Some((start, end)) = cached_emoji.earliest_in(remaining, current_offset, |suffix| {
            EMOJI_SHORTCODE_REGEX.find(suffix).map(|m| (m.start(), m.end()))
        }) && earliest_match.as_ref().is_none_or(|(s, _, _)| start < *s)
        {
            earliest_match = Some((start, end, "emoji"));
        }

        // Check for HTML entities - &nbsp; etc
        if let Some((start, end)) = cached_html_entity.earliest_in(remaining, current_offset, |suffix| {
            HTML_ENTITY_REGEX.find(suffix).map(|m| (m.start(), m.end()))
        }) && earliest_match.as_ref().is_none_or(|(s, _, _)| start < *s)
        {
            earliest_match = Some((start, end, "html_entity"));
        }

        // Check for Hugo shortcodes - {{< ... >}} or {{% ... %}}
        // Must be checked before other patterns to avoid false sentence breaks
        if let Some((start, end)) = cached_hugo_shortcode.earliest_in(remaining, current_offset, |suffix| {
            HUGO_SHORTCODE_REGEX.find(suffix).map(|m| (m.start(), m.end()))
        }) && earliest_match.as_ref().is_none_or(|(s, _, _)| start < *s)
        {
            earliest_match = Some((start, end, "hugo_shortcode"));
        }

        // Check for HTML tags - <tag> </tag> <tag/>
        // But exclude autolinks like <https://...> or <mailto:...> or email
        // autolinks <user@domain.com>: those are left for link_span handling.
        // The search skips past autolinks instead of giving up so the cache
        // lands on the first real tag; bailing out at an autolink would re-run
        // this scan from the same spot on every iteration.
        if let Some((start, end)) = cached_html_tag.earliest_in(remaining, current_offset, |suffix| {
            let mut from = 0;
            while let Some(m) = HTML_TAG_PATTERN.find(&suffix[from..]) {
                let (tag_start, tag_end) = (from + m.start(), from + m.end());
                let tag = &suffix[tag_start..tag_end];
                // Autolink starting with a protocol or mailto:?
                let is_url_autolink = tag.starts_with("<http://")
                    || tag.starts_with("<https://")
                    || tag.starts_with("<mailto:")
                    || tag.starts_with("<ftp://")
                    || tag.starts_with("<ftps://");
                // Email autolink (per CommonMark spec: <local@domain.tld>)?
                // Use centralized EMAIL_PATTERN for consistency with MD034 and other rules
                let is_email_autolink = {
                    let content = tag.trim_start_matches('<').trim_end_matches('>');
                    EMAIL_PATTERN.is_match(content)
                };
                if is_url_autolink || is_email_autolink {
                    from = tag_end;
                } else {
                    return Some((tag_start, tag_end));
                }
            }
            None
        }) && earliest_match.as_ref().is_none_or(|(s, _, _)| start < *s)
        {
            earliest_match = Some((start, end, "html_tag"));
        }

        // Find earliest non-link special characters
        let mut next_special = remaining.len();
        let mut special_type = "";
        let mut pulldown_emphasis: Option<&EmphasisSpan> = None;
        let mut attr_list_len: usize = 0;
        let mut myst_role_len: usize = 0;

        // Check for code spans using pulldown-cmark pre-extracted spans
        while code_span_idx < code_spans.len() && code_spans[code_span_idx].start < current_offset {
            code_span_idx += 1;
        }
        let next_code_span: Option<&CodeSpan> = code_spans.get(code_span_idx);
        if let Some(span) = next_code_span {
            let pos_in_remaining = span.start - current_offset;
            if pos_in_remaining < next_special {
                next_special = pos_in_remaining;
                special_type = "pulldown_code";
            }
        }

        // Position of the next `{`, shared by the MyST-role and attr-list
        // probes below
        let next_curly_pos = cached_next_curly
            .earliest_in(remaining, current_offset, |suffix| {
                suffix.find('{').map(|pos| (pos, pos + 1))
            })
            .map(|(start, _)| start);

        // Check for MyST inline roles - {role}`content` (e.g. {cite:p}`ref`).
        // Checked before the bare code-span handling so the role's trailing code
        // span is absorbed into the atomic role rather than split off, and before
        // attr lists since a role's `{` would otherwise be probed as an attr list.
        if myst_roles
            && let Some(pos) = next_curly_pos
            && pos < next_special
            && let Some(role_len) = myst_role_len_at(&remaining[pos..], current_offset + pos, &code_spans)
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
        while emphasis_span_idx < emphasis_spans.len() && emphasis_spans[emphasis_span_idx].start < current_offset {
            emphasis_span_idx += 1;
        }
        if let Some(span) = emphasis_spans.get(emphasis_span_idx) {
            let pos_in_remaining = span.start - current_offset;
            if pos_in_remaining < next_special {
                next_special = pos_in_remaining;
                special_type = "pulldown_emphasis";
                pulldown_emphasis = Some(span);
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
                _ => unreachable!("unknown pattern type: {}", pattern_type),
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
                    let code_raw = &remaining[..span_len];
                    if let Some((content, marker)) = decompose_code_span(code_raw) {
                        elements.push(Element::Code {
                            content: content.to_string(),
                            marker: marker.to_string(),
                        });
                    } else {
                        elements.push(Element::Text(code_raw.to_string()));
                    }
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
                    let span = pulldown_emphasis.expect("pulldown_emphasis must be set");
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
                }
                _ => {
                    // No special elements found, add all remaining text
                    elements.push(Element::Text(remaining.to_string()));
                    break;
                }
            }
        }
    }

    // Merge contiguous text elements to clean up the output.
    let mut merged_elements = Vec::new();
    for el in elements {
        match el {
            Element::Text(s) => {
                if let Some(Element::Text(last_s)) = merged_elements.last_mut() {
                    last_s.push_str(&s);
                } else {
                    merged_elements.push(Element::Text(s));
                }
            }
            other => merged_elements.push(other),
        }
    }
    merged_elements
}

/// The whitespace the source put in front of the element at `idx`, as reflow
/// should re-emit it.
///
/// A span carries no whitespace of its own, so the gap before it lives at the
/// end of the text element preceding it, which the sentence and clause paths
/// trim away as they accumulate. Reading the gap back from the source is what
/// keeps a standalone `-` from being glued onto the span after it: the
/// characters a line happens to end with cannot tell a dash that closes a word
/// from one that stands alone, and the same holds for a bracket or paren.
///
/// A run of breakable whitespace renders as one space and comes back as one. A
/// non-breaking space is a character the reader sees, so a gap containing one
/// is carried through exactly as written.
fn source_gap_before(elements: &[Element], idx: usize) -> &str {
    let Some(Element::Text(previous)) = idx.checked_sub(1).map(|prev| &elements[prev]) else {
        return "";
    };

    let gap = &previous[previous.trim_end_matches(char::is_whitespace).len()..];
    if gap.is_empty() {
        ""
    } else if gap.contains(is_non_breaking_space) {
        gap
    } else {
        " "
    }
}

/// Open `gap` before the element about to be appended, unless the line has
/// nothing for it to follow or already ends with whitespace of its own.
fn push_source_gap(current_line: &mut String, gap: &str) {
    if !gap.is_empty() && !current_line.is_empty() && !current_line.ends_with(char::is_whitespace) {
        current_line.push_str(gap);
    }
}

/// True when `text` consists solely of setext-underline or thematic-break
/// characters: a run of `=` or `-` (setext underline, any count, no internal
/// spaces) or 3+ `-`/`*`/`_` optionally separated by spaces (thematic break).
/// A paragraph-continuation line like this converts the previous line into a
/// heading or inserts a horizontal rule.
fn is_setext_or_thematic(text: &str) -> bool {
    let mut marker = 0u8;
    let mut count = 0usize;
    let mut has_space = false;
    for &b in text.as_bytes() {
        match b {
            b' ' | b'\t' => has_space = true,
            b'-' | b'=' | b'*' | b'_' => {
                if marker == 0 {
                    marker = b;
                } else if b != marker {
                    return false;
                }
                count += 1;
            }
            _ => return false,
        }
    }
    match marker {
        b'=' => !has_space,
        b'-' => !has_space || count >= 3,
        b'*' | b'_' => count >= 3,
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
        b':' => is_definition_list_item(text) || text.starts_with(":::"),
        b'|' => true,
        b'#' => {
            let hashes = bytes.iter().take_while(|&&b| b == b'#').count();
            hashes <= 6 && marker_then_boundary(hashes)
        }
        b'`' => bytes.iter().take_while(|&&b| b == b'`').count() >= 3,
        b'~' => bytes.iter().take_while(|&&b| b == b'~').count() >= 3,
        // An ordered list is the one construct here that cannot always
        // interrupt a paragraph: it does so only when it is numbered 1 and its
        // first item has content. `7. item` and a bare `123456.` are prose to
        // the parser, so guarding them would refuse a legal wrap and leave an
        // unfixable long line. Leading zeros still make the number 1 (`01.`),
        // and a marker is at most 9 digits.
        b'0'..=b'9' => {
            let digits = bytes.iter().take_while(|b| b.is_ascii_digit()).count();
            digits <= 9
                && text[..digits].trim_start_matches('0') == "1"
                && bytes.len() > digits + 1
                && (bytes[digits] == b'.' || bytes[digits] == b')')
                && (bytes[digits + 1] == b' ' || bytes[digits + 1] == b'\t')
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
        merged.push(line);
        // A merge can itself produce an opener: a line holding just `1.` is
        // inert on its own, but absorbing a following `[ref]:` turns it into
        // `1. [ref]:`, a real list item. Keep folding until the tail is inert.
        while merged.len() > 1 && starts_block_construct(merged.last().expect("non-empty")) {
            let last = merged.pop().expect("non-empty");
            let prev = merged.last_mut().expect("len > 1");
            prev.push(' ');
            prev.push_str(last.trim_start());
        }
    }
    merged
}

/// Reflow elements for sentence-per-line mode
fn reflow_elements_sentence_per_line(elements: &[Element], options: &ReflowOptions) -> Vec<String> {
    let abbreviations = get_abbreviations(&options.abbreviations);
    let require_sentence_capital = options.require_sentence_capital;
    let mut lines = Vec::new();
    let mut current_line = String::new();

    for (idx, element) in elements.iter().enumerate() {
        // Text and emphasis are absorbed the same way. An emphasis span is
        // rendered back to its source form and then treated as ordinary text,
        // so a sentence boundary inside it breaks the line without closing and
        // reopening the markers: a line break inside a span is whitespace, and
        // whitespace is all a reflow is allowed to change.
        let is_span = matches!(
            element,
            Element::Italic { .. } | Element::Bold { .. } | Element::Strikethrough { .. }
        );
        let piece = match element {
            // Text already carries its own spacing from tokenization.
            Element::Text(text) => Some(text.clone()),
            Element::Italic { content, underscore } => Some(wrap_emphasis(
                content,
                if *underscore { "_" } else { "*" },
                &mut current_line,
                source_gap_before(elements, idx),
            )),
            Element::Bold { content, underscore } => Some(wrap_emphasis(
                content,
                if *underscore { "__" } else { "**" },
                &mut current_line,
                source_gap_before(elements, idx),
            )),
            Element::Strikethrough { content, double } => Some(wrap_emphasis(
                content,
                if *double { "~~" } else { "~" },
                &mut current_line,
                source_gap_before(elements, idx),
            )),
            _ => None,
        };

        if let Some(piece) = piece {
            // Where the piece lands in the combined line. A span begins with its
            // own opening marker, which the splitter must not read as the marker
            // closing the sentence in front of it.
            let appended_span_start = is_span.then_some(current_line.len());
            let combined = format!("{current_line}{piece}");
            // Use the pre-computed abbreviations set to avoid redundant computation
            let sentences = split_into_sentences_with_set(
                &combined,
                &abbreviations,
                require_sentence_capital,
                appended_span_start,
                options.defined_references.as_ref(),
            );

            // A bracketed element right after the piece may hold the sentence
            // in front of it open: `Claim ends here. [smith](url) continues.`
            // is one sentence to the splitter, since the link's text starts
            // with a lowercase letter, and so is `Claim ends here. [Smith 2020]`.
            // The splitter decides by reading the sentence with the element
            // appended, so the check counting sentences and this reflow agree
            // on where the line breaks. Every other kind of element closes the
            // sentence in front of it.
            let next_bracketed = elements
                .get(idx + 1)
                .filter(|next| next.opens_with_bracket())
                .map(|next| (source_gap_before(elements, idx + 1), next.to_string()));
            let closes_before_next = |sentence: &str| -> bool {
                let Some((gap, next_str)) = &next_bracketed else {
                    return true;
                };
                let mut probe = sentence.to_string();
                push_source_gap(&mut probe, gap);
                probe.push_str(next_str);
                let probe_sentences = split_into_sentences_with_set(
                    &probe,
                    &abbreviations,
                    require_sentence_capital,
                    None,
                    options.defined_references.as_ref(),
                );
                probe_sentences.last().is_some_and(|last| last == next_str)
            };

            if sentences.len() > 1 {
                // Accumulate rather than emit-and-overwrite: a sentence held
                // back for the next element must absorb what follows it, or the
                // text that follows would reach the output ahead of it.
                let mut pending = String::new();
                let last = sentences.len() - 1;
                for (i, sentence) in sentences.iter().enumerate() {
                    if !pending.is_empty() {
                        pending.push(' ');
                    }
                    pending.push_str(sentence);

                    // The splitter already decided every boundary except the
                    // final one, which is just the leftover tail. Hold a tail
                    // that no punctuation closed, and hold any piece ending in
                    // an abbreviation the splitter broke after regardless.
                    let closed = i < last || (ends_with_sentence_punct(&pending) && closes_before_next(&pending));
                    if closed && !text_ends_with_abbreviation(&pending, &abbreviations) {
                        lines.push(std::mem::take(&mut pending));
                    }
                }
                current_line = pending;
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

                if ends_with_sentence_punct
                    && !text_ends_with_abbreviation(trimmed, &abbreviations)
                    && closes_before_next(trimmed)
                {
                    // Complete single sentence - emit it (trimming only
                    // breakable whitespace so edge NBSPs survive)
                    lines.push(combined.trim_matches(is_breakable_whitespace).to_string());
                    current_line.clear();
                } else {
                    // Incomplete sentence - continue accumulating
                    current_line = combined;
                }
            }
        } else {
            // Non-text, non-emphasis elements (Code, Links, etc.)
            let element_str = format!("{element}");
            push_source_gap(&mut current_line, source_gap_before(elements, idx));
            current_line.push_str(&element_str);
        }
    }

    // Add any remaining content.
    //
    // An atomic element — a code span, a link, an autolink — is appended without
    // the splitter ever reading it, on the understanding that a later text
    // element re-splits the line. A trailing one has no later element, so a
    // sentence boundary in front of it is taken here or lost, and a lost one
    // leaves `check` reporting a paragraph that `fmt` will not break.
    //
    // Not when the tail carries a non-breaking space. The splitter trims each
    // sentence with `str::trim`, which counts one as whitespace, and the edge
    // trimming below exists precisely to keep it.
    if !current_line.is_empty() {
        let split_tail = (!current_line.contains(is_non_breaking_space))
            .then(|| {
                split_into_sentences_with_set(
                    &current_line,
                    &abbreviations,
                    require_sentence_capital,
                    None,
                    options.defined_references.as_ref(),
                )
            })
            .filter(|sentences| sentences.len() > 1);

        match split_tail {
            Some(sentences) => lines.extend(sentences),
            None => lines.push(current_line.trim_matches(is_breakable_whitespace).to_string()),
        }
    }
    lines
}

/// Restore an emphasis span to its source form, opening the gap the source had
/// in front of it. Unlike text elements, a span carries no surrounding
/// whitespace of its own.
fn wrap_emphasis(content: &str, marker: &str, current_line: &mut String, gap: &str) -> String {
    push_source_gap(current_line, gap);
    format!("{marker}{content}{marker}")
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
/// A real clause boundary is followed by breakable whitespace (or ends the
/// text). Two reasons, and they agree: `,;:` with no following space sit
/// *inside* a token (`16:9`, `key:value`, a MyST role like `{cite:p}`), and a
/// line break renders as a space, so breaking where the source has none inserts
/// one. That holds for the em dash too: `cost—benefit` renders as one word,
/// `cost—\nbenefit` as two. A non-breaking space is not a boundary either: it
/// exists to forbid the break, so the scan keeps looking for an earlier one.
fn clause_break_allowed_after(chars: &[char], i: usize) -> bool {
    match chars.get(i + 1) {
        None => true,
        Some(next) => is_breakable_whitespace(*next),
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
fn paren_group_end<'a>(slice: &'a str, element_spans: &[ElementSpan], offset: usize) -> Option<(usize, &'a str)> {
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
    element_spans: &[ElementSpan],
    length_mode: ReflowLengthMode,
) -> Option<(String, String)> {
    let min_first_len = ((line_length as f64) * MIN_SPLIT_RATIO) as usize;

    // Strategy 1: text starts with '(' — isolate the parenthetical as its own line.
    if text.starts_with('(')
        && let Some((end_local, inner)) = paren_group_end(text, element_spans, 0)
        && inner.contains(' ')
    {
        // Whatever follows the closing ')' up to the next breakable whitespace
        // belongs to the parenthetical: a break there would render as a space the
        // text does not have, and it would orphan the punctuation (`).`, `),`,
        // `)"`) at the head of the continuation line. Whitespace inside an inline
        // element is that element's own content, so a boundary landing there is
        // no boundary at all and the scan resumes past the element.
        let mut first_end = end_local;
        loop {
            first_end += text[first_end..]
                .char_indices()
                .take_while(|(_, c)| !is_breakable_whitespace(*c))
                .last()
                .map_or(0, |(idx, c)| idx + c.len_utf8());
            match element_containing(first_end, element_spans) {
                Some(span) => first_end = span.end,
                None => break,
            }
        }
        let rest_start = first_end;
        let first = &text[..first_end];
        // No MIN_SPLIT_RATIO check: a parenthetical unit is always a valid
        // semantic line regardless of its length.
        if measure(first, 0, element_spans, length_mode).fits(line_length) {
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
            let first = text[..pos].trim_end_matches(is_breakable_whitespace);
            let first_len = measure(first, 0, element_spans, length_mode).effective();
            // The '(' must follow breakable whitespace: splitting `f(a b)` into
            // `f` and `(a b)` would render as `f (a b)`.
            if first.len() < pos
                && !first.is_empty()
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
    let first = text[..open_byte].trim_end_matches(is_breakable_whitespace).to_string();
    let rest = text[open_byte..].to_string();
    if first.is_empty() || rest.trim().is_empty() {
        return None;
    }
    Some((first, rest))
}

/// A non-Text element's byte span in a flat text representation, with the
/// columns each of the checker's exemptions forgives it.
///
/// The offsets exist so a split position can be kept out of an element; the
/// savings exist so a substring can be measured the way the checker measures it.
/// Both are computed from the same walk over the same elements, which is what
/// keeps them consistent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ElementSpan {
    start: usize,
    end: usize,
    full: usize,
    /// Columns the link/image URL exemption forgives, zero when it is off or
    /// does not apply to this element.
    link_saving: usize,
    /// Columns the code-span exemption forgives, on the same terms.
    code_saving: usize,
    /// Whether this element is hard atomic (cannot be broken even as fallback)
    is_hard: bool,
}

impl ElementSpan {
    /// A span covering `len` bytes from `start`, for an element whose full
    /// width is `full` and whose exempt widths are `width`.
    fn new(start: usize, len: usize, full: usize, width: LineWidth, is_hard: bool) -> Self {
        Self {
            start,
            end: start + len,
            full,
            link_saving: full - width.link_exempt,
            code_saving: full - width.code_exempt,
            is_hard,
        }
    }

    fn contains(&self, pos: usize) -> bool {
        pos > self.start && pos < self.end
    }

    fn within(&self, start: usize, end: usize) -> bool {
        self.start >= start && self.end <= end
    }

    fn exempt_width(&self) -> LineWidth {
        LineWidth {
            link_exempt: self.full - self.link_saving,
            code_exempt: self.full - self.code_saving,
        }
    }
}

/// The wrappable text of a link or image element, with the source around it:
/// `(prefix, inner, suffix)` where `prefix` is `[` or `![`, `inner` the
/// bracketed text, and `suffix` everything from the closing `]` on. `None`
/// when the element's text may not wrap: the option is off, or the element
/// has no prose text to wrap (a linked image's "text" is an image).
fn link_text_parts(element: &Element, break_link_text: bool) -> Option<(&str, &str, &str)> {
    if !break_link_text {
        return None;
    }
    let (raw, open) = match element {
        Element::Link(raw)
        | Element::ReferenceLink(raw)
        | Element::EmptyReferenceLink(raw)
        | Element::ShortcutReference(raw) => (raw.as_str(), 0),
        Element::InlineImage(raw) | Element::ReferenceImage(raw) | Element::EmptyReferenceImage(raw) => {
            (raw.as_str(), 1)
        }
        _ => return None,
    };
    let inner = bracketed_text(raw, open)?;
    Some((&raw[..=open], inner, &raw[open + 1 + inner.len()..]))
}

/// Whether an element's span is hard atomic: even the fallback pass that
/// relaxes soft spans may not break inside it. An emphasis span is soft
/// unless it nests a construct whose whitespace is not prose (a code span,
/// link, HTML tag, math or attr list). A link or image whose text may wrap
/// (see [`ReflowOptions::break_link_text`]) is soft on the same terms, and
/// additionally only when its suffix holds no whitespace: a break inside a
/// title or a spaced destination would rewrite the link.
fn element_is_hard(element: &Element, break_link_text: bool) -> bool {
    match element {
        Element::Bold { content, .. } | Element::Italic { content, .. } | Element::Strikethrough { content, .. } => {
            content.contains(['[', '`', '<', '$', '{'])
        }
        _ => match link_text_parts(element, break_link_text) {
            Some((_, inner, suffix)) => {
                inner.contains(['[', '`', '<', '$', '{']) || suffix.chars().any(char::is_whitespace)
            }
            None => true,
        },
    }
}

/// Compute element spans for a flat text representation of elements.
///
/// The offsets are byte positions, so they are always measured in
/// [`ReflowLengthMode::Bytes`] regardless of how lines are measured; only the
/// savings depend on `mode` and the active exemptions.
fn compute_element_spans(
    elements: &[Element],
    mode: ReflowLengthMode,
    exemptions: LengthExemptions,
    break_link_text: bool,
) -> Vec<ElementSpan> {
    let mut spans = Vec::new();
    let mut offset = 0;
    for element in elements {
        let len = element.display_len(ReflowLengthMode::Bytes);
        if !matches!(element, Element::Text(_)) {
            let full = element.display_len(mode);
            let width = element.exempt_width(mode, exemptions);
            spans.push(ElementSpan::new(
                offset,
                len,
                full,
                width,
                element_is_hard(element, break_link_text),
            ));
        }
        offset += len;
    }
    spans
}

/// Width of `text`, which sits at `[offset, offset + text.len())` of the line the
/// spans were computed for, under each exemption separately.
///
/// Only elements lying wholly inside the range are discounted. A split never
/// lands inside an element, so a partially covered element means the caller is
/// measuring something that is not a candidate line, and charging it in full is
/// the safe reading.
fn measure(text: &str, offset: usize, spans: &[ElementSpan], mode: ReflowLengthMode) -> LineWidth {
    let full = display_len(text, mode);
    let end = offset + text.len();
    let mut width = LineWidth::plain(full);
    for span in spans.iter().filter(|span| span.within(offset, end)) {
        width.link_exempt -= span.link_saving;
        width.code_exempt -= span.code_saving;
    }
    width
}

/// Width of a standalone line under each exemption.
///
/// Callers that already hold the line's element spans should use [`measure`];
/// this is for the sites that see only the finished line and so have to parse it.
fn line_width_components(line: &str, options: &ReflowOptions) -> LineWidth {
    let raw = display_len(line, options.length_mode);
    if !options.length_exemptions.any() {
        return LineWidth::plain(raw);
    }
    let elements = parse_markdown_elements_inner(
        line,
        options.attr_lists,
        options.myst_roles,
        options.defined_references.as_ref(),
    );
    let spans = compute_element_spans(
        &elements,
        options.length_mode,
        options.length_exemptions,
        options.break_link_text,
    );
    measure(line, 0, &spans, options.length_mode)
}

/// Width of a standalone line as the checker measures it.
fn line_width(line: &str, options: &ReflowOptions) -> usize {
    line_width_components(line, options).effective()
}

/// Whether a standalone line fits the budget as the checker measures it.
///
/// A saving is never negative, so the exempt width never exceeds the raw width:
/// a line that already fits as written fits under any exemption too, and needs
/// no parse. That keeps the parse off the path most lines take.
fn line_fits(line: &str, options: &ReflowOptions) -> bool {
    display_len(line, options.length_mode) <= options.line_length || line_width(line, options) <= options.line_length
}

/// The non-Text element span that strictly contains `pos`, if any.
fn element_containing(pos: usize, spans: &[ElementSpan]) -> Option<ElementSpan> {
    spans.iter().copied().find(|span| span.contains(pos))
}

/// Check if a byte position falls inside any non-Text element span
fn is_inside_element(pos: usize, spans: &[ElementSpan]) -> bool {
    element_containing(pos, spans).is_some()
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
    element_spans: &[ElementSpan],
    length_mode: ReflowLengthMode,
) -> Option<(String, String)> {
    let chars: Vec<char> = text.chars().collect();
    let min_first_len = ((line_length as f64) * MIN_SPLIT_RATIO) as usize;

    // Find the char index where accumulated display width exceeds line_length.
    // An element the checker discounts is charged its reduced width and stepped
    // over whole, so the search window reaches as far as the exempt measure of a
    // prefix allows; scanning char by char through a discounted URL would stop
    // short of break points that are legal under it.
    let mut width_acc = LineWidth::default();
    let mut search_end_char = 0;
    let mut byte = 0usize;
    let mut idx = 0usize;
    while idx < chars.len() {
        let (advance_chars, advance_bytes, width) = match element_spans.iter().find(|s| s.start == byte) {
            Some(span) => {
                let source = &text[span.start..span.end];
                (
                    source.chars().count(),
                    source.len(),
                    measure(source, span.start, element_spans, length_mode),
                )
            }
            None => {
                let c = chars[idx];
                (
                    1,
                    c.len_utf8(),
                    LineWidth::plain(display_len(&c.to_string(), length_mode)),
                )
            }
        };
        if !(width_acc + width).fits(line_length) {
            break;
        }
        width_acc += width;
        byte += advance_bytes;
        idx += advance_chars;
        search_end_char = idx;
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
    if measure(&first, 0, element_spans, length_mode).effective() < min_first_len {
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
fn paren_depth_map(text: &str, element_spans: &[ElementSpan]) -> Vec<i32> {
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
/// group — i.e. it starts with `(`, ends with `)` (possibly followed by the
/// punctuation `split_at_parenthetical` attaches to it), has balanced parens
/// throughout, and the inner content contains at least one space (matching the
/// ≥2-word threshold used by `split_at_parenthetical` when deciding to split).
///
/// Used to prevent the short-line merge step from collapsing intentional
/// parenthetical splits back into the previous line.
fn is_standalone_parenthetical(line: &str) -> bool {
    let trimmed = line.trim();
    if !trimmed.starts_with('(') {
        return false;
    }
    // Strip the attached tail to find the real end: everything after the last
    // ')' belongs to the group only when no whitespace separates it.
    let Some(close) = trimmed.rfind(')') else {
        return false;
    };
    if trimmed[close + 1..].contains(char::is_whitespace) {
        return false;
    }
    let core = &trimmed[..=close];
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
    element_spans: &[ElementSpan],
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
                let first_part_len = measure(first_part, 0, element_spans, length_mode).effective();

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

/// Whether a proposed split takes the place of whitespace that `text` already has.
///
/// `first` is a prefix of `text` with its trailing whitespace removed and `rest`
/// a suffix with its leading whitespace removed, so the bytes between them are
/// exactly what the split consumed. That gap must be non-empty and hold nothing
/// but breakable whitespace the paragraph owns: the newline replacing it renders
/// as a single space, so an empty gap inserts a word boundary the author did not
/// write, and a gap holding anything else drops content. Whitespace inside an
/// inline element belongs to that element, where it is literal (a code span) or
/// structural (a link destination), never a place a line may break.
fn replaces_whitespace(text: &str, first: &str, rest: &str, element_spans: &[ElementSpan]) -> bool {
    if !text.starts_with(first) || !text.ends_with(rest) {
        return false;
    }
    let gap_end = text.len() - rest.len();
    gap_end > first.len()
        && text[first.len()..gap_end].chars().all(is_breakable_whitespace)
        && !element_spans
            .iter()
            .any(|span| first.len() < span.end && span.start < gap_end)
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
fn cascade_split_line(text: &str, options: &ReflowOptions) -> Vec<String> {
    let line_length = options.line_length;
    let length_mode = options.length_mode;
    let attr_lists = options.attr_lists;
    let myst_roles = options.myst_roles;
    let defined_references = options.defined_references.as_ref();
    if line_length == 0 || display_len(text, length_mode) <= line_length {
        return vec![text.to_string()];
    }

    let elements = parse_markdown_elements_inner(text, attr_lists, myst_roles, defined_references);
    let element_spans = compute_element_spans(
        &elements,
        length_mode,
        options.length_exemptions,
        options.break_link_text,
    );

    // The raw width is over budget, but an exemption may still bring the line
    // under it, in which case the checker accepts it as written.
    if measure(text, 0, &element_spans, length_mode).fits(line_length) {
        return vec![text.to_string()];
    }

    // Element spans of the remaining suffix `text[start..]`, re-based so their
    // offsets are relative to the suffix. Split points never fall inside an
    // element, so every span lies wholly before or wholly at/after `start`.
    let rebased_spans = |start: usize| -> Vec<ElementSpan> {
        if start == 0 {
            return element_spans.clone();
        }
        element_spans
            .iter()
            .filter(|span| span.end > start)
            .map(|span| ElementSpan {
                start: span.start.saturating_sub(start),
                end: span.end.saturating_sub(start),
                ..*span
            })
            .collect()
    };

    let mut result = Vec::new();
    let mut start = 0usize;

    loop {
        let remaining = &text[start..];
        let spans = rebased_spans(start);
        if measure(remaining, 0, &spans, length_mode).fits(line_length) {
            result.push(remaining.to_string());
            return result;
        }

        // `rest` is always a suffix of `remaining` (the splitters only trim its
        // leading whitespace), so `remaining.len() - rest.len()` is the number of
        // bytes consumed, and the new absolute offset is `start + consumed`.
        //
        // Every candidate must stand in for whitespace the text already has: a
        // line break renders as a space, so one placed between two characters
        // that were adjacent changes the rendered paragraph. A strategy that
        // proposes such a split is skipped and the next one gets a turn.
        let at_whitespace = |candidate: Option<(String, String)>| {
            candidate.filter(|(first, rest)| replaces_whitespace(remaining, first, rest, &spans))
        };
        let split = at_whitespace(split_at_parenthetical(remaining, line_length, &spans, length_mode))
            .or_else(|| at_whitespace(split_at_clause_punctuation(remaining, line_length, &spans, length_mode)))
            .or_else(|| at_whitespace(split_at_break_word(remaining, line_length, &spans, length_mode)));

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
    let mut fallback_options = options.clone();
    fallback_options.break_on_sentences = false;
    fallback_options.preserve_breaks = false;
    fallback_options.sentence_per_line = false;
    fallback_options.semantic_line_breaks = false;
    fallback_options.require_sentence_capital = true;
    fallback_options.max_list_continuation_indent = None;
    fallback_options.defined_references = None;
    let remaining = &text[start..];
    let tail_elements = if start == 0 {
        elements
    } else {
        parse_markdown_elements_inner(remaining, attr_lists, myst_roles, defined_references)
    };
    result.extend(reflow_elements(&tail_elements, &fallback_options));
    result
}

/// Reflow elements using semantic line breaks strategy:
/// 1. Split at sentence boundaries (always)
/// 2. For lines exceeding line_length, cascade through clause punct → break-words → word wrap
fn reflow_elements_semantic(elements: &[Element], options: &ReflowOptions) -> Vec<String> {
    // Step 1: Split into sentences using existing sentence-per-line logic
    let sentence_lines = reflow_elements_sentence_per_line(elements, options);

    // Step 2: For each sentence line, apply cascading splits if it exceeds line_length
    // When line_length is 0 (unlimited), skip cascading — sentence splits only
    if options.line_length == 0 {
        return sentence_lines;
    }

    let mut result = Vec::new();
    for line in sentence_lines {
        if line_fits(&line, options) {
            result.push(line);
        } else {
            result.extend(cascade_split_line(&line, options));
        }
    }

    // Step 3: Merge very short trailing lines back into the previous line.
    // Word wrap can produce lines like "was" or "see" on their own, which reads poorly.
    let min_line_len = ((options.line_length as f64) * MIN_SPLIT_RATIO) as usize;
    let mut merged: Vec<String> = Vec::with_capacity(result.len());
    for line in result {
        if !merged.is_empty() && line_width(&line, options) < min_line_len && !line.trim().is_empty() {
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
                if line_fits(&combined, options) {
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
/// `element_spans` locates the non-Text elements in the line. Spans use
/// exclusive bounds (pos > start && pos < end) because element delimiters
/// (e.g., `[`, `]`, `(`, `)`, `<`, `>`, `` ` ``) are never spaces, so only
/// interior positions need protection. The scan keeps looking left past
/// construct-leading suffixes (e.g. a trailing `- `), so a usable earlier break
/// point is found instead of forcing an overlong line.
fn rfind_safe_space(
    line: &str,
    element_spans: &[ElementSpan],
    options: &ReflowOptions,
    relax_soft_spans: bool,
) -> Option<usize> {
    line.char_indices().rev().map(|(pos, _)| pos).find(|&pos| {
        line.as_bytes()[pos] == b' '
            && !is_inside_element_filtered(pos, element_spans, options, relax_soft_spans)
            && !starts_block_construct(&line[pos + 1..])
    })
}

fn is_inside_element_filtered(
    pos: usize,
    spans: &[ElementSpan],
    options: &ReflowOptions,
    relax_soft_spans: bool,
) -> bool {
    spans.iter().any(|span| {
        span.contains(pos)
            && (!relax_soft_spans
                || span.is_hard
                || (options.atomic_spans && span.exempt_width().fits(options.line_length)))
    })
}

/// A token that must not start a wrapped line, together with the width it
/// contributes and the separator that precedes it. The width travels with the
/// text because only the caller knows which construct produced it, and so which
/// exemption it earns.
#[derive(Clone, Copy)]
struct Attached<'a> {
    text: &'a str,
    width: LineWidth,
    separator: &'a str,
}

/// Break `current_line` one word earlier so `attach` never starts a wrapped
/// line: everything before the line's last safe space is emitted as a
/// finished line, and the carried word plus the separator plus the attached
/// text becomes the new current line. The returned byte length of the carried
/// word lets callers re-record a span for the attached text. Returns `None`
/// (line untouched) when the line has no safe break point.
///
/// The new width is the carried text measured through the spans it came with,
/// plus the attached width, so an exemption the carried text or the attached
/// token earns is preserved across the break instead of being re-derived from a
/// bare string.
///
/// The carried text keeps the element spans that fell inside it, rebased to the
/// new line. Dropping them would leave a later break blind to an element the
/// carried text still holds, and so free to split a link or a code span down
/// the middle.
fn break_before_attached(
    lines: &mut Vec<String>,
    current_line: &mut String,
    current_width: &mut LineWidth,
    element_spans: &mut Vec<ElementSpan>,
    attach: Attached<'_>,
    options: &ReflowOptions,
) -> Option<usize> {
    let length_mode = options.length_mode;
    let last_space = rfind_safe_space(current_line, element_spans, options, false)
        .or_else(|| rfind_safe_space(current_line, element_spans, options, true))?;
    let before = current_line[..last_space]
        .trim_end_matches(is_breakable_whitespace)
        .to_string();
    let after = current_line[last_space + 1..].to_string();
    let after_width = measure(&after, last_space + 1, element_spans, length_mode);
    lines.push(before);
    let carried = after.len();
    let Attached { text, width, separator } = attach;
    *current_line = format!("{after}{separator}{text}");
    *current_width = after_width + LineWidth::plain(display_len(separator, length_mode)) + width;
    rebase_spans_after_break(element_spans, last_space + 1);
    Some(carried)
}

/// Keep the spans that reach into the text starting at `carried_start` and move
/// them into that text's coordinates, discarding the ones that belong to the
/// line just emitted.
///
/// A span that starts before `carried_start` is clamped to 0 rather than
/// dropped. `rfind_safe_space` never breaks inside a span, so this cannot
/// normally happen; clamping keeps the whole prefix protected if it ever does,
/// where dropping the span would license a break inside an element.
fn rebase_spans_after_break(element_spans: &mut Vec<ElementSpan>, carried_start: usize) {
    element_spans.retain(|span| span.end > carried_start);
    for span in element_spans.iter_mut() {
        span.start = span.start.saturating_sub(carried_start);
        span.end -= carried_start;
    }
}

/// Reflow elements into lines that fit within the line length
fn reflow_elements(elements: &[Element], options: &ReflowOptions) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current_line = String::new();
    // The line's width under each exemption the checker applies. With no
    // exemption active both components are the plain display width.
    let mut current_width = LineWidth::default();
    // Track byte spans of non-Text elements in current_line for safe splitting
    let mut current_line_element_spans: Vec<ElementSpan> = Vec::new();
    let length_mode = options.length_mode;
    let exemptions = options.length_exemptions;

    for (idx, element) in elements.iter().enumerate() {
        let element_len = element.display_len(length_mode);
        let element_width = element.exempt_width(length_mode, exemptions);
        let is_hard = element_is_hard(element, options.break_link_text);

        // Determine adjacency from the original elements, not from current_line.
        // Elements are adjacent when there's no breakable whitespace between them
        // in the source (a non-breaking space stays inside the neighboring token,
        // so the pair must also stay attached):
        // - Text("v") → HugoShortcode("{{<...>}}") = adjacent (text has no trailing space)
        // - Text(" and ") → InlineLink("[a](url)") = NOT adjacent (text has trailing space)
        // - HugoShortcode("{{<...>}}") → Text(",") = adjacent (text has no leading space)
        // - Code("`x`") → Text("\u{00A0}:") = adjacent (only a non-breaking space between)
        let is_adjacent_to_prev = if idx > 0 {
            match (&elements[idx - 1], element) {
                (Element::Text(t), _) => !t.is_empty() && !t.ends_with(is_breakable_whitespace),
                (_, Element::Text(t)) => !t.is_empty() && !t.starts_with(is_breakable_whitespace),
                _ => true,
            }
        } else {
            false
        };

        // For text elements that might need breaking
        if let Element::Text(text) = element {
            // Check if original text had leading breakable whitespace
            let has_leading_space = text.starts_with(is_breakable_whitespace);
            // If this is a text element, always process it word by word
            let words: Vec<&str> = split_breakable_words(text).collect();

            for (i, word) in words.iter().enumerate() {
                // A bare word carries no construct the checker exempts.
                let word_width = LineWidth::plain(display_len(word, length_mode));
                // A token that is only punctuation (optionally led by a
                // non-breaking space, e.g. French "\u{00A0}:") must never be
                // hoisted to the start of a line. Tokens are never empty
                // (`split_breakable_words` filters), so `all` cannot be
                // vacuously true.
                let is_trailing_punct = word.chars().all(|c| {
                    matches!(c, ',' | '.' | ':' | ';' | '!' | '?' | ')' | ']' | '}') || is_non_breaking_space(c)
                });

                // First word of text adjacent to preceding non-text element
                // must stay attached (e.g., shortcode followed by punctuation or text)
                let is_first_adjacent = i == 0 && is_adjacent_to_prev;

                if is_first_adjacent {
                    // Attach directly without space, preventing line break
                    if !(current_width + word_width).fits(options.line_length)
                        && !current_width.is_empty()
                        && break_before_attached(
                            &mut lines,
                            &mut current_line,
                            &mut current_width,
                            &mut current_line_element_spans,
                            Attached {
                                text: word,
                                width: word_width,
                                separator: "",
                            },
                            options,
                        )
                        .is_some()
                    {
                        // Would exceed — broke before the adjacent group at the
                        // last safe space (element-aware, so links/code stay
                        // intact); with no safe break point the group is
                        // attached and the long line accepted.
                    } else {
                        current_line.push_str(word);
                        current_width += word_width;
                    }
                } else if !current_width.is_empty()
                    && !(current_width + LineWidth::plain(1) + word_width).fits(options.line_length)
                {
                    if is_trailing_punct {
                        // The overflowing token is bare punctuation, which must
                        // not start a line. Break one word earlier so the mark
                        // travels with the word it follows ("… mot :"), keeping
                        // the source space (French double punctuation requires
                        // it); with no safe earlier break point, accept the
                        // overlong line rather than rewrite content.
                        if break_before_attached(
                            &mut lines,
                            &mut current_line,
                            &mut current_width,
                            &mut current_line_element_spans,
                            Attached {
                                text: word,
                                width: word_width,
                                separator: " ",
                            },
                            options,
                        )
                        .is_none()
                        {
                            current_line.push(' ');
                            current_line.push_str(word);
                            current_width += LineWidth::plain(1) + word_width;
                        }
                    } else if !starts_block_construct(word) {
                        // Start a new line
                        lines.push(current_line.trim_matches(is_breakable_whitespace).to_string());
                        current_line = word.to_string();
                        current_width = word_width;
                        current_line_element_spans.clear();
                    } else if break_before_attached(
                        &mut lines,
                        &mut current_line,
                        &mut current_width,
                        &mut current_line_element_spans,
                        Attached {
                            text: word,
                            width: word_width,
                            separator: " ",
                        },
                        options,
                    )
                    .is_some()
                    {
                        // The overflowing word would open a block construct at line
                        // start. Broke one word earlier instead so the marker stays
                        // mid-line: "... and then" + "- clause" becomes "... and" +
                        // "then - clause".
                    } else {
                        // No safe earlier break point — keep the marker attached and
                        // accept the long line rather than corrupt the structure.
                        if i > 0 || has_leading_space {
                            current_line.push(' ');
                            current_width += LineWidth::plain(1);
                        }
                        current_line.push_str(word);
                        current_width += word_width;
                    }
                } else {
                    // Add a space wherever the source had breakable whitespace at
                    // this position. For the first word of a text run (i == 0)
                    // that means the run had a leading space — and reaching this
                    // branch already implies the word is not adjacent to the
                    // previous element, so the space is real. Later words
                    // (i > 0) always had whitespace before them: that is what
                    // separated them during tokenization. This holds for bare
                    // punctuation too ("ligne : la" keeps its French
                    // orthographic space): reflow moves line breaks, it does not
                    // rewrite characters. The no-space (adjacent) case is
                    // handled above by `is_first_adjacent`.
                    let add_space = !current_width.is_empty() && (i > 0 || has_leading_space);
                    if add_space {
                        current_line.push(' ');
                        current_width += LineWidth::plain(1);
                    }
                    current_line.push_str(word);
                    current_width += word_width;
                }
            }
        } else {
            let link_parts = link_text_parts(element, options.break_link_text);
            let span_info = match element {
                Element::Italic { content, underscore } => {
                    let marker = if *underscore { "_" } else { "*" };
                    Some((content.as_str(), marker, marker, false))
                }
                Element::Bold { content, underscore } => {
                    let marker = if *underscore { "__" } else { "**" };
                    Some((content.as_str(), marker, marker, false))
                }
                Element::Strikethrough { content, double } => {
                    let marker = if *double { "~~" } else { "~" };
                    Some((content.as_str(), marker, marker, false))
                }
                Element::Code { content, marker } => Some((content.as_str(), marker.as_str(), marker.as_str(), true)),
                _ => link_parts.map(|(prefix, inner, suffix)| (inner, prefix, suffix, false)),
            };
            let is_link = link_parts.is_some();

            // A span that alone exceeds the line budget is broken even when
            // spans are atomic, since keeping it whole would leave a line that can
            // never fit. `breakable_units` decides where that is safe. A link is
            // measured through its exemptions (a whole link may be forgiven where
            // a split one is not), and `link_text_break_units` additionally rules
            // out splits whose lines the checker would report.
            let breakable: Option<Vec<&str>> = match span_info {
                Some((content, _, suffix, is_code)) => {
                    if is_code {
                        (!options.atomic_spans && code_span_wraps_losslessly(content))
                            .then(|| split_breakable_words(content).collect())
                    } else if is_link {
                        (!options.atomic_spans || !element_width.fits(options.line_length))
                            .then(|| {
                                link_text_break_units(
                                    content,
                                    suffix,
                                    options.line_length,
                                    length_mode,
                                    options.defined_references.as_ref(),
                                    options.attr_lists,
                                )
                            })
                            .flatten()
                    } else {
                        (!options.atomic_spans || element_len > options.line_length)
                            .then(|| breakable_units(content, options.defined_references.as_ref(), options.attr_lists))
                            .flatten()
                    }
                }
                None => None,
            };

            if let Some(words) = breakable {
                let (_, prefix, suffix, is_code) = span_info.expect("breakable implies a span");
                let n = words.len();
                if n == 0 {
                    // Empty span — treat as atomic
                    let full = format!("{prefix}{suffix}");
                    let full_width = LineWidth::plain(display_len(&full, length_mode));
                    if !is_adjacent_to_prev && !current_width.is_empty() {
                        current_line.push(' ');
                        current_width += LineWidth::plain(1);
                    }
                    current_line.push_str(&full);
                    current_width += full_width;
                } else {
                    // A split link's tail earns no exemption from the checker
                    // (only an intact inline link does), so the suffix is
                    // measured at its plain width. The span is hard: a title or
                    // spaced destination inside it must never host a fallback
                    // break.
                    let suffix_span_width = LineWidth::plain(display_len(suffix, length_mode));

                    for (i, word) in words.iter().enumerate() {
                        let is_first = i == 0;
                        let is_last = i == n - 1;

                        let space_start = if is_first && is_code && word.starts_with('`') {
                            " "
                        } else {
                            ""
                        };
                        let space_end = if is_last && is_code && word.ends_with('`') {
                            " "
                        } else {
                            ""
                        };

                        let word_str: String = match (is_first, is_last) {
                            (true, true) => format!("{prefix}{space_start}{word}{space_end}{suffix}"),
                            (true, false) => format!("{prefix}{space_start}{word}"),
                            (false, true) => format!("{word}{space_end}{suffix}"),
                            (false, false) => word.to_string(),
                        };
                        let word_elements = parse_elements(&word_str, options);
                        let word_spans =
                            compute_element_spans(&word_elements, length_mode, exemptions, options.break_link_text);
                        let word_width = measure(&word_str, 0, &word_spans, length_mode);

                        let needs_space = if is_first {
                            !is_adjacent_to_prev && !current_width.is_empty()
                        } else {
                            !current_width.is_empty()
                        };

                        if needs_space
                            && !(current_width + LineWidth::plain(1) + word_width).fits(options.line_length)
                            && !starts_block_construct(&word_str)
                        {
                            lines.push(current_line.trim_matches(is_breakable_whitespace).to_string());
                            current_line = word_str;
                            current_width = word_width;
                            current_line_element_spans.clear();
                            for span in word_spans {
                                current_line_element_spans.push(span);
                            }
                            if is_link && is_last {
                                current_line_element_spans.push(ElementSpan::new(
                                    word.len(),
                                    suffix.len(),
                                    display_len(suffix, length_mode),
                                    suffix_span_width,
                                    true,
                                ));
                            }
                        } else {
                            let mut start_pos = current_line.len();
                            if needs_space {
                                current_line.push(' ');
                                current_width += LineWidth::plain(1);
                                start_pos += 1;
                            }
                            current_line.push_str(&word_str);
                            current_width += word_width;
                            for mut span in word_spans {
                                span.start += start_pos;
                                span.end += start_pos;
                                current_line_element_spans.push(span);
                            }
                            if is_link && is_last {
                                current_line_element_spans.push(ElementSpan::new(
                                    start_pos + word.len(),
                                    suffix.len(),
                                    display_len(suffix, length_mode),
                                    suffix_span_width,
                                    true,
                                ));
                            }
                        }
                    }
                }
            } else {
                // For non-text elements (code, links, references), treat as atomic units
                // These should never be broken across lines
                let element_str = format!("{element}");

                if is_adjacent_to_prev {
                    // Adjacent to preceding text — attach directly without space
                    if !(current_width + element_width).fits(options.line_length)
                        && let Some(carried) = break_before_attached(
                            &mut lines,
                            &mut current_line,
                            &mut current_width,
                            &mut current_line_element_spans,
                            Attached {
                                text: &element_str,
                                width: element_width,
                                separator: "",
                            },
                            options,
                        )
                    {
                        // Would exceed limit — broke before the adjacent word group
                        // at the last safe space (element-aware, so links/code stay
                        // intact). Record the element span in the new current_line.
                        current_line_element_spans.push(ElementSpan::new(
                            carried,
                            element_str.len(),
                            element_len,
                            element_width,
                            is_hard,
                        ));
                    } else {
                        let start = current_line.len();
                        current_line.push_str(&element_str);
                        current_width += element_width;
                        current_line_element_spans.push(ElementSpan::new(
                            start,
                            element_str.len(),
                            element_len,
                            element_width,
                            is_hard,
                        ));
                    }
                } else if !current_width.is_empty()
                    && !(current_width + LineWidth::plain(1) + element_width).fits(options.line_length)
                {
                    if !starts_block_construct(&element_str) {
                        // Not adjacent, would exceed — start new line
                        lines.push(current_line.trim_matches(is_breakable_whitespace).to_string());
                        current_line.clone_from(&element_str);
                        current_width = element_width;
                        current_line_element_spans.clear();
                        current_line_element_spans.push(ElementSpan::new(
                            0,
                            element_str.len(),
                            element_len,
                            element_width,
                            is_hard,
                        ));
                    } else if let Some(carried) = break_before_attached(
                        &mut lines,
                        &mut current_line,
                        &mut current_width,
                        &mut current_line_element_spans,
                        Attached {
                            text: &element_str,
                            width: element_width,
                            separator: " ",
                        },
                        options,
                    ) {
                        // The overflowing element would open a block construct at
                        // line start (e.g. an HtmlTag like `<div>`). Broke one word
                        // earlier instead so the element stays mid-line.
                        let start = carried + 1;
                        current_line_element_spans.push(ElementSpan::new(
                            start,
                            element_str.len(),
                            element_len,
                            element_width,
                            is_hard,
                        ));
                    } else {
                        // No safe earlier break point — keep the element attached
                        // and accept the long line rather than corrupt the structure.
                        let ends_with_opener =
                            current_line.ends_with('(') || current_line.ends_with('[') || current_line.ends_with('{');
                        if !ends_with_opener {
                            current_line.push(' ');
                            current_width += LineWidth::plain(1);
                        }
                        let start = current_line.len();
                        current_line.push_str(&element_str);
                        current_width += element_width;
                        current_line_element_spans.push(ElementSpan::new(
                            start,
                            element_str.len(),
                            element_len,
                            element_width,
                            is_hard,
                        ));
                    }
                } else {
                    // Not adjacent, fits — add with space
                    let ends_with_opener =
                        current_line.ends_with('(') || current_line.ends_with('[') || current_line.ends_with('{');
                    if !current_width.is_empty() && !ends_with_opener {
                        current_line.push(' ');
                        current_width += LineWidth::plain(1);
                    }
                    let start = current_line.len();
                    current_line.push_str(&element_str);
                    current_width += element_width;
                    current_line_element_spans.push(ElementSpan::new(
                        start,
                        element_str.len(),
                        element_len,
                        element_width,
                        is_hard,
                    ));
                }
            }
        }
    }

    // Don't forget the last line
    if !current_line.is_empty() {
        lines.push(current_line.trim_end_matches(is_breakable_whitespace).to_string());
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
        if is_single_line_paragraph && line_fits(line, options) {
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
/// Decomposes a raw inline code span string into its inner content and backtick marker.
///
/// For example, `decompose_code_span("`code`")` returns `Some(("code", "`"))`.
/// If the input is not a valid code span (e.g., it doesn't start and end with the
/// same number of backticks), returns `None`.
fn decompose_code_span(raw: &str) -> Option<(&str, &str)> {
    let marker_len = raw.bytes().take_while(|&b| b == b'`').count();
    if marker_len == 0 {
        return None;
    }
    let marker = &raw[..marker_len];
    if raw.len() < marker_len * 2 {
        return None;
    }
    let content = &raw[marker_len..raw.len() - marker_len];
    Some((content, marker))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `preserves_content` is the last line of defense against a reflow writing
    /// corrupted prose into a file, so it has to actually reject the ways a
    /// reflow can go wrong - not merely accept the ways it can go right.
    #[test]
    fn preserves_content_accepts_whitespace_changes_and_rejects_the_rest() {
        let accepted: &[(&str, &[&str])] = &[
            ("one two three", &["one two three"]),
            ("one two three", &["one two", "three"]),
            ("one two three", &["one", "two", "three"]),
            // Collapsing runs of whitespace and dropping trailing whitespace
            ("one   two  ", &["one two"]),
            // A script written without spaces has to break somewhere
            ("日本語のテキスト", &["日本語の", "テキスト"]),
            // Markers move to the line their content moved to
            ("_First. Second._", &["_First.", "Second._"]),
        ];
        for (original, reflowed) in accepted {
            let reflowed: Vec<String> = reflowed.iter().map(ToString::to_string).collect();
            assert!(
                preserves_content(original, &reflowed),
                "{original:?} -> {reflowed:?} only moves whitespace"
            );
        }

        let rejected: &[(&str, &[&str])] = &[
            // Dropped
            ("one two three", &["one two"]),
            // Invented
            ("one two", &["one two three"]),
            // Reordered
            ("one two", &["two one"]),
            // Duplicated
            ("_First. Second._", &["_First._", "_Second._"]),
            // Two words glued into one
            ("alpha and beta", &["alpha", "andbeta"]),
            // A space deleted around punctuation
            ("mot suivant : autre", &["mot suivant: autre"]),
        ];
        for (original, reflowed) in rejected {
            let reflowed: Vec<String> = reflowed.iter().map(ToString::to_string).collect();
            assert!(
                !preserves_content(original, &reflowed),
                "{original:?} -> {reflowed:?} changes the text, not just its line breaks"
            );
        }
    }

    /// A rejected reflow leaves the line alone rather than writing the damage.
    #[test]
    fn reflow_line_falls_back_to_the_input_when_content_would_change() {
        let options = ReflowOptions {
            line_length: 40,
            ..Default::default()
        };
        let line = "one two three four five six seven eight nine ten";

        assert!(preserves_content(line, &reflow_line(line, &options)));
        assert_eq!(reflow_line(line, &options), reflow_line_unchecked(line, &options));
    }

    #[test]
    fn cascade_split_line_handles_a_very_long_line_without_overflowing() {
        // A single line of thousands of words once drove `cascade_split_line`
        // into deep recursion (stack overflow / hang). The iterative version
        // must complete and split it into many lines that each fit the width and
        // that together preserve every word. The test finishing at all is the
        // core assertion (no stack overflow); the content checks guard behavior.
        let words: Vec<String> = (0..4000).map(|i| format!("word{i}")).collect();
        let line = words.join(" ");

        let options = ReflowOptions {
            line_length: 80,
            length_mode: ReflowLengthMode::Chars,
            ..Default::default()
        };
        let out = cascade_split_line(&line, &options);

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
        let sentences = split_into_sentences(text, None);
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
        let sentences = split_into_sentences(text, None);
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
        let sentences = split_into_sentences(text, None);
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
        let sentences = split_into_sentences(text, None);
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
        let sentences = split_into_sentences(text, None);
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
        let sentences = split_into_sentences(text, None);
        assert_eq!(sentences, vec![text.to_string()]);
    }

    #[test]
    fn test_footnote_at_end_of_text_is_preserved() {
        // A footnote reference at the very end of the text has nothing after it
        // to split off; it is preserved as part of the single trailing sentence.
        let text = "Sentence.[^1]";
        let sentences = split_into_sentences(text, None);
        assert_eq!(sentences, vec![text.to_string()]);
    }

    #[test]
    fn test_abbreviation_before_footnote_does_not_split() {
        // The existing abbreviation guard must still apply when a footnote
        // reference immediately follows the abbreviation's period.
        let text = "See the notes, e.g.[^1] this one.";
        let sentences = split_into_sentences(text, None);
        assert_eq!(
            sentences,
            vec![text.to_string()],
            "e.g. is an abbreviation, not a sentence boundary"
        );
    }

    #[test]
    fn sentence_boundary_never_falls_inside_an_atomic_construct() {
        // Each construct holds text that reads like a sentence boundary
        // (`. ` followed by a capital) but is one unit to the renderer: a
        // break inside it rewrites the document. The boundary after each
        // construct is real and must still split, so a construct that simply
        // silenced the splitter would fail here too.
        let cases = [
            "Prefix [link. Still link](https://example.com) tail. Next sentence.",
            "Prefix [target](<https://example.com/First. Second>) tail. Next sentence.",
            "Prefix [text](url \"Title. More\") tail. Next sentence.",
            "Prefix ![alt. Alt](img.png) tail. Next sentence.",
            "Prefix [ref text. More][ref] tail. Next sentence.",
            "Prefix [collapsed. More][] tail. Next sentence.",
            "Prefix [[Page name. Title]] tail. Next sentence.",
            "Prefix $x. Y$ tail. Next sentence.",
            "Prefix $$x. Y$$ tail. Next sentence.",
            "Prefix <span title=\"A. B\">x</span> tail. Next sentence.",
            "Prefix `code. Still code` tail. Next sentence.",
        ];
        for text in cases {
            let sentences = split_into_sentences(text, None);
            let (head, tail) = text.rsplit_once(" tail. ").expect("case has a tail");
            assert_eq!(
                sentences,
                vec![format!("{head} tail."), tail.to_string()],
                "input {text:?}"
            );
        }

        // A bare `[text]` is a link only when its label is defined. Defined,
        // or with the definitions unknown, it is held whole like the rest;
        // known undefined, it is prose and the boundary inside it is real.
        let text = "Prefix [shortcut. More] tail. Next sentence.";
        let whole = vec![
            "Prefix [shortcut. More] tail.".to_string(),
            "Next sentence.".to_string(),
        ];
        let defined = HashSet::from(["shortcut. more".to_string()]);
        assert_eq!(split_into_sentences(text, Some(&defined)), whole);
        assert_eq!(split_into_sentences(text, None), whole);
        assert_eq!(
            split_into_sentences(text, Some(&HashSet::new())),
            vec!["Prefix [shortcut.", "More] tail.", "Next sentence."]
        );
    }

    #[test]
    fn a_sentence_may_open_with_a_link_or_image() {
        // The next sentence's first letter sits behind the link opener; a
        // capital there is a capital start. The reflow already emits the text
        // before the link as its own line, so the check has to count the same
        // boundary or it never asks for that split.
        for text in [
            "Opening sentence. [First. Second](https://example.com)",
            "Opening sentence. ![First. Second](img.png)",
            "Opening sentence. [[First. Second]]",
            "Opening sentence. [[first-note|First. Second]]",
            "Opening sentence. [Ref link][ref]",
            // A link whose text is an image opens with the image's alt text,
            // one construct inside another; each is walked into at its own start.
            "Opening sentence. [![First image](img.png)](url) continues.",
            "Opening sentence. [![First image](img.png)][ref] continues.",
            // The nested image may be a reference image; its full and
            // collapsed forms are images whether or not the label is defined.
            "Opening sentence. [![First image][img]](url) continues.",
            "Opening sentence. [![First image][]](url) continues.",
            "Opening sentence. [![First image][img]][ref] continues.",
        ] {
            let (head, tail) = text.split_once(". ").expect("case has a boundary");
            assert_eq!(
                split_into_sentences(text, None),
                vec![format!("{head}."), tail.to_string()],
                "input {text:?}"
            );
        }
        // A shortcut reference image nested in a link is an image only when
        // its label is defined, exactly as at the top level.
        let text = "Opening sentence. [![First image]](url) continues.";
        let defined = HashSet::from(["first image".to_string()]);
        assert_eq!(
            split_into_sentences(text, Some(&defined)),
            vec!["Opening sentence.", "[![First image]](url) continues."]
        );
        assert_eq!(
            split_into_sentences(text, Some(&HashSet::new())),
            vec![text.to_string()],
            "an undefined shortcut is bracketed text, and `!` opens no sentence"
        );
        // The nested image's alt text is what has to be capitalized: the same
        // link with a lowercase alt opens no sentence.
        assert_eq!(
            split_into_sentences("Opening sentence. [![first image](img.png)](url) continues.", None),
            vec!["Opening sentence. [![first image](img.png)](url) continues."]
        );
        // A bare `[text]` whose label is defined is a link too, and its text
        // opens the sentence the same way.
        let defined = HashSet::from(["smith 2020".to_string()]);
        assert_eq!(
            split_into_sentences("Claim ends here. [Smith 2020] more text.", Some(&defined)),
            vec!["Claim ends here.", "[Smith 2020] more text."]
        );
        // Controls: a lowercase link text is no sentence start, and a bracket
        // the parse reads as text is not a link opener, so a citation, an
        // undefined shortcut, a footnote label or a link left unterminated
        // starts no sentence however it is capitalized. The definitions are
        // known and empty here, as the rule always supplies them.
        let none_defined = HashSet::new();
        for text in [
            "Opening sentence. [first link](https://example.com) continues.",
            "Opening sentence. [[first note]] continues.",
            "Opening sentence. [[First Note|first note]] continues.",
            "Opening sentence. [[Page continues.",
            "Opening sentence. [[First] stray]] continues.",
            "Opening sentence. ![first alt](img.png) continues.",
            "Opening sentence. [1] is the citation.",
            "Opening sentence. [First](unterminated",
            "Opening sentence. [First][unterminated",
            "Opening sentence. [First] (aside) continues.",
            "Claim ends here. [Smith 2020]",
            "Claim ends here. [Smith 2020] more text.",
            "See the RFC. [RFC] More text.",
            "Claim ends here. [^Note] more text.",
        ] {
            assert_eq!(
                split_into_sentences(text, Some(&none_defined)),
                vec![text.to_string()],
                "input {text:?}"
            );
        }
    }

    #[test]
    fn link_opener_is_read_off_the_parse() {
        // Length of the opener at the start of `text`, or 0 when the parse
        // (with the given definitions) finds no link, image or wikilink there.
        let len = |text: &str, defs: Option<&HashSet<String>>| {
            let chars: Vec<char> = text.chars().collect();
            let char_offsets = char_byte_offsets(&chars);
            let NestedStructure { links, .. } = sentence_structure(text, defs);
            let st = SentenceText {
                text,
                chars: &chars,
                char_offsets: &char_offsets,
                links: &links,
                code_spans: &[],
            };
            st.link_end_at(0).map_or(0, |end| link_opener_len(&chars, 0, end))
        };
        let none = HashSet::new();
        assert_eq!(len("[text](url)", Some(&none)), 1);
        assert_eq!(
            len("[text][ref]", Some(&none)),
            1,
            "a full reference is a link whether or not defined"
        );
        assert_eq!(len("[text][]", Some(&none)), 1);
        assert_eq!(len("![alt](img.png)", Some(&none)), 2);
        assert_eq!(len("[[wiki]]", Some(&none)), 2);
        assert_eq!(
            len("[[wiki|shown]]", Some(&none)),
            7,
            "the displayed text starts after the alias pipe"
        );
        assert_eq!(len("![[img.png|100]]", Some(&none)), 11);
        assert_eq!(len("[[wiki|a|b]]", Some(&none)), 7, "the first pipe starts the alias");
        assert_eq!(
            len("[[wiki|shown]] [[a|b]]", Some(&none)),
            7,
            "a pipe past the closing `]]` is not this alias"
        );
        assert_eq!(
            len("[a \\] b](url)", Some(&none)),
            1,
            "an escaped bracket does not close the text"
        );
        assert_eq!(
            len("[![alt](img)](url)", Some(&none)),
            1,
            "the outer opener is skipped first"
        );
        // Text to the parse, so no opener: unterminated links, an unclosed or
        // malformed wikilink, a shortcut nothing defines, and a footnote
        // reference, which the paragraph-level parse has no definition for.
        for text in [
            "[^1]",
            "[text](unterminated",
            "[text][unterminated",
            "[text] (url)",
            "[[wiki",
            "[[wiki]",
            "[[First] stray]]",
            "[Smith 2020]",
            "[Smith 2020] (see also)",
            "[unclosed",
            "!bang",
            "text",
        ] {
            assert_eq!(len(text, Some(&none)), 0, "input {text:?}");
        }
        // The same shortcut is a link once its label is defined, or when the
        // definitions are unknown.
        let smith = HashSet::from(["smith 2020".to_string()]);
        assert_eq!(len("[Smith 2020]", Some(&smith)), 1);
        assert_eq!(len("[Smith 2020]", None), 1);
    }

    #[test]
    fn sentence_per_line_reflow_breaks_before_a_bracket_only_where_the_check_counts() {
        // The check counts a boundary before a link, image or wikilink whose
        // text starts a sentence and none before a lowercase one, a bare
        // citation or a glued link. The reflow has to break at exactly those
        // boundaries: a break the check never counts is a fix that keeps
        // reporting, and a boundary the reflow ignores is a line it never
        // splits. The definitions are known, as the rule always supplies
        // them: `[RFC]` alone is a citation, `[Spec]` a defined shortcut link.
        let defined = HashSet::from(["spec".to_string()]);
        let options = ReflowOptions {
            line_length: 120,
            sentence_per_line: true,
            defined_references: Some(defined.clone()),
            ..Default::default()
        };
        for (text, expected) in [
            (
                "Claim ends here. [Smith](https://example.com) more text. Second sentence.",
                vec![
                    "Claim ends here.",
                    "[Smith](https://example.com) more text.",
                    "Second sentence.",
                ],
            ),
            (
                "Wow! [smith](https://example.com) more text. Second sentence.",
                vec!["Wow!", "[smith](https://example.com) more text.", "Second sentence."],
            ),
            (
                "Claim ends here. [smith](https://example.com) more text. Second sentence.",
                vec![
                    "Claim ends here. [smith](https://example.com) more text.",
                    "Second sentence.",
                ],
            ),
            (
                "Claim ends here. [smith][ref] more text. Second sentence.",
                vec!["Claim ends here. [smith][ref] more text.", "Second sentence."],
            ),
            (
                "Claim ends here. ![alt](img.png) more text. Second sentence.",
                vec!["Claim ends here. ![alt](img.png) more text.", "Second sentence."],
            ),
            (
                "Claim ends here.[Link](https://example.com) more text. Second sentence.",
                vec![
                    "Claim ends here.[Link](https://example.com) more text.",
                    "Second sentence.",
                ],
            ),
            (
                "See the RFC. [RFC] More text. Second sentence.",
                vec!["See the RFC. [RFC] More text.", "Second sentence."],
            ),
            (
                "See the spec. [Spec] More text. Second sentence.",
                vec!["See the spec.", "[Spec] More text.", "Second sentence."],
            ),
            (
                "See the spec. [spec] more text. Second sentence.",
                vec!["See the spec. [spec] more text.", "Second sentence."],
            ),
            (
                "Claim ends here. [[page|Second sentence]] continues. Third sentence.",
                vec![
                    "Claim ends here.",
                    "[[page|Second sentence]] continues.",
                    "Third sentence.",
                ],
            ),
            (
                "Claim ends here. [[Page|second sentence]] continues. Third sentence.",
                vec![
                    "Claim ends here. [[Page|second sentence]] continues.",
                    "Third sentence.",
                ],
            ),
        ] {
            let lines = reflow_line(text, &options);
            assert_eq!(lines, expected, "input {text:?}");
            // The check counts the same number of sentences on the input as
            // the reflow produced lines, and one on each line it produced.
            assert_eq!(
                split_into_sentences(text, Some(&defined)).len(),
                expected.len(),
                "check count for {text:?}"
            );
            for line in &lines {
                assert_eq!(
                    split_into_sentences(line, Some(&defined)).len(),
                    1,
                    "line {line:?} of {text:?}"
                );
            }
        }
    }

    #[test]
    fn sentence_per_line_reflow_holds_atomic_constructs_whole() {
        // The reflow assembles a line one element at a time and re-splits the
        // whole line after each text element, so an atomic element already on
        // the line is exposed to the splitter along with the text after it.
        let options = ReflowOptions {
            line_length: 80,
            sentence_per_line: true,
            ..Default::default()
        };
        let lines = reflow_line(
            "Prefix `code. Still code` and [link. Still link](https://example.com) tail. Next sentence.",
            &options,
        );
        assert_eq!(
            lines,
            vec![
                "Prefix `code. Still code` and [link. Still link](https://example.com) tail.".to_string(),
                "Next sentence.".to_string(),
            ]
        );

        let lines = reflow_line(
            "Prefix ![alt. Alt](img.png) and [target](<https://example.com/First. Second>) tail. Next sentence.",
            &options,
        );
        assert_eq!(
            lines,
            vec![
                "Prefix ![alt. Alt](img.png) and [target](<https://example.com/First. Second>) tail.".to_string(),
                "Next sentence.".to_string(),
            ]
        );

        // Control: a link that carries no boundary of its own leaves the
        // surrounding boundaries exactly where they were.
        let lines = reflow_line("First one. Then [link](url) second. Third one.", &options);
        assert_eq!(
            lines,
            vec![
                "First one.".to_string(),
                "Then [link](url) second.".to_string(),
                "Third one.".to_string(),
            ]
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
        // Ordered list markers: only a list numbered 1 with a non-empty first
        // item interrupts a paragraph. Leading zeros keep the number 1.
        for case in ["1. item", "1) item", "01. x", "000000001. x", "1.\titem"] {
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
            "0000000001. ten digits is not a list marker either",
            // A number other than 1 cannot interrupt a paragraph, nor can an
            // empty first item, so neither changes the parse at line start.
            "2. item",
            "7. item",
            "0. item",
            "42) x",
            "123456. item",
            "1.",
            "1)",
            "123456.",
            "123456)",
            "1.item",
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

        // Folding cascades: `1.` alone is inert, but absorbing `[ref]:` makes
        // it a list item, so the grown line has to fold back in turn.
        let lines = vec!["prose".to_string(), "1.".to_string(), "[ref]:".to_string()];
        assert_eq!(
            merge_block_construct_continuations(lines),
            vec!["prose 1. [ref]:".to_string()],
            "a merge that creates an opener must fold again"
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

    /// Sentence-per-line reflow of `input` under `require-sentence-capital`.
    fn strict_sentence_lines(input: &str, require_sentence_capital: bool) -> Vec<String> {
        let options = ReflowOptions {
            line_length: 80,
            sentence_per_line: true,
            require_sentence_capital,
            ..Default::default()
        };
        reflow_line(input, &options)
    }

    #[test]
    fn strict_mode_lets_a_sentence_open_with_a_number() {
        // `require-sentence-capital` exists to keep `word. lowercase`
        // continuations together; a digit is not a lowercase letter, so a
        // sentence may open with a count, a year, an ordinal or a time.
        for (input, expected) in [
            (
                "The number of items was 5. 2 of them failed.",
                vec!["The number of items was 5.", "2 of them failed."],
            ),
            (
                "Sometimes we have 2. 3 might be here.",
                vec!["Sometimes we have 2.", "3 might be here."],
            ),
            (
                "The number of items was 5. 2nd sentence.",
                vec!["The number of items was 5.", "2nd sentence."],
            ),
            (
                "Released in 2020. 3 of them failed.",
                vec!["Released in 2020.", "3 of them failed."],
            ),
            (
                "First sentence. 2nd sentence.",
                vec!["First sentence.", "2nd sentence."],
            ),
            (
                "We met at 6:00 sharp. 6:00 is early.",
                vec!["We met at 6:00 sharp.", "6:00 is early."],
            ),
            ("Pi is 3.14 roughly. Next.", vec!["Pi is 3.14 roughly.", "Next."]),
            // A `?` inside a quotation follows the same rule as a period, so a
            // digit after the closing quote opens a sentence there too.
            (
                "A \"Is this a test?\" 2020 was memorable.",
                vec!["A \"Is this a test?\"", "2020 was memorable."],
            ),
        ] {
            assert_eq!(strict_sentence_lines(input, true), expected, "input {input:?}");
        }

        // A lowercase continuation still holds the sentence open, and an
        // abbreviation before a number is still an abbreviation.
        for input in [
            "The count was 5. and that was all.",
            "See fig. 3 for details.",
            "See no. 5 in the list.",
            "See ch. 12 and vol. 3 for more.",
            "A \"Is this a test?\" guide to it.",
        ] {
            assert_eq!(
                strict_sentence_lines(input, true),
                vec![input.to_string()],
                "input {input:?}"
            );
        }
    }

    #[test]
    fn sentence_never_opens_with_an_ordered_list_marker() {
        // An inline enumerator keeps its place after the sentence before it,
        // in either mode. Every line the splitter produces ends a sentence,
        // and `2. Do that.` under such a line is a list item to MD032 (in any
        // document that has a list) and to CommonMark for `1.`, so the
        // enumerator is never hoisted to line start; the enumerated text opens
        // the next line instead.
        for (input, require_capital, expected) in [
            (
                "Steps: 1. Do this. 2. Do that.",
                true,
                vec!["Steps: 1.", "Do this. 2.", "Do that."],
            ),
            (
                "First sentence. 1. Do that.",
                true,
                vec!["First sentence. 1.", "Do that."],
            ),
            ("Do this! 2. Do that.", true, vec!["Do this! 2.", "Do that."]),
            ("Do this. 12) Do that.", true, vec!["Do this. 12) Do that."]),
            ("Do this. 2. do that.", true, vec!["Do this. 2. do that."]),
            ("Do this. 2. do that.", false, vec!["Do this. 2.", "do that."]),
            (
                "Twelve. 1234567890. next one here.",
                true,
                vec!["Twelve. 1234567890. next one here."],
            ),
            // A number that is not followed by a marker's `.`/`)` and space
            // opens a sentence as usual.
            ("Do this. 2 more times.", true, vec!["Do this.", "2 more times."]),
            ("How many? 2.", true, vec!["How many?", "2."]),
            // CJK punctuation needs no space before the next sentence, and the
            // marker rule holds after it as well.
            ("第一句。2. Do that.", true, vec!["第一句。2.", "Do that."]),
            ("第一句。 2) 第二句。", true, vec!["第一句。 2) 第二句。"]),
            ("第一句。2 more.", true, vec!["第一句。", "2 more."]),
            ("第一句。第二句。", true, vec!["第一句。", "第二句。"]),
        ] {
            let lines = strict_sentence_lines(input, require_capital);
            assert_eq!(lines, expected, "input {input:?}, require capital {require_capital}");
            for line in &lines {
                let chars: Vec<char> = line.chars().collect();
                assert!(
                    !opens_ordered_list_marker(&chars),
                    "line opens with an ordered-list marker: {line:?} (input {input:?})"
                );
            }
        }
    }

    #[test]
    fn opens_ordered_list_marker_matches_the_marker_shape() {
        let chars = |s: &str| s.chars().collect::<Vec<char>>();
        for text in ["2. x", "1) x", "12. x", "1.\tx", "1234567890. x", "0. x"] {
            assert!(opens_ordered_list_marker(&chars(text)), "{text:?} is a marker");
        }
        for text in ["2.x", "2.", "2)", "2 x", "x. y", "", " 2. x", "2.5 x", "-2. x"] {
            assert!(!opens_ordered_list_marker(&chars(text)), "{text:?} is not a marker");
        }
    }

    #[test]
    fn inline_math_directly_after_display_math_stays_atomic() {
        // The inline-math regex's lookbehind `(?<!\$)` is slice-start-sensitive:
        // a search anchored at the cursor accepts a `$` whose real predecessor
        // is a `$` (the lookbehind sees nothing before the slice), while a
        // cached search anchored earlier sees the `$` and rejects it. After
        // display math consumes `$$a$$`, the cursor sits directly after a `$`;
        // the match cache must re-search there or `$bb cc dd$` degrades to
        // plain text and gets wrapped apart, breaking math rendering.
        let options = ReflowOptions {
            line_length: 8,
            ..Default::default()
        };
        let lines = reflow_line("$$a$$$bb cc dd$ x", &options);
        assert_eq!(lines, vec!["$$a$$$bb cc dd$".to_string(), "x".to_string()]);
    }

    #[test]
    fn test_code_span_parsing() {
        // 1. Single backtick
        let elements = parse_markdown_elements_inner("`code`", false, false, None);
        assert_eq!(elements.len(), 1);
        assert!(matches!(&elements[0], Element::Code { content, marker } if content == "code" && marker == "`"));

        // 2. Double backtick
        let elements = parse_markdown_elements_inner("``code``", false, false, None);
        assert_eq!(elements.len(), 1);
        assert!(matches!(&elements[0], Element::Code { content, marker } if content == "code" && marker == "``"));

        // 3. Double backtick with single backtick inside
        let elements = parse_markdown_elements_inner("``code`inside``", false, false, None);
        assert_eq!(elements.len(), 1);
        assert!(
            matches!(&elements[0], Element::Code { content, marker } if content == "code`inside" && marker == "``")
        );

        // 4. Spaces inside
        let elements = parse_markdown_elements_inner("`` code ``", false, false, None);
        assert_eq!(elements.len(), 1);
        assert!(matches!(&elements[0], Element::Code { content, marker } if content == " code " && marker == "``"));

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

    #[test]
    fn test_reflow_performance_display_math_heavy() {
        // Every consumed `$$a$$` leaves the cursor directly after a `$`. The
        // inline-math slice-start probe must run in place at the cursor; a
        // suffix rescan there makes this input quadratic (~9s in a debug
        // build for these 4000 spans).
        let text = "$$a$$".repeat(4000);

        let start = std::time::Instant::now();
        let elements = parse_markdown_elements_inner(&text, false, false, None);
        let duration = start.elapsed();

        assert!(duration.as_millis() < 100, "Parsing took too long: {duration:?}");
        assert_eq!(elements.len(), 4000);
    }

    #[test]
    fn inline_math_len_at_start_matches_regex_at_slice_start() {
        // Exhaustive parity with INLINE_MATH_REGEX over short `$`-soup
        // strings: the helper must equal "regex match starting at position 0"
        // exactly, since the regex's leading lookbehind is vacuous at a slice
        // start. Any drift silently changes which math spans stay atomic.
        let alphabet = ['$', 'a', ' '];
        let mut inputs: Vec<String> = vec![String::new()];
        let mut frontier: Vec<String> = vec![String::new()];
        for _ in 0..6 {
            let mut longer = Vec::new();
            for prefix in &frontier {
                for ch in alphabet {
                    let mut s = prefix.clone();
                    s.push(ch);
                    longer.push(s);
                }
            }
            inputs.extend(longer.iter().cloned());
            frontier = longer;
        }
        // Multi-byte content must count bytes, not characters.
        inputs.push("$αβ$x".to_string());
        inputs.push("$α$$".to_string());

        for s in &inputs {
            let expected = INLINE_MATH_REGEX
                .find(s)
                .ok()
                .flatten()
                .filter(|m| m.start() == 0)
                .map(|m| m.end());
            assert_eq!(inline_math_len_at_start(s), expected, "input: {s:?}");
        }
    }

    #[test]
    fn inline_math_probe_after_dollar_matches_uncached_parse() {
        // Expected element lists verified against the uncached parser (the
        // parent of the match-cache commit): when a consumed span leaves the
        // cursor directly after a `$`, the at-cursor probe must reproduce
        // exactly what rescanning the suffix used to find - both the hits
        // (the lookbehind is vacuous at the cursor) and the misses.
        let cases = [
            ("$$a$$$b c$ x", r#"[DisplayMath("a"), InlineMath("b c"), Text(" x")]"#),
            (
                "$$a$$$b$ $$a$$$b$",
                r#"[DisplayMath("a"), InlineMath("b"), Text(" "), DisplayMath("a"), InlineMath("b")]"#,
            ),
            // Probe hit whose content is only whitespace.
            (
                "$$a$$$ x $y z$",
                r#"[DisplayMath("a"), InlineMath(" x "), Text("y z$")]"#,
            ),
            // Probe miss: `$$` after the cursor is not inline math.
            ("$$a$$$$ x", r#"[DisplayMath("a"), Text("$$ x")]"#),
            ("$$a$$$$b$$ x", r#"[DisplayMath("a"), DisplayMath("b"), Text(" x")]"#),
            // Probe miss: the trailing lookahead rejects `$c$$`.
            (
                "$a$$b$$c$$d$ tail",
                r#"[Text("$a"), DisplayMath("b"), Text("c$$d$ tail")]"#,
            ),
        ];
        for (input, expected) in cases {
            let elements = parse_markdown_elements_inner(input, false, false, None);
            assert_eq!(format!("{elements:?}"), expected, "input: {input:?}");
        }
    }

    #[test]
    fn test_atomic_spans() {
        // --- Emphasis Spans ---
        let text_emphasis = "hello **word1 word2**";

        let options_disabled = ReflowOptions {
            line_length: 18,
            atomic_spans: true,
            ..Default::default()
        };
        let lines_disabled = reflow_line(text_emphasis, &options_disabled);
        assert_eq!(lines_disabled, vec!["hello", "**word1 word2**"]);

        let options_enabled = ReflowOptions {
            line_length: 18,
            atomic_spans: false,
            ..Default::default()
        };
        let lines_enabled = reflow_line(text_emphasis, &options_enabled);
        assert_eq!(lines_enabled, vec!["hello **word1", "word2**"]);

        // --- Code Spans ---
        let text_code = "hello `word1 word2`";

        let lines_code_disabled = reflow_line(text_code, &options_disabled);
        assert_eq!(lines_code_disabled, vec!["hello", "`word1 word2`"]);

        let lines_code_enabled = reflow_line(text_code, &options_enabled);
        assert_eq!(lines_code_enabled, vec!["hello `word1", "word2`"]);

        // Test multiple backticks with space padding
        let text_code_padding = "hello `` `word1` `word2` ``";
        let lines_padding_enabled = reflow_line(text_code_padding, &options_enabled);
        assert_eq!(lines_padding_enabled, vec![r#"hello `` `word1`"#, r#"`word2` ``"#]);

        // Test atomic span wrapping with attached punctuation (maintainer feedback)
        let text_attached = "**one two**,"; // length 12, bold span is 11

        // With limit 11, the bold span (11) fits, so it should NOT be split even though the total (12) exceeds 11.
        let options_11 = ReflowOptions {
            line_length: 11,
            atomic_spans: true,
            ..Default::default()
        };
        assert_eq!(reflow_line(text_attached, &options_11), vec!["**one two**,"]);

        // With limit 10, the bold span (11) exceeds 10, so it is allowed to be split.
        let options_10 = ReflowOptions {
            line_length: 10,
            atomic_spans: true,
            ..Default::default()
        };
        assert_eq!(reflow_line(text_attached, &options_10), vec!["**one", "two**,"]);
    }

    #[test]
    fn test_emphasis_containing_markers_is_not_split() {
        let options = ReflowOptions {
            line_length: 5,
            atomic_spans: false,
            ..Default::default()
        };
        // Emphasis containing internal markers (e.g. escaped asterisks) should not be split to avoid formatting corruption
        let lines = reflow_line(r#"*foo \*bar*"#, &options);
        assert_eq!(lines, vec![r#"*foo \*bar*"#.to_string()]);
    }

    /// The parsed shape of a markdown fragment, normalized the way wrapping is
    /// allowed to change it and no further: block/inline structure and
    /// code-span contents are compared exactly, while prose whitespace is
    /// collapsed, because a wrap only ever swaps a space for a newline.
    fn semantic_shape(markdown: &str) -> String {
        let mut options = Options::empty();
        options.insert(Options::ENABLE_STRIKETHROUGH);
        let mut out = String::new();
        let push_prose = |out: &mut String, text: &str| {
            for c in text.chars() {
                if c.is_whitespace() {
                    if !out.ends_with(char::is_whitespace) {
                        out.push(' ');
                    }
                } else {
                    out.push(c);
                }
            }
        };
        for event in Parser::new_ext(markdown, options) {
            match event {
                Event::Text(text) => push_prose(&mut out, &text),
                Event::SoftBreak | Event::HardBreak => push_prose(&mut out, " "),
                // Interior whitespace in a code span is literal: compare verbatim.
                Event::Code(code) => out.push_str(&format!("<code>{code}</code>")),
                Event::Start(tag) => out.push_str(&format!("<{tag:?}>")),
                Event::End(tag) => out.push_str(&format!("</{tag:?}>")),
                other => out.push_str(&format!("{other:?}")),
            }
        }
        out.trim().to_string()
    }

    #[test]
    fn test_wrapping_a_span_never_changes_what_it_parses_to() {
        // Breaking a span is only safe if the document still parses the same.
        // Cover both settings and several budgets so the break lands in a
        // different place in each run.
        let corpus = [
            "_This is a very, very, very, very, very long line with some `code` inside._",
            "_alpha beta gamma delta epsilon `a  b` zeta eta theta iota kappa lambda_",
            "**strong text with `code` and more words than fit on one single line**",
            "~~struck text with `code` and more words than fit on one single line~~",
            "_emphasis with **nested strong that is quite long** and trailing words_",
            // Doubly nested spans: the whole content of the outer span is one
            // nested span, so there is no prose outside it to break at.
            "***A doubly nested bold italic span with more words than fit on a line***",
            "___Another doubly nested span with more words than fit on a single line___",
            "**_mixed strong then emphasis with more words than fit on a single line_**",
            "*__mixed emphasis then strong with more words than fit on a single line__*",
            "**~~strong strikethrough with more words than fit on a single line here~~**",
            // A marker that belongs to no well-formed span. Breaking at these
            // spaces would start a line with `* `, making it a list item.
            "**a * b with a stray marker and plenty more words to pass the budget**",
            "_foo `a` bar `b` baz qux quux corge grault garply waldo fred plugh xyzzy_",
            "text before _a long emphasis with `code` inside of it here_ and after",
            "(_a parenthesized long emphasis with `code` inside of it right here_)",
            r#"*foo \*bar baz qux quux corge grault garply waldo fred plugh xyzzy*"#,
            "_tab\tseparated `a\tb` words spread out over quite a long emphasis span_",
            // A link nested in the span: its destination and title are not prose
            // and cannot absorb a line break.
            "_This has [text](<a b c d e f g h i j k>) and trailing words to wrap._",
            "_This has [`code`](<a b c d e f g h i j k>) and trailing words to wrap._",
            r#"_See [x](https://example.com "a long link title here") and `code` too._"#,
            "_A [link with a long label](https://example.com/path) and `code` here._",
            "_An image ![alt text here](<a b c d e f g h>) plus `code` and more text_",
        ];
        for text in corpus {
            let expected = semantic_shape(text);
            for line_length in [20, 30, 40, 80] {
                for atomic_spans in [true, false] {
                    let options = ReflowOptions {
                        line_length,
                        atomic_spans,
                        ..Default::default()
                    };
                    let wrapped = reflow_line(text, &options).join("\n");
                    assert_eq!(
                        semantic_shape(&wrapped),
                        expected,
                        "reflow changed the parse of {text:?} at line_length={line_length} \
                         atomic_spans={atomic_spans}\n  wrapped: {wrapped:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn test_wrapping_a_span_keeps_whole_the_constructs_pulldown_cannot_see() {
        // Wiki links, Hugo shortcodes and math are atomic elements at the top
        // level but are invisible to the CommonMark parser, so `semantic_shape`
        // cannot catch a break inside one. Assert directly that they survive.
        let cases = [
            (
                "_alpha beta gamma [[a wiki link]] delta epsilon zeta eta_",
                "[[a wiki link]]",
            ),
            (
                "_alpha beta gamma {{< foo bar >}} delta epsilon zeta eta_",
                "{{< foo bar >}}",
            ),
            ("_alpha beta gamma $a + b$ delta epsilon zeta eta theta_", "$a + b$"),
            ("_alpha beta gamma $$a + b$$ delta epsilon zeta eta theta_", "$$a + b$$"),
        ];
        for (text, construct) in cases {
            for line_length in [12, 20, 30] {
                for atomic_spans in [true, false] {
                    let options = ReflowOptions {
                        line_length,
                        atomic_spans,
                        ..Default::default()
                    };
                    let wrapped = reflow_line(text, &options).join("\n");
                    assert!(
                        wrapped.contains(construct),
                        "{construct} was broken at line_length={line_length} \
                         atomic_spans={atomic_spans}: {wrapped:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn test_overlong_emphasis_with_nested_code_span_wraps() {
        // An emphasis span longer than the whole line budget must still wrap,
        // even when it contains a nested code span: keeping it atomic would
        // leave a line that can never fit.
        let options = ReflowOptions {
            line_length: 80,
            atomic_spans: true,
            ..Default::default()
        };
        let text = "_This is a very, very, very, very, very, very, very long line that exceeds 80 characters with some `code` inside._";
        let lines = reflow_line(text, &options);
        assert_eq!(
            lines,
            vec![
                "_This is a very, very, very, very, very, very, very long line that exceeds 80",
                "characters with some `code` inside._",
            ]
        );
    }

    #[test]
    fn test_overlong_emphasis_with_nested_strong_wraps() {
        // Same for a nested strong span. The nested span itself stays whole.
        let options = ReflowOptions {
            line_length: 80,
            atomic_spans: true,
            ..Default::default()
        };
        let text = "_This is a very, very, very, very, very, very, very long line that exceeds 80 characters with some **bold** inside._";
        let lines = reflow_line(text, &options);
        assert_eq!(
            lines,
            vec![
                "_This is a very, very, very, very, very, very, very long line that exceeds 80",
                "characters with some **bold** inside._",
            ]
        );
    }

    #[test]
    fn test_overlong_doubly_nested_span_wraps() {
        // The whole content of the outer span is a single nested emphasis span.
        // Holding a nested span whole regardless of length left no break point
        // anywhere inside, so the line could never be wrapped and MD013 reported
        // a violation its own fixer refused to touch.
        let options = ReflowOptions {
            line_length: 80,
            atomic_spans: true,
            ..Default::default()
        };
        let body = "This is a very, very, very, very, very, very, very, very, very, very long line that is emphasised.";
        for (open, close) in [
            ("***", "***"),
            ("___", "___"),
            ("**_", "_**"),
            ("*__", "__*"),
            ("**~~", "~~**"),
        ] {
            let text = format!("{open}{body}{close}");
            assert!(text.len() > options.line_length, "case must start over budget");
            let lines = reflow_line(&text, &options);
            assert!(
                lines.len() > 1,
                "{open}...{close} should wrap but stayed on one line: {lines:?}"
            );
            assert!(
                lines.iter().all(|line| line.len() <= options.line_length),
                "{open}...{close} left a line over the budget: {lines:?}"
            );
            assert_eq!(
                lines.join(" "),
                text,
                "{open}...{close} wrapping must only replace a space with a newline"
            );
        }
    }

    #[test]
    fn test_overlong_span_with_stray_marker_stays_whole() {
        // A `*` that belongs to no well-formed span means the content is not
        // fully modelled. Breaking at these spaces would put `* ` at the start
        // of a line, turning literal text into a list item.
        let options = ReflowOptions {
            line_length: 40,
            atomic_spans: true,
            ..Default::default()
        };
        let text = "**alpha * beta gamma delta epsilon zeta eta theta iota kappa**";
        let lines = reflow_line(text, &options);
        assert_eq!(lines, vec![text], "stray marker must keep the span whole");
    }

    #[test]
    fn test_overlong_span_never_breaks_inside_a_nested_reference_link() {
        // A reference-style link only looks like a link once the document's
        // definitions are in scope, so the span's own parse sees plain text and
        // used to break inside the label. The top level holds these atomic, and
        // an inner span has to agree or `fmt` splits a link in one context and
        // not the other.
        let options = ReflowOptions {
            line_length: 30,
            atomic_spans: true,
            defined_references: Some(HashSet::from([
                "ref".to_string(),
                // A bare `[text]` is a link only when its own label is defined.
                "one two three four five six seven".to_string(),
            ])),
            ..Default::default()
        };
        for (text, link) in [
            (
                "_**alpha [one two three four five six seven][ref] beta gamma delta**_",
                "[one two three four five six seven][ref]",
            ),
            (
                "**alpha [one two three four five six seven][ref] beta gamma delta**",
                "[one two three four five six seven][ref]",
            ),
            (
                "_**alpha ![one two three four five six seven][ref] beta gamma delta**_",
                "![one two three four five six seven][ref]",
            ),
            (
                "_**alpha [one two three four five six seven][] beta gamma delta**_",
                "[one two three four five six seven][]",
            ),
            (
                "_**alpha [one two three four five six seven] beta gamma delta**_",
                "[one two three four five six seven]",
            ),
        ] {
            let lines = reflow_line(text, &options);
            assert!(lines.len() > 1, "over-long span should wrap: {lines:?}");
            assert!(
                lines.iter().any(|line| line.contains(link)),
                "{link} must stay on one line: {lines:?}"
            );
            assert_eq!(lines.join(" "), text, "wrapping must only move line breaks");
        }
    }

    #[test]
    fn test_overlong_span_breaks_inside_an_undefined_shortcut_reference() {
        // A bare `[text]` is only a link when its label is defined. With the
        // definitions in scope and no match, it is literal prose and breaks like
        // any other words, exactly as the top level treats it.
        let options = ReflowOptions {
            line_length: 30,
            atomic_spans: true,
            defined_references: Some(HashSet::new()),
            ..Default::default()
        };
        let text = "_**alpha [one two three four five six seven] beta gamma delta**_";
        let lines = reflow_line(text, &options);
        assert!(lines.len() > 1, "over-long span should wrap: {lines:?}");
        assert!(
            !lines
                .iter()
                .any(|line| line.contains("[one two three four five six seven]")),
            "an undefined shortcut is prose and should break: {lines:?}"
        );
        assert_eq!(lines.join(" "), text, "wrapping must only move line breaks");
    }

    #[test]
    fn test_overlong_span_never_breaks_inside_a_nested_attr_list() {
        // A MkDocs/kramdown attr list carries structural interior whitespace, so
        // splitting it rewrites the attributes. The top level holds it whole; an
        // inner span has to agree. Only when the flavor is enabled.
        let attr = "{.highlight key=\"a b c\"}";
        let text = format!("_**alpha beta gamma delta epsilon zeta{attr} eta theta iota kappa**_");
        let options = ReflowOptions {
            line_length: 20,
            atomic_spans: true,
            attr_lists: true,
            ..Default::default()
        };
        let lines = reflow_line(&text, &options);
        assert!(lines.len() > 1, "over-long span should wrap: {lines:?}");
        assert!(
            lines.iter().any(|line| line.contains(attr)),
            "attr list must stay on one line: {lines:?}"
        );
        assert_eq!(lines.join(" "), text, "wrapping must only move line breaks");

        // With the flavor off, the same braces are literal prose and break like
        // any other words, exactly as the top level treats them.
        let plain = ReflowOptions {
            attr_lists: false,
            ..options
        };
        let lines = reflow_line(&text, &plain);
        assert!(
            !lines.iter().any(|line| line.contains(attr)),
            "without the flavor the braces are prose and should break: {lines:?}"
        );
        assert_eq!(lines.join(" "), text, "wrapping must only move line breaks");
    }

    #[test]
    fn test_overlong_emphasis_never_breaks_inside_nested_code_span() {
        // Interior whitespace in a code span is literal, so a break inside one
        // would rewrite the code. The nested span is a single unbreakable unit
        // and its interior survives byte-for-byte.
        let options = ReflowOptions {
            line_length: 30,
            atomic_spans: true,
            ..Default::default()
        };
        let text = "_alpha beta gamma delta epsilon `a  b` zeta eta theta iota kappa_";
        let lines = reflow_line(text, &options);
        assert!(lines.len() > 1, "over-long emphasis should wrap: {lines:?}");
        assert!(
            lines.iter().any(|line| line.contains("`a  b`")),
            "nested code span must stay whole with its interior spaces: {lines:?}"
        );
        for line in &lines {
            assert_eq!(
                line.matches('`').count() % 2,
                0,
                "no line may contain half a code span: {line:?}"
            );
        }
    }

    #[test]
    fn test_definition_list_marker_does_not_start_line() {
        let options = ReflowOptions {
            line_length: 20,
            ..Default::default()
        };
        // Wrap should not start a line with ": "
        let lines = reflow_line("This is a term and : definition here.", &options);
        for line in &lines {
            assert!(
                !line.trim_start().starts_with(": "),
                "Wrapped line should not start with definition marker: {line}"
            );
        }
    }

    #[test]
    fn test_div_marker_does_not_start_line() {
        let options = ReflowOptions {
            line_length: 20,
            ..Default::default()
        };
        // Wrap should not start a line with ":::"
        let lines = reflow_line("This is some text with ::: class marker.", &options);
        for line in &lines {
            assert!(
                !line.trim_start().starts_with(":::"),
                "Wrapped line should not start with div marker: {line}"
            );
        }
    }
}
