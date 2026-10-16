//! Inline configuration comment handling for markdownlint compatibility
//!
//! Supports:
//! - `<!-- markdownlint-disable -->` - Disable all rules from this point
//! - `<!-- markdownlint-enable -->` - Re-enable all rules from this point
//! - `<!-- markdownlint-disable MD001 MD002 -->` - Disable specific rules
//! - `<!-- markdownlint-enable MD001 MD002 -->` - Re-enable specific rules
//! - `<!-- markdownlint-disable-line MD001 -->` - Disable rules for current line
//! - `<!-- markdownlint-disable-next-line MD001 -->` - Disable rules for next line
//! - `<!-- markdownlint-capture -->` - Capture current configuration state
//! - `<!-- markdownlint-restore -->` - Restore captured configuration state
//! - `<!-- markdownlint-disable-file -->` - Disable all rules for entire file
//! - `<!-- markdownlint-enable-file -->` - Re-enable all rules for entire file
//! - `<!-- markdownlint-disable-file MD001 MD002 -->` - Disable specific rules for entire file
//! - `<!-- markdownlint-enable-file MD001 MD002 -->` - Re-enable specific rules for entire file
//! - `<!-- markdownlint-configure-file { "MD013": { "line_length": 120 } } -->` - Configure rules for entire file
//! - `<!-- prettier-ignore -->` - Disable all rules for next line (compatibility with prettier)
//!
//! Also supports rumdl-specific syntax with same semantics.
//!
//! `configure-file` differs from every other directive in two ways: its comment
//! may span multiple lines, and a rule may be given a boolean instead of an
//! options object to turn it off (`false`) or on (`true`) for the whole file.
//! Directives inside fenced code blocks are ignored.

use crate::config::MarkdownFlavor;
use crate::markdownlint_config::markdownlint_to_rumdl_rule_key;
use crate::utils::code_block_utils::CodeBlockUtils;
use serde_json::Value as JsonValue;
use std::collections::{HashMap, HashSet};

/// Normalize a rule name to its canonical form (e.g., "line-length" -> "MD013").
/// If the rule name is not recognized, returns it uppercase (for forward compatibility).
pub(crate) fn normalize_rule_name(rule: &str) -> String {
    markdownlint_to_rumdl_rule_key(rule).map_or_else(|| rule.to_uppercase(), std::string::ToString::to_string)
}

/// Whether this source can contain a recognized inline configuration comment.
/// This is a candidate check only; parsing still decides whether it is valid,
/// inside code, or applicable to the document.
pub fn has_inline_config_markers(content: &str) -> bool {
    let mut quotes = BlockquoteTracker::new();
    memchr::memmem::find_iter(content.as_bytes(), COMMENT_OPEN.as_bytes()).any(|offset| {
        let after_open = comment_body(
            &content[offset + COMMENT_OPEN.len()..],
            quotes.quote_depth_at(content, offset),
        );
        after_open.starts_with("rumdl-")
            || after_open.starts_with("markdownlint-")
            || prettier_ignore_len(after_open).is_some()
    })
}

/// Candidate option overrides use the native directive body and quote handling.
/// Invalid comments may be candidates; a valid override must never be missed.
pub(crate) fn inline_rule_option_candidate_offsets(content: &str) -> impl Iterator<Item = usize> + '_ {
    let mut quotes = BlockquoteTracker::new();
    memchr::memmem::find_iter(content.as_bytes(), COMMENT_OPEN.as_bytes()).filter(move |&offset| {
        let body = comment_body(
            &content[offset + COMMENT_OPEN.len()..],
            quotes.quote_depth_at(content, offset),
        );
        TOOL_PREFIXES.iter().any(|prefix| {
            body.strip_prefix(prefix)
                .is_some_and(|tail| tail.starts_with(CONFIGURE_FILE_KEYWORD))
        })
    })
}

/// The opener of an HTML comment.
const COMMENT_OPEN: &str = "<!--";

/// Whether `ch` is whitespace inside a comment.
///
/// The body of an HTML comment is free text, and the tools whose directives rumdl
/// reads (markdownlint, remark-lint) split and trim it with JavaScript `\s`, which
/// is Unicode White_Space plus U+FEFF. `char::is_whitespace` is White_Space alone,
/// so U+FEFF is added: the union accepts everything either side accepts. U+200B
/// is whitespace in neither and separates nothing. Every helper that reads a
/// comment uses this one class.
pub(crate) fn is_comment_space(ch: char) -> bool {
    ch.is_whitespace() || ch == '\u{feff}'
}

/// The text of a comment after its `<!--` opener and any whitespace that follows
/// it, so `<!--rumdl-disable -->`, `<!-- rumdl-disable -->` and a tab- or
/// multi-space-separated opener all lead to the same directive text.
///
/// `after_open` is the text immediately after `<!--`. When the comment opens
/// inside `depth` levels of blockquote and goes on to a later line, that many
/// blockquote markers at the start of the line are not part of it either (`> <!--`
/// then `> rumdl-disable`). Any further `>` is comment text.
fn comment_body(after_open: &str, depth: usize) -> &str {
    let mut line_lead = false;
    let mut markers_left = depth;
    for (idx, ch) in after_open.char_indices() {
        match ch {
            '\n' => {
                line_lead = true;
                markers_left = depth;
            }
            '>' if line_lead && markers_left > 0 => markers_left -= 1,
            _ if is_comment_space(ch) => {}
            _ => return &after_open[idx..],
        }
    }
    ""
}

/// A line of a comment that continues from an earlier one, without the
/// whitespace that opens it and, in a blockquote, its markers.
fn comment_continuation(line: &str, depth: usize) -> &str {
    let mut markers_left = depth;
    line.trim_start_matches(|ch: char| {
        if ch == '>' && markers_left > 0 {
            markers_left -= 1;
            return true;
        }
        is_comment_space(ch)
    })
}

/// Whether a line begins with container syntax that includes a blockquote marker:
/// a run of `>` markers, `-`/`+`/`*` bullets and `1.`/`1)` numbers, each bullet or
/// number followed by whitespace, with at least one `>`. Anything else ends the
/// prefix, so `1 > 0 <!--` is prose, not a quote.
fn line_quote_depth(line: &str) -> usize {
    let mut rest = line;
    let mut depth = 0;
    loop {
        rest = rest.trim_start_matches([' ', '\t']);
        if let Some(after) = rest.strip_prefix('>') {
            depth += 1;
            rest = after;
            continue;
        }
        let after_marker = if let Some(after) = rest.strip_prefix(['-', '+', '*']) {
            Some(after)
        } else {
            let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
            if (1..=9).contains(&digits) {
                rest[digits..].strip_prefix(['.', ')'])
            } else {
                None
            }
        };
        match after_marker {
            Some(after) if after.starts_with([' ', '\t']) => rest = after,
            _ => return depth,
        }
    }
}

/// Answers whether the comment opening at a byte offset sits in a blockquote: the
/// container prefix of its line (the leading run of blockquote `>` markers, list
/// bullets and numbers) holds at least one `>`. Prose between the prefix and the
/// opener does not matter.
///
/// Openers are asked about in increasing order. Each question looks only at the
/// text since the previous one, and the answer is worked out once per line, so a
/// long line of comments costs no more than the line.
struct BlockquoteTracker {
    scanned_to: usize,
    /// Whether the line at `scanned_to` has been classified yet
    classified: bool,
    depth: usize,
}

impl BlockquoteTracker {
    fn new() -> Self {
        Self {
            scanned_to: 0,
            classified: false,
            depth: 0,
        }
    }

    fn quote_depth_at(&mut self, text: &str, open: usize) -> usize {
        let line_start = match text[self.scanned_to..open].rfind('\n') {
            Some(newline) => Some(self.scanned_to + newline + 1),
            None if !self.classified => Some(0),
            None => None,
        };
        if let Some(line_start) = line_start {
            self.depth = line_quote_depth(&text[line_start..]);
            self.classified = true;
        }
        self.scanned_to = open;
        self.depth
    }
}

/// Byte length of a `prettier-ignore` comment body (`body` is the text after
/// `<!--` and its whitespace) through its closing `-->`, or `None` when the body
/// is not exactly `prettier-ignore` followed by optional whitespace and `-->`.
/// `prettier-ignore-start` and `-end` are other directives and do not match.
fn prettier_ignore_len(body: &str) -> Option<usize> {
    const KEYWORD: &str = "prettier-ignore";
    let after_keyword = body.strip_prefix(KEYWORD)?;
    let after_space = after_keyword.trim_start_matches(is_comment_space);
    let after_close = after_space.strip_prefix("-->")?;
    Some(body.len() - after_close.len())
}

/// Whether a comment body (the text after `<!--` and its whitespace) is a linter
/// configuration comment. Vale and remark-lint have no common prefix, so each is
/// matched by its own syntax with a word boundary after every keyword:
/// `vale offset ...` and `lint disabled ...` are prose.
fn is_tool_comment_body(body: &str, depth: usize) -> bool {
    TOOL_PREFIXES.iter().any(|tool| body.starts_with(tool)) || is_vale_body(body, depth) || is_lint_body(body, depth)
}

/// Whether `rest` ends a word: nothing, whitespace, or the comment's closer.
fn at_word_end(rest: &str) -> bool {
    rest.is_empty() || rest.starts_with(is_comment_space) || rest.starts_with("-->")
}

/// `vale off`, `vale on`, or a setting such as `vale Style.Rule = NO`.
fn is_vale_body(body: &str, depth: usize) -> bool {
    let Some(rest) = body.strip_prefix("vale") else {
        return false;
    };
    if !rest.starts_with(is_comment_space) {
        return false;
    }
    let rest = comment_body(rest, depth);
    if ["off", "on"]
        .iter()
        .any(|keyword| rest.strip_prefix(keyword).is_some_and(at_word_end))
    {
        return true;
    }
    let name_end = rest
        .find(|ch: char| is_comment_space(ch) || ch == '=')
        .unwrap_or(rest.len());
    name_end > 0 && comment_body(&rest[name_end..], depth).starts_with('=')
}

/// `lint disable`, `lint enable` or `lint ignore`.
fn is_lint_body(body: &str, depth: usize) -> bool {
    let Some(rest) = body.strip_prefix("lint") else {
        return false;
    };
    if !rest.starts_with(is_comment_space) {
        return false;
    }
    let rest = comment_body(rest, depth);
    ["disable", "enable", "ignore"]
        .iter()
        .any(|keyword| rest.strip_prefix(keyword).is_some_and(at_word_end))
}

/// The byte ranges of the linter configuration comments `in_code` does not call
/// code, each from its `<!--` through its `-->` (or to the end of its line when
/// nothing closes it), in document order.
///
/// A rule that must leave directive text alone skips what falls inside these
/// ranges. Prose outside the comment, before its opener or after its `-->`, stays
/// the rule's, and a comment in code is an example rather than a directive.
pub(crate) fn config_comment_ranges(content: &str, in_code: impl Fn(usize) -> bool) -> Vec<std::ops::Range<usize>> {
    let mut ranges = Vec::new();
    let mut pos = 0;
    let mut quotes = BlockquoteTracker::new();
    // The first `-->` at or after the last search start, and whether any remains.
    // A closer is searched for again only once the scan has passed it, and never
    // after a search found none, so openers that share a closer (or have none)
    // cost one search between them.
    let mut next_close: Option<usize> = None;
    let mut closers_left = true;
    while let Some(open_offset) = content[pos..].find(COMMENT_OPEN) {
        let comment_start = pos + open_offset;
        let depth = quotes.quote_depth_at(content, comment_start);
        let body = comment_body(&content[comment_start + COMMENT_OPEN.len()..], depth);
        pos = comment_start + COMMENT_OPEN.len();
        if !is_tool_comment_body(body, depth) || in_code(comment_start) {
            continue;
        }
        let body_start = content.len() - body.len();
        if closers_left && next_close.is_none_or(|close| close < body_start) {
            next_close = body.find("-->").map(|close| body_start + close);
            closers_left = next_close.is_some();
        }
        let comment_end = match next_close {
            Some(close) => close + 3,
            None => content[comment_start..]
                .find('\n')
                .map_or(content.len(), |newline| comment_start + newline),
        };
        pos = comment_end;
        ranges.push(comment_start..comment_end);
    }
    ranges
}

/// Whether `offset` falls inside one of `ranges`, as `config_comment_ranges`
/// returns them (sorted, disjoint). Found by binary search, so a rule asking about
/// every match of a large document does not scan the whole inventory each time.
pub(crate) fn offset_in_ranges(ranges: &[std::ops::Range<usize>], offset: usize) -> bool {
    let after = ranges.partition_point(|range| range.end <= offset);
    ranges.get(after).is_some_and(|range| range.start <= offset)
}

/// Type alias for the export_for_file_index return type:
/// (file_disabled_rules, persistent_transitions, line_disabled_rules)
pub type FileIndexExport = (
    HashSet<String>,
    Vec<(usize, HashSet<String>, HashSet<String>)>,
    HashMap<usize, HashSet<String>>,
);

/// A state transition recording which rules are disabled/enabled starting at a given line.
/// Transitions are stored in ascending line order. The state at any line is determined by
/// the most recent transition at or before that line.
#[derive(Debug, Clone)]
struct StateTransition {
    /// The 1-indexed line number where this state takes effect
    line: usize,
    /// The set of disabled rules at this point ("*" means all rules disabled)
    disabled: HashSet<String>,
    /// The set of explicitly enabled rules (only meaningful when disabled contains "*")
    enabled: HashSet<String>,
}

/// The kind of directive a rule's disabled state at a line comes from.
///
/// The three layers are resolved independently of each other, so removing a
/// comment from one of them leaves the other two answering exactly as before.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisableLayer {
    /// `disable-file`, or a `configure-file` entry set to `false`
    File,
    /// `disable`, up to the `enable` that closes it
    Block,
    /// `disable-line`, `disable-next-line` or `prettier-ignore`
    Line,
}

#[derive(Debug, Clone)]
pub struct InlineConfig {
    /// State transitions for persistent disable/enable directives, sorted by line number.
    /// Only stores entries where the state actually changes, not for every line.
    transitions: Vec<StateTransition>,
    /// Rules disabled for specific lines via disable-line (1-indexed)
    line_disabled_rules: HashMap<usize, HashSet<String>>,
    /// Rules disabled for the entire file
    file_disabled_rules: HashSet<String>,
    /// Rules explicitly enabled for the entire file (used when all rules are disabled)
    file_enabled_rules: HashSet<String>,
    /// Configuration overrides for specific rules from configure-file comments
    /// Maps rule name to configuration JSON value
    file_rule_config: HashMap<String, JsonValue>,
}

impl Default for InlineConfig {
    fn default() -> Self {
        Self::new()
    }
}

impl InlineConfig {
    pub fn new() -> Self {
        Self {
            transitions: Vec::new(),
            line_disabled_rules: HashMap::new(),
            file_disabled_rules: HashSet::new(),
            file_enabled_rules: HashSet::new(),
            file_rule_config: HashMap::new(),
        }
    }

    /// Find the state transition that applies to the given line number.
    /// Uses binary search to find the last transition at or before the given line.
    fn find_transition(&self, line_number: usize) -> Option<&StateTransition> {
        if self.transitions.is_empty() {
            return None;
        }
        // Binary search for the rightmost transition with line <= line_number
        match self.transitions.binary_search_by_key(&line_number, |t| t.line) {
            Ok(idx) => Some(&self.transitions[idx]),
            Err(idx) => {
                if idx > 0 {
                    Some(&self.transitions[idx - 1])
                } else {
                    None
                }
            }
        }
    }

    /// Process all inline comments in the content and return the configuration state
    pub fn from_content(content: &str) -> Self {
        if !has_inline_config_markers(content) {
            return Self::new();
        }

        // One parse answers for both: a directive inside a code block, and one
        // inside an inline code span, each configure nothing.
        let parsed = CodeBlockUtils::detect_code_blocks_and_spans(content);
        Self::from_content_with_code_blocks_internal(content, &parsed.code_blocks, &parsed.code_spans)
    }

    /// Process all inline comments in the content with precomputed code ranges.
    pub fn from_content_with_code_blocks(
        content: &str,
        code_blocks: &[(usize, usize)],
        code_spans: &[(usize, usize)],
    ) -> Self {
        if !has_inline_config_markers(content) {
            return Self::new();
        }

        Self::from_content_with_code_blocks_internal(content, code_blocks, code_spans)
    }

    fn from_content_with_code_blocks_internal(
        content: &str,
        code_blocks: &[(usize, usize)],
        code_spans: &[(usize, usize)],
    ) -> Self {
        let mut config = Self::new();
        // Lines measured over `split('\n')`, whose pieces keep any `\r`, so the
        // offsets below match the code ranges for a CRLF document too.

        // configure-file is scanned over the whole document rather than per
        // line, because it is the one directive allowed to span lines, and it
        // applies before any enable/disable directive regardless of where it
        // sits. Comments inside fenced code blocks are skipped.
        for (offset, json_config) in scan_configure_file_comments(content) {
            if offset_in_code_block(offset, code_blocks) || offset_in_code_span(offset, code_spans) {
                continue;
            }
            let Some(obj) = json_config.as_object() else {
                continue;
            };
            for (rule_name, rule_config) in obj {
                // A boolean turns a rule off or back on, e.g.
                // `{ "no-trailing-spaces": false }`, so route those to the
                // disable set instead of storing them as rule options.
                let normalized = normalize_rule_name(rule_name);
                if let Some(enabled) = rule_config.as_bool() {
                    if enabled {
                        config.file_disabled_rules.remove(&normalized);
                    } else {
                        config.file_disabled_rules.insert(normalized);
                    }
                    continue;
                }
                // Store under the canonical rule id so lookups by `MDxxx` also
                // find configs written with an alias, e.g.
                // `{ "line-length": { "line_length": 70 } }`.
                config.file_rule_config.insert(normalized, rule_config.clone());
            }
        }

        // Track current state of disabled rules
        let mut currently_disabled: HashSet<String> = HashSet::new();
        let mut currently_enabled: HashSet<String> = HashSet::new();
        let mut capture_stack: Vec<(HashSet<String>, HashSet<String>)> = Vec::new();

        // Track the previously recorded transition state to detect changes
        let mut prev_disabled: HashSet<String> = HashSet::new();
        let mut prev_enabled: HashSet<String> = HashSet::new();

        // Record initial state (line 1: nothing disabled)
        config.transitions.push(StateTransition {
            line: 1,
            disabled: HashSet::new(),
            enabled: HashSet::new(),
        });

        // Directive comments that span lines take effect on the line holding their
        // `-->`, ahead of any comment later on that line.
        let mut multiline_by_end_line: HashMap<usize, Vec<MultilineDirective<'_>>> = HashMap::new();
        let in_code = CommentCodeTest::new(content, code_blocks, code_spans);
        for directive in scan_multiline_directives(content, |open| in_code.contains(open)) {
            multiline_by_end_line
                .entry(directive.end_line)
                .or_default()
                .push(directive);
        }

        let mut position = 0;
        let mut line_count = 0;
        for (idx, line) in content.split('\n').enumerate() {
            let line_num = idx + 1; // 1-indexed
            line_count = line_num;
            let line_position = position;
            position += line.len() + 1;

            // Record a transition only if state changed since last recorded transition.
            // State for this line is the state BEFORE processing comments on this line.
            if currently_disabled != prev_disabled || currently_enabled != prev_enabled {
                config.transitions.push(StateTransition {
                    line: line_num,
                    disabled: currently_disabled.clone(),
                    enabled: currently_enabled.clone(),
                });
                prev_disabled.clone_from(&currently_disabled);
                prev_enabled.clone_from(&currently_enabled);
            }

            // A line inside a code block holds no single-line directive
            let in_code_block = line_in_code_block(line_position, line, code_blocks);

            // Parse all directives on this line once via the unified parser.
            // Directives come back in left-to-right order with correct disambiguation.
            let ending_here = multiline_by_end_line.remove(&line_num).unwrap_or_default();
            let single_line = if in_code_block {
                Vec::new()
            } else {
                directives_outside_code_spans(line, line_position, code_spans)
            };
            let directives: Vec<InlineDirective<'_>> = ending_here
                .into_iter()
                .map(|multiline| InlineDirective {
                    kind: multiline.kind,
                    rules: multiline.rules,
                    span: 0..0,
                })
                .chain(single_line)
                .collect();

            // Also check for prettier-ignore (not part of the rumdl/markdownlint format)
            let has_prettier_ignore = !in_code_block
                && line.match_indices(COMMENT_OPEN).any(|(offset, _)| {
                    let body = comment_body(&line[offset + COMMENT_OPEN.len()..], 0);
                    prettier_ignore_len(body).is_some() && !offset_in_code_span(line_position + offset, code_spans)
                });

            // Pass 1: file-wide directives (affect the entire file, not state-tracked)
            for directive in &directives {
                match directive.kind {
                    DirectiveKind::DisableFile => {
                        if directive.rules.is_empty() {
                            config.file_disabled_rules.clear();
                            config.file_disabled_rules.insert("*".to_string());
                        } else if config.file_disabled_rules.contains("*") {
                            for rule in &directive.rules {
                                config.file_enabled_rules.remove(&normalize_rule_name(rule));
                            }
                        } else {
                            for rule in &directive.rules {
                                config.file_disabled_rules.insert(normalize_rule_name(rule));
                            }
                        }
                    }
                    DirectiveKind::EnableFile => {
                        if directive.rules.is_empty() {
                            config.file_disabled_rules.clear();
                            config.file_enabled_rules.clear();
                        } else if config.file_disabled_rules.contains("*") {
                            for rule in &directive.rules {
                                config.file_enabled_rules.insert(normalize_rule_name(rule));
                            }
                        } else {
                            for rule in &directive.rules {
                                config.file_disabled_rules.remove(&normalize_rule_name(rule));
                            }
                        }
                    }
                    // configure-file is handled document-wide before this loop.
                    _ => {}
                }
            }

            // Pass 2: line-specific and state-changing directives (in document order)
            for directive in &directives {
                match directive.kind {
                    DirectiveKind::DisableNextLine => {
                        let next_line = line_num + 1;
                        let line_rules = config.line_disabled_rules.entry(next_line).or_default();
                        if directive.rules.is_empty() {
                            line_rules.insert("*".to_string());
                        } else {
                            for rule in &directive.rules {
                                line_rules.insert(normalize_rule_name(rule));
                            }
                        }
                    }
                    DirectiveKind::DisableLine => {
                        let line_rules = config.line_disabled_rules.entry(line_num).or_default();
                        if directive.rules.is_empty() {
                            line_rules.insert("*".to_string());
                        } else {
                            for rule in &directive.rules {
                                line_rules.insert(normalize_rule_name(rule));
                            }
                        }
                    }
                    DirectiveKind::Disable => {
                        if directive.rules.is_empty() {
                            currently_disabled.clear();
                            currently_disabled.insert("*".to_string());
                            currently_enabled.clear();
                        } else if currently_disabled.contains("*") {
                            for rule in &directive.rules {
                                currently_enabled.remove(&normalize_rule_name(rule));
                            }
                        } else {
                            for rule in &directive.rules {
                                currently_disabled.insert(normalize_rule_name(rule));
                            }
                        }
                    }
                    DirectiveKind::Enable => {
                        if directive.rules.is_empty() {
                            currently_disabled.clear();
                            currently_enabled.clear();
                        } else if currently_disabled.contains("*") {
                            for rule in &directive.rules {
                                currently_enabled.insert(normalize_rule_name(rule));
                            }
                        } else {
                            for rule in &directive.rules {
                                currently_disabled.remove(&normalize_rule_name(rule));
                            }
                        }
                    }
                    DirectiveKind::Capture => {
                        capture_stack.push((currently_disabled.clone(), currently_enabled.clone()));
                    }
                    DirectiveKind::Restore => {
                        if let Some((disabled, enabled)) = capture_stack.pop() {
                            currently_disabled = disabled;
                            currently_enabled = enabled;
                        }
                    }
                    // File-wide directives already handled in pass 1
                    DirectiveKind::DisableFile | DirectiveKind::EnableFile | DirectiveKind::ConfigureFile => {}
                }
            }

            // prettier-ignore: disables all rules for next line
            if has_prettier_ignore {
                let next_line = line_num + 1;
                let line_rules = config.line_disabled_rules.entry(next_line).or_default();
                line_rules.insert("*".to_string());
            }
        }

        // Record final transition if state changed after the last line was processed
        if currently_disabled != prev_disabled || currently_enabled != prev_enabled {
            config.transitions.push(StateTransition {
                line: line_count + 1,
                disabled: currently_disabled,
                enabled: currently_enabled,
            });
        }

        config
    }

    /// Check if a rule is disabled at a specific line
    pub fn is_rule_disabled(&self, rule_name: &str, line_number: usize) -> bool {
        self.disabling_layer(rule_name, line_number).is_some()
    }

    /// Which kind of directive keeps `rule_name` off at `line_number`, if any.
    ///
    /// A file-wide disable answers first: it decides on its own, so a rule enabled
    /// for the file stays enabled however the line below is written. The remaining
    /// two layers both merely disable, so the wider one answers, naming the
    /// directive a caller would have to remove to see the rule report again.
    pub fn disabling_layer(&self, rule_name: &str, line_number: usize) -> Option<DisableLayer> {
        if self.file_disabled_rules.contains("*") {
            // All rules are disabled for the file, check if this rule is explicitly enabled
            return (!self.file_enabled_rules.contains(rule_name)).then_some(DisableLayer::File);
        } else if self.file_disabled_rules.contains(rule_name) {
            return Some(DisableLayer::File);
        }

        // Persistent disables via state transitions (binary search)
        if let Some(transition) = self.find_transition(line_number) {
            let disabled = if transition.disabled.contains("*") {
                !transition.enabled.contains(rule_name)
            } else {
                transition.disabled.contains(rule_name)
            };
            if disabled {
                return Some(DisableLayer::Block);
            }
        }

        // Line-specific disables (disable-line, disable-next-line, prettier-ignore)
        if let Some(line_rules) = self.line_disabled_rules.get(&line_number)
            && (line_rules.contains("*") || line_rules.contains(rule_name))
        {
            return Some(DisableLayer::Line);
        }

        None
    }

    /// Get all disabled rules at a specific line
    pub fn get_disabled_rules(&self, line_number: usize) -> HashSet<String> {
        let mut disabled = HashSet::new();

        // Add persistent disables via state transitions (binary search)
        if let Some(transition) = self.find_transition(line_number) {
            if transition.disabled.contains("*") {
                disabled.insert("*".to_string());
            } else {
                for rule in &transition.disabled {
                    disabled.insert(rule.clone());
                }
            }
        }

        // Add line-specific disables
        if let Some(line_rules) = self.line_disabled_rules.get(&line_number) {
            for rule in line_rules {
                disabled.insert(rule.clone());
            }
        }

        disabled
    }

    /// Get configuration overrides for a specific rule from configure-file comments
    pub fn get_rule_config(&self, rule_name: &str) -> Option<&JsonValue> {
        self.file_rule_config.get(rule_name)
    }

    /// Get all configuration overrides from configure-file comments
    pub fn get_all_rule_configs(&self) -> &HashMap<String, JsonValue> {
        &self.file_rule_config
    }

    /// Export the disabled rules data for storage in FileIndex.
    ///
    /// Returns (file_disabled_rules, persistent_transitions, line_disabled_rules).
    pub fn export_for_file_index(&self) -> FileIndexExport {
        let file_disabled = self.file_disabled_rules.clone();

        let persistent_transitions: Vec<(usize, HashSet<String>, HashSet<String>)> = self
            .transitions
            .iter()
            .map(|t| (t.line, t.disabled.clone(), t.enabled.clone()))
            .collect();

        let line_disabled = self.line_disabled_rules.clone();

        (file_disabled, persistent_transitions, line_disabled)
    }
}

// ── Unified inline directive parser ──────────────────────────────────────────
//
// All inline config comments follow one pattern:
//   <!-- (rumdl|markdownlint)-KEYWORD [RULES...] -->
//
// Disambiguation (e.g., "disable" vs "disable-line" vs "disable-next-line")
// is handled ONCE here by matching the longest keyword first.

/// The type of an inline configuration directive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DirectiveKind {
    Disable,
    DisableLine,
    DisableNextLine,
    DisableFile,
    Enable,
    EnableFile,
    Capture,
    Restore,
    ConfigureFile,
}

/// A parsed inline configuration directive.
#[derive(Debug, Clone, PartialEq)]
pub struct InlineDirective<'a> {
    pub kind: DirectiveKind,
    pub rules: Vec<&'a str>,
    /// Byte range of the whole comment, `<!--` through `-->`, within its line
    pub span: std::ops::Range<usize>,
}

/// Tool prefixes recognized in inline config comments.
const TOOL_PREFIXES: &[&str] = &["rumdl-", "markdownlint-"];

/// Directive keywords ordered so that more-specific prefixes come first.
/// "disable-next-line" before "disable-line" before "disable-file" before "disable";
/// "enable-file" before "enable". This ensures longest-match-first disambiguation.
const DIRECTIVE_KEYWORDS: &[(DirectiveKind, &str)] = &[
    (DirectiveKind::DisableNextLine, "disable-next-line"),
    (DirectiveKind::DisableLine, "disable-line"),
    (DirectiveKind::DisableFile, "disable-file"),
    (DirectiveKind::Disable, "disable"),
    (DirectiveKind::EnableFile, "enable-file"),
    (DirectiveKind::Enable, "enable"),
    (DirectiveKind::ConfigureFile, "configure-file"),
    (DirectiveKind::Capture, "capture"),
    (DirectiveKind::Restore, "restore"),
];

/// Try to parse a single directive from the text after `<!--` and its whitespace.
/// Returns the kind, its rule list, and the number of bytes consumed (from `s`
/// onward) so the caller can advance past `-->` and place the comment's span.
fn try_parse_directive(s: &str, depth: usize) -> Option<(DirectiveKind, Vec<&str>, usize)> {
    for tool in TOOL_PREFIXES {
        if !s.starts_with(tool) {
            continue;
        }
        let after_tool = &s[tool.len()..];

        for &(kind, keyword) in DIRECTIVE_KEYWORDS {
            if !after_tool.starts_with(keyword) {
                continue;
            }
            let after_kw = &after_tool[keyword.len()..];

            // Word boundary: the keyword must be followed by whitespace, `-->`, or end-of-string.
            // This prevents "disablefoo" from matching "disable".
            if !after_kw.is_empty() && !after_kw.starts_with(is_comment_space) && !after_kw.starts_with("-->") {
                continue;
            }

            // Find closing -->
            let close_offset = after_kw.find("-->")?;

            let mut lines = after_kw[..close_offset].split('\n');
            let first_line = lines.next().unwrap_or("");
            let rules: Vec<&str> = std::iter::once(first_line)
                .chain(lines.map(|line| comment_continuation(line, depth)))
                .flat_map(|line| line.split(is_comment_space))
                .filter(|rule| !rule.is_empty())
                .collect();

            let consumed = tool.len() + keyword.len() + close_offset + 3; // 3 for "-->"
            return Some((kind, rules, consumed));
        }

        // Tool prefix matched but no keyword — not a directive we recognize.
        return None;
    }
    None
}

/// Parse all inline configuration directives from a line, in left-to-right order.
///
/// Each directive is a typed `InlineDirective` with its kind and rule list.
/// Disambiguation between overlapping prefixes (e.g., `disable` vs `disable-line`)
/// is handled by matching the longest keyword first — no ad-hoc guards needed.
pub fn parse_inline_directives(line: &str) -> Vec<InlineDirective<'_>> {
    let mut results = Vec::new();
    let mut pos = 0;

    while pos < line.len() {
        let remaining = &line[pos..];
        let Some(open_offset) = remaining.find(COMMENT_OPEN) else {
            break;
        };
        let comment_start = pos + open_offset;
        let after_open = &line[comment_start + COMMENT_OPEN.len()..];
        let body = comment_body(after_open, 0);
        let body_start = line.len() - body.len();

        if let Some((kind, rules, consumed)) = try_parse_directive(body, 0) {
            let comment_end = body_start + consumed;
            results.push(InlineDirective {
                kind,
                rules,
                span: comment_start..comment_end,
            });
            pos = comment_end;
        } else {
            pos = comment_start + COMMENT_OPEN.len();
        }
    }

    results
}

/// A directive comment whose text spans lines: `<!--` and the keyword, or the
/// rule list and the `-->`, sit on different lines.
///
/// An HTML comment is one block, so its text, line breaks included, is the
/// directive. A comment that fits on one line is left to `parse_inline_directives`.
#[derive(Debug, Clone, PartialEq)]
struct MultilineDirective<'a> {
    kind: DirectiveKind,
    rules: Vec<&'a str>,
    /// Byte offset of the opening `<!--`
    open: usize,
    /// Byte offset just past the closing `-->`
    end: usize,
    /// 1-indexed line the comment opens on
    start_line: usize,
    /// 1-indexed line holding the `-->`: the line the directive takes effect on
    end_line: usize,
}

/// Find every directive comment of `text` that spans lines, in document order.
///
/// Walks the openers the way `parse_inline_directives` does (an opener that is
/// not a directive is stepped over by its four bytes), so a comment is a directive
/// here exactly when it would be on one line. A directive takes effect where its
/// comment ends: `disable` and `enable` from the line after the `-->` line,
/// `disable-line` on it, `disable-next-line` on the one after.
///
/// An opener `in_code` calls code is stepped over by its four bytes like any other
/// non-directive, so an unterminated example in code cannot swallow the real
/// directive that follows it. `in_code` is asked only about comments that span
/// lines, since only those are kept.
fn scan_multiline_directives(text: &str, in_code: impl Fn(usize) -> bool) -> Vec<MultilineDirective<'_>> {
    let mut found = Vec::new();
    let mut pos = 0;
    // Newlines before `counted_to`, so each opener's line is found by counting
    // only the text since the previous one.
    let mut counted_to = 0;
    let mut lines_before = 0;
    // The first `-->` at or after `pos`, found again only once `pos` passes it, so
    // a run of openers that share one closer costs one search.
    let mut next_close: Option<usize> = None;
    // The first newline at or after `pos` (`text.len()` when none), kept the same way.
    let mut next_newline = 0;
    let mut quotes = BlockquoteTracker::new();

    while let Some(open_offset) = text[pos..].find(COMMENT_OPEN) {
        let open = pos + open_offset;
        pos = open + COMMENT_OPEN.len();
        let close = match next_close.filter(|&close| close >= pos) {
            Some(close) => close,
            None => match text[pos..].find("-->") {
                Some(found) => *next_close.insert(pos + found),
                // No comment can close, so no directive follows.
                None => break,
            },
        };
        // A comment in code is ruled out before its text is parsed, so an example
        // of many openers over one closer is not parsed once per opener.
        if next_newline < pos {
            next_newline = text[pos..].find('\n').map_or(text.len(), |newline| pos + newline);
        }
        let spans_lines = next_newline < close;
        if spans_lines && in_code(open) {
            continue;
        }
        let depth = quotes.quote_depth_at(text, open);
        let body = comment_body(&text[pos..], depth);
        let body_start = text.len() - body.len();

        let Some((kind, rules, consumed)) = try_parse_directive(body, depth) else {
            continue;
        };
        let end = body_start + consumed;
        let newlines = text[open..end].matches('\n').count();
        if newlines == 0 {
            pos = end;
            continue;
        }
        pos = end;
        // configure-file has its own document-wide scan.
        if kind == DirectiveKind::ConfigureFile {
            continue;
        }
        lines_before += text[counted_to..open].matches('\n').count();
        counted_to = open;
        let start_line = lines_before + 1;
        found.push(MultilineDirective {
            kind,
            rules,
            open,
            end,
            start_line,
            end_line: start_line + newlines,
        });
    }

    found
}

/// Decides whether a comment opening at a byte offset is code: inside a code block,
/// inside a code span, or on a line an indented code block holds.
///
/// The line answer is kept for the line last asked about, so many openers on one
/// long line cost one look at it between them.
struct CommentCodeTest<'a> {
    content: &'a str,
    code_blocks: &'a [(usize, usize)],
    code_spans: &'a [(usize, usize)],
    /// (start, end, in code) of the line last asked about
    line: std::cell::Cell<Option<(usize, usize, bool)>>,
}

impl<'a> CommentCodeTest<'a> {
    fn new(content: &'a str, code_blocks: &'a [(usize, usize)], code_spans: &'a [(usize, usize)]) -> Self {
        Self {
            content,
            code_blocks,
            code_spans,
            line: std::cell::Cell::new(None),
        }
    }

    fn contains(&self, open: usize) -> bool {
        if offset_in_code_block(open, self.code_blocks) || offset_in_code_span(open, self.code_spans) {
            return true;
        }
        if let Some((start, end, in_code)) = self.line.get()
            && (start..end).contains(&open)
        {
            return in_code;
        }
        let line_start = self.content[..open].rfind('\n').map_or(0, |newline| newline + 1);
        let line_end = self.content[line_start..]
            .find('\n')
            .map_or(self.content.len(), |newline| line_start + newline);
        let in_code = line_in_code_block(line_start, &self.content[line_start..line_end], self.code_blocks);
        self.line.set(Some((line_start, line_end.max(line_start + 1), in_code)));
        in_code
    }
}

/// The directive-kind name a validation warning reports, for the kinds that name
/// rules.
fn directive_comment_type(kind: DirectiveKind) -> Option<&'static str> {
    match kind {
        DirectiveKind::Disable => Some("disable"),
        DirectiveKind::Enable => Some("enable"),
        DirectiveKind::DisableLine => Some("disable-line"),
        DirectiveKind::DisableNextLine => Some("disable-next-line"),
        DirectiveKind::DisableFile => Some("disable-file"),
        DirectiveKind::EnableFile => Some("enable-file"),
        DirectiveKind::ConfigureFile | DirectiveKind::Capture | DirectiveKind::Restore => None,
    }
}

// ── Backward-compatible wrapper functions ────────────────────────────────────
//
// These delegate to parse_inline_directives and filter by DirectiveKind.
// External callers (e.g., MD040) use these; internal code uses the unified parser.

fn find_directive_rules(line: &str, kind: DirectiveKind) -> Option<Vec<&str>> {
    parse_inline_directives(line)
        .into_iter()
        .find(|d| d.kind == kind)
        .map(|d| d.rules)
}

/// Parse a disable comment and return the list of rules (empty vec means all rules)
pub fn parse_disable_comment(line: &str) -> Option<Vec<&str>> {
    find_directive_rules(line, DirectiveKind::Disable)
}

/// Parse an enable comment and return the list of rules (empty vec means all rules)
pub fn parse_enable_comment(line: &str) -> Option<Vec<&str>> {
    find_directive_rules(line, DirectiveKind::Enable)
}

/// Parse a disable-line comment
pub fn parse_disable_line_comment(line: &str) -> Option<Vec<&str>> {
    find_directive_rules(line, DirectiveKind::DisableLine)
}

/// Parse a disable-next-line comment
pub fn parse_disable_next_line_comment(line: &str) -> Option<Vec<&str>> {
    find_directive_rules(line, DirectiveKind::DisableNextLine)
}

/// Parse a disable-file comment and return the list of rules (empty vec means all rules)
pub fn parse_disable_file_comment(line: &str) -> Option<Vec<&str>> {
    find_directive_rules(line, DirectiveKind::DisableFile)
}

/// Parse an enable-file comment and return the list of rules (empty vec means all rules)
pub fn parse_enable_file_comment(line: &str) -> Option<Vec<&str>> {
    find_directive_rules(line, DirectiveKind::EnableFile)
}

/// Check if line contains a capture comment
pub fn is_capture_comment(line: &str) -> bool {
    parse_inline_directives(line)
        .iter()
        .any(|d| d.kind == DirectiveKind::Capture)
}

/// Check if line contains a restore comment
pub fn is_restore_comment(line: &str) -> bool {
    parse_inline_directives(line)
        .iter()
        .any(|d| d.kind == DirectiveKind::Restore)
}

const CONFIGURE_FILE_KEYWORD: &str = "configure-file";

/// Whether a byte offset falls inside one of the given ranges.
fn offset_within(offset: usize, ranges: &[(usize, usize)]) -> bool {
    ranges.iter().any(|&(start, end)| offset >= start && offset < end)
}

/// Whether a byte offset falls inside one of the given code block ranges.
fn offset_in_code_block(offset: usize, code_blocks: &[(usize, usize)]) -> bool {
    offset_within(offset, code_blocks)
}

/// Whether a byte offset falls inside one of the given inline code spans.
///
/// A code span holds literal text, so a directive written between backticks
/// documents a directive rather than writing one. Unlike a code block, a span
/// covers part of a line, so the test is made against the directive's own offset
/// and a live directive beside one in a span still applies.
fn offset_in_code_span(offset: usize, code_spans: &[(usize, usize)]) -> bool {
    offset_within(offset, code_spans)
}

/// The directives on a line that are not inside an inline code span.
///
/// `line_start` is the line's byte offset in the document, which is what makes a
/// directive's span comparable with the document-wide code span ranges.
fn directives_outside_code_spans<'a>(
    line: &'a str,
    line_start: usize,
    code_spans: &[(usize, usize)],
) -> Vec<InlineDirective<'a>> {
    let mut directives = parse_inline_directives(line);
    if !code_spans.is_empty() {
        directives.retain(|directive| !offset_in_code_span(line_start + directive.span.start, code_spans));
    }
    directives
}

/// Whether a directive written on this line sits inside a code block.
///
/// The line is probed at its first non-whitespace byte, because an indented code
/// block's range starts at the indented content rather than at the start of the
/// line. Asking whether the whole line span is contained would answer "no" for
/// every indented block and let a directive written in a code sample configure
/// the document.
fn line_in_code_block(line_start: usize, line: &str, code_blocks: &[(usize, usize)]) -> bool {
    let probe = line
        .find(|c: char| !c.is_whitespace())
        .map_or(line_start, |indent| line_start + indent);
    offset_in_code_block(probe, code_blocks)
}

/// The 1-indexed line a byte offset falls on.
fn line_of_offset(text: &str, offset: usize) -> usize {
    text[..offset].bytes().filter(|&b| b == b'\n').count() + 1
}

/// A warning together with the byte offset of the directive that raised it.
///
/// The offset is what lets a warning be tested against inline code spans, which
/// a line number cannot answer: a span covers part of a line.
type LocatedWarning = (InlineConfigWarning, usize);

/// Drop warnings raised by a directive that code covers, and shed the offsets.
///
/// `InlineConfig` skips directives inside code blocks and inline code spans, so
/// a fenced example or a backticked mention documents a directive rather than
/// writing one, and there is nothing to report about it. The ranges are the
/// flavor's own, so an indented container body a directive does configure keeps
/// its warning. They cost a parse of the document, so they are computed only
/// once there is a warning to filter.
fn drop_warnings_inside_code(
    content: &str,
    flavor: MarkdownFlavor,
    mut warnings: Vec<LocatedWarning>,
) -> Vec<InlineConfigWarning> {
    if warnings.is_empty() {
        return Vec::new();
    }
    let code = crate::lint_context::code_ranges(content, flavor);
    if !code.blocks.is_empty() {
        // Lines measured over `split('\n')`, whose pieces keep any `\r`, so a
        // CRLF document's offsets match the ranges above. A code block is judged
        // by the line, since an indented block's range starts at the indented
        // content rather than at the start of the line.
        let line_spans: Vec<(usize, &str)> = {
            let mut spans = Vec::new();
            let mut start = 0;
            for line in content.split('\n') {
                spans.push((start, line));
                start += line.len() + 1;
            }
            spans
        };

        warnings.retain(|(warning, _)| {
            let Some(&(line_start, line)) = line_spans.get(warning.line_number.saturating_sub(1)) else {
                return true;
            };
            !line_in_code_block(line_start, line, &code.blocks)
        });
    }
    if !code.spans.is_empty() {
        warnings.retain(|&(_, offset)| !offset_in_code_span(offset, &code.spans));
    }

    warnings.into_iter().map(|(warning, _)| warning).collect()
}

/// Find every configure-file comment in `text`, returning each JSON payload
/// with the byte offset of its opening `<!--`.
///
/// `text` is normally the whole document: unlike every other directive, a
/// configure-file comment may span lines, so its `-->` is searched for without
/// regard to line boundaries. The offset lets callers map a payload back to a
/// line number or test it against code block ranges.
///
/// Payloads that are empty or not valid JSON are skipped, and scanning
/// continues past them.
fn scan_configure_file_comments(text: &str) -> Vec<(usize, JsonValue)> {
    let mut found = Vec::new();
    let mut pos = 0;
    let mut quotes = BlockquoteTracker::new();

    while let Some(open_offset) = text[pos..].find(COMMENT_OPEN) {
        let comment_start = pos + open_offset;
        let after_open = comment_body(
            &text[comment_start + COMMENT_OPEN.len()..],
            quotes.quote_depth_at(text, comment_start),
        );
        let body_start = text.len() - after_open.len();
        // Advance past this opener by default, so an unrecognized or malformed
        // comment cannot stall the scan.
        pos = comment_start + COMMENT_OPEN.len();

        for tool in TOOL_PREFIXES {
            let Some(after_tool) = after_open.strip_prefix(tool) else {
                continue;
            };
            let Some(after_kw) = after_tool.strip_prefix(CONFIGURE_FILE_KEYWORD) else {
                break;
            };
            // Word boundary: the keyword must be followed by whitespace or `-->`,
            // so `configure-files` does not match.
            if !after_kw.is_empty() && !after_kw.starts_with(is_comment_space) && !after_kw.starts_with("-->") {
                break;
            }
            let Some(close_offset) = after_kw.find("-->") else {
                break;
            };

            let json_str = after_kw[..close_offset].trim_matches(is_comment_space);
            if !json_str.is_empty()
                && let Ok(value) = serde_json::from_str(json_str)
            {
                found.push((comment_start, value));
            }
            pos = body_start + tool.len() + CONFIGURE_FILE_KEYWORD.len() + close_offset + 3;
            break;
        }
    }

    found
}

/// Parse a configure-file comment and return the JSON configuration.
///
/// Returns the first payload found. The text may span lines.
pub fn parse_configure_file_comment(line: &str) -> Option<JsonValue> {
    scan_configure_file_comments(line).into_iter().next().map(|(_, v)| v)
}

// ── Disable-comment inventory ────────────────────────────────────────────────

/// The lines an inline disable comment can suppress a finding on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisableScope {
    /// Exactly this 1-indexed line
    Line(usize),
    /// The lines below this 1-indexed one, up to the end of the document
    Block(usize),
    /// Every line of the document
    File,
}

impl DisableScope {
    /// Whether this comment is one of those keeping a rule off at `line`, given the
    /// layer that answered for it there.
    ///
    /// Answering for a line credits every comment of that layer reaching it, so no
    /// comment loses its credit to another. A comment left uncredited is one whose
    /// every line is spoken for by a layer it is not in, and removing it therefore
    /// changes nothing the run reports.
    ///
    /// A block `disable` is treated as reaching the end of the document even when a
    /// later `enable` closes it. Crediting a comment for more than it holds can only
    /// leave a stale comment unreported.
    pub fn carries(self, layer: DisableLayer, line: usize) -> bool {
        match (self, layer) {
            (Self::Line(target), DisableLayer::Line) => target == line,
            // A block disable takes effect on the line after the comment.
            (Self::Block(start), DisableLayer::Block) => start < line,
            (Self::File, DisableLayer::File) => true,
            _ => false,
        }
    }
}

/// An inline comment that disables rules, and the lines it can act on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DisableSite {
    /// 1-indexed line the comment opens on
    pub line: usize,
    /// Byte range of the comment within that line, clipped to the line's end
    pub span: std::ops::Range<usize>,
    /// The directive as written, e.g. `disable-line`
    pub kind: &'static str,
    /// The rule names the comment carries, as written; empty means every rule
    pub rules: Vec<String>,
    /// The lines the comment can suppress a finding on
    pub scope: DisableScope,
}

/// Every inline comment that disables rules, in document order.
///
/// Comments inside code blocks or inline code spans configure nothing, so they
/// are left out, matching what `InlineConfig` applies. `prettier-ignore` belongs
/// to another formatter and is left out as well.
///
/// `code_blocks` and `code_spans` are the ranges the document's flavor really
/// holds as code, the same ones `InlineConfig` was built from. Recomputing them
/// from the text alone would read an indented container body (a MkDocs
/// admonition, a MyST directive) as an indented code block and leave out
/// comments that do configure the document.
pub fn collect_disable_sites(
    content: &str,
    code_blocks: &[(usize, usize)],
    code_spans: &[(usize, usize)],
) -> Vec<DisableSite> {
    if !has_inline_config_markers(content) {
        return Vec::new();
    }

    // Lines measured over `split('\n')`, whose pieces keep any `\r`, so a CRLF
    // document's offsets match the code block ranges.
    let line_spans: Vec<(usize, &str)> = {
        let mut spans = Vec::new();
        let mut start = 0;
        for line in content.split('\n') {
            spans.push((start, line));
            start += line.len() + 1;
        }
        spans
    };

    let mut sites = Vec::new();

    for (idx, &(line_start, line)) in line_spans.iter().enumerate() {
        if line_in_code_block(line_start, line, code_blocks) {
            continue;
        }
        let line_num = idx + 1;
        for directive in directives_outside_code_spans(line, line_start, code_spans) {
            let (kind, scope) = match directive.kind {
                DirectiveKind::Disable => ("disable", DisableScope::Block(line_num)),
                DirectiveKind::DisableLine => ("disable-line", DisableScope::Line(line_num)),
                DirectiveKind::DisableNextLine => ("disable-next-line", DisableScope::Line(line_num + 1)),
                DirectiveKind::DisableFile => ("disable-file", DisableScope::File),
                DirectiveKind::Enable
                | DirectiveKind::EnableFile
                | DirectiveKind::Capture
                | DirectiveKind::Restore
                | DirectiveKind::ConfigureFile => continue,
            };
            sites.push(DisableSite {
                line: line_num,
                span: directive.span,
                kind,
                rules: directive.rules.iter().map(|rule| (*rule).to_string()).collect(),
                scope,
            });
        }
    }

    // A directive comment spanning lines opens on one line and takes effect where
    // it ends. The site is reported on the opening line with its span clipped to it.
    let in_code = CommentCodeTest::new(content, code_blocks, code_spans);
    for directive in scan_multiline_directives(content, |open| in_code.contains(open)) {
        let (kind, scope) = match directive.kind {
            DirectiveKind::Disable => ("disable", DisableScope::Block(directive.end_line)),
            DirectiveKind::DisableLine => ("disable-line", DisableScope::Line(directive.end_line)),
            DirectiveKind::DisableNextLine => ("disable-next-line", DisableScope::Line(directive.end_line + 1)),
            DirectiveKind::DisableFile => ("disable-file", DisableScope::File),
            _ => continue,
        };
        let Some(&(line_start, line)) = line_spans.get(directive.start_line - 1) else {
            continue;
        };
        sites.push(DisableSite {
            line: directive.start_line,
            span: (directive.open - line_start)..(directive.end - line_start).min(line.len()),
            kind,
            rules: directive.rules.iter().map(|rule| (*rule).to_string()).collect(),
            scope,
        });
    }

    // A configure-file entry given `false` turns its rule off for the whole file,
    // exactly as disable-file does. The comment may span lines, so it is scanned
    // over the document and its span is clipped to the line it opens on.
    for (offset, json_config) in scan_configure_file_comments(content) {
        if offset_in_code_block(offset, code_blocks) || offset_in_code_span(offset, code_spans) {
            continue;
        }
        let Some(obj) = json_config.as_object() else {
            continue;
        };
        let rules: Vec<String> = obj
            .iter()
            .filter(|(_, value)| value.as_bool() == Some(false))
            .map(|(name, _)| name.clone())
            .collect();
        if rules.is_empty() {
            continue;
        }
        let line_num = line_of_offset(content, offset);
        let Some(&(line_start, line)) = line_spans.get(line_num - 1) else {
            continue;
        };
        let comment_end = content[offset..]
            .find("-->")
            .map_or(content.len(), |close| offset + close + 3);
        sites.push(DisableSite {
            line: line_num,
            span: (offset - line_start)..(comment_end - line_start).min(line.len()),
            kind: "configure-file",
            rules,
            scope: DisableScope::File,
        });
    }

    sites.sort_by_key(|site| (site.line, site.span.start));
    sites
}

/// What is wrong with an inline config comment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InlineConfigProblem {
    /// A rule name that is not recognized.
    UnknownRule,
    /// A recognized rule carrying an unrecognized option key inside a
    /// configure-file config object.
    UnknownOption { key: String },
    /// An inline directive tries to enable a rule that will not run over this
    /// document. rumdl treats config-level rule selection as final, so the
    /// enable has no effect; this makes that silent no-op visible.
    EnableHasNoEffect { reason: EnableNoEffectReason },
}

/// Why an inline enable cannot take effect.
///
/// The two are separate settings, so the message names the one actually in play
/// rather than sending the reader to a knob their config does not have set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnableNoEffectReason {
    /// Configuration did not enable the rule for this run at all.
    NotEnabled,
    /// Configuration enabled the rule, but `per-file-ignores` suppresses it for
    /// this particular file.
    IgnoredForFile,
}

/// Warning about an inline config comment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InlineConfigWarning {
    /// The line number where the warning occurred (1-indexed)
    pub line_number: usize,
    /// The rule the warning concerns
    pub rule_name: String,
    /// The type of inline config comment (e.g. "disable", "configure-file")
    pub comment_type: String,
    /// Suggestion for a similar rule name or option key, when one is close
    pub suggestion: Option<String>,
    /// What is wrong
    pub problem: InlineConfigProblem,
}

impl InlineConfigWarning {
    /// Format the warning message
    pub fn format_message(&self) -> String {
        // Wording for unknown rules/options matches the config-file validator so
        // the same mistake reads the same way wherever it is written.
        match &self.problem {
            InlineConfigProblem::UnknownOption { key } => match self.suggestion {
                Some(ref suggestion) => format!(
                    "Unknown option for rule {}: {} (did you mean: {}?)",
                    self.rule_name, key, suggestion
                ),
                None => format!("Unknown option for rule {}: {}", self.rule_name, key),
            },
            InlineConfigProblem::UnknownRule => match self.suggestion {
                Some(ref suggestion) => format!(
                    "Unknown rule in inline {} comment: {} (did you mean: {}?)",
                    self.comment_type, self.rule_name, suggestion
                ),
                None => format!(
                    "Unknown rule in inline {} comment: {}",
                    self.comment_type, self.rule_name
                ),
            },
            InlineConfigProblem::EnableHasNoEffect {
                reason: EnableNoEffectReason::NotEnabled,
            } => format!(
                "Rule {} is not enabled in configuration, so the inline {} comment enabling it has no effect",
                self.rule_name, self.comment_type
            ),
            InlineConfigProblem::EnableHasNoEffect {
                reason: EnableNoEffectReason::IgnoredForFile,
            } => format!(
                "Rule {} is ignored for this file by per-file-ignores, so the inline {} comment enabling it has no effect",
                self.rule_name, self.comment_type
            ),
        }
    }

    /// Print the warning to stderr with file context
    pub fn print_warning(&self, file_path: &str) {
        eprintln!(
            "{} {}:{}: {}",
            crate::utils::warning_label("inline config warning"),
            file_path,
            self.line_number,
            self.format_message()
        );
    }

    /// The warning as a lint diagnostic anchored to the comment that raised it.
    ///
    /// A directive naming a rule that does not exist silently does nothing, and on
    /// the CLI that is a line of stderr next to the findings. Editors show
    /// diagnostics, so the language server reports it as one and the mistake is
    /// visible where it was written. The span covers the whole line: the validator
    /// locates the comment, not the rule name inside it.
    pub fn to_lint_warning(&self, content: &str) -> crate::rule::LintWarning {
        let line_width = content
            .lines()
            .nth(self.line_number.saturating_sub(1))
            .map_or(0, |line| line.chars().count());
        crate::rule::LintWarning {
            message: self.format_message(),
            line: self.line_number,
            column: 1,
            end_line: self.line_number,
            end_column: line_width + 1,
            severity: crate::rule::Severity::Warning,
            fix: None,
            rule_name: Some(INLINE_CONFIG_DIAGNOSTIC_NAME.to_string()),
        }
    }
}

/// The diagnostic code inline config warnings carry.
///
/// Not a rule name: it names the class of problem so an editor can tell these
/// apart from rule violations, and so nothing looks it up in the rule registry.
pub const INLINE_CONFIG_DIAGNOSTIC_NAME: &str = "inline-config";

/// Validate all inline config comments in content and return warnings for unknown rules.
///
/// This function extracts rule names from all types of inline config comments
/// (disable, enable, disable-line, disable-next-line, disable-file, enable-file)
/// and validates them against the known rule alias map. Comments inside code
/// blocks or inline code spans are documentation rather than configuration, so
/// they are left alone, matching what `InlineConfig` applies. `flavor` is the
/// document's own, which decides what its indentation means.
pub fn validate_inline_config_rules(content: &str, flavor: MarkdownFlavor) -> Vec<InlineConfigWarning> {
    let (warnings, _) = collect_inline_config_warnings(content, flavor, true, None);
    drop_warnings_inside_code(content, flavor, warnings)
}

/// Warn when an inline enable names a valid rule excluded by the active rule
/// set or by per-file ignores. Unknown names are reported by
/// [`validate_inline_config_rules`]. Bare enables target no specific rule, and
/// directives in code configure nothing and produce no warning.
pub fn validate_inline_enables_against_active_rules(
    content: &str,
    flavor: MarkdownFlavor,
    active_rules: &HashSet<String>,
    ignored_for_file: &HashSet<String>,
) -> Vec<InlineConfigWarning> {
    let (_, warnings) = collect_inline_config_warnings(content, flavor, false, Some((active_rules, ignored_for_file)));
    drop_warnings_inside_code(content, flavor, warnings)
}

/// Validate names, options, and ineffective enables in one source scan and one
/// flavor-aware code reading. Name/option warnings precede enable warnings,
/// matching the separate validators' concatenated reports.
pub fn validate_inline_configuration(
    content: &str,
    flavor: MarkdownFlavor,
    active_rules: &HashSet<String>,
    ignored_for_file: &HashSet<String>,
) -> Vec<InlineConfigWarning> {
    let (mut warnings, enables) =
        collect_inline_config_warnings(content, flavor, true, Some((active_rules, ignored_for_file)));
    warnings.extend(enables);
    drop_warnings_inside_code(content, flavor, warnings)
}

/// Keep the original offsets until all warnings can share code filtering.
/// Each directive and multiline JSON payload is parsed only once.
fn collect_inline_config_warnings(
    content: &str,
    flavor: MarkdownFlavor,
    validate_names: bool,
    active: Option<(&HashSet<String>, &HashSet<String>)>,
) -> (Vec<LocatedWarning>, Vec<LocatedWarning>) {
    use crate::config::{RULE_ALIAS_MAP, is_valid_rule_name, suggest_similar_key};

    if !has_inline_config_markers(content) {
        return (Vec::new(), Vec::new());
    }
    let mut warnings = Vec::new();
    let mut enables = Vec::new();
    // Most directives name valid rules and need no spelling suggestions.
    let all_rule_names = std::cell::OnceCell::<Vec<String>>::new();
    let suggest = |name: &str| {
        let names =
            all_rule_names.get_or_init(|| RULE_ALIAS_MAP.keys().map(std::string::ToString::to_string).collect());
        suggest_similar_key(name, names).map(|s| if s.starts_with("MD") { s } else { s.to_lowercase() })
    };
    let mut flag_enable = |name: &str, comment_type: &str, line: usize, offset: usize| {
        let Some((active_rules, ignored_for_file)) = active else {
            return;
        };
        if !is_valid_rule_name(name) {
            return;
        }
        let canonical = normalize_rule_name(name);
        let reason = if !active_rules.contains(&canonical) {
            EnableNoEffectReason::NotEnabled
        } else if ignored_for_file.contains(&canonical) {
            EnableNoEffectReason::IgnoredForFile
        } else {
            return;
        };
        enables.push((
            InlineConfigWarning {
                line_number: line,
                rule_name: canonical,
                comment_type: comment_type.to_string(),
                suggestion: None,
                problem: InlineConfigProblem::EnableHasNoEffect { reason },
            },
            offset,
        ));
    };

    for (offset, json_config) in scan_configure_file_comments(content) {
        let Some(obj) = json_config.as_object() else {
            continue;
        };
        let line_number = line_of_offset(content, offset);
        for (rule_name, rule_config) in obj {
            if validate_names {
                if !is_valid_rule_name(rule_name) {
                    warnings.push((
                        InlineConfigWarning {
                            line_number,
                            rule_name: rule_name.clone(),
                            comment_type: "configure-file".to_string(),
                            suggestion: suggest(rule_name),
                            problem: InlineConfigProblem::UnknownRule,
                        },
                        offset,
                    ));
                    continue;
                }
                if let Some(options) = rule_config.as_object() {
                    let canonical = normalize_rule_name(rule_name);
                    // Do not initialize the schema registry for enable/disable
                    // comments or boolean configure-file entries.
                    if let Some(valid_keys) = crate::config::default_registry().config_keys_for(&canonical) {
                        let valid_keys_vec: Vec<String> = valid_keys.iter().cloned().collect();
                        for key in options.keys().filter(|key| !valid_keys.contains(*key)) {
                            warnings.push((
                                InlineConfigWarning {
                                    line_number,
                                    rule_name: canonical.clone(),
                                    comment_type: "configure-file".to_string(),
                                    suggestion: suggest_similar_key(key, &valid_keys_vec),
                                    problem: InlineConfigProblem::UnknownOption { key: key.clone() },
                                },
                                offset,
                            ));
                        }
                    }
                }
            }
            if rule_config.as_bool() == Some(true) {
                flag_enable(rule_name, "configure-file", line_number, offset);
            }
        }
    }

    // Every directive that names rules, as (kind, rules, reported line, offset).
    // split keeps CR bytes, so document offsets match the code ranges for both
    // LF and CRLF input. configure-file is handled document-wide above, and a
    // comment spanning lines is reported on the line it opens on.
    let mut named: Vec<(DirectiveKind, Vec<&str>, usize, usize)> = Vec::new();
    let mut line_start = 0;
    for (idx, line) in content.split('\n').enumerate() {
        for directive in parse_inline_directives(line) {
            named.push((
                directive.kind,
                directive.rules,
                idx + 1,
                line_start + directive.span.start,
            ));
        }
        line_start += line.len() + 1;
    }
    // The document's code ranges cost a parse, so they are read only once a
    // directive spanning lines needs them.
    let code = std::cell::OnceCell::new();
    let code_test = std::cell::OnceCell::new();
    let multiline_in_code = |open: usize| {
        code_test
            .get_or_init(|| {
                let ranges = code.get_or_init(|| crate::lint_context::code_ranges(content, flavor));
                CommentCodeTest::new(content, &ranges.blocks, &ranges.spans)
            })
            .contains(open)
    };
    for directive in scan_multiline_directives(content, multiline_in_code) {
        named.push((directive.kind, directive.rules, directive.start_line, directive.open));
    }
    for (kind, rules, line_number, offset) in named {
        let Some(comment_type) = directive_comment_type(kind) else {
            continue;
        };
        for rule_name in rules {
            if validate_names && !is_valid_rule_name(rule_name) {
                warnings.push((
                    InlineConfigWarning {
                        line_number,
                        rule_name: rule_name.to_string(),
                        comment_type: comment_type.to_string(),
                        suggestion: suggest(rule_name),
                        problem: InlineConfigProblem::UnknownRule,
                    },
                    offset,
                ));
            }
            if matches!(kind, DirectiveKind::Enable | DirectiveKind::EnableFile) {
                flag_enable(rule_name, comment_type, line_number, offset);
            }
        }
    }
    warnings.sort_by_key(|(warning, _)| warning.line_number);
    enables.sort_by_key(|(warning, _)| warning.line_number);
    (warnings, enables)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn combined_validation_preserves_warning_order_and_literal_code() {
        let source = concat!(
            "<!-- rumdl-enable MD009 -->\n",
            "<!-- rumdl-disable MD999 -->\n",
            "<!-- rumdl-configure-file {\"MD011\":true,\"MD013\":{\"line-lenght\":40}} -->\n",
            "<!-- rumdl-enable MD013 -->\n",
            "```markdown\n",
            "<!-- rumdl-enable MD010 -->\n",
            "<!-- rumdl-disable MD998 -->\n",
            "```\n",
            "`<!-- rumdl-enable MD012 -->`\n",
        );
        let active = HashSet::from(["MD013".to_string()]);
        let ignored = active.clone();
        for ending in ["\n", "\r\n"] {
            let content = source.replace('\n', ending);
            for flavor in [
                MarkdownFlavor::Standard,
                MarkdownFlavor::MkDocs,
                MarkdownFlavor::MDX,
                MarkdownFlavor::Pandoc,
                MarkdownFlavor::Quarto,
                MarkdownFlavor::Obsidian,
                MarkdownFlavor::Kramdown,
                MarkdownFlavor::AzureDevOps,
                MarkdownFlavor::MyST,
                MarkdownFlavor::Hugo,
                MarkdownFlavor::MDG,
                MarkdownFlavor::GhAw,
            ] {
                let mut separate = validate_inline_config_rules(&content, flavor);
                separate.extend(validate_inline_enables_against_active_rules(
                    &content, flavor, &active, &ignored,
                ));
                let combined = validate_inline_configuration(&content, flavor, &active, &ignored);
                assert_eq!(format!("{combined:?}"), format!("{separate:?}"), "{flavor:?}");
                assert_eq!(
                    combined
                        .iter()
                        .map(|warning| (warning.rule_name.as_str(), warning.line_number))
                        .collect::<Vec<_>>(),
                    vec![("MD999", 2), ("MD013", 3), ("MD009", 1), ("MD011", 3), ("MD013", 4)],
                    "{flavor:?}",
                );
            }
        }
    }

    // ── Whitespace around the comment delimiters is optional ─────────────

    const COMPACT_OPENERS: &[&str] = &["<!--", "<!-- ", "<!--  ", "<!--\t", "<!-- \t "];
    const COMPACT_CLOSERS: &[&str] = &["-->", " -->", "  -->", "\t-->"];

    #[test]
    fn test_parse_inline_directives_whitespace_around_delimiters_is_optional() {
        let cases: &[(&str, DirectiveKind, &[&str])] = &[
            ("rumdl-disable MD013", DirectiveKind::Disable, &["MD013"]),
            (
                "markdownlint-disable MD013 MD009",
                DirectiveKind::Disable,
                &["MD013", "MD009"],
            ),
            ("markdownlint-disable", DirectiveKind::Disable, &[]),
            ("rumdl-enable", DirectiveKind::Enable, &[]),
            ("markdownlint-enable MD013", DirectiveKind::Enable, &["MD013"]),
            ("rumdl-disable-line MD013", DirectiveKind::DisableLine, &["MD013"]),
            (
                "markdownlint-disable-next-line MD013",
                DirectiveKind::DisableNextLine,
                &["MD013"],
            ),
            (
                "markdownlint-disable-file MD013",
                DirectiveKind::DisableFile,
                &["MD013"],
            ),
            ("rumdl-enable-file", DirectiveKind::EnableFile, &[]),
            ("markdownlint-capture", DirectiveKind::Capture, &[]),
            ("rumdl-restore", DirectiveKind::Restore, &[]),
            ("markdownlint-configure-file {}", DirectiveKind::ConfigureFile, &["{}"]),
        ];
        for &(body, kind, rules) in cases {
            for open in COMPACT_OPENERS {
                for close in COMPACT_CLOSERS {
                    let comment = format!("{open}{body}{close}");
                    // Embedded in a line, so the span offsets are exercised too.
                    let line = format!("text {comment} tail");
                    let directives = parse_inline_directives(&line);
                    assert_eq!(directives.len(), 1, "{line:?}");
                    assert_eq!(directives[0].kind, kind, "{line:?}");
                    assert_eq!(directives[0].rules, rules, "{line:?}");
                    assert_eq!(&line[directives[0].span.clone()], comment, "{line:?}");
                }
            }
        }
    }

    #[test]
    fn test_parse_inline_directives_compact_not_a_directive() {
        for line in [
            "<!--markdownlint-disablefoo -->",
            "<!--rumdl-disableMD013-->",
            "<!--rumdl-enablebar-->",
            "<!--rumdl-captures-->",
            "<!--RUMDL-disable MD009 -->",
            "<!--Markdownlint-Disable-->",
            "<!--note rumdl-disable MD009 -->",
            "<!---- rumdl-disable MD009 -->",
            "<!--rumdl-disable MD009",
            "<!--rumdl-",
            "<!--",
        ] {
            assert!(parse_inline_directives(line).is_empty(), "{line:?}");
        }
    }

    #[test]
    fn test_parse_inline_directives_finds_compact_directive_after_other_comment() {
        let line = "<!-- other <!--rumdl-disable MD009--> <!--markdownlint-enable MD009-->";
        let directives = parse_inline_directives(line);
        assert_eq!(directives.len(), 2, "{line:?}");
        assert_eq!(directives[0].kind, DirectiveKind::Disable);
        assert_eq!(directives[1].kind, DirectiveKind::Enable);
    }

    #[test]
    fn test_scan_configure_file_compact_and_multiline() {
        for text in [
            "<!--rumdl-configure-file {\"MD013\":false}-->",
            "<!--markdownlint-configure-file {\"MD013\":false} -->",
            "<!--\trumdl-configure-file\n{\n\"MD013\":false\n}\n-->",
            "<!--\nrumdl-configure-file {\"MD013\":false}-->",
            "x <!-- other --> <!--rumdl-configure-file {\"MD013\":false}-->",
        ] {
            let found = scan_configure_file_comments(text);
            assert_eq!(found.len(), 1, "{text:?}");
            assert_eq!(found[0].1, serde_json::json!({"MD013": false}), "{text:?}");
            assert_eq!(&text[found[0].0..found[0].0 + 4], "<!--", "{text:?}");
        }
        for text in [
            "<!--rumdl-configure-files {\"MD013\":false}-->",
            "<!--rumdl-configure-file{\"MD013\":false}-->",
            "<!--RUMDL-configure-file {\"MD013\":false}-->",
            "<!--rumdl-configure-file {\"MD013\":false}",
        ] {
            assert!(scan_configure_file_comments(text).is_empty(), "{text:?}");
        }
        // Two comments in a row are both found, the second after the first's end.
        let two = "<!--rumdl-configure-file {\"MD013\":false}--><!--rumdl-configure-file {\"MD009\":false}-->";
        assert_eq!(scan_configure_file_comments(two).len(), 2);
    }

    #[test]
    fn test_from_content_compact_directives_match_spaced_form() {
        // Each pair is (compact, spaced); the resulting state must be identical.
        let pairs: &[(&str, &str)] = &[
            (
                "<!--rumdl-disable MD013-->\nA\n<!--rumdl-enable MD013-->\nB\n",
                "<!-- rumdl-disable MD013 -->\nA\n<!-- rumdl-enable MD013 -->\nB\n",
            ),
            (
                "A <!--markdownlint-disable-line MD013-->\nB\n",
                "A <!-- markdownlint-disable-line MD013 -->\nB\n",
            ),
            (
                "<!--markdownlint-disable-next-line MD013-->\nA\nB\n",
                "<!-- markdownlint-disable-next-line MD013 -->\nA\nB\n",
            ),
            (
                "<!--rumdl-disable-file MD013-->\nA\n",
                "<!-- rumdl-disable-file MD013 -->\nA\n",
            ),
            (
                "<!--rumdl-disable-file-->\n<!--rumdl-enable-file MD013-->\nA\n",
                "<!-- rumdl-disable-file -->\n<!-- rumdl-enable-file MD013 -->\nA\n",
            ),
            (
                "<!--markdownlint-disable MD013-->\n<!--markdownlint-capture-->\n<!--markdownlint-enable-->\nA\n<!--markdownlint-restore-->\nB\n",
                "<!-- markdownlint-disable MD013 -->\n<!-- markdownlint-capture -->\n<!-- markdownlint-enable -->\nA\n<!-- markdownlint-restore -->\nB\n",
            ),
            ("<!--prettier-ignore-->\nA\nB\n", "<!-- prettier-ignore -->\nA\nB\n"),
            ("<!--prettier-ignore -->\nA\nB\n", "<!-- prettier-ignore -->\nA\nB\n"),
            (
                "<!--rumdl-configure-file {\"MD013\":{\"line-length\":10}}-->\nA\n",
                "<!-- rumdl-configure-file {\"MD013\":{\"line-length\":10}} -->\nA\n",
            ),
            (
                "<!--markdownlint-configure-file\n{\"MD013\":false}\n-->\nA\n",
                "<!-- markdownlint-configure-file\n{\"MD013\":false}\n-->\nA\n",
            ),
            (
                "> <!--rumdl-disable-next-line MD013-->\n> A\n",
                "> <!-- rumdl-disable-next-line MD013 -->\n> A\n",
            ),
            (
                "- item\n\n  <!--rumdl-disable-next-line MD013-->\n  A\n",
                "- item\n\n  <!-- rumdl-disable-next-line MD013 -->\n  A\n",
            ),
        ];
        for (compact, spaced) in pairs {
            let a = InlineConfig::from_content(compact);
            let b = InlineConfig::from_content(spaced);
            let lines = compact.split('\n').count() + 2;
            for line in 1..=lines {
                for rule in ["MD013", "MD009"] {
                    assert_eq!(
                        a.disabling_layer(rule, line),
                        b.disabling_layer(rule, line),
                        "{compact:?} vs {spaced:?}, {rule} at line {line}"
                    );
                }
            }
            assert_eq!(a.get_all_rule_configs(), b.get_all_rule_configs(), "{compact:?}");
        }
        // Absolute expectations, so a pair that is wrong the same way cannot pass.
        let c = InlineConfig::from_content("<!--rumdl-disable MD013-->\nA\n<!--rumdl-enable MD013-->\nB\n");
        assert!(c.is_rule_disabled("MD013", 2));
        assert!(!c.is_rule_disabled("MD013", 4));
        let c = InlineConfig::from_content("<!--prettier-ignore-->\nA\nB\n");
        assert!(c.is_rule_disabled("MD013", 2));
        assert!(!c.is_rule_disabled("MD013", 3));
        let c = InlineConfig::from_content("<!--rumdl-configure-file {\"MD013\":{\"line-length\":10}}-->\nA\n");
        assert!(c.get_rule_config("MD013").is_some());
    }

    #[test]
    fn test_from_content_compact_directive_in_code_configures_nothing() {
        for content in [
            "`<!--rumdl-disable MD013-->`\nA\n",
            "```\n<!--rumdl-disable MD013-->\n```\nA\n",
            "    <!--rumdl-disable MD013-->\n\nA\n",
            "`<!--rumdl-disable-file MD013-->`\nA\n",
            "`<!--prettier-ignore-->`\nA\n",
            "`<!--rumdl-configure-file {\"MD013\":false}-->`\nA\n",
        ] {
            let config = InlineConfig::from_content(content);
            for line in 1..=6 {
                assert!(!config.is_rule_disabled("MD013", line), "{content:?} line {line}");
            }
            assert!(config.get_all_rule_configs().is_empty(), "{content:?}");
        }
    }

    #[test]
    fn test_validate_inline_config_reports_unknown_rule_in_compact_directive() {
        let warnings = validate_inline_config_rules("<!--rumdl-disable MD998-->\n", MarkdownFlavor::Standard);
        assert_eq!(warnings.len(), 1, "{warnings:?}");
        assert_eq!(warnings[0].line_number, 1);
    }

    // ── Unified parser tests ─────────────────────────────────────────────

    #[test]
    fn candidate_scan_keeps_nested_and_multiline_directives() {
        for source in [
            "<!-- rumdl-disable MD009 -->",
            "prose <!-- markdownlint-enable-file MD013 -->",
            "<!-- rumdl-configure-file {\n\"MD013\":false\n} -->",
            "<!-- other <!-- rumdl-disable-next-line MD009 -->",
            "<!-- prettier-ignore -->",
            "<!--rumdl-disable MD009 -->",
            "<!--markdownlint-disable-->",
            "<!--\trumdl-enable-->",
            "<!--prettier-ignore-->",
            "<!-- prettier-ignore-->",
            "<!--prettier-ignore -->",
            "<!--\nrumdl-configure-file {} -->",
        ] {
            assert!(has_inline_config_markers(source), "{source:?}");
        }
        for source in [
            "rumdl prose",
            "<!-- unrelated --> rumdl",
            "<!-- RUMDL-disable MD009 -->",
            "<!--RUMDL-disable MD009 -->",
            "<!---- rumdl-disable MD009 -->",
            "<!--prettier-ignore-start-->",
            "<!-- prettier-ignore-end -->",
            "<!--x--> rumdl-disable",
        ] {
            assert!(!has_inline_config_markers(source), "{source:?}");
            assert!(validate_inline_config_rules(source, MarkdownFlavor::Standard).is_empty());
        }
    }

    #[test]
    fn test_parse_inline_directives_all_kinds() {
        // Every directive kind is correctly identified
        let cases: &[(&str, DirectiveKind)] = &[
            ("<!-- rumdl-disable -->", DirectiveKind::Disable),
            ("<!-- rumdl-disable-line -->", DirectiveKind::DisableLine),
            ("<!-- rumdl-disable-next-line -->", DirectiveKind::DisableNextLine),
            ("<!-- rumdl-disable-file -->", DirectiveKind::DisableFile),
            ("<!-- rumdl-enable -->", DirectiveKind::Enable),
            ("<!-- rumdl-enable-file -->", DirectiveKind::EnableFile),
            ("<!-- rumdl-capture -->", DirectiveKind::Capture),
            ("<!-- rumdl-restore -->", DirectiveKind::Restore),
            ("<!-- rumdl-configure-file {} -->", DirectiveKind::ConfigureFile),
            // markdownlint variants
            ("<!-- markdownlint-disable -->", DirectiveKind::Disable),
            ("<!-- markdownlint-disable-line -->", DirectiveKind::DisableLine),
            (
                "<!-- markdownlint-disable-next-line -->",
                DirectiveKind::DisableNextLine,
            ),
            ("<!-- markdownlint-enable -->", DirectiveKind::Enable),
            ("<!-- markdownlint-capture -->", DirectiveKind::Capture),
            ("<!-- markdownlint-restore -->", DirectiveKind::Restore),
        ];
        for (input, expected_kind) in cases {
            let directives = parse_inline_directives(input);
            assert_eq!(
                directives.len(),
                1,
                "Expected 1 directive for {input:?}, got {directives:?}"
            );
            assert_eq!(directives[0].kind, *expected_kind, "Wrong kind for {input:?}");
        }
    }

    #[test]
    fn test_parse_inline_directives_disambiguation() {
        // The core property: "disable" must NOT match "disable-line" etc.
        let line = "<!-- rumdl-disable-line MD001 -->";
        let directives = parse_inline_directives(line);
        assert_eq!(directives.len(), 1);
        assert_eq!(directives[0].kind, DirectiveKind::DisableLine);

        let line = "<!-- rumdl-disable-next-line -->";
        let directives = parse_inline_directives(line);
        assert_eq!(directives.len(), 1);
        assert_eq!(directives[0].kind, DirectiveKind::DisableNextLine);

        let line = "<!-- rumdl-disable-file MD001 -->";
        let directives = parse_inline_directives(line);
        assert_eq!(directives.len(), 1);
        assert_eq!(directives[0].kind, DirectiveKind::DisableFile);

        let line = "<!-- rumdl-enable-file -->";
        let directives = parse_inline_directives(line);
        assert_eq!(directives.len(), 1);
        assert_eq!(directives[0].kind, DirectiveKind::EnableFile);
    }

    #[test]
    fn test_parse_inline_directives_no_space_before_close() {
        // <!-- rumdl-disable--> must parse as Disable (the bug that started this refactor)
        let directives = parse_inline_directives("<!-- rumdl-disable-->");
        assert_eq!(directives.len(), 1);
        assert_eq!(directives[0].kind, DirectiveKind::Disable);
        assert!(directives[0].rules.is_empty());

        let directives = parse_inline_directives("<!-- rumdl-enable-->");
        assert_eq!(directives.len(), 1);
        assert_eq!(directives[0].kind, DirectiveKind::Enable);
    }

    #[test]
    fn test_parse_inline_directives_multiple_on_one_line() {
        let line = "<!-- rumdl-disable MD001 --> text <!-- rumdl-enable MD001 -->";
        let directives = parse_inline_directives(line);
        assert_eq!(directives.len(), 2);
        assert_eq!(directives[0].kind, DirectiveKind::Disable);
        assert_eq!(directives[0].rules, vec!["MD001"]);
        assert_eq!(directives[1].kind, DirectiveKind::Enable);
        assert_eq!(directives[1].rules, vec!["MD001"]);
    }

    #[test]
    fn test_parse_inline_directives_global_disable_then_specific_enable() {
        let line = "<!-- rumdl-disable --> <!-- rumdl-enable MD001 -->";
        let directives = parse_inline_directives(line);
        assert_eq!(directives.len(), 2);
        assert_eq!(directives[0].kind, DirectiveKind::Disable);
        assert!(directives[0].rules.is_empty());
        assert_eq!(directives[1].kind, DirectiveKind::Enable);
        assert_eq!(directives[1].rules, vec!["MD001"]);
    }

    #[test]
    fn test_parse_inline_directives_word_boundary() {
        // "disablefoo" should NOT match "disable"
        assert!(parse_inline_directives("<!-- rumdl-disablefoo -->").is_empty());
        // "enablebar" should NOT match "enable"
        assert!(parse_inline_directives("<!-- rumdl-enablebar -->").is_empty());
        // "captures" should NOT match "capture"
        assert!(parse_inline_directives("<!-- rumdl-captures -->").is_empty());
    }

    #[test]
    fn test_parse_inline_directives_no_closing_tag() {
        // Missing --> means no directive
        assert!(parse_inline_directives("<!-- rumdl-disable MD001").is_empty());
        assert!(parse_inline_directives("<!-- rumdl-enable").is_empty());
    }

    #[test]
    fn test_parse_inline_directives_not_a_comment() {
        assert!(parse_inline_directives("rumdl-disable MD001 -->").is_empty());
        assert!(parse_inline_directives("Some regular text").is_empty());
        assert!(parse_inline_directives("").is_empty());
    }

    #[test]
    fn test_parse_inline_directives_case_sensitive() {
        assert!(parse_inline_directives("<!-- RUMDL-DISABLE -->").is_empty());
        assert!(parse_inline_directives("<!-- Markdownlint-Disable -->").is_empty());
    }

    #[test]
    fn test_parse_inline_directives_rules_extraction() {
        let directives = parse_inline_directives("<!-- rumdl-disable MD001 MD002 MD013 -->");
        assert_eq!(directives[0].rules, vec!["MD001", "MD002", "MD013"]);

        // Tabs between rules
        let directives = parse_inline_directives("<!-- rumdl-disable\tMD001\tMD002 -->");
        assert_eq!(directives[0].rules, vec!["MD001", "MD002"]);

        // Extra whitespace
        let directives = parse_inline_directives("<!-- rumdl-disable   MD001   -->");
        assert_eq!(directives[0].rules, vec!["MD001"]);
    }

    #[test]
    fn test_parse_inline_directives_embedded_in_text() {
        let line = "Some text <!-- rumdl-disable MD001 --> more text";
        let directives = parse_inline_directives(line);
        assert_eq!(directives.len(), 1);
        assert_eq!(directives[0].rules, vec!["MD001"]);

        let line = "🚀 <!-- rumdl-disable MD001 --> 🎉";
        let directives = parse_inline_directives(line);
        assert_eq!(directives.len(), 1);
        assert_eq!(directives[0].rules, vec!["MD001"]);
    }

    #[test]
    fn test_parse_inline_directives_mixed_tools_same_line() {
        let line = "<!-- rumdl-disable MD001 --> <!-- markdownlint-enable MD002 -->";
        let directives = parse_inline_directives(line);
        assert_eq!(directives.len(), 2);
        assert_eq!(directives[0].kind, DirectiveKind::Disable);
        assert_eq!(directives[0].rules, vec!["MD001"]);
        assert_eq!(directives[1].kind, DirectiveKind::Enable);
        assert_eq!(directives[1].rules, vec!["MD002"]);
    }

    // ── Backward-compatible wrapper tests ────────────────────────────────

    #[test]
    fn test_parse_disable_comment() {
        // Global disable
        assert_eq!(parse_disable_comment("<!-- markdownlint-disable -->"), Some(vec![]));
        assert_eq!(parse_disable_comment("<!-- rumdl-disable -->"), Some(vec![]));

        // Specific rules
        assert_eq!(
            parse_disable_comment("<!-- markdownlint-disable MD001 MD002 -->"),
            Some(vec!["MD001", "MD002"])
        );

        // No comment
        assert_eq!(parse_disable_comment("Some regular text"), None);
    }

    #[test]
    fn test_parse_disable_line_comment() {
        // Global disable-line
        assert_eq!(
            parse_disable_line_comment("<!-- markdownlint-disable-line -->"),
            Some(vec![])
        );

        // Specific rules
        assert_eq!(
            parse_disable_line_comment("<!-- markdownlint-disable-line MD013 -->"),
            Some(vec!["MD013"])
        );

        // No comment
        assert_eq!(parse_disable_line_comment("Some regular text"), None);
    }

    #[test]
    fn test_inline_config_from_content() {
        let content = r#"# Test Document

<!-- markdownlint-disable MD013 -->
This is a very long line that would normally trigger MD013 but it's disabled

<!-- markdownlint-enable MD013 -->
This line will be checked again

<!-- markdownlint-disable-next-line MD001 -->
# This heading will not be checked for MD001
## But this one will

Some text <!-- markdownlint-disable-line MD013 -->

<!-- markdownlint-capture -->
<!-- markdownlint-disable MD001 MD002 -->
# Heading with MD001 disabled
<!-- markdownlint-restore -->
# Heading with MD001 enabled again
"#;

        let config = InlineConfig::from_content(content);

        // Line 4 should have MD013 disabled (line after disable comment on line 3)
        assert!(config.is_rule_disabled("MD013", 4));

        // Line 7 should have MD013 enabled (line after enable comment on line 6)
        assert!(!config.is_rule_disabled("MD013", 7));

        // Line 10 should have MD001 disabled (from disable-next-line on line 9)
        assert!(config.is_rule_disabled("MD001", 10));

        // Line 11 should not have MD001 disabled
        assert!(!config.is_rule_disabled("MD001", 11));

        // Line 13 should have MD013 disabled (from disable-line)
        assert!(config.is_rule_disabled("MD013", 13));

        // After restore (line 18), MD001 should be enabled again on line 19
        assert!(!config.is_rule_disabled("MD001", 19));
    }

    #[test]
    fn test_capture_restore() {
        let content = r#"<!-- markdownlint-disable MD001 -->
<!-- markdownlint-capture -->
<!-- markdownlint-disable MD002 MD003 -->
<!-- markdownlint-restore -->
Some content after restore
"#;

        let config = InlineConfig::from_content(content);

        // After restore (line 4), line 5 should only have MD001 disabled
        assert!(config.is_rule_disabled("MD001", 5));
        assert!(!config.is_rule_disabled("MD002", 5));
        assert!(!config.is_rule_disabled("MD003", 5));
    }

    #[test]
    fn test_validate_inline_config_rules_unknown_rule() {
        let content = "<!-- rumdl-disable abc -->\nSome content";
        let warnings = validate_inline_config_rules(content, MarkdownFlavor::Standard);
        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].line_number, 1);
        assert_eq!(warnings[0].rule_name, "abc");
        assert_eq!(warnings[0].comment_type, "disable");
    }

    #[test]
    fn test_validate_inline_config_rules_valid_rule() {
        let content = "<!-- rumdl-disable MD001 -->\nSome content";
        let warnings = validate_inline_config_rules(content, MarkdownFlavor::Standard);
        assert!(
            warnings.is_empty(),
            "MD001 is a valid rule, should not produce warnings"
        );
    }

    #[test]
    fn test_validate_inline_config_rules_alias() {
        let content = "<!-- rumdl-disable heading-increment -->\nSome content";
        let warnings = validate_inline_config_rules(content, MarkdownFlavor::Standard);
        assert!(warnings.is_empty(), "heading-increment is a valid alias for MD001");
    }

    #[test]
    fn test_validate_inline_config_rules_multiple_unknown() {
        let content = r#"<!-- rumdl-disable abc xyz -->
<!-- rumdl-disable-line foo -->
<!-- markdownlint-disable-next-line bar -->
"#;
        let warnings = validate_inline_config_rules(content, MarkdownFlavor::Standard);
        assert_eq!(warnings.len(), 4);
        assert_eq!(warnings[0].rule_name, "abc");
        assert_eq!(warnings[1].rule_name, "xyz");
        assert_eq!(warnings[2].rule_name, "foo");
        assert_eq!(warnings[3].rule_name, "bar");
    }

    #[test]
    fn test_validate_inline_config_rules_suggestion() {
        // "MD00" should suggest "MD001" (or similar)
        let content = "<!-- rumdl-disable MD00 -->\n";
        let warnings = validate_inline_config_rules(content, MarkdownFlavor::Standard);
        assert_eq!(warnings.len(), 1);
        // Should have a suggestion since "MD00" is close to "MD001"
        assert!(warnings[0].suggestion.is_some());
    }

    #[test]
    fn test_validate_inline_config_rules_file_comments() {
        let content = "<!-- rumdl-disable-file nonexistent -->\n<!-- markdownlint-enable-file another_fake -->";
        let warnings = validate_inline_config_rules(content, MarkdownFlavor::Standard);
        assert_eq!(warnings.len(), 2);
        assert_eq!(warnings[0].comment_type, "disable-file");
        assert_eq!(warnings[1].comment_type, "enable-file");
    }

    #[test]
    fn test_validate_inline_config_rules_global_disable() {
        // Global disable (no specific rules) should not produce warnings
        let content = "<!-- rumdl-disable -->\n<!-- markdownlint-enable -->";
        let warnings = validate_inline_config_rules(content, MarkdownFlavor::Standard);
        assert!(warnings.is_empty(), "Global disable/enable should not produce warnings");
    }

    #[test]
    fn test_validate_inline_config_rules_mixed_valid_invalid() {
        // Use MD001 and MD003 which are valid rules; abc and xyz are invalid
        let content = "<!-- rumdl-disable MD001 abc MD003 xyz -->";
        let warnings = validate_inline_config_rules(content, MarkdownFlavor::Standard);
        assert_eq!(warnings.len(), 2);
        assert_eq!(warnings[0].rule_name, "abc");
        assert_eq!(warnings[1].rule_name, "xyz");
    }

    #[test]
    fn test_validate_inline_config_rules_configure_file() {
        // configure-file comments contain rule names as JSON keys
        let content =
            r#"<!-- rumdl-configure-file { "MD013": { "line_length": 120 }, "nonexistent": { "foo": true } } -->"#;
        let warnings = validate_inline_config_rules(content, MarkdownFlavor::Standard);
        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].rule_name, "nonexistent");
        assert_eq!(warnings[0].comment_type, "configure-file");
    }

    #[test]
    fn test_validate_inline_config_rules_markdownlint_variants() {
        // Test markdownlint-* variants (not just rumdl-*)
        let content = r#"<!-- markdownlint-disable unknown_rule -->
<!-- markdownlint-enable another_fake -->
<!-- markdownlint-disable-line bad_rule -->
<!-- markdownlint-disable-next-line fake_rule -->
<!-- markdownlint-disable-file missing_rule -->
<!-- markdownlint-enable-file nonexistent -->
"#;
        let warnings = validate_inline_config_rules(content, MarkdownFlavor::Standard);
        assert_eq!(warnings.len(), 6);
        assert_eq!(warnings[0].rule_name, "unknown_rule");
        assert_eq!(warnings[1].rule_name, "another_fake");
        assert_eq!(warnings[2].rule_name, "bad_rule");
        assert_eq!(warnings[3].rule_name, "fake_rule");
        assert_eq!(warnings[4].rule_name, "missing_rule");
        assert_eq!(warnings[5].rule_name, "nonexistent");
    }

    /// A directive inside a code block configures nothing, so documenting one in
    /// a fenced example must not be reported. The control outside the fence
    /// proves the same directive is still validated where it takes effect.
    #[test]
    fn test_validate_inline_config_rules_ignores_code_blocks() {
        let fenced = "# Doc\n\n```markdown\n<!-- rumdl-disable made_up_rule -->\n```\n";
        assert!(
            validate_inline_config_rules(fenced, MarkdownFlavor::Standard).is_empty(),
            "a directive inside a fence is documentation, not configuration"
        );

        let tilde = "# Doc\n\n~~~markdown\n<!-- rumdl-disable made_up_rule -->\n~~~\n";
        assert!(validate_inline_config_rules(tilde, MarkdownFlavor::Standard).is_empty());

        let indented = "# Doc\n\n    <!-- rumdl-disable made_up_rule -->\n";
        assert!(validate_inline_config_rules(indented, MarkdownFlavor::Standard).is_empty());

        let outside = "# Doc\n\n<!-- rumdl-disable made_up_rule -->\n";
        assert_eq!(validate_inline_config_rules(outside, MarkdownFlavor::Standard).len(), 1);
    }

    /// An indented container body is structure rather than code, so a directive
    /// written there does configure the document and its problems are reported.
    /// The same lines under a flavor with no admonitions really are an indented
    /// code block, which is the control on the exemption above.
    #[test]
    fn test_validate_inline_config_rules_reports_inside_an_indented_container() {
        let admonition = "# Doc\n\n!!! example\n\n    <!-- rumdl-disable made_up_rule -->\n";
        assert_eq!(
            validate_inline_config_rules(admonition, MarkdownFlavor::MkDocs).len(),
            1,
            "an admonition holds its content at an indent that is not code"
        );
        assert!(
            validate_inline_config_rules(admonition, MarkdownFlavor::Standard).is_empty(),
            "without admonitions the same lines are an indented code block"
        );

        let tab = "# Doc\n\n=== \"Tab\"\n\n    <!-- rumdl-disable made_up_rule -->\n";
        assert_eq!(validate_inline_config_rules(tab, MarkdownFlavor::MkDocs).len(), 1);
        assert!(validate_inline_config_rules(tab, MarkdownFlavor::Standard).is_empty());
    }

    /// An inline enable that can do nothing is reported wherever the directive
    /// takes effect, which includes an indented container body.
    #[test]
    fn test_validate_inline_enables_reports_inside_an_indented_container() {
        let admonition = "# Doc\n\n!!! example\n\n    <!-- rumdl-enable MD013 -->\n";
        let active = active_set(&["MD009"]);
        assert_eq!(
            validate_inline_enables_against_active_rules(admonition, MarkdownFlavor::MkDocs, &active, &no_ignores())
                .len(),
            1,
            "the enable is live in an admonition body, and configuration never enabled MD013"
        );
        assert!(
            validate_inline_enables_against_active_rules(admonition, MarkdownFlavor::Standard, &active, &no_ignores())
                .is_empty(),
            "without admonitions the same lines are an indented code block"
        );
    }

    /// configure-file is scanned over the whole document, so its code-block
    /// exemption is checked separately from the per-line directives.
    #[test]
    fn test_validate_inline_config_rules_ignores_configure_file_in_code_block() {
        let fenced = "# Doc\n\n```markdown\n<!-- rumdl-configure-file { \"MD013\": { \"bogus\": 1 } } -->\n```\n";
        assert!(validate_inline_config_rules(fenced, MarkdownFlavor::Standard).is_empty());

        let multiline = "# Doc\n\n```markdown\n<!-- rumdl-configure-file\n{ \"MD013\": { \"bogus\": 1 } }\n-->\n```\n";
        assert!(validate_inline_config_rules(multiline, MarkdownFlavor::Standard).is_empty());

        let outside = "# Doc\n\n<!-- rumdl-configure-file { \"MD013\": { \"bogus\": 1 } } -->\n";
        assert_eq!(validate_inline_config_rules(outside, MarkdownFlavor::Standard).len(), 1);
    }

    #[test]
    fn test_validate_inline_config_rules_markdownlint_configure_file() {
        let content = r#"<!-- markdownlint-configure-file { "fake_rule": {} } -->"#;
        let warnings = validate_inline_config_rules(content, MarkdownFlavor::Standard);
        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].rule_name, "fake_rule");
        assert_eq!(warnings[0].comment_type, "configure-file");
    }

    #[test]
    fn test_get_rule_config_from_configure_file() {
        let content = r#"<!-- markdownlint-configure-file {"MD013": {"line_length": 50}} -->

This is a test line."#;

        let inline_config = InlineConfig::from_content(content);
        let config_override = inline_config.get_rule_config("MD013");

        assert!(config_override.is_some(), "MD013 config should be found");
        let json = config_override.unwrap();
        assert!(json.is_object(), "Config should be an object");
        let obj = json.as_object().unwrap();
        assert!(obj.contains_key("line_length"), "Should have line_length key");
        assert_eq!(obj.get("line_length").unwrap().as_u64().unwrap(), 50);
    }

    #[test]
    fn test_get_rule_config_tables_false() {
        // Test that tables=false inline config is correctly parsed
        let content = r#"<!-- markdownlint-configure-file {"MD013": {"tables": false}} -->"#;

        let inline_config = InlineConfig::from_content(content);
        let config_override = inline_config.get_rule_config("MD013");

        assert!(config_override.is_some(), "MD013 config should be found");
        let json = config_override.unwrap();
        let obj = json.as_object().unwrap();
        assert!(obj.contains_key("tables"), "Should have tables key");
        assert!(!obj.get("tables").unwrap().as_bool().unwrap());
    }

    // ── multi-line configure-file ────────────────────────────────────────
    //
    // markdownlint scans configure-file over the whole document rather than
    // per line, so the comment may span lines. Every other directive stays
    // line-scoped in both tools.

    // ── inline enable of a rule that will not run ────────────────────────
    //
    // rumdl treats rule selection as final: a rule configuration disabled is
    // never instantiated, and one per-file-ignores excludes is dropped before
    // the file is linted, so an inline enable of either does nothing. These
    // warnings make that silent no-op visible.

    fn active_set(names: &[&str]) -> HashSet<String> {
        names.iter().map(|s| (*s).to_string()).collect()
    }

    fn no_ignores() -> HashSet<String> {
        HashSet::new()
    }

    #[test]
    fn test_inline_enable_of_inactive_rule_warns() {
        let active = active_set(&["MD013", "MD022"]);
        let content = "<!-- rumdl-enable MD012 -->\n";

        let warnings =
            validate_inline_enables_against_active_rules(content, MarkdownFlavor::Standard, &active, &no_ignores());

        assert_eq!(warnings.len(), 1, "expected one no-effect warning: {warnings:?}");
        assert_eq!(warnings[0].rule_name, "MD012");
        assert_eq!(warnings[0].comment_type, "enable");
        assert_eq!(
            warnings[0].problem,
            InlineConfigProblem::EnableHasNoEffect {
                reason: EnableNoEffectReason::NotEnabled
            }
        );
        assert_eq!(warnings[0].line_number, 1);
    }

    #[test]
    fn test_inline_enable_of_active_rule_does_not_warn() {
        // The load-bearing false-positive guard: enabling a rule that IS active
        // must stay silent.
        let active = active_set(&["MD012", "MD013"]);
        let content = "<!-- rumdl-enable MD012 -->\n";

        let warnings =
            validate_inline_enables_against_active_rules(content, MarkdownFlavor::Standard, &active, &no_ignores());

        assert!(warnings.is_empty(), "active rule warned: {warnings:?}");
    }

    #[test]
    fn test_inline_enable_of_per_file_ignored_rule_warns() {
        // The rule is enabled in configuration, so nothing about the config
        // explains the silence; per-file-ignores is what drops it here.
        let active = active_set(&["MD012", "MD013"]);
        let ignored = active_set(&["MD012"]);
        let content = "<!-- rumdl-enable MD012 -->\n";

        let warnings =
            validate_inline_enables_against_active_rules(content, MarkdownFlavor::Standard, &active, &ignored);

        assert_eq!(warnings.len(), 1, "expected one no-effect warning: {warnings:?}");
        assert_eq!(warnings[0].rule_name, "MD012");
        assert_eq!(
            warnings[0].problem,
            InlineConfigProblem::EnableHasNoEffect {
                reason: EnableNoEffectReason::IgnoredForFile
            }
        );
        assert!(
            warnings[0].format_message().contains("per-file-ignores"),
            "the message must name the setting in play: {}",
            warnings[0].format_message()
        );
    }

    #[test]
    fn test_enable_of_rule_both_disabled_and_ignored_blames_configuration() {
        // A per-file-ignores entry for a rule configuration never enabled is
        // redundant; the setting standing in the way is the config one.
        let active = active_set(&["MD013"]);
        let ignored = active_set(&["MD012"]);
        let content = "<!-- rumdl-enable MD012 -->\n";

        let warnings =
            validate_inline_enables_against_active_rules(content, MarkdownFlavor::Standard, &active, &ignored);

        assert_eq!(warnings.len(), 1, "{warnings:?}");
        assert_eq!(
            warnings[0].problem,
            InlineConfigProblem::EnableHasNoEffect {
                reason: EnableNoEffectReason::NotEnabled
            }
        );
    }

    #[test]
    fn test_per_file_ignores_of_an_unrelated_rule_does_not_warn() {
        // The negative control for the per-file-ignores arm: an ignore entry
        // naming a different rule must leave this enable alone.
        let active = active_set(&["MD012", "MD013"]);
        let ignored = active_set(&["MD013"]);
        let content = "<!-- rumdl-enable MD012 -->\n";

        let warnings =
            validate_inline_enables_against_active_rules(content, MarkdownFlavor::Standard, &active, &ignored);

        assert!(warnings.is_empty(), "unrelated ignore warned: {warnings:?}");
    }

    #[test]
    fn test_enable_inside_code_block_does_not_warn() {
        // A fenced example enables nothing, so neither reason applies to it.
        // The same document with the fence removed is the positive control.
        let active = active_set(&["MD012", "MD013"]);
        let ignored = active_set(&["MD012"]);

        for content in [
            "# Doc\n\n```markdown\n<!-- rumdl-enable MD012 -->\n```\n",
            "# Doc\n\n```markdown\n<!-- rumdl-configure-file { \"MD012\": true } -->\n```\n",
            "# Doc\n\n    <!-- rumdl-enable MD012 -->\n",
        ] {
            let warnings =
                validate_inline_enables_against_active_rules(content, MarkdownFlavor::Standard, &active, &ignored);
            assert!(warnings.is_empty(), "code block warned: {content} -> {warnings:?}");
        }

        for content in [
            "# Doc\n\n<!-- rumdl-enable MD012 -->\n",
            "# Doc\n\n<!-- rumdl-configure-file { \"MD012\": true } -->\n",
        ] {
            let warnings =
                validate_inline_enables_against_active_rules(content, MarkdownFlavor::Standard, &active, &ignored);
            assert_eq!(warnings.len(), 1, "control did not warn: {content} -> {warnings:?}");
        }
    }

    #[test]
    fn test_bare_enable_all_does_not_warn() {
        // `enable` with no rule list means "all"; it targets no specific rule.
        let active = active_set(&["MD013"]);
        for content in ["<!-- rumdl-enable -->\n", "<!-- rumdl-enable-file -->\n"] {
            let warnings = validate_inline_enables_against_active_rules(
                content,
                MarkdownFlavor::Standard,
                &active,
                &active_set(&["MD013"]),
            );
            assert!(warnings.is_empty(), "bare enable warned: {content} -> {warnings:?}");
        }
    }

    #[test]
    fn test_enable_file_and_alias_of_inactive_rule_warn() {
        let active = active_set(&["MD013"]);
        // enable-file, plus an alias for an inactive rule, both flagged.
        let content = "<!-- rumdl-enable-file no-multiple-blanks -->\n";

        let warnings =
            validate_inline_enables_against_active_rules(content, MarkdownFlavor::Standard, &active, &no_ignores());

        assert_eq!(warnings.len(), 1, "{warnings:?}");
        assert_eq!(warnings[0].rule_name, "MD012", "alias must normalize to the id");
        assert_eq!(warnings[0].comment_type, "enable-file");
    }

    #[test]
    fn test_configure_file_true_for_inactive_rule_warns_but_false_does_not() {
        let active = active_set(&["MD013"]);
        let enable = r#"<!-- markdownlint-configure-file {"MD012": true} -->"#;
        let disable = r#"<!-- markdownlint-configure-file {"MD012": false} -->"#;

        let warn_true =
            validate_inline_enables_against_active_rules(enable, MarkdownFlavor::Standard, &active, &no_ignores());
        let warn_false =
            validate_inline_enables_against_active_rules(disable, MarkdownFlavor::Standard, &active, &no_ignores());

        assert_eq!(warn_true.len(), 1, "configure-file true should warn: {warn_true:?}");
        assert_eq!(warn_true[0].rule_name, "MD012");
        assert!(
            warn_false.is_empty(),
            "configure-file false is a disable, not an ignored enable: {warn_false:?}"
        );
    }

    #[test]
    fn test_unknown_rule_in_enable_does_not_warn_as_no_effect() {
        // An unrecognized name is reported by validate_inline_config_rules; it
        // must not also be flagged here (it is not a known-but-inactive rule).
        let active = active_set(&["MD013"]);
        let content = "<!-- rumdl-enable NotARule -->\n";

        let warnings =
            validate_inline_enables_against_active_rules(content, MarkdownFlavor::Standard, &active, &no_ignores());

        assert!(warnings.is_empty(), "unknown rule double-warned: {warnings:?}");
    }

    #[test]
    fn test_disable_directive_never_warns_as_no_effect() {
        // Only enables are no-ops against a rule that will not run; a disable of
        // one is meaningless but harmless and must stay silent.
        let active = active_set(&["MD012", "MD013"]);
        let ignored = active_set(&["MD012"]);
        let content = "<!-- rumdl-disable MD012 -->\n<!-- rumdl-disable-file MD012 -->\n";

        let warnings =
            validate_inline_enables_against_active_rules(content, MarkdownFlavor::Standard, &active, &ignored);

        assert!(warnings.is_empty(), "disable warned: {warnings:?}");
    }

    // ── unknown option keys inside configure-file ────────────────────────
    //
    // A typo'd option key used to be dropped in silence, while the same typo
    // in a config file was reported with a suggestion.

    #[test]
    fn test_unknown_option_key_in_configure_file_warns() {
        let content = r#"<!-- markdownlint-configure-file {"MD013": {"line_lenght": 20}} -->"#;

        let warnings = validate_inline_config_rules(content, MarkdownFlavor::Standard);

        assert_eq!(warnings.len(), 1, "expected one option warning: {warnings:?}");
        assert_eq!(warnings[0].rule_name, "MD013");
        assert_eq!(
            warnings[0].problem,
            InlineConfigProblem::UnknownOption {
                key: "line_lenght".to_string()
            }
        );
        assert!(
            warnings[0].suggestion.is_some(),
            "a near-miss key should suggest the real one"
        );
        assert!(
            warnings[0].format_message().contains("Unknown option for rule MD013"),
            "message was: {}",
            warnings[0].format_message()
        );
    }

    #[test]
    fn test_valid_option_keys_do_not_warn_in_either_case_style() {
        // The false-positive guard that matters: both spellings are legal, and
        // warning on them would be worse than the silence this replaces.
        for content in [
            r#"<!-- markdownlint-configure-file {"MD013": {"line_length": 20}} -->"#,
            r#"<!-- markdownlint-configure-file {"MD013": {"line-length": 20}} -->"#,
        ] {
            let warnings = validate_inline_config_rules(content, MarkdownFlavor::Standard);
            assert!(warnings.is_empty(), "valid key warned: {content} -> {warnings:?}");
        }
    }

    #[test]
    fn test_unknown_rule_does_not_also_warn_about_its_options() {
        // The rule name is already reported; validating options of a rule that
        // does not exist would just be noise.
        let content = r#"<!-- markdownlint-configure-file {"nonexistent": {"whatever": 1}} -->"#;

        let warnings = validate_inline_config_rules(content, MarkdownFlavor::Standard);

        assert_eq!(
            warnings.len(),
            1,
            "expected only the unknown-rule warning: {warnings:?}"
        );
        assert_eq!(warnings[0].rule_name, "nonexistent");
        assert_eq!(warnings[0].problem, InlineConfigProblem::UnknownRule);
    }

    #[test]
    fn test_boolean_rule_value_has_no_option_warnings() {
        let content = r#"<!-- markdownlint-configure-file {"MD013": false} -->"#;

        let warnings = validate_inline_config_rules(content, MarkdownFlavor::Standard);

        assert!(warnings.is_empty(), "a boolean has no option keys: {warnings:?}");
    }

    #[test]
    fn test_unknown_option_key_in_multiline_comment_reports_start_line() {
        let content = "# Head\n\n<!-- markdownlint-configure-file\n{\n  \"MD013\": { \"line_lenght\": 20 }\n}\n-->\n";

        let warnings = validate_inline_config_rules(content, MarkdownFlavor::Standard);

        assert_eq!(warnings.len(), 1, "expected one option warning: {warnings:?}");
        assert_eq!(warnings[0].line_number, 3, "must point at the comment's opening line");
    }

    #[test]
    fn test_configure_file_spanning_multiple_lines_applies() {
        let content = "<!-- markdownlint-configure-file\n{\n  \"MD013\": { \"line_length\": 20 }\n}\n-->\n\n# Head\n";

        let inline_config = InlineConfig::from_content(content);
        let config_override = inline_config.get_rule_config("MD013");

        assert!(config_override.is_some(), "a multi-line configure-file must apply");
        let obj = config_override.unwrap().as_object().unwrap();
        assert_eq!(obj.get("line_length").unwrap().as_u64().unwrap(), 20);
    }

    #[test]
    fn test_every_configure_file_comment_applies_not_just_the_first() {
        // Scanning the whole document must not collapse to the first match:
        // both comments configure a different rule and both must land.
        let content = "<!-- markdownlint-configure-file { \"MD013\": { \"line_length\": 20 } } -->\n\n<!-- markdownlint-configure-file\n{\n  \"MD007\": { \"indent\": 4 }\n}\n-->\n\n# Head\n";

        let inline_config = InlineConfig::from_content(content);

        assert!(
            inline_config.get_rule_config("MD013").is_some(),
            "first (single-line) configure-file dropped"
        );
        assert!(
            inline_config.get_rule_config("MD007").is_some(),
            "second (multi-line) configure-file dropped"
        );
    }

    #[test]
    fn test_multiline_configure_file_in_code_block_is_ignored() {
        // rumdl ignores inline config inside fences; markdownlint does not.
        // Widening to a whole-document scan must not lose that.
        let content = "# Head\n\n```markdown\n<!-- markdownlint-configure-file\n{\n  \"MD013\": { \"line_length\": 20 }\n}\n-->\n```\n";

        let inline_config = InlineConfig::from_content(content);

        assert!(
            inline_config.get_rule_config("MD013").is_none(),
            "configure-file inside a fenced code block must not apply"
        );
    }

    #[test]
    fn test_multiline_configure_file_bool_and_alias_still_honored() {
        // The boolean and alias handling must survive the move off the
        // per-line path, in the multi-line form too.
        let content = "<!-- markdownlint-configure-file\n{\n  \"no-multiple-blanks\": false,\n  \"line-length\": { \"line_length\": 70 }\n}\n-->\n";

        let inline_config = InlineConfig::from_content(content);

        assert!(
            inline_config.is_rule_disabled("MD012", 1),
            "boolean alias key must disable the rule"
        );
        let obj = inline_config
            .get_rule_config("MD013")
            .expect("alias-keyed config should resolve to MD013")
            .as_object()
            .unwrap();
        assert_eq!(obj.get("line_length").unwrap().as_u64().unwrap(), 70);
    }

    #[test]
    fn test_multiline_configure_file_warning_reports_start_line() {
        // An unknown rule inside a multi-line comment is reported at the line
        // the comment opens on, not line 1 and not the closing line.
        let content = "# Head\n\n<!-- markdownlint-configure-file\n{\n  \"nonexistent\": { \"foo\": true }\n}\n-->\n";

        let warnings = validate_inline_config_rules(content, MarkdownFlavor::Standard);

        assert_eq!(warnings.len(), 1, "expected one unknown-rule warning: {warnings:?}");
        assert_eq!(warnings[0].rule_name, "nonexistent");
        assert_eq!(warnings[0].comment_type, "configure-file");
        assert_eq!(
            warnings[0].line_number, 3,
            "warning must point at the line the comment starts on"
        );
    }

    #[test]
    fn test_configure_file_bool_false_disables_rule() {
        // markdownlint documents a boolean as a way to turn a rule off for the
        // whole file, e.g. `{ "no-trailing-spaces": false }`.
        let content = r#"<!-- markdownlint-configure-file {"MD012": false} -->"#;

        let inline_config = InlineConfig::from_content(content);

        assert!(inline_config.is_rule_disabled("MD012", 1));
        assert!(
            inline_config.get_rule_config("MD012").is_none(),
            "a boolean should not be stored as rule options"
        );
    }

    #[test]
    fn test_configure_file_bool_false_disables_rule_by_alias() {
        let content = r#"<!-- markdownlint-configure-file {"no-multiple-blanks": false} -->"#;

        let inline_config = InlineConfig::from_content(content);

        assert!(inline_config.is_rule_disabled("MD012", 1));
    }

    #[test]
    fn test_configure_file_bool_true_leaves_rule_enabled() {
        let content = r#"<!-- markdownlint-configure-file {"MD012": true} -->"#;

        let inline_config = InlineConfig::from_content(content);

        assert!(!inline_config.is_rule_disabled("MD012", 1));
    }

    #[test]
    fn test_get_rule_config_from_configure_file_alias_key() {
        // A config written with the rule's alias must be reachable by its id.
        let content = r#"<!-- markdownlint-configure-file {"line-length": {"line_length": 50}} -->"#;

        let inline_config = InlineConfig::from_content(content);
        let config_override = inline_config.get_rule_config("MD013");

        assert!(config_override.is_some(), "alias-keyed config should resolve to MD013");
        let obj = config_override.unwrap().as_object().unwrap();
        assert_eq!(obj.get("line_length").unwrap().as_u64().unwrap(), 50);
    }

    // ── parse_disable_comment / parse_enable_comment edge cases ──────────

    #[test]
    fn test_parse_disable_does_not_match_disable_line() {
        // parse_disable_comment must NOT match disable-line or disable-next-line
        assert_eq!(parse_disable_comment("<!-- rumdl-disable-line MD001 -->"), None);
        assert_eq!(parse_disable_comment("<!-- markdownlint-disable-line MD001 -->"), None);
        assert_eq!(parse_disable_comment("<!-- rumdl-disable-next-line MD001 -->"), None);
        assert_eq!(parse_disable_comment("<!-- markdownlint-disable-next-line -->"), None);
        assert_eq!(parse_disable_comment("<!-- rumdl-disable-file MD001 -->"), None);
        assert_eq!(parse_disable_comment("<!-- markdownlint-disable-file -->"), None);
    }

    #[test]
    fn test_parse_enable_does_not_match_enable_file() {
        assert_eq!(parse_enable_comment("<!-- rumdl-enable-file MD001 -->"), None);
        assert_eq!(parse_enable_comment("<!-- markdownlint-enable-file -->"), None);
    }

    #[test]
    fn test_parse_disable_comment_edge_cases() {
        // No space before closing
        assert_eq!(parse_disable_comment("<!-- rumdl-disable-->"), Some(vec![]));

        // Tabs between rules
        assert_eq!(
            parse_disable_comment("<!-- rumdl-disable\tMD001\tMD002 -->"),
            Some(vec!["MD001", "MD002"])
        );

        // Comment not at start of line
        assert_eq!(
            parse_disable_comment("Some text <!-- rumdl-disable MD001 --> more text"),
            Some(vec!["MD001"])
        );

        // Malformed: no closing
        assert_eq!(parse_disable_comment("<!-- rumdl-disable MD001"), None);

        // Malformed: no opening
        assert_eq!(parse_disable_comment("rumdl-disable MD001 -->"), None);

        // Case sensitive: uppercase should not match
        assert_eq!(parse_disable_comment("<!-- RUMDL-DISABLE -->"), None);

        // Empty rule list with whitespace
        assert_eq!(parse_disable_comment("<!-- rumdl-disable   -->"), Some(vec![]));

        // Duplicate rules preserved (caller may deduplicate)
        assert_eq!(
            parse_disable_comment("<!-- rumdl-disable MD001 MD001 MD002 -->"),
            Some(vec!["MD001", "MD001", "MD002"])
        );

        // Unicode around the comment
        assert_eq!(
            parse_disable_comment("🚀 <!-- rumdl-disable MD001 --> 🎉"),
            Some(vec!["MD001"])
        );

        // 100 rules
        let many_rules = (1..=100).map(|i| format!("MD{i:03}")).collect::<Vec<_>>().join(" ");
        let comment = format!("<!-- rumdl-disable {many_rules} -->");
        let parsed = parse_disable_comment(&comment);
        assert!(parsed.is_some());
        assert_eq!(parsed.unwrap().len(), 100);

        // Special characters in rule names (forward compat)
        assert_eq!(
            parse_disable_comment("<!-- rumdl-disable MD001-test -->"),
            Some(vec!["MD001-test"])
        );
        assert_eq!(
            parse_disable_comment("<!-- rumdl-disable custom_rule -->"),
            Some(vec!["custom_rule"])
        );
    }

    #[test]
    fn test_parse_enable_comment_edge_cases() {
        assert_eq!(parse_enable_comment("<!-- rumdl-enable-->"), Some(vec![]));
        assert_eq!(parse_enable_comment("<!-- RUMDL-ENABLE -->"), None);
        assert_eq!(parse_enable_comment("<!-- rumdl-enable MD001"), None);
        assert_eq!(parse_enable_comment("<!-- rumdl-enable   -->"), Some(vec![]));
    }

    // ── InlineConfig: code blocks must be transparent ────────────────────

    #[test]
    fn test_disable_inside_fenced_code_block_ignored() {
        let content = "# Document\n```markdown\n<!-- rumdl-disable MD001 -->\nContent\n```\nAfter code block\n";
        let config = InlineConfig::from_content(content);
        // The disable comment is inside a code block — must have no effect
        assert!(!config.is_rule_disabled("MD001", 6));
    }

    #[test]
    fn test_disable_inside_tilde_fence_ignored() {
        let content = "# Document\n~~~\n<!-- rumdl-disable -->\nContent\n~~~\nAfter code block\n";
        let config = InlineConfig::from_content(content);
        assert!(!config.is_rule_disabled("MD001", 6));
    }

    #[test]
    fn test_disable_before_code_block_persists_after() {
        // Disable before code block should persist through and after it
        let content = "<!-- rumdl-disable MD001 -->\n```\ncode\n```\nStill disabled\n";
        let config = InlineConfig::from_content(content);
        assert!(config.is_rule_disabled("MD001", 5));
    }

    #[test]
    fn test_enable_inside_code_block_ignored() {
        // Disable before, enable inside code block (should be ignored), still disabled after
        let content = "<!-- rumdl-disable MD001 -->\n```\n<!-- rumdl-enable MD001 -->\n```\nShould still be disabled\n";
        let config = InlineConfig::from_content(content);
        assert!(config.is_rule_disabled("MD001", 5));
    }

    #[test]
    fn test_disable_inside_indented_code_block_ignored() {
        // An indented code block's range starts at the indented content rather
        // than at the start of the line, so containment of the whole line span
        // would miss it and let a code sample disable a rule.
        let content = "# Document\n\n    <!-- rumdl-disable MD001 -->\n\nAfter code block\n";
        let config = InlineConfig::from_content(content);
        assert!(!config.is_rule_disabled("MD001", 5));

        // Control: the same comment at column 1 is a directive and applies.
        let unindented = "# Document\n\n<!-- rumdl-disable MD001 -->\n\nAfter comment\n";
        let config = InlineConfig::from_content(unindented);
        assert!(config.is_rule_disabled("MD001", 5));
    }

    #[test]
    fn test_configure_file_inside_indented_code_block_ignored() {
        // configure-file is scanned over the whole document, so it reaches the
        // same conclusion by its own path; both must agree.
        let content = "# Document\n\n    <!-- rumdl-configure-file { \"MD013\": { \"line_length\": 20 } } -->\n";
        let config = InlineConfig::from_content(content);
        assert!(config.get_rule_config("MD013").is_none());

        let unindented = "# Document\n\n<!-- rumdl-configure-file { \"MD013\": { \"line_length\": 20 } } -->\n";
        let config = InlineConfig::from_content(unindented);
        assert!(config.get_rule_config("MD013").is_some());
    }

    // ── InlineConfig: directives inside inline code spans ────────────────

    #[test]
    fn test_disable_inside_code_span_ignored() {
        // A code span holds literal text, so a directive written between
        // backticks documents one rather than writing one.
        let content = "# Document\n\n`<!-- rumdl-disable MD001 -->`\n\nAfter the span\n";
        let config = InlineConfig::from_content(content);
        assert!(!config.is_rule_disabled("MD001", 5));

        // Control: the same comment without backticks is a directive and applies.
        let bare = "# Document\n\n<!-- rumdl-disable MD001 -->\n\nAfter the comment\n";
        let config = InlineConfig::from_content(bare);
        assert!(config.is_rule_disabled("MD001", 5));
    }

    #[test]
    fn test_directive_beside_one_in_a_code_span_still_applies() {
        // A span silences the directive it holds, not the line it sits on, so
        // filtering is per directive rather than per line.
        let content = "`<!-- rumdl-disable MD001 -->` <!-- rumdl-disable MD013 -->\n\ntext\n";
        let config = InlineConfig::from_content(content);
        assert!(!config.is_rule_disabled("MD001", 3), "the span's directive applied");
        assert!(config.is_rule_disabled("MD013", 3), "the live directive was dropped");
    }

    #[test]
    fn test_disable_file_inside_code_span_ignored() {
        let content = "`<!-- rumdl-disable-file MD013 -->`\n\ntext\n";
        let config = InlineConfig::from_content(content);
        assert!(!config.is_rule_disabled("MD013", 3));

        let bare = "<!-- rumdl-disable-file MD013 -->\n\ntext\n";
        let config = InlineConfig::from_content(bare);
        assert!(config.is_rule_disabled("MD013", 3));
    }

    #[test]
    fn test_configure_file_inside_code_span_ignored() {
        // configure-file is scanned over the whole document, so it reaches the
        // same conclusion by its own path; both must agree.
        let content = "`<!-- rumdl-configure-file { \"MD013\": { \"line_length\": 20 } } -->`\n";
        let config = InlineConfig::from_content(content);
        assert!(config.get_rule_config("MD013").is_none());

        let disabling = "`<!-- rumdl-configure-file { \"MD013\": false } -->`\n\ntext\n";
        let config = InlineConfig::from_content(disabling);
        assert!(!config.is_rule_disabled("MD013", 3));
    }

    #[test]
    fn test_disable_inside_multi_line_code_span_ignored() {
        // A code span may cross a line break, and what it holds is still text.
        let content = "`a\ntext <!-- rumdl-disable MD001 -->\nb`\n\nAfter the span\n";
        let config = InlineConfig::from_content(content);
        assert!(!config.is_rule_disabled("MD001", 5));
    }

    #[test]
    fn test_comment_opening_a_line_inside_backticks_still_applies() {
        // An HTML comment at the start of a line interrupts the paragraph, so the
        // opening backtick never finds a partner and no code span exists. The
        // comment is a real one and its directive is live.
        let content = "`a\n<!-- rumdl-disable MD001 -->\nb`\n\nAfter the comment\n";
        let config = InlineConfig::from_content(content);
        assert!(config.is_rule_disabled("MD001", 5));
    }

    #[test]
    fn test_disable_inside_double_backtick_code_span_ignored() {
        let content = "``<!-- rumdl-disable MD001 -->``\n\nAfter the span\n";
        let config = InlineConfig::from_content(content);
        assert!(!config.is_rule_disabled("MD001", 3));
    }

    #[test]
    fn test_code_ranges_line_up_on_a_crlf_document() {
        // Line offsets are measured over pieces that keep their `\r`, so they
        // match the byte ranges the parser reports. Measuring them without it
        // drifts a byte per line, and the drift is what lets a directive held
        // by a code span read as sitting outside it.
        let fenced = "# Doc\r\n\r\n```markdown\r\n<!-- rumdl-disable MD001 -->\r\n```\r\n\r\nAfter\r\n";
        let config = InlineConfig::from_content(fenced);
        assert!(!config.is_rule_disabled("MD001", 7), "a fenced directive applied");

        let spanned = "# Doc\r\n\r\nSee `<!-- rumdl-disable MD001 -->` here.\r\n\r\nAfter\r\n";
        let config = InlineConfig::from_content(spanned);
        assert!(!config.is_rule_disabled("MD001", 5), "a directive in a span applied");

        // Control: the same document with the directive in prose still applies.
        let bare = "# Doc\r\n\r\n<!-- rumdl-disable MD001 -->\r\n\r\nAfter\r\n";
        let config = InlineConfig::from_content(bare);
        assert!(config.is_rule_disabled("MD001", 5));
    }

    #[test]
    fn test_unmatched_backtick_does_not_open_a_code_span() {
        // A lone backtick is literal text, so the directive after it is live.
        let content = "` <!-- rumdl-disable MD001 -->\n\nAfter the comment\n";
        let config = InlineConfig::from_content(content);
        assert!(config.is_rule_disabled("MD001", 3));
    }

    #[test]
    fn test_validate_inline_config_rules_ignores_code_spans() {
        // An unknown rule name shown in prose names nothing this run would have
        // applied, so there is nothing to warn about.
        let spanned = "# Doc\n\nSee `<!-- rumdl-disable made_up_rule -->` here.\n";
        assert!(
            validate_inline_config_rules(spanned, MarkdownFlavor::Standard).is_empty(),
            "a directive shown in a code span warned"
        );

        let bare = "# Doc\n\n<!-- rumdl-disable made_up_rule -->\n";
        assert_eq!(
            validate_inline_config_rules(bare, MarkdownFlavor::Standard).len(),
            1,
            "the same name outside a span is still validated"
        );
    }

    #[test]
    fn test_validate_inline_config_rules_ignores_configure_file_in_code_span() {
        let spanned = "# Doc\n\nSee `<!-- rumdl-configure-file { \"MD013\": { \"bogus\": 1 } } -->` here.\n";
        assert!(validate_inline_config_rules(spanned, MarkdownFlavor::Standard).is_empty());
    }

    #[test]
    fn test_enable_inside_code_span_does_not_warn() {
        // A directive shown in prose enables nothing, so neither reason applies.
        let active = active_set(&["MD012", "MD013"]);
        let ignored = active_set(&["MD012"]);

        for content in [
            "# Doc\n\nSee `<!-- rumdl-enable MD012 -->` here.\n",
            "# Doc\n\nSee `<!-- rumdl-configure-file { \"MD012\": true } -->` here.\n",
        ] {
            let warnings =
                validate_inline_enables_against_active_rules(content, MarkdownFlavor::Standard, &active, &ignored);
            assert!(warnings.is_empty(), "code span warned: {content} -> {warnings:?}");
        }
    }

    // ── InlineConfig: mixed comment styles ───────────────────────────────

    #[test]
    fn test_markdownlint_disable_rumdl_enable_interop() {
        let content = "<!-- markdownlint-disable MD001 -->\nDisabled\n<!-- rumdl-enable MD001 -->\nEnabled\n";
        let config = InlineConfig::from_content(content);
        assert!(config.is_rule_disabled("MD001", 2));
        assert!(!config.is_rule_disabled("MD001", 4));
    }

    #[test]
    fn test_rumdl_disable_markdownlint_enable_interop() {
        let content = "<!-- rumdl-disable MD013 -->\nDisabled\n<!-- markdownlint-enable MD013 -->\nEnabled\n";
        let config = InlineConfig::from_content(content);
        assert!(config.is_rule_disabled("MD013", 2));
        assert!(!config.is_rule_disabled("MD013", 4));
    }

    // ── InlineConfig: nested/overlapping disable/enable ──────────────────

    #[test]
    fn test_global_disable_then_specific_enable() {
        let content = "<!-- rumdl-disable -->\nAll off\n<!-- rumdl-enable MD001 -->\nMD001 on, rest off\n";
        let config = InlineConfig::from_content(content);
        assert!(!config.is_rule_disabled("MD001", 4));
        assert!(config.is_rule_disabled("MD002", 4));
        assert!(config.is_rule_disabled("MD013", 4));
    }

    #[test]
    fn test_specific_disable_then_global_enable() {
        let content = "<!-- rumdl-disable MD001 MD002 -->\nBoth off\n<!-- rumdl-enable -->\nAll on\n";
        let config = InlineConfig::from_content(content);
        assert!(config.is_rule_disabled("MD001", 2));
        assert!(config.is_rule_disabled("MD002", 2));
        assert!(!config.is_rule_disabled("MD001", 4));
        assert!(!config.is_rule_disabled("MD002", 4));
    }

    #[test]
    fn test_multiple_rules_disable_enable_independently() {
        let content = "\
Line 1\n\
<!-- rumdl-disable MD001 MD002 -->\n\
Line 3\n\
<!-- rumdl-enable MD001 -->\n\
Line 5\n\
<!-- rumdl-disable -->\n\
Line 7\n\
<!-- rumdl-enable MD002 -->\n\
Line 9\n";
        let config = InlineConfig::from_content(content);

        // Line 1: nothing disabled
        assert!(!config.is_rule_disabled("MD001", 1));
        assert!(!config.is_rule_disabled("MD002", 1));

        // Line 3: both disabled
        assert!(config.is_rule_disabled("MD001", 3));
        assert!(config.is_rule_disabled("MD002", 3));

        // Line 5: MD001 enabled, MD002 still disabled
        assert!(!config.is_rule_disabled("MD001", 5));
        assert!(config.is_rule_disabled("MD002", 5));

        // Line 7: all disabled
        assert!(config.is_rule_disabled("MD001", 7));
        assert!(config.is_rule_disabled("MD002", 7));

        // Line 9: MD002 enabled, MD001 still disabled
        assert!(config.is_rule_disabled("MD001", 9));
        assert!(!config.is_rule_disabled("MD002", 9));
    }

    // ── InlineConfig: empty/minimal content ──────────────────────────────

    #[test]
    fn test_empty_content() {
        let config = InlineConfig::from_content("");
        assert!(!config.is_rule_disabled("MD001", 1));
    }

    #[test]
    fn test_single_disable_comment_only() {
        // Persistent disable takes effect from the NEXT line, not the current line.
        // For a single-line document, the disable on line 1 takes effect at line 2+.
        let config = InlineConfig::from_content("<!-- rumdl-disable -->");
        assert!(!config.is_rule_disabled("MD001", 1));
        assert!(config.is_rule_disabled("MD001", 2));
        assert!(config.is_rule_disabled("MD999", 2));

        // With content after the disable, rules are disabled from line 2 onward
        let config = InlineConfig::from_content("<!-- rumdl-disable -->\n# Heading\nSome text");
        assert!(!config.is_rule_disabled("MD001", 1));
        assert!(config.is_rule_disabled("MD001", 2));
        assert!(config.is_rule_disabled("MD001", 3));
    }

    #[test]
    fn test_no_inline_markers() {
        let config = InlineConfig::from_content("# Heading\n\nSome text\n\n- list item\n");
        assert!(!config.is_rule_disabled("MD001", 1));
        assert!(!config.is_rule_disabled("MD001", 5));
    }

    // ── InlineConfig: export_for_file_index correctness ──────────────────

    #[test]
    fn test_export_for_file_index_persistent_transitions() {
        let content = "Line 1\n<!-- rumdl-disable MD001 -->\nLine 3\n<!-- rumdl-enable MD001 -->\nLine 5\n";
        let config = InlineConfig::from_content(content);
        let (file_disabled, persistent, _line_disabled) = config.export_for_file_index();

        assert!(file_disabled.is_empty());
        // Should have transitions for the disable and enable
        assert!(
            persistent.len() >= 2,
            "Expected at least 2 transitions, got {}",
            persistent.len()
        );
    }

    #[test]
    fn test_export_for_file_index_disable_file() {
        let content = "<!-- rumdl-disable-file MD001 -->\n# Heading\n";
        let config = InlineConfig::from_content(content);
        let (file_disabled, _persistent, _line_disabled) = config.export_for_file_index();

        assert!(file_disabled.contains("MD001"));
    }

    #[test]
    fn test_export_for_file_index_disable_line() {
        let content = "Line 1\nLine 2 <!-- rumdl-disable-line MD001 -->\nLine 3\n";
        let config = InlineConfig::from_content(content);
        let (_file_disabled, _persistent, line_disabled) = config.export_for_file_index();

        assert!(line_disabled.contains_key(&2), "Line 2 should have disabled rules");
        assert!(line_disabled[&2].contains("MD001"));
        assert!(!line_disabled.contains_key(&3), "Line 3 should not be affected");
    }

    // ── multi-line directive comments ───────────────────────────────────────

    fn disabled_lines(content: &str, rule: &str) -> Vec<usize> {
        let config = InlineConfig::from_content(content);
        let lines = content.split('\n').count();
        (1..=lines)
            .filter(|&line| config.is_rule_disabled(rule, line))
            .collect()
    }

    #[test]
    fn multi_line_disable_takes_effect_after_the_closing_line() {
        // The comment ends on line 3, so line 4 is the first disabled one.
        let content = "<!--\nrumdl-disable MD013\n-->\nlong\nlong\n";
        assert_eq!(disabled_lines(content, "MD013"), [4, 5, 6]);
    }

    #[test]
    fn multi_line_markdownlint_forms_and_rule_lists_split_over_lines() {
        let content = "<!--\nmarkdownlint-disable MD013\nMD001\n  MD009\n-->\nx\n";
        for rule in ["MD013", "MD001", "MD009"] {
            assert_eq!(disabled_lines(content, rule), [6, 7], "{rule}");
        }
        let all = "<!-- rumdl-disable\n-->\nx\n";
        assert_eq!(disabled_lines(all, "MD013"), [3, 4]);
    }

    #[test]
    fn multi_line_enable_restores_and_capture_restore_nest() {
        let content = "<!-- rumdl-disable MD013 -->\na\n<!--\nrumdl-enable MD013\n-->\nb\n";
        assert_eq!(disabled_lines(content, "MD013"), [2, 3, 4, 5]);
        let content = "<!--\nrumdl-capture\n-->\n<!-- rumdl-disable MD013 -->\na\n<!--\nrumdl-restore\n-->\nb\n";
        assert_eq!(disabled_lines(content, "MD013"), [5, 6, 7, 8]);
    }

    #[test]
    fn multi_line_line_and_next_line_forms_use_the_closing_line() {
        let content = "<!--\nrumdl-disable-line MD013\n-->\nx\n";
        assert_eq!(disabled_lines(content, "MD013"), [3]);
        let content = "<!--\nrumdl-disable-next-line MD013\n-->\nx\ny\n";
        assert_eq!(disabled_lines(content, "MD013"), [4]);
    }

    #[test]
    fn multi_line_file_forms_apply_to_the_whole_document() {
        let content = "x\n<!--\nrumdl-disable-file MD013\n-->\ny\n";
        assert_eq!(disabled_lines(content, "MD013"), [1, 2, 3, 4, 5, 6]);
    }

    #[test]
    fn single_line_comment_after_a_multi_line_one_on_its_closing_line_applies_in_order() {
        // Disable (multi-line) then enable (single-line) on the closing line: net enabled.
        let content = "<!--\nrumdl-disable MD013\n--> <!-- rumdl-enable MD013 -->\nx\n";
        assert!(disabled_lines(content, "MD013").is_empty());
        // Enable first, then disable: net disabled from the next line.
        let content = "<!--\nrumdl-enable MD013\n--> <!-- rumdl-disable MD013 -->\nx\n";
        assert_eq!(disabled_lines(content, "MD013"), [4, 5]);
    }

    #[test]
    fn multi_line_directive_inside_a_code_block_or_span_configures_nothing() {
        let fenced = "```\n<!--\nrumdl-disable MD013\n-->\n```\nx\n";
        assert!(disabled_lines(fenced, "MD013").is_empty());
        let span = "`<!--\nrumdl-disable MD013\n-->`\nx\n";
        assert!(disabled_lines(span, "MD013").is_empty());
    }

    #[test]
    fn multi_line_comment_with_other_leading_text_is_not_a_directive() {
        let content = "<!--\nnote\nrumdl-disable MD013\n-->\nx\n";
        assert!(disabled_lines(content, "MD013").is_empty());
    }

    #[test]
    fn single_line_directives_are_not_counted_twice() {
        let content = "<!-- rumdl-disable MD013 -->\n<!-- rumdl-disable-next-line MD001 -->\nx\n";
        let config = InlineConfig::from_content(content);
        let (_, transitions, line_disabled) = config.export_for_file_index();
        // The initial state plus the one disable; a double count would add a third.
        assert_eq!(transitions.len(), 2, "{transitions:?}");
        assert_eq!(line_disabled.len(), 1, "{line_disabled:?}");
    }

    #[test]
    fn multi_line_disable_site_reports_the_opening_line_and_the_closing_line_scope() {
        let content = "x\n<!--\nrumdl-disable-next-line MD013\n-->\ny\n";
        let sites = collect_disable_sites(content, &[], &[]);
        assert_eq!(sites.len(), 1, "{sites:?}");
        assert_eq!(sites[0].line, 2);
        assert_eq!(sites[0].kind, "disable-next-line");
        assert_eq!(sites[0].scope, DisableScope::Line(5));
        assert_eq!(sites[0].span.start, 0);
        assert_eq!(sites[0].span.end, 4, "clipped to the opening line");
    }

    #[test]
    fn multi_line_directive_unknown_rule_is_reported_on_the_opening_line() {
        let content = "x\n<!--\nrumdl-disable MD9999\n-->\n";
        let warnings = validate_inline_config_rules(content, MarkdownFlavor::Standard);
        assert_eq!(warnings.len(), 1, "{warnings:?}");
        assert_eq!(warnings[0].line_number, 2);
        assert_eq!(warnings[0].rule_name, "MD9999");
    }

    // ── comment whitespace ──────────────────────────────────────────────────

    #[test]
    fn comment_whitespace_is_unicode_whitespace_plus_bom() {
        for ch in [
            '\u{20}', '\t', '\n', '\u{a0}', '\u{3000}', '\u{85}', '\u{2028}', '\u{feff}',
        ] {
            assert!(is_comment_space(ch), "{ch:?}");
        }
        for ch in ['\u{200b}', 'a', '-'] {
            assert!(!is_comment_space(ch), "{ch:?}");
        }
    }

    #[test]
    fn bom_counts_as_space_in_every_part_of_a_directive() {
        let content = "<!--\u{feff}rumdl-disable\u{feff}MD013\u{feff}MD001\u{feff}-->\nx\n";
        assert_eq!(disabled_lines(content, "MD013"), [2, 3]);
        assert_eq!(disabled_lines(content, "MD001"), [2, 3]);
        let zero_width = "<!--\u{200b}rumdl-disable MD013 -->\nx\n";
        assert!(disabled_lines(zero_width, "MD013").is_empty());
    }

    #[test]
    fn prettier_ignore_accepts_the_same_whitespace() {
        assert!(prettier_ignore_len("prettier-ignore\u{feff}-->").is_some());
        assert!(prettier_ignore_len("prettier-ignore\u{200b}-->").is_none());
    }

    // ── tool comment recogniser ─────────────────────────────────────────────

    fn is_tool_comment(text: &str) -> bool {
        !config_comment_ranges(text, |_| false).is_empty()
    }

    #[test]
    fn tool_comment_recognises_every_tool_in_any_spelling() {
        for text in [
            "<!-- rumdl-disable MD013 -->",
            "<!--markdownlint-disable-->",
            "<!-- vale off -->",
            "<!--vale off-->",
            "<!--  vale on  -->",
            "<!-- vale Style.Rule = NO -->",
            "<!-- vale style = X -->",
            "<!-- lint disable no-javascript -->",
            "<!--lint disable x-->",
            "<!--  lint enable x -->",
            "<!--\tlint ignore x\t-->",
            "<!--\u{feff}lint disable x-->",
            "<!--\nlint disable x\n-->",
        ] {
            assert!(is_tool_comment(text), "{text:?}");
        }
        for text in [
            "<!-- vale offset javascript -->",
            "<!-- vale onboarding javascript -->",
            "<!-- vale style guide javascript -->",
            "<!-- lint disabled x -->",
            "<!-- linting disable x -->",
            "<!-- lint -->",
            "<!-- valet off -->",
            "<!-- vale -->",
            "<!-- plain comment -->",
        ] {
            assert!(!is_tool_comment(text), "{text:?}");
        }
    }

    #[test]
    fn config_comment_ranges_cover_tool_comments_only() {
        let content = "a\n<!--\nlint disable x\n-->\nb\n<!-- vale off -->\n<!--\nplain\n-->\n";
        let ranges = config_comment_ranges(content, |_| false);
        let texts: Vec<&str> = ranges.iter().map(|range| &content[range.clone()]).collect();
        assert_eq!(texts, ["<!--\nlint disable x\n-->", "<!-- vale off -->"]);
    }

    #[test]
    fn config_comment_ranges_stop_at_the_comment_edges() {
        let content = "prose <!--vale off--> tail\n";
        let ranges = config_comment_ranges(content, |_| false);
        assert_eq!(&content[ranges[0].clone()], "<!--vale off-->");
    }

    #[test]
    fn config_comment_ranges_skip_code_openers_and_end_an_unclosed_one_at_its_line() {
        let content = "<!-- vale off\nprose\n";
        assert_eq!(config_comment_ranges(content, |_| false), vec![0..13]);
        assert!(config_comment_ranges(content, |_| true).is_empty());
    }

    #[test]
    fn blockquote_tracker_classifies_each_line_once_in_order() {
        let text = "<!-- a --> <!-- b -->\n> q <!-- c --> <!-- d -->\n- > l <!-- e -->\nx > y <!-- f -->\n1 > 0 <!-- g -->\n\n> <!-- h -->";
        let mut tracker = BlockquoteTracker::new();
        let got: Vec<usize> = text
            .match_indices(COMMENT_OPEN)
            .map(|(open, _)| tracker.quote_depth_at(text, open))
            .collect();
        assert_eq!(got, [0, 0, 1, 1, 1, 0, 0, 1]);
    }

    #[test]
    fn quote_prefix_needs_real_container_syntax() {
        for (line, depth) in [
            ("> x", 1),
            (">> x", 2),
            ("  > x", 1),
            ("- > x", 1),
            ("1. > x", 1),
            ("10) - > x", 1),
            ("> - > x", 2),
            (">x", 1),
            ("x", 0),
            ("1 > 0", 0),
            ("a > b", 0),
            ("-> x", 0),
            ("1.> x", 0),
            ("-x > y", 0),
            ("- x > y", 0),
            ("1234567890. > x", 0),
            ("", 0),
            ("- x", 0),
        ] {
            assert_eq!(line_quote_depth(line), depth, "{line:?}");
        }
    }

    #[test]
    fn config_comment_ranges_unterminated_openers_linear_complexity() {
        // Every opener is an unterminated tool comment: none may search the rest
        // of the document for a closer again. Compares the cost of one doubling.
        let cost = |count: usize| {
            let content = "<!-- vale off\n".repeat(count);
            let mut best = std::time::Duration::MAX;
            for _ in 0..5 {
                let start = std::time::Instant::now();
                let ranges = config_comment_ranges(std::hint::black_box(&content), |_| false);
                best = best.min(start.elapsed());
                assert_eq!(ranges.len(), count);
            }
            best.as_secs_f64()
        };
        let (small, large) = (cost(20_000), cost(80_000));
        assert!(
            large / small < 8.0,
            "4x the openers took {:.1}x the time (linear is about 4x, quadratic about 16x)",
            large / small
        );
    }

    #[test]
    fn offset_in_ranges_matches_a_linear_scan() {
        let ranges = vec![2..5, 5..6, 9..12, 20..21];
        for offset in 0..25 {
            assert_eq!(
                offset_in_ranges(&ranges, offset),
                ranges.iter().any(|range| range.contains(&offset)),
                "offset {offset}"
            );
        }
        assert!(!offset_in_ranges(&[], 0));
    }

    #[test]
    fn a_code_opener_does_not_swallow_the_directive_after_it() {
        let content = "```\n<!-- rumdl-disable\n```\n\n<!--\nrumdl-disable MD013\n-->\n";
        let kept = scan_multiline_directives(content, |open| open < 20);
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].start_line, 5);
    }

    #[test]
    fn comment_body_skips_blockquote_markers_on_later_lines_only() {
        assert_eq!(comment_body("\n> rumdl-disable", 1), "rumdl-disable");
        assert_eq!(comment_body("\n>> \t> rumdl-disable", 3), "rumdl-disable");
        assert_eq!(
            comment_body("> rumdl-disable", 1),
            "> rumdl-disable",
            "same line: not a marker"
        );
        assert_eq!(
            comment_body("\n> rumdl-disable", 0),
            "> rumdl-disable",
            "outside a blockquote the `>` is comment text"
        );
        assert_eq!(
            comment_body("\n> > rumdl-disable", 1),
            "> rumdl-disable",
            "a marker beyond the enclosing depth is text"
        );
        assert_eq!(comment_body("  \n  ", 1), "");
    }

    #[test]
    fn multi_line_directive_in_a_blockquote_splits_rules_across_marked_lines() {
        let content = "> <!-- rumdl-disable MD013\n> MD009\n> -->\n> text\n";
        let found = scan_multiline_directives(content, |_| false);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].rules, ["MD013", "MD009"]);
        assert_eq!((found[0].start_line, found[0].end_line), (1, 3));
    }

    #[test]
    fn an_unclosed_configure_file_example_in_code_does_not_swallow_the_directive_after_it() {
        let content = "```\n<!-- rumdl-configure-file\n```\n\n<!--\nrumdl-disable MD013\n-->\ntext\n";
        let code_end = content.find("\n\n<!--\n").unwrap();
        let found = scan_multiline_directives(content, |open| open < code_end);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].rules, ["MD013"]);
    }
}
