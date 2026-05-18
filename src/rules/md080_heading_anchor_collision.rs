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

    /// The anchor a heading actually resolves to: an explicit `{#custom-id}`
    /// wins over the generated slug (it is what platforms emit).
    fn effective_anchor(&self, heading: &crate::lint_context::HeadingInfo) -> String {
        match &heading.custom_id {
            Some(id) => id.to_lowercase(),
            None => self.config.anchor_style.generate_fragment(&heading.text),
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

    fn should_skip(&self, ctx: &LintContext) -> bool {
        !ctx.has_char('#')
    }

    fn check(&self, ctx: &LintContext) -> LintResult {
        let mut warnings = Vec::new();
        // anchor -> 1-based line of the first heading that produced it.
        let mut seen: HashMap<String, usize> = HashMap::new();

        for (idx, line_info) in ctx.lines.iter().enumerate() {
            if line_info.in_front_matter || line_info.in_code_block {
                continue;
            }
            let Some(heading) = &line_info.heading else {
                continue;
            };
            if !heading.is_valid || heading.text.is_empty() {
                continue;
            }
            if !self.config.levels.contains(&heading.level) {
                continue;
            }

            let anchor = self.effective_anchor(heading);
            // An empty anchor (e.g. a CJK-only heading under an ASCII style)
            // is not a usable fragment target; "colliding" empties are a
            // different defect and would only add noise here.
            if anchor.is_empty() {
                continue;
            }

            let line_num = idx + 1;
            if let Some(&first_line) = seen.get(&anchor) {
                let content = line_info.content(ctx.content);
                let text_start = content.find(heading.text.as_str()).unwrap_or(0);
                let (start_line, start_col, end_line, end_col) =
                    calculate_match_range(line_num, content, text_start, heading.text.len());
                warnings.push(LintWarning {
                    rule_name: Some(self.name().to_string()),
                    message: format!(
                        "Heading anchor '{anchor}' collides with the heading at line {first_line}; \
                         fragment links and any derived page identifier resolve only to the first occurrence"
                    ),
                    line: start_line,
                    column: start_col,
                    end_line,
                    end_column: end_col,
                    severity: Severity::Warning,
                    fix: None,
                });
            } else {
                seen.insert(anchor, line_num);
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
