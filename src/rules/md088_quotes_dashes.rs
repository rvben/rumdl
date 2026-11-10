//! Rule MD088: Replace look-alike quotes, dashes and spaces with ASCII equivalents.
//!
//! This rule flags all kinds of Unicode quotes and optionally dashes and spaces
//! that are often introduced by smart-quote substitutions and rich text copy/paste,
//! and replaces them with plain ASCII equivalents.

mod md088_config;

use crate::filtered_lines::FilteredLinesExt;
use crate::lint_context::LintContext;
use crate::rule::{Fix, FixCapability, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::utils::parser_options::rumdl_parser_options_for;
use crate::utils::skip_context::is_in_math_context;
use crate::utils::unicode;
use md088_config::MD088Config;
use pulldown_cmark::{Event, Parser, Tag};
use std::cell::Cell;
use std::cmp::Ordering;
use std::collections::HashSet;
use std::ops::Range;

/// Name of a non-ASCII space character that `normalize-spaces` covers, or `None`
/// for any other character.
///
/// The set is an explicit list rather than `char::is_whitespace`: U+200B belongs to
/// MD084, U+2028/U+2029/U+0085 are line separators, and U+1680 is a script-specific
/// space mark, so none of them are look-alikes for a typed space.
fn space_name(c: char) -> Option<&'static str> {
    Some(match c {
        '\u{00A0}' => "No-Break Space",
        '\u{2000}' => "En Quad",
        '\u{2001}' => "Em Quad",
        '\u{2002}' => "En Space",
        '\u{2003}' => "Em Space",
        '\u{2004}' => "Three-Per-Em Space",
        '\u{2005}' => "Four-Per-Em Space",
        '\u{2006}' => "Six-Per-Em Space",
        '\u{2007}' => "Figure Space",
        '\u{2008}' => "Punctuation Space",
        '\u{2009}' => "Thin Space",
        '\u{200A}' => "Hair Space",
        '\u{202F}' => "Narrow No-Break Space",
        '\u{205F}' => "Medium Mathematical Space",
        '\u{3000}' => "Ideographic Space",
        _ => return None,
    })
}

/// Whether `token` is a block marker that a following space would activate: an ATX
/// heading run, bullet, blockquote or thematic-break character run (any token with
/// no letter or digit), an ordered-list number, or a link reference label.
fn is_block_marker_token(token: &str) -> bool {
    if !token.chars().any(char::is_alphanumeric) {
        return true;
    }
    if let Some(number) = token.strip_suffix(['.', ')']) {
        return (1..=9).contains(&number.len()) && number.bytes().all(|b| b.is_ascii_digit());
    }
    token.starts_with('[') && token.ends_with("]:")
}

/// The byte ranges of the whitespace runs of `line`, in order, in which replacing a
/// space character with U+0020 leaves the line's Markdown structure unchanged.
///
/// CommonMark gives only an ASCII space or tab structural meaning, so a Unicode
/// space can sit where an ASCII one would change the block: `#` + space opens a
/// heading, `1.` + space a list item, a trailing pair a hard break, four leading
/// spaces an indented code block. The substitution is therefore only safe when the
/// whitespace run holding the character has text before and after it on the line
/// and the text before it is not just block markers (which include a footnote or link
/// label such as `[^1]:`). The tail of a URL is excluded as well, because a URL ends at
/// an ASCII space but not necessarily at a Unicode one.
///
/// The line is read once, token by token, so a line holding many spaces costs no more
/// than its length.
fn structure_keeping_space_runs(line: &str) -> Vec<Range<usize>> {
    let mut runs = Vec::new();
    // Whether a token precedes the current position, whether every such token is a
    // block marker, and whether the nearest one is the tail of a URL.
    let mut seen_token = false;
    let mut only_markers = true;
    let mut after_url = false;
    let mut pos = 0;
    while pos < line.len() {
        let rest = &line[pos..];
        let token_start = pos + rest.len() - rest.trim_start_matches(char::is_whitespace).len();
        let token = line[token_start..].split(char::is_whitespace).next().unwrap_or("");
        if token.is_empty() {
            break;
        }
        if token_start > pos && seen_token && !only_markers && !after_url {
            runs.push(pos..token_start);
        }
        seen_token = true;
        only_markers = only_markers && is_block_marker_token(token);
        after_url = token.contains("://") || token.starts_with("www.");
        pos = token_start + token.len();
    }
    runs
}

/// Whether byte `idx` lies in one of the ordered, disjoint `runs`.
fn in_runs(runs: &[Range<usize>], idx: usize) -> bool {
    runs.binary_search_by(|run| {
        if run.end <= idx {
            Ordering::Less
        } else if run.start > idx {
            Ordering::Greater
        } else {
            Ordering::Equal
        }
    })
    .is_ok()
}

/// Appended to a space finding that is reported without a fix.
const STRUCTURE_RISK_NOTE: &str =
    "; not fixed automatically because an ASCII space here could change the Markdown structure";

/// Maps every space `normalize-spaces` covers to an ASCII space, so that text can be
/// compared with and without the replacement applied.
fn fold_spaces(text: &str) -> String {
    text.chars()
        .map(|c| if space_name(c).is_some() { ' ' } else { c })
        .collect()
}

/// The block and inline structure of `content` as the Markdown parser reads it: every
/// event with its kind and tag data, and the text of each run of text, with the covered
/// Unicode spaces folded to ASCII ones.
///
/// Two documents that differ only in which of those spaces are written as ASCII yield
/// the same signature exactly when the substitution did not change how the document
/// parses: a heading, list item, task marker, hard break, table, link or HTML block
/// appearing or disappearing shows as a different event, and a closing-hash sequence or
/// a collapsed trailing space shows as different text.
fn structure_signature(content: &str) -> Vec<String> {
    let mut signature = Vec::new();
    let mut text = String::new();
    for event in Parser::new_ext(content, rumdl_parser_options_for(content)) {
        if let Event::Text(t) = &event {
            text.push_str(&fold_spaces(t));
            continue;
        }
        if !text.is_empty() {
            signature.push(format!("text:{}", std::mem::take(&mut text)));
        }
        signature.push(match &event {
            Event::Code(t) => format!("code:{}", fold_spaces(t)),
            Event::Html(t) => format!("html:{}", fold_spaces(t)),
            Event::InlineHtml(t) => format!("inline-html:{}", fold_spaces(t)),
            Event::InlineMath(t) => format!("inline-math:{}", fold_spaces(t)),
            Event::DisplayMath(t) => format!("display-math:{}", fold_spaces(t)),
            other => format!("{other:?}"),
        });
    }
    if !text.is_empty() {
        signature.push(format!("text:{text}"));
    }
    signature
}

/// `content` with the character at each of the byte offsets `at` replaced by an ASCII
/// space.
fn with_ascii_spaces(content: &str, at: &[usize]) -> String {
    let mut out = String::with_capacity(content.len());
    let mut last = 0;
    for &offset in at {
        out.push_str(&content[last..offset]);
        out.push(' ');
        last = offset + content[offset..].chars().next().map_or(0, char::len_utf8);
    }
    out.push_str(&content[last..]);
    out
}

/// Cap on the text the structure check may parse, as a multiple of the document length
/// (plus a fixed allowance for small documents). Isolating many offending spaces in one
/// block costs a parse of that block per probe; the cap keeps the total linear in the
/// document, and a probe the cap refuses counts as a structure change, so the space it
/// covers is reported without a fix instead of fixed unchecked.
const PARSE_BUDGET_FACTOR: usize = 16;
const PARSE_BUDGET_FLOOR: usize = 16 * 1024;

/// Counts the bytes handed to the parser and refuses further parses once the allowance
/// is spent.
struct ParseBudget {
    remaining: Cell<usize>,
}

impl ParseBudget {
    fn for_document(content: &str) -> Self {
        Self::with_allowance(content.len() * PARSE_BUDGET_FACTOR + PARSE_BUDGET_FLOOR)
    }

    fn with_allowance(bytes: usize) -> Self {
        Self {
            remaining: Cell::new(bytes),
        }
    }

    /// The structure signature of `text`, or `None` when the allowance cannot cover it.
    fn signature(&self, text: &str) -> Option<Vec<String>> {
        self.spend(text.len())?;
        Some(structure_signature(text))
    }

    /// The structure signature of `content` with the characters at `at` replaced by ASCII
    /// spaces, or `None` when the allowance cannot cover it. The allowance is checked
    /// against the length of the result before the result is built, so a refused probe
    /// costs nothing proportional to the block.
    fn signature_with_spaces(&self, content: &str, at: &[usize]) -> Option<Vec<String>> {
        let saved: usize = at
            .iter()
            .map(|&offset| content[offset..].chars().next().map_or(0, |c| c.len_utf8() - 1))
            .sum();
        self.spend(content.len() - saved)?;
        Some(structure_signature(&with_ascii_spaces(content, at)))
    }

    fn spend(&self, bytes: usize) -> Option<()> {
        let left = self.remaining.get();
        if left < bytes {
            self.remaining.set(0);
            return None;
        }
        self.remaining.set(left - bytes);
        Some(())
    }

    /// Whether a probe has been refused, after which every further probe is too.
    fn exhausted(&self) -> bool {
        self.remaining.get() == 0
    }
}

/// The offsets in `candidates` (sorted ascending) whose replacement by an ASCII space
/// would change how `content` parses, either alone or together with the others.
///
/// A group that leaves the signature unchanged is safe as a whole, so a document in which
/// every substitution is harmless costs one extra parse; a group that changes it is split
/// until the offending offsets are isolated. Offsets the budget cannot cover are returned
/// as changing the structure.
fn isolate_structure_changes(content: &str, candidates: &[usize], budget: &ParseBudget) -> HashSet<usize> {
    let Some(base) = budget.signature(content) else {
        return candidates.iter().copied().collect();
    };
    let changes = |group: &[usize]| match budget.signature_with_spaces(content, group) {
        Some(signature) => signature != base,
        None => true,
    };

    fn isolate(group: &[usize], budget: &ParseBudget, changes: &dyn Fn(&[usize]) -> bool, found: &mut HashSet<usize>) {
        if group.is_empty() {
            return;
        }
        // Once the allowance is spent nothing more can be proven safe, so the whole
        // group is reported without descending into it.
        if budget.exhausted() {
            found.extend(group);
            return;
        }
        if !changes(group) {
            return;
        }
        if let [only] = group {
            found.insert(*only);
            return;
        }
        let (left, right) = group.split_at(group.len() / 2);
        isolate(left, budget, changes, found);
        isolate(right, budget, changes, found);
    }

    let mut found = HashSet::new();
    isolate(candidates, budget, &changes, &mut found);

    // Spaces that are harmless alone can still change the structure together (the two
    // spaces of a trailing pair, say). Re-test the remainder, one by one when needed,
    // and give up on whatever still interacts.
    let mut remaining: Vec<usize> = candidates.iter().copied().filter(|c| !found.contains(c)).collect();
    if budget.exhausted() {
        found.extend(remaining);
        return found;
    }
    if changes(&remaining) {
        for &offset in &remaining {
            if budget.exhausted() {
                break;
            }
            if changes(&[offset]) {
                found.insert(offset);
            }
        }
        remaining.retain(|c| !found.contains(c));
        if changes(&remaining) {
            found.extend(remaining);
        }
    }
    found
}

/// Byte offsets, at line starts and ascending, where each top-level block of `content`
/// (and each item of a top-level list) begins. The first entry is always 0.
fn top_level_block_starts(content: &str) -> Vec<usize> {
    let mut starts = vec![0];
    let mut depth = 0usize;
    for (event, range) in Parser::new_ext(content, rumdl_parser_options_for(content)).into_offset_iter() {
        match event {
            Event::Start(tag) => {
                // The items of a top-level list are checked separately too: a list is
                // often most of a document.
                if depth == 0 || (depth == 1 && matches!(tag, Tag::Item)) {
                    starts.push(range.start);
                }
                depth += 1;
            }
            Event::End(_) => depth = depth.saturating_sub(1),
            _ if depth == 0 => starts.push(range.start),
            _ => {}
        }
    }
    for start in &mut starts {
        *start = content[..*start].rfind('\n').map_or(0, |i| i + 1);
    }
    starts.sort_unstable();
    starts.dedup();
    starts
}

/// The offsets in `candidates` (sorted ascending) whose replacement by an ASCII space
/// would change how `content` parses, either alone or together with the others.
///
/// Each top-level block is checked on its own text, so a probe costs a parse of one block
/// rather than of the document. A change can still reach across blocks (a link
/// reference definition whose title is read differently, a fence that opens), so the
/// spaces judged safe are then checked together against the whole document, and
/// isolated there when that fails. Both stages draw on a parse budget; whatever it
/// cannot cover is returned as changing the structure.
fn spaces_changing_structure(content: &str, candidates: &[usize]) -> HashSet<usize> {
    if candidates.is_empty() {
        return HashSet::new();
    }
    let starts = top_level_block_starts(content);
    let budget = ParseBudget::for_document(content);
    let mut found = HashSet::new();
    let mut from = 0;
    for (index, &start) in starts.iter().enumerate() {
        let end = starts.get(index + 1).copied().unwrap_or(content.len());
        let upto = from + candidates[from..].partition_point(|&c| c < end);
        let block = &candidates[from..upto];
        from = upto;
        if block.is_empty() {
            continue;
        }
        let local: Vec<usize> = block.iter().map(|&c| c - start).collect();
        found.extend(
            isolate_structure_changes(&content[start..end], &local, &budget)
                .into_iter()
                .map(|c| c + start),
        );
    }

    let safe: Vec<usize> = candidates.iter().copied().filter(|c| !found.contains(c)).collect();
    if !safe.is_empty() {
        found.extend(isolate_structure_changes(
            content,
            &safe,
            &ParseBudget::for_document(content),
        ));
    }
    found
}

/// Rule MD088: Look-alike punctuation.
#[derive(Debug, Clone)]
pub struct MD088QuotesDashes {
    config: MD088Config,
}

impl Default for MD088QuotesDashes {
    fn default() -> Self {
        Self::from_config_struct(MD088Config::default())
    }
}

impl MD088QuotesDashes {
    fn from_config_struct(config: MD088Config) -> Self {
        Self { config }
    }

    #[inline]
    fn is_allowed(&self, c: char) -> bool {
        self.config.allow.contains(&c)
    }

    #[inline]
    fn replacement_for(&self, c: char) -> Option<&'static str> {
        if self.is_allowed(c) {
            return None;
        }

        // U+02BC MODIFIER LETTER APOSTROPHE is deliberately absent. It is a
        // Modifier_Letter, not punctuation: Crimean Tatar, Nenets and Chechen spell
        // words with it, so rewriting it to ASCII changes a word rather than its
        // typography. U+02BB, the Hawaiian okina, is the same category and is absent
        // for the same reason.
        match c {
            '\u{2018}' | '\u{2019}' | '\u{201A}' | '\u{201B}' | '\u{2032}' if self.config.normalize_quotes => Some("'"),
            '\u{201C}' | '\u{201D}' | '\u{201E}' | '\u{201F}' | '\u{2033}' if self.config.normalize_quotes => {
                Some("\"")
            }
            '\u{2010}' | '\u{2011}' | '\u{2012}' | '\u{2013}' | '\u{2014}' | '\u{2015}'
                if self.config.normalize_dashes =>
            {
                Some("-")
            }
            c if self.config.normalize_spaces && space_name(c).is_some() => Some(" "),
            _ => None,
        }
    }

    #[inline]
    fn has_target_char(&self, ctx: &LintContext) -> bool {
        ctx.content.chars().any(|c| self.replacement_for(c).is_some())
    }
}

impl Rule for MD088QuotesDashes {
    fn name(&self) -> &'static str {
        "MD088"
    }

    fn description(&self) -> &'static str {
        "Quotes and dashes should be replaced with ASCII equivalents"
    }

    fn category(&self) -> RuleCategory {
        RuleCategory::Whitespace
    }

    fn fix_capability(&self) -> FixCapability {
        FixCapability::FullyFixable
    }

    fn should_skip(&self, ctx: &LintContext) -> bool {
        ctx.content.is_empty() || !self.has_target_char(ctx)
    }

    fn check(&self, ctx: &LintContext) -> LintResult {
        let mut warnings = Vec::new();
        // (warning index, absolute byte) of each space finding that carries a fix.
        let mut space_fixes: Vec<(usize, usize)> = Vec::new();

        for line in ctx.filtered_lines().skip_front_matter().skip_code_blocks() {
            // Computed for the first space finding of the line and shared by the rest.
            let mut safe_space_runs: Option<Vec<Range<usize>>> = None;
            for (char_idx, (byte_idx, c)) in line.content.char_indices().enumerate() {
                let Some(replacement) = self.replacement_for(c) else {
                    continue;
                };

                let absolute_byte = line.line_info.byte_offset + byte_idx;
                if ctx.is_byte_offset_in_code_span(absolute_byte) {
                    continue;
                }

                // Math is notation, not prose: `′` and `″` are the prime and double
                // prime of a derivative or a unit, and their ASCII look-alikes do not
                // typeset as primes.
                if is_in_math_context(ctx, absolute_byte) {
                    continue;
                }

                let column = char_idx + 1;
                let space_name = space_name(c);
                let structural_risk = space_name.is_some()
                    && (ctx.is_in_html_tag(absolute_byte)
                        || !in_runs(
                            safe_space_runs.get_or_insert_with(|| structure_keeping_space_runs(line.content)),
                            byte_idx,
                        ));
                let fix = if ctx.is_in_link(absolute_byte)
                    || ctx.is_in_bare_url(absolute_byte)
                    || ctx.is_in_unresolved_autoref_identifier(absolute_byte)
                    || structural_risk
                {
                    None
                } else {
                    Some(Fix::new(
                        absolute_byte..absolute_byte + c.len_utf8(),
                        replacement.to_string(),
                    ))
                };

                // A space is invisible, so the diagnostic names the code point rather
                // than printing the character.
                let message = match space_name {
                    Some(name) => {
                        let mut message = format!(
                            "Unicode space {} ({}) should be replaced with an ASCII space",
                            unicode::format_codepoint(c),
                            name
                        );
                        if structural_risk {
                            message.push_str(STRUCTURE_RISK_NOTE);
                        }
                        message
                    }
                    None => format!(
                        "Unicode character {} ({}) should be replaced with {}",
                        c,
                        unicode::format_codepoint(c),
                        replacement
                    ),
                };

                if space_name.is_some() && fix.is_some() {
                    space_fixes.push((warnings.len(), absolute_byte));
                }
                warnings.push(LintWarning {
                    rule_name: Some(self.name().to_string()),
                    line: line.line_num,
                    column,
                    end_line: line.line_num,
                    end_column: column + 1,
                    severity: Severity::Warning,
                    message,
                    fix,
                });
            }
        }

        // The cheap checks above only rule spaces out. A fix is kept when the parser
        // reads the document the same way with the space replaced.
        if !space_fixes.is_empty() {
            let offsets: Vec<usize> = space_fixes.iter().map(|&(_, offset)| offset).collect();
            let unsafe_offsets = spaces_changing_structure(ctx.content, &offsets);
            for (index, offset) in space_fixes {
                if unsafe_offsets.contains(&offset) {
                    let warning = &mut warnings[index];
                    warning.fix = None;
                    warning.message.push_str(STRUCTURE_RISK_NOTE);
                }
            }
        }

        Ok(warnings)
    }

    fn fixes_from_diagnostics(&self) -> bool {
        true
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
        crate::utils::fix_utils::apply_warning_fixes(ctx.content, &warnings)
            .map_err(crate::rule::LintError::InvalidInput)
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    crate::impl_rule_config_methods!(MD088Config);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Config, MarkdownFlavor};

    #[test]
    fn test_detects_and_fixes_single_and_double_quotes_look_alikes() {
        let rule = MD088QuotesDashes::default();
        let ctx = LintContext::new("It\u{2019}s \u{201C}fine\u{201D}.", MarkdownFlavor::Standard, None);

        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 3);
        assert!(warnings.iter().all(|w| w.fix.is_some()));

        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, "It's \"fine\".");
    }

    #[test]
    fn test_detects_and_fixes_dash_look_alikes() {
        let rule = MD088QuotesDashes::from_config_struct(MD088Config {
            normalize_dashes: true,
            ..Default::default()
        });
        let ctx = LintContext::new(
            "Dash variants: \u{2010}\u{2011}\u{2012}\u{2013}\u{2014}\u{2015}",
            MarkdownFlavor::Standard,
            None,
        );

        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 6);
        assert!(warnings.iter().all(|w| w.fix.is_some()));

        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, "Dash variants: ------");
    }

    #[test]
    fn test_detects_and_fixes_prime_marks() {
        let rule = MD088QuotesDashes::default();
        let ctx = LintContext::new("Sizes: 6\u{2032} and 8\u{2033}", MarkdownFlavor::Standard, None);

        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 2);
        assert!(warnings.iter().all(|w| w.fix.is_some()));

        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, "Sizes: 6' and 8\"");
    }

    #[test]
    fn test_modifier_letters_are_spelling_not_typography() {
        // U+02BC and U+02BB are Modifier_Letter, so they spell words rather than quote
        // them. Rewriting either to ASCII would change `Qaradenʼiz` into a different
        // word, which no `allow` entry should be needed to prevent.
        let rule = MD088QuotesDashes::default();
        let content = "Qaraden\u{02BC}iz and Hawai\u{02BB}i.";
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

        assert!(
            rule.check(&ctx).unwrap().is_empty(),
            "a modifier letter is part of a word's spelling"
        );
        assert_eq!(rule.fix(&ctx).unwrap(), content, "fix must not respell the words");

        // Control: a real quotation mark in the same sentence is still normalized.
        let ctx = LintContext::new("Qaraden\u{02BC}iz \u{2019}s.", MarkdownFlavor::Standard, None);
        assert_eq!(rule.check(&ctx).unwrap().len(), 1);
        assert_eq!(rule.fix(&ctx).unwrap(), "Qaraden\u{02BC}iz 's.");
    }

    #[test]
    fn test_skips_math() {
        // In math, `′` and `″` are prime and double prime. ASCII `'` renders as a prime
        // but `"` does not, so normalizing changes the notation the reader sees.
        let rule = MD088QuotesDashes::default();
        let content = "Prose \u{2019}quote.\n\n$$\nx\u{2032} = y\u{2033}\n$$\n\nInline $a\u{2033}$ too.\n";
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(
            warnings.len(),
            1,
            "only the prose quote is a finding, got {warnings:#?}"
        );
        assert_eq!(warnings[0].line, 1);
        assert_eq!(
            rule.fix(&ctx).unwrap(),
            "Prose 'quote.\n\n$$\nx\u{2032} = y\u{2033}\n$$\n\nInline $a\u{2033}$ too.\n"
        );

        // Control: the same primes outside math are still normalized, so the guard is
        // about math and not about primes.
        let ctx = LintContext::new("Sizes: 6\u{2032} and 8\u{2033}.", MarkdownFlavor::Standard, None);
        assert_eq!(rule.check(&ctx).unwrap().len(), 2);
        assert_eq!(rule.fix(&ctx).unwrap(), "Sizes: 6' and 8\".");
    }

    #[test]
    fn test_skips_inline_code_spans() {
        let rule = MD088QuotesDashes::default();
        let content = "Prose \u{2019}quote and `code \u{2019}quote`";
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].column, 7);
        assert_eq!(rule.fix(&ctx).unwrap(), "Prose 'quote and `code \u{2019}quote`");
    }

    #[test]
    fn test_skips_fenced_and_indented_code_blocks() {
        let rule = MD088QuotesDashes::default();
        let content = r#"
This is a ‘quote in prose’.

```
This is a ‘quote in fenced code block’
```

    This is a ‘quote in indented code block’
"#;
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 2);
        assert_eq!(
            rule.fix(&ctx).unwrap(),
            r#"
This is a 'quote in prose'.

```
This is a ‘quote in fenced code block’
```

    This is a ‘quote in indented code block’
"#
        );
    }

    #[test]
    fn test_no_findings_for_plain_ascii_quotes() {
        let rule = MD088QuotesDashes::default();
        let content = "He said, \"it's fine\".";
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

        let warnings = rule.check(&ctx).unwrap();
        assert!(warnings.is_empty());
        assert_eq!(rule.fix(&ctx).unwrap(), content);
    }

    #[test]
    fn test_allow_list_keeps_configured_codepoints() {
        let config: Config = toml::from_str(
            r#"
            [MD088]
            allow = ["U+2019", "U+2014"]
            "#,
        )
        .unwrap();

        let rule = MD088QuotesDashes::from_config(&config);
        let rule = rule.as_any().downcast_ref::<MD088QuotesDashes>().unwrap();

        let content = "It\u{2019}s \u{2014} and \u{201C}quoted\u{201D}";
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        assert_eq!(fixed, "It\u{2019}s \u{2014} and \"quoted\"");
    }

    #[test]
    fn test_reports_but_does_not_fix_bare_urls() {
        let config: Config = toml::from_str(
            r#"
            [MD088]
            normalize-quotes = true
            normalize-dashes = true
            "#,
        )
        .unwrap();

        let rule = MD088QuotesDashes::from_config(&config);
        let rule = rule.as_any().downcast_ref::<MD088QuotesDashes>().unwrap();

        let content = "Visit https://this\u{2010}site.com and \u{201C}enjoy\u{201D}!.";
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 3, "Expected 3 warnings, got {warnings:#?}");

        let url_warning = warnings.iter().find(|w| w.column > 10).unwrap();
        assert!(url_warning.fix.is_none());

        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, "Visit https://this\u{2010}site.com and \"enjoy\"!.");
    }

    #[test]
    fn test_reports_but_does_not_fix_findings_in_link_destinations() {
        let rule = MD088QuotesDashes::from_config_struct(MD088Config {
            normalize_quotes: true,
            normalize_dashes: true,
            ..Default::default()
        });
        let rule = rule.as_any().downcast_ref::<MD088QuotesDashes>().unwrap();

        let content = "[link](https://example\u{2010}\u{2018}.com) and prose \u{2010}.";
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 3, "Expected 3 warnings, got {warnings:#?}");

        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, "[link](https://example\u{2010}\u{2018}.com) and prose -.");
    }

    // ---- normalize-spaces ----

    fn spaces_rule() -> MD088QuotesDashes {
        MD088QuotesDashes::from_config_struct(MD088Config {
            normalize_spaces: true,
            ..Default::default()
        })
    }

    fn check_spaces(content: &str) -> (Vec<LintWarning>, String) {
        let rule = spaces_rule();
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        (rule.check(&ctx).unwrap(), rule.fix(&ctx).unwrap())
    }

    /// Block structure of `content` per pulldown-cmark, with text stripped: the
    /// shape a space substitution must not change.
    fn block_shape(content: &str) -> Vec<String> {
        use pulldown_cmark::{Event, Options, Parser};
        Parser::new_ext(content, Options::all())
            .filter_map(|event| match event {
                Event::Start(tag) => Some(format!("start {tag:?}")),
                Event::End(tag) => Some(format!("end {tag:?}")),
                Event::HardBreak => Some("hardbreak".to_string()),
                Event::Rule => Some("rule".to_string()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn test_spaces_reported_and_fixed_in_prose() {
        let (warnings, fixed) = check_spaces("a\u{00A0}b and c\u{202F}d");
        assert_eq!(warnings.len(), 2, "{warnings:#?}");
        assert!(warnings.iter().all(|w| w.fix.is_some()));
        assert_eq!(fixed, "a b and c d");
        assert_eq!((warnings[0].column, warnings[1].column), (2, 10));
    }

    #[test]
    fn test_space_diagnostics_name_the_code_point_not_the_glyph() {
        let (warnings, _) = check_spaces("a\u{00A0}b c\u{202F}d");
        assert!(warnings[0].message.contains("U+00A0"), "{}", warnings[0].message);
        assert!(
            warnings[0].message.contains("No-Break Space"),
            "{}",
            warnings[0].message
        );
        assert!(warnings[1].message.contains("U+202F"), "{}", warnings[1].message);
        assert!(
            warnings[1].message.contains("Narrow No-Break Space"),
            "{}",
            warnings[1].message
        );
        for w in &warnings {
            assert!(
                !w.message.chars().any(|c| c.is_whitespace() && c != ' '),
                "invisible glyph printed: {:?}",
                w.message
            );
        }
    }

    #[test]
    fn test_every_listed_space_is_normalized() {
        let listed: Vec<char> = std::iter::once('\u{00A0}')
            .chain('\u{2000}'..='\u{200A}')
            .chain(['\u{202F}', '\u{205F}', '\u{3000}'])
            .collect();
        for c in listed {
            let content = format!("x{c}y");
            let (warnings, fixed) = check_spaces(&content);
            assert_eq!(warnings.len(), 1, "U+{:04X} not reported", c as u32);
            assert_eq!(fixed, "x y", "U+{:04X} not fixed", c as u32);
        }
    }

    #[test]
    fn test_characters_owned_elsewhere_are_not_spaces_to_normalize() {
        // U+200B belongs to MD084, U+2028/2029/0085 are line separators, and U+1680
        // is the Ogham space mark, a script-specific character.
        for c in ['\u{200B}', '\u{2028}', '\u{2029}', '\u{0085}', '\u{1680}'] {
            let content = format!("x{c}y");
            let (warnings, fixed) = check_spaces(&content);
            assert!(warnings.is_empty(), "U+{:04X} reported: {warnings:#?}", c as u32);
            assert_eq!(fixed, content);
        }
    }

    #[test]
    fn test_default_config_leaves_spaces_alone() {
        let rule = MD088QuotesDashes::default();
        let content = "a\u{00A0}b c\u{202F}d\u{3000}e";
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        assert!(rule.should_skip(&ctx));
        assert!(rule.check(&ctx).unwrap().is_empty());
        assert_eq!(rule.fix(&ctx).unwrap(), content);

        // Control: the same rule still handles a quote in the same text.
        let content = "a\u{00A0}b \u{2019}c";
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        assert_eq!(rule.check(&ctx).unwrap().len(), 1);
        assert_eq!(rule.fix(&ctx).unwrap(), "a\u{00A0}b 'c");
    }

    #[test]
    fn test_spaces_option_is_independent_of_quotes_and_dashes() {
        let rule = MD088QuotesDashes::from_config_struct(MD088Config {
            normalize_quotes: false,
            normalize_spaces: true,
            ..Default::default()
        });
        let content = "it\u{2019}s a\u{00A0}b";
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        assert_eq!(rule.check(&ctx).unwrap().len(), 1);
        assert_eq!(rule.fix(&ctx).unwrap(), "it\u{2019}s a b");
    }

    #[test]
    fn test_allow_exempts_spaces() {
        let config: Config = toml::from_str(
            r#"
            [MD088]
            normalize-spaces = true
            allow = ["U+202F"]
            "#,
        )
        .unwrap();
        let rule = MD088QuotesDashes::from_config(&config);
        let rule = rule.as_any().downcast_ref::<MD088QuotesDashes>().unwrap();

        let content = "Quoi\u{202F}? Oui\u{00A0}!";
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 1, "{warnings:#?}");
        assert!(warnings[0].message.contains("U+00A0"));
        assert_eq!(rule.fix(&ctx).unwrap(), "Quoi\u{202F}? Oui !");
    }

    #[test]
    fn test_spaces_skipped_in_code_front_matter_and_math() {
        let content = "---\ntitle: a\u{00A0}b\n---\n\nCode `a\u{00A0}b` here.\n\n```\na\u{00A0}b\n```\n\n    a\u{00A0}b\n\n$$\nx\u{00A0}= 1\n$$\n\nInline $a\u{00A0}b$ too.\n";
        let (warnings, fixed) = check_spaces(content);
        assert!(warnings.is_empty(), "{warnings:#?}");
        assert_eq!(fixed, content);

        // Control: the same character in prose is reported.
        let (warnings, _) = check_spaces("Prose a\u{00A0}b.\n");
        assert_eq!(warnings.len(), 1);
    }

    #[test]
    fn test_spaces_in_link_destinations_and_urls_are_reported_without_fix() {
        let content = "[l](https://example.com/a\u{00A0}b) https://example.com/c\u{00A0}d and e\u{00A0}f";
        let (warnings, fixed) = check_spaces(content);
        assert_eq!(warnings.len(), 3, "{warnings:#?}");
        assert!(warnings[0].fix.is_none());
        assert!(warnings[1].fix.is_none());
        assert!(warnings[2].fix.is_some());
        assert_eq!(
            fixed,
            "[l](https://example.com/a\u{00A0}b) https://example.com/c\u{00A0}d and e f"
        );
    }

    #[test]
    fn test_space_fix_never_creates_markdown_structure() {
        // Each of these is a paragraph today. Replacing the space would make it a
        // heading, list item, quote, thematic break, definition, task item, hard
        // break, indented code block or an HTML tag, so the space is reported only.
        let reported_only = [
            "#\u{00A0}x",
            "###\u{00A0}x",
            "1.\u{00A0}x",
            "1)\u{00A0}x",
            "-\u{00A0}x",
            "+\u{00A0}x",
            "*\u{00A0}x",
            "- 1.\u{00A0}x",
            ">\u{00A0}x",
            "> -\u{00A0}x",
            "*\u{00A0}*\u{00A0}*",
            "-\u{00A0}-\u{00A0}-",
            "[a]:\u{00A0}/url",
            "[^1]:\u{00A0}note",
            "[a]: /url\u{00A0}\"title\"",
            "see https://example.com/c\u{00A0}d",
            "see www.example.com/c\u{00A0}d",
            "- [\u{00A0}] task",
            "foo\u{00A0}",
            "foo\u{00A0}\u{00A0}",
            "foo\u{00A0} ",
            "\u{00A0}foo",
            "\u{00A0}\u{00A0}\u{00A0}\u{00A0}code",
            "<a\u{00A0}href=\"x\">t</a>",
        ];
        for content in reported_only {
            let document = format!("para\n\n{content}\n\nend\n");
            let (warnings, fixed) = check_spaces(&document);
            assert!(!warnings.is_empty(), "{content:?}: not reported");
            assert!(
                warnings.iter().all(|w| w.fix.is_none()),
                "{content:?}: offered a fix: {warnings:#?}"
            );
            assert_eq!(fixed, document, "{content:?}: fix changed the document");
        }
    }

    #[test]
    fn test_space_fix_applies_where_structure_cannot_change() {
        let fixable = [
            ("100\u{00A0}km", "100 km"),
            ("foo\u{00A0}bar", "foo bar"),
            ("a.\u{00A0}b", "a. b"),
            ("# Title\u{00A0}here", "# Title here"),
            ("- item\u{00A0}x", "- item x"),
            ("1. item\u{00A0}x", "1. item x"),
            ("> quote\u{00A0}x", "> quote x"),
            ("Hello\u{00A0}\u{00A0}world", "Hello  world"),
            ("Hello \u{00A0}world", "Hello  world"),
            ("| a\u{00A0}b | c |", "| a b | c |"),
            ("Quoi\u{202F}?", "Quoi ?"),
        ];
        for (content, expected) in fixable {
            let document = format!("para\n\n{content}\n\nend\n");
            let (warnings, fixed) = check_spaces(&document);
            assert!(!warnings.is_empty(), "{content:?}: not reported");
            assert!(warnings.iter().all(|w| w.fix.is_some()), "{content:?}: {warnings:#?}");
            assert_eq!(fixed, format!("para\n\n{expected}\n\nend\n"), "{content:?}");
            assert_eq!(
                block_shape(&fixed),
                block_shape(&document),
                "{content:?}: the fix changed the block structure"
            );
        }
    }

    #[test]
    fn test_space_fix_with_following_lines_keeps_structure() {
        // A paragraph continuation line that starts after a marker-like word, and a
        // line ending in a lone space before a blank line.
        let document = "foo\u{00A0}bar\nbaz\u{00A0}\n\nqux\n";
        let (warnings, fixed) = check_spaces(document);
        assert_eq!(warnings.len(), 2);
        assert!(warnings[0].fix.is_some());
        assert!(warnings[1].fix.is_none(), "trailing space is reported only");
        assert_eq!(fixed, "foo bar\nbaz\u{00A0}\n\nqux\n");
        assert_eq!(block_shape(&fixed), block_shape(document));
    }

    #[test]
    fn test_spaces_with_crlf_line_endings() {
        let document = "a\u{00A0}b\r\nc\u{00A0}\r\n\r\n#\u{00A0}x\r\nd\u{00A0}e\r\n";
        let (warnings, fixed) = check_spaces(document);
        assert_eq!(warnings.len(), 4, "{warnings:#?}");
        let fixes: Vec<bool> = warnings.iter().map(|w| w.fix.is_some()).collect();
        assert_eq!(fixes, [true, false, false, true], "{warnings:#?}");
        assert_eq!(fixed, "a b\r\nc\u{00A0}\r\n\r\n#\u{00A0}x\r\nd e\r\n");
    }

    #[test]
    fn test_space_fix_is_idempotent() {
        let document = "---\ntitle: a\u{00A0}b\n---\n\nIt\u{2019}s a\u{00A0}b.\n\n#\u{00A0}x\n\nfoo\u{00A0}\n\n- 1.\u{00A0}y\u{202F}z\n";
        let rule = MD088QuotesDashes::from_config_struct(MD088Config {
            normalize_spaces: true,
            normalize_dashes: true,
            ..Default::default()
        });
        let ctx = LintContext::new(document, MarkdownFlavor::Standard, None);
        let once = rule.fix(&ctx).unwrap();
        assert_ne!(once, document, "the fix must change something");
        let ctx = LintContext::new(&once, MarkdownFlavor::Standard, None);
        let twice = rule.fix(&ctx).unwrap();
        assert_eq!(once, twice);
    }

    /// Block structure and text of `content` per pulldown-cmark with every option on,
    /// the covered spaces folded to ASCII ones: what a substitution must not change.
    fn parsed_shape(content: &str) -> Vec<String> {
        use pulldown_cmark::{Event, Options, Parser};
        let mut shape = Vec::new();
        let mut text = String::new();
        for event in Parser::new_ext(content, Options::all()) {
            if let Event::Text(t) = &event {
                text.extend(t.chars().map(|c| if space_name(c).is_some() { ' ' } else { c }));
                continue;
            }
            if !text.is_empty() {
                shape.push(format!("text {}", std::mem::take(&mut text)));
            }
            shape.push(format!("{event:?}").replace(['\u{a0}', '\u{202f}', '\u{3000}'], " "));
        }
        if !text.is_empty() {
            shape.push(format!("text {text}"));
        }
        shape
    }

    /// Fixing `doc` must leave the parse unchanged; returns (findings, fixes offered, fixed text).
    fn fix_keeps_parse(doc: &str) -> (usize, usize, String) {
        let (warnings, fixed) = check_spaces(doc);
        assert_eq!(
            parsed_shape(&fixed),
            parsed_shape(doc),
            "fix changed the parse of {doc:?} -> {fixed:?}"
        );
        let offered = warnings.iter().filter(|w| w.fix.is_some()).count();
        (warnings.len(), offered, fixed)
    }

    #[test]
    fn test_space_fix_is_withheld_where_the_parser_reads_the_result_differently() {
        // Each document has exactly one covered space, shown as U+00A0, and an ASCII
        // space there would change the parse; the finding stays without a fix.
        let refused = [
            "# title\u{a0}###\n",                   // closing hash sequence swallows the hashes
            "## title\u{a0}##   \n",                // same, with trailing spaces
            "- [x]\u{a0}text\n",                    // task list marker
            "- [ ]\u{a0}text\n",                    // unchecked task marker
            "Title\n===\u{a0}\n",                   // setext underline
            "Title\n---\u{a0}\n",                   // setext underline, dashes
            "*\u{a0}*\u{a0}*\n",                    // thematic break
            "- -\u{a0}-\n",                         // thematic break, dashes
            "| a | b |\n|---|\u{a0}---|\n",         // delimiter row that stops being invalid
            "[a]:\u{a0}/url\n\n[a]\n",              // link reference definition
            "[a]: /url\u{a0}\"title\"\n\n[a]\n",    // definition title
            "<div\u{a0}class=\"x\">\nhi\n</div>\n", // HTML block start
            "foo \u{a0}\nbar\n",                    // hard break
            "foo\u{a0}\u{a0}\nbar\n",               // hard break from two spaces
            "text\n\n  \u{a0}  code\n",             // indented code
            ">\u{a0}quote\n",                       // block quote marker
            "-\u{a0}item\n",                        // bullet
            "1.\u{a0}item\n",                       // ordered marker
            "*\u{a0}a*\n",                          // emphasis marker turned list marker
            "- a\n  -\u{a0}b\n",                    // nested bullet
            "> -\u{a0}b\n",                         // bullet in a quote
        ];
        for doc in refused {
            let (found, offered, fixed) = fix_keeps_parse(doc);
            assert_eq!(found, doc.matches('\u{a0}').count(), "{doc:?}");
            assert_eq!(offered, 0, "a fix was offered for {doc:?}");
            assert_eq!(fixed, doc, "{doc:?}");
        }
    }

    #[test]
    fn test_space_fix_is_offered_where_the_parse_cannot_change() {
        let applied = [
            ("100\u{a0}km\n", "100 km\n"),
            ("# title\u{a0}text\n", "# title text\n"),
            ("- [x] done\u{a0}today\n", "- [x] done today\n"),
            ("x *\u{a0}a* y\n", "x * a* y\n"),
            (
                "| a | b |\n|---|---|\n| c\u{a0}d | e |\n",
                "| a | b |\n|---|---|\n| c d | e |\n",
            ),
            ("> quote\u{a0}text\n", "> quote text\n"),
            ("Title\u{a0}one\n===\n", "Title one\n===\n"),
            ("1. item\u{a0}two\n", "1. item two\n"),
        ];
        for (doc, expected) in applied {
            let (found, offered, fixed) = fix_keeps_parse(doc);
            assert_eq!((found, offered), (1, 1), "{doc:?}");
            assert_eq!(fixed, expected);
        }
    }

    #[test]
    fn test_space_fix_withholds_only_the_offending_spaces_of_a_document() {
        // Many harmless spaces and one that closes a heading's hash run: the harmless
        // ones are still fixed, so the parser check is not all-or-nothing.
        let mut doc = String::new();
        for i in 0..40 {
            doc.push_str(&format!("line{i} a\u{a0}b\n\n"));
        }
        doc.push_str("# title\u{a0}###\n\ntail c\u{a0}d\n");
        let (found, offered, fixed) = fix_keeps_parse(&doc);
        assert_eq!((found, offered), (42, 41));
        assert!(fixed.contains("# title\u{a0}###\n"));
        assert!(fixed.contains("tail c d"));
        assert!(!fixed[..fixed.find("# title").unwrap()].contains('\u{a0}'));
    }

    #[test]
    fn test_space_fix_withholds_spaces_that_only_interact() {
        // Alone, either space leaves the line a plain paragraph; replaced together they
        // would form a hard break. Neither may be fixed.
        let doc = "foo\u{a0}\u{a0}\nbar\n";
        let (found, offered, fixed) = fix_keeps_parse(doc);
        assert_eq!((found, offered), (2, 0));
        assert_eq!(fixed, doc);
    }

    #[test]
    fn test_spaces_that_change_the_parse_only_together_are_all_flagged() {
        // Replacing either space alone leaves the second row an invalid delimiter row;
        // replacing both turns the lines into a table.
        let doc = "| a | b |\n|---\u{a0}|\u{a0}---|\n";
        let offsets: Vec<usize> = doc.match_indices('\u{a0}').map(|(i, _)| i).collect();
        assert_eq!(offsets.len(), 2);
        for &single in &offsets {
            assert!(spaces_changing_structure(doc, &[single]).is_empty());
        }
        let flagged = spaces_changing_structure(doc, &offsets);
        assert_eq!(flagged, offsets.iter().copied().collect::<HashSet<_>>());

        // A harmless space elsewhere is not flagged when the interacting pair is isolated.
        let doc = format!("{doc}\nplain a\u{a0}b\n");
        let all: Vec<usize> = doc.match_indices('\u{a0}').map(|(i, _)| i).collect();
        let flagged = spaces_changing_structure(&doc, &all);
        assert!(flagged.contains(&all[0]) && flagged.contains(&all[1]));
    }

    #[test]
    fn test_space_fix_is_withheld_where_the_change_reaches_another_block() {
        // Each space is harmless within its own block read alone; together with the
        // neighbouring block the parse changes, so no fix may be offered.
        let refused = [
            "- a\n\n\u{a0}\u{a0}b\n",                 // text joins the list item above
            "[a]: /url\u{a0}\"title\"\n\n[a]\n",      // definition read differently by a later link
            "\u{a0}```\nx\n\n# h\n\n```\n",           // a fence opens and swallows what follows
            "para\n\n\u{a0}\u{a0}\u{a0}\u{a0}code\n", // indented code
            "[a\u{a0}b]: /url\n\n[a b]\n",            // a later reference starts to resolve
        ];
        for doc in refused {
            let (found, offered, fixed) = fix_keeps_parse(doc);
            assert_eq!(found, doc.matches('\u{a0}').count(), "{doc:?}");
            assert_eq!(offered, 0, "a fix was offered for {doc:?}");
            assert_eq!(fixed, doc);
        }
    }

    #[test]
    fn test_space_guard_catches_a_change_visible_only_in_another_block() {
        // The definition reads the same alone with either space; the link in a later
        // block resolves only with the ASCII one. Blocks are checked apart, so the
        // spaces judged safe are checked once more against the whole document.
        for doc in [
            "[a\u{a0}b]: /url\n\n# h\n\n[a b]\n",
            "[a]: /url\u{a0}\"t\"\n\n# h\n\n[a]\n",
        ] {
            let offsets: Vec<usize> = doc.match_indices('\u{a0}').map(|(i, _)| i).collect();
            let starts = top_level_block_starts(doc);
            assert!(starts.len() > 2, "{doc:?}");
            let alone = isolate_structure_changes(&doc[..starts[1]], &offsets, &ParseBudget::for_document(doc));
            assert!(alone.is_empty(), "the block alone must look harmless: {doc:?}");
            let flagged = spaces_changing_structure(doc, &offsets);
            assert_eq!(flagged, offsets.iter().copied().collect::<HashSet<_>>(), "{doc:?}");
        }
    }

    #[test]
    fn test_space_guard_budget_never_turns_into_an_unchecked_fix() {
        let doc = "a\u{a0}b\n\nc\u{a0}d\n";
        let candidates: Vec<usize> = doc.match_indices('\u{a0}').map(|(i, _)| i).collect();
        let all: HashSet<usize> = candidates.iter().copied().collect();

        // Without a cap both are proven harmless.
        let ample = ParseBudget::for_document(doc);
        assert!(isolate_structure_changes(doc, &candidates, &ample).is_empty());

        // A cap too small to parse the document leaves every offset unproven.
        let none = ParseBudget::with_allowance(0);
        assert_eq!(isolate_structure_changes(doc, &candidates, &none), all);

        // A cap that covers the base parse and nothing else does too.
        let base_only = ParseBudget::with_allowance(doc.len());
        assert_eq!(isolate_structure_changes(doc, &candidates, &base_only), all);

        // End to end: one block quote with thousands of interleaved unsafe and harmless
        // spaces needs more probes than the cap pays for. The unsafe spaces never get a
        // fix, whatever the cap leaves unproven, and the result parses as before.
        let unit = "> - [x]\u{a0}done\n> - ok a\u{a0}b\n";
        let doc = unit.repeat(3000);
        let (warnings, fixed) = check_spaces(&doc);
        assert_eq!(warnings.len(), 6000);
        for (i, warning) in warnings.iter().enumerate() {
            if i % 2 == 0 {
                assert!(warning.fix.is_none(), "unsafe space {i} was offered a fix");
            }
        }
        let offered = warnings.iter().filter(|w| w.fix.is_some()).count();
        assert!(offered < 3000, "the cap should have left harmless spaces unproven");
        assert_eq!(parsed_shape(&fixed), parsed_shape(&doc));
    }

    #[test]
    fn test_space_guard_checks_each_block_on_its_own_text() {
        // Offending spaces in separate blocks are isolated without reparsing the whole
        // document for each: the cap here is far below what a whole-document probe per
        // space would need, and every offender is still found.
        let unit = "# title\u{a0}###\n\nplain a\u{a0}b\n\n";
        let doc = unit.repeat(300);
        let offsets: Vec<usize> = doc.match_indices('\u{a0}').map(|(i, _)| i).collect();
        let flagged = spaces_changing_structure(&doc, &offsets);
        assert_eq!(flagged.len(), 300);
        for (i, offset) in offsets.iter().enumerate() {
            assert_eq!(flagged.contains(offset), i % 2 == 0, "offset {offset}");
        }
    }

    #[test]
    fn test_space_fix_respects_tag_data_such_as_heading_attributes() {
        // The attribute block keeps its classes only with an ASCII space between them.
        let doc = "# t {.a\u{a0}.b}\n";
        let (found, offered, fixed) = fix_keeps_parse(doc);
        assert_eq!((found, offered), (1, 0), "{fixed:?}");
    }
}
