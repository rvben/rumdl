//! Rule MD087: Inline disable comments should suppress something.
//!
//! A `<!-- rumdl-disable-line MD013 -->` written to silence a finding stays behind
//! after the line is rewritten or the rule stops reporting it. Nothing else
//! notices: a comment that suppresses nothing costs no findings, so it survives
//! every run and quietly widens the set of rules that cannot report on that line
//! again.
//!
//! The rule judges a comment by what the run around it actually suppressed, so it
//! reports only what the current configuration makes unnecessary. A comment a
//! wider one already covers is reported too: with the rule off for the whole file,
//! a `disable-line` naming it silences nothing of its own, and the wider comment
//! keeps the line quiet once the narrower one is gone. Three kinds of comment are
//! deliberately left alone:
//!
//! - one naming a rule this run does not carry, since a rule configuration turned
//!   off produced nothing and its comment cannot be judged by that silence
//! - one naming no rule at all, which disables every rule at once, including ones
//!   a given run may not carry
//! - `<!-- prettier-ignore -->`, which belongs to another formatter
//!
//! Detection only. Removing a comment is a content decision: the author may be
//! about to restore the line that needed it, and `rumdl fmt` must not delete
//! authored comments on its own.

use std::collections::{HashMap, HashSet};

use crate::inline_config::{DisableLayer, DisableScope, DisableSite, collect_disable_sites, normalize_rule_name};
use crate::lint_context::{LintContext, SourcePositions};
use crate::rule::{FixCapability, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity, SuppressionReport};

#[derive(Default)]
struct SuppressionUse {
    file: bool,
    last_block_line: usize,
    lines: HashSet<usize>,
}

/// Keep each suppression layer separate: a file disable must not give a line
/// comment credit. Block scopes reach every later line, so only their last
/// suppressed line is needed to answer whether a block comment was used.
struct SuppressionIndex<'a>(HashMap<&'a str, SuppressionUse>);

impl<'a> SuppressionIndex<'a> {
    fn new(report: &'a SuppressionReport) -> Self {
        let mut uses: HashMap<&str, SuppressionUse> = HashMap::new();
        for warning in &report.suppressed {
            let usage = uses.entry(&warning.rule_name).or_default();
            match warning.layer {
                DisableLayer::File => usage.file = true,
                DisableLayer::Block => usage.last_block_line = usage.last_block_line.max(warning.line),
                DisableLayer::Line => {
                    usage.lines.insert(warning.line);
                }
            }
        }
        Self(uses)
    }

    fn used(&self, rule: &str, scope: DisableScope) -> bool {
        self.0.get(rule).is_some_and(|usage| match scope {
            DisableScope::File => usage.file,
            DisableScope::Block(start) => start < usage.last_block_line,
            DisableScope::Line(line) => usage.lines.contains(&line),
        })
    }
}

/// Keep cheap early matches cheap. Across all queries, scan at most two reports'
/// worth of entries before switching to indexed lookups. Cheap matches for
/// broad disable comments do not need an index.
struct SuppressionLookup<'a> {
    report: &'a SuppressionReport,
    scans_left: usize,
    index: std::cell::OnceCell<SuppressionIndex<'a>>,
}

impl<'a> SuppressionLookup<'a> {
    fn new(report: &'a SuppressionReport) -> Self {
        Self {
            report,
            scans_left: report.suppressed.len(),
            index: std::cell::OnceCell::new(),
        }
    }

    fn used(&mut self, rule: &str, scope: DisableScope) -> bool {
        if self.scans_left > 0 {
            let found = self
                .report
                .suppressed
                .iter()
                .position(|warning| warning.rule_name == rule && scope.carries(warning.layer, warning.line));
            let scanned = found.map_or(self.report.suppressed.len(), |index| index + 1);
            self.scans_left = self.scans_left.saturating_sub(scanned);
            found.is_some()
        } else {
            self.index
                .get_or_init(|| SuppressionIndex::new(self.report))
                .used(rule, scope)
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct MD087UnusedDisableComment;

impl MD087UnusedDisableComment {
    pub fn new() -> Self {
        Self
    }

    /// The rules a comment names that suppressed nothing, in the order written.
    ///
    /// A comment naming no rule returns nothing: it disables every rule, so the
    /// findings of one run cannot show that it silenced nothing.
    fn unused_rules(&self, site: &DisableSite, lookup: &mut SuppressionLookup<'_>) -> Vec<String> {
        let mut unused: Vec<String> = Vec::new();

        for written in &site.rules {
            // The same canonicalization the inline config applies, so a comment is
            // judged against the rule it really disables. An unrecognized name
            // canonicalizes to nothing the run carries and drops out below.
            let canonical = normalize_rule_name(written);
            // This rule's own findings are raised after the report is assembled,
            // so a comment silencing them cannot appear in it.
            if canonical == self.name() || !lookup.report.judged_rules.contains(&canonical) {
                continue;
            }
            let used = lookup.used(&canonical, site.scope);
            if !used && !unused.contains(written) {
                unused.push(written.clone());
            }
        }

        unused
    }

    fn diagnostic_position(ctx: &LintContext, positions: &mut SourcePositions<'_>, offset: usize) -> (usize, usize) {
        let line = ctx.line_number_at_offset(offset);
        if let Some(info) = ctx.line_info(line)
            && offset <= info.byte_offset + info.byte_len
        {
            (line, positions.column_on_line(line - 1, offset) + 1)
        } else {
            // Multiline comments are clipped to split('\n') lines, which retain
            // CR. Preserve scalar positions outside the content-only line table.
            ctx.offset_to_line_col(offset)
        }
    }

    fn warning(
        &self,
        ctx: &LintContext,
        positions: &mut SourcePositions<'_>,
        site: &DisableSite,
        unused: &[String],
    ) -> LintWarning {
        let line_offset = ctx.line_info(site.line).map_or(0, |info| info.byte_offset);
        let (line, column) = Self::diagnostic_position(ctx, positions, line_offset + site.span.start);
        let (_, end_column) = Self::diagnostic_position(ctx, positions, line_offset + site.span.end);
        let names = unused.join(", ");
        // A configure-file comment configures rules rather than disabling a span,
        // so it is named for what the entry does instead of what the comment is.
        let message = if site.kind == "configure-file" {
            format!("Unused configure-file disable: {names}")
        } else {
            format!("Unused {} comment: {names}", site.kind)
        };
        LintWarning {
            rule_name: Some(self.name().to_string()),
            severity: Severity::Warning,
            line,
            column,
            end_line: line,
            end_column,
            message,
            fix: None,
        }
    }
}

impl Rule for MD087UnusedDisableComment {
    fn name(&self) -> &'static str {
        "MD087"
    }

    fn description(&self) -> &'static str {
        "Inline disable comments should suppress something"
    }

    fn category(&self) -> RuleCategory {
        RuleCategory::Other
    }

    fn should_skip(&self, ctx: &LintContext) -> bool {
        !ctx.content.contains("<!--")
    }

    fn check(&self, _ctx: &LintContext) -> LintResult {
        // A comment is judged by what the rest of the run suppressed, which is
        // only known once every other rule has finished. That arrives through
        // check_suppressions.
        Ok(Vec::new())
    }

    fn observes_suppressions(&self) -> bool {
        true
    }

    fn check_suppressions(&self, ctx: &LintContext, report: &SuppressionReport) -> LintResult {
        let mut warnings = Vec::new();
        let sites = collect_disable_sites(ctx.content, &ctx.code_blocks, ctx.inline_code_ranges());
        if sites.is_empty() {
            return Ok(warnings);
        }
        let mut lookup = SuppressionLookup::new(report);
        let mut positions = SourcePositions::new(ctx.content, &ctx.lines);
        for site in sites {
            let unused = self.unused_rules(&site, &mut lookup);
            if unused.is_empty() {
                continue;
            }
            warnings.push(self.warning(ctx, &mut positions, &site, &unused));
        }
        Ok(warnings)
    }

    fn fix_capability(&self) -> FixCapability {
        FixCapability::Unfixable
    }

    fn fix(&self, ctx: &LintContext) -> Result<String, LintError> {
        Ok(ctx.content.to_string())
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn from_config(_config: &crate::config::Config) -> Box<dyn Rule>
    where
        Self: Sized,
    {
        Box::new(Self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::MarkdownFlavor;
    use crate::inline_config::DisableLayer;
    use crate::rule::SuppressedWarning;
    use std::collections::HashSet;

    #[test]
    fn suppression_index_matches_layer_and_scope_scans() {
        let suppressed = [
            ("MD013", DisableLayer::Block, 0),
            ("MD013", DisableLayer::Block, 30),
            ("MD013", DisableLayer::Block, 10),
            ("MD001", DisableLayer::Line, 7),
            ("md013", DisableLayer::File, 0),
            ("MD013", DisableLayer::Line, 0),
            ("MD001", DisableLayer::Line, usize::MAX),
            ("MD013", DisableLayer::Block, usize::MAX),
            ("MD001", DisableLayer::File, 2),
        ];
        for count in 0..=suppressed.len() {
            let report = report(&suppressed[..count], &["MD013", "MD001"]);
            let index = SuppressionIndex::new(&report);
            let mut lookup = SuppressionLookup::new(&report);
            for rule in ["MD013", "md013", "MD001", "MD087", "unknown"] {
                for scope in [0, 1, 7, 10, 20, 30, usize::MAX]
                    .into_iter()
                    .flat_map(|line| [DisableScope::Line(line), DisableScope::Block(line)])
                    .chain(std::iter::once(DisableScope::File))
                {
                    let expected = report
                        .suppressed
                        .iter()
                        .any(|warning| warning.rule_name == rule && scope.carries(warning.layer, warning.line));
                    assert_eq!(index.used(rule, scope), expected, "{rule} {scope:?}, {count} entries");
                    assert_eq!(lookup.used(rule, scope), expected, "{rule} {scope:?}, {count} entries");
                }
            }
        }
    }

    #[test]
    fn broad_disables_keep_cheap_early_matches() {
        let report = report(&[("MD013", DisableLayer::File, 1); 100], &["MD013"]);
        let mut lookup = SuppressionLookup::new(&report);
        for _ in 0..10 {
            assert!(lookup.used("MD013", DisableScope::File));
        }
        assert!(
            lookup.index.get().is_none(),
            "early matches should not index the whole report"
        );
        assert!(!lookup.used("MD001", DisableScope::File));
        assert!(lookup.used("MD013", DisableScope::File));
        assert!(
            lookup.index.get().is_some(),
            "repeated scans should switch to the index"
        );
    }

    #[test]
    fn cached_comment_positions_match_scalar_positions() {
        for content in [
            String::new(),
            "\n".into(),
            "é日本🙂\r\n<!-- rumdl-disable-line MD013 -->\r\n".into(),
            format!("{}<!-- rumdl-disable\nMD013 -->\r\n", "é日本🙂 ".repeat(80)),
        ] {
            let ctx = LintContext::new(&content, MarkdownFlavor::Standard, None);
            let mut positions = SourcePositions::new(&content, &ctx.lines);
            // Also exercise backwards queries and endpoints outside the parser's
            // content-only line table, including CRLF and trailing EOF.
            for offset in (0..=content.len() + 1)
                .chain((0..=content.len() + 1).rev())
                .chain(std::iter::once(usize::MAX))
            {
                assert_eq!(
                    MD087UnusedDisableComment::diagnostic_position(&ctx, &mut positions, offset),
                    ctx.offset_to_line_col(offset),
                    "{content:?}, byte {offset}"
                );
            }
        }
    }

    fn report(suppressed: &[(&str, DisableLayer, usize)], judged: &[&str]) -> SuppressionReport {
        SuppressionReport {
            suppressed: suppressed
                .iter()
                .map(|&(rule_name, layer, line)| SuppressedWarning {
                    rule_name: rule_name.to_string(),
                    line,
                    layer,
                })
                .collect(),
            judged_rules: judged.iter().map(|name| (*name).to_string()).collect::<HashSet<_>>(),
        }
    }

    fn check(content: &str, suppressed: &[(&str, DisableLayer, usize)], judged: &[&str]) -> Vec<LintWarning> {
        check_in(MarkdownFlavor::Standard, content, suppressed, judged)
    }

    fn check_in(
        flavor: MarkdownFlavor,
        content: &str,
        suppressed: &[(&str, DisableLayer, usize)],
        judged: &[&str],
    ) -> Vec<LintWarning> {
        let ctx = LintContext::new(content, flavor, None);
        MD087UnusedDisableComment::new()
            .check_suppressions(&ctx, &report(suppressed, judged))
            .unwrap()
    }

    #[test]
    fn reports_a_compact_disable_line_comment_that_suppressed_nothing() {
        let content = "# Title\n\nA short line <!--rumdl-disable-line MD013-->\n";
        let warnings = check(content, &[], &["MD013"]);
        assert_eq!(warnings.len(), 1, "got: {warnings:?}");
        assert_eq!(warnings[0].message, "Unused disable-line comment: MD013");
        // A compact comment that did suppress something is not reported.
        let used = check(content, &[("MD013", DisableLayer::Line, 3)], &["MD013"]);
        assert!(used.is_empty(), "got: {used:?}");
    }

    #[test]
    fn reports_a_disable_line_comment_that_suppressed_nothing() {
        let content = "# Title\n\nA short line <!-- rumdl-disable-line MD013 -->\n";
        let warnings = check(content, &[], &["MD013"]);
        assert_eq!(warnings.len(), 1, "got: {warnings:?}");
        assert_eq!(warnings[0].message, "Unused disable-line comment: MD013");
        assert_eq!((warnings[0].line, warnings[0].column), (3, 14));
        assert_eq!(warnings[0].end_column, 47, "the warning spans the comment");
        assert!(
            warnings[0].fix.is_none(),
            "removing an authored comment is not automatic"
        );
    }

    #[test]
    fn keeps_quiet_when_the_comment_suppressed_a_finding() {
        let content = "# Title\n\nA short line <!-- rumdl-disable-line MD013 -->\n";
        let warnings = check(content, &[("MD013", DisableLayer::Line, 3)], &["MD013"]);
        assert!(warnings.is_empty(), "got: {warnings:?}");
    }

    #[test]
    fn a_disable_line_comment_is_judged_on_its_own_line_only() {
        let content = "<!-- rumdl-disable-line MD013 -->\nA long line\n";
        let warnings = check(content, &[("MD013", DisableLayer::Line, 2)], &["MD013"]);
        assert_eq!(warnings.len(), 1, "a finding on line 2 is not this comment's doing");
        assert_eq!(warnings[0].line, 1);
    }

    #[test]
    fn a_disable_next_line_comment_is_judged_on_the_following_line() {
        let content = "<!-- rumdl-disable-next-line MD013 -->\nA long line\n";
        assert!(
            check(content, &[("MD013", DisableLayer::Line, 2)], &["MD013"]).is_empty(),
            "the suppression on line 2 is what the comment is for"
        );
        let warnings = check(content, &[("MD013", DisableLayer::Line, 1)], &["MD013"]);
        assert_eq!(warnings.len(), 1, "a finding on line 1 is not this comment's doing");
        assert_eq!(warnings[0].message, "Unused disable-next-line comment: MD013");
    }

    #[test]
    fn a_block_disable_reaches_the_end_of_the_document() {
        let content = "<!-- rumdl-disable MD013 -->\n\ntext\n\n<!-- rumdl-enable MD013 -->\n\nmore\n";
        assert!(
            check(content, &[("MD013", DisableLayer::Block, 7)], &["MD013"]).is_empty(),
            "a scope wider than the truth may only under-report"
        );
        let warnings = check(content, &[], &["MD013"]);
        assert_eq!(warnings.len(), 1, "got: {warnings:?}");
        assert_eq!(warnings[0].message, "Unused disable comment: MD013");
    }

    #[test]
    fn a_disable_file_comment_covers_a_finding_above_it() {
        let content = "A long line\n\n<!-- rumdl-disable-file MD013 -->\n";
        assert!(
            check(content, &[("MD013", DisableLayer::File, 1)], &["MD013"]).is_empty(),
            "disable-file applies to the whole document, including lines above it"
        );
    }

    #[test]
    fn a_comment_a_wider_one_already_covers_is_reported() {
        let content = "<!-- rumdl-disable-file MD013 -->\n\nA long line <!-- rumdl-disable-line MD013 -->\n";
        let warnings = check(content, &[("MD013", DisableLayer::File, 3)], &["MD013"]);
        assert_eq!(warnings.len(), 1, "got: {warnings:?}");
        assert_eq!(warnings[0].message, "Unused disable-line comment: MD013");
        assert_eq!(warnings[0].line, 3, "the file-wide comment is the one doing the work");
    }

    #[test]
    fn the_wider_comment_is_the_one_reported_when_the_narrow_one_does_the_work() {
        // The converse of the case above: the block disable is closed before the
        // finding, so the line comment is what keeps it quiet.
        let content = "<!-- rumdl-disable MD013 -->\n<!-- rumdl-enable MD013 -->\nA long line <!-- rumdl-disable-line MD013 -->\n";
        let warnings = check(content, &[("MD013", DisableLayer::Line, 3)], &["MD013"]);
        assert_eq!(warnings.len(), 1, "got: {warnings:?}");
        assert_eq!(warnings[0].message, "Unused disable comment: MD013");
        assert_eq!(warnings[0].line, 1);
    }

    #[test]
    fn only_the_unused_names_of_a_multi_rule_comment_are_reported() {
        let content = "text <!-- rumdl-disable-line MD013 MD033 MD009 -->\n";
        let warnings = check(
            content,
            &[("MD033", DisableLayer::Line, 1)],
            &["MD009", "MD013", "MD033"],
        );
        assert_eq!(warnings.len(), 1, "got: {warnings:?}");
        assert_eq!(warnings[0].message, "Unused disable-line comment: MD013, MD009");
    }

    #[test]
    fn a_rule_the_run_does_not_carry_is_not_judged() {
        let content = "text <!-- rumdl-disable-line MD013 -->\n";
        assert!(
            check(content, &[], &["MD009"]).is_empty(),
            "MD013 produced nothing because it did not run"
        );
    }

    #[test]
    fn an_unknown_rule_name_is_not_judged() {
        let content = "text <!-- rumdl-disable-line MD999 -->\n";
        assert!(
            check(content, &[], &["MD013"]).is_empty(),
            "MD999 is not a rule the run carries"
        );
    }

    #[test]
    fn a_comment_naming_no_rule_is_never_reported() {
        let content = "text <!-- rumdl-disable-line -->\n<!-- rumdl-disable -->\n";
        assert!(
            check(content, &[], &["MD013"]).is_empty(),
            "a bare comment disables rules this run may not carry"
        );
    }

    #[test]
    fn prettier_ignore_belongs_to_another_formatter() {
        let content = "<!-- prettier-ignore -->\n| a | b |\n";
        assert!(
            check(content, &[], &["MD013"]).is_empty(),
            "not rumdl's comment to judge"
        );
    }

    #[test]
    fn a_comment_inside_a_code_block_configures_nothing() {
        let content = "# Title\n\n```markdown\n<!-- rumdl-disable-line MD013 -->\n```\n";
        assert!(
            check(content, &[], &["MD013"]).is_empty(),
            "a fenced example documents a comment rather than writing one"
        );
    }

    #[test]
    fn a_comment_inside_a_code_span_configures_nothing() {
        let content = "# Title\n\nSee `<!-- rumdl-disable-line MD013 -->` here.\n";
        assert!(
            check(content, &[], &["MD013"]).is_empty(),
            "backticks show a comment rather than writing one"
        );
    }

    #[test]
    fn a_comment_beside_one_in_a_code_span_is_still_judged() {
        // The positive control for the case above: a span speaks only for the
        // comment it holds.
        let content = "`<!-- rumdl-disable-line MD013 -->` <!-- rumdl-disable-line MD033 -->\n";
        let warnings = check(content, &[], &["MD013", "MD033"]);
        assert_eq!(warnings.len(), 1, "got: {warnings:?}");
        assert_eq!(warnings[0].message, "Unused disable-line comment: MD033");
    }

    #[test]
    fn a_comment_in_an_indented_container_body_is_judged() {
        // A MkDocs admonition holds its content at a four-space indent, which is
        // structure rather than code, so the comment written there is live and
        // stale once it suppresses nothing.
        let content = "# Title\n\n!!! example\n\n    A short line <!-- rumdl-disable-line MD013 -->\n";
        let warnings = check_in(MarkdownFlavor::MkDocs, content, &[], &["MD013"]);
        assert_eq!(warnings.len(), 1, "got: {warnings:?}");
        assert_eq!(warnings[0].line, 5);
        assert!(
            check_in(MarkdownFlavor::Standard, content, &[], &["MD013"]).is_empty(),
            "without admonitions the same lines are an indented code block"
        );
    }

    #[test]
    fn an_alias_is_reported_as_the_author_wrote_it() {
        let content = "text <!-- rumdl-disable-line line-length -->\n";
        let warnings = check(content, &[], &["MD013"]);
        assert_eq!(warnings.len(), 1, "got: {warnings:?}");
        assert_eq!(warnings[0].message, "Unused disable-line comment: line-length");
    }

    #[test]
    fn a_markdownlint_comment_is_judged_the_same_way() {
        let content = "text <!-- markdownlint-disable-line MD013 -->\n";
        let warnings = check(content, &[], &["MD013"]);
        assert_eq!(warnings.len(), 1, "got: {warnings:?}");
        assert_eq!(warnings[0].message, "Unused disable-line comment: MD013");
    }

    #[test]
    fn a_configure_file_entry_turning_a_rule_off_is_judged_like_a_disable() {
        let content = "<!-- rumdl-configure-file { \"MD013\": false } -->\n\ntext\n";
        let warnings = check(content, &[], &["MD013"]);
        assert_eq!(warnings.len(), 1, "got: {warnings:?}");
        assert_eq!(warnings[0].message, "Unused configure-file disable: MD013");
        assert!(
            check(content, &[("MD013", DisableLayer::File, 3)], &["MD013"]).is_empty(),
            "the entry turned the rule off for the whole file"
        );
    }

    #[test]
    fn a_configure_file_entry_carrying_options_is_not_a_disable() {
        let content = "<!-- rumdl-configure-file { \"MD013\": { \"line_length\": 200 } } -->\n\ntext\n";
        assert!(
            check(content, &[], &["MD013"]).is_empty(),
            "configuring a rule is not suppressing it"
        );
    }

    #[test]
    fn this_rule_never_judges_a_comment_silencing_itself() {
        let content = "text <!-- rumdl-disable-line MD087 -->\n";
        assert!(
            check(content, &[], &["MD013", "MD087"]).is_empty(),
            "MD087 findings are raised after the report is assembled"
        );
    }

    #[test]
    fn the_column_is_measured_in_characters() {
        let content = "héllo wörld <!-- rumdl-disable-line MD013 -->\n";
        let warnings = check(content, &[], &["MD013"]);
        assert_eq!(warnings.len(), 1, "got: {warnings:?}");
        assert_eq!(warnings[0].column, 13, "two multi-byte characters precede the comment");
    }

    #[test]
    fn check_reports_nothing_on_its_own() {
        let content = "text <!-- rumdl-disable-line MD013 -->\n";
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        assert!(
            MD087UnusedDisableComment::new().check(&ctx).unwrap().is_empty(),
            "the verdict needs the run's suppressions"
        );
    }
}
