//! MD092 detects unresolved conflicts and protects the entire document from edits.
//!
//! Scan raw lines, including fenced code: Git can insert conflicts anywhere.
//! Opening or closing markers suffice because conflicts may be partially resolved.
//! Separators alone are valid Setext headings, and diff3 base markers alone can
//! be table content, so neither is evidence of a conflict on its own.

use crate::rule::{FixCapability, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::utils::bom::strip_bom;
use std::borrow::Cow;

pub const RULE_NAME: &str = "MD092";

/// Conflict-marker facts attached to immutable source text. Configuration and
/// suppression are evaluated separately for each caller's effective rule set.
pub struct ConflictScan<'a> {
    content: Cow<'a, str>,
    markers: Vec<Marker>,
}

impl<'a> ConflictScan<'a> {
    /// Scan supplied document text, including its encoding marker if present.
    /// Analysis text must use the `detect_for_analysis` entry point instead.
    pub fn new(content: impl Into<Cow<'a, str>>) -> Self {
        let content = content.into();
        let markers = scan_markers(strip_bom(&content)).collect();
        Self { content, markers }
    }

    pub fn content(&self) -> &str {
        &self.content
    }

    /// Retain the scan for unchanged LF input; rescan any normalized text.
    /// For example, `<<<<<<<\r\r\n` becomes a marker only after normalization.
    pub fn normalize_lf(mut self) -> Self {
        if let Cow::Owned(normalized) = crate::utils::normalize_line_ending(&self.content, crate::utils::LineEnding::Lf)
        {
            self.markers = scan_markers(strip_bom(&normalized)).collect();
            self.content = Cow::Owned(normalized);
        }
        self
    }

    pub fn into_content(self) -> Cow<'a, str> {
        self.content
    }

    pub fn detect_for_rules(
        &self,
        rules: &[Box<dyn Rule>],
        config: &crate::config::Config,
        path: Option<&std::path::Path>,
    ) -> Option<LintWarning> {
        if self.markers.is_empty() || !rules.iter().any(|rule| rule.name() == RULE_NAME) {
            return None;
        }
        detect_suppressed(strip_bom(self.content()), &self.markers, config, path)
    }

    pub(crate) fn markers(&self) -> &[Marker] {
        &self.markers
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Marker {
    line: usize,
    column: usize,
    width: usize,
}

impl Marker {
    pub(crate) fn warning(self) -> LintWarning {
        LintWarning {
            rule_name: Some(RULE_NAME.to_string()),
            message: "Unresolved merge conflict; formatting skipped until conflict markers are removed".to_string(),
            line: self.line,
            column: self.column,
            end_line: self.line,
            end_column: self.width + self.column,
            severity: Severity::Error,
            fix: None,
        }
    }
}

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
        Ok(ctx.merge_conflicts().iter().map(|marker| marker.warning()).collect())
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
    let scan = ConflictScan::new(content);
    let content = strip_bom(content);
    if scan.markers.is_empty() {
        return None;
    }
    let rules: Vec<Box<dyn Rule>> = vec![Box::new(MD092MergeConflict)];
    if crate::rules::filter_rules(&rules, &config.global).is_empty() {
        return None;
    }
    detect_suppressed(content, &scan.markers, config, path)
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
    ConflictScan::new(content).detect_for_rules(rules, config, path)
}

/// Like `detect_for_rules` for text that is already analysis text (the leading
/// byte order mark removed), so a second mark is not stripped again.
pub fn detect_for_analysis(
    analysis: &str,
    rules: &[Box<dyn Rule>],
    config: &crate::config::Config,
    path: Option<&std::path::Path>,
) -> Option<LintWarning> {
    if !rules.iter().any(|rule| rule.name() == RULE_NAME) {
        return None;
    }
    let markers: Vec<Marker> = scan_markers(analysis).collect();
    detect_suppressed(analysis, &markers, config, path)
}

/// Like `detect_configured` for text that is already analysis text.
pub fn detect_configured_for_analysis(
    analysis: &str,
    config: &crate::config::Config,
    path: Option<&std::path::Path>,
) -> Option<LintWarning> {
    let markers: Vec<Marker> = scan_markers(analysis).collect();
    if markers.is_empty() {
        return None;
    }
    let rules: Vec<Box<dyn Rule>> = vec![Box::new(MD092MergeConflict)];
    if crate::rules::filter_rules(&rules, &config.global).is_empty() {
        return None;
    }
    detect_suppressed(analysis, &markers, config, path)
}

pub(crate) fn detect_for_context(
    ctx: &crate::lint_context::LintContext<'_>,
    rules: &[Box<dyn Rule>],
    config: &crate::config::Config,
    path: Option<&std::path::Path>,
) -> Option<LintWarning> {
    if !rules.iter().any(|rule| rule.name() == RULE_NAME) {
        return None;
    }
    detect_suppressed(ctx.content, ctx.merge_conflicts(), config, path)
}

/// The first marker this document's own suppressions leave standing, at the
/// severity configuration gives the rule. The caller has already decided that
/// the rule runs at all.
fn detect_suppressed(
    content: &str,
    markers: &[Marker],
    config: &crate::config::Config,
    path: Option<&std::path::Path>,
) -> Option<LintWarning> {
    if markers.is_empty() {
        return None;
    }
    if path.is_some_and(|path| config.get_ignored_rules_for_file(path).contains(RULE_NAME)) {
        return None;
    }
    let inline = crate::inline_config::InlineConfig::from_content(content);
    let mut warning = markers
        .iter()
        .find(|marker| !inline.is_rule_disabled(RULE_NAME, marker.line))?
        .warning();
    if let Some(severity) = config.get_rule_severity(RULE_NAME) {
        warning.severity = severity;
    }
    Some(warning)
}

/// Find the first Git conflict marker, including custom marker widths >= 7.
/// Labels must be separated by whitespace, as in Git's marker syntax.
/// Raw detection, without configuration or inline suppression.
pub fn detect(content: &str) -> Option<LintWarning> {
    scan_markers(strip_bom(content)).next().map(Marker::warning)
}

/// Markers in analysis text. The caller removes the byte order mark first, so
/// line 1 starts at column 1 and a second mark is ordinary content.
pub(crate) fn scan_markers(content: &str) -> impl Iterator<Item = Marker> + '_ {
    // Search for the required runs before walking lines. Most documents have
    // none, even when they contain HTML and blockquotes.
    let content = if content.contains("<<<<<<<") || content.contains(">>>>>>>") {
        content
    } else {
        ""
    };
    content.lines().enumerate().filter_map(|(index, line)| {
        let marker = *line.as_bytes().first()?;
        if !matches!(marker, b'<' | b'>') {
            return None;
        }
        let width = line.bytes().take_while(|&byte| byte == marker).count();
        if width < 7 || !matches!(line.as_bytes().get(width), None | Some(b' ' | b'\t')) {
            return None;
        }
        Some(Marker {
            line: index + 1,
            column: 1,
            width,
        })
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn analysis_detection_leaves_a_second_mark_in_front_of_the_marker() {
        let config = crate::config::Config::default();
        let conflicted = "<<<<<<< HEAD\nours\n";
        assert!(super::detect_configured_for_analysis(conflicted, &config, None).is_some());
        // Not stripped again: the mark hides the marker, as in `detect_configured`.
        let hidden = format!("\u{feff}{conflicted}");
        assert!(super::detect_configured_for_analysis(&hidden, &config, None).is_none());
        // The carrier entry point strips one mark, so one mark in front still finds it.
        assert!(super::detect_configured(&hidden, &config, None).is_some());
    }

    use super::*;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn scan_and_normalization_match_raw_line_detection(
            parts in prop::collection::vec(prop_oneof![
                Just("\n".to_string()), Just("\r".to_string()),
                Just("\u{feff}".to_string()), Just("<<<<<<<".to_string()),
                Just(">>>>>>>".to_string()), Just(" ".to_string()),
                Just("\t".to_string()), any::<char>().prop_map(|c| c.to_string()),
            ], 0..160)
        ) {
            let original = parts.concat();
            // The raw-line reading, with exactly one leading mark removed, without the substring prefilter.
            let reference = |content: &str| {
                crate::utils::bom::strip_bom(content).lines().enumerate().filter_map(|(index, line)| {
                    let first = *line.as_bytes().first()?;
                    let width = line.bytes().take_while(|&byte| byte == first).count();
                    (matches!(first, b'<' | b'>') && width >= 7
                        && matches!(line.as_bytes().get(width), None | Some(b' ' | b'\t')))
                        .then_some(Marker { line: index + 1, column: 1, width })
                }).collect::<Vec<_>>()
            };
            let scan = ConflictScan::new(&original);
            prop_assert_eq!(scan.markers(), reference(&original));
            let scan = scan.normalize_lf();
            let normalized = original.replace("\r\n", "\n");
            prop_assert_eq!(scan.content(), normalized.as_str());
            prop_assert_eq!(scan.markers(), reference(&normalized));
            prop_assert_eq!(scan.into_content(), normalized);
        }
    }

    #[test]
    fn normalization_rechecks_repeated_carriage_returns() {
        let original = "<<<<<<<\r\r\ntext   \n";
        let config = crate::config::Config::default();
        let rules = crate::rules::all_rules(&config);
        let scan = ConflictScan::new(original);
        assert!(scan.detect_for_rules(&rules, &config, None).is_none());
        let scan = scan.normalize_lf();
        assert!(scan.detect_for_rules(&rules, &config, None).is_some());
        let result = crate::document_run::DocumentRun::from_scanned(&scan, &rules, &config).analyze_and_fix(100);
        assert_eq!(result.warnings.unwrap().len(), 1);
        let (fixed, stats) = result.fixes.unwrap().unwrap();
        assert_eq!(fixed, scan.content());
        assert_eq!(stats.iterations, 0);
    }

    #[test]
    fn shared_scan_keeps_selection_configuration_and_suppression_local() {
        let content = "<!-- rumdl-disable-next-line MD092 -->\n<<<<<<< HEAD\ntext\n>>>>>>> side\n";
        let scan = ConflictScan::new(content);
        let mut config = crate::config::Config::default();
        config.global.disable.push(RULE_NAME.into());
        let selection: Vec<Box<dyn Rule>> = vec![Box::new(MD092MergeConflict)];
        assert!(scan.detect_for_rules(&[], &config, None).is_none());
        assert_eq!(scan.detect_for_rules(&selection, &config, None).unwrap().line, 4);
        let run = crate::document_run::DocumentRun::from_scanned(&scan, &selection, &config);
        assert_eq!(run.analyze().unwrap().warnings[0].line, 4);

        // Suppressed raw findings remain available to MD087's observer.
        let ctx = crate::lint_context::LintContext::new(content, config.markdown_flavor(), None)
            .with_merge_conflicts(scan.markers());
        assert_eq!(MD092MergeConflict.check(&ctx).unwrap().len(), 2);
    }

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
    fn a_marker_behind_the_byte_order_mark_is_at_column_one() {
        let warning = detect("\u{feff}<<<<<<< HEAD\n").unwrap();
        assert_eq!((warning.line, warning.column), (1, 1));
    }

    #[test]
    fn a_second_byte_order_mark_hides_the_marker_and_is_not_stripped_again() {
        let content = "\u{feff}\u{feff}<<<<<<< HEAD\n";
        assert!(detect(content).is_none());
        assert!(ConflictScan::new(content).markers().is_empty());
        let config = crate::config::Config::default();
        let rules = crate::rules::all_rules(&config);
        assert!(detect_for_analysis("<<<<<<< HEAD\n", &rules, &config, None).is_some());
        // Analysis text already lost its one mark; the remaining mark is content.
        assert!(detect_for_analysis(crate::utils::bom::strip_bom(content), &rules, &config, None).is_none());
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
