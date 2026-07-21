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
use std::borrow::Cow;
use std::ffi::OsStr;
use std::path::Path;

/// Glob metacharacters recognized when deciding whether an include pattern
/// names files explicitly.
const GLOB_METACHARS: &[char] = &['*', '?', '[', ']', '{', '}'];

/// The file-name glob of an `include` pattern that explicitly names files,
/// if it does.
///
/// A pattern names files explicitly when its final path component pins a
/// literal dotted suffix: a wildcard stem ending in a literal extension
/// chain (`**/*.md.jinja` yields `*.md.jinja`) or a fully literal file name
/// with an extension (`templates/NOTES.tmpl` yields `NOTES.tmpl`). Such
/// patterns widen the lintable-file filter beyond the standard markdown
/// extensions: the user has spelled out exactly which files to process.
///
/// Directory patterns (`docs/`, `docs/**`), bare wildcards (`*`, `**/*`),
/// patterns whose extension itself contains wildcards (`*.md*`,
/// `*.{md,jinja}`), and negations (`!drafts/*.md.jinja`) yield `None`; they
/// express "look here" or "not this", not "this exact kind of file", so the
/// markdown-only filter stays in force for them.
pub fn explicit_file_name_glob(pattern: &str) -> Option<&str> {
    if pattern.starts_with('!') {
        return None;
    }
    let file_name = pattern.rsplit('/').next().unwrap_or(pattern);
    if file_name.is_empty() {
        return None;
    }
    // The literal tail after the last glob metacharacter (the whole
    // component when there is none) must end in a non-empty extension.
    let literal_tail = match file_name.rfind(GLOB_METACHARS) {
        Some(idx) => &file_name[idx + 1..],
        None => file_name,
    };
    match literal_tail.rsplit_once('.') {
        Some((_, ext)) if !ext.is_empty() => Some(file_name),
        _ => None,
    }
}

/// Compiled matchers for the explicitly-named files in a set of config
/// `include` patterns (see [`explicit_file_name_glob`]).
///
/// The CLI walker consults this in two places that otherwise restrict
/// discovery to markdown extensions: the walker's file-type filter and the
/// final lintable-file filter. The type filter can only match file names,
/// so it uses the (over-inclusive) file-name globs; the final filter is
/// the precise gate and matches the full pattern against the root-relative
/// path. Without the path check, a broad sibling pattern like `docs/**`
/// would inherit the non-standard-extension allowance of an explicit
/// pattern like `templates/NOTES.tmpl` for every file sharing its name.
///
/// Path matching follows gitignore anchoring: patterns without a `/` match
/// at any depth, patterns with one are anchored to the root the relative
/// path was computed against. `*` does not cross directory separators.
///
/// Invalid globs are skipped silently; the caller's override handling
/// already warns about unparseable include patterns.
pub struct ExplicitIncludeMatchers {
    matchers: Vec<ExplicitInclude>,
}

struct ExplicitInclude {
    file_name_glob: String,
    path_matcher: GlobMatcher,
}

impl ExplicitIncludeMatchers {
    pub fn new(patterns: &[String]) -> Self {
        let matchers = patterns
            .iter()
            .filter_map(|pattern| {
                let file_name_glob = explicit_file_name_glob(pattern)?;
                let path_glob = if let Some(anchored) = pattern.strip_prefix('/') {
                    anchored.to_string()
                } else if pattern.contains('/') {
                    pattern.clone()
                } else {
                    format!("**/{pattern}")
                };
                let path_matcher = globset::GlobBuilder::new(&path_glob)
                    .literal_separator(true)
                    .build()
                    .ok()?
                    .compile_matcher();
                Some(ExplicitInclude {
                    file_name_glob: file_name_glob.to_string(),
                    path_matcher,
                })
            })
            .collect();
        Self { matchers }
    }

    pub fn is_empty(&self) -> bool {
        self.matchers.is_empty()
    }

    /// The file-name globs, e.g. for registering on a walker type filter.
    pub fn file_name_globs(&self) -> impl Iterator<Item = &str> {
        self.matchers.iter().map(|m| m.file_name_glob.as_str())
    }

    /// Whether the root-relative `path` matches any explicit include
    /// pattern in full.
    pub fn matches_relative_path(&self, path: &str) -> bool {
        self.matchers.iter().any(|m| m.path_matcher.is_match(path))
    }
}

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

/// Normalize path separators to `/` for glob matching. On Windows `\` is
/// globset's escape character, so a native path must be rewritten before it can
/// be used as - or matched against - a pattern. No-op on Unix, where `\` is a
/// legal filename character.
fn normalize_pattern_separators(path: Cow<'_, str>) -> Cow<'_, str> {
    if cfg!(windows) && path.contains('\\') {
        Cow::Owned(path.replace('\\', "/"))
    } else {
        path
    }
}

/// Expands directory-style patterns to also match files within them.
/// Pattern "dir/path" becomes ["dir/path", "dir/path/**"] to match both
/// the directory itself and all contents recursively.
///
/// The expansion is driven by the pattern's *final* component: it names a
/// directory only when it holds no wildcard. `docs/*` therefore stays as
/// written (it names direct children, and `docs/*/**` would newly exclude
/// nested contents), while `**/.cursor/plans` gains its contents-expansion
/// despite the wildcard earlier in the pattern.
pub fn expand_directory_pattern(pattern: &str) -> Vec<String> {
    let base = pattern.trim_end_matches('/');
    let final_component = base.rsplit('/').next().unwrap_or(base);

    if final_component.is_empty() || final_component.contains(['*', '?', '[']) {
        return vec![pattern.to_string()];
    }

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
    /// Whether any pattern is absolute, i.e. whether matching has to consider
    /// a file's absolute path at all. Keeps the common (all-relative) case
    /// from paying for the canonicalization that check needs.
    has_absolute: bool,
    /// Patterns that failed to compile, with their errors. Callers decide
    /// how to surface these (CLI prints to stderr, LSP logs).
    pub invalid: Vec<(String, String)>,
}

/// Whether `pattern` names an absolute location. A leading `/` counts on every
/// platform: patterns use `/` separators, so a Unix-style path stays absolute
/// when the same config is read on Windows.
fn is_absolute_pattern(pattern: &str) -> bool {
    pattern.starts_with('/') || Path::new(pattern).is_absolute()
}

impl ExcludeMatchers {
    pub fn new(patterns: &[String]) -> Self {
        let mut matchers = Vec::new();
        let mut invalid = Vec::new();
        let mut has_absolute = false;
        for pattern in patterns.iter().flat_map(|p| expand_directory_pattern(p)) {
            has_absolute |= is_absolute_pattern(&pattern);
            match Glob::new(&pattern) {
                Ok(glob) => matchers.push((pattern, glob.compile_matcher())),
                Err(e) => invalid.push((pattern, e.to_string())),
            }
        }
        Self {
            matchers,
            has_absolute,
            invalid,
        }
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

    /// The first pattern matching a file, if any.
    ///
    /// Both forms of the file are tried: its `relative` form (how patterns are
    /// normally written - relative to the project or workspace root) and its
    /// absolute path, which is what an absolute pattern matches. Absolute
    /// patterns reach config either written literally or through `~` expansion,
    /// and the walker's overrides cannot apply them (the `ignore` crate anchors
    /// a leading `/` to the walk root), so this is where they take effect.
    ///
    /// Checking the absolute path cannot widen a relative pattern: globs are
    /// anchored at the start of the matched string, so `drafts/**` never
    /// matches `/home/dev/proj/drafts/note.md`.
    ///
    /// `absolute` is canonicalized before matching, since an expanded `~`
    /// resolves to a canonical location. Files that cannot be canonicalized
    /// (already deleted, unreadable) are matched as given.
    pub fn matched_pattern_for_file(&self, relative: Option<&str>, absolute: &Path) -> Option<&str> {
        if let Some(pattern) = relative.and_then(|rel| self.matched_pattern(rel)) {
            return Some(pattern);
        }
        if !self.has_absolute {
            return None;
        }
        let canonical = absolute.canonicalize();
        let absolute = canonical.as_deref().unwrap_or(absolute);
        self.matched_pattern(&normalize_pattern_separators(absolute.to_string_lossy()))
    }

    /// Whether any pattern matches the file (see [`matched_pattern_for_file`](Self::matched_pattern_for_file)).
    pub fn excludes_file(&self, relative: Option<&str>, absolute: &Path) -> bool {
        self.matched_pattern_for_file(relative, absolute).is_some()
    }
}

/// Relativize `path` against `base` for exclude-pattern matching,
/// canonicalizing both sides so symlinks (e.g. macOS `/tmp`) and Windows
/// path-representation differences don't defeat the prefix strip. Returns
/// `None` when `path` is not under `base`.
///
/// Separators are normalized to `/` on Windows, following the project
/// convention for path strings; globset matches either form, but log
/// output and assertions see one canonical shape.
pub fn path_relative_to(path: &Path, base: &Path) -> Option<String> {
    let canonical_base = base.canonicalize().ok()?;
    let canonical_path = path.canonicalize().ok()?;
    canonical_path.strip_prefix(&canonical_base).ok().map(|rel| {
        let rel = rel.to_string_lossy();
        if cfg!(windows) {
            rel.replace('\\', "/")
        } else {
            rel.to_string()
        }
    })
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
                    // Disable gitignore handling so ambient .gitignore files in the
                    // temp directory's ancestry cannot mask the vendor-dir filtering
                    // this test exercises.
                    respect_gitignore: false,
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
    fn explicit_file_name_glob_extracts_literal_extensions() {
        assert_eq!(explicit_file_name_glob("**/*.md.jinja"), Some("*.md.jinja"));
        assert_eq!(explicit_file_name_glob("*.md.jinja"), Some("*.md.jinja"));
        assert_eq!(explicit_file_name_glob("docs/*.txt"), Some("*.txt"));
        assert_eq!(explicit_file_name_glob("templates/NOTES.tmpl"), Some("NOTES.tmpl"));
        assert_eq!(explicit_file_name_glob("*.md"), Some("*.md"));
        assert_eq!(explicit_file_name_glob("a/b/c/*.md.tmpl"), Some("*.md.tmpl"));
    }

    #[test]
    fn explicit_file_name_glob_rejects_unpinned_patterns() {
        for pattern in [
            "docs/",
            "docs/**",
            "docs",
            "*",
            "**",
            "**/*",
            "*.*",
            "*.md*",
            "*.{md,jinja}",
            "*.md?",
            "data.[ch]",
            "!drafts/*.md.jinja",
            "",
            "**/Makefile",
            "*.",
        ] {
            assert_eq!(explicit_file_name_glob(pattern), None, "{pattern:?} should not qualify");
        }
    }

    #[test]
    fn explicit_include_matchers_match_full_relative_paths() {
        let matchers = ExplicitIncludeMatchers::new(&[
            "**/*.md.jinja".to_string(),
            "docs/**".to_string(),
            "templates/NOTES.tmpl".to_string(),
        ]);
        assert!(!matchers.is_empty());
        assert!(matchers.matches_relative_path("test.md.jinja"));
        assert!(matchers.matches_relative_path("a/b/test.md.jinja"));
        assert!(matchers.matches_relative_path("templates/NOTES.tmpl"));
        // The directory pattern must not widen the filter to arbitrary files.
        assert!(!matchers.matches_relative_path("docs/anything.txt"));
        assert!(!matchers.matches_relative_path("test.jinja"));
        // A broad sibling pattern must not inherit the literal pattern's
        // allowance for files that merely share its name.
        assert!(!matchers.matches_relative_path("docs/NOTES.tmpl"));
        assert!(!matchers.matches_relative_path("x/templates/NOTES.tmpl"));

        let globs: Vec<_> = matchers.file_name_globs().collect();
        assert_eq!(globs, vec!["*.md.jinja", "NOTES.tmpl"]);
    }

    #[test]
    fn explicit_include_matchers_follow_gitignore_anchoring() {
        // No slash: matches at any depth.
        let unanchored = ExplicitIncludeMatchers::new(&["*.md.jinja".to_string()]);
        assert!(unanchored.matches_relative_path("test.md.jinja"));
        assert!(unanchored.matches_relative_path("a/b/test.md.jinja"));

        // Slash: anchored to the root, and `*` does not cross separators.
        let anchored = ExplicitIncludeMatchers::new(&["docs/*.txt".to_string()]);
        assert!(anchored.matches_relative_path("docs/a.txt"));
        assert!(!anchored.matches_relative_path("docs/sub/a.txt"));
        assert!(!anchored.matches_relative_path("other/docs/a.txt"));

        // Leading slash: anchored, slash stripped for matching.
        let rooted = ExplicitIncludeMatchers::new(&["/NOTES.tmpl".to_string()]);
        assert!(rooted.matches_relative_path("NOTES.tmpl"));
        assert!(!rooted.matches_relative_path("docs/NOTES.tmpl"));
    }

    #[test]
    fn explicit_include_matchers_empty_for_directory_and_wildcard_patterns() {
        let matchers = ExplicitIncludeMatchers::new(&["docs/".to_string(), "**/*".to_string()]);
        assert!(matchers.is_empty());
        assert!(!matchers.matches_relative_path("x.md.jinja"));
    }

    #[test]
    fn explicit_include_matchers_skip_invalid_globs() {
        // The unclosed bracket pins a literal `.tmpl` suffix but fails glob
        // compilation; it must be skipped without poisoning valid patterns.
        let matchers = ExplicitIncludeMatchers::new(&["bad[.tmpl".to_string(), "**/*.md.jinja".to_string()]);
        assert!(matchers.matches_relative_path("ok.md.jinja"));
        assert_eq!(matchers.file_name_globs().collect::<Vec<_>>(), vec!["*.md.jinja"]);
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
    fn expand_directory_pattern_expands_a_literal_final_component() {
        // A glob earlier in the pattern must not block contents-expansion: the
        // final component names a directory, so its contents are excluded too.
        assert_eq!(
            expand_directory_pattern("**/.cursor/plans"),
            vec!["**/.cursor/plans", "**/.cursor/plans/**"]
        );
        assert_eq!(
            expand_directory_pattern("docs/**/drafts"),
            vec!["docs/**/drafts", "docs/**/drafts/**"]
        );
        // Alternation names literal directories, so it keeps its expansion.
        assert_eq!(
            expand_directory_pattern("logs/{a,b}"),
            vec!["logs/{a,b}", "logs/{a,b}/**"]
        );
    }

    #[test]
    fn expand_directory_pattern_leaves_a_wildcard_final_component_alone() {
        // `docs/*` names direct children only; expanding it to `docs/*/**` would
        // newly exclude nested contents.
        for pattern in ["docs/*", "*.tmp.md", "build/**", "data.[ch]", "notes?"] {
            assert_eq!(
                expand_directory_pattern(pattern),
                vec![pattern.to_string()],
                "{pattern:?} must not gain a contents-expansion"
            );
        }
    }

    #[test]
    fn exclude_matchers_match_an_absolute_pattern_against_an_absolute_path() {
        let matchers = ExcludeMatchers::new(&["/home/dev/.cursor/plans".to_string()]);
        let excluded = Path::new("/home/dev/.cursor/plans/plan.md");
        assert!(
            matchers.excludes_file(None, excluded),
            "an absolute pattern must match the absolute path when there is no relative form"
        );
        assert_eq!(
            matchers.matched_pattern_for_file(None, excluded),
            Some("/home/dev/.cursor/plans/**")
        );
        // A file inside a project root still has a relative form; the absolute
        // pattern must match it through the absolute path.
        assert!(matchers.excludes_file(Some(".cursor/plans/plan.md"), excluded));
        assert!(!matchers.excludes_file(Some("docs/guide.md"), Path::new("/home/dev/docs/guide.md")));
    }

    #[test]
    fn exclude_matchers_do_not_let_relative_patterns_match_absolute_paths() {
        // Relative patterns are anchored at the start of the matched string, so
        // adding the absolute-path check must not widen them into `**/drafts`.
        let matchers = ExcludeMatchers::new(&["drafts".to_string()]);
        assert!(!matchers.excludes_file(None, Path::new("/home/dev/proj/drafts/note.md")));
        assert!(matchers.excludes_file(Some("drafts/note.md"), Path::new("/home/dev/proj/drafts/note.md")));
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
