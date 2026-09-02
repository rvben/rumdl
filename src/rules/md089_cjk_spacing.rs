//! Rule MD089: CJK letters and Latin text should be separated by a space.
//!
//! Chinese, Japanese and Korean copywriting guidelines put one space between a
//! CJK letter and an adjacent ASCII letter or digit (`日本語 english`,
//! `中文 123`, `한글 english`). The rule reports every such boundary that has
//! no space and inserts one. It is opt-in: Japanese technical writing largely
//! prefers the opposite, so the convention is a project's choice.
//!
//! Each prose line is split into units: a run of CJK letters, a run of ASCII
//! letters and digits, one symbol, or an inline construct. The rule looks at
//! both neighbours of every CJK run. A code span, an inline math span, a link,
//! a wikilink or a bare URL is one opaque unit that behaves like Latin text: it
//! gets a space on its outside and its inside is never touched. An image, an
//! HTML tag, an HTML comment, a `#tag` or a link reference definition is a
//! wall: the rule neither enters it nor spaces against it. An emphasis
//! delimiter run is transparent:
//! `**中**english` compares `中` with `english`, and the space goes on the
//! outer side of the delimiter, where it keeps the emphasis intact. A
//! configured symbol counts only when it is attached to a Latin run on its far
//! side (`90°的` fires, `你好-世界` does not).

mod md089_config;
#[cfg(test)]
mod tests;

use std::collections::HashSet;
use std::sync::LazyLock;

use regex::Regex;

use crate::filtered_lines::FilteredLinesExt;
use crate::lint_context::LintContext;
use crate::rule::{Fix, FixCapability, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::utils::obsidian_tag::TAG_PATTERN;
use crate::utils::range_utils::byte_to_char_count;
use crate::utils::unicode::is_cjk_letter;
use md089_config::MD089Config;

/// Rule MD089: CJK spacing.
#[derive(Debug, Clone)]
pub struct MD089CjkSpacing {
    /// Symbols that lead a Latin run and take a space after a CJK letter (`$5`).
    symbols_after_cjk: HashSet<char>,
    /// Symbols that trail a Latin run and take a space before a CJK letter (`90°`).
    symbols_before_cjk: HashSet<char>,
}

impl Default for MD089CjkSpacing {
    fn default() -> Self {
        Self::from_config_struct(MD089Config::default())
    }
}

impl MD089CjkSpacing {
    fn from_config_struct(config: MD089Config) -> Self {
        let set = |symbols: String| symbols.chars().filter(|c| !c.is_whitespace()).collect();
        Self {
            symbols_after_cjk: set(config.symbols_after_cjk),
            symbols_before_cjk: set(config.symbols_before_cjk),
        }
    }

    /// For each unit, whether a CJK letter directly after it needs a space
    /// (`latin_right`) and whether a CJK letter directly before it needs one
    /// (`latin_left`). Latin runs and opaque constructs always qualify; a
    /// configured symbol qualifies only when the unit on its far side does,
    /// so `90°` and `$5` count while a lone `-` between two CJK words does not.
    /// Delimiter runs pass the flag through unchanged.
    fn latin_edges(&self, units: &[Unit]) -> (Vec<bool>, Vec<bool>) {
        let n = units.len();
        let mut right = vec![false; n];
        for i in 0..n {
            right[i] = match units[i].kind {
                Kind::Latin | Kind::Opaque => true,
                Kind::Symbol(c) => i > 0 && right[i - 1] && self.symbols_before_cjk.contains(&c),
                Kind::Delimiter { .. } => i > 0 && right[i - 1],
                Kind::Cjk | Kind::Other | Kind::Wall => false,
            };
        }
        let mut left = vec![false; n];
        for i in (0..n).rev() {
            left[i] = match units[i].kind {
                Kind::Latin | Kind::Opaque => true,
                Kind::Symbol(c) => i + 1 < n && left[i + 1] && self.symbols_after_cjk.contains(&c),
                Kind::Delimiter { .. } => i + 1 < n && left[i + 1],
                Kind::Cjk | Kind::Other | Kind::Wall => false,
            };
        }
        (right, left)
    }

    /// Every missing space on one line, ordered by position.
    fn missing_spaces(&self, units: &[Unit]) -> Vec<Gap> {
        let (latin_right, latin_left) = self.latin_edges(units);
        let is_delimiter = |j: &usize| matches!(units[*j].kind, Kind::Delimiter { .. });
        let mut gaps = Vec::new();
        for (k, unit) in units.iter().enumerate() {
            if unit.kind != Kind::Cjk {
                continue;
            }
            // Latin text to the right of the CJK run.
            if let Some(j) = (k + 1..units.len()).find(|j| !is_delimiter(j))
                && latin_left[j]
            {
                gaps.push(Gap {
                    insert_at: first_opener(&units[k + 1..j]).unwrap_or(units[j].start),
                    left: (unit.start, unit.end),
                    right: attached_run(units, j, &latin_right, &latin_left, true),
                });
            }
            // Latin text to the left of the CJK run.
            if let Some(j) = (0..k).rev().find(|j| !is_delimiter(j))
                && latin_right[j]
            {
                gaps.push(Gap {
                    insert_at: first_opener(&units[j + 1..k]).unwrap_or(unit.start),
                    left: attached_run(units, j, &latin_right, &latin_left, false),
                    right: (unit.start, unit.end),
                });
            }
        }
        gaps.sort_by_key(|gap| gap.insert_at);
        gaps
    }
}

/// How the rule treats one unit of a line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    /// A run of CJK letters.
    Cjk,
    /// A run of ASCII letters and digits.
    Latin,
    /// One character that is neither. Whether it joins a Latin run depends on
    /// the configured symbol sets, so the character travels with the kind.
    Symbol(char),
    /// A run of characters the rule never spaces against and never looks
    /// through: whitespace, and letters or digits of other scripts (`１`,
    /// `é`, Cyrillic).
    Other,
    /// An emphasis delimiter run. Transparent: the rule looks through it.
    Delimiter { opener: bool },
    /// An inline construct treated as one Latin-like unit: code span, math
    /// span, link, wikilink, bare URL.
    Opaque,
    /// An inline construct the rule neither enters nor spaces against: image,
    /// HTML tag, HTML comment, `#tag`.
    Wall,
}

/// A unit of a line, as absolute byte offsets into the document.
#[derive(Debug, Clone, Copy)]
struct Unit {
    kind: Kind,
    start: usize,
    end: usize,
}

/// One missing space: where to insert it and the text on either side.
struct Gap {
    insert_at: usize,
    left: (usize, usize),
    right: (usize, usize),
}

/// Whether `c` renders as part of the character before it: a combining mark,
/// which is `Mn` or `Me` and covers the variation selectors that pick a glyph
/// for the kanji or emoji they follow. A format character is not bound to a
/// base, so a joiner stays a run breaker rather than trailing a space. ASCII
/// holds no marks, so the common case answers without the regex.
fn is_attached_mark(c: char) -> bool {
    if c.is_ascii() {
        return false;
    }
    static ATTACHED_MARK: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"^[\p{Mn}\p{Me}]$").expect("attached mark class is a valid regex"));
    let mut buf = [0u8; 4];
    ATTACHED_MARK.is_match(c.encode_utf8(&mut buf))
}

fn classify(c: char) -> Kind {
    if is_cjk_letter(c) {
        Kind::Cjk
    } else if c.is_ascii_alphanumeric() {
        Kind::Latin
    } else if c.is_whitespace() || c.is_alphanumeric() {
        Kind::Other
    } else {
        Kind::Symbol(c)
    }
}

/// The space goes outside the emphasis: before the first opener between the
/// two units, or, when only closers lie between, at the boundary after them.
fn first_opener(between: &[Unit]) -> Option<usize> {
    between
        .iter()
        .find(|unit| unit.kind == Kind::Delimiter { opener: true })
        .map(|unit| unit.start)
}

/// The byte extent of unit `j` together with every consecutive unit attached
/// to it, so that `90°` or `"hello"` reads as one thing in the message. A
/// symbol earns its place in the run through either edge array (`C++` holds
/// together because `+` has Latin on its left, `++C` because `+` has Latin on
/// its right), so both are checked at every step; passing one alone would
/// stop the walk at the run's far end.
fn attached_run(units: &[Unit], j: usize, latin_right: &[bool], latin_left: &[bool], forward: bool) -> (usize, usize) {
    let (mut start, mut end) = (units[j].start, units[j].end);
    if forward {
        let mut m = j;
        while m + 1 < units.len() && (latin_right[m + 1] || latin_left[m + 1]) {
            m += 1;
            end = units[m].end;
        }
    } else {
        let mut m = j;
        while m > 0 && (latin_right[m - 1] || latin_left[m - 1]) {
            m -= 1;
            start = units[m].start;
        }
    }
    (start, end)
}

/// Whether the text of the link spanning `link` is exactly one image
/// (`[![alt](img.png)](target)`): an image inside the link with nothing but
/// whitespace between the opening `[` and the image, and nothing but
/// whitespace between the image and the `]` that closes the text.
fn link_wraps_only_an_image(content: &str, link: (usize, usize), images: &[(usize, usize)]) -> bool {
    images.iter().any(|&(start, end)| {
        link.0 < start
            && end < link.1
            && content
                .get(link.0 + 1..start)
                .is_some_and(|before| before.trim().is_empty())
            && content
                .get(end..link.1)
                .is_some_and(|after| after.trim_start().starts_with(']'))
    })
}

/// The end of the `[^id]` marker that starts at `start`. `FootnoteRef` carries
/// no end offset, so the marker is measured from the source: it runs from `[^`
/// to the next `]`. A slice that does not open a marker has no end.
fn footnote_marker_end(content: &str, start: usize) -> Option<usize> {
    let rest = content.get(start..)?;
    if !rest.starts_with("[^") {
        return None;
    }
    rest.find(']').map(|offset| start + offset + 1)
}

/// The `[^id]:` label of a footnote definition, as byte offsets into `line`.
/// Indentation and blockquote markers may stand before the label; a
/// continuation line of the same definition has none.
fn footnote_label_range(line: &str) -> Option<(usize, usize)> {
    let start = line.find("[^")?;
    if !line[..start].chars().all(|c| c.is_whitespace() || c == '>') {
        return None;
    }
    let close = start + line[start..].find(']')?;
    line[close + 1..].starts_with(':').then_some((start, close + 2))
}

/// The text a container marker holds, or `None` when `rest` opens no
/// container. A list marker (`-`, `+`, `*`, or one to nine digits and `.` or
/// `)`) and a footnote-definition label (`[^id]:`) each open one, and each is
/// separated from its content by whitespace: a marker with nothing after it
/// holds nothing, so the whitespace is what makes it a marker.
fn container_marker_content(rest: &str) -> Option<&str> {
    let after_marker = if let Some(tail) = rest.strip_prefix(['-', '+', '*']) {
        tail
    } else if let Some(tail) = rest.strip_prefix("[^") {
        let close = tail.find(']')?;
        tail[close + 1..].strip_prefix(':')?
    } else {
        let digits = rest.len() - rest.trim_start_matches(|c: char| c.is_ascii_digit()).len();
        if !(1..=9).contains(&digits) {
            return None;
        }
        rest[digits..].strip_prefix([')', '.'])?
    };
    let content = after_marker.trim_start();
    (content.len() < after_marker.len()).then_some(content)
}

/// Whether a space inserted at the end of `prefix` would complete a list
/// marker, where `prefix` is the line content before the gap. `1)中文` is an
/// enumeration label, and `1) 中文` is an ordered list item, so the space would
/// change the block type of the line. A marker starts a block wherever its
/// container starts one, so everything that opens a container before it is
/// stripped first: indentation, blockquote markers, an enclosing list marker
/// and a footnote-definition label, in any nesting.
fn completes_list_marker(prefix: &str) -> bool {
    let mut rest = prefix.trim_start();
    loop {
        if let Some(tail) = rest.strip_prefix('>') {
            rest = tail.trim_start();
        } else if let Some(content) = container_marker_content(rest) {
            rest = content;
        } else {
            break;
        }
    }
    if matches!(rest, "-" | "+" | "*") {
        return true;
    }
    let Some(digits) = rest.strip_suffix([')', '.']) else {
        return false;
    };
    (1..=9).contains(&digits.len()) && digits.bytes().all(|b| b.is_ascii_digit())
}

/// Byte ranges of `#tag` tokens on a line, each running to the next
/// whitespace. A tag opens only at the start of a word, so a `#` glued to the
/// end of one opens nothing (`C#编程` is one word, `修复#123` an issue
/// reference). A word ends at an alphanumeric character, which covers Latin
/// letters, digits and CJK letters; everything else before the `#` starts a
/// word, so an emphasis marker, a bracket or punctuation lets a tag open. What
/// counts as a tag from there is [`TAG_PATTERN`], the definition MD018 reads
/// too, and a heading marker (`# `, `## `) never qualifies.
fn hashtag_ranges(line: &str, line_start: usize) -> Vec<(usize, usize)> {
    let mut ranges = Vec::new();
    let mut chars = line.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        if c != '#' {
            continue;
        }
        if line[..i].chars().next_back().is_some_and(char::is_alphanumeric) {
            continue;
        }
        if !TAG_PATTERN.is_match(&line[i..]) {
            continue;
        }
        let end = line[i..]
            .find(char::is_whitespace)
            .map_or(line.len(), |offset| i + offset);
        ranges.push((line_start + i, line_start + end));
        while chars.peek().is_some_and(|&(j, _)| j < end) {
            chars.next();
        }
    }
    ranges
}

/// Every inline construct in the document that the character walk must not
/// enter, sorted by start. Emphasis contributes its two delimiter runs, not
/// its content.
fn collect_specials(ctx: &LintContext) -> Vec<Unit> {
    let mut specials = Vec::new();
    let mut push = |start: usize, end: usize, kind: Kind| {
        if start < end {
            specials.push(Unit { kind, start, end });
        }
    };
    for span in ctx.code_spans().iter() {
        push(span.byte_offset, span.byte_end, Kind::Opaque);
    }
    for span in ctx.math_spans().iter() {
        push(span.byte_offset, span.byte_end, Kind::Opaque);
    }
    let images: Vec<(usize, usize)> = ctx
        .images()
        .iter()
        .map(|image| (image.byte_offset, image.byte_end))
        .collect();
    for link in ctx.links() {
        // A link whose text is one image is the clickable-badge construct: a
        // file reference with a target, not prose, so it is a wall like the
        // image it holds. Every other link is Latin-like and takes a space.
        let kind = if link_wraps_only_an_image(ctx.content, (link.byte_offset, link.byte_end), &images) {
            Kind::Wall
        } else {
            Kind::Opaque
        };
        push(link.byte_offset, link.byte_end, kind);
    }
    for url in ctx.bare_urls().iter() {
        push(url.byte_offset, url.byte_end, Kind::Opaque);
    }
    for &(start, end) in &images {
        push(start, end, Kind::Wall);
    }
    for tag in ctx.html_tags().iter() {
        push(tag.byte_offset, tag.byte_end, Kind::Wall);
    }
    for comment in ctx.html_comment_ranges() {
        push(comment.start, comment.end, Kind::Wall);
    }
    // A reference definition is a whole-line construct (its title may sit on
    // the next line); a blockquoted one starts after the `> ` prefix.
    for def in ctx.reference_definitions() {
        push(def.byte_offset, def.byte_end, Kind::Wall);
    }
    // A footnote marker names a footnote, so spacing it renames it and the
    // reference stops matching its definition. Like an image it takes no
    // space on either side.
    for footnote in ctx.footnote_references() {
        if let Some(end) = footnote_marker_end(ctx.content, footnote.byte_offset) {
            push(footnote.byte_offset, end, Kind::Wall);
        }
    }
    // Only the `[^id]:` label of a definition is a marker; the body after the
    // colon is prose and stays reachable.
    for line in ctx.lines.iter().filter(|line| line.in_footnote_definition) {
        if let Some((start, end)) = footnote_label_range(line.content(ctx.content)) {
            push(line.byte_offset + start, line.byte_offset + end, Kind::Wall);
        }
    }
    for span in ctx.emphasis_spans().iter() {
        let width = if span.is_strong { 2 } else { 1 };
        push(
            span.byte_offset,
            span.byte_offset + width,
            Kind::Delimiter { opener: true },
        );
        push(
            span.byte_end.saturating_sub(width),
            span.byte_end,
            Kind::Delimiter { opener: false },
        );
    }
    // A hashtag only exists in text the walk still owns, so every other
    // construct has to be in place and sorted before the scan runs.
    specials.sort_by_key(|unit| (unit.start, unit.end));
    let mut tags = Vec::new();
    let mut cursor = 0;
    let mut offset = 0;
    for line in ctx.content.split_inclusive('\n') {
        for (start, end) in hashtag_ranges(line.trim_end_matches(['\n', '\r']), offset) {
            while cursor < specials.len() && specials[cursor].end <= start {
                cursor += 1;
            }
            if let Some((start, end)) = tag_outside_specials(&specials[cursor..], start, end) {
                tags.push(Unit {
                    kind: Kind::Wall,
                    start,
                    end,
                });
            }
        }
        offset += line.len();
    }
    specials.extend(tags);
    specials.sort_by_key(|unit| (unit.start, unit.end));
    specials
}

/// The part of a `#tag` range the character walk still owns, or `None` when
/// the `#` sits inside another construct. An anchor link, a code span or an
/// image can hold a `#`, and there the `#` is that construct's business, not
/// a tag opener; a tag that runs into a construct ends where it begins.
/// `specials` is sorted by start.
fn tag_outside_specials(specials: &[Unit], start: usize, end: usize) -> Option<(usize, usize)> {
    for special in specials {
        if special.start > start {
            return Some((start, end.min(special.start)));
        }
        if special.end > start {
            return None;
        }
    }
    Some((start, end))
}

/// Split one line into units. `specials` holds every document special that
/// ends after the line starts, in start order; a special that overlaps the
/// line becomes one unit clamped to the line, and specials nested inside it
/// are skipped.
fn line_units(content: &str, line_start: usize, specials: &[Unit]) -> Vec<Unit> {
    let line_end = line_start + content.len();
    let mut units: Vec<Unit> = Vec::new();
    let mut next_special = 0;
    let mut pos = line_start;
    while pos < line_end {
        while next_special < specials.len() && specials[next_special].end <= pos {
            next_special += 1;
        }
        if let Some(special) = specials.get(next_special).filter(|special| special.start <= pos) {
            let end = special.end.min(line_end);
            units.push(Unit {
                kind: special.kind,
                start: pos,
                end,
            });
            pos = end;
            next_special += 1;
            continue;
        }
        let c = content[pos - line_start..]
            .chars()
            .next()
            .expect("pos is on a char boundary inside the line");
        let end = pos + c.len_utf8();
        // A mark or a variation selector belongs to the character before it, so
        // it continues that unit rather than ending it. At the start of a line
        // there is nothing for it to attach to and it stands on its own.
        if is_attached_mark(c)
            && let Some(last) = units.last_mut()
            && last.end == pos
        {
            last.end = end;
            pos = end;
            continue;
        }
        let kind = classify(c);
        match units.last_mut() {
            Some(last)
                if last.end == pos && last.kind == kind && matches!(kind, Kind::Cjk | Kind::Latin | Kind::Other) =>
            {
                last.end = end;
            }
            _ => units.push(Unit { kind, start: pos, end }),
        }
        pos = end;
    }
    units
}

/// Text of a byte range for the message, cut to sixteen characters.
fn excerpt(content: &str, (start, end): (usize, usize)) -> String {
    const MAX_CHARS: usize = 16;
    let text = &content[start..end];
    match text.char_indices().nth(MAX_CHARS) {
        Some((cut, _)) => format!("{}...", &text[..cut]),
        None => text.to_string(),
    }
}

impl Rule for MD089CjkSpacing {
    fn name(&self) -> &'static str {
        "MD089"
    }

    fn description(&self) -> &'static str {
        "CJK letters and Latin letters or digits should be separated by a space"
    }

    fn check(&self, ctx: &LintContext) -> LintResult {
        if self.should_skip(ctx) {
            return Ok(Vec::new());
        }
        let specials = collect_specials(ctx);
        let mut cursor = 0;
        let mut warnings = Vec::new();
        for line in ctx
            .filtered_lines()
            .skip_front_matter()
            .skip_code_blocks()
            .skip_html_blocks()
            .skip_html_comments()
            .skip_math_blocks()
            .skip_esm_blocks()
            .skip_jsx_expressions()
            .skip_mdx_comments()
            .skip_obsidian_comments()
        {
            // A kramdown block IAL (`{:.class}`) and the body of a kramdown
            // extension block are attribute metadata rather than prose, and a
            // space inside a class name renames the class.
            if line.line_info.is_kramdown_block_ial || line.line_info.in_kramdown_extension_block {
                continue;
            }
            let line_start = line.line_info.byte_offset;
            while cursor < specials.len() && specials[cursor].end <= line_start {
                cursor += 1;
            }
            let units = line_units(line.content, line_start, &specials[cursor..]);
            for gap in self.missing_spaces(&units) {
                // A marker followed straight by CJK is an enumeration label,
                // not prose touching prose. The whole warning goes, not just
                // the fix: the reader cannot act on it without turning the
                // line into a list item, so reporting it is worse than
                // staying quiet.
                if completes_list_marker(&line.content[..gap.insert_at - line_start]) {
                    continue;
                }
                // Pandoc attribute metadata is not prose either. A bracketed
                // span's range covers its text and its attributes as one
                // unit, so the whole construct is left alone rather than
                // guessing which half a gap belongs to.
                if ctx.is_in_inline_code_attr(gap.insert_at) || ctx.is_in_bracketed_span(gap.insert_at) {
                    continue;
                }
                let column = byte_to_char_count(line.content, gap.insert_at - line_start);
                warnings.push(LintWarning {
                    rule_name: Some(self.name().to_string()),
                    line: line.line_num,
                    column,
                    end_line: line.line_num,
                    end_column: column + 1,
                    severity: Severity::Warning,
                    message: format!(
                        "Missing space between \"{}\" and \"{}\"",
                        excerpt(ctx.content, gap.left),
                        excerpt(ctx.content, gap.right)
                    ),
                    fix: Some(Fix::new(gap.insert_at..gap.insert_at, " ".to_string())),
                });
            }
        }
        Ok(warnings)
    }

    fn fix(&self, ctx: &LintContext) -> Result<String, LintError> {
        if self.should_skip(ctx) {
            return Ok(ctx.content.to_string());
        }
        let warnings = self.check(ctx)?;
        if warnings.is_empty() {
            return Ok(ctx.content.to_string());
        }
        let warnings =
            crate::utils::fix_utils::filter_warnings_by_inline_config(warnings, ctx.inline_config(), self.name());
        crate::utils::fix_utils::apply_warning_fixes(ctx.content, &warnings).map_err(LintError::InvalidInput)
    }

    fn should_skip(&self, ctx: &LintContext) -> bool {
        !ctx.content.chars().any(is_cjk_letter)
    }

    fn category(&self) -> RuleCategory {
        RuleCategory::Whitespace
    }

    fn fix_capability(&self) -> FixCapability {
        FixCapability::FullyFixable
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    crate::impl_rule_config_methods!(MD089Config);
}
