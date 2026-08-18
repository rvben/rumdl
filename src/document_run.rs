//! Path-aware document analysis shared by CLI, stdin, LSP, and virtual adapters.

use std::borrow::Cow;
use std::path::{Path, PathBuf};

use crate::config::{Config, MarkdownFlavor};
use crate::fix_coordinator::{FixCoordinator, FixResult};
use crate::rule::{LintError, LintWarning, Rule};
use crate::utils::{LineEnding, detect_line_ending_enum, normalize_line_ending};
use crate::workspace_index::FileIndex;

/// The deterministic lint and index result for one document.
pub struct DocumentAnalysis {
    pub warnings: Vec<LintWarning>,
    pub file_index: FileIndex,
}

/// A document run with path roles made explicit.
///
/// `config_path` is a logical identity used by per-file configuration. A
/// separate `source_file` is exposed to rules that may access the filesystem.
/// Native adapters usually set both; browser and other virtual adapters set only
/// `config_path`.
pub struct DocumentRun<'a> {
    content: &'a str,
    rules: &'a [Box<dyn Rule>],
    config: &'a Config,
    config_path: Option<&'a Path>,
    source_file: Option<&'a Path>,
    verbose: bool,
}

impl<'a> DocumentRun<'a> {
    pub fn new(content: &'a str, rules: &'a [Box<dyn Rule>], config: &'a Config) -> Self {
        Self {
            content,
            rules,
            config,
            config_path: None,
            source_file: None,
            verbose: false,
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

    pub fn verbose(mut self, verbose: bool) -> Self {
        self.verbose = verbose;
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
        crate::lint_and_index_with_paths(
            self.content,
            self.rules,
            self.verbose,
            self.flavor(),
            self.paths(),
            Some(self.config),
        )
    }

    /// Apply every fix and return the document in its own line-ending
    /// convention.
    ///
    /// Rules fix LF text: a `fix()` that rebuilds the document joins its lines
    /// with `\n`, and an inserted line ending is `\n`. The CLI normalises a file
    /// to LF before it gets here and restores the ending on write; the same
    /// happens here for every caller (LSP, wasm), so a CRLF document comes back
    /// CRLF, and one with mixed endings comes back LF exactly as `rumdl fmt`
    /// writes it. A document nothing changed comes back byte-identical.
    pub fn fix(&self, max_iterations: usize) -> Result<(String, FixResult), String> {
        let line_ending = detect_line_ending_enum(self.content);
        let normalized = normalize_line_ending(self.content, LineEnding::Lf);
        let mut content = normalized.to_string();
        let result = FixCoordinator::new().apply_fixes_iterative_with_paths(
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
        let restored = match normalize_line_ending(&content, line_ending) {
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
