//! Rule MD081: Flag excessive inline emphasis.
//!
//! AI-generated Markdown tends to sprinkle inline `**bold**` across running
//! prose (`**this** and **that** and **the other**`), which hurts readability
//! in both raw and rendered form without adding meaning. This rule flags
//! paragraphs that exceed a configurable density of emphasis spans, and runs of
//! adjacent emphasis spans separated only by whitespace and punctuation.
//!
//! Scope is controlled by `targets`:
//! - `strong` (default) - only `**bold**` / `__bold__`
//! - `emphasis` - only `*italic*` / `_italic_`
//! - `all` - both, counting a combined `***bold italic***` once
//!
//! Diagnostic only: stripping or down-converting emphasis is semantically lossy
//! (`**critical**` may be deliberate), so there is no auto-fix. Both thresholds
//! are unset by default, so the rule is silent until a project opts in by setting
//! a limit. Setting a limit to `0` forbids the construct entirely (a paragraph or
//! run may contain no emphasis at all).

use crate::lint_context::LintContext;
use crate::rule::{FixCapability, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::rule_config_serde::RuleConfig;
use crate::utils::range_utils::calculate_match_range;
use crate::utils::skip_context::{compute_html_code_ranges, should_skip_emphasis_span};
use serde::{Deserialize, Serialize};

/// A counted emphasis span: byte range plus its 1-indexed line.
#[derive(Debug, Clone, Copy)]
struct CountedSpan {
    start: usize,
    end: usize,
    line: usize,
}

/// Which inline emphasis spans the rule counts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EmphasisTarget {
    /// Only strong emphasis (`**bold**`, `__bold__`).
    #[default]
    Strong,
    /// Only ordinary emphasis (`*italic*`, `_italic_`).
    Emphasis,
    /// Both strong and ordinary emphasis.
    All,
}

/// Configuration for MD081 (Excessive emphasis).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub struct MD081Config {
    /// Which emphasis spans to count. Defaults to `strong` (bold only), the
    /// pattern reported as the primary readability problem.
    #[serde(default)]
    pub targets: EmphasisTarget,

    /// Maximum emphasis spans allowed in a single paragraph. A paragraph with
    /// more than this many spans is flagged. Unset disables the check; `Some(0)`
    /// forbids all emphasis in a paragraph.
    #[serde(default)]
    pub max_per_paragraph: Option<usize>,

    /// Maximum length of a run of adjacent emphasis spans separated only by
    /// whitespace and punctuation. A longer run is flagged. Unset disables the
    /// check; `Some(0)` forbids any emphasis (every span is at least a run of one).
    #[serde(default)]
    pub max_consecutive: Option<usize>,
}

impl Default for MD081Config {
    fn default() -> Self {
        Self {
            targets: EmphasisTarget::Strong,
            max_per_paragraph: None,
            max_consecutive: None,
        }
    }
}

impl RuleConfig for MD081Config {
    const RULE_NAME: &'static str = "MD081";
}

#[derive(Debug, Clone, Default)]
pub struct MD081NoExcessiveEmphasis {
    config: MD081Config,
}

impl MD081NoExcessiveEmphasis {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_config_struct(config: MD081Config) -> Self {
        Self { config }
    }

    /// Collect the emphasis spans the rule counts: filtered by `targets`,
    /// stripped of non-prose contexts (code, links, HTML, math, ...), and -
    /// for `targets = all` - deduplicated so a nested `***bold italic***`
    /// region counts once rather than as overlapping strong + emphasis spans.
    fn counted_spans(&self, ctx: &LintContext) -> Vec<CountedSpan> {
        let html_tags = ctx.html_tags();
        let html_code_ranges = compute_html_code_ranges(&html_tags);

        let mut spans: Vec<CountedSpan> = ctx
            .emphasis_spans()
            .iter()
            .filter(|s| match self.config.targets {
                EmphasisTarget::Strong => s.is_strong,
                EmphasisTarget::Emphasis => !s.is_strong,
                EmphasisTarget::All => true,
            })
            .filter(|s| !should_skip_emphasis_span(ctx, &html_tags, &html_code_ranges, s.byte_offset))
            .map(|s| CountedSpan {
                start: s.byte_offset,
                end: s.byte_end,
                line: s.line,
            })
            .collect();

        spans.sort_by_key(|s| (s.start, std::cmp::Reverse(s.end)));

        if self.config.targets == EmphasisTarget::All {
            // Drop spans fully contained within an earlier (outer) span so a
            // combined `***x***` - reported as both a strong and an emphasis
            // span over overlapping ranges - is counted only once.
            let mut deduped: Vec<CountedSpan> = Vec::with_capacity(spans.len());
            let mut max_end = 0usize;
            for span in spans {
                if span.end <= max_end {
                    continue;
                }
                max_end = span.end;
                deduped.push(span);
            }
            deduped
        } else {
            spans
        }
    }

    /// Mark lines that are the text of a setext heading. The shared heading
    /// detector skips text lines that start with `-`/`*`/`+` (to avoid
    /// misreading list items), which leaves `**bold**\n===` looking like prose.
    /// Here a line is setext heading text if a contiguous run of prose lines
    /// ending at it is immediately followed by a `=`/`-` underline.
    fn setext_text_lines(ctx: &LintContext) -> Vec<bool> {
        let mut flags = vec![false; ctx.lines.len()];
        for (idx, line) in ctx.lines.iter().enumerate() {
            if idx == 0 || line.in_code_block {
                continue;
            }
            let text = Self::line_inner(line, ctx.content);
            let is_underline = !text.is_empty() && (text.bytes().all(|b| b == b'=') || text.bytes().all(|b| b == b'-'));
            if !is_underline {
                continue;
            }
            let level = Self::blockquote_level(line);
            // Walk back over the heading's text lines (prose, non-blank). The
            // underline only heads text at its own blockquote level, so stop at a
            // level change. A list item is never setext heading text either: an
            // unindented `=`/`-` after a list item is a thematic break / list
            // boundary.
            let mut j = idx;
            while j > 0 {
                let prev = &ctx.lines[j - 1];
                if prev.is_blank
                    || !prev.is_paragraph_context()
                    || prev.list_item.is_some()
                    || Self::blockquote_level(prev) != level
                {
                    break;
                }
                flags[j - 1] = true;
                j -= 1;
            }
        }
        flags
    }

    /// The trimmed text of a line, ignoring any blockquote markers.
    fn line_inner<'a>(line: &'a crate::lint_context::LineInfo, source: &'a str) -> &'a str {
        match line.blockquote.as_ref() {
            Some(bq) => bq.content.trim(),
            None => line.content(source).trim(),
        }
    }

    /// The blockquote nesting level a line sits at (0 = top level).
    fn blockquote_level(line: &crate::lint_context::LineInfo) -> usize {
        line.blockquote.as_ref().map_or(0, |b| b.nesting_level)
    }

    /// Assign each line (0-indexed into `ctx.lines`) a paragraph id, or `None`
    /// when the line is not paragraph prose. A new paragraph begins when prose
    /// resumes after a boundary (blank line, heading, code block, ...), when a
    /// list item starts, or when the blockquote nesting level changes - so list
    /// items and nested quotes are counted independently.
    fn paragraph_ids(ctx: &LintContext) -> Vec<Option<usize>> {
        let mut ids = vec![None; ctx.lines.len()];
        let setext_text = Self::setext_text_lines(ctx);
        let mut current: Option<usize> = None;
        let mut next_id = 0usize;
        let mut prev_bq_level = 0usize;

        for (idx, line) in ctx.lines.iter().enumerate() {
            let bq_level = Self::blockquote_level(line);
            let is_prose =
                !line.is_blank && line.is_paragraph_context() && !setext_text[idx] && !ctx.is_in_table_block(idx + 1);

            if !is_prose {
                current = None;
                prev_bq_level = bq_level;
                continue;
            }

            let starts_new = current.is_none() || line.list_item.is_some() || bq_level != prev_bq_level;
            if starts_new {
                current = Some(next_id);
                next_id += 1;
            }
            ids[idx] = current;
            prev_bq_level = bq_level;
        }

        ids
    }

    /// Flag a run of adjacent emphasis spans if it exceeds `limit`, pointing at
    /// the run's first span.
    fn emit_run(&self, ctx: &LintContext, run: &[CountedSpan], limit: usize, warnings: &mut Vec<LintWarning>) {
        if run.len() > limit
            && let Some(first) = run.first()
        {
            warnings.push(self.warn_at(
                ctx,
                first,
                format!(
                    "{} consecutive emphasis spans (limit {limit}); consider rephrasing to reduce emphasis",
                    run.len(),
                ),
            ));
        }
    }

    fn warn_at(&self, ctx: &LintContext, span: &CountedSpan, message: String) -> LintWarning {
        let line_content = ctx.lines.get(span.line - 1).map_or("", |l| l.content(ctx.content));
        let line_start = ctx.lines.get(span.line - 1).map_or(0, |l| l.byte_offset);
        let match_start_in_line = span.start.saturating_sub(line_start);
        let (start_line, start_col, end_line, end_col) =
            calculate_match_range(span.line, line_content, match_start_in_line, span.end - span.start);
        LintWarning {
            rule_name: Some(self.name().to_string()),
            severity: Severity::Warning,
            line: start_line,
            column: start_col,
            end_line,
            end_column: end_col,
            message,
            fix: None,
        }
    }
}

impl Rule for MD081NoExcessiveEmphasis {
    fn name(&self) -> &'static str {
        "MD081"
    }

    fn description(&self) -> &'static str {
        "Inline emphasis should not be excessive"
    }

    fn category(&self) -> RuleCategory {
        RuleCategory::Emphasis
    }

    fn check(&self, ctx: &LintContext) -> LintResult {
        if self.config.max_per_paragraph.is_none() && self.config.max_consecutive.is_none() {
            return Ok(Vec::new());
        }

        let spans = self.counted_spans(ctx);
        if spans.is_empty() {
            return Ok(Vec::new());
        }

        let para_ids = Self::paragraph_ids(ctx);
        let mut warnings = Vec::new();

        if let Some(limit) = self.config.max_per_paragraph {
            // Count spans per paragraph; flag the first span of any paragraph
            // whose count exceeds the limit. Spans are ordered by position, so
            // the first per paragraph is the earliest occurrence.
            let mut counts: std::collections::HashMap<usize, (usize, CountedSpan)> = std::collections::HashMap::new();
            for span in &spans {
                let Some(pid) = para_ids.get(span.line - 1).copied().flatten() else {
                    continue;
                };
                counts.entry(pid).and_modify(|(n, _)| *n += 1).or_insert((1, *span));
            }
            let mut flagged: Vec<(usize, CountedSpan)> = counts
                .into_iter()
                .filter(|(_, (n, _))| *n > limit)
                .map(|(_, (n, first))| (n, first))
                .collect();
            flagged.sort_by_key(|(_, first)| (first.line, first.start));
            for (count, first) in flagged {
                warnings.push(self.warn_at(
                    ctx,
                    &first,
                    format!(
                        "Paragraph contains {count} emphasis spans (limit {limit}); consider reducing emphasis to improve readability"
                    ),
                ));
            }
        }

        if let Some(limit) = self.config.max_consecutive {
            // A run is a maximal sequence of spans in the same paragraph where
            // the text between neighbours is only whitespace and punctuation.
            // Anything else (including connector words like "and") breaks it.
            let mut run_start = 0usize; // index into `spans` of the run's first span
            for i in 0..spans.len() {
                let breaks = if i == 0 {
                    true
                } else {
                    let prev = &spans[i - 1];
                    let cur = &spans[i];
                    let same_para = para_ids.get(prev.line - 1).copied().flatten()
                        == para_ids.get(cur.line - 1).copied().flatten()
                        && para_ids.get(cur.line - 1).copied().flatten().is_some();
                    let between = ctx.content.get(prev.end..cur.start).unwrap_or("");
                    // Only whitespace and punctuation (any script - em dashes, CJK
                    // punctuation, etc.) keeps a run together. Any word character
                    // (a connector like "and") breaks it.
                    let only_filler = !between.chars().any(char::is_alphanumeric);
                    !(same_para && only_filler)
                };

                if breaks && i > run_start {
                    self.emit_run(ctx, &spans[run_start..i], limit, &mut warnings);
                }
                if breaks {
                    run_start = i;
                }
            }
            if !spans.is_empty() {
                self.emit_run(ctx, &spans[run_start..], limit, &mut warnings);
            }
        }

        Ok(warnings)
    }

    fn fix_capability(&self) -> FixCapability {
        FixCapability::Unfixable
    }

    fn fix(&self, ctx: &LintContext) -> Result<String, LintError> {
        // Diagnostic only: emphasis is never rewritten, so fixing is a no-op
        // that returns the content unchanged.
        Ok(ctx.content.to_string())
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn default_config_section(&self) -> Option<(String, toml::Value)> {
        let table = crate::rule_config_serde::config_schema_table(&MD081Config::default())?;
        if table.is_empty() {
            None
        } else {
            Some((MD081Config::RULE_NAME.to_string(), toml::Value::Table(table)))
        }
    }

    fn from_config(config: &crate::config::Config) -> Box<dyn Rule>
    where
        Self: Sized,
    {
        let rule_config = crate::rule_config_serde::load_rule_config::<MD081Config>(config);
        Box::new(Self::from_config_struct(rule_config))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::MarkdownFlavor;
    use crate::rule::LintWarning;

    fn check(content: &str, config: MD081Config) -> Vec<LintWarning> {
        let rule = MD081NoExcessiveEmphasis::from_config_struct(config);
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        rule.check(&ctx).unwrap()
    }

    #[test]
    fn flags_paragraph_over_max_per_paragraph() {
        let config = MD081Config {
            max_per_paragraph: Some(3),
            ..Default::default()
        };
        let content = "The **a** is **b** and **c** plus **d**.";
        let warnings = check(content, config);
        assert_eq!(warnings.len(), 1, "4 bold spans should exceed max-per-paragraph=3");
        assert_eq!(warnings[0].line, 1);
    }

    #[test]
    fn flags_consecutive_run_separated_only_by_punctuation() {
        let config = MD081Config {
            max_consecutive: Some(2),
            ..Default::default()
        };
        // Three bolds separated only by ", " - a run of 3 exceeds max-consecutive=2.
        let content = "Tags: **one**, **two**, **three**.";
        let warnings = check(content, config);
        assert_eq!(
            warnings.len(),
            1,
            "run of 3 adjacent bolds should exceed max-consecutive=2"
        );
        assert_eq!(warnings[0].line, 1);
    }

    #[test]
    fn unicode_punctuation_does_not_break_consecutive_run() {
        // Em dashes are punctuation, not words, so a run separated by them must
        // still be treated as consecutive.
        let config = MD081Config {
            max_consecutive: Some(2),
            ..Default::default()
        };
        let content = "Tags: **one** \u{2014} **two** \u{2014} **three**.";
        let warnings = check(content, config);
        assert_eq!(
            warnings.len(),
            1,
            "em-dash-separated bolds form one run of 3, exceeding max-consecutive=2. Got: {warnings:?}"
        );
    }

    #[test]
    fn connector_word_breaks_consecutive_run() {
        let config = MD081Config {
            max_consecutive: Some(2),
            ..Default::default()
        };
        // "and" between the second and third bold breaks the run into 2 + 1.
        let content = "Tags: **one**, **two**, and **three**.";
        let warnings = check(content, config);
        assert!(
            warnings.is_empty(),
            "a connector word should break the run below the limit. Got: {warnings:?}"
        );
    }

    #[test]
    fn disabled_by_default() {
        // Default config has both thresholds at 0, so the rule is silent even
        // on heavily bolded prose.
        let content = "**a** **b** **c** **d** **e** **f** **g** **h**.";
        let warnings = check(content, MD081Config::default());
        assert!(warnings.is_empty(), "rule must be off by default. Got: {warnings:?}");
    }

    #[test]
    fn does_not_flag_setext_heading_text() {
        // A setext heading's text line is a heading, not prose, so emphasis in
        // it must not be counted - same as ATX headings.
        let config = MD081Config {
            max_per_paragraph: Some(2),
            max_consecutive: Some(1),
            ..Default::default()
        };
        let content = "**A** **B** **C**\n=================\n";
        let warnings = check(content, config);
        assert!(
            warnings.is_empty(),
            "emphasis in setext heading text must not be flagged. Got: {warnings:?}"
        );
    }

    #[test]
    fn flags_list_item_before_thematic_break() {
        // `- ...\n---` is a list item followed by a thematic break, not a setext
        // heading (setext underlines inside list items must be indented). The
        // emphasis in the list item must still be counted.
        let config = MD081Config {
            max_per_paragraph: Some(1),
            ..Default::default()
        };
        let content = "- **a** and **b**\n---\n";
        let warnings = check(content, config);
        assert_eq!(
            warnings.len(),
            1,
            "list item with 2 bolds before a thematic break should be flagged. Got: {warnings:?}"
        );
    }

    #[test]
    fn parses_kebab_case_keys_and_lowercase_targets_from_config() {
        // Exercise the production config path: kebab-case keys and the
        // lowercase `targets` enum must round-trip through TOML, or real user
        // configs would silently fall back to defaults (rule disabled).
        let mut config = crate::config::Config::default();
        let mut rule_config = crate::config::RuleConfig::default();
        rule_config
            .values
            .insert("max-per-paragraph".to_string(), toml::Value::Integer(1));
        rule_config
            .values
            .insert("targets".to_string(), toml::Value::String("all".to_string()));
        config.rules.insert("MD081".to_string(), rule_config);

        let rule = MD081NoExcessiveEmphasis::from_config(&config);
        // One bold + one italic = two spans under `targets = all`, exceeding
        // max-per-paragraph = 1. This only fires if both keys parsed: the
        // kebab key (else the limit stays 0 and the rule is off) and the
        // lowercase enum (else it defaults to `strong` and counts one span).
        let ctx = LintContext::new("This is **bold** and *italic*.", MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(
            warnings.len(),
            1,
            "kebab-case max-per-paragraph and targets=\"all\" must parse from config. Got: {warnings:?}"
        );
    }

    #[test]
    fn does_not_flag_setext_heading_inside_blockquote() {
        // `> **A** **B**\n> ===` is a setext heading inside a blockquote; its
        // text line must not be counted as prose.
        let config = MD081Config {
            max_per_paragraph: Some(1),
            ..Default::default()
        };
        let content = "> **A** **B**\n> ===\n";
        let warnings = check(content, config);
        assert!(
            warnings.is_empty(),
            "emphasis in a blockquoted setext heading must not be flagged. Got: {warnings:?}"
        );
    }

    #[test]
    fn flags_blockquote_paragraph_before_top_level_break() {
        // A top-level `---` after a blockquote is outside the quote, so the
        // quoted paragraph is not a setext heading and its emphasis still counts.
        let config = MD081Config {
            max_per_paragraph: Some(1),
            ..Default::default()
        };
        let content = "> **a** and **b**\n---\n";
        let warnings = check(content, config);
        assert_eq!(
            warnings.len(),
            1,
            "blockquote paragraph with 2 bolds before a top-level break should be flagged. Got: {warnings:?}"
        );
    }

    #[test]
    fn does_not_flag_emphasis_in_table_rows() {
        // Table cells are not prose; emphasis inside a table must not be counted.
        let config = MD081Config {
            max_per_paragraph: Some(1),
            ..Default::default()
        };
        let content = "| Col A | Col B |\n| ----- | ----- |\n| **a** | **b** |\n";
        let warnings = check(content, config);
        assert!(
            warnings.is_empty(),
            "emphasis in table cells must not be flagged. Got: {warnings:?}"
        );
    }

    #[test]
    fn does_not_flag_at_or_below_limit() {
        let config = MD081Config {
            max_per_paragraph: Some(3),
            ..Default::default()
        };
        let content = "The **a** is **b** and **c**.";
        assert!(check(content, config).is_empty(), "3 spans must not exceed limit 3");
    }

    #[test]
    fn excludes_code_blocks_and_inline_code() {
        let config = MD081Config {
            max_per_paragraph: Some(1),
            ..Default::default()
        };
        // Bold markers inside fences and inline code must not count.
        let content = "```python\nfoo(**a**, **b**, **c**, **d**)\n```\n\nText with `**x** **y** **z**` only.";
        let warnings = check(content, config);
        assert!(
            warnings.is_empty(),
            "emphasis inside code must be ignored. Got: {warnings:?}"
        );
    }

    #[test]
    fn counts_paragraphs_independently() {
        let config = MD081Config {
            max_per_paragraph: Some(2),
            ..Default::default()
        };
        // Two paragraphs of 2 bolds each: neither exceeds the limit of 2.
        let content = "First **a** and **b** here.\n\nSecond **c** and **d** here.";
        assert!(
            check(content, config).is_empty(),
            "spans must not aggregate across the blank-line paragraph boundary"
        );
    }

    #[test]
    fn counts_list_items_independently() {
        let config = MD081Config {
            max_per_paragraph: Some(2),
            ..Default::default()
        };
        // Each list item has 2 bolds; neither item alone exceeds the limit.
        let content = "- item **a** and **b**\n- item **c** and **d**";
        assert!(
            check(content, config).is_empty(),
            "each list item is its own paragraph and must be counted independently"
        );
    }

    #[test]
    fn targets_strong_ignores_italic() {
        let config = MD081Config {
            targets: EmphasisTarget::Strong,
            max_per_paragraph: Some(1),
            ..Default::default()
        };
        // Many italics but only one bold: strong-only must not flag.
        let content = "Here is *a* and *b* and *c* and *d* with one **bold**.";
        assert!(
            check(content, config).is_empty(),
            "targets=strong must ignore italic spans"
        );
    }

    #[test]
    fn targets_emphasis_counts_italic_only() {
        let config = MD081Config {
            targets: EmphasisTarget::Emphasis,
            max_per_paragraph: Some(2),
            ..Default::default()
        };
        let content = "Lots of *a* and *b* and *c* italics, plus **bold**.";
        let warnings = check(content, config);
        assert_eq!(warnings.len(), 1, "3 italics exceed limit 2 under targets=emphasis");
    }

    #[test]
    fn targets_all_dedups_combined_bold_italic() {
        let config = MD081Config {
            targets: EmphasisTarget::All,
            max_per_paragraph: Some(1),
            ..Default::default()
        };
        // A single ***bold italic*** region is reported by the parser as both a
        // strong and an emphasis span. It must count as one, not exceed limit 1.
        let content = "Just ***one region*** here.";
        assert!(
            check(content, config).is_empty(),
            "combined ***...*** must count once under targets=all"
        );
    }

    #[test]
    fn targets_all_counts_distinct_regions() {
        let config = MD081Config {
            targets: EmphasisTarget::All,
            max_per_paragraph: Some(1),
            ..Default::default()
        };
        let content = "Mix ***a*** and **b** here.";
        let warnings = check(content, config);
        assert_eq!(warnings.len(), 1, "two distinct emphasis regions exceed limit 1");
    }

    #[test]
    fn max_per_paragraph_zero_forbids_all_emphasis() {
        // `Some(0)` is distinct from unset: it forbids any emphasis, so a single
        // bold span (count 1 > 0) must be flagged.
        let config = MD081Config {
            max_per_paragraph: Some(0),
            ..Default::default()
        };
        let content = "A paragraph with one **bold** word.";
        let warnings = check(content, config);
        assert_eq!(
            warnings.len(),
            1,
            "max-per-paragraph=0 must flag even a single emphasis span. Got: {warnings:?}"
        );
    }

    #[test]
    fn max_consecutive_zero_forbids_all_emphasis() {
        // A lone span is a run of length 1; with limit 0 it exceeds the limit
        // and must be flagged.
        let config = MD081Config {
            max_consecutive: Some(0),
            ..Default::default()
        };
        let content = "A paragraph with one **bold** word.";
        let warnings = check(content, config);
        assert_eq!(
            warnings.len(),
            1,
            "max-consecutive=0 must flag even a single emphasis span. Got: {warnings:?}"
        );
    }

    #[test]
    fn explicit_zero_in_toml_parses_as_forbid_all() {
        // A user-set `max-per-paragraph = 0` must deserialize to Some(0)
        // (forbid all), not be confused with the unset/disabled state.
        let mut config = crate::config::Config::default();
        let mut rule_config = crate::config::RuleConfig::default();
        rule_config
            .values
            .insert("max-per-paragraph".to_string(), toml::Value::Integer(0));
        config.rules.insert("MD081".to_string(), rule_config);

        let rule = MD081NoExcessiveEmphasis::from_config(&config);
        let ctx = LintContext::new("One **bold** here.", MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(
            warnings.len(),
            1,
            "explicit max-per-paragraph = 0 must forbid all emphasis. Got: {warnings:?}"
        );
    }
}
