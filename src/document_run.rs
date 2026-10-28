//! Path-aware document analysis shared by CLI, stdin, LSP, and virtual adapters.

use std::borrow::Cow;
use std::path::{Path, PathBuf};

use crate::config::{Config, MarkdownFlavor};
use crate::fix_coordinator::{FixCoordinator, FixResult};
use crate::rule::{LintError, LintWarning, Rule};
use crate::utils::{LineEnding, NormalizedLineEndingMap, normalize_line_ending};
use crate::workspace_index::FileIndex;

/// The deterministic lint and index result for one document.
/// Fix byte ranges address the supplied text, including any leading BOM.
pub struct DocumentAnalysis {
    pub warnings: Vec<LintWarning>,
    pub file_index: FileIndex,
}

/// Diagnostics and index for the original source, with its prepared rewrite.
/// Keeping the rewrite separate lets adapters finish validation before writing.
pub struct DocumentFormatResult {
    pub warnings: crate::LintResult,
    pub file_index: FileIndex,
    pub fixes: Option<Result<(String, FixResult), String>>,
    /// Findings on the fixed source, when the fix loop reached a stable changed
    /// document and could share its last context. Adapters must discard these
    /// after any further edit and lint that text again.
    pub remaining_warnings: Option<crate::LintResult>,
}

impl DocumentFormatResult {
    pub(crate) fn without_fixes(warnings: crate::LintResult, file_index: FileIndex) -> Self {
        Self {
            warnings,
            file_index,
            fixes: None,
            remaining_warnings: None,
        }
    }
}

/// A document run with path roles made explicit.
///
/// `config_path` is a logical identity used by per-file configuration. A
/// separate `source_file` is exposed to rules that may access the filesystem.
/// Native adapters usually set both; browser and other virtual adapters set only
/// `config_path`.
pub struct DocumentRun<'a> {
    content: &'a str,
    conflict_scan: Option<&'a crate::merge_conflict::ConflictScan<'a>>,
    rules: &'a [Box<dyn Rule>],
    config: &'a Config,
    config_path: Option<&'a Path>,
    source_file: Option<&'a Path>,
    link_target_policy: Option<&'a crate::lint_context::LinkTargetPolicy>,
    invalid_utf8: Option<&'a [crate::encoding::InvalidSeq]>,
    verbose: bool,
    embedded_rules: Option<&'a [Box<dyn Rule>]>,
}

impl<'a> DocumentRun<'a> {
    pub fn new(content: &'a str, rules: &'a [Box<dyn Rule>], config: &'a Config) -> Self {
        Self {
            content,
            conflict_scan: None,
            rules,
            config,
            config_path: None,
            source_file: None,
            link_target_policy: None,
            invalid_utf8: None,
            verbose: false,
            embedded_rules: None,
        }
    }

    /// Reuse detection performed before normalization or external tool work.
    /// The scan supplies its own immutable content, so facts cannot be attached
    /// to a different document or retained after an edit.
    pub fn from_scanned(
        scan: &'a crate::merge_conflict::ConflictScan<'a>,
        rules: &'a [Box<dyn Rule>],
        config: &'a Config,
    ) -> Self {
        Self {
            conflict_scan: Some(scan),
            ..Self::new(scan.content(), rules, config)
        }
    }

    /// Set a real file path for both per-file configuration and rule filesystem access.
    pub fn file_path(mut self, path: &'a Path) -> Self {
        self.config_path = Some(path);
        self.source_file = Some(path);
        self
    }

    /// Set a logical path used only for per-file configuration matching.
    pub fn config_path(mut self, path: Option<&'a Path>) -> Self {
        self.config_path = path;
        self
    }

    /// Set the filesystem path visible to rules independently of configuration matching.
    pub fn source_file(mut self, path: Option<&'a Path>) -> Self {
        self.source_file = path;
        self
    }

    /// Supply a run-scoped view of virtual document paths for link validation.
    pub fn link_target_policy(mut self, policy: &'a crate::lint_context::LinkTargetPolicy) -> Self {
        self.link_target_policy = Some(policy);
        self
    }

    /// Mark the content as lossily decoded from bytes holding these invalid
    /// UTF-8 sequences. MD094 reports them, and no finding carries a fix.
    pub fn invalid_utf8(mut self, invalid: Option<&'a [crate::encoding::InvalidSeq]>) -> Self {
        self.invalid_utf8 = invalid;
        self
    }

    pub fn verbose(mut self, verbose: bool) -> Self {
        self.verbose = verbose;
        self
    }

    /// Include built-in fenced Markdown in this run's suppression accounting.
    /// Adapters decide whether the current command enables this lint phase.
    pub fn embedded_markdown(mut self, rules: &'a [Box<dyn Rule>]) -> Self {
        self.embedded_rules = Some(rules);
        self
    }

    pub fn flavor(&self) -> MarkdownFlavor {
        self.config_path.map_or_else(
            || self.config.markdown_flavor(),
            |path| self.config.get_flavor_for_file(path),
        )
    }

    pub fn analyze(&self) -> Result<DocumentAnalysis, LintError> {
        let (warnings, file_index) = self.analyze_raw();
        warnings.map(|warnings| DocumentAnalysis { warnings, file_index })
    }

    pub fn analyze_raw(&self) -> (Result<Vec<LintWarning>, LintError>, FileIndex) {
        if let Some(scan) = self.conflict_scan {
            let result = crate::analyze_scanned_document(
                scan,
                self.rules,
                self.verbose,
                self.flavor(),
                self.paths(),
                Some(self.config),
                None,
            );
            return (result.warnings, result.file_index);
        }
        crate::lint_and_index_with_paths(
            self.content,
            self.rules,
            self.verbose,
            self.flavor(),
            self.paths(),
            Some(self.config),
        )
    }

    /// Analyze and prepare a fix while the first context and rule checks are
    /// still available. Every rewrite invalidates that analysis; subsequent
    /// iterations build a fresh context as they do in `fix`.
    pub fn analyze_and_fix(&self, max_iterations: usize) -> DocumentFormatResult {
        // The fix engine works on LF input. Preserve the separate operations'
        // diagnostic offsets and line-ending restoration for other input.
        // Virtual link policies also belong to analysis, while `fix` uses the
        // filesystem context; keep that existing distinction for these runs.
        if self.content.contains('\r') || self.link_target_policy.is_some() {
            let (warnings, file_index) = self.analyze_raw();
            let fixes = self.invalid_utf8.is_none() && warnings.as_ref().is_ok_and(|warnings| !warnings.is_empty());
            let fixes = fixes.then(|| self.fix(max_iterations));
            return DocumentFormatResult {
                warnings,
                file_index,
                fixes,
                remaining_warnings: None,
            };
        }
        let scan;
        let scan = match self.conflict_scan {
            Some(scan) => scan,
            None => {
                scan = crate::merge_conflict::ConflictScan::new(self.content);
                &scan
            }
        };
        crate::analyze_scanned_document(
            scan,
            self.rules,
            self.verbose,
            self.flavor(),
            self.paths(),
            Some(self.config),
            Some(max_iterations),
        )
    }

    /// Apply every fix and return the document in its own line-ending
    /// convention.
    ///
    /// Rules fix LF text: a `fix()` that rebuilds the document joins its lines
    /// with `\n`, and an inserted line ending is `\n`. The CLI normalises a file
    /// to LF before it gets here and restores the endings on write; the same
    /// happens here for every caller (LSP, wasm), so the document comes back
    /// with its line endings exactly as `rumdl fmt` writes it (see
    /// `NormalizedLineEndingMap::restore_fixed`). A document nothing changed
    /// comes back byte-identical.
    pub fn fix(&self, max_iterations: usize) -> Result<(String, FixResult), String> {
        self.fix_with_coordinator(max_iterations, &FixCoordinator::new())
    }

    /// Formatter-only execution with the same bytes, iterations, cycle report
    /// and fixed rule names as `fix`. Only `context_creations` may differ.
    /// Analysis consumers retain `fix`.
    pub fn fix_for_formatter(&self, max_iterations: usize) -> Result<(String, FixResult), String> {
        self.fix_with_coordinator(max_iterations, &FixCoordinator::for_formatter())
    }

    fn fix_with_coordinator(
        &self,
        max_iterations: usize,
        coordinator: &FixCoordinator,
    ) -> Result<(String, FixResult), String> {
        let normalized = normalize_line_ending(self.content, LineEnding::Lf);
        let mut content = normalized.to_string();
        let result = coordinator.apply_fixes_iterative_with_paths(
            self.rules,
            &[],
            &mut content,
            self.config,
            max_iterations,
            self.paths(),
        )?;
        if content == *normalized {
            return Ok((self.content.to_string(), result));
        }
        let restored = match NormalizedLineEndingMap::new(self.content).restore_fixed(&normalized, &content) {
            Cow::Borrowed(_) => content,
            Cow::Owned(restored) => restored,
        };
        Ok((restored, result))
    }

    pub fn config_path_buf(&self) -> Option<PathBuf> {
        self.config_path.map(Path::to_path_buf)
    }

    fn paths(&self) -> crate::DocumentPaths<'a> {
        crate::DocumentPaths {
            config_path: self.config_path,
            source_file: self.source_file,
            link_target_policy: self.link_target_policy,
            invalid_utf8: self.invalid_utf8,
            embedded_rules: self.embedded_rules,
            embedded_depth: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::any::Any;

    use indexmap::IndexMap;

    use super::*;
    use crate::lint_context::LintContext;
    use crate::rule::{LintResult, Severity};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[derive(Clone)]
    struct CountingRewrite {
        name: &'static str,
        from: &'static str,
        to: &'static str,
        checks: Arc<AtomicUsize>,
        category: crate::rule::RuleCategory,
    }

    impl Rule for CountingRewrite {
        fn name(&self) -> &'static str {
            self.name
        }
        fn description(&self) -> &'static str {
            "Count source-dependent checks"
        }
        fn check(&self, ctx: &LintContext) -> LintResult {
            self.checks.fetch_add(1, Ordering::Relaxed);
            Ok(ctx
                .content
                .find(self.from)
                .map(|offset| {
                    let (line, column) = ctx.offset_to_line_col(offset);
                    LintWarning {
                        rule_name: Some(self.name.to_string()),
                        message: self.description().to_string(),
                        line,
                        column,
                        end_line: line,
                        end_column: column + self.from.len(),
                        severity: Severity::Warning,
                        fix: None,
                    }
                })
                .into_iter()
                .collect())
        }
        fn fix(&self, ctx: &LintContext) -> Result<String, LintError> {
            Ok(ctx.content.replace(self.from, self.to))
        }
        fn as_any(&self) -> &dyn Any {
            self
        }
        fn category(&self) -> crate::rule::RuleCategory {
            self.category
        }
    }

    fn counting_rule(name: &'static str, from: &'static str, to: &'static str) -> (Box<dyn Rule>, Arc<AtomicUsize>) {
        let checks = Arc::new(AtomicUsize::new(0));
        (
            Box::new(CountingRewrite {
                name,
                from,
                to,
                checks: checks.clone(),
                category: crate::rule::RuleCategory::Other,
            }),
            checks,
        )
    }

    #[test]
    fn combined_run_reuses_the_initial_context_and_checks() {
        let config = Config::default();
        let (rule, checks) = counting_rule("MD009", "text", "text");
        let rules = vec![rule];
        let result = DocumentRun::new("text\n", &rules, &config).analyze_and_fix(100);
        assert_eq!(result.warnings.unwrap().len(), 1);
        let (text, fixed) = result.fixes.unwrap().unwrap();
        assert_eq!(text, "text\n");
        assert_eq!(checks.load(Ordering::Relaxed), 1);
        assert_eq!(fixed.context_creations, 0);
        assert!(fixed.converged);
    }

    #[test]
    fn combined_run_invalidates_all_checks_after_a_rewrite() {
        let config = Config::default();
        let (first, first_checks) = counting_rule("MD009", "bad", "middle");
        let (second, second_checks) = counting_rule("MD011", "middle", "good");
        let rules = vec![first, second];
        let result = DocumentRun::new("bad\n", &rules, &config).analyze_and_fix(100);
        let (text, fixed) = result.fixes.unwrap().unwrap();
        assert_eq!(text, "good\n");
        assert_eq!(first_checks.load(Ordering::Relaxed), 2);
        assert_eq!(second_checks.load(Ordering::Relaxed), 3);
        assert_eq!(fixed.context_creations, 2);
        assert_eq!(fixed.rules_fixed, 2);
        assert!(fixed.converged);
    }

    #[test]
    fn stable_analysis_reuses_checks_and_still_runs_unfixable_rules() {
        let mut config = Config::default();
        config.global.unfixable.push("MD041".into());
        let (rewrite, rewrite_checks) = counting_rule("MD009", "bad", "good");
        let (unchanged, unchanged_checks) = counting_rule("MD011", "stable", "stable");
        let (unfixable, unfixable_checks) = counting_rule("MD041", "other", "other");
        let rules = vec![rewrite, unchanged, unfixable];
        let result = DocumentRun::new("bad stable other\n", &rules, &config).analyze_and_fix(100);
        let (text, fixed) = result.fixes.unwrap().unwrap();
        assert_eq!(text, "good stable other\n");
        assert!(fixed.converged);
        // Both the fix loop and reporting need the final checks, but each
        // source version is checked only once. Warning-only rules still run.
        assert_eq!(rewrite_checks.load(Ordering::Relaxed), 2);
        assert_eq!(unchanged_checks.load(Ordering::Relaxed), 2);
        assert_eq!(unfixable_checks.load(Ordering::Relaxed), 2);
        let remaining = result.remaining_warnings.unwrap().unwrap();
        assert_eq!(remaining.len(), 2);
        assert_eq!(
            format!("{remaining:?}"),
            format!(
                "{:?}",
                DocumentRun::new(&text, &rules, &config).analyze().unwrap().warnings
            ),
        );
    }

    #[test]
    fn remaining_bom_warnings_match_a_fresh_public_analysis() {
        let mut config = Config::default();
        config.global.unfixable.push("MD018".into());
        let rules: Vec<_> = crate::rules::all_rules(&config)
            .into_iter()
            .filter(|r| r.name() == "MD009" || r.name() == "MD018")
            .collect();
        let result = DocumentRun::new("\u{feff}#Title\n\ntext   \n", &rules, &config).analyze_and_fix(100);
        let (text, _) = result.fixes.expect("prepared fix").unwrap();
        assert_eq!(text, "\u{feff}#Title\n\ntext\n");
        let remaining = result.remaining_warnings.expect("final analysis was reused").unwrap();
        assert_eq!(remaining.len(), 1);
        assert!(remaining[0].fix.is_some());
        assert_eq!(remaining[0].fix.as_ref().unwrap().range, 3..9);
        let fresh = DocumentRun::new(&text, &rules, &config).analyze().unwrap().warnings;
        assert_eq!(remaining, fresh);
    }

    #[test]
    fn stable_analysis_preserves_suppression_observers_and_severity() {
        let mut config = Config::default();
        config.rules.insert(
            "MD087".into(),
            crate::config::RuleConfig {
                severity: Some(Severity::Error),
                ..Default::default()
            },
        );
        let (rewrite, _) = counting_rule("MD009", "bad", "good");
        let (suppressed, _) = counting_rule("MD011", "stable", "stable");
        let rules: Vec<Box<dyn Rule>> = vec![rewrite, suppressed, Box::new(crate::rules::MD087UnusedDisableComment)];
        let source =
            "<!-- rumdl-disable-next-line MD011 -->\nstable\n<!-- rumdl-disable-next-line MD009 -->\nquiet\nbad\n";
        let result = DocumentRun::new(source, &rules, &config).analyze_and_fix(100);
        let (text, _) = result.fixes.unwrap().unwrap();
        let remaining = result.remaining_warnings.unwrap().unwrap();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].rule_name.as_deref(), Some("MD087"));
        assert_eq!(remaining[0].line, 3);
        assert_eq!(remaining[0].severity, Severity::Error);
        assert_eq!(
            format!("{remaining:?}"),
            format!(
                "{:?}",
                DocumentRun::new(&text, &rules, &config).analyze().unwrap().warnings
            ),
        );
    }

    #[test]
    fn stable_analysis_distinguishes_zero_sized_rule_instances() {
        let config = Config::default();
        let rules: Vec<Box<dyn Rule>> = vec![
            Box::new(ContextProbe),
            crate::rules::create_rule_by_name("MD018", &config).unwrap(),
        ];
        let result = DocumentRun::new("#Title\n", &rules, &config).analyze_and_fix(100);
        let (text, _) = result.fixes.unwrap().unwrap();
        assert_eq!(text, "# Title\n");
        let remaining = result.remaining_warnings.unwrap().unwrap();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].rule_name.as_deref(), Some("TEST001"));
        assert_eq!(
            format!("{remaining:?}"),
            format!(
                "{:?}",
                DocumentRun::new(&text, &rules, &config).analyze().unwrap().warnings
            ),
        );
    }

    #[test]
    fn unfinished_fix_runs_leave_final_analysis_to_the_caller() {
        let config = Config::default();
        let (first, _) = counting_rule("MD009", "bad", "middle");
        let (second, _) = counting_rule("MD011", "middle", "good");
        let rules = vec![first, second];
        let result = DocumentRun::new("bad\n", &rules, &config).analyze_and_fix(1);
        let (text, fixed) = result.fixes.unwrap().unwrap();
        assert_eq!(text, "middle\n");
        assert!(!fixed.converged);
        assert!(result.remaining_warnings.is_none());
    }

    #[test]
    fn combined_run_keeps_inline_disabled_findings_out_of_fixes() {
        let config = Config::default();
        let (disabled, checks) = counting_rule("MD009", "bad", "good");
        let (warning_only, _) = counting_rule("MD011", "other", "other");
        let rules = vec![disabled, warning_only];
        let source = "<!-- rumdl-disable MD009 -->\nbad other\n";
        let result = DocumentRun::new(source, &rules, &config).analyze_and_fix(100);
        assert_eq!(result.warnings.unwrap().len(), 1);
        let (text, fixed) = result.fixes.unwrap().unwrap();
        assert_eq!(text, source);
        assert_eq!(checks.load(Ordering::Relaxed), 1);
        assert_eq!(fixed.rules_fixed, 0);
    }

    #[test]
    fn combined_run_checks_rules_that_the_lint_prefilter_did_not_run() {
        let config = Config::default();
        let checks = Arc::new(AtomicUsize::new(0));
        let skipped: Box<dyn Rule> = Box::new(CountingRewrite {
            name: "MD001",
            from: "plain",
            to: "changed",
            checks: checks.clone(),
            category: crate::rule::RuleCategory::Heading,
        });
        let (reported, _) = counting_rule("MD009", "warning", "warning");
        let rules = vec![skipped, reported];
        let result = DocumentRun::new("plain warning\n", &rules, &config).analyze_and_fix(100);
        assert_eq!(result.warnings.unwrap().len(), 1);
        let (text, fixed) = result.fixes.unwrap().unwrap();
        assert_eq!(text, "changed warning\n");
        assert_eq!(checks.load(Ordering::Relaxed), 2);
        assert!(fixed.converged);
    }

    #[test]
    fn combined_run_never_prepares_fixes_for_lossy_input() {
        let config = Config::default();
        let (rule, checks) = counting_rule("MD009", "bad", "good");
        let rules = vec![rule];
        let result = DocumentRun::new("bad\n", &rules, &config)
            .invalid_utf8(Some(&[]))
            .analyze_and_fix(100);
        assert!(!result.warnings.unwrap().is_empty());
        assert!(result.fixes.is_none());
        assert_eq!(checks.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn combined_run_matches_separate_analysis_and_fixing() {
        for flavor in crate::format_proof::ALL_FLAVORS {
            let mut config = Config::default();
            config.global.flavor = flavor;
            let rules = crate::rules::all_rules(&config);
            for source in [
                "#Title\n\ntext  \n",
                "\u{feff}#Title\n\ntext  \n",
                "# Title\r\n\r\ntext  \r\n",
                "<!-- rumdl-disable MD018 -->\n#Title\n",
                "<!-- rumdl-configure-file {\"MD013\":{\"line-length\":12}} -->\n\nA rather long line that can be wrapped.\n",
                "- item\n\n```\ncode\n```\n",
            ] {
                let run = DocumentRun::new(source, &rules, &config);
                let original = run.analyze().unwrap();
                let combined = run.analyze_and_fix(100);
                assert_eq!(
                    format!("{:?}", combined.warnings.unwrap()),
                    format!("{:?}", original.warnings)
                );
                if let Some(fixes) = combined.fixes {
                    let text = fixes.unwrap().0;
                    assert_eq!(text, run.fix(100).unwrap().0, "{flavor}: {source:?}");
                    if let Some(remaining) = combined.remaining_warnings {
                        assert_eq!(
                            format!("{:?}", remaining.unwrap()),
                            format!(
                                "{:?}",
                                DocumentRun::new(&text, &rules, &config).analyze().unwrap().warnings
                            ),
                            "{flavor}: {source:?}",
                        );
                    }
                } else {
                    assert_eq!(run.fix(100).unwrap().0, source, "{flavor}: {source:?}");
                }
            }
        }
    }

    #[derive(Clone)]
    struct ContextProbe;

    impl Rule for ContextProbe {
        fn name(&self) -> &'static str {
            "TEST001"
        }

        fn description(&self) -> &'static str {
            "Probe document context"
        }

        fn check(&self, ctx: &LintContext) -> LintResult {
            let message = format!("flavor={};source={}", ctx.flavor, ctx.source_file().is_some());
            Ok(vec![LintWarning {
                message,
                line: 1,
                column: 1,
                end_line: 1,
                end_column: 1,
                severity: Severity::Warning,
                fix: None,
                rule_name: Some(self.name().to_string()),
            }])
        }

        fn fix(&self, ctx: &LintContext) -> Result<String, LintError> {
            Ok(ctx.content.to_string())
        }

        fn as_any(&self) -> &dyn Any {
            self
        }
    }

    #[test]
    fn logical_path_selects_flavor_without_exposing_a_filesystem_path() {
        let mut config = Config::default();
        config.per_file_flavor = IndexMap::from([("docs/**".to_string(), MarkdownFlavor::MkDocs)]);
        let rules: Vec<Box<dyn Rule>> = vec![Box::new(ContextProbe)];
        let path = Path::new("docs/page.md");

        let analysis = DocumentRun::new("text", &rules, &config)
            .config_path(Some(path))
            .analyze()
            .unwrap();

        assert_eq!(analysis.warnings[0].message, "flavor=mkdocs;source=false");
    }

    #[test]
    fn native_file_path_selects_flavor_and_reaches_rules() {
        let mut config = Config::default();
        config.per_file_flavor = IndexMap::from([("docs/**".to_string(), MarkdownFlavor::MkDocs)]);
        let rules: Vec<Box<dyn Rule>> = vec![Box::new(ContextProbe)];
        let path = Path::new("docs/page.md");

        let analysis = DocumentRun::new("text", &rules, &config)
            .file_path(path)
            .analyze()
            .unwrap();

        assert_eq!(analysis.warnings[0].message, "flavor=mkdocs;source=true");
    }
}
