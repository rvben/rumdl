//! Shared markdown file discovery semantics.
//!
//! The CLI walker (`file_processor::discovery` in the binary crate) and the
//! LSP workspace index scanner answer the same question: which files does
//! rumdl process here? The pieces of that answer that must never diverge
//! live in this module:
//!
//! - the markdown extension set and how it is matched,
//! - how ignore-file handling (`.gitignore`, `.markdownlintignore`, hidden
//!   entries) is configured on a walker,
//! - how `exclude` patterns from config are expanded and matched.
//!
//! Callers still differ deliberately: the LSP skips `.git`/`node_modules`/
//! `target` outright as an editor-performance safety net, while the CLI
//! walks whatever gitignore semantics allow.

use globset::{Glob, GlobMatcher};
use std::ffi::OsStr;
use std::path::Path;

/// File extensions rumdl treats as markdown, lowercase.
pub const MARKDOWN_EXTENSIONS: &[&str] = &["md", "markdown", "mdx", "mkd", "mkdn", "mdown", "mdwn", "qmd", "rmd"];

/// Whether `ext` is a markdown extension. Matches case-insensitively so
/// conventional variants like `Rmd` (and shouting-case `MD`) qualify.
#[inline]
pub fn is_markdown_extension(ext: &OsStr) -> bool {
    ext.to_str()
        .is_some_and(|s| MARKDOWN_EXTENSIONS.iter().any(|known| s.eq_ignore_ascii_case(known)))
}

/// Whether `path` has a markdown extension.
#[inline]
pub fn has_markdown_extension(path: &Path) -> bool {
    path.extension().is_some_and(is_markdown_extension)
}

/// Ignore-handling options applied to a markdown discovery walk.
#[derive(Debug, Clone)]
pub struct MarkdownWalkOptions {
    /// Honor `.gitignore`, `.ignore`, global gitignore, `.git/info/exclude`,
    /// and parent ignore files. Driven by `global.respect_gitignore`.
    pub respect_gitignore: bool,
    /// Skip `.git`, `node_modules`, and `target` directories outright, even
    /// when gitignore handling is disabled or would not cover them.
    pub skip_vendor_dirs: bool,
}

impl Default for MarkdownWalkOptions {
    fn default() -> Self {
        Self {
            respect_gitignore: true,
            skip_vendor_dirs: false,
        }
    }
}

/// Apply the shared ignore-handling configuration to a walker.
///
/// Hidden entries are always walked (a hidden `docs/.pages.md` lints the
/// same as a visible one); generated content is kept out by gitignore
/// semantics and, for callers that opt in, the vendor-directory skip.
/// `.markdownlintignore` is honored for markdownlint compatibility.
pub fn apply_markdown_walk_options(builder: &mut ignore::WalkBuilder, options: &MarkdownWalkOptions) {
    let gitignore = options.respect_gitignore;
    builder
        .ignore(gitignore)
        .git_ignore(gitignore)
        .git_global(gitignore)
        .git_exclude(gitignore)
        .parents(gitignore)
        .hidden(false)
        // Honor ignore files even outside a git repository.
        .require_git(false)
        .add_custom_ignore_filename(".markdownlintignore");

    if options.skip_vendor_dirs {
        builder.filter_entry(|entry| {
            let name = entry.file_name().to_str().unwrap_or("");
            name != ".git" && name != "node_modules" && name != "target"
        });
    }
}

/// Build a walker over `root` configured with the shared options.
pub fn markdown_walk_builder(root: &Path, options: &MarkdownWalkOptions) -> ignore::WalkBuilder {
    let mut builder = ignore::WalkBuilder::new(root);
    apply_markdown_walk_options(&mut builder, options);
    builder
}

/// Expands directory-style patterns to also match files within them.
/// Pattern "dir/path" becomes ["dir/path", "dir/path/**"] to match both
/// the directory itself and all contents recursively.
///
/// Patterns containing glob characters (*, ?, [) are returned unchanged.
pub fn expand_directory_pattern(pattern: &str) -> Vec<String> {
    if pattern.contains('*') || pattern.contains('?') || pattern.contains('[') {
        return vec![pattern.to_string()];
    }

    let base = pattern.trim_end_matches('/');
    vec![
        base.to_string(),     // Match the directory itself
        format!("{base}/**"), // Match everything underneath
    ]
}

/// Compiled `exclude` patterns with directory-pattern expansion applied.
///
/// Match paths through [`matched_pattern`](Self::matched_pattern) using a
/// root-relative path (the CLI relativizes against the project root, the
/// LSP against the containing workspace root) so patterns like
/// `docs/drafts` behave identically everywhere.
pub struct ExcludeMatchers {
    matchers: Vec<(String, GlobMatcher)>,
    /// Patterns that failed to compile, with their errors. Callers decide
    /// how to surface these (CLI prints to stderr, LSP logs).
    pub invalid: Vec<(String, String)>,
}

impl ExcludeMatchers {
    pub fn new(patterns: &[String]) -> Self {
        let mut matchers = Vec::new();
        let mut invalid = Vec::new();
        for pattern in patterns.iter().flat_map(|p| expand_directory_pattern(p)) {
            match Glob::new(&pattern) {
                Ok(glob) => matchers.push((pattern, glob.compile_matcher())),
                Err(e) => invalid.push((pattern, e.to_string())),
            }
        }
        Self { matchers, invalid }
    }

    pub fn is_empty(&self) -> bool {
        self.matchers.is_empty()
    }

    /// The first pattern matching `relative_path`, if any.
    pub fn matched_pattern(&self, relative_path: &str) -> Option<&str> {
        self.matchers
            .iter()
            .find(|(_, matcher)| matcher.is_match(relative_path))
            .map(|(pattern, _)| pattern.as_str())
    }

    pub fn is_match(&self, relative_path: &str) -> bool {
        self.matched_pattern(relative_path).is_some()
    }
}

/// Relativize `path` against `base` for exclude-pattern matching,
/// canonicalizing both sides so symlinks (e.g. macOS `/tmp`) and Windows
/// path-representation differences don't defeat the prefix strip. Returns
/// `None` when `path` is not under `base`.
pub fn path_relative_to(path: &Path, base: &Path) -> Option<String> {
    let canonical_base = base.canonicalize().ok()?;
    let canonical_path = path.canonicalize().ok()?;
    canonical_path
        .strip_prefix(&canonical_base)
        .ok()
        .map(|rel| rel.to_string_lossy().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn markdown_extensions_match_case_insensitively() {
        for ext in ["md", "MD", "Rmd", "rmd", "MarkDown", "qmd", "mdx"] {
            assert!(is_markdown_extension(OsStr::new(ext)), "{ext} should match");
        }
        for ext in ["rs", "txt", "mdq", ""] {
            assert!(!is_markdown_extension(OsStr::new(ext)), "{ext} should not match");
        }
        assert!(has_markdown_extension(Path::new("a/b/README.md")));
        assert!(has_markdown_extension(Path::new("notebook.Rmd")));
        assert!(!has_markdown_extension(Path::new("no_extension")));
        assert!(!has_markdown_extension(Path::new("lib.rs")));
    }

    #[test]
    fn walk_includes_hidden_files() {
        let temp = tempdir().unwrap();
        fs::create_dir_all(temp.path().join(".github")).unwrap();
        fs::write(temp.path().join(".github/PULL_REQUEST_TEMPLATE.md"), "# hi").unwrap();
        fs::write(temp.path().join("README.md"), "# hi").unwrap();

        let files: Vec<_> = markdown_walk_builder(temp.path(), &MarkdownWalkOptions::default())
            .build()
            .flatten()
            .filter(|e| e.file_type().is_some_and(|t| t.is_file()))
            .map(|e| e.path().to_path_buf())
            .collect();
        assert!(files.iter().any(|p| p.ends_with(".github/PULL_REQUEST_TEMPLATE.md")));
        assert!(files.iter().any(|p| p.ends_with("README.md")));
    }

    #[test]
    fn walk_honors_gitignore_when_enabled_only() {
        let temp = tempdir().unwrap();
        fs::write(temp.path().join(".gitignore"), "ignored.md\n").unwrap();
        fs::write(temp.path().join("ignored.md"), "# hi").unwrap();
        fs::write(temp.path().join("kept.md"), "# hi").unwrap();

        let walk = |respect: bool| -> Vec<std::path::PathBuf> {
            markdown_walk_builder(
                temp.path(),
                &MarkdownWalkOptions {
                    respect_gitignore: respect,
                    ..Default::default()
                },
            )
            .build()
            .flatten()
            .filter(|e| e.file_type().is_some_and(|t| t.is_file()))
            .map(|e| e.path().to_path_buf())
            .collect()
        };

        let respected = walk(true);
        assert!(!respected.iter().any(|p| p.ends_with("ignored.md")));
        assert!(respected.iter().any(|p| p.ends_with("kept.md")));

        let unrespected = walk(false);
        assert!(unrespected.iter().any(|p| p.ends_with("ignored.md")));
    }

    #[test]
    fn walk_honors_markdownlintignore() {
        let temp = tempdir().unwrap();
        fs::write(temp.path().join(".markdownlintignore"), "legacy.md\n").unwrap();
        fs::write(temp.path().join("legacy.md"), "# hi").unwrap();
        fs::write(temp.path().join("kept.md"), "# hi").unwrap();

        let files: Vec<_> = markdown_walk_builder(temp.path(), &MarkdownWalkOptions::default())
            .build()
            .flatten()
            .filter(|e| e.file_type().is_some_and(|t| t.is_file()))
            .map(|e| e.path().to_path_buf())
            .collect();
        assert!(!files.iter().any(|p| p.ends_with("legacy.md")));
        assert!(files.iter().any(|p| p.ends_with("kept.md")));
    }

    #[test]
    fn vendor_dirs_skipped_only_when_requested() {
        let temp = tempdir().unwrap();
        for dir in ["node_modules", "target", "src"] {
            fs::create_dir_all(temp.path().join(dir)).unwrap();
            fs::write(temp.path().join(dir).join("doc.md"), "# hi").unwrap();
        }

        let walk = |skip: bool| -> Vec<std::path::PathBuf> {
            markdown_walk_builder(
                temp.path(),
                &MarkdownWalkOptions {
                    skip_vendor_dirs: skip,
                    ..Default::default()
                },
            )
            .build()
            .flatten()
            .filter(|e| e.file_type().is_some_and(|t| t.is_file()))
            .map(|e| e.path().to_path_buf())
            .collect()
        };

        let skipped = walk(true);
        assert!(!skipped.iter().any(|p| p.to_string_lossy().contains("node_modules")));
        assert!(!skipped.iter().any(|p| p.to_string_lossy().contains("target")));
        assert!(skipped.iter().any(|p| p.ends_with("src/doc.md")));

        let unskipped = walk(false);
        assert!(unskipped.iter().any(|p| p.to_string_lossy().contains("node_modules")));
    }

    #[test]
    fn exclude_matchers_expand_directory_patterns() {
        let matchers = ExcludeMatchers::new(&["drafts".to_string(), "*.tmp.md".to_string()]);
        assert!(matchers.is_match("drafts"));
        assert!(
            matchers.is_match("drafts/inner.md"),
            "directory pattern must match contents"
        );
        assert!(matchers.is_match("note.tmp.md"));
        assert!(!matchers.is_match("docs/guide.md"));
        assert_eq!(matchers.matched_pattern("drafts/inner.md"), Some("drafts/**"));
        assert!(matchers.invalid.is_empty());
    }

    #[test]
    fn exclude_matchers_report_invalid_patterns() {
        let matchers = ExcludeMatchers::new(&["[".to_string(), "ok.md".to_string()]);
        assert_eq!(matchers.invalid.len(), 1);
        assert_eq!(matchers.invalid[0].0, "[");
        assert!(matchers.is_match("ok.md"));
    }

    #[test]
    fn path_relative_to_strips_through_symlinked_base() {
        let temp = tempdir().unwrap();
        let base = temp.path().join("base");
        fs::create_dir_all(base.join("docs")).unwrap();
        fs::write(base.join("docs/a.md"), "# hi").unwrap();

        assert_eq!(
            path_relative_to(&base.join("docs/a.md"), &base).as_deref(),
            Some("docs/a.md")
        );
        assert_eq!(
            path_relative_to(&base.join("docs/a.md"), &base.join("docs")).as_deref(),
            Some("a.md")
        );
        assert_eq!(path_relative_to(temp.path(), &base), None, "path outside base");
    }
}
