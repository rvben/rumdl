use crate::utils::html_block::{opens_tag_line_html_block, opens_untagged_html_block, parse_html_block_start};
use crate::utils::list_fix_guard::{Allowed, drop_mdx_structure_changing_fixes, drop_structure_changing_fixes};
use crate::utils::range_utils::calculate_match_range;

use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::rule_config_serde::{RuleConfig, load_rule_config};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::sync::LazyLock;

/// Configuration for MD027 (Multiple spaces after blockquote symbol).
///
/// `list_items` mirrors markdownlint's option but rumdl's default is `false`
/// rather than `true`. See `docs/markdownlint-comparison.md` for the rationale:
/// list items inside blockquotes inherently need extra indentation, so flagging
/// them by default produces noise. Set `list-items = true` to opt into the
/// strict markdownlint behavior.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub struct MD027Config {
    /// When `true`, also flag blockquoted lines that introduce or continue a
    /// list item. When `false` (default), such lines are skipped.
    #[serde(default, alias = "list_items")]
    pub list_items: bool,
}

impl RuleConfig for MD027Config {
    const RULE_NAME: &'static str = "MD027";
}

// New patterns for detecting malformed blockquote attempts where user intent is clear
static MALFORMED_BLOCKQUOTE_PATTERNS: LazyLock<Vec<(Regex, &'static str)>> = LazyLock::new(|| {
    vec![
        // Double > without space: >>text (looks like nested but missing spaces)
        (
            Regex::new(r"^(\s*)>>([^\s>].*|$)").unwrap(),
            "missing spaces in nested blockquote",
        ),
        // Triple > without space: >>>text
        (
            Regex::new(r"^(\s*)>>>([^\s>].*|$)").unwrap(),
            "missing spaces in deeply nested blockquote",
        ),
        // Space then > then text: > >text (extra > by mistake)
        (
            Regex::new(r"^(\s*)>\s+>([^\s>].*|$)").unwrap(),
            "extra blockquote marker",
        ),
        // Multiple spaces then >: (spaces)>text (indented blockquote without space)
        (
            Regex::new(r"^(\s{4,})>([^\s].*|$)").unwrap(),
            "indented blockquote missing space",
        ),
    ]
});

// Cached regex for blockquote validation
static BLOCKQUOTE_PATTERN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\s*>").unwrap());

/// An item of a list inside a quote, in columns counted from the first
/// character after the quote markers and the one space that may follow them.
#[derive(Clone, Copy)]
struct QuotedItem {
    /// The marker character, or the delimiter of an ordered marker with the top
    /// bit set: items of one list share it.
    kind: u8,
    /// The column a continuation of the item has to reach.
    content_column: usize,
    /// Nothing has followed the marker yet: a blank line then ends the item.
    empty: bool,
}

/// The list items open at a quoted line.
///
/// Whitespace that reaches an open item's content column nests or continues the
/// item, and a marker of another kind than the item it closes opens a new list.
/// Neither is spacing after the quote marker. The indentation of a sibling in the
/// same list is.
#[derive(Default)]
struct QuotedItems {
    quote_level: usize,
    items: Vec<QuotedItem>,
    /// A paragraph runs into the next line, which may then continue it lazily.
    paragraph: bool,
    /// The line above is a row of a table, which no later line continues lazily.
    table: bool,
    /// The line above is inside an HTML block, which runs to the next blank line.
    html: bool,
    /// The cells of the header a delimiter row on the next line would belong to:
    /// the line above when it opened the running paragraph.
    header_cells: Option<usize>,
    /// The quotes this one is nested in, outermost first, as their lines left them.
    parents: Vec<QuotedItems>,
}

impl QuotedItems {
    /// Reads the quoted `line` (nested `level` deep) and returns whether its
    /// whitespace belongs to the list structure of an item opened earlier.
    fn enters(&mut self, line: &str, level: usize) -> bool {
        if level > self.quote_level && self.quote_level > 0 {
            self.close_before_nested_quote(line);
        }
        self.enter_quote(level);
        let mut seen = 0;
        let mut rest = line;
        while seen < level {
            let Some(at) = rest.find('>') else { return false };
            rest = &rest[at + 1..];
            seen += 1;
        }
        let (edge, rest) = Self::quote_content(line, rest);
        let text = rest.trim_start_matches([' ', '\t']);
        let indent = Self::columns(&line[..line.len() - text.len()]) - edge;
        if text.is_empty() {
            if self.items.last().is_some_and(|item| item.empty) {
                self.items.pop();
            }
            self.paragraph = false;
            self.table = false;
            self.html = false;
            self.header_cells = None;
            return false;
        }
        let breaks = Self::is_thematic_break(text);
        let mut marker = Self::marker_len(text).filter(|_| !breaks);
        // A line of plain text under a running paragraph continues it lazily and
        // keeps the items open; anything else is measured against them.
        let lazy = self.paragraph && marker.is_none() && !breaks && !self.items.is_empty() && !Self::starts_block(text);
        let mut shallowest_closed = None;
        if !lazy {
            while let Some(item) = self.items.last().filter(|item| item.content_column > indent) {
                shallowest_closed = Some(*item);
                self.items.pop();
            }
        }
        if shallowest_closed.is_some() {
            self.table = false;
            self.html = false;
        }
        // A paragraph running into the line, inside the container the line
        // re-enters, is only interrupted by a marker that can start a list while
        // it runs: not an empty item, and not an ordered one starting past 1.
        if self.paragraph && shallowest_closed.is_none() && marker.is_some_and(|(.., interrupts)| !interrupts) {
            marker = None;
        }
        if let Some(item) = self.items.last_mut() {
            item.empty = false;
        }
        let mut belongs = self.items.iter().any(|item| item.content_column <= indent);
        // Four columns past the enclosing item's content make code or a
        // paragraph continuation, not a marker.
        let base = self.items.last().map_or(0, |item| item.content_column);
        // Written four columns past that, a line starts no block and no list.
        let shallow = indent - base.min(indent) < 4;
        let starts_block = |text: &str| shallow && Self::starts_block(text);
        let breaks = shallow && breaks;
        let marker = marker.filter(|_| shallow);
        let mut first_text = marker.map_or(text, |(width, ..)| text[width..].trim_start_matches([' ', '\t']));
        if let Some(first) = marker {
            belongs |= shallowest_closed.is_some_and(|item| item.kind != first.2);
            // Markers written one after another (`- - a`) each open an item
            // inside the one before.
            let mut current = Some(first);
            let mut at = text;
            let mut column = indent;
            while let Some((width, padding, kind, _)) = current {
                let after = &at[width..];
                let marker_end = Self::columns(&line[..line.len() - after.len()]);
                let rest = after.trim_start_matches([' ', '\t']);
                let gap = Self::columns(&line[..line.len() - rest.len()]) - marker_end;
                let spaced = (1..=4).contains(&gap) && padding < after.len();
                let content_column = column + width + if spaced { gap } else { 1 };
                self.items.push(QuotedItem {
                    kind,
                    content_column,
                    empty: rest.is_empty(),
                });
                at = rest;
                column = content_column;
                current = Some(at)
                    .filter(|at| spaced && !at.is_empty() && !Self::is_thematic_break(at))
                    .and_then(Self::marker_len);
            }
            first_text = at;
        }
        // A run of `=` or `-` written inside the container of the paragraph above
        // turns it into a heading, which no later line continues.
        let run = text.trim_end_matches([' ', '\t']);
        let reentered = self.items.iter().all(|item| item.content_column <= indent);
        let underlines = self.paragraph
            && shallow
            && marker.is_none()
            && (run.bytes().all(|byte| byte == b'=') || run.bytes().all(|byte| byte == b'-'))
            && reentered;
        // A delimiter row turns the paragraph above into the header of a table,
        // whose rows then run on until a line that is not one.
        self.table = marker.is_none()
            && !breaks
            && !starts_block(text)
            && (self.table
                || (self.paragraph
                    && shallow
                    && !underlines
                    && reentered
                    && Self::is_table_delimiter(text)
                    && self.header_cells == Some(Self::cells(text))));
        let continues = self.paragraph && marker.is_none();
        self.html |= first_text.starts_with('<')
            && (starts_block(first_text) || (!continues && opens_tag_line_html_block(first_text)));
        self.paragraph =
            !breaks && !underlines && !self.table && !self.html && !first_text.is_empty() && !starts_block(first_text);
        self.header_cells = self.paragraph.then(|| Self::cells(first_text));
        belongs
    }

    /// Reads a line without a quote marker: plain text under a running paragraph
    /// continues it lazily and keeps the quote; any other line ends the quote.
    fn unquoted_line(&mut self, line: &str) {
        let text = line.trim_start_matches([' ', '\t']);
        let deep = Self::columns(&line[..line.len() - text.len()]) >= 4;
        let lazy = self.paragraph
            && !text.is_empty()
            && (deep
                || (!Self::starts_block(text) && !Self::is_thematic_break(text) && Self::marker_len(text).is_none()));
        if !lazy {
            *self = Self::default();
        }
    }

    /// A quote nested inside this one stays in the items it is indented into; the
    /// items it is written to the left of are closed by it.
    fn close_before_nested_quote(&mut self, line: &str) {
        let mut rest = line;
        for _ in 0..self.quote_level {
            let Some(at) = rest.find('>') else { return };
            rest = &rest[at + 1..];
        }
        let (edge, rest) = Self::quote_content(line, rest);
        let text = rest.trim_start_matches([' ', '\t']);
        let indent = Self::columns(&line[..line.len() - text.len()]) - edge;
        while self.items.last().is_some_and(|item| item.content_column > indent) {
            self.items.pop();
        }
    }

    /// Switches to the items of the quote nested `level` deep. A quote nested
    /// inside an item leaves the quote around it as it was, to be picked up again
    /// when a line returns to that depth; a depth not seen before starts empty.
    fn enter_quote(&mut self, level: usize) {
        if level == self.quote_level {
            return;
        }
        let nested_paragraph = self.paragraph;
        let mut outer = std::mem::take(&mut self.parents);
        if level > self.quote_level {
            if self.quote_level > 0 {
                outer.push(std::mem::take(self));
            }
        } else {
            while outer.last().is_some_and(|parent| parent.quote_level > level) {
                outer.pop();
            }
        }
        *self = match outer.pop_if(|parent| parent.quote_level == level) {
            // The line back in the outer quote continues the paragraph the nested
            // one ended in, if any, and nothing the outer one had running.
            Some(parent) => Self {
                paragraph: level < self.quote_level && nested_paragraph,
                table: false,
                html: false,
                header_cells: None,
                ..parent
            },
            None => Self {
                quote_level: level,
                ..Self::default()
            },
        };
        self.parents = outer;
    }

    /// Reads a line inside a code block of the quote: it closes the items it is
    /// not indented into and leaves no paragraph running.
    fn code_line(&mut self, line: &str, level: usize) {
        if level > self.quote_level && self.quote_level > 0 {
            self.close_before_nested_quote(line);
        }
        self.enter_quote(level);
        let mut rest = line;
        for _ in 0..level {
            let Some(at) = rest.find('>') else { return };
            rest = &rest[at + 1..];
        }
        let (edge, rest) = Self::quote_content(line, rest);
        let text = rest.trim_start_matches([' ', '\t']);
        let indent = Self::columns(&line[..line.len() - text.len()]) - edge;
        if !text.is_empty() {
            while self.items.last().is_some_and(|item| item.content_column > indent) {
                self.items.pop();
            }
        }
        self.paragraph = false;
    }

    /// The column a line's text reaches after `prefix`, with a tab running to the
    /// next multiple of four.
    fn columns(prefix: &str) -> usize {
        prefix
            .chars()
            .fold(0, |column, c| if c == '\t' { (column / 4 + 1) * 4 } else { column + 1 })
    }

    /// The column the content of the quote starts at, and the text from there.
    /// `after_marker` is what follows the innermost `>` of `line`; the `>` takes
    /// one space or tab, and a tab reaching further leaves the rest as indent.
    fn quote_content<'a>(line: &str, after_marker: &'a str) -> (usize, &'a str) {
        let marker_end = Self::columns(&line[..line.len() - after_marker.len()]);
        match after_marker.strip_prefix([' ', '\t']) {
            Some(rest) => (marker_end + 1, rest),
            None => (marker_end, after_marker),
        }
    }

    /// Whether `text` opens a block that is not paragraph text: it cannot be a
    /// lazy continuation line, and no paragraph runs on from it.
    fn starts_block(text: &str) -> bool {
        let hashes = text.bytes().take_while(|&byte| byte == b'#').count();
        match text.as_bytes()[0] {
            b'#' => (1..=6).contains(&hashes) && (text.len() == hashes || text[hashes..].starts_with([' ', '\t'])),
            b'>' => true,
            b'<' => parse_html_block_start(text).is_some() || opens_untagged_html_block(text),
            b'`' => {
                let ticks = text.bytes().take_while(|&byte| byte == b'`').count();
                ticks >= 3 && !text[ticks..].contains('`')
            }
            b'~' => text.starts_with("~~~"),
            _ => false,
        }
    }

    /// How many cells a table row of `text` has: what lies between its pipes,
    /// the outer ones optional.
    fn cells(text: &str) -> usize {
        let row = text.trim_matches([' ', '\t']);
        let row = row.strip_prefix('|').unwrap_or(row);
        let row = row.strip_suffix('|').filter(|row| !row.ends_with('\\')).unwrap_or(row);
        let mut escaped = false;
        1 + row
            .chars()
            .filter(|&c| {
                let separator = c == '|' && !escaped;
                escaped = c == '\\' && !escaped;
                separator
            })
            .count()
    }

    /// A table's delimiter row: every cell is dashes with optional colons at
    /// either end, and the row has a pipe.
    fn is_table_delimiter(text: &str) -> bool {
        let row = text.trim_matches([' ', '\t']);
        text.contains('|')
            && row
                .strip_prefix('|')
                .unwrap_or(row)
                .strip_suffix('|')
                .unwrap_or_else(|| row.strip_prefix('|').unwrap_or(row))
                .split('|')
                .all(|cell| {
                    let cell = cell.trim_matches([' ', '\t']);
                    let dashes = cell.strip_prefix(':').unwrap_or(cell);
                    let dashes = dashes.strip_suffix(':').unwrap_or(dashes);
                    !dashes.is_empty() && dashes.bytes().all(|byte| byte == b'-')
                })
    }

    /// A line of three or more `-`, `_` or `*`, spaces allowed between them.
    fn is_thematic_break(text: &str) -> bool {
        let Some(first) = text.chars().next().filter(|c| matches!(c, '-' | '_' | '*')) else {
            return false;
        };
        let mut count = 0;
        for c in text.chars() {
            if c == first {
                count += 1;
            } else if !matches!(c, ' ' | '\t') {
                return false;
            }
        }
        count >= 3
    }

    /// Width of a list marker opening `text`, the whitespace that follows it, the
    /// marker's kind, and whether it could interrupt a running paragraph.
    fn marker_len(text: &str) -> Option<(usize, usize, u8, bool)> {
        let bytes = text.as_bytes();
        let (width, kind, starts_list) = match bytes[0] {
            b @ (b'-' | b'+' | b'*') => (1, b, true),
            b'0'..=b'9' => {
                let digits = bytes.iter().take_while(|b| b.is_ascii_digit()).count();
                match bytes.get(digits) {
                    Some(&d @ (b'.' | b')')) if digits <= 9 => (digits + 1, d | 0x80, text[..digits].parse() == Ok(1)),
                    _ => return None,
                }
            }
            _ => return None,
        };
        let padding = match bytes.get(width) {
            None => 0,
            Some(b' ' | b'\t') => text[width..].len() - text[width..].trim_start_matches([' ', '\t']).len(),
            _ => return None,
        };
        let interrupts = starts_list && padding < text.len() - width;
        Some((width, padding, kind, interrupts))
    }
}

/// Rule MD027: No multiple spaces after blockquote symbol
///
/// See [docs/md027.md](../../docs/md027.md) for full documentation, configuration, and examples.
#[derive(Debug, Default, Clone)]
pub struct MD027MultipleSpacesBlockquote {
    config: MD027Config,
}

impl MD027MultipleSpacesBlockquote {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_config(config: MD027Config) -> Self {
        Self { config }
    }
}

impl Rule for MD027MultipleSpacesBlockquote {
    fn native_format_policy(&self) -> Option<crate::format_proof::NativeFormatPolicy> {
        (self.config == Default::default()).then_some(crate::format_proof::NativeFormatPolicy::for_rule::<Self>(27))
    }

    fn name(&self) -> &'static str {
        "MD027"
    }

    fn description(&self) -> &'static str {
        "Multiple spaces after quote marker (>)"
    }

    fn category(&self) -> RuleCategory {
        RuleCategory::Blockquote
    }

    fn check(&self, ctx: &crate::lint_context::LintContext) -> LintResult {
        // Spacing fixes only trim whitespace, so they must leave the document's
        // structure alone. Malformed-quote fixes exist to create a quote, so they
        // are held to nothing.
        let mut spacing = Vec::new();
        let mut malformed = Vec::new();
        let mut quoted_items = QuotedItems::default();

        let previous_blockquote_line_had_list = if self.config.list_items {
            Vec::new()
        } else {
            Self::previous_blockquote_lines_had_list(ctx)
        };
        for (line_idx, line_info) in ctx.lines.iter().enumerate() {
            let line_num = line_idx + 1;
            let marker_byte = line_info.byte_offset + line_info.indent;
            if line_info.in_esm_block
                || ctx.overlaps_template_code(marker_byte, marker_byte + 1)
                || ctx.overlaps_mdx_inline_code(marker_byte, marker_byte + 1)
            {
                continue;
            }

            if let Some(blockquote) = &line_info.blockquote {
                if line_info.in_code_block {
                    // An indented block can begin on the line of a list marker, past
                    // the marker's padding: the marker still opens its item.
                    let line_end = line_info.byte_offset + line_info.content(ctx.content).len();
                    let opens_on_line = ctx
                        .code_block_details
                        .iter()
                        .any(|block| !block.is_fenced && (line_info.byte_offset..line_end).contains(&block.start));
                    if opens_on_line {
                        quoted_items.enters(line_info.content(ctx.content), blockquote.nesting_level);
                        quoted_items.paragraph = false;
                    } else {
                        quoted_items.code_line(line_info.content(ctx.content), blockquote.nesting_level);
                    }
                }
            } else {
                quoted_items.unquoted_line(line_info.content(ctx.content));
            }

            // Skip lines in code blocks, HTML blocks, and other skippable regions
            if line_info.in_code_block
                || line_info.in_html_block
                || line_info.in_html_comment
                || line_info.in_mdx_comment
                || line_info.in_front_matter
                || line_info.in_mkdocstrings
                || line_info.in_jsx_block
                || line_info.in_kramdown_extension_block
            {
                continue;
            }

            // Check if this line is a blockquote using cached info
            if let Some(blockquote) = &line_info.blockquote {
                let mut inside_item = quoted_items.enters(line_info.content(ctx.content), blockquote.nesting_level);
                // A definition's later paragraph is indented to belong to it; that
                // indentation is structure, not spacing after the quote marker.
                inside_item |= ctx
                    .definition_text_at(line_num)
                    .is_some_and(|text| text.marker_prefix_len.is_none());
                // Part 1: Check for multiple spaces after the blockquote marker.
                //
                // When `list_items = false` (rumdl default), skip lines that are part
                // of a list inside a blockquote — the extra spaces are list-indent,
                // not formatting noise. When `list_items = true` (markdownlint default),
                // flag those lines too.
                let skip_list_lines = !self.config.list_items;
                let is_likely_list_continuation = skip_list_lines
                    && (ctx.is_in_list_block(line_num)
                        || line_info.list_item.is_some()
                        || previous_blockquote_line_had_list[line_idx]);
                if blockquote.has_multiple_spaces_after_marker && !is_likely_list_continuation && !inside_item {
                    // Find where the extra spaces start in the line
                    // We need to find the position after the markers and first space/tab
                    let mut byte_pos = 0;
                    let mut found_markers = 0;
                    let mut found_first_space = false;

                    for (i, ch) in line_info.content(ctx.content).char_indices() {
                        if found_markers < blockquote.nesting_level {
                            if ch == '>' {
                                found_markers += 1;
                            }
                        } else if !found_first_space && (ch == ' ' || ch == '\t') {
                            // This is the first space/tab after markers
                            found_first_space = true;
                        } else if found_first_space && (ch == ' ' || ch == '\t') {
                            // This is where extra spaces start
                            byte_pos = i;
                            break;
                        }
                    }

                    // Count how many extra spaces/tabs there are
                    let extra_spaces_bytes = line_info.content(ctx.content)[byte_pos..]
                        .chars()
                        .take_while(|&c| c == ' ' || c == '\t')
                        .fold(0, |acc, ch| acc + ch.len_utf8());

                    if extra_spaces_bytes > 0 {
                        // When blockquote content is empty, remove all spaces
                        // after the marker to avoid creating trailing whitespace
                        let (fix_byte_pos, fix_bytes) = if blockquote.content.is_empty() {
                            // Remove the first space too (byte_pos - 1 points to
                            // the first space we skipped)
                            let first_space_pos = byte_pos - 1;
                            let all_spaces_bytes = line_info.content(ctx.content)[first_space_pos..]
                                .chars()
                                .take_while(|&c| c == ' ' || c == '\t')
                                .fold(0, |acc, ch| acc + ch.len_utf8());
                            (first_space_pos, all_spaces_bytes)
                        } else {
                            (byte_pos, extra_spaces_bytes)
                        };

                        let (start_line, start_col, end_line, end_col) =
                            calculate_match_range(line_num, line_info.content(ctx.content), fix_byte_pos, fix_bytes);

                        spacing.push(LintWarning {
                            rule_name: Some(self.name().to_string()),
                            line: start_line,
                            column: start_col,
                            end_line,
                            end_column: end_col,
                            message: "Multiple spaces after quote marker (>)".to_string(),
                            severity: Severity::Warning,
                            fix: Some(Fix::new(
                                {
                                    let start_byte = ctx.line_column_byte_range(line_num, start_col).start;
                                    let end_byte = ctx.line_column_byte_range(line_num, end_col).start;
                                    start_byte..end_byte
                                },
                                String::new(),
                            )),
                        });
                    }
                }
            } else {
                quoted_items.unquoted_line(line_info.content(ctx.content));
                // Part 2: Check for malformed blockquote attempts on non-blockquote lines
                let malformed_attempts = self.detect_malformed_blockquote_attempts(line_info.content(ctx.content));
                for (start, len, fixed_line, description) in malformed_attempts {
                    let (start_line, start_col, end_line, end_col) =
                        calculate_match_range(line_num, line_info.content(ctx.content), start, len);

                    malformed.push(LintWarning {
                        rule_name: Some(self.name().to_string()),
                        line: start_line,
                        column: start_col,
                        end_line,
                        end_column: end_col,
                        message: format!("Malformed quote: {description}"),
                        severity: Severity::Warning,
                        fix: Some(Fix::new(
                            ctx.line_column_byte_range_with_length(
                                line_num,
                                1,
                                line_info.content(ctx.content).chars().count(),
                            ),
                            fixed_line,
                        )),
                    });
                }
            }
        }

        // A line indented four or more columns past the marker continues the
        // paragraph above it. Whatever it is shaped like, one space would turn it
        // into a heading, list, fence or break, so it stays reported and unfixed.
        // MDX reads no indented code, so its own parse decides what a fix changes.
        if ctx.flavor == crate::config::MarkdownFlavor::MDX {
            drop_mdx_structure_changing_fixes(ctx, &mut spacing);
        } else {
            drop_structure_changing_fixes(ctx, &mut spacing, Allowed::Nothing);
        }

        let mut warnings = spacing;
        warnings.extend(malformed);
        warnings.sort_by_key(|warning| warning.line);
        Ok(warnings)
    }

    fn fixes_from_diagnostics(&self) -> bool {
        true
    }

    fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, LintError> {
        if self.should_skip(ctx) {
            return Ok(ctx.content.to_string());
        }
        let warnings = self.check(ctx)?;
        if warnings.is_empty() {
            return Ok(ctx.content.to_string());
        }
        let warnings =
            crate::utils::fix_utils::filter_warnings_by_inline_config(warnings, ctx.inline_config(), self.name());
        crate::utils::fix_utils::apply_warning_fixes(ctx.content, &warnings)
            .map_err(crate::rule::LintError::InvalidInput)
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn from_config(config: &crate::config::Config) -> Box<dyn Rule>
    where
        Self: Sized,
    {
        let rule_config: MD027Config = load_rule_config(config);
        Box::new(MD027MultipleSpacesBlockquote::with_config(rule_config))
    }

    crate::impl_rule_config_sections!(MD027Config);

    /// Check if this rule should be skipped
    fn should_skip(&self, ctx: &crate::lint_context::LintContext) -> bool {
        ctx.content.is_empty() || !ctx.likely_has_blockquotes()
    }
}

impl MD027MultipleSpacesBlockquote {
    /// For each line, whether the run of blockquote lines directly above it holds a list
    /// item or a line inside a list block. This helps identify list continuation lines
    /// even when list block detection doesn't catch all continuation lines. A line that
    /// is not in a blockquote ends the run.
    fn previous_blockquote_lines_had_list(ctx: &crate::lint_context::LintContext) -> Vec<bool> {
        let mut had_list = Vec::with_capacity(ctx.lines.len());
        let mut run_has_list = false;
        for (idx, line) in ctx.lines.iter().enumerate() {
            had_list.push(run_has_list);
            if line.blockquote.is_none() {
                run_has_list = false;
            } else if line.list_item.is_some() || ctx.is_in_list_block(idx + 1) {
                run_has_list = true;
            }
        }
        had_list
    }

    /// Detect malformed blockquote attempts where user intent is clear
    fn detect_malformed_blockquote_attempts(&self, line: &str) -> Vec<(usize, usize, String, String)> {
        let mut results = Vec::new();
        // Every malformed pattern starts with Unicode whitespace followed by
        // `>`. Ordinary prose cannot match any of them.
        if !line.trim_start().starts_with('>') {
            return results;
        }

        for (pattern, issue_type) in MALFORMED_BLOCKQUOTE_PATTERNS.iter() {
            if let Some(cap) = pattern.captures(line) {
                let match_obj = cap.get(0).unwrap();
                let start = match_obj.start();
                let len = match_obj.len();

                // Extract potential blockquote components
                if let Some((fixed_line, description)) = self.extract_blockquote_fix_from_match(&cap, issue_type, line)
                {
                    // Only proceed if this looks like a genuine blockquote attempt
                    if self.looks_like_blockquote_attempt(line, &fixed_line) {
                        results.push((start, len, fixed_line, description));
                    }
                }
            }
        }

        results
    }

    /// Extract the proper blockquote format from a malformed match
    fn extract_blockquote_fix_from_match(
        &self,
        cap: &regex::Captures,
        issue_type: &str,
        _original_line: &str,
    ) -> Option<(String, String)> {
        match issue_type {
            "missing spaces in nested blockquote" => {
                // >>text -> > > text
                let indent = cap.get(1).map_or("", |m| m.as_str());
                let content = cap.get(2).map_or("", |m| m.as_str());
                Some((
                    format!("{}> > {}", indent, content.trim()),
                    "Missing spaces in nested blockquote".to_string(),
                ))
            }
            "missing spaces in deeply nested blockquote" => {
                // >>>text -> > > > text
                let indent = cap.get(1).map_or("", |m| m.as_str());
                let content = cap.get(2).map_or("", |m| m.as_str());
                Some((
                    format!("{}> > > {}", indent, content.trim()),
                    "Missing spaces in deeply nested blockquote".to_string(),
                ))
            }
            "extra blockquote marker" => {
                // > >text -> > text
                let indent = cap.get(1).map_or("", |m| m.as_str());
                let content = cap.get(2).map_or("", |m| m.as_str());
                Some((
                    format!("{}> {}", indent, content.trim()),
                    "Extra blockquote marker".to_string(),
                ))
            }
            "indented blockquote missing space" => {
                // (spaces)>text -> (spaces)> text
                let indent = cap.get(1).map_or("", |m| m.as_str());
                let content = cap.get(2).map_or("", |m| m.as_str());
                Some((
                    format!("{}> {}", indent, content.trim()),
                    "Indented blockquote missing space".to_string(),
                ))
            }
            _ => None,
        }
    }

    /// Check if the pattern looks like a genuine blockquote attempt
    fn looks_like_blockquote_attempt(&self, original: &str, fixed: &str) -> bool {
        // Basic heuristics to avoid false positives

        // 1. Content should not be too short (avoid flagging things like ">>>" alone)
        let trimmed_original = original.trim();
        if trimmed_original.len() < 5 {
            // More restrictive
            return false;
        }

        // 2. Should contain some text content after the markers
        let content_after_markers = trimmed_original.trim_start_matches('>').trim_start_matches(' ');
        if content_after_markers.is_empty() || content_after_markers.len() < 3 {
            // More restrictive
            return false;
        }

        // 3. Content should contain some alphabetic characters (not just symbols)
        if !content_after_markers.chars().any(char::is_alphabetic) {
            return false;
        }

        // 4. Fixed version should actually be a valid blockquote
        // Check if it starts with optional whitespace followed by >
        if !BLOCKQUOTE_PATTERN.is_match(fixed) {
            return false;
        }

        // 5. Avoid flagging things that might be code or special syntax
        if content_after_markers.starts_with('#') // Headers
            || content_after_markers.starts_with('[') // Links
            || content_after_markers.starts_with('`') // Code
            || content_after_markers.starts_with("http") // URLs
            || content_after_markers.starts_with("www.") // URLs
            || content_after_markers.starts_with("ftp")
        // URLs
        {
            return false;
        }

        // 6. Content should look like prose, not code or markup
        let word_count = content_after_markers.split_whitespace().count();
        if word_count < 3 {
            // Should be at least 3 words to look like prose
            return false;
        }

        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lint_context::LintContext;

    #[test]
    fn test_valid_blockquote() {
        let rule = MD027MultipleSpacesBlockquote::default();
        let content = "> This is a blockquote\n> > Nested quote";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Valid blockquotes should not be flagged");
    }

    #[test]
    fn test_multiple_spaces_after_marker() {
        let rule = MD027MultipleSpacesBlockquote::default();
        let content = ">  This has two spaces\n>   This has three spaces";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].line, 1);
        assert_eq!(result[0].column, 3); // Points to the extra space (after > and first space)
        assert_eq!(result[0].message, "Multiple spaces after quote marker (>)");
        assert_eq!(result[1].line, 2);
        assert_eq!(result[1].column, 3);
    }

    #[test]
    fn test_nested_multiple_spaces() {
        let rule = MD027MultipleSpacesBlockquote::default();
        // LintContext sees these as single-level blockquotes because of the space between markers
        let content = ">  Two spaces after marker\n>>  Two spaces in nested blockquote";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 2);
        assert!(result[0].message.contains("Multiple spaces"));
        assert!(result[1].message.contains("Multiple spaces"));
    }

    #[test]
    fn test_malformed_nested_quote() {
        let rule = MD027MultipleSpacesBlockquote::default();
        // LintContext sees >>text as a valid nested blockquote with no space after marker
        // MD027 doesn't flag this as malformed, only as missing space after marker
        let content = ">>This is a nested blockquote without space after markers";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        // This should not be flagged at all since >>text is valid CommonMark
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_malformed_deeply_nested() {
        let rule = MD027MultipleSpacesBlockquote::default();
        // LintContext sees >>>text as a valid triple-nested blockquote
        let content = ">>>This is deeply nested without spaces after markers";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        // This should not be flagged - >>>text is valid CommonMark
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_extra_quote_marker() {
        let rule = MD027MultipleSpacesBlockquote::default();
        // "> >text" is parsed as single-level blockquote with ">text" as content
        // This is valid CommonMark and not detected as malformed
        let content = "> >This looks like nested but is actually single level with >This as content";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_indented_missing_space() {
        let rule = MD027MultipleSpacesBlockquote::default();
        // 4+ spaces makes this a code block, not a blockquote
        let content = "   >This has 3 spaces indent and no space after marker";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        // LintContext sees this as a blockquote with no space after marker
        // MD027 doesn't flag this as malformed
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_fix_multiple_spaces() {
        let rule = MD027MultipleSpacesBlockquote::default();
        let content = ">  Two spaces\n>   Three spaces";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, "> Two spaces\n> Three spaces");
    }

    #[test]
    fn test_fix_malformed_quotes() {
        let rule = MD027MultipleSpacesBlockquote::default();
        // These are valid nested blockquotes, not malformed
        let content = ">>Nested without spaces\n>>>Deeply nested without spaces";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        // No fix needed - these are valid
        assert_eq!(fixed, content);
    }

    #[test]
    fn test_fix_extra_marker() {
        let rule = MD027MultipleSpacesBlockquote::default();
        // This is valid - single blockquote with >Extra as content
        let content = "> >Extra marker here";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        // No fix needed
        assert_eq!(fixed, content);
    }

    #[test]
    fn test_code_block_ignored() {
        let rule = MD027MultipleSpacesBlockquote::default();
        let content = "```\n>  This is in a code block\n```";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Code blocks should be ignored");
    }

    #[test]
    fn test_short_content_not_flagged() {
        let rule = MD027MultipleSpacesBlockquote::default();
        let content = ">>>\n>>";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Very short content should not be flagged");
    }

    #[test]
    fn test_non_prose_not_flagged() {
        let rule = MD027MultipleSpacesBlockquote::default();
        let content = ">>#header\n>>[link]\n>>`code`\n>>http://example.com";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Non-prose content should not be flagged");
    }

    #[test]
    fn test_preserve_trailing_newline() {
        let rule = MD027MultipleSpacesBlockquote::default();
        let content = ">  Two spaces\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, "> Two spaces\n");

        let content_no_newline = ">  Two spaces";
        let ctx2 = LintContext::new(content_no_newline, crate::config::MarkdownFlavor::Standard, None);
        let fixed2 = rule.fix(&ctx2).unwrap();
        assert_eq!(fixed2, "> Two spaces");
    }

    #[test]
    fn test_mixed_issues() {
        let rule = MD027MultipleSpacesBlockquote::default();
        let content = ">  Multiple spaces here\n>>Normal nested quote\n> Normal quote";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1, "Should only flag the multiple spaces");
        assert_eq!(result[0].line, 1);
    }

    #[test]
    fn test_looks_like_blockquote_attempt() {
        let rule = MD027MultipleSpacesBlockquote::default();

        // Should return true for genuine attempts
        assert!(rule.looks_like_blockquote_attempt(
            ">>This is a real blockquote attempt with text",
            "> > This is a real blockquote attempt with text"
        ));

        // Should return false for too short
        assert!(!rule.looks_like_blockquote_attempt(">>>", "> > >"));

        // Should return false for no alphabetic content
        assert!(!rule.looks_like_blockquote_attempt(">>123", "> > 123"));

        // Should return false for code-like content
        assert!(!rule.looks_like_blockquote_attempt(">>#header", "> > #header"));
    }

    #[test]
    fn test_extract_blockquote_fix() {
        let rule = MD027MultipleSpacesBlockquote::default();
        let regex = Regex::new(r"^(\s*)>>([^\s>].*|$)").unwrap();
        let cap = regex.captures(">>content").unwrap();

        let result = rule.extract_blockquote_fix_from_match(&cap, "missing spaces in nested blockquote", ">>content");
        assert!(result.is_some());
        let (fixed, desc) = result.unwrap();
        assert_eq!(fixed, "> > content");
        assert!(desc.contains("Missing spaces"));
    }

    #[test]
    fn test_empty_blockquote() {
        let rule = MD027MultipleSpacesBlockquote::default();
        let content = ">\n>  \n> content";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        // Empty blockquotes with multiple spaces should still be flagged
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].line, 2);
    }

    #[test]
    fn test_fix_preserves_indentation() {
        let rule = MD027MultipleSpacesBlockquote::default();
        let content = "  >  Indented with multiple spaces";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, "  > Indented with multiple spaces");
    }

    #[test]
    fn test_tabs_after_marker_not_flagged() {
        // MD027 only flags multiple SPACES, not tabs
        // Tabs after blockquote markers are handled by MD010 (no-hard-tabs)
        // This matches markdownlint reference behavior
        let rule = MD027MultipleSpacesBlockquote::default();

        // Tab after marker - NOT flagged by MD027 (that's MD010's job)
        let content = ">\tTab after marker";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 0, "Single tab should not be flagged by MD027");

        // Two tabs after marker - NOT flagged by MD027
        let content2 = ">\t\tTwo tabs";
        let ctx2 = LintContext::new(content2, crate::config::MarkdownFlavor::Standard, None);
        let result2 = rule.check(&ctx2).unwrap();
        assert_eq!(result2.len(), 0, "Tabs should not be flagged by MD027");
    }

    #[test]
    fn test_mixed_spaces_and_tabs() {
        let rule = MD027MultipleSpacesBlockquote::default();
        // Space then tab - only flags if there are multiple spaces
        // The tab itself is MD010's domain
        let content = ">  Space Space";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].column, 3); // Points to the extra space

        // Three spaces should be flagged
        let content2 = ">   Three spaces";
        let ctx2 = LintContext::new(content2, crate::config::MarkdownFlavor::Standard, None);
        let result2 = rule.check(&ctx2).unwrap();
        assert_eq!(result2.len(), 1);
    }

    #[test]
    fn test_fix_multiple_spaces_various() {
        let rule = MD027MultipleSpacesBlockquote::default();
        // Fix should remove extra spaces
        let content = ">   Three spaces";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, "> Three spaces");

        // Fix multiple spaces
        let content2 = ">    Four spaces";
        let ctx2 = LintContext::new(content2, crate::config::MarkdownFlavor::Standard, None);
        let fixed2 = rule.fix(&ctx2).unwrap();
        assert_eq!(fixed2, "> Four spaces");
    }

    #[test]
    fn test_list_continuation_inside_blockquote_not_flagged() {
        // List continuation indentation inside blockquotes should NOT be flagged
        // This matches markdownlint-cli behavior
        let rule = MD027MultipleSpacesBlockquote::default();

        // List with continuation inside blockquote
        let content = "> - Item starts here\n>   This continues the item\n> - Another item";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "List continuation inside blockquote should not be flagged, got: {result:?}"
        );

        // Multiple list items with continuations
        let content2 = "> * First item\n>   First item continuation\n>   Still continuing\n> * Second item";
        let ctx2 = LintContext::new(content2, crate::config::MarkdownFlavor::Standard, None);
        let result2 = rule.check(&ctx2).unwrap();
        assert!(
            result2.is_empty(),
            "List continuations should not be flagged, got: {result2:?}"
        );
    }

    #[test]
    fn test_list_continuation_fix_preserves_indentation() {
        // Ensure fix doesn't break list continuation indentation
        let rule = MD027MultipleSpacesBlockquote::default();

        let content = "> - Item\n>   continuation";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        // Should preserve the list continuation indentation
        assert_eq!(fixed, "> - Item\n>   continuation");
    }

    #[test]
    fn test_non_list_multiple_spaces_still_flagged() {
        // Non-list lines with multiple spaces should still be flagged
        let rule = MD027MultipleSpacesBlockquote::default();

        // Just extra spaces, not a list
        let content = ">  This has extra spaces";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1, "Non-list line should be flagged");
    }

    // =========================================================================
    // list_items config option tests
    // =========================================================================

    #[test]
    fn test_list_items_default_false_skips_list_lines() {
        // rumdl default: list_items=false → list lines in blockquotes are skipped
        let rule = MD027MultipleSpacesBlockquote::default();
        let content = "# Test\n\n>  - item one\n>  - item two\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Default (list_items=false) should skip list-item lines, got {result:?}"
        );
    }

    #[test]
    fn test_list_items_true_flags_unordered_list_lines() {
        // markdownlint-style strict: list_items=true → flag list-item lines
        let rule = MD027MultipleSpacesBlockquote::with_config(MD027Config { list_items: true });
        let content = "# Test\n\n>  - item one\n>  - item two\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(
            result.len(),
            2,
            "list_items=true should flag both list-item lines, got {result:?}"
        );
        assert_eq!(result[0].line, 3);
        assert_eq!(result[1].line, 4);
    }

    #[test]
    fn test_list_items_true_flags_ordered_list_lines() {
        let rule = MD027MultipleSpacesBlockquote::with_config(MD027Config { list_items: true });
        let content = "# Test\n\n>  1. first\n>  2. second\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(
            result.len(),
            2,
            "list_items=true should flag ordered list-item lines, got {result:?}"
        );
    }

    #[test]
    fn test_list_items_true_flags_list_continuation() {
        // Continuation line inside a blockquoted list should also fire
        let rule = MD027MultipleSpacesBlockquote::with_config(MD027Config { list_items: true });
        let content = "# Test\n\n>  - first item\n>  more list-y text\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(
            result.len(),
            2,
            "list_items=true should flag both list-item and continuation, got {result:?}"
        );
    }

    /// Lines `list_items = true` reports, with the markdownlint result for each
    /// document as the expectation.
    #[test]
    fn test_list_items_true_measures_indent_from_the_enclosing_item() {
        let rule = MD027MultipleSpacesBlockquote::with_config(MD027Config { list_items: true });
        for (content, expected) in [
            // Indentation that nests or continues an item is list structure.
            ("> - a\n>   - b\n", vec![]),
            ("> - a\n>\n>   para in a\n", vec![]),
            ("> 1. a\n>    - b\n>      - c\n", vec![]),
            ("> - a\n>     - b\n", vec![]),
            ("> - a\n>   continued\n", vec![]),
            ("> - a\n>\n>    over-indented para\n", vec![]),
            (">> - a\n>>   - b\n", vec![]),
            ("> - a\n>   - b\n>     - c\n>   - d\n", vec![]),
            // Controls: spacing the item structure does not account for.
            ("> - a\n>  - b\n", vec![2]),
            ("> 1. a\n>  2. b\n", vec![2]),
            ("> - a\n>  * b\n", vec![]),
            ("> - a\n>  1. b\n", vec![]),
            ("> 1. a\n>  1) b\n", vec![]),
            ("> text\n>     - b\n", vec![2]),
            (">   1. x\n>     * x\n", vec![1, 2]),
            ("> - a\n>>   x\n", vec![2]),
            ("> - a\n> >   x\n", vec![2]),
            (">  - a\n>   - b\n", vec![1, 2]),
            ("> a\n>   b\n", vec![2]),
            ("> - a\n> text\n>   x\n", vec![]),
            ("> - a\n>\n> text\n>   x\n", vec![4]),
            ("> - a\n\n>   x\n", vec![3]),
            (">>  - a\n>>   - b\n", vec![1, 2]),
            // A block that is not paragraph text ends the list, so what follows
            // is not a lazy continuation of it.
            ("> - a\n> ---\n>   text\n", vec![3]),
            ("> - a\n> * * *\n>   text\n", vec![3]),
            ("> - a\n> ```\n> code\n> ```\n>   text\n", vec![5]),
            ("> - a\n> ~~~\n> code\n> ~~~\n>   text\n", vec![5]),
            ("> -\n> text\n>   text\n", vec![3]),
            ("> - # heading\n> text\n>   text\n", vec![3]),
            // Controls: a fence inside the item, and text lazily continuing it.
            ("> - a\n>   ```\n>   code\n>   ```\n>   text\n", vec![]),
            ("> - a\n>   ```\n>   code\n>   ```\n> text\n>   x\n", vec![6]),
            ("> - a\n> text\n>   more\n", vec![]),
            // A marker that cannot interrupt a paragraph is its text, and a blank
            // line ends an item that is still empty.
            ("> text\n> 2. not a list\n>    bad spacing\n", vec![3]),
            ("> text\n> -\n>   bad spacing\n", vec![3]),
            ("> -\n>\n>   text\n", vec![3]),
            ("> - a\n>> ```\n>> code\n>> ```\n>   bad\n", vec![5]),
            ("> - a\n>   ===\n> text\n>   x\n", vec![4]),
            ("> - a\n> ===\n>   x\n", vec![]),
            ("> - a\n>   --\n> text\n>   bad\n", vec![4]),
            ("> - a | b\n>   --- | ---\n> outside\n>   bad\n", vec![4]),
            ("> `a`\n> -\n>   bad spacing\n", vec![3]),
            ("> a | b\n> --- | --- | ---\n> -\n>   bad spacing\n", vec![4]),
            ("> #hashtag\n> -\n>   bad spacing\n", vec![3]),
            ("> <3 love\n> -\n>   bad spacing\n", vec![3]),
            ("> - <em>a</em>\n>   ===\n> text\n>   bad\n", vec![4]),
            ("> - -\n> outside\n>   bad\n", vec![3]),
            ("> - - # heading\n> outside\n>   bad\n", vec![3]),
            ("> - - a\n>     b\n>   c\n", vec![]),
            ("> - a\n>   b | c\n>   --- | ---\n> outside\n>   bad\n", vec![5]),
            ("> - <span>\n> outside\n>   bad\n", vec![3]),
            ("> p\n>     # fake\n> 2. text\n>    c\n", vec![2, 4]),
            ("> p\nlazy\n    # fake\n> 2. text\n>    c\n", vec![5]),
            ("> p\n>     ---\n> 2. text\n>    c\n", vec![2, 4]),
            ("> a | b\n>     --- | ---\n> 2. x\n>    c\n", vec![2, 4]),
            ("> p\n>     <div>\n> 2. x\n>    c\n", vec![2, 4]),
            ("> p\n>     ```\n> 2. x\n>    c\n", vec![2, 4]),
            ("> - a\n>> q\n>\n>   - b\n>  - c\n", vec![4, 5]),
            ("> - a\n>   > # q\n> text\n>   bad\n", vec![4]),
            ("> - a\n>   > q\n> text\n>   bad\n", vec![]),
            ("# T\n\n> - a\n>   > q\n>\n>   - b\n", vec![]),
            ("# T\n\n> -     code\n>   - b\n", vec![]),
            ("# T\n\n> 1.     code\n>    1. b\n>  - x\n", vec![]),
            ("> 1.     code\n>  2. x\n", vec![2]),
            ("> -     code\n>  - x\n", vec![2]),
            ("> -     code\n>   - b\n>  - x\n", vec![3]),
            ("> - a\n>   > q\n>\n>   - b\n>  - c\n", vec![5]),
            ("> - a\n>   > q\n>   > - n\n>\n>   - b\n", vec![]),
            ("> text\nlazy\n> 2. not a list\n>    bad\n", vec![4]),
            ("> text\n\n> 2. a\n>    ok\n", vec![]),
            ("> a | b\n> --- | :\n> -\n>   bad\n", vec![4]),
            ("> a | b\n> --- | ---\n> -\n>   bad\n", vec![]),
            ("> text\n> ``` foo`bar\n> -\n>   bad\n", vec![4]),
            ("> text\n> ~~~ foo`bar\n> -\n>   bad\n", vec![]),
            ("> text\n> - <span>\n>   inside\n>   bad\n", vec![]),
            ("> text\n> - <span>\n> outside\n>   bad\n", vec![4]),
            ("> text\n> <span>\n> -\n>   bad\n", vec![4]),
            ("> - <div>\n>   ===\n> text\n>   bad\n", vec![4]),
            ("> - a \\| b | c\n>   --- | ---\n> outside\n>   bad\n", vec![4]),
            ("> - a | b\n>   --- | ---\n>   c | d\n> outside\n>   bad\n", vec![5]),
            ("> - a | b\n> --- | ---\n> text\n>   x\n", vec![]),
            ("> - a\n>   -\n> text\n>   bad\n", vec![4]),
            ("> - a\n>   2. b\n>      c\n", vec![]),
            ("> text\n> 1. is a list\n>    c\n", vec![]),
            ("> -\n>   text\n", vec![]),
            // A fence outside the quote ends it, and a tab pads a marker to the next
            // multiple of four columns.
            ("> - a\n```\ncode\n```\n>   text\n", vec![5]),
            ("> 1.\ta\n>\n>    b\n", vec![3]),
            ("> 1.\ta\n>\n>        b\n", vec![]),
            ("> - a\n>\t- b\n", vec![]),
        ] {
            let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
            let lines: Vec<usize> = rule.check(&ctx).unwrap().iter().map(|w| w.line).collect();
            assert_eq!(lines, expected, "{content:?}");
        }
    }

    #[test]
    fn test_list_items_default_skips_continuation() {
        // Continuation line inside a blockquoted list is skipped by default
        let rule = MD027MultipleSpacesBlockquote::default();
        let content = "# Test\n\n>  - first item\n>  more list-y text\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Default should skip both list-item and continuation, got {result:?}"
        );
    }

    #[test]
    fn test_plain_blockquote_text_flagged_in_both_modes() {
        let content = "# Test\n\n>  Plain blockquote text with extra space.\n";
        for cfg in [MD027Config { list_items: false }, MD027Config { list_items: true }] {
            let rule = MD027MultipleSpacesBlockquote::with_config(cfg.clone());
            let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
            let result = rule.check(&ctx).unwrap();
            assert_eq!(
                result.len(),
                1,
                "Plain blockquote text with extra spaces should always be flagged (cfg={cfg:?}), got {result:?}"
            );
        }
    }

    #[test]
    fn test_md027_config_kebab_case_parses() {
        let toml_str = r#"
            list-items = true
        "#;
        let config: MD027Config = toml::from_str(toml_str).unwrap();
        assert!(config.list_items);
    }

    #[test]
    fn test_md027_config_snake_case_alias_parses() {
        let toml_str = r#"
            list_items = true
        "#;
        let config: MD027Config = toml::from_str(toml_str).unwrap();
        assert!(config.list_items);
    }

    #[test]
    fn test_md027_config_default_is_false() {
        let cfg = MD027Config::default();
        assert!(!cfg.list_items, "rumdl default for list_items should be false");
    }

    #[test]
    fn test_md027_html_comment() {
        let rule = MD027MultipleSpacesBlockquote::default();
        let content = "<!--\n>  comment\n-->";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "MD027 should not flag blockquotes inside HTML comments: {result:?}"
        );
    }

    #[test]
    fn test_lazy_line_indent_that_changes_the_parse_is_reported_but_not_fixed() {
        let rule = MD027MultipleSpacesBlockquote::default();
        // Four or more columns past the marker, the line continues the
        // paragraph. One space would make it a break, heading, list or fence.
        for content in [
            "> o\n>     ---\n",
            "> o\n>     ===\n",
            "> o\n>     # x\n",
            "> o\n>     -  x\n",
            "> o\n>     ```\n",
            "> > o\n> >     ---\n",
        ] {
            let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
            let warnings = rule.check(&ctx).unwrap();
            assert!(!warnings.is_empty(), "{content:?} must still be reported");
            assert!(
                warnings.iter().all(|w| w.fix.is_none()),
                "{content:?} must carry no fix: {warnings:?}"
            );
            assert_eq!(rule.fix(&ctx).unwrap(), content, "{content:?}");
        }
    }

    #[test]
    fn test_lazy_line_indent_on_plain_prose_is_still_fixed() {
        let rule = MD027MultipleSpacesBlockquote::default();
        let content = "> o\n>     more text\n>     and more\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        assert_eq!(rule.fix(&ctx).unwrap(), "> o\n> more text\n> and more\n");
    }
}
