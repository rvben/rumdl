/// Rule MD026: No trailing punctuation in headings
///
/// See [docs/md026.md](../../docs/md026.md) for full documentation, configuration, and examples.
use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::rule_config_serde::FlavorOverrideNotice;
use crate::utils::regex_cache::{EMOJI_SHORTCODE_REGEX, HTML_ENTITY_REGEX};
use regex::Regex;
use std::collections::HashMap;
use std::ops::Range;
use std::sync::LazyLock;
use std::sync::RwLock;

mod md026_config;
use md026_config::{DEFAULT_PUNCTUATION, MD026Config};

// Fast check patterns for early returns - match defaults
static QUICK_PUNCTUATION_CHECK: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(&format!(r"[{}]", regex::escape(DEFAULT_PUNCTUATION))).unwrap());

// Regex cache for punctuation patterns
static PUNCTUATION_REGEX_CACHE: LazyLock<RwLock<HashMap<String, Regex>>> =
    LazyLock::new(|| RwLock::new(HashMap::new()));

/// Reports the MDG punctuation override once per process, across every config group.
static MDG_PUNCTUATION_OVERRIDE: FlavorOverrideNotice = FlavorOverrideNotice::new();

/// Rule MD026: Trailing punctuation in heading
#[derive(Clone)]
pub struct MD026NoTrailingPunctuation {
    config: MD026Config,
    // A Gherkin structure is an ATX heading spelled `Keyword: name`, so the colon after the
    // keyword is what makes the keyword a keyword. Dropping it from the punctuation set for
    // that flavor keeps the rule from ever deleting it, however `punctuation` is configured.
    mdg_punctuation: String,
    // Whether the colon the Gherkin flavor drops was asked for rather than inherited from
    // the default.
    colon_configured_explicitly: bool,
}

impl Default for MD026NoTrailingPunctuation {
    fn default() -> Self {
        Self::new(None)
    }
}

impl MD026NoTrailingPunctuation {
    /// The default policy can only remove a suffix from its own punctuation
    /// set. Question marks and other Unicode punctuation keep their text.
    pub(crate) fn default_source_has_trailing_punctuation(text: &str) -> bool {
        text.trim_end()
            .chars()
            .next_back()
            .is_some_and(|character| DEFAULT_PUNCTUATION.contains(character))
    }

    pub fn new(punctuation: Option<String>) -> Self {
        let explicit = punctuation.is_some();
        Self::build(
            MD026Config {
                punctuation: punctuation.unwrap_or_else(|| DEFAULT_PUNCTUATION.to_string()),
            },
            explicit,
        )
    }

    pub fn from_config_struct(config: MD026Config) -> Self {
        Self::build(config, false)
    }

    fn build(config: MD026Config, punctuation_explicit: bool) -> Self {
        let colon_configured_explicitly = punctuation_explicit && config.punctuation.contains(':');
        let mdg_punctuation = config.punctuation.replace(':', "");

        Self {
            config,
            mdg_punctuation,
            colon_configured_explicitly,
        }
    }

    /// The punctuation set the flavor actually enforces.
    #[inline]
    fn effective_punctuation(&self, flavor: crate::config::MarkdownFlavor) -> &str {
        if flavor == crate::config::MarkdownFlavor::MDG {
            &self.mdg_punctuation
        } else {
            &self.config.punctuation
        }
    }

    /// Whether MDG is overriding a colon that came from explicit configuration.
    fn mdg_colon_override_applies(&self, flavor: crate::config::MarkdownFlavor) -> bool {
        flavor == crate::config::MarkdownFlavor::MDG && self.colon_configured_explicitly
    }

    /// Report the effective punctuation override before any content-based
    /// shortcut can skip this rule. Direct `check` callers also pass through
    /// here; the shared notice keeps it process-local and one-shot even when a
    /// run creates separate rule instances for multiple config groups.
    fn warn_once_about_mdg_colon_override(&self, flavor: crate::config::MarkdownFlavor) {
        if self.mdg_colon_override_applies(flavor) {
            MDG_PUNCTUATION_OVERRIDE.report(
                "MD026",
                "punctuation",
                &self.config.punctuation,
                &self.mdg_punctuation,
                "the ASCII colon after a Gherkin keyword is structural",
            );
        }
    }

    #[inline]
    fn get_punctuation_regex(&self, punctuation: &str) -> Result<Regex, regex::Error> {
        // Check cache first
        {
            let cache = PUNCTUATION_REGEX_CACHE.read().unwrap();
            if let Some(cached_regex) = cache.get(punctuation) {
                return Ok(cached_regex.clone());
            }
        }

        // Compile and cache the regex
        let pattern = format!(r"([{}]+)$", regex::escape(punctuation));
        let regex = Regex::new(&pattern)?;

        {
            let mut cache = PUNCTUATION_REGEX_CACHE.write().unwrap();
            cache.insert(punctuation.to_string(), regex.clone());
        }

        Ok(regex)
    }

    /// The run of punctuation that ends `text`, as a byte range.
    ///
    /// A punctuation character that closes an HTML entity reference (`&amp;`, `&#59;`,
    /// `&#x3B;`) or an emoji shortcode (`:tada:`) is that construct's delimiter, not
    /// punctuation: removing it would turn the construct into literal text. The run
    /// therefore starts after the last such construct, so `Fish &amp;` has no run and
    /// `Fish &amp;.` has the run `.`.
    ///
    /// A backslash-escaped opener (`\&amp;`, `\:tada:`) renders as literal text, so
    /// its closing character is ordinary punctuation and counts toward the run.
    fn trailing_punctuation_run(&self, text: &str, re: &Regex) -> Option<Range<usize>> {
        let mut start = re.find(text)?.start();
        let constructs = HTML_ENTITY_REGEX
            .find_iter(text)
            .chain(EMOJI_SHORTCODE_REGEX.find_iter(text));
        for construct in constructs {
            if Self::is_backslash_escaped(text, construct.start()) {
                continue;
            }
            start = start.max(construct.end());
        }
        (start < text.len()).then_some(start..text.len())
    }

    /// True when the character at `pos` is preceded by an odd number of backslashes,
    /// which escapes it: `\&` is a literal ampersand while `\\&` is an escaped
    /// backslash followed by a live one.
    fn is_backslash_escaped(text: &str, pos: usize) -> bool {
        text[..pos].bytes().rev().take_while(|&b| b == b'\\').count() % 2 == 1
    }

    // Remove trailing punctuation from text.
    //
    // A single removal is not enough when punctuation is separated from the end by
    // interior whitespace (e.g. `. :`): removing `:` leaves `. `, whose trailing space
    // then exposes `.`, so a second fix pass would change the result again. This keeps
    // stripping while trimming the exposed whitespace reveals further trailing
    // punctuation, making one fix call fully converge (idempotent). Trailing whitespace
    // that does not hide more punctuation is preserved, matching the single-punctuation
    // behavior (e.g. `Title :` -> `Title `).
    #[inline]
    fn remove_trailing_punctuation(&self, text: &str, re: &Regex) -> String {
        let mut result = text.trim().to_string();
        loop {
            let Some(run) = self.trailing_punctuation_run(&result, re) else {
                // No trailing punctuation run at the very end.
                return result;
            };
            result.truncate(run.start);
            // Continue only if trimming the whitespace exposed by this removal reveals
            // more trailing punctuation; otherwise keep the result (whitespace and all).
            let trimmed_len = result.trim_end().len();
            if trimmed_len != result.len() && self.trailing_punctuation_run(&result[..trimmed_len], re).is_some() {
                result.truncate(trimmed_len);
            } else {
                return result;
            }
        }
    }
}

/// The bytes of `source` before `end` that `removed`, a suffix of a heading's
/// display text, is written as, read backwards together: a whitespace run of
/// the text is the whitespace run of the source it renders, a line ending and
/// the indentation below it included, and any other character is itself. None
/// where the source reads differently, as it does when an anchor element sits
/// inside the removed text.
fn removed_source_range(source: &str, end: usize, removed: &str) -> Option<Range<usize>> {
    let mut source = source.get(..end)?;
    let mut rest = removed;
    while !rest.is_empty() {
        let text = rest.trim_end();
        if text.len() < rest.len() {
            let trimmed = source.trim_end();
            if trimmed.len() == source.len() {
                return None;
            }
            source = trimmed;
            rest = text;
        } else {
            let ch = rest.chars().next_back()?;
            source = source.strip_suffix(ch)?;
            rest = &rest[..rest.len() - ch.len_utf8()];
        }
    }
    Some(source.len()..end)
}

/// Whether removing the trailing punctuation from an ATX heading without a
/// closing sequence would turn what is left into one. `a #` followed by a
/// removed `.` renders as `a #.` today and as `a` once the dot is gone, so the
/// fix would delete more than the punctuation. `remaining` is the display text
/// that stays.
fn exposes_closing_sequence(remaining: &str) -> bool {
    let kept = remaining.trim_end_matches([' ', '\t']);
    if !kept.ends_with('#') {
        return false;
    }
    let before_hashes = kept.trim_end_matches('#');
    before_hashes.is_empty() || before_hashes.ends_with([' ', '\t'])
}

/// Whether `rest`, the source after the text of an ATX heading, leaves the text
/// at the end of the heading: nothing follows it but spaces and tabs, or an
/// attribute block (`{#id}`, `{.class}`) set off from it by a space or tab.
fn ends_the_heading_text(rest: &str) -> bool {
    let rest_end = rest.trim_end_matches([' ', '\t']);
    let attributes = rest_end.trim_start_matches([' ', '\t']);
    attributes.is_empty()
        || (attributes.len() < rest_end.len() && attributes.starts_with('{') && attributes.ends_with('}'))
}

/// The block structure of a document: its block-level start events in order.
fn block_skeleton(content: &str) -> Vec<String> {
    use pulldown_cmark::{Event, Parser, Tag};
    Parser::new_ext(content, crate::utils::parser_options::rumdl_parser_options_for(content))
        .filter_map(|event| match event {
            Event::Start(Tag::Heading { level, .. }) => Some(format!("h{}", level as u8)),
            Event::Start(
                tag @ (Tag::List(_)
                | Tag::Item
                | Tag::BlockQuote(_)
                | Tag::Paragraph
                | Tag::CodeBlock(_)
                | Tag::HtmlBlock
                | Tag::Table(_)),
            ) => Some(
                format!("{tag:?}")
                    .split(['(', ' ', '{'])
                    .next()
                    .unwrap_or("")
                    .to_string(),
            ),
            Event::Rule => Some("rule".to_string()),
            _ => None,
        })
        .collect()
}

/// Whether a line that reads `line` once an edit is made cannot open a block of
/// its own: it does not start with a list marker, a thematic break, a fence, an
/// HTML tag, a reference definition or any other block syntax, so the edit
/// cannot change how the document divides into blocks. Container prefixes (`>`
/// and indentation) are ignored. Every start that is not known to be inline
/// text is unknown, and the caller checks it with a parse.
fn line_cannot_open_block(line: &str) -> bool {
    let line = line.trim_start_matches(|c: char| c.is_whitespace() || c == '>');
    let mut chars = line.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    let second = chars.next();
    match first {
        // No block syntax starts with a letter or any non-ASCII character
        c if !c.is_ascii() || c.is_ascii_alphabetic() => true,
        // An ordered list marker is digits, `.` or `)`, then a space, a tab or the end
        '0'..='9' => {
            let after_digits = line.trim_start_matches(|c: char| c.is_ascii_digit());
            match after_digits.strip_prefix(['.', ')']) {
                Some(after_marker) => !(after_marker.is_empty() || after_marker.starts_with([' ', '\t'])),
                None => true,
            }
        }
        '"' | '\'' | '(' | ',' | '.' | '?' | '&' | '@' | '\\' => true,
        // Emphasis: a delimiter touching text. A list marker has a space, a tab or
        // the end after it, and a thematic break has nothing but delimiters.
        '*' | '_' => {
            second.is_some_and(|c| !matches!(c, ' ' | '\t'))
                && line.chars().any(|c| !matches!(c, '*' | '_' | '-' | '=' | ' ' | '\t'))
        }
        // A code span or strikethrough opens with fewer than the three characters of a fence
        '`' | '~' => line.chars().take_while(|&c| c == first).count() < 3,
        // A link or footnote reference is text unless it is a definition
        '[' => !line.contains("]:"),
        '!' => second == Some('['),
        _ => false,
    }
}

/// Whether the line `before` + `rest`, where `before` is the start of a line up
/// to an edit and `rest` what follows it, cannot open a block of its own once the
/// edit is made. List markers and quote markers that end inside `before` are
/// unaffected by the edit and are skipped, so an item's text is judged on its own.
fn edited_line_cannot_open_block(before: &str, rest: &str) -> bool {
    let mut before = before;
    loop {
        before = before.trim_start_matches(|c: char| c.is_whitespace() || c == '>');
        let after_digits = before.trim_start_matches(|c: char| c.is_ascii_digit());
        let marker_len = if before.starts_with(['-', '+', '*']) {
            1
        } else if after_digits.len() < before.len() && after_digits.starts_with(['.', ')']) {
            before.len() - after_digits.len() + 1
        } else {
            0
        };
        match before.get(marker_len..) {
            Some(after_marker) if marker_len > 0 && after_marker.starts_with([' ', '\t']) => before = after_marker,
            _ => break,
        }
    }
    line_cannot_open_block(&format!("{before}{rest}"))
}

/// A leaf block of the document that can be checked on its own: the byte range
/// from the start of its first line to its end, and the width of the container
/// prefix (list indentation, blockquote markers) in front of its first line.
struct BlockWindow {
    range: Range<usize>,
    prefix_width: usize,
}

/// The leaf blocks of a document, in order: every block that is not itself a
/// list, list item or blockquote, however deeply those nest.
fn block_windows(content: &str) -> Vec<BlockWindow> {
    use pulldown_cmark::{Event, Parser, Tag};
    let parser = Parser::new_ext(content, crate::utils::parser_options::rumdl_parser_options_for(content));
    let mut windows = Vec::new();
    // Nesting depth of the block being walked, and the depth at which a leaf block opened
    let mut depth = 0usize;
    let mut leaf_depth: Option<usize> = None;
    let push = |range: &Range<usize>, windows: &mut Vec<BlockWindow>| {
        let start = content[..range.start].rfind('\n').map_or(0, |i| i + 1);
        windows.push(BlockWindow {
            range: start..range.end,
            prefix_width: range.start - start,
        });
    };
    for (event, range) in parser.into_offset_iter() {
        match event {
            Event::Start(tag) => {
                if leaf_depth.is_none() && !matches!(tag, Tag::List(_) | Tag::Item | Tag::BlockQuote(_)) {
                    push(&range, &mut windows);
                    leaf_depth = Some(depth);
                }
                depth += 1;
            }
            Event::End(_) => {
                depth = depth.saturating_sub(1);
                if leaf_depth == Some(depth) {
                    leaf_depth = None;
                }
            }
            Event::Rule if leaf_depth.is_none() => push(&range, &mut windows),
            _ => {}
        }
    }
    windows
}

/// Replaces the tabs in `prefix` by the spaces they stand for, `column` being
/// the column the text starts at.
fn expand_tabs(prefix: &str, mut column: usize) -> String {
    let mut out = String::with_capacity(prefix.len());
    for c in prefix.chars() {
        if c == '\t' {
            let width = 4 - column % 4;
            out.extend(std::iter::repeat_n(' ', width));
            column += width;
        } else {
            out.push(c);
            column += 1;
        }
    }
    out
}

/// Removes the container prefix from the lines of a leaf block so the block
/// reads the same parsed alone. The first line loses `prefix_width` bytes; each
/// later line loses as many leading spaces and `>` markers as the first line's
/// prefix is wide. The tabs in a prefix are expanded to spaces first, which
/// reads the same to the parser.
fn strip_container_prefix(unit: &str, prefix_width: usize) -> String {
    let mut out = String::with_capacity(unit.len());
    let mut width = 0;
    for (i, line) in unit.split_inclusive('\n').enumerate() {
        if i == 0 {
            let (prefix, rest) = line.split_at(prefix_width);
            width = expand_tabs(prefix, 0).len();
            out.push_str(rest);
        } else {
            let run = line.find(|c| !matches!(c, ' ' | '\t' | '>')).unwrap_or(line.len());
            let expanded = expand_tabs(&line[..run], 0);
            let strip = expanded
                .bytes()
                .take(width)
                .take_while(|b| matches!(b, b' ' | b'>'))
                .count();
            out.push_str(&expanded[strip..]);
            out.push_str(&line[run..]);
        }
    }
    out
}

/// Whether deleting `range` from `content` leaves the blocks of the document
/// as they were. Deleting the text before a line break can turn the rest of
/// the heading into a list item, a thematic break or a paragraph of its own.
/// A line that cannot open a block needs no parse. Any other is checked in the
/// leaf block that holds the heading, with its container prefix removed: no
/// other block depends on the end of one of its lines. `windows` is built on
/// first use and shared by the headings of one document.
fn deletion_keeps_block_structure(content: &str, range: &Range<usize>, windows: &mut Option<Vec<BlockWindow>>) -> bool {
    let line_start = content[..range.start].rfind('\n').map_or(0, |i| i + 1);
    let line_end = content[range.end..].find('\n').map_or(content.len(), |i| range.end + i);
    if edited_line_cannot_open_block(&content[line_start..range.start], &content[range.end..line_end]) {
        return true;
    }
    let windows = windows.get_or_insert_with(|| block_windows(content));
    let idx = windows.partition_point(|w| w.range.end < range.end);
    let (window, prefix_width) = windows
        .get(idx)
        .filter(|w| w.range.start <= range.start)
        .map_or((0..content.len(), 0), |w| (w.range.clone(), w.prefix_width));
    let unit = &content[window.clone()];
    let mut edited = String::with_capacity(unit.len());
    edited.push_str(&content[window.start..range.start]);
    edited.push_str(&content[range.end..window.end]);
    let before = strip_container_prefix(unit, prefix_width);
    let after = strip_container_prefix(&edited, prefix_width);
    block_skeleton(&before) == block_skeleton(&after)
}

impl Rule for MD026NoTrailingPunctuation {
    fn native_format_policy(&self) -> Option<crate::format_proof::NativeFormatPolicy> {
        (self.config == Default::default()).then_some(crate::format_proof::NativeFormatPolicy::for_rule::<Self>(26))
    }

    fn name(&self) -> &'static str {
        "MD026"
    }

    fn description(&self) -> &'static str {
        "Trailing punctuation in heading"
    }

    fn category(&self) -> RuleCategory {
        RuleCategory::Heading
    }

    fn should_skip(&self, ctx: &crate::lint_context::LintContext) -> bool {
        self.warn_once_about_mdg_colon_override(ctx.flavor);

        // Skip if no heading markers
        if !ctx.likely_has_headings() {
            return true;
        }
        // Skip if none of the configured punctuation exists
        let punctuation = self.effective_punctuation(ctx.flavor);
        !punctuation.chars().any(|p| ctx.has_char(p))
    }

    fn check(&self, ctx: &crate::lint_context::LintContext) -> LintResult {
        let content = ctx.content;
        let punctuation = self.effective_punctuation(ctx.flavor);

        self.warn_once_about_mdg_colon_override(ctx.flavor);

        // Early returns for performance
        if content.is_empty() {
            return Ok(Vec::new());
        }

        // Quick check for any punctuation we care about
        // For custom punctuation, we need to check differently
        if punctuation == DEFAULT_PUNCTUATION {
            if !QUICK_PUNCTUATION_CHECK.is_match(content) {
                return Ok(Vec::new());
            }
        } else {
            // For custom punctuation, check if any of those characters exist
            let has_custom_punctuation = punctuation.chars().any(|c| content.contains(c));
            if !has_custom_punctuation {
                return Ok(Vec::new());
            }
        }

        // Check if we have any headings from pre-computed line info
        if ctx.headings().next().is_none() {
            return Ok(Vec::new());
        }

        let mut warnings = Vec::new();
        let Ok(re) = self.get_punctuation_regex(punctuation) else {
            return Ok(warnings);
        };

        // Headings come from the parsed view, which includes the ones inside
        // blockquotes. A Setext heading is recorded on the last line of its
        // text, and the parsed form locates where that text ends.
        let mut windows = None;
        for parsed in ctx.headings() {
            let heading = parsed.heading;
            let is_setext = parsed.is_setext();
            let marker_idx = if is_setext {
                parsed.line_num
            } else {
                parsed.line_num - 1
            };
            if ctx
                .lines
                .get(marker_idx)
                .is_some_and(|marker_line| ctx.is_inside_template_code(marker_line.byte_offset + marker_line.indent))
            {
                continue;
            }

            // LintContext already strips Kramdown IDs from heading.text
            // So we just check the heading text directly for trailing punctuation
            // This correctly flags "# Heading." even if it has {#id}
            let text_to_check = heading.text.as_str();

            let Some(run) = self.trailing_punctuation_run(text_to_check, &re) else {
                continue;
            };

            // The run closes the heading's display text, so in the source
            // it sits at the end of the text range, whichever line of a
            // Setext heading that ends on, unless an anchor element written
            // inside the run keeps the two apart. Then the whole text is
            // reported, and the punctuation is left in place.
            let range = parsed.text_byte_range(ctx.content);
            let run_text = &text_to_check[run.clone()];
            let run_start = range
                .end
                .checked_sub(run_text.len())
                .filter(|&start| ctx.content.get(start..range.end) == Some(run_text));
            if run_start.is_some_and(|start| ctx.overlaps_template_code(start, range.end)) {
                continue;
            }
            let (start_line, start_col) = ctx.offset_to_line_col(run_start.unwrap_or(range.start));
            let (end_line, end_col) = ctx.offset_to_line_col(range.end);

            // The fix deletes the removed text where it stands, so a closing
            // sequence, an attribute list or anchor element after it, the
            // whitespace around it, and the container prefix of a heading
            // written inside a quote or list item, stay as they are.
            // Whitespace between two runs of punctuation goes with them, a
            // line break included; where the source reads differently, only
            // the run itself goes. Template-bearing headings use the mapped
            // source run too: an anchor lookalike inside a template value is
            // not an ID.
            let fix = run_start.and_then(|start| {
                let fixed = self.remove_trailing_punctuation(text_to_check, &re);
                let removed = text_to_check.trim().strip_prefix(fixed.as_str()).unwrap_or(run_text);
                let mut delete = removed_source_range(ctx.content, range.end, removed).unwrap_or(start..range.end);
                if !is_setext {
                    // Spaces and tabs between the text and the end of the line go
                    // with the punctuation; a closing sequence or attribute list
                    // after them keeps its separator.
                    let tail = &ctx.content[range.end..];
                    let line_tail = tail.split(['\n', '\r']).next().unwrap_or("");
                    if line_tail.chars().all(|c| c == ' ' || c == '\t') {
                        delete.end = range.end + line_tail.len();
                    }
                }
                // What is left closes the heading only when nothing but spaces and
                // tabs, or an attribute block set off by them, follows it on the
                // line; other whitespace stays text.
                if !is_setext && !heading.has_closing_sequence {
                    let rest = ctx.content[delete.end..].split(['\n', '\r']).next().unwrap_or("");
                    if ends_the_heading_text(rest) && exposes_closing_sequence(&fixed) {
                        return None;
                    }
                }
                if is_setext && !deletion_keeps_block_structure(ctx.content, &delete, &mut windows) {
                    return None;
                }
                Some(Fix::new(delete, String::new()))
            });

            let fix = fix.filter(|fix| !ctx.overlaps_template_code(fix.range.start, fix.range.end));

            let last_char = text_to_check.chars().last().unwrap_or(' ');
            warnings.push(LintWarning {
                rule_name: Some(self.name().to_string()),
                line: start_line,
                column: start_col,
                end_line,
                end_column: end_col,
                message: format!("Heading '{text_to_check}' ends with punctuation '{last_char}'"),
                severity: Severity::Warning,
                fix,
            });
        }

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

    crate::impl_rule_config_sections!(MD026Config);

    fn from_config(config: &crate::config::Config) -> Box<dyn Rule>
    where
        Self: Sized,
    {
        let rule_config = crate::rule_config_serde::load_rule_config::<MD026Config>(config);

        // Check if punctuation was explicitly set in the config; the Gherkin flavor drops
        // the colon from it, and an override the user asked for is worth reporting.
        let punctuation_explicit = config
            .rules
            .get("MD026")
            .is_some_and(|rule_cfg| rule_cfg.values.contains_key("punctuation"));

        Box::new(Self::build(rule_config, punctuation_explicit))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lint_context::LintContext;

    #[test]
    fn test_no_trailing_punctuation() {
        let rule = MD026NoTrailingPunctuation::new(None);
        let content = "# This is a heading\n\n## Another heading";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Headings without punctuation should not be flagged");
    }

    #[test]
    fn test_trailing_period() {
        let rule = MD026NoTrailingPunctuation::new(None);
        let content = "# This is a heading.\n\n## Another one.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].line, 1);
        assert_eq!(result[0].column, 20);
        assert!(result[0].message.contains("ends with punctuation '.'"));
        assert_eq!(result[1].line, 3);
        assert_eq!(result[1].column, 15);
    }

    #[test]
    fn test_trailing_comma() {
        let rule = MD026NoTrailingPunctuation::new(None);
        let content = "# Heading,\n## Sub-heading,";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 2);
        assert!(result[0].message.contains("ends with punctuation ','"));
    }

    #[test]
    fn test_trailing_semicolon() {
        let rule = MD026NoTrailingPunctuation::new(None);
        let content = "# Title;\n## Subtitle;";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 2);
        assert!(result[0].message.contains("ends with punctuation ';'"));
    }

    #[test]
    fn test_custom_punctuation() {
        let rule = MD026NoTrailingPunctuation::new(Some("!".to_string()));
        let content = "# Important!\n## Regular heading.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1, "Only exclamation should be flagged with custom config");
        assert_eq!(result[0].line, 1);
        assert!(result[0].message.contains("ends with punctuation '!'"));
    }

    #[test]
    fn test_legitimate_question_mark() {
        let rule = MD026NoTrailingPunctuation::new(Some(".,;?".to_string()));
        let content = "# What is this?\n# This is bad.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        // With custom punctuation, legitimate punctuation exceptions don't apply
        assert_eq!(result.len(), 2, "Both should be flagged with custom punctuation");
    }

    #[test]
    fn test_question_marks_not_in_default() {
        let rule = MD026NoTrailingPunctuation::new(None);
        let content = "# What is Rust?\n# How does it work?\n# Is it fast?";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Question marks are not in default punctuation list");
    }

    #[test]
    fn test_colons_in_default() {
        let rule = MD026NoTrailingPunctuation::new(None);
        let content = "# FAQ:\n# API Reference:\n# Step 1:\n# Version 2.0:";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(
            result.len(),
            4,
            "Colons are in default punctuation list and should be flagged"
        );
    }

    #[test]
    fn test_fix_atx_headings() {
        let rule = MD026NoTrailingPunctuation::new(None);
        let content = "# Title.\n## Subtitle,\n### Sub-subtitle;";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, "# Title\n## Subtitle\n### Sub-subtitle");
    }

    #[test]
    fn test_fix_atx_heading_on_a_list_marker_line_keeps_the_marker() {
        let rule = MD026NoTrailingPunctuation::new(None);
        for (content, expected) in [
            ("- ## a.\n", "- ## a\n"),
            ("1. # a,\n", "1. # a\n"),
            ("- - ## a: ##\n", "- - ## a ##\n"),
            ("  ## a.\n", "  ## a\n"),
        ] {
            let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
            assert_eq!(rule.fix(&ctx).unwrap(), expected, "{content:?}");
        }
    }

    fn fix_of(content: &str) -> String {
        let rule = MD026NoTrailingPunctuation::new(None);
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        rule.fix(&ctx).unwrap()
    }

    fn check_of(content: &str) -> Vec<crate::rule::LintWarning> {
        let rule = MD026NoTrailingPunctuation::new(None);
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        rule.check(&ctx).unwrap()
    }

    #[test]
    fn test_headings_inside_containers_are_checked_and_fixed() {
        for (content, expected) in [
            ("> ## a.\n", "> ## a\n"),
            ("> # a:\n", "> # a\n"),
            (">## a.\n", ">## a\n"),
            ("> > ## a.\n", "> > ## a\n"),
            ("> - ## a.\n", "> - ## a\n"),
            ("- > ## a.\n", "- > ## a\n"),
            ("- x\n\n  > ## a.\n", "- x\n\n  > ## a\n"),
            ("- x\n\n    > ## a.\n", "- x\n\n    > ## a\n"),
            ("- x\n\n    ## a.\n", "- x\n\n    ## a\n"),
            ("> ## a. ##\n", "> ## a ##\n"),
            ("> ## a. {#id}\n", "> ## a {#id}\n"),
            ("> ## a. ## {#id}\n", "> ## a ## {#id}\n"),
            ("> a.\n> ===\n", "> a\n> ===\n"),
            ("> - a.\n>   ---\n", "> - a\n>   ---\n"),
        ] {
            let warnings = check_of(content);
            assert_eq!(warnings.len(), 1, "{content:?}");
            assert!(warnings[0].fix.is_some(), "{content:?} must be fixable");
            assert_eq!(fix_of(content), expected, "{content:?}");
            assert!(check_of(&fix_of(content)).is_empty(), "{content:?} not clean after fix");
        }
    }

    #[test]
    fn test_quoted_heading_warning_range_excludes_the_quote_prefix() {
        let warnings = check_of("> ## Title.\n");
        assert_eq!(warnings.len(), 1);
        assert_eq!((warnings[0].line, warnings[0].column), (1, 11));
        assert_eq!((warnings[0].end_line, warnings[0].end_column), (1, 12));
    }

    #[test]
    fn test_container_text_that_is_not_a_heading_is_not_flagged() {
        for content in [
            "> a.\n",
            "> ##a.\n",
            "> text\n>\n>     ## a.\n",
            "    ## a.\n",
            "- x\n\n      ## a.\n",
            "```\n> ## a.\n```\n",
        ] {
            assert!(check_of(content).is_empty(), "{content:?}");
        }
    }

    #[test]
    fn test_deletion_fix_handles_closing_sequences_of_any_length() {
        for (content, expected) in [
            ("# a. #######\n", "# a #######\n"),
            ("# a. ## \n", "# a ## \n"),
            ("> ## a. #######\n", "> ## a #######\n"),
        ] {
            assert_eq!(fix_of(content), expected, "{content:?}");
        }
    }

    #[test]
    fn test_fix_keeps_trailing_unicode_whitespace() {
        assert_eq!(fix_of("# a.\u{2003}\n"), "# a\u{2003}\n");
        assert_eq!(fix_of("> ## a.\u{2003}\n"), "> ## a\u{2003}\n");
        assert_eq!(fix_of("# a.\u{a0}\n"), "# a\u{a0}\n");
    }

    #[test]
    fn test_fix_declines_when_removal_would_expose_a_closing_sequence() {
        for content in ["# a #.\n", "> ## a #.\n", "## #.\n", "- ## a ###.\n"] {
            let warnings = check_of(content);
            assert_eq!(warnings.len(), 1, "{content:?}");
            assert!(warnings[0].fix.is_none(), "{content:?} must not offer a fix");
            assert_eq!(fix_of(content), content, "{content:?}");
        }
        assert_eq!(fix_of("# a\\#.\n"), "# a\\#\n");
        assert_eq!(fix_of("# a#.\n"), "# a#\n");
        assert_eq!(fix_of("# a. #\n"), "# a #\n");
    }

    #[test]
    fn test_setext_fix_declines_when_removal_would_change_block_structure() {
        for content in [
            "a\n.\n===\n",
            "> a\n> .\n> ===\n",
            "> .\n> ===\n",
            "> -.\n> ===\n",
            "> ___.\n> ===\n",
            "- item\n\n    > .\n    > ===\n",
        ] {
            let warnings = check_of(content);
            assert_eq!(warnings.len(), 1, "{content:?}");
            assert!(warnings[0].fix.is_none(), "{content:?} must not offer a fix");
            assert_eq!(fix_of(content), content, "{content:?}");
        }
    }

    #[test]
    fn test_line_cannot_open_block() {
        for (line, expected) in [
            ("a", true),
            ("> Version 1.2.3", true),
            ("1.2.3", true),
            ("2024", true),
            ("1x", true),
            ("1.x", true),
            ("\"quoted\"", true),
            ("(note)", true),
            ("*bold* text", true),
            ("_em_", true),
            ("`a`", true),
            ("``a``", true),
            ("~~a~~", true),
            ("**Bold**", true),
            ("[link](x)", true),
            ("![img](x)", true),
            ("\u{201c}quoted\u{201d}", true),
            ("\\# a", true),
            // list markers, thematic breaks, fences and the like need a parse
            ("1.", false),
            ("1. ", false),
            ("1)\t", false),
            ("- ", false),
            ("-", false),
            ("*", false),
            ("* ", false),
            ("***", false),
            ("___", false),
            ("```", false),
            ("```rust", false),
            ("~~~", false),
            ("***", false),
            ("* *", false),
            ("**", false),
            ("[ref]: x", false),
            ("[^1]: x", false),
            ("!!! note", false),
            ("|a|", false),
            (":::", false),
            ("$$", false),
            ("=", false),
            ("<div>", false),
            ("[ref]: x", false),
            ("#", false),
            ("", false),
            ("> ", false),
        ] {
            assert_eq!(line_cannot_open_block(line), expected, "{line:?}");
        }
    }

    #[test]
    fn test_edited_line_cannot_open_block_skips_unchanged_markers() {
        for (before, rest, expected) in [
            ("- a", "", true),
            ("> - a", "", true),
            ("1. a", "", true),
            ("  * Version 1.2", "", true),
            ("- - a", "", true),
            // a marker the edit completes or leaves bare still needs a parse
            ("-", " a", false),
            ("- ", "", false),
            ("- -", "", false),
            ("- 1.", "", false),
            ("> -", "", false),
            ("1.", " a", false),
            ("", "", false),
        ] {
            assert_eq!(
                edited_line_cannot_open_block(before, rest),
                expected,
                "{before:?} + {rest:?}"
            );
        }
    }

    /// Setext headings with several container prefixes and punctuation shapes,
    /// two per document so the second sits in a container the first shared.
    fn container_headings() -> Vec<String> {
        let mut docs = Vec::new();
        for (first, rest) in [
            ("", ""),
            ("- ", "  "),
            ("> ", "> "),
            ("1. ", "   "),
            ("> - ", ">   "),
            ("- > ", "  > "),
            ("   - ", "     "),
            ("* ", "  "),
        ] {
            for body in [
                "a.",
                ".",
                "-.",
                "1.2.",
                "#x.",
                "> a.",
                "* *.",
                "***.",
                "<span>a</span>.",
                "a:",
            ] {
                for underline in ["===", "---"] {
                    let blank = rest.trim_end();
                    docs.push(format!(
                        "{first}{body}\n{rest}{underline}\n{blank}\n{rest}{body}\n{rest}{underline}\n"
                    ));
                }
            }
        }
        docs
    }

    /// The windowed structure check agrees with parsing the whole document, for
    /// every punctuation character that ends a line of the documents below.
    #[test]
    fn test_windowed_structure_check_matches_the_whole_document() {
        for content in [
            "intro\n\na\n.\n===\n\nafter\n",
            "- x\n\n- .\n  ===\n\n- y.\n  ---\n",
            "- item\n\n    > .\n    > ===\n\ntail\n",
            "> a\n> .\n> ===\n\n> b.\n> ===\n",
            "para\n\n> -.\n> ===\n\n1. z\n\n   2) .\n   ===\n",
            "```\ncode\n```\n\n___.\n===\n\n- - .\n    ===\n",
            "> a.\n> ===\n>\n> ---\n>\n> b,\n> ---\n>\n> -.\n> ===\n>\n> - c.\n>   ===\n",
            "- item\n\n  a.\n  ===\n\n  -.\n  ===\n\n  b,\n  ---\n",
            "1. item\n\n   > x.\n   > ===\n   >\n   > .\n   > ===\n",
            "-\t.\n\t===\n\n- y.\n  ===\n",
            "-\t<span>a</span>.\n\t===\n\n-\t<b>c</b>,\n\t---\n",
            ">\t- x.\n>\t  ===\n",
        ]
        .map(String::from)
        .into_iter()
        .chain(container_headings())
        {
            let content = content.as_str();
            let mut windows = None;
            for (i, c) in content.char_indices() {
                // the rule deletes a run at the end of a line
                let at_line_end = content[i + 1..].starts_with('\n');
                if !matches!(c, '.' | ',' | ':') || !at_line_end {
                    continue;
                }
                let range = i..i + 1;
                let mut edited = content.to_string();
                edited.replace_range(range.clone(), "");
                let whole = block_skeleton(content) == block_skeleton(&edited);
                assert_eq!(
                    deletion_keeps_block_structure(content, &range, &mut windows),
                    whole,
                    "{content:?} at {i}"
                );
            }
        }
    }

    #[test]
    fn test_setext_fix_applies_when_structure_is_kept() {
        for (content, expected) in [
            ("a.\n===\n", "a\n===\n"),
            ("> a.\n> ===\n", "> a\n> ===\n"),
            ("a\nb.\n===\n", "a\nb\n===\n"),
            ("- a.\n  ===\n", "- a\n  ===\n"),
            ("1. a.\n   ===\n", "1. a\n   ===\n"),
            ("1.2.3.\n======\n", "1.2.3\n======\n"),
            ("> *Bold.*.\n> ===\n", "> *Bold.*\n> ===\n"),
        ] {
            assert_eq!(fix_of(content), expected, "{content:?}");
        }
    }

    #[test]
    fn test_fix_setext_headings() {
        let rule = MD026NoTrailingPunctuation::new(None);
        let content = "Title.\n======\n\nSubtitle,\n---------";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, "Title\n======\n\nSubtitle\n---------");
    }

    #[test]
    fn test_fix_preserves_trailing_hashes() {
        let rule = MD026NoTrailingPunctuation::new(None);
        let content = "# Title. #\n## Subtitle, ##";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, "# Title #\n## Subtitle ##");
    }

    #[test]
    fn test_indented_headings() {
        let rule = MD026NoTrailingPunctuation::new(None);
        let content = "   # Title.\n  ## Subtitle.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 2, "Indented headings (< 4 spaces) should be checked");
    }

    #[test]
    fn test_deeply_indented_ignored() {
        let rule = MD026NoTrailingPunctuation::new(None);
        let content = "    # This is code.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Deeply indented lines (4+ spaces) should be ignored");
    }

    #[test]
    fn test_multiple_punctuation() {
        let rule = MD026NoTrailingPunctuation::new(None);
        let content = "# Title...";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].column, 8); // Points to first period
    }

    #[test]
    fn test_empty_content() {
        let rule = MD026NoTrailingPunctuation::new(None);
        let content = "";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_no_headings() {
        let rule = MD026NoTrailingPunctuation::new(None);
        let content = "This is just text.\nMore text with punctuation.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Non-heading lines should not be checked");
    }

    #[test]
    fn test_get_punctuation_regex() {
        let rule = MD026NoTrailingPunctuation::new(Some("!?".to_string()));
        let regex = rule.get_punctuation_regex(&rule.config.punctuation).unwrap();
        assert!(regex.is_match("text!"));
        assert!(regex.is_match("text?"));
        assert!(!regex.is_match("text."));
    }

    #[test]
    fn test_regex_caching() {
        let rule1 = MD026NoTrailingPunctuation::new(Some("!".to_string()));
        let rule2 = MD026NoTrailingPunctuation::new(Some("!".to_string()));

        // Both should get the same cached regex
        let _regex1 = rule1.get_punctuation_regex(&rule1.config.punctuation).unwrap();
        let _regex2 = rule2.get_punctuation_regex(&rule2.config.punctuation).unwrap();

        // Check cache has the entry
        let cache = PUNCTUATION_REGEX_CACHE.read().unwrap();
        assert!(cache.contains_key("!"));
    }

    #[test]
    fn test_config_from_toml() {
        let mut config = crate::config::Config::default();
        let mut rule_config = crate::config::RuleConfig::default();
        rule_config
            .values
            .insert("punctuation".to_string(), toml::Value::String("!?".to_string()));
        config.rules.insert("MD026".to_string(), rule_config);

        let rule = MD026NoTrailingPunctuation::from_config(&config);
        let content = "# Title!\n# Another?";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 2, "Custom punctuation from config should be used");
    }

    #[test]
    fn test_fix_removes_punctuation() {
        let rule = MD026NoTrailingPunctuation::new(None);
        let content = "# Title.   \n## Subtitle,  ";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        // The current implementation doesn't preserve trailing whitespace after punctuation removal
        assert_eq!(fixed, "# Title\n## Subtitle");
    }

    #[test]
    fn test_final_newline_preservation() {
        let rule = MD026NoTrailingPunctuation::new(None);
        let content = "# Title.\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, "# Title\n");

        let content_no_newline = "# Title.";
        let ctx2 = LintContext::new(content_no_newline, crate::config::MarkdownFlavor::Standard, None);
        let fixed2 = rule.fix(&ctx2).unwrap();
        assert_eq!(fixed2, "# Title");
    }

    /// The whole MDG matrix in one place, against the Standard behavior it departs from.
    ///
    /// The rule's pattern is `([<punctuation>]+)$`, so taking the colon out of the set is
    /// the entire flavor difference: no heading whose last character is a colon can match
    /// any more, and `## Scenario!:` is left exactly as its author wrote it.
    #[test]
    fn test_mdg_punctuation_matrix() {
        let rule = MD026NoTrailingPunctuation::new(None);
        // (input, standard warnings, standard fix, MDG warnings, MDG fix)
        let cases = [
            ("## Notes:\n", 1, "## Notes\n", 0, "## Notes:\n"),
            ("## Scenario!:\n", 1, "## Scenario\n", 0, "## Scenario!:\n"),
            ("# Scenario! :\n", 1, "# Scenario\n", 0, "# Scenario! :\n"),
            ("## Scenario!\n", 1, "## Scenario\n", 1, "## Scenario\n"),
            ("## Notes::\n", 1, "## Notes\n", 0, "## Notes::\n"),
            (
                "# Feature: Checkout:\n",
                1,
                "# Feature: Checkout\n",
                0,
                "# Feature: Checkout:\n",
            ),
            ("### Rule.:\n", 1, "### Rule\n", 0, "### Rule.:\n"),
        ];

        for (input, standard_count, standard_fixed, mdg_count, mdg_fixed) in cases {
            for (flavor, count, expected) in [
                (crate::config::MarkdownFlavor::Standard, standard_count, standard_fixed),
                (crate::config::MarkdownFlavor::MDG, mdg_count, mdg_fixed),
            ] {
                let ctx = LintContext::new(input, flavor, None);
                assert_eq!(
                    rule.check(&ctx).unwrap().len(),
                    count,
                    "{flavor:?} warning count for {input:?}"
                );

                let fixed = rule.fix(&ctx).unwrap();
                assert_eq!(fixed, expected, "{flavor:?} fix for {input:?}");

                let fixed_ctx = LintContext::new(&fixed, flavor, None);
                assert!(
                    rule.check(&fixed_ctx).unwrap().is_empty(),
                    "{flavor:?} left a warning on the fixed {input:?}"
                );
                assert_eq!(
                    rule.fix(&fixed_ctx).unwrap(),
                    fixed,
                    "{flavor:?} fix for {input:?} should be idempotent"
                );
            }
        }
    }

    /// The colon leaves the effective set whatever `punctuation` says, so a future change
    /// to the default cannot put it back; only an explicit setting is worth reporting.
    #[test]
    fn test_mdg_reports_an_explicitly_configured_colon_once() {
        fn configured(punctuation: &str) -> MD026NoTrailingPunctuation {
            let mut config = crate::config::Config::default();
            let mut rule_config = crate::config::RuleConfig::default();
            rule_config
                .values
                .insert("punctuation".to_string(), toml::Value::String(punctuation.to_string()));
            config.rules.insert("MD026".to_string(), rule_config);

            MD026NoTrailingPunctuation::from_config(&config)
                .as_any()
                .downcast_ref::<MD026NoTrailingPunctuation>()
                .expect("MD026::from_config builds an MD026NoTrailingPunctuation")
                .clone()
        }

        let explicit = configured(".,;:!");
        assert_eq!(
            explicit.effective_punctuation(crate::config::MarkdownFlavor::Standard),
            ".,;:!"
        );
        assert_eq!(
            explicit.effective_punctuation(crate::config::MarkdownFlavor::MDG),
            ".,;!"
        );
        assert!(
            !explicit.mdg_colon_override_applies(crate::config::MarkdownFlavor::Standard),
            "a non-Gherkin file never reports the override"
        );
        assert!(explicit.mdg_colon_override_applies(crate::config::MarkdownFlavor::MDG));

        let emitted_by_check = configured(".,;:!");
        let ctx = LintContext::new("## Scenario!\n", crate::config::MarkdownFlavor::MDG, None);
        assert_eq!(emitted_by_check.check(&ctx).unwrap().len(), 1);

        let emitted_before_skip = configured(".,;:!");
        let only_protected_punctuation = LintContext::new("#### Examples:\n", crate::config::MarkdownFlavor::MDG, None);
        assert!(
            emitted_before_skip.should_skip(&only_protected_punctuation),
            "the post-override punctuation set has nothing to inspect"
        );

        let explicit_without_colon = configured(".,;!");
        assert!(
            !explicit_without_colon.mdg_colon_override_applies(crate::config::MarkdownFlavor::MDG),
            "nothing is overridden when the configured set has no colon"
        );

        let default = MD026NoTrailingPunctuation::new(None);
        assert_eq!(
            default.effective_punctuation(crate::config::MarkdownFlavor::MDG),
            ".,;!",
            "the colon leaves the default set too"
        );
        assert!(
            !default.mdg_colon_override_applies(crate::config::MarkdownFlavor::MDG),
            "the default set is not an explicit configuration, so it is silent"
        );
    }

    #[test]
    fn test_mdg_exempts_a_lone_colon_behind_whitespace() {
        // The colon is not punctuation under MDG, and it is the last character here, so
        // there is nothing for the `$`-anchored pattern to match. Standard still strips it,
        // keeping the whitespace that preceded it.
        let rule = MD026NoTrailingPunctuation::new(None);
        let content = "# Scenario :\n## Notes:\n";

        let standard_ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let standard = rule.check(&standard_ctx).unwrap();
        assert_eq!(standard.len(), 2);
        assert_eq!((standard[0].line, standard[0].column), (1, 12));
        assert_eq!((standard[1].line, standard[1].column), (2, 9));
        let standard_fixed = rule.fix(&standard_ctx).unwrap();
        assert_eq!(standard_fixed, "# Scenario \n## Notes\n");
        let standard_fixed_ctx = LintContext::new(&standard_fixed, crate::config::MarkdownFlavor::Standard, None);
        assert_eq!(
            rule.fix(&standard_fixed_ctx).unwrap(),
            standard_fixed,
            "Standard fix should be idempotent"
        );

        let mdg_ctx = LintContext::new(content, crate::config::MarkdownFlavor::MDG, None);
        assert!(rule.check(&mdg_ctx).unwrap().is_empty());
        assert_eq!(
            rule.fix(&mdg_ctx).unwrap(),
            content,
            "MDG leaves headings whose only trailing punctuation is the colon untouched"
        );
    }

    #[test]
    fn test_mdg_does_not_exempt_a_full_width_colon() {
        // Gherkin only recognizes the ASCII colon, so a full-width one carries
        // no structural meaning and stays in the punctuation set.
        let rule = MD026NoTrailingPunctuation::new(Some(".,;:!?：".to_string()));
        assert_eq!(
            rule.effective_punctuation(crate::config::MarkdownFlavor::MDG),
            ".,;!?：",
            "only the ASCII colon leaves the set"
        );

        let content = "## Scenario：\n";
        for flavor in [
            crate::config::MarkdownFlavor::MDG,
            crate::config::MarkdownFlavor::Standard,
        ] {
            let ctx = LintContext::new(content, flavor, None);
            assert_eq!(rule.check(&ctx).unwrap().len(), 1, "{flavor:?} must flag the `：`");
            assert_eq!(rule.fix(&ctx).unwrap(), "## Scenario\n");
        }
    }
}
