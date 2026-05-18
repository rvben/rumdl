//! Rule MD080: Heading anchors must be unique.
//!
//! Two headings whose generated URL-safe anchor (slug) is identical produce a
//! collision: a `[text](#slug)` link and, under the MDXG virtual-page model,
//! the page identifier derived from an H1/H2 title can only resolve to the
//! *first* occurrence. GitHub/MkDocs paper over this by auto-suffixing the
//! later anchor (`slug-1`), which is functional but surprising and breaks any
//! hand-written `#slug` link that meant the second heading.
//!
//! This is distinct from:
//! - **MD024** (duplicate heading *text*) - misses distinct texts that
//!   slugify identically (`Setup & Run` vs `Setup Run`, `C++` vs `C`).
//! - **MD051** (broken/missing fragment *targets*) - this flags *ambiguous*
//!   targets, where the reference resolves but not unambiguously.
//!
//! Diagnostic only: renaming a heading is a semantic choice, so there is no
//! auto-fix. Opt-in, because the collision is functional under platform
//! auto-suffixing and flagging it changes established lint output.

use crate::lint_context::LintContext;
use crate::rule::{LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::rule_config_serde::RuleConfig;
use crate::utils::anchor_styles::AnchorStyle;
use crate::utils::range_utils::calculate_match_range;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

fn default_levels() -> Vec<u8> {
    vec![1, 2, 3, 4, 5, 6]
}

/// Configuration for MD080 (Heading anchor collision)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub struct MD080Config {
    /// Anchor generation style to match the target platform.
    #[serde(default, alias = "anchor_style")]
    pub anchor_style: AnchorStyle,

    /// Heading levels whose anchors must be unique. Defaults to all levels
    /// (any heading can be a fragment target). Set to `[1, 2]` to check only
    /// the MDXG virtual-page identifiers derived from H1/H2 titles.
    #[serde(default = "default_levels")]
    pub levels: Vec<u8>,
}

impl Default for MD080Config {
    fn default() -> Self {
        Self {
            anchor_style: AnchorStyle::default(),
            levels: default_levels(),
        }
    }
}

impl RuleConfig for MD080Config {
    const RULE_NAME: &'static str = "MD080";
}

#[derive(Debug, Clone, Default)]
pub struct MD080HeadingAnchorCollision {
    config: MD080Config,
}

impl MD080HeadingAnchorCollision {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_config_struct(config: MD080Config) -> Self {
        Self { config }
    }

    /// The anchor a heading actually resolves to. An explicit `{#custom-id}`
    /// wins over the generated slug (it is what platforms emit) and is
    /// compared in its emitted case: HTML `id` matching is case-sensitive, so
    /// `{#API}` and `{#api}` are distinct anchors. Generated slugs are already
    /// case-normalized by the anchor style.
    fn effective_anchor(&self, text: &str, custom_id: Option<&str>) -> String {
        match custom_id {
            Some(id) => id.to_string(),
            None => self.config.anchor_style.generate_fragment(text),
        }
    }

    /// Resolve a heading's anchor and either record it as the first occurrence
    /// or, if some earlier heading already produced the same anchor, emit a
    /// collision warning pointing back at that first heading.
    #[allow(clippy::too_many_arguments)]
    fn record(
        &self,
        text: &str,
        custom_id: Option<&str>,
        level: u8,
        line_num: usize,
        content: &str,
        seen: &mut HashMap<String, usize>,
        warnings: &mut Vec<LintWarning>,
    ) {
        if !self.config.levels.contains(&level) {
            return;
        }

        let anchor = self.effective_anchor(text, custom_id);
        if anchor.is_empty() {
            return;
        }

        if let Some(&first_line) = seen.get(&anchor) {
            let (start_line, start_col, end_line, end_col) =
                calculate_match_range(line_num, content, content.find(text).unwrap_or(0), text.len());
            warnings.push(LintWarning {
                rule_name: Some(self.name().to_string()),
                severity: Severity::Warning,
                line: start_line,
                column: start_col,
                end_line,
                end_column: end_col,
                message: format!(
                    "Heading anchor '{anchor}' collides with the heading at line {first_line}; \
                     fragment links and any derived page identifier resolve only to the first occurrence"
                ),
                fix: None,
            });
        } else {
            seen.insert(anchor, line_num);
        }
    }
}

impl Rule for MD080HeadingAnchorCollision {
    fn name(&self) -> &'static str {
        "MD080"
    }

    fn description(&self) -> &'static str {
        "Heading anchors must be unique"
    }

    fn check(&self, ctx: &LintContext) -> LintResult {
        let mut warnings = Vec::new();
        // anchor -> 1-based line of the first heading that produced it.
        let mut seen: HashMap<String, usize> = HashMap::new();

        for (idx, line_info) in ctx.lines.iter().enumerate() {
            if line_info.in_front_matter || line_info.in_code_block {
                continue;
            }
            let line_num = idx + 1;
            let content = line_info.content(ctx.content);

            // Regular ATX/Setext headings parsed by the line scanner.
            if let Some(heading) = &line_info.heading {
                if heading.is_valid && !heading.text.is_empty() {
                    self.record(
                        &heading.text,
                        heading.custom_id.as_deref(),
                        heading.level,
                        line_num,
                        content,
                        &mut seen,
                        &mut warnings,
                    );
                }
                continue;
            }

            // Blockquote headings (`> ## Intro`) are not seen by the line
            // scanner but still emit fragment anchors - mirror MD051 so the
            // two rules agree on what targets exist.
            if let Some(bq) = &line_info.blockquote
                && let Some((clean_text, custom_id)) =
                    crate::utils::header_id_utils::parse_blockquote_atx_heading(&bq.content)
                && !clean_text.is_empty()
            {
                let level = bq
                    .content
                    .trim_start()
                    .bytes()
                    .take_while(|&b| b == b'#')
                    .count()
                    .clamp(1, 6) as u8;
                self.record(
                    &clean_text,
                    custom_id.as_deref(),
                    level,
                    line_num,
                    content,
                    &mut seen,
                    &mut warnings,
                );
            }
        }

        Ok(warnings)
    }

    fn fix(&self, _ctx: &LintContext) -> Result<String, LintError> {
        // Renaming a heading (and every link that targets it) is a semantic
        // decision the linter must not make automatically.
        Err(LintError::FixFailed("MD080 has no auto-fix".to_string()))
    }

    fn category(&self) -> RuleCategory {
        RuleCategory::Heading
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn default_config_section(&self) -> Option<(String, toml::Value)> {
        let table = crate::rule_config_serde::config_schema_table(&MD080Config::default())?;
        if table.is_empty() {
            None
        } else {
            Some((MD080Config::RULE_NAME.to_string(), toml::Value::Table(table)))
        }
    }

    fn from_config(config: &crate::config::Config) -> Box<dyn Rule>
    where
        Self: Sized,
    {
        let mut rule_config = crate::rule_config_serde::load_rule_config::<MD080Config>(config);

        // Mirror MD051: when the user has not pinned an anchor style, follow
        // the active flavor's native anchor generation.
        let explicit_style_present = config
            .rules
            .get("MD080")
            .is_some_and(|rc| rc.values.contains_key("anchor-style") || rc.values.contains_key("anchor_style"));
        if !explicit_style_present {
            rule_config.anchor_style = match config.global.flavor {
                crate::config::MarkdownFlavor::MkDocs => AnchorStyle::PythonMarkdown,
                crate::config::MarkdownFlavor::Kramdown => AnchorStyle::KramdownGfm,
                _ => AnchorStyle::GitHub,
            };
        }

        Box::new(MD080HeadingAnchorCollision::from_config_struct(rule_config))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::MarkdownFlavor;

    fn check(content: &str) -> Vec<LintWarning> {
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        MD080HeadingAnchorCollision::new().check(&ctx).unwrap()
    }

    fn check_with(config: MD080Config, content: &str) -> Vec<LintWarning> {
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        MD080HeadingAnchorCollision::from_config_struct(config)
            .check(&ctx)
            .unwrap()
    }

    #[test]
    fn flags_distinct_text_same_github_slug() {
        // "Setup & Run" and "Setup Run" both slugify to `setup--run` /
        // `setup-run` family; under GitHub they collide on `setup--run`.
        let w = check("# Setup & Run\n\n# Setup  Run\n");
        assert_eq!(w.len(), 1, "got: {w:?}");
        assert!(w[0].message.contains("collides with the heading at line 1"));
        assert_eq!(w[0].line, 3);
    }

    #[test]
    fn flags_punctuation_only_difference() {
        // "C++" -> "c", "C" -> "c" under GitHub.
        let w = check("# C++\n\n## C\n");
        assert_eq!(w.len(), 1, "got: {w:?}");
    }

    #[test]
    fn flags_same_text_across_levels() {
        // Same text at different levels: MD024 with allow_different_nesting
        // would NOT flag this, but the anchor `#intro` is genuinely ambiguous.
        let w = check("# Intro\n\nbody\n\n## Intro\n");
        assert_eq!(w.len(), 1, "distinct-level slug collision must flag: {w:?}");
        assert_eq!(w[0].line, 5);
    }

    #[test]
    fn no_warning_when_slugs_differ() {
        assert!(check("# Alpha\n\n## Beta\n\n### Gamma\n").is_empty());
    }

    #[test]
    fn flags_three_way_collision_once_per_extra() {
        let w = check("# Dup\n\n## Dup\n\n### Dup\n");
        assert_eq!(w.len(), 2, "first defines, each later collides: {w:?}");
        assert_eq!(w[0].line, 3);
        assert_eq!(w[1].line, 5);
    }

    #[test]
    fn flags_colliding_custom_ids() {
        let w = check("# Alpha {#dup}\n\n## Beta {#dup}\n");
        assert_eq!(w.len(), 1, "got: {w:?}");
        assert!(w[0].message.contains("'dup'"));
    }

    #[test]
    fn custom_id_disambiguates_same_text() {
        // Same visible text but explicit distinct ids => no collision.
        let w = check("# Repeat {#first}\n\n## Repeat {#second}\n");
        assert!(w.is_empty(), "explicit ids disambiguate: {w:?}");
    }

    #[test]
    fn ignores_headings_in_code_fences() {
        let w = check("# Title\n\n```\n# Title\n```\n");
        assert!(w.is_empty(), "fenced `# Title` is not a heading: {w:?}");
    }

    #[test]
    fn ignores_front_matter() {
        let w = check("---\ntitle: Title\n---\n\n# Title\n\n## Title\n");
        // Two real headings still collide; front matter must not add a third.
        assert_eq!(w.len(), 1, "got: {w:?}");
        assert_eq!(w[0].line, 7);
    }

    #[test]
    fn levels_filter_restricts_scope() {
        // H3 collision is ignored when only H1/H2 page ids are checked.
        let cfg = MD080Config {
            anchor_style: AnchorStyle::GitHub,
            levels: vec![1, 2],
        };
        let w = check_with(cfg, "# Page\n\n### Dup\n\n### Dup\n");
        assert!(w.is_empty(), "H3 collisions excluded by levels=[1,2]: {w:?}");
    }

    #[test]
    fn anchor_style_changes_collision_outcome() {
        // "a_b" vs "ab": GitHub preserves `_` (slugs `a_b` / `ab`, distinct),
        // Kramdown strips `_` (both become `ab`, a collision).
        let content = "# a_b\n\n## ab\n";
        assert!(
            check_with(
                MD080Config {
                    anchor_style: AnchorStyle::GitHub,
                    levels: default_levels()
                },
                content
            )
            .is_empty(),
            "GitHub keeps the underscore, slugs stay distinct"
        );
        assert_eq!(
            check_with(
                MD080Config {
                    anchor_style: AnchorStyle::Kramdown,
                    levels: default_levels()
                },
                content
            )
            .len(),
            1,
            "Kramdown removes `_`, so both headings slug to `ab`"
        );
    }

    #[test]
    fn flags_setext_heading_collision() {
        // Setext headings produce fragment anchors too; a Setext H1 and an
        // ATX H2 with the same slug collide just like two ATX headings.
        let w = check("Intro\n=====\n\nbody\n\n## Intro\n");
        assert_eq!(w.len(), 1, "setext + atx slug collision must flag: {w:?}");
        assert_eq!(w[0].line, 6);
    }

    #[test]
    fn custom_id_case_is_significant() {
        // HTML id matching is case-sensitive: {#API} and {#api} are distinct
        // anchors, so they must NOT be reported as a collision.
        let w = check("# Alpha {#API}\n\n## Beta {#api}\n");
        assert!(w.is_empty(), "custom ids differing only in case are distinct: {w:?}");
    }

    #[test]
    fn flags_blockquote_heading_collision() {
        // A blockquoted ATX heading still emits a fragment anchor (mirrors
        // MD051), so it collides with a same-slug top-level heading.
        let w = check("> ## Intro\n\n## Intro\n");
        assert_eq!(w.len(), 1, "blockquote heading slug collision must flag: {w:?}");
        assert_eq!(w[0].line, 3);
    }

    #[test]
    fn no_auto_fix_offered() {
        let w = check("# Dup\n\n## Dup\n");
        assert!(w[0].fix.is_none());
        let ctx = LintContext::new("# Dup\n\n## Dup\n", MarkdownFlavor::Standard, None);
        assert!(MD080HeadingAnchorCollision::new().fix(&ctx).is_err());
    }

    #[test]
    fn empty_document_is_clean() {
        assert!(check("").is_empty());
        assert!(check("Just prose, no headings.\n").is_empty());
    }
}
