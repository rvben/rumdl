//! MD092 detects unresolved conflicts and protects the entire document from edits.
//!
//! Scan raw lines, including fenced code: Git can insert conflicts anywhere.
//! Opening or closing markers suffice because conflicts may be partially resolved.
//! Separators alone are valid Setext headings, and diff3 base markers alone can
//! be table content, so neither is evidence of a conflict on its own.

use crate::rule::{FixCapability, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};

pub const RULE_NAME: &str = "MD092";

#[derive(Debug, Clone, Default)]
pub struct MD092MergeConflict;

impl Rule for MD092MergeConflict {
    fn name(&self) -> &'static str {
        RULE_NAME
    }
    fn description(&self) -> &'static str {
        "Unresolved merge conflict markers"
    }
    fn category(&self) -> RuleCategory {
        RuleCategory::Other
    }
    fn check(&self, ctx: &crate::lint_context::LintContext) -> LintResult {
        // The lint engine records suppressed findings for MD087 before filtering.
        Ok(markers(ctx.content).collect())
    }
    fn fix_capability(&self) -> FixCapability {
        FixCapability::Unfixable
    }
    fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, LintError> {
        Ok(ctx.content.to_string())
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn from_config(_config: &crate::config::Config) -> Box<dyn Rule> {
        Box::new(Self)
    }
}

/// Find the first conflict that the document configuration has not suppressed.
/// Scan every marker: a documented example must not conceal a later conflict.
/// Keep this guard before normalization, fixes, and external tool execution.
///
/// Configuration alone decides whether the rule runs, so this is the entry point
/// for callers with no rule list of their own, and for callers whose list is a
/// role-scoped subset rather than an invocation's selection (the LSP indexes
/// with MD051 and MD057 alone).
pub fn detect_configured(
    content: &str,
    config: &crate::config::Config,
    path: Option<&std::path::Path>,
) -> Option<LintWarning> {
    // Most documents contain no markers; avoid parsing directives in that case.
    detect(content)?;
    let rules: Vec<Box<dyn Rule>> = vec![Box::new(MD092MergeConflict)];
    if crate::rules::filter_rules(&rules, &config.global).is_empty() {
        return None;
    }
    detect_suppressed(content, config, path)
}

/// The finding for a conflicted document, if the invocation reports MD092 for it.
///
/// `rules` is the invocation's effective rule set, already resolved from
/// configuration and CLI rule selection, so it is the whole answer to whether the
/// rule runs: `--enable MD092` re-enables a rule the configuration disabled, the
/// way it does for every other rule, and this guard follows it. A caller holding a
/// role-scoped subset instead wants `detect_configured`, or its guard silently
/// disappears along with the rules it never listed.
pub fn detect_for_rules(
    content: &str,
    rules: &[Box<dyn Rule>],
    config: &crate::config::Config,
    path: Option<&std::path::Path>,
) -> Option<LintWarning> {
    // Most documents contain no markers; avoid parsing directives in that case.
    detect(content)?;
    if !rules.iter().any(|rule| rule.name() == RULE_NAME) {
        return None;
    }
    detect_suppressed(content, config, path)
}

/// The first marker this document's own suppressions leave standing, at the
/// severity configuration gives the rule. The caller has already decided that
/// the rule runs at all.
fn detect_suppressed(
    content: &str,
    config: &crate::config::Config,
    path: Option<&std::path::Path>,
) -> Option<LintWarning> {
    if path.is_some_and(|path| config.get_ignored_rules_for_file(path).contains(RULE_NAME)) {
        return None;
    }
    let inline = crate::inline_config::InlineConfig::from_content(content);
    let mut warning = markers(content).find(|warning| !inline.is_rule_disabled(RULE_NAME, warning.line))?;
    if let Some(severity) = config.get_rule_severity(RULE_NAME) {
        warning.severity = severity;
    }
    Some(warning)
}

/// Find the first Git conflict marker, including custom marker widths >= 7.
/// Labels must be separated by whitespace, as in Git's marker syntax.
/// Raw detection, without configuration or inline suppression.
pub fn detect(content: &str) -> Option<LintWarning> {
    markers(content).next()
}

fn markers(content: &str) -> impl Iterator<Item = LintWarning> + '_ {
    content.lines().enumerate().filter_map(|(index, line)| {
        let column = if index == 0 && line.starts_with('\u{feff}') {
            2
        } else {
            1
        };
        let line = if index == 0 {
            line.trim_start_matches('\u{feff}')
        } else {
            line
        };
        let marker = *line.as_bytes().first()?;
        if !matches!(marker, b'<' | b'>') {
            return None;
        }
        let width = line.bytes().take_while(|&byte| byte == marker).count();
        if width < 7 || !matches!(line.as_bytes().get(width), None | Some(b' ' | b'\t')) {
            return None;
        }
        Some(LintWarning {
            rule_name: Some(RULE_NAME.to_string()),
            message: "Unresolved merge conflict; formatting skipped until conflict markers are removed".to_string(),
            line: index + 1,
            column,
            end_line: index + 1,
            end_column: width + column,
            severity: Severity::Error,
            fix: None,
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_partial_custom_and_fenced_conflicts() {
        for content in [
            "<<<<<<< HEAD\nours\n=======\ntheirs\n>>>>>>> branch\n",
            ">>>>>>> branch",
            "<<<<<<<",
            "<<<<<<<<< HEAD",
            "```text\n<<<<<<< HEAD\n```\n",
            "\u{feff}<<<<<<< HEAD\r\n",
            "<<<<<<< HEAD\nours\n||||||| base\nbase\n=======\ntheirs\n>>>>>>> branch",
        ] {
            assert!(detect(content).is_some(), "{content:?}");
        }
        assert_eq!(detect("# Title\r\n\r\n>>>>>>> branch").unwrap().line, 3);
    }

    #[test]
    fn merge_conflict_protects_shared_fix_engine_and_document_run() {
        let content = "# Title\r\n\n<<<<<<< HEAD\r\ntext   ";
        let config = crate::config::Config::default();
        let rules = crate::rules::all_rules(&config);
        let mut fixed = content.to_string();
        let result = crate::fix_coordinator::FixCoordinator::new()
            .apply_fixes_iterative(&rules, &[], &mut fixed, &config, 100, None)
            .unwrap();
        assert_eq!(fixed, content);
        assert_eq!(result.rules_fixed, 0);
        assert_eq!(result.iterations, 0);
        let run = crate::document_run::DocumentRun::new(content, &rules, &config);
        assert_eq!(run.fix(100).unwrap().0, content);
        let analysis = run.analyze().unwrap();
        assert_eq!(analysis.warnings.len(), 1);
        assert!(analysis.warnings[0].fix.is_none());
    }

    #[test]
    fn merge_conflict_line_suppression_does_not_hide_other_markers() {
        let config = crate::config::Config::default();
        let content = "<!-- rumdl-disable-next-line merge-conflict -->\n<<<<<<< HEAD\ntext\n>>>>>>> side";
        let warning = detect_configured(content, &config, None).unwrap();
        assert_eq!(warning.line, 4);
        assert_eq!(warning.rule_name.as_deref(), Some(RULE_NAME));
    }

    #[test]
    fn merge_conflict_configured_index_keeps_documented_headings() {
        // A run's own selection decides, and the cache fast path is handed the
        // same one, so the two paths index this document identically whether the
        // configuration keeps the rule or drops it.
        let content = "# Example\n\n```text\n<<<<<<< HEAD\n```\n";
        for (disabled, indexed) in [(true, true), (false, false)] {
            let mut config = crate::config::Config::default();
            if disabled {
                config.global.disable.push(RULE_NAME.into());
            }
            let rules = crate::rules::filter_rules(&crate::rules::all_rules(&config), &config.global);
            let run = crate::document_run::DocumentRun::new(content, &rules, &config);
            let normal = run.analyze().unwrap().file_index;
            let cached =
                crate::build_file_index_only_for_selection(content, &rules, config.markdown_flavor(), None, &config);
            assert_eq!(!normal.headings.is_empty(), indexed, "disabled: {disabled}");
            assert_eq!(normal.headings.len(), cached.headings.len(), "disabled: {disabled}");
        }
    }

    #[test]
    fn merge_conflict_selection_outranks_a_configuration_disable() {
        let content = "<<<<<<< HEAD\ntext\n>>>>>>> side\n";
        let mut config = crate::config::Config::default();
        config.global.disable.push(RULE_NAME.into());
        // What `--enable MD092` resolves to: a selection the configuration lost.
        let selection: Vec<Box<dyn Rule>> = vec![Box::new(MD092MergeConflict)];
        assert!(detect_for_rules(content, &selection, &config, None).is_some());
        assert!(detect_configured(content, &config, None).is_none());
        // A selection without the rule reports nothing, whatever configuration says.
        assert!(detect_for_rules(content, &[], &crate::config::Config::default(), None).is_none());
    }

    #[test]
    fn merge_conflict_index_guard_survives_a_role_scoped_rule_list() {
        // The LSP indexes with the cross-file rules alone, which never include
        // MD092, so configuration has to be what decides there.
        let content = "# Title\n\n<<<<<<< HEAD\ntext\n>>>>>>> side\n";
        let config = crate::config::Config::default();
        let cross_file: Vec<Box<dyn Rule>> = vec![Box::new(crate::rules::MD051LinkFragments::new())];
        let index =
            crate::build_file_index_only_with_config(content, &cross_file, config.markdown_flavor(), None, &config);
        assert!(index.headings.is_empty());
        let unconflicted =
            crate::build_file_index_only_with_config("# Title\n", &cross_file, config.markdown_flavor(), None, &config);
        assert_eq!(unconflicted.headings.len(), 1);
    }

    #[test]
    fn preserves_ordinary_markdown_syntax() {
        for content in [
            "Title\n=======\n",
            "||||||| base",
            "<<<<<< HEAD",
            ">>> quote",
            ">>>>>>>quote",
            "<<<<<<<not-a-label",
            "text <<<<<<< HEAD",
            "    <<<<<<< HEAD",
            "> > > > > > > quote",
        ] {
            assert!(detect(content).is_none(), "{content:?}");
        }
    }
}
