//! Shared markdown file discovery semantics.
//!
//! The CLI walker (`file_processor::discovery` in the binary crate) and the
//! LSP workspace index scanner answer the same question: which files does
//! rumdl process here? The pieces of that answer that must never diverge
//! live in this module:
//!
//! - the markdown extension set and how it is matched,
//! - the final source-kind gate for each adapter's capabilities,
//! - how ignore-file handling (`.gitignore`, `.markdownlintignore`, hidden
//!   entries) is configured on a walker,
//! - how `exclude` patterns from config are expanded and matched.
//!
//! Callers still differ deliberately: the LSP skips `.git`/`node_modules`/
//! `target` outright as an editor-performance safety net, while the CLI
//! walks whatever gitignore semantics allow.

use globset::{Glob, GlobBuilder, GlobMatcher, GlobSet, GlobSetBuilder};
use std::borrow::Cow;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};

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
                let path_pattern = if let Some(anchored) = pattern.strip_prefix('/') {
                    anchored.to_string()
                } else if pattern.contains('/') {
                    pattern.clone()
                } else {
                    format!("**/{pattern}")
                };
                let path_matcher = path_glob(&path_pattern, true).ok()?.compile_matcher();
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

/// Source kinds an adapter can interpret after a path passes include matching.
///
/// The CLI can extract Markdown from Rust doc comments, while the language
/// server indexes complete Markdown documents and must not parse a Rust source
/// file as if the whole file were Markdown. A CLI `--include` is stronger still:
/// it explicitly asks rumdl to process whatever the pattern selects.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LintableFileMode {
    Markdown,
    MarkdownAndRust,
    Any,
}

/// The shared final gate for files yielded by CLI and LSP discovery walks.
///
/// Include overrides decide *where* to look. This selector decides whether a
/// matching file is a source the adapter can interpret. Explicit config
/// includes can name template-like Markdown files beyond the standard
/// extensions; Rust remains capability-gated even when explicitly named.
pub struct LintablePathSelector {
    base: Option<PathBuf>,
    explicit: ExplicitIncludeMatchers,
    mode: LintableFileMode,
    /// Under [`LintableFileMode::Any`], the include patterns as written. A file
    /// without a Markdown extension is kept only when one of them selects the
    /// file itself, so a pattern naming a directory selects its Markdown.
    named: Option<ignore::overrides::Override>,
}

impl LintablePathSelector {
    pub fn new(base: Option<&Path>, includes: &[String], mode: LintableFileMode) -> Self {
        Self {
            base: base.map(Path::to_path_buf),
            explicit: ExplicitIncludeMatchers::new(includes),
            mode,
            named: None,
        }
    }

    /// Restrict [`LintableFileMode::Any`] to Markdown plus the files `named`
    /// selects directly. `named` is built from the include patterns as written,
    /// without the directory expansion, on the walker's own base.
    #[must_use]
    pub fn with_named_files(mut self, named: ignore::overrides::Override) -> Self {
        self.named = Some(named);
        self
    }

    /// Whether an included path is a source this adapter can interpret.
    pub fn keeps(&self, path: &Path) -> bool {
        if self.mode == LintableFileMode::Any {
            return self
                .named
                .as_ref()
                .is_none_or(|named| has_markdown_extension(path) || named.matched(path, false).is_whitelist());
        }
        if has_markdown_extension(path) {
            return true;
        }

        // Rust doc-comment extraction currently dispatches on lowercase `.rs`.
        // Keep this capability gate identical to the downstream processor.
        let is_rust = path.extension().and_then(OsStr::to_str) == Some("rs");
        if is_rust {
            return self.mode == LintableFileMode::MarkdownAndRust;
        }

        match self.base.as_deref().and_then(|base| path_relative_to(path, base)) {
            Some(relative) => self.explicit.matches_relative_path(&relative),
            // Outside the pattern base only unanchored patterns can still apply;
            // matching the full path covers those.
            None => self.explicit.matches_relative_path(&path.to_string_lossy()),
        }
    }

    /// Whether the walk is left unfiltered by file type.
    pub fn selects_every_file_type(&self) -> bool {
        self.mode == LintableFileMode::Any
    }

    /// Apply the corresponding coarse file-type filter to a discovery walk.
    /// [`Self::keeps`] remains the precise final gate because type filters only
    /// see file names, not root-relative include paths.
    pub fn configure_types(&self, builder: &mut ignore::WalkBuilder) -> Result<(), ignore::Error> {
        if self.mode == LintableFileMode::Any {
            return Ok(());
        }

        let mut types = ignore::types::TypesBuilder::new();
        // The final selector admits exactly MARKDOWN_EXTENSIONS, Rust doc
        // comments and explicit includes. Registering unrelated built-in types
        // cannot add a lintable file, and builds their definitions for no use.
        for extension in MARKDOWN_EXTENSIONS {
            types.add("markdown", &any_case_extension_glob(extension))?;
        }
        types.select("markdown");
        if self.mode == LintableFileMode::MarkdownAndRust {
            types.add("rustdoc", "*.rs")?;
            types.select("rustdoc");
        }
        for glob in self.explicit.file_name_globs() {
            types.add("configinclude", type_filter_glob(glob))?;
        }
        if !self.explicit.is_empty() {
            types.select("configinclude");
        }
        builder.types(types.build()?);
        Ok(())
    }
}

/// The file-name glob to register on the walker's type filter for an explicit
/// include's `glob`.
///
/// The type filter compiles with `globset`'s platform default, which reads `\`
/// as a path separator on Windows, so an escaped name would never be kept. A
/// glob holding a backslash is widened to every name instead: the filter only
/// narrows the walk, and [`LintablePathSelector::keeps`] applies the precise pattern.
fn type_filter_glob(glob: &str) -> &str {
    if glob.contains('\\') { "*" } else { glob }
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

/// How rumdl reads a file, decided by its name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileKind {
    /// Linted as Markdown, the whole file.
    Markdown,
    /// Rust source, linted through its doc comments only.
    RustDocComments,
    /// Source code rumdl cannot read: linting it as Markdown reports on the
    /// program, and fixing it rewrites the program.
    SourceCode,
}

/// Classify `path` by its file name.
///
/// In order: a Markdown final extension is Markdown; a lowercase `.rs` is Rust
/// doc comments; a Markdown extension earlier in the name (`README.md.jinja`, a
/// template of a Markdown file) or Literate CoffeeScript (`.litcoffee`, which is
/// Markdown with code blocks) is Markdown; a final extension of a programming
/// or data language is source code; anything else (`.txt`, no extension, an
/// unknown one) stays Markdown, as rumdl has always treated a file it is
/// pointed at.
pub fn classify_file(path: &Path) -> FileKind {
    if has_markdown_extension(path) {
        return FileKind::Markdown;
    }
    if crate::doc_comment_lint::is_rust_source(path) {
        return FileKind::RustDocComments;
    }
    let Some(extension) = path.extension().and_then(OsStr::to_str) else {
        return FileKind::Markdown;
    };
    if extension.eq_ignore_ascii_case("litcoffee") || has_inner_markdown_extension(path) {
        return FileKind::Markdown;
    }
    if crate::linguist_data::is_source_code_extension(extension) {
        FileKind::SourceCode
    } else {
        FileKind::Markdown
    }
}

/// Whether an extension before the final one is a Markdown extension, as `md`
/// in `README.md.jinja`. Stems follow `Path` semantics, so a leading dot
/// (`.md.swift`) names a hidden file, not an extension.
fn has_inner_markdown_extension(path: &Path) -> bool {
    let mut stem = path.file_stem();
    while let Some(current) = stem {
        let current = Path::new(current);
        if has_markdown_extension(current) {
            return true;
        }
        stem = current.extension().and(current.file_stem());
    }
    false
}

/// A glob selecting `ext` in any letter case, as `*.[mM][dD]` for `md`.
///
/// Walk type globs match case-sensitively, so a plain `*.md` hides `README.MD`
/// from a directory scan even though [`is_markdown_extension`] calls it
/// markdown and naming the file on the command line lints it. Deriving the glob
/// from the same extension keeps the walk's filter from being narrower than the
/// definition it stands in for.
pub fn any_case_extension_glob(ext: &str) -> String {
    let mut glob = String::with_capacity(2 + ext.len() * 4);
    glob.push_str("*.");
    for ch in ext.chars() {
        if ch.is_ascii_alphabetic() {
            glob.push('[');
            glob.push(ch.to_ascii_lowercase());
            glob.push(ch.to_ascii_uppercase());
            glob.push(']');
        } else {
            glob.push(ch);
        }
    }
    glob
}

/// The linter-specific ignore file, read on every directory walk.
pub const MARKDOWNLINTIGNORE: &str = ".markdownlintignore";

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

/// Whether a walk over `roots` stops reading gitignores at the repository root.
///
/// Git reads no `.gitignore` above the repository root, so a walk that does hides
/// files `git check-ignore` reports as visible. Worse, such a file can hide a
/// whole directory, and a pruned directory is never descended into, so no include
/// pattern gets the chance to name anything inside it.
///
/// Outside a repository there is no root to stop at, and ignore files are all a
/// walk has to go on, so there they keep applying upward. One walk has one
/// setting for all of its roots, so the boundary is only applied when every root
/// has a repository to bound it.
pub fn stops_at_repository_root<P: AsRef<Path>>(roots: &[P]) -> bool {
    !roots.is_empty() && roots.iter().all(|root| in_repository(root.as_ref()))
}

/// Whether `path` sits inside a git or jujutsu repository.
///
/// A `.git` entry is a directory in an ordinary clone and a file in a worktree or
/// submodule, so existence alone is the marker. This recognizes a repository the
/// same way the walker does, which is what puts the boundary in the same place.
fn in_repository(path: &Path) -> bool {
    let Ok(absolute) = std::fs::canonicalize(path) else {
        return false;
    };
    absolute
        .ancestors()
        .any(|dir| dir.join(".git").exists() || dir.join(".jj").exists())
}

/// Apply the shared ignore-handling configuration to a walker over `roots`.
///
/// Hidden entries are always walked (a hidden `docs/.pages.md` lints the
/// same as a visible one); generated content is kept out by gitignore
/// semantics and, for callers that opt in, the vendor-directory skip.
/// `.markdownlintignore` is honored for markdownlint compatibility, whatever
/// `respect_gitignore` says: markdownlint-cli applies it independently of any
/// gitignore handling, and it is a list written for the linter alone, so
/// turning off git's ignore files is not a request to lint what it names.
///
/// The roots decide where gitignore reading stops, so a caller passes the same
/// ones it walks.
pub fn apply_markdown_walk_options<P: AsRef<Path>>(
    builder: &mut ignore::WalkBuilder,
    roots: &[P],
    options: &MarkdownWalkOptions,
) {
    apply_walk_options_with_markdownlintignore(builder, roots, options, true);
}

/// [`apply_markdown_walk_options`], with `.markdownlintignore` switchable.
///
/// Only an explanation of an empty run turns it off, to find out which ignore
/// source hid a file. A walk deciding what to lint always reads it.
pub fn apply_walk_options_with_markdownlintignore<P: AsRef<Path>>(
    builder: &mut ignore::WalkBuilder,
    roots: &[P],
    options: &MarkdownWalkOptions,
    markdownlintignore: bool,
) {
    let gitignore = options.respect_gitignore;
    builder
        .ignore(gitignore)
        .git_ignore(gitignore)
        .git_global(gitignore)
        .git_exclude(gitignore)
        // Parent directories are read for every walk: each source toggle above
        // still decides what they contribute, so with the gitignore family off
        // a parent `.markdownlintignore` is the only file read from them.
        .parents(true)
        .hidden(false)
        // This setting does double duty in the walker: it gates gitignore
        // handling on a repository being present, and it is what stops the walk
        // reading gitignores above the repository root. Inside a repository both
        // are wanted. Outside one, requiring a repository would drop `.gitignore`
        // handling entirely, and there is no root to stop at in any case.
        .require_git(stops_at_repository_root(roots));
    if markdownlintignore {
        builder.add_custom_ignore_filename(MARKDOWNLINTIGNORE);
    }

    if options.skip_vendor_dirs {
        let roots: Vec<PathBuf> = roots.iter().map(|root| root.as_ref().to_path_buf()).collect();
        builder.filter_entry(move |entry| {
            if roots.iter().any(|root| root == entry.path()) {
                return true;
            }
            let name = entry.file_name().to_str().unwrap_or("");
            name != ".git" && name != "node_modules" && name != "target"
        });
    }
}

/// Directories that hold a version control system's internal state.
const VCS_DIRECTORIES: &[&str] = &[".git", ".hg", ".svn", ".jj"];

/// Keep a walk out of the version control directories beneath its roots.
///
/// A walk restricted to Markdown by the file type filter reaches only the
/// Markdown a repository happens to hold there. A walk that selects every file
/// type (`--include '*'`) would lint the repository's own config and hook files,
/// and `rumdl fmt` would rewrite them. A root the caller names is still entered:
/// the walker does not apply its entry filter to a root. Replaces any other
/// entry filter on `builder`.
pub fn skip_version_control_directories(builder: &mut ignore::WalkBuilder) {
    builder.filter_entry(|entry| {
        let is_dir = entry.file_type().is_some_and(|file_type| file_type.is_dir());
        !(is_dir && VCS_DIRECTORIES.contains(&entry.file_name().to_str().unwrap_or("")))
    });
}

/// Build a walker over `root` configured with the shared options.
pub fn markdown_walk_builder(root: &Path, options: &MarkdownWalkOptions) -> ignore::WalkBuilder {
    let mut builder = ignore::WalkBuilder::new(root);
    apply_markdown_walk_options(&mut builder, &[root], options);
    builder
}

/// A complete, configured Markdown workspace scan.
///
/// This owns the selection policy shared by full scans and incremental file
/// events: standard Markdown extensions, explicit nonstandard file includes,
/// include filtering, excludes, ignore files, and optional vendor-directory
/// pruning. Adapters choose the options; they do not reconstruct the policy.
pub struct MarkdownWorkspaceScan<'a> {
    options: &'a MarkdownWalkOptions,
    includes: &'a [String],
    excludes: &'a ExcludeMatchers,
}

impl<'a> MarkdownWorkspaceScan<'a> {
    pub fn new(options: &'a MarkdownWalkOptions, includes: &'a [String], excludes: &'a ExcludeMatchers) -> Self {
        Self {
            options,
            includes,
            excludes,
        }
    }

    /// Collect all selected files under `roots`.
    pub fn collect(&self, roots: &[PathBuf]) -> Vec<PathBuf> {
        let mut files = Vec::new();
        for root in roots {
            let selection = RootSelection::new(root, self.includes);
            let mut builder = markdown_walk_builder(root, self.options);
            selection.configure_walk(&mut builder);

            for result in builder.build() {
                match result {
                    Ok(entry)
                        if entry.file_type().is_some_and(|file_type| file_type.is_file())
                            && selection.is_lintable(entry.path())
                            && !self.excluded(root, entry.path()) =>
                    {
                        files.push(entry.into_path());
                    }
                    Ok(_) => {}
                    Err(error) => log::warn!("Error scanning {}: {error}", root.display()),
                }
            }
        }
        files.sort();
        files.dedup();
        files
    }

    /// Whether an incremental file event would be absent from a full scan.
    pub fn path_is_ignored(&self, roots: &[PathBuf], path: &Path) -> bool {
        let Some(root) = roots
            .iter()
            .filter(|root| path.starts_with(root))
            .max_by_key(|root| root.components().count())
        else {
            return false;
        };

        let selection = RootSelection::new(root, self.includes);
        if !selection.selects(path) || self.excluded(root, path) {
            return true;
        }

        if self.options.skip_vendor_dirs
            && let Ok(relative) = path.strip_prefix(root)
            && relative.components().any(|component| {
                matches!(component, std::path::Component::Normal(name) if name == ".git" || name == "node_modules" || name == "target")
            })
        {
            return true;
        }

        let target = path.to_path_buf();
        let mut builder = markdown_walk_builder(root, self.options);
        selection.configure_walk(&mut builder);
        // `filter_entry` replaces the vendor filter, which was checked above.
        builder.filter_entry(move |entry| target.starts_with(entry.path()));
        !builder.build().flatten().any(|entry| entry.path() == path)
    }

    fn excluded(&self, root: &Path, path: &Path) -> bool {
        self.excludes
            .excludes_file(path_relative_to(path, root).as_deref(), path)
    }
}

struct RootSelection {
    lintable: LintablePathSelector,
    overrides: Option<ignore::overrides::Override>,
}

impl RootSelection {
    fn new(root: &Path, includes: &[String]) -> Self {
        let normalized: Vec<String> = includes
            .iter()
            .map(|pattern| normalize_pattern_for_base(pattern, Some(root)))
            .collect();
        let overrides = if normalized.is_empty() {
            None
        } else {
            let mut builder = ignore::overrides::OverrideBuilder::new(root);
            for pattern in &normalized {
                for rule in expand_include_directory(pattern) {
                    if let Err(error) = builder.add(&rule) {
                        log::warn!("Invalid include pattern '{pattern}': {error}");
                        break;
                    }
                }
            }
            builder.build().ok()
        };
        Self {
            lintable: LintablePathSelector::new(Some(root), &normalized, LintableFileMode::Markdown),
            overrides,
        }
    }

    fn configure_walk(&self, builder: &mut ignore::WalkBuilder) {
        if let Err(error) = self.lintable.configure_types(builder) {
            log::warn!("Failed to configure workspace source types: {error}");
        }
        if let Some(overrides) = &self.overrides {
            builder.overrides(overrides.clone());
        }
    }

    fn selects(&self, path: &Path) -> bool {
        self.overrides
            .as_ref()
            .is_none_or(|overrides| overrides.matched(path, false).is_whitelist())
            && self.is_lintable(path)
    }

    fn is_lintable(&self, path: &Path) -> bool {
        self.lintable.keeps(path)
    }
}

/// Drop Windows' verbatim `\\?\` prefix from a canonicalized path string.
///
/// `std::fs::canonicalize` returns the verbatim form (`\\?\C:\Users\dev`) on
/// Windows. That form is useless for pattern matching: it does not compare
/// equal to the ordinary paths rumdl works with, and normalizing its
/// separators for globbing mangles it into `//?/C:/Users/dev`, which matches
/// nothing. Only a drive path (`\\?\C:\...`) and a UNC share
/// (`\\?\UNC\server\share` -> `\\server\share`) are unwrapped; any other
/// verbatim path names a device namespace that has no ordinary equivalent, so
/// it is left alone.
///
/// The prefix is recognized in whichever separator the path is written with:
/// `\\?\` as `canonicalize` returns it, or `//?/` once the separators have
/// been normalized. Displayed paths are stripped by the CLI's display layer;
/// output formatters strip again because a path can reach them unchanged.
///
/// Pure string logic, compiled on every platform so it stays under test where
/// Windows is not available. On other platforms no path ever carries this
/// prefix.
pub fn strip_verbatim_prefix(path: &str) -> Cow<'_, str> {
    let Some(sep) = path.chars().next().filter(|c| matches!(c, '\\' | '/')) else {
        return Cow::Borrowed(path);
    };
    let Some(rest) = path.strip_prefix(&format!("{sep}{sep}?{sep}")) else {
        return Cow::Borrowed(path);
    };
    // `\\?\UNC\server\share` -> `\\server\share`. The remainder already starts
    // with one separator, so restoring the UNC form needs one more prepended.
    if let Some(share) = rest.strip_prefix("UNC")
        && share.starts_with(sep)
    {
        return Cow::Owned(format!("{sep}{share}"));
    }
    let is_drive_path = rest.as_bytes().get(1) == Some(&b':');
    if is_drive_path {
        Cow::Borrowed(rest)
    } else {
        Cow::Borrowed(path)
    }
}

/// Canonicalize `path` for pattern matching, or `None` when it cannot be
/// resolved (a missing or unreadable file).
///
/// Canonical form is what patterns are matched against, so a symlinked
/// location (`/home/dev` -> `/mnt/dev`, or a macOS `/var` -> `/private/var`)
/// still matches. Windows' verbatim prefix is removed (see
/// [`strip_verbatim_prefix`]).
pub fn canonicalize_for_matching(path: &Path) -> Option<PathBuf> {
    let canonical = path.canonicalize().ok()?;
    if !cfg!(windows) {
        return Some(canonical);
    }
    let as_str = canonical.to_string_lossy();
    Some(PathBuf::from(strip_verbatim_prefix(&as_str).as_ref()))
}

/// Resolve `path` to the absolute form patterns are matched against, whether or
/// not anything exists there.
///
/// A path that exists resolves exactly as [`canonicalize_for_matching`] resolves
/// it. One that does not (an unsaved editor buffer, a file not written yet) is
/// resolved through its deepest existing ancestor: that ancestor is
/// canonicalized and the missing remainder appended, with its `.` and `..`
/// resolved lexically, so the file is spelled the way it will canonicalize once
/// it exists. A relative path is taken relative to the working directory, which
/// is how the filesystem reads it.
pub fn resolve_for_matching(path: &Path) -> PathBuf {
    if let Some(canonical) = canonicalize_for_matching(path) {
        return canonical;
    }
    let absolute = std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
    let resolved = resolve_through_existing_ancestor(&absolute);
    // A `..` in the missing remainder can climb back into directories that
    // exist, and one of those may be a symlink. The lexical result holds no
    // `..` any more, so resolving it once more settles it.
    if absolute.components().any(|c| c == std::path::Component::ParentDir) {
        resolve_through_existing_ancestor(&resolved)
    } else {
        resolved
    }
}

/// Canonicalize the deepest ancestor of `absolute` that exists and append the
/// rest of the path to it lexically (see [`resolve_for_matching`]).
fn resolve_through_existing_ancestor(absolute: &Path) -> PathBuf {
    for ancestor in absolute.ancestors() {
        if let Some(canonical) = canonicalize_for_matching(ancestor) {
            let remainder = absolute.strip_prefix(ancestor).unwrap_or(Path::new(""));
            if remainder.as_os_str().is_empty() {
                return canonical;
            }
            return crate::workspace_index::normalize_relative_path(&canonical.join(remainder));
        }
    }
    crate::workspace_index::normalize_relative_path(absolute)
}

/// The user's home directory, or `None` when it cannot be resolved.
///
/// Canonicalized for matching (see [`canonicalize_for_matching`]), falling
/// back to the path as reported when it cannot be canonicalized.
///
/// Wasm and WASI builds have no home directory to resolve, so patterns keep
/// their `~` there (see [`expand_home_prefix`]).
fn home_dir() -> Option<PathBuf> {
    crate::config::user_paths::process_home_path().map(|home| canonicalize_for_matching(&home).unwrap_or(home))
}

/// Expand a leading `~` in a path pattern to the user's home directory, so a
/// user-level config (`~/.config/rumdl/rumdl.toml`) can name a home path
/// without hardcoding a username.
///
/// Only a bare `~` and a `~/` prefix expand. `~` is a legal filename character
/// everywhere else (editor backups like `notes.md~`, a literal `docs/~drafts`),
/// so it is left alone there. `~user` is not expanded either: resolving another
/// user's home needs the password database, and treating it as the current
/// user's home would silently match the wrong directory.
///
/// The expansion is a glob pattern, so separators are normalized to `/` on
/// Windows: `\` is globset's escape character, and matched paths are normalized
/// the same way (see [`path_relative_to`]).
pub fn expand_home_prefix(pattern: &str) -> Cow<'_, str> {
    // Resolve the home directory only for a pattern that references it: every
    // other pattern would otherwise pay for the lookup and its canonicalization.
    if !has_home_prefix(pattern) {
        return Cow::Borrowed(pattern);
    }
    expand_home_prefix_impl(pattern, home_dir().as_deref())
}

/// Whether `pattern` starts with a home reference (`~` or `~/`).
fn has_home_prefix(pattern: &str) -> bool {
    pattern == "~" || pattern.starts_with("~/")
}

fn expand_home_prefix_impl<'a>(pattern: &'a str, home: Option<&Path>) -> Cow<'a, str> {
    let Some(suffix) = (if pattern == "~" {
        Some("")
    } else {
        pattern.strip_prefix("~/")
    }) else {
        return Cow::Borrowed(pattern);
    };
    let Some(home) = home else {
        return Cow::Borrowed(pattern);
    };

    let home = normalize_pattern_separators(home.to_string_lossy());
    let home = home.trim_end_matches('/');
    if suffix.is_empty() {
        Cow::Owned(home.to_string())
    } else {
        Cow::Owned(format!("{home}/{suffix}"))
    }
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

/// `pattern` as written, read for the platform and with, when `home`, a leading
/// `~` expanded. On Windows a `\` is a path separator and becomes `/`, so no
/// escape survives there; elsewhere it stays the escape. The text still holds
/// every `.` and `..` component.
fn spell_pattern(pattern: &str, home: bool, windows: bool) -> Cow<'_, str> {
    let separated = if windows && pattern.contains('\\') {
        Cow::Owned(pattern.replace('\\', "/"))
    } else {
        Cow::Borrowed(pattern)
    };
    // Canonical Windows paths use a device prefix whose `?` is not a glob.
    let separated = if windows {
        match strip_verbatim_prefix(&separated) {
            Cow::Owned(path) => Cow::Owned(path),
            Cow::Borrowed(path) if path.len() != separated.len() => Cow::Owned(path.to_string()),
            Cow::Borrowed(_) => separated,
        }
    } else {
        separated
    };
    if !home {
        return separated;
    }
    match expand_home_prefix(&separated) {
        Cow::Owned(expanded) => Cow::Owned(expanded),
        Cow::Borrowed(_) => separated,
    }
}

/// A pattern as every glob consumer compiles it: separators read for the
/// platform, `~` expanded, and `.`/`..` components resolved (see
/// `normalize_dot_segments`).
pub fn prepare_pattern(pattern: &str) -> Cow<'_, str> {
    prepare_pattern_for(pattern, cfg!(windows))
}

/// [`prepare_pattern`] for an explicit platform, so both readings of `\` are
/// testable on any host.
pub fn prepare_pattern_for(pattern: &str, windows: bool) -> Cow<'_, str> {
    let spelled = spell_pattern(pattern, true, windows);
    match normalize_dot_segments(&spelled, windows) {
        Cow::Owned(normalized) => Cow::Owned(normalized),
        Cow::Borrowed(_) => spelled,
    }
}

/// Whether a pattern component names exactly itself: no glob metacharacter and
/// no escape.
fn is_plain_component(component: &str) -> bool {
    !component.contains(GLOB_METACHARS) && !component.contains('\\')
}

/// The components of `pattern` between its top-level `/`, or `None` when its
/// braces or character class are unbalanced.
///
/// A `/` inside a `[...]` class, inside `{...}` braces or after a `\` is part of
/// its component. A component holding a brace is opaque to the callers.
fn split_components(pattern: &str) -> Option<Vec<&str>> {
    let bytes = pattern.as_bytes();
    let mut parts = Vec::new();
    let mut start = 0;
    let mut depth = 0usize;
    let mut in_class = false;
    // The first `]` of a class (after an optional `!` or `^`) is a member.
    let mut class_start = 0;
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'\\' if !in_class => index += 1,
            b'[' if !in_class => {
                in_class = true;
                class_start = index + 1;
                if matches!(bytes.get(class_start), Some(b'!' | b'^')) {
                    class_start += 1;
                }
            }
            b']' if in_class && index == class_start => {}
            b']' if in_class => in_class = false,
            _ if in_class => {}
            b'{' => depth += 1,
            b'}' if depth > 0 => depth -= 1,
            b'/' if depth == 0 => {
                parts.push(&pattern[start..index]);
                start = index + 1;
            }
            _ => {}
        }
        index += 1;
    }
    if in_class || depth != 0 {
        return None;
    }
    parts.push(&pattern[start..]);
    Some(parts)
}

/// Resolve the `.` and `..` components of a path pattern lexically, so
/// `./docs`, `docs/./x` and `a/../docs` name the same files as `docs`.
///
/// - A `.` component and an interior empty one (`a//b`) are dropped. A trailing
///   `/` is kept: it marks a directory.
/// - A `..` removes the plain literal component before it. In an absolute
///   pattern whose earlier components all exist on disk the `..` is kept
///   instead, because the filesystem resolves it (`link/..` is the parent of
///   the link's target, which [`PathAliases`] honours). A prefix that does not
///   exist is resolved lexically. `/..` is `/`.
/// - A `..` that cannot be resolved stays as written: one that leads a relative
///   pattern, follows a wildcard or an escape, or sits in an absolute pattern
///   after any component that is not a plain literal (so existence cannot be
///   checked before the glob is expanded). Such a pattern matches nothing by
///   itself, as it always has.
/// - Roots are never removed: the leading `/` (a run of them collapses to one,
///   except a UNC `//server/share` on Windows) and, on Windows, a drive
///   (`C:`) and the `server`/`share` of a UNC path.
///
/// A relative pattern that would end up empty (`.`, `./`, `a/..`) is left as
/// written. The result borrows `pattern` when nothing changed.
fn normalize_dot_segments(pattern: &str, windows: bool) -> Cow<'_, str> {
    if !(pattern.contains("//") || pattern.split('/').any(|part| part == "." || part == "..")) {
        return Cow::Borrowed(pattern);
    }
    let Some(parts) = split_components(pattern) else {
        return Cow::Borrowed(pattern);
    };
    let count = parts.len();
    let leading_empty = parts.iter().take_while(|part| part.is_empty()).count();
    let mut index = 0;
    let mut lead = "";
    let mut absolute = false;
    let mut kept: Vec<&str> = Vec::new();
    // Components at the front of `kept` that a `..` may not remove.
    let mut protected = 0;
    if leading_empty > 0 {
        absolute = true;
        let slashes = if leading_empty == count {
            count - 1
        } else {
            leading_empty
        };
        index = leading_empty.min(count);
        lead = if windows && slashes >= 2 { "//" } else { "/" };
        if lead == "//" {
            // The `server`/`share` of a UNC path, or `?`/`C:` of a verbatim one.
            for part in parts.iter().skip(index).take(2).filter(|part| !part.is_empty()) {
                kept.push(part);
                index += 1;
                protected += 1;
            }
        }
    } else if windows
        && let [letter, b':'] = parts[0].as_bytes()
        && letter.is_ascii_alphabetic()
    {
        absolute = true;
        kept.push(parts[0]);
        protected = 1;
        index = 1;
    }

    let mut opaque = false;
    let mut trailing = false;
    for (position, &part) in parts.iter().enumerate().skip(index) {
        let last = position + 1 == count;
        match part {
            "" => trailing |= last,
            "." => {}
            ".." => {
                let removable = kept.len() > protected
                    && kept
                        .last()
                        .is_some_and(|prev| *prev != ".." && is_plain_component(prev));
                if removable {
                    if !absolute {
                        kept.pop();
                    } else if opaque {
                        kept.push("..");
                    } else {
                        let prefix = format!("{lead}{}", kept.join("/"));
                        if Path::new(&prefix).exists() {
                            kept.push("..");
                        } else {
                            kept.pop();
                        }
                    }
                } else if !(absolute && kept.len() == protected) {
                    kept.push("..");
                }
            }
            _ => {
                opaque |= !is_plain_component(part);
                kept.push(part);
            }
        }
    }

    if kept.is_empty() && !absolute {
        return Cow::Borrowed(pattern);
    }
    let mut normalized = format!("{lead}{}", kept.join("/"));
    // A bare drive (`C:`) is drive-relative: keep the `/` that makes it the root.
    let bare_drive = lead.is_empty() && absolute && protected == 1 && kept.len() == 1;
    if (trailing && !kept.is_empty()) || bare_drive {
        normalized.push('/');
    }
    if normalized == pattern {
        Cow::Borrowed(pattern)
    } else {
        Cow::Owned(normalized)
    }
}

/// `normalized` with a leading `/` when removing `.` components left it with no
/// `/` of its own.
///
/// A relative pattern with a `/` is anchored to the root, one without floats to
/// any depth. `./dir` only has its `/` through the `./`, so it must keep naming
/// the root's `dir` and not start to float. `spelled` is the pattern before
/// normalization. `ignore` reads a leading `/` as the anchor.
fn keep_anchored(spelled: &str, normalized: String) -> String {
    let floated = !trim_directory_slash(spelled).contains('/');
    let directory = trim_directory_slash(&normalized);
    if !floated && !directory.is_empty() && !directory.contains('/') {
        format!("/{normalized}")
    } else {
        normalized
    }
}

/// Normalize a config path pattern for matching against paths discovered under
/// `base`: expand a leading `~`, then rewrite an absolute pattern as one
/// relative to `base` when `base` contains it.
///
/// The rewrite is what makes an absolute pattern usable as a walker override:
/// the `ignore` crate reads a leading `/` as "anchored to the walk base", so
/// `/home/dev/docs/**` would otherwise be understood as
/// `<base>/home/dev/docs/**` and match nothing. A pattern pointing outside
/// `base` is left absolute - nothing under this walk can match it, which is the
/// correct outcome.
///
/// A pattern can also name `base`'s location through a symlink
/// (`/var/folders/…` for a base at `/private/var/folders/…`), which no strip of
/// `base` in either form removes. Its leading literal components are then
/// resolved, giving the same location in the base's own spelling. Only that
/// prefix is rewritten and the strip consumes it, so what survives is the
/// pattern as written. A pattern whose *first* component holds a wildcard or a
/// brace alternation has no such prefix and stays absolute.
pub fn normalize_pattern_for_base(pattern: &str, base: Option<&Path>) -> String {
    normalize_pattern_for_base_for(pattern, base, cfg!(windows))
}

/// [`normalize_pattern_for_base`] for an explicit platform (see
/// [`prepare_pattern_for`]).
///
/// `.` and `..` components are resolved first (see `normalize_dot_segments`).
/// A leading `!` is an `ignore` override negation, not part of the path: it is
/// set aside while the operand is prepared and put back. The pattern is left
/// with its dots where resolving them would turn a positive include into a
/// comment (`#`) or a negation (`!`) line.
pub fn normalize_pattern_for_base_for(pattern: &str, base: Option<&Path>, windows: bool) -> String {
    let (negated, operand) = match pattern.strip_prefix('!') {
        Some(operand) => (true, operand),
        None => (false, pattern),
    };
    let spelled = spell_pattern(operand, !negated, windows);
    let normalized = match normalize_dot_segments(&spelled, windows) {
        Cow::Owned(normalized) if negated || !(spelled.starts_with('#') || normalized.starts_with(['#', '!'])) => {
            normalized
        }
        _ => spelled.to_string(),
    };
    if negated {
        return format!("!{}", keep_anchored(&spelled, normalized));
    }
    let Some(base) = base else {
        return keep_anchored(&spelled, normalized);
    };
    if !is_absolute_pattern(&normalized) {
        return keep_anchored(&spelled, normalized);
    }
    let prepared = normalized;

    if let Some(relative) = strip_base_prefix(&prepared, base) {
        return anchor_remainder(resolve_remainder_dots(relative, base).unwrap_or(prepared));
    }
    if let Some(canonical_pattern) = canonicalize_pattern_prefix(&prepared)
        && let Some(relative) = strip_base_prefix(&canonical_pattern, base)
    {
        return anchor_remainder(resolve_remainder_dots(relative, base).unwrap_or(prepared));
    }
    prepared
}

/// `remainder` with a leading `/` when it is a single component.
///
/// A pattern written as an absolute path names one location. Once the base is
/// stripped, a remainder without a `/` (`<base>/top.md` -> `top.md`) would float
/// to every depth like a bare relative name, so it keeps the root anchor `ignore`
/// reads as a leading `/`. A remainder with an interior `/` is anchored already,
/// and an unstripped absolute pattern keeps its own leading `/`.
fn anchor_remainder(remainder: String) -> String {
    let directory = trim_directory_slash(&remainder);
    if directory.is_empty() || directory.contains('/') {
        remainder
    } else {
        format!("/{remainder}")
    }
}

/// The pattern left after `base` was stripped, with any `..` that was kept for
/// the filesystem resolved against `base` itself, or `None` when that leaves the
/// base.
///
/// An absolute pattern keeps a `..` after an existing directory, and one under a
/// base whose own name holds a metacharacter (`/tmp/project[1]`) keeps every
/// `..`. Once the base is stripped the rest is relative to a location that is
/// known, so each such `..` is resolved there: the longest existing leading part
/// is canonicalized (symlinks honoured), the missing remainder is resolved
/// lexically, and the parent is expressed relative to the canonical base.
fn resolve_remainder_dots(remainder: &str, base: &Path) -> Option<String> {
    if !remainder.split('/').any(|part| part == "..") {
        return Some(remainder.to_string());
    }
    let parts = split_components(remainder)?;
    let canonical_base = canonicalize_for_matching(base)?;
    let mut kept: Vec<String> = Vec::new();
    let mut opaque = false;
    let mut trailing = false;
    for (position, part) in parts.iter().enumerate() {
        match *part {
            "" => trailing |= position + 1 == parts.len(),
            "." => {}
            ".." => {
                let removable = !opaque && kept.last().is_some_and(|prev| prev != ".." && is_plain_component(prev));
                if removable {
                    kept = parent_relative_to(&canonical_base, &kept)?;
                } else if kept.is_empty() && !opaque {
                    // Above the base: nothing under this walk can match it.
                    return None;
                } else {
                    kept.push("..".to_string());
                }
            }
            _ => {
                opaque |= !is_plain_component(part);
                kept.push((*part).to_string());
            }
        }
    }
    let mut resolved = kept.join("/");
    if trailing && !kept.is_empty() {
        resolved.push('/');
    }
    Some(resolved)
}

/// The parent of `canonical_base/kept...` as components relative to
/// `canonical_base`, or `None` when it leaves the base or a component is not a
/// plain name.
fn parent_relative_to(canonical_base: &Path, kept: &[String]) -> Option<Vec<String>> {
    let full = kept
        .iter()
        .fold(canonical_base.to_path_buf(), |path, part| path.join(part));
    let resolved = full.ancestors().find_map(|ancestor| {
        let canonical = canonicalize_for_matching(ancestor)?;
        let rest = full.strip_prefix(ancestor).ok()?;
        Some(canonical.join(rest))
    })?;
    let relative = resolved.parent()?.strip_prefix(canonical_base).ok()?;
    relative
        .components()
        .map(|component| {
            let name = component.as_os_str().to_str()?;
            is_plain_component(name).then(|| name.to_string())
        })
        .collect()
}

/// `pattern` with `base` removed, trying the base as given and canonicalized so
/// a symlinked or non-canonical base (macOS `/var`, a Windows 8.3 short name)
/// still strips. `None` when the pattern does not live under `base`.
fn strip_base_prefix<'a>(pattern: &'a str, base: &Path) -> Option<&'a str> {
    strip_base_text(pattern, base).or_else(|| {
        let canonical = canonicalize_for_matching(base)?;
        strip_base_text(pattern, &canonical)
    })
}

/// `pattern` with the leading `base` removed, working on the text as written.
///
/// The pattern is not a path: a backslash in it is an escape, and a `Path`
/// would read it as a separator on Windows and drop it with the component. So
/// the base is only ever matched up to a `/` of the pattern, and what follows is
/// returned verbatim apart from the `.` components and separators a path
/// comparison would also have ignored (`/root/./docs`, `/root//docs/`).
fn strip_base_text<'a>(pattern: &'a str, base: &Path) -> Option<&'a str> {
    let boundaries = pattern
        .match_indices('/')
        .map(|(index, _)| index)
        .chain(std::iter::once(pattern.len()));
    for end in boundaries {
        // A root base (`/`, `C:/`) owns the separator that closes it.
        let rest = if Path::new(&pattern[..end]) == base {
            &pattern[end..]
        } else if end < pattern.len() && Path::new(&pattern[..=end]) == base {
            &pattern[end + 1..]
        } else {
            continue;
        };
        return Some(trim_relative_remainder(rest));
    }
    None
}

/// The pattern left after its base: without leading `/` and `./` components and
/// without trailing `/` and `/.`, which name the directory itself.
fn trim_relative_remainder(mut rest: &str) -> &str {
    loop {
        if let Some(tail) = rest.strip_prefix('/').or_else(|| rest.strip_prefix("./")) {
            rest = tail;
        } else if rest == "." {
            rest = "";
        } else {
            break;
        }
    }
    loop {
        let trimmed = trim_directory_slash(rest);
        let trimmed = match trimmed.strip_suffix("/.") {
            // `\/.` has an escaped `/`: that dot is not a component of its own.
            Some(head) if trailing_backslashes(head).is_multiple_of(2) => head,
            _ => trimmed,
        };
        if trimmed.len() == rest.len() {
            return rest;
        }
        rest = trimmed;
    }
}

fn trailing_backslashes(text: &str) -> usize {
    text.bytes().rev().take_while(|&byte| byte == b'\\').count()
}

/// `pattern` without its trailing directory marker.
///
/// A trailing `/` run is dropped. When exactly one `/` was there and a backslash
/// then ends the text, it goes too, as `ignore` does: it escaped the `/` that was
/// just dropped, so `docs\/` names the directory `docs`. `docs\\/` becomes
/// `docs\`, a dangling escape that does not compile, which is also how the walker
/// reads it. After several slashes (`docs\//`) the backslash is kept and the
/// result is that same dangling escape, which is reported rather than silently
/// matching nothing.
fn trim_directory_slash(pattern: &str) -> &str {
    let trimmed = pattern.trim_end_matches('/');
    match pattern.len() - trimmed.len() {
        0 => pattern,
        1 => trimmed.strip_suffix('\\').unwrap_or(trimmed),
        _ => trimmed,
    }
}

/// Expands directory-style patterns to also match files within them.
/// Pattern "dir/path" becomes ["dir/path", "dir/path/**"] to match both
/// the directory itself and all contents recursively. A leading `~` is
/// expanded first (see [`expand_home_prefix`]) and `.`/`..` components are
/// resolved (see [`prepare_pattern`]).
///
/// The result is the form the walker reads as `ignore` overrides, so a pattern
/// anchored to the root by an `./` keeps a leading `/` once the `./` is gone.
///
/// The expansion is driven by the pattern's *final* component: it names a
/// directory only when it holds no wildcard. `docs/*` therefore stays as
/// written (it names direct children, and `docs/*/**` would newly exclude
/// nested contents), while `**/.cursor/plans` gains its contents-expansion
/// despite the wildcard earlier in the pattern.
pub fn expand_directory_pattern(pattern: &str) -> Vec<String> {
    expand_directory_pattern_for(pattern, cfg!(windows))
}

/// [`expand_directory_pattern`] for an explicit platform (see [`prepare_pattern_for`]).
pub fn expand_directory_pattern_for(pattern: &str, windows: bool) -> Vec<String> {
    let expanded = expand_exclude(pattern, windows);
    if expanded.walker_anchor {
        expanded.entries.iter().map(|entry| format!("/{entry}")).collect()
    } else {
        expanded.entries
    }
}

/// The `ignore` override rules an `include` pattern turns into.
///
/// A whitelist glob that names a directory (`docs`) selects the directory entry
/// but none of the files beneath it, so the run lints nothing. The pattern is
/// therefore followed by a rule for the contents, the same expansion `exclude`
/// applies (see [`expand_directory_pattern`]): it is driven by the *final*
/// component, which names a directory only when it holds no wildcard.
///
/// The contents keep the pattern's own anchoring. A bare name floats like
/// `gitignore` and like the same `exclude`, so its contents are matched at any
/// depth (`**/docs/**`; plain `docs/**` would anchor to the root). A pattern with
/// a `/`, or a leading one, names one location and its contents stay under it.
///
/// Takes a pattern already run through [`normalize_pattern_for_base`]. A negation
/// and a comment (`#`) are returned as written: neither names files to select.
pub fn expand_include_directory(pattern: &str) -> Vec<String> {
    // A blank pattern is discarded by the glob builder; expanding it would turn
    // that no-op into a whitelist rule that selects nothing.
    if pattern.trim().is_empty() || pattern.starts_with(['!', '#']) {
        return vec![pattern.to_string()];
    }
    // `directory_entries` leaves `*`, `?` and `[` finals alone; a brace
    // alternation is a glob here too, so it names its entries and nothing below.
    let final_component = trim_directory_slash(pattern).rsplit('/').next().unwrap_or_default();
    if final_component.contains('{') {
        return vec![pattern.to_string()];
    }
    let mut rules = directory_entries(pattern);
    if let [directory, contents] = rules.as_mut_slice() {
        if !directory.contains('/') {
            *contents = format!("**/{directory}/**");
        }
        // The pattern itself stays as written, so a trailing `/` keeps it
        // matching directories only.
        *directory = pattern.to_string();
    }
    rules
}

/// An `exclude` pattern after preparation.
struct ExpandedExclude {
    /// The directory itself and its contents, in the form `globset` matches
    /// against a root-relative path.
    entries: Vec<String>,
    /// Whether the pattern, as written, matches at any depth (gitignore: no `/`
    /// except a trailing one). Decided before `.` components are removed, which
    /// is what anchors `./dir`.
    floats: bool,
    /// Whether the walker needs a leading `/` on every entry to keep the
    /// pattern anchored once its `./` is gone.
    walker_anchor: bool,
}

fn expand_exclude(pattern: &str, windows: bool) -> ExpandedExclude {
    let spelled = spell_pattern(pattern, true, windows);
    let floats = !trim_directory_slash(&spelled).contains('/');
    let prepared = match normalize_dot_segments(&spelled, windows) {
        Cow::Owned(normalized) => Cow::Owned(normalized),
        Cow::Borrowed(_) => spelled,
    };
    let directory = trim_directory_slash(&prepared);
    let walker_anchor = !floats && !is_absolute_pattern(&prepared) && !directory.is_empty() && !directory.contains('/');
    ExpandedExclude {
        entries: directory_entries(&prepared),
        floats,
        walker_anchor,
    }
}

fn directory_entries(pattern: &str) -> Vec<String> {
    let base = trim_directory_slash(pattern);
    let final_component = base.rsplit('/').next().unwrap_or(base);

    // A dangling escape names nothing: appending `/**` would turn it into an
    // escaped slash and revive a pattern the walker rejects, so it stays whole.
    let dangling_escape = trailing_backslashes(base) % 2 == 1;
    if final_component.is_empty() || final_component.contains(['*', '?', '[']) || dangling_escape {
        return vec![pattern.to_string()];
    }

    vec![
        base.to_string(),     // Match the directory itself
        format!("{base}/**"), // Match everything underneath
    ]
}

/// Why an `exclude` pattern with a leading `!` is rejected, and where the
/// exception it was reaching for can live instead.
pub const EXCLUDE_NEGATION_UNSUPPORTED: &str = "a leading '!' does not negate in exclude and is not supported; \
move both the broad exclusion and its exception into .markdownlintignore, which supports '!' re-inclusion";

/// Whether `pattern` is an `exclude` negation (a leading `!`).
///
/// `exclude` has no re-inclusion: the walker reads its patterns as an unordered
/// set of `ignore` overrides and the post-walk matchers read them as globs, and
/// neither can honor an exception to an earlier pattern. Such a pattern is
/// rejected everywhere, so that it matches nothing in every consumer instead of
/// excluding on a walk and nothing on a named file. A literal leading bang is
/// still reachable by escaping it (`\!name`).
pub fn is_exclude_negation(pattern: &str) -> bool {
    pattern.starts_with('!')
}

/// The `ignore` override rule that excludes `pattern`.
///
/// The crate spells exclusion with a leading `!`. Callers hand over only
/// patterns that are not negations (see [`is_exclude_negation`]), because a
/// leading `!` of the user's would otherwise be read as this marker.
pub fn exclude_override_rule(pattern: &str) -> String {
    format!("!{pattern}")
}

/// Whether every glob an `exclude` pattern turns into compiles.
///
/// An exclude pattern reaches two consumers: [`ExcludeMatchers`] compiles each
/// expansion with `globset`, and the walker adds each as an `ignore` override.
/// Both are mirrored here so a caller holding only the pattern can tell whether
/// either would reject it, which is also when either would print it. A negation
/// is rejected by both (see [`is_exclude_negation`]).
pub fn exclude_pattern_compiles(pattern: &str) -> bool {
    if is_exclude_negation(pattern) {
        return false;
    }
    expand_directory_pattern(pattern).iter().all(|expanded| {
        exclude_glob(expanded).is_ok()
            && ignore::overrides::OverrideBuilder::new(Path::new("."))
                .add(&exclude_override_rule(expanded))
                .is_ok()
    })
}

/// Whether an `include` pattern compiles as a walker override.
///
/// Answers for the pattern as given, which is only the form the walker uses once
/// [`normalize_pattern_for_base`] has run: stripping a base prefix removes
/// whatever the base's own name held, and an absolute pattern under a directory
/// called `notes [2019-2021]` carries a character class over a descending range
/// until the prefix comes off. Ask this about the pattern the walker is about to
/// add, never about the one a config file spelled.
pub fn include_pattern_compiles(pattern: &str) -> bool {
    ignore::overrides::OverrideBuilder::new(Path::new("."))
        .add(&prepare_pattern(pattern))
        .is_ok()
}

/// The globs matching everything one expanded exclude pattern removes: the
/// pattern itself, the contents of a directory it matches when `with_contents`,
/// and each of those at any depth when the pattern `floats` (see
/// [`ExcludeMatchers::new`]).
fn exclusion_globs(pattern: &str, with_contents: bool, floats: bool, windows: bool) -> Result<GlobSet, globset::Error> {
    let mut located = vec![pattern.to_string()];
    // The directory form is what the walker compiles, so a dangling escape it
    // would reject is rejected here too, before a `/**` can turn it into `\/**`.
    let directory = trim_directory_slash(pattern);
    if directory.len() != pattern.len() {
        path_glob_for(directory, true, windows)?;
    }
    if with_contents && !pattern.ends_with("/**") {
        located.push(format!("{directory}/**"));
    }
    if floats {
        let anywhere: Vec<String> = located.iter().map(|glob| format!("**/{glob}")).collect();
        located.extend(anywhere);
    }
    let mut builder = GlobSetBuilder::new();
    for glob in &located {
        builder.add(path_glob_for(glob, true, windows)?);
    }
    builder.build()
}

/// One exclude glob, compiled the way `.gitignore` reads it: `*` and `?` match
/// within one path component, and only `**` spans a `/`.
fn exclude_glob(glob: &str) -> Result<Glob, globset::Error> {
    path_glob(glob, true)
}

/// A glob over `/`-separated paths, with `\` as the escape for the next
/// character on Unix and as a path separator on Windows (see [`path_glob_for`]).
///
/// This is how the walker's `ignore` overrides read the same patterns, so a
/// file is selected by the walk and by rumdl's own matching (named files, stdin,
/// the editor integration, `per-file-*` keys) or by neither. `globset` alone
/// would read `\` as a path separator on Windows, so the choice is made
/// explicitly. `/` is the separator patterns are written with everywhere.
///
/// `literal_separator` keeps `*` and `?` within one path component.
pub(crate) fn path_glob(pattern: &str, literal_separator: bool) -> Result<Glob, globset::Error> {
    path_glob_for(pattern, literal_separator, cfg!(windows))
}

/// [`path_glob`] for an explicit platform. On Windows a `\` is a path
/// separator, as in [`prepare_pattern_for`], so it is never compiled as an
/// escape whichever route built the pattern.
pub(crate) fn path_glob_for(pattern: &str, literal_separator: bool, windows: bool) -> Result<Glob, globset::Error> {
    GlobBuilder::new(&spell_pattern(pattern, false, windows))
        .literal_separator(literal_separator)
        .backslash_escape(true)
        .build()
}

/// One expanded `exclude` pattern and the globs matching what it excludes.
struct ExcludeMatcher {
    pattern: String,
    globs: GlobSet,
    /// Whether any spelling of the pattern is absolute, which is what makes a
    /// file's absolute path worth matching against it.
    absolute: bool,
}

/// Compiled `exclude` patterns with directory-pattern expansion applied.
///
/// Match paths through [`matched_pattern`](Self::matched_pattern) using a
/// root-relative path (the CLI relativizes against the project root, the
/// LSP against the containing workspace root) so patterns like
/// `docs/drafts` behave identically everywhere.
pub struct ExcludeMatchers {
    /// Each expanded pattern (see [`ExcludeMatchers::new`]).
    matchers: Vec<ExcludeMatcher>,
    /// Spellings of a file the absolute patterns reach through a symlink.
    aliases: PathAliases,
    /// Patterns that failed to compile, with their errors. Callers decide
    /// how to surface these (CLI prints to stderr, LSP logs).
    pub invalid: Vec<(String, String)>,
}

/// Whether `pattern` names an absolute location. A leading `/` counts on every
/// platform: patterns use `/` separators, so a Unix-style path stays absolute
/// when the same config is read on Windows.
pub fn is_absolute_pattern(pattern: &str) -> bool {
    pattern.starts_with('/') || Path::new(pattern).is_absolute()
}

/// Whether `pattern` names an absolute location in any of its spellings.
///
/// A brace alternation can put the absolute part past the start of the pattern
/// (`{/opt,/srv}/docs/**`), where [`is_absolute_pattern`] cannot see it. Callers
/// deciding whether to match a file's absolute path at all must ask this, or
/// such a pattern is never given an absolute path to match.
pub fn has_absolute_spelling(pattern: &str) -> bool {
    if is_absolute_pattern(pattern) {
        return true;
    }
    pattern.contains('{')
        && expand_braces(pattern)
            .iter()
            .any(|spelling| is_absolute_pattern(spelling))
}

/// How many literal spellings one pattern's brace alternations may produce.
///
/// Expansion only *discovers* directory prefixes to canonicalize (see
/// [`PathAliases`]); it never decides whether a pattern matches. Past this
/// point a pattern like `{a,b}{c,d}{e,f}…` is multiplying out work that buys
/// nothing, so the pattern is left unexpanded.
const MAX_BRACE_EXPANSIONS: usize = 64;

/// Every literal spelling of `pattern`'s brace alternations:
/// `/{var/folders,tmp}/**` yields `/var/folders/**` and `/tmp/**`.
///
/// Returns just `pattern` when it holds no alternation, when its braces are
/// unbalanced, or when expanding would exceed [`MAX_BRACE_EXPANSIONS`].
/// Character classes are opaque, so a comma inside `[...]` stays literal.
fn expand_braces(pattern: &str) -> Vec<String> {
    let mut pending = vec![pattern.to_string()];
    let mut expanded: Vec<String> = Vec::new();
    while let Some(current) = pending.pop() {
        let Some((prefix, alternatives, suffix)) = split_first_alternation(&current) else {
            expanded.push(current);
            continue;
        };
        if pending.len() + expanded.len() + alternatives.len() > MAX_BRACE_EXPANSIONS {
            return vec![pattern.to_string()];
        }
        for alternative in alternatives {
            pending.push(format!("{prefix}{alternative}{suffix}"));
        }
    }
    expanded
}

/// Split `pattern` at its first top-level brace alternation into the text
/// before it, its alternatives, and the text after it. `None` when there is no
/// alternation to split on, including an unclosed `{`.
///
/// Empty alternatives are dropped, mirroring globset: it compiles `x{,y}` to
/// `^x(?:y)$`, so `x` is not one of that pattern's spellings.
fn split_first_alternation(pattern: &str) -> Option<(&str, Vec<&str>, &str)> {
    let bytes = pattern.as_bytes();
    let mut open = None;
    let mut depth = 0usize;
    let mut in_class = false;
    // The first `]` of a class (after an optional `!` or `^`) is a member.
    let mut class_start = 0;
    let mut alternatives = Vec::new();
    let mut alternative_start = 0;
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'\\' if !in_class => index += 1,
            b'[' if !in_class => {
                in_class = true;
                class_start = index + 1;
                if matches!(bytes.get(class_start), Some(b'!' | b'^')) {
                    class_start += 1;
                }
            }
            b']' if in_class && index == class_start => {}
            b']' if in_class => in_class = false,
            _ if in_class => {}
            b'{' => {
                depth += 1;
                if depth == 1 {
                    open = Some(index);
                    alternative_start = index + 1;
                }
            }
            b',' if depth == 1 => {
                alternatives.push(&pattern[alternative_start..index]);
                alternative_start = index + 1;
            }
            b'}' if depth > 0 => {
                depth -= 1;
                if depth == 0 {
                    alternatives.push(&pattern[alternative_start..index]);
                    alternatives.retain(|alternative| !alternative.is_empty());
                    if alternatives.is_empty() {
                        return None;
                    }
                    return Some((&pattern[..open?], alternatives, &pattern[index + 1..]));
                }
            }
            _ => {}
        }
        index += 1;
    }
    None
}

/// The leading run of `pattern`'s path components that hold no glob
/// metacharacter: `/var/folders/**` yields `/var/folders`, `/var/log/app*.md`
/// yields `/var/log`, and a fully literal pattern yields itself.
///
/// `None` when the run is empty or names only the filesystem root, neither of
/// which can resolve to a different location. The result is always a prefix
/// slice of `pattern`, so the remainder can be re-attached by byte offset.
///
/// A component holding a backslash (an escaped metacharacter, `\*`) ends the run
/// early on every platform. That yields a shorter prefix, never a wrong one.
fn literal_path_prefix(pattern: &str) -> Option<&str> {
    let mut end = 0;
    let mut saw_component = false;
    for component in pattern.split('/') {
        // A backslash makes the component an escape sequence, not a name: the
        // text is not what the pattern matches, so it cannot be resolved as a path.
        if component.contains(GLOB_METACHARS) || component.contains('\\') {
            break;
        }
        saw_component |= !component.is_empty();
        // Skip past this component and the separator that follows it.
        end += component.len() + 1;
    }
    if !saw_component {
        return None;
    }
    // The loop counted a separator after the final component; the pattern only
    // has one when the run did not reach its end. A trailing separator is
    // dropped so the remainder re-attaches with exactly one.
    let prefix = &pattern[..(end - 1).min(pattern.len())];
    let prefix = prefix.strip_suffix('/').unwrap_or(prefix);
    // A bare drive designator (`C:`) is the current directory of that drive, not
    // a location a pattern names.
    let bare_drive = matches!(prefix.as_bytes(), [letter, b':'] if letter.is_ascii_alphabetic());
    (!bare_drive).then_some(prefix)
}

/// `pattern` with its leading literal components resolved through symlinks, or
/// `None` when there is nothing to resolve, the prefix does not exist, or
/// resolving changes nothing.
fn canonicalize_pattern_prefix(pattern: &str) -> Option<String> {
    let prefix = literal_path_prefix(pattern)?;
    let canonical = canonicalize_for_matching(Path::new(prefix))?;
    let canonical = normalize_pattern_separators(canonical.to_string_lossy()).into_owned();
    if canonical == prefix {
        return None;
    }
    Some(format!("{canonical}{}", &pattern[prefix.len()..]))
}

/// Alternative spellings of a path, implied by the absolute patterns in a
/// configuration.
///
/// A pattern names a location the way the user wrote it (`/var/folders/**` on
/// macOS); the file it is matched against arrives canonicalized
/// (`/private/var/folders/…`), so the two never meet. Each pair recorded here
/// is one symlinked prefix some pattern reached a location through: the
/// canonical form of that pattern's leading literal components, and the
/// spelling the pattern used for them.
///
/// Rewriting the *path* rather than the pattern leaves globset the only
/// authority on what a pattern means, and cannot invent a match:
/// `canonicalize(as_written) == canonical` together with `path == canonical +
/// rest` say that `as_written + rest` names that same file. Brace alternations
/// are expanded only to find more prefixes to canonicalize, so an expansion
/// that disagrees with globset can cost a spelling, never fabricate one.
#[derive(Debug, Default)]
pub struct PathAliases {
    /// `(canonical prefix, the spelling a pattern used for it)`.
    prefixes: Vec<(PathBuf, String)>,
}

impl PathAliases {
    /// Collect the symlinked prefixes `patterns` reach locations through.
    ///
    /// Each pattern is canonicalized once here, at cache-build time, so
    /// per-file matching pays no syscall.
    pub fn new<'a>(patterns: impl IntoIterator<Item = &'a str>) -> Self {
        let mut prefixes: Vec<(PathBuf, String)> = Vec::new();
        for pattern in patterns {
            let pattern = prepare_pattern(pattern);
            if !has_absolute_spelling(&pattern) {
                continue;
            }
            for spelling in expand_braces(&pattern) {
                let Some(as_written) = literal_path_prefix(&spelling) else {
                    continue;
                };
                if !is_absolute_pattern(as_written) {
                    continue;
                }
                let Some(canonical) = canonicalize_for_matching(Path::new(as_written)) else {
                    continue;
                };
                if canonical == Path::new(as_written) {
                    continue;
                }
                let as_written = normalize_pattern_separators(Cow::Borrowed(as_written)).into_owned();
                if !prefixes.iter().any(|(c, w)| c == &canonical && w == &as_written) {
                    prefixes.push((canonical, as_written));
                }
            }
        }
        Self { prefixes }
    }

    pub fn is_empty(&self) -> bool {
        self.prefixes.is_empty()
    }

    /// The spellings of `path` reachable through a recorded prefix, as glob
    /// match candidates. Empty when no pattern reached `path`'s location
    /// through a symlink, which is every configuration that has none.
    pub fn spellings_of(&self, path: &Path) -> Vec<String> {
        self.prefixes
            .iter()
            .filter_map(|(canonical, as_written)| {
                let rest = path.strip_prefix(canonical).ok()?;
                if rest.as_os_str().is_empty() {
                    return Some(as_written.clone());
                }
                let rest = normalize_pattern_separators(rest.to_string_lossy());
                Some(format!("{as_written}/{rest}"))
            })
            .collect()
    }
}

impl ExcludeMatchers {
    /// Compile `patterns` to exclude what the discovery walk excludes.
    ///
    /// The walk applies each pattern as an `ignore` override, which reads it the
    /// way `.gitignore` does, so a file named directly has to be matched by the
    /// same rules or it is linted where the walk skips it:
    ///
    /// - A pattern with no `/` except a trailing one matches at any depth:
    ///   `node_modules` excludes `packages/app/node_modules/readme.md`.
    /// - A pattern that matches a directory excludes everything in it, including
    ///   one whose final component is a wildcard: `draft?` excludes
    ///   `drafts/note.md`.
    /// - `*` and `?` match within one path component: `sub/*.md` excludes
    ///   `sub/top.md` but not `sub/a/b.md`.
    ///
    /// Relative patterns are otherwise anchored at the root the path is relative
    /// to, and an absolute pattern never floats.
    pub fn new(patterns: &[String]) -> Self {
        Self::for_platform(patterns, cfg!(windows))
    }

    /// [`ExcludeMatchers::new`] for an explicit platform (see [`prepare_pattern_for`]).
    pub fn for_platform(patterns: &[String], windows: bool) -> Self {
        let mut matchers = Vec::new();
        let mut invalid = Vec::new();
        for written in patterns {
            if is_exclude_negation(written) {
                // The same pattern listed twice is one problem, not two.
                if !invalid.iter().any(|(pattern, _): &(String, String)| pattern == written) {
                    invalid.push((written.clone(), EXCLUDE_NEGATION_UNSUPPORTED.to_string()));
                }
                continue;
            }
            let ExpandedExclude {
                entries: expansions,
                floats,
                ..
            } = expand_exclude(written, windows);
            // A literal directory name already expanded to its own `/**` entry,
            // which the notice names. Only a wildcard final component is left
            // without one, and a directory it matches still has contents.
            let with_contents = expansions.len() == 1;
            for pattern in expansions {
                let absolute = has_absolute_spelling(&pattern);
                match exclusion_globs(&pattern, with_contents, floats && !absolute, windows) {
                    Ok(globs) => matchers.push(ExcludeMatcher {
                        pattern,
                        globs,
                        absolute,
                    }),
                    Err(e) => invalid.push((pattern, e.to_string())),
                }
            }
        }
        let aliases = PathAliases::new(matchers.iter().map(|matcher| matcher.pattern.as_str()));
        Self {
            matchers,
            aliases,
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
            .find(|matcher| matcher.globs.is_match(relative_path))
            .map(|matcher| matcher.pattern.as_str())
    }

    /// The first absolute pattern matching `absolute_path`, if any.
    fn matched_absolute_pattern(&self, absolute_path: &str) -> Option<&str> {
        self.matchers
            .iter()
            .find(|matcher| matcher.absolute && matcher.globs.is_match(absolute_path))
            .map(|matcher| matcher.pattern.as_str())
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
    /// Only absolute patterns are matched against the absolute path. A relative
    /// pattern belongs to the root its path is relative to, and one that floats
    /// would otherwise match a directory above that root: `docs` would exclude
    /// every file of a project checked out at `/home/dev/docs/site`.
    ///
    /// `absolute` is canonicalized before matching, since an expanded `~`
    /// resolves to a canonical location. A file that does not exist (an unsaved
    /// buffer, one already deleted) is resolved through its deepest existing
    /// ancestor (see [`resolve_for_matching`]).
    ///
    /// The path as given is tried as well, because resolving can respell a
    /// location the pattern names directly: on macOS `/home` canonicalizes to
    /// `/System/Volumes/Data/home`, which `/home/dev/**` does not match.
    ///
    /// A pattern that named its location through a symlink (`/var/folders/**`
    /// for a macOS temp directory) never matches that canonical form, so the
    /// file's other spellings are tried too (see [`PathAliases`]).
    pub fn matched_pattern_for_file(&self, relative: Option<&str>, absolute: &Path) -> Option<&str> {
        if let Some(pattern) = relative.and_then(|rel| self.matched_pattern(rel)) {
            return Some(pattern);
        }
        // Resolving the path costs syscalls, which a run with only relative
        // patterns has no use for.
        if !self.matchers.iter().any(|matcher| matcher.absolute) {
            return None;
        }
        let resolved = resolve_for_matching(absolute);
        if let Some(pattern) = self.matched_absolute_pattern(&normalize_pattern_separators(resolved.to_string_lossy()))
        {
            return Some(pattern);
        }
        // A rooted path is what an absolute pattern can name: a pattern is
        // absolute when it starts with `/` (see `is_absolute_pattern`), and on
        // Windows `\home\dev` has a root without being `is_absolute`.
        if absolute.has_root()
            && resolved != absolute
            && let Some(pattern) =
                self.matched_absolute_pattern(&normalize_pattern_separators(absolute.to_string_lossy()))
        {
            return Some(pattern);
        }
        self.aliases
            .spellings_of(&resolved)
            .into_iter()
            .find_map(|alias| self.matched_absolute_pattern(&alias))
    }

    /// Whether any pattern matches the file (see [`matched_pattern_for_file`](Self::matched_pattern_for_file)).
    pub fn excludes_file(&self, relative: Option<&str>, absolute: &Path) -> bool {
        self.matched_pattern_for_file(relative, absolute).is_some()
    }
}

/// Relativize `path` against `base` for exclude-pattern matching,
/// canonicalizing both sides so symlinks (e.g. macOS `/tmp`) and Windows
/// path-representation differences don't defeat the prefix strip. Returns
/// `None` when `path` is not under `base`, or when `base` does not exist.
///
/// `path` need not exist: it is resolved through its deepest existing ancestor
/// (see [`resolve_for_matching`]), so a file about to be created relativizes
/// the way it will once it is there.
///
/// Separators are normalized to `/` on Windows, following the project
/// convention for path strings; globset matches either form, but log
/// output and assertions see one canonical shape.
pub fn path_relative_to(path: &Path, base: &Path) -> Option<String> {
    let canonical_base = canonicalize_for_matching(base)?;
    let resolved_path = resolve_for_matching(path);
    resolved_path.strip_prefix(&canonical_base).ok().map(|rel| {
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
    #[test]
    fn type_filter_preserves_legacy_final_file_selection() {
        use super::*;
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        for name in [
            "README.md",
            "notes.MD",
            "mixed.mArKdOwN",
            "page.mdx",
            "index.mkd",
            "a.mkdn",
            "a.mdown",
            "a.mdwn",
            "a.qmd",
            "a.Rmd",
            "a.mkdown",
            "a.rs",
            "a.RS",
            "a.json",
            "a.tmpl",
            "文档.MD",
            "no-extension",
        ] {
            std::fs::write(root.join(name), "body\n").unwrap();
        }
        for mode in [
            LintableFileMode::Markdown,
            LintableFileMode::MarkdownAndRust,
            LintableFileMode::Any,
        ] {
            for includes in [vec![], vec!["a.tmpl".to_string()], vec!["*.json".to_string()]] {
                let selector = LintablePathSelector::new(Some(root), &includes, mode);
                let mut old = ignore::WalkBuilder::new(root);
                if mode != LintableFileMode::Any {
                    let mut types = ignore::types::TypesBuilder::new();
                    types.add_defaults();
                    for ext in MARKDOWN_EXTENSIONS {
                        types.add("markdown", &any_case_extension_glob(ext)).unwrap();
                    }
                    types.select("markdown");
                    if mode == LintableFileMode::MarkdownAndRust {
                        types.add("rustdoc", "*.rs").unwrap();
                        types.select("rustdoc");
                    }
                    for glob in selector.explicit.file_name_globs() {
                        types.add("configinclude", type_filter_glob(glob)).unwrap();
                    }
                    if !selector.explicit.is_empty() {
                        types.select("configinclude");
                    }
                    old.types(types.build().unwrap());
                }
                let mut new = ignore::WalkBuilder::new(root);
                selector.configure_types(&mut new).unwrap();
                let collect = |builder: ignore::WalkBuilder| {
                    let mut files: Vec<_> = builder
                        .build()
                        .map(Result::unwrap)
                        .filter(|entry| {
                            entry.file_type().is_some_and(|kind| kind.is_file()) && selector.keeps(entry.path())
                        })
                        .map(|entry| entry.path().strip_prefix(root).unwrap().to_path_buf())
                        .collect();
                    files.sort();
                    files
                };
                assert_eq!(collect(new), collect(old), "{mode:?} {includes:?}");
            }
        }
    }

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
    fn lintable_selector_makes_adapter_capabilities_explicit() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        fs::create_dir_all(root.join("docs")).unwrap();
        fs::create_dir_all(root.join("templates")).unwrap();
        fs::create_dir_all(root.join("src")).unwrap();
        for relative in [
            "docs/guide.md",
            "docs/notes.txt",
            "templates/page.md.jinja",
            "src/lib.rs",
            "src/upper.RS",
        ] {
            fs::write(root.join(relative), "content\n").unwrap();
        }
        let includes = vec![
            "docs/**".to_string(),
            "templates/**/*.md.jinja".to_string(),
            "src/**/*.rs".to_string(),
        ];

        let markdown = LintablePathSelector::new(Some(root), &includes, LintableFileMode::Markdown);
        assert!(markdown.keeps(&root.join("docs/guide.md")));
        assert!(markdown.keeps(&root.join("templates/page.md.jinja")));
        assert!(!markdown.keeps(&root.join("docs/notes.txt")));
        assert!(
            !markdown.keeps(&root.join("src/lib.rs")),
            "an LSP must not parse a complete Rust source file as Markdown"
        );

        let rustdoc = LintablePathSelector::new(Some(root), &includes, LintableFileMode::MarkdownAndRust);
        assert!(rustdoc.keeps(&root.join("src/lib.rs")));
        assert!(!rustdoc.keeps(&root.join("src/upper.RS")));
        assert!(!rustdoc.keeps(&root.join("docs/notes.txt")));

        let unrestricted = LintablePathSelector::new(Some(root), &includes, LintableFileMode::Any);
        assert!(unrestricted.keeps(&root.join("docs/notes.txt")));
    }

    #[test]
    fn workspace_scan_rejects_explicit_rust_includes() {
        let dir = tempdir().unwrap();
        let root = dir.path().to_path_buf();
        fs::create_dir(root.join("src")).unwrap();
        fs::write(root.join("src/lib.rs"), "/// # Not a document\n").unwrap();
        fs::write(root.join("README.md"), "# Readme\n").unwrap();

        let options = MarkdownWalkOptions {
            respect_gitignore: false,
            skip_vendor_dirs: true,
        };
        let includes = vec!["src/**/*.rs".to_string()];
        let excludes = ExcludeMatchers::new(&[]);
        let scan = MarkdownWorkspaceScan::new(&options, &includes, &excludes);

        assert!(scan.collect(std::slice::from_ref(&root)).is_empty());
        assert!(scan.path_is_ignored(std::slice::from_ref(&root), &root.join("src/lib.rs")));
    }

    #[test]
    fn workspace_scan_applies_includes_to_standard_and_explicit_files() {
        let dir = tempdir().unwrap();
        let root = dir.path().to_path_buf();
        fs::create_dir(root.join("docs")).unwrap();
        fs::create_dir(root.join("templates")).unwrap();
        fs::write(root.join("README.md"), "# Root\n").unwrap();
        fs::write(root.join("docs/guide.md"), "# Guide\n").unwrap();
        fs::write(root.join("templates/page.md.jinja"), "# Template\n").unwrap();
        fs::write(root.join("templates/page.txt"), "not markdown\n").unwrap();

        let options = MarkdownWalkOptions {
            respect_gitignore: false,
            skip_vendor_dirs: true,
        };
        let includes = vec!["docs/**".to_string(), "templates/**/*.md.jinja".to_string()];
        let excludes = ExcludeMatchers::new(&[]);
        let scan = MarkdownWorkspaceScan::new(&options, &includes, &excludes);

        // The test creates these names itself, so normalizing separators
        // unconditionally is safe and keeps one expected value for every platform.
        let names: Vec<String> = scan
            .collect(std::slice::from_ref(&root))
            .iter()
            .map(|path| path.strip_prefix(&root).unwrap().to_string_lossy().replace('\\', "/"))
            .collect();
        assert_eq!(names, vec!["docs/guide.md", "templates/page.md.jinja"]);

        assert!(!scan.path_is_ignored(std::slice::from_ref(&root), &root.join("templates/page.md.jinja")));
        assert!(scan.path_is_ignored(std::slice::from_ref(&root), &root.join("README.md")));
        assert!(scan.path_is_ignored(std::slice::from_ref(&root), &root.join("templates/page.txt")));
    }

    #[test]
    fn workspace_scan_selects_the_markdown_beneath_an_included_directory() {
        let dir = tempdir().unwrap();
        let root = dir.path().to_path_buf();
        for path in [
            "docs/guide.md",
            "docs/sub/deep.md",
            "docs/image.png",
            "other/docs/x.md",
            "README.md",
        ] {
            fs::create_dir_all(root.join(path).parent().unwrap()).unwrap();
            fs::write(root.join(path), "# T\n").unwrap();
        }
        let options = MarkdownWalkOptions {
            respect_gitignore: false,
            skip_vendor_dirs: true,
        };
        let excludes = ExcludeMatchers::new(&[]);
        let names = |includes: &[&str]| -> Vec<String> {
            let includes: Vec<String> = includes.iter().map(ToString::to_string).collect();
            let scan = MarkdownWorkspaceScan::new(&options, &includes, &excludes);
            let mut names: Vec<String> = scan
                .collect(std::slice::from_ref(&root))
                .iter()
                .map(|path| path.strip_prefix(&root).unwrap().to_string_lossy().replace('\\', "/"))
                .collect();
            names.sort();
            names
        };

        // A bare name floats, as the same `exclude` does.
        assert_eq!(
            names(&["docs"]),
            ["docs/guide.md", "docs/sub/deep.md", "other/docs/x.md"]
        );
        assert_eq!(
            names(&["docs/"]),
            ["docs/guide.md", "docs/sub/deep.md", "other/docs/x.md"]
        );
        // A pattern with a `/`, or an `./`, names one location.
        assert_eq!(names(&["./docs"]), ["docs/guide.md", "docs/sub/deep.md"]);
        assert_eq!(names(&["other/docs"]), ["other/docs/x.md"]);
        // A comment names nothing.
        assert_eq!(names(&["./docs", "#other"]), ["docs/guide.md", "docs/sub/deep.md"]);
    }

    #[test]
    fn workspace_scan_does_not_prune_a_vendor_named_root() {
        let dir = tempdir().unwrap();
        let root = dir.path().join("target");
        fs::create_dir(&root).unwrap();
        fs::write(root.join("README.md"), "# Root\n").unwrap();
        fs::create_dir(root.join("target")).unwrap();
        fs::write(root.join("target/generated.md"), "# Generated\n").unwrap();

        let options = MarkdownWalkOptions {
            respect_gitignore: false,
            skip_vendor_dirs: true,
        };
        let excludes = ExcludeMatchers::new(&[]);
        let scan = MarkdownWorkspaceScan::new(&options, &[], &excludes);

        assert_eq!(scan.collect(std::slice::from_ref(&root)), vec![root.join("README.md")]);
        assert!(!scan.path_is_ignored(std::slice::from_ref(&root), &root.join("README.md")));
        assert!(scan.path_is_ignored(std::slice::from_ref(&root), &root.join("target/generated.md")));
    }

    #[test]
    fn the_type_glob_selects_exactly_what_counts_as_markdown() {
        assert_eq!(any_case_extension_glob("md"), "*.[mM][dD]");

        // The glob stands in for `is_markdown_extension` inside a walk, so the
        // two have to agree on every spelling, not just the lowercase one.
        let mut builder = globset::GlobSetBuilder::new();
        for ext in MARKDOWN_EXTENSIONS {
            builder.add(
                globset::GlobBuilder::new(&any_case_extension_glob(ext))
                    .literal_separator(true)
                    .build()
                    .unwrap(),
            );
        }
        let globs = builder.build().unwrap();

        for ext in MARKDOWN_EXTENSIONS {
            for spelling in [ext.to_ascii_lowercase(), ext.to_ascii_uppercase(), capitalize(ext)] {
                let name = format!("README.{spelling}");
                assert!(
                    globs.is_match(&name),
                    "{name} is markdown by extension but no type glob selects it"
                );
                assert!(is_markdown_extension(OsStr::new(&spelling)), "{spelling} should match");
            }
        }

        // Control: the glob widens case, not the extension set.
        for name in ["lib.rs", "notes.txt", "README.mdq", "README.m"] {
            assert!(!globs.is_match(name), "{name} should not be selected");
        }
    }

    fn capitalize(ext: &str) -> String {
        let mut chars = ext.chars();
        match chars.next() {
            Some(first) => first.to_ascii_uppercase().to_string() + chars.as_str(),
            None => String::new(),
        }
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
    fn a_gitignore_above_the_repository_root_stays_outside_it() {
        let temp = tempdir().unwrap();
        fs::write(temp.path().join(".gitignore"), "*.md\n").unwrap();
        let repo = temp.path().join("repo");
        fs::create_dir_all(repo.join(".git")).unwrap();
        fs::write(repo.join("kept.md"), "# hi").unwrap();

        let walk = |root: &Path| -> Vec<std::path::PathBuf> {
            markdown_walk_builder(root, &MarkdownWalkOptions::default())
                .build()
                .flatten()
                .filter(|e| e.file_type().is_some_and(|t| t.is_file()))
                .map(|e| e.path().to_path_buf())
                .collect()
        };

        assert!(
            walk(&repo).iter().any(|p| p.ends_with("kept.md")),
            "git reads no gitignore above the repository root, so neither does the walk"
        );

        // Control: outside a repository there is no root to stop at, and the
        // ignore files above are all the walk has to go on.
        fs::remove_dir(repo.join(".git")).unwrap();
        assert!(
            !walk(&repo).iter().any(|p| p.ends_with("kept.md")),
            "with no repository to bound it, the walk keeps reading upward"
        );
    }

    #[test]
    fn the_repository_boundary_needs_every_root_to_have_one() {
        let temp = tempdir().unwrap();
        let inside = temp.path().join("repo/docs");
        fs::create_dir_all(&inside).unwrap();
        fs::create_dir_all(temp.path().join("repo/.git")).unwrap();
        let outside = temp.path().join("plain");
        fs::create_dir_all(&outside).unwrap();

        assert!(stops_at_repository_root(&[&inside]), "a root under a repository root");
        assert!(!stops_at_repository_root(&[&outside]), "a root under no repository");

        // A walk has one setting for all of its roots. Bounding this one would
        // strip the outside root of gitignore handling altogether, which is a
        // worse answer than reading one file too many.
        assert!(!stops_at_repository_root(&[inside.as_path(), outside.as_path()]));
        assert!(!stops_at_repository_root(&[] as &[&Path]), "no root is no repository");

        // A worktree and a submodule mark their root with a `.git` file rather
        // than a directory, and both are still repository roots.
        let worktree = temp.path().join("worktree");
        fs::create_dir_all(&worktree).unwrap();
        fs::write(worktree.join(".git"), "gitdir: /elsewhere/.git/worktrees/x\n").unwrap();
        assert!(stops_at_repository_root(&[&worktree]));
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
    fn exclude_matchers_float_a_pattern_without_a_slash_to_any_depth() {
        let matchers =
            ExcludeMatchers::new(&["node_modules".to_string(), "build/".to_string(), "notes.md".to_string()]);
        for path in [
            "node_modules/readme.md",
            "packages/app/node_modules/readme.md",
            "build/out.md",
            "site/build/out.md",
            "notes.md",
            "docs/notes.md",
        ] {
            assert!(matchers.is_match(path), "{path} must be excluded");
        }
        for path in ["node_modules.md", "docs/my-notes.md", "rebuild/out.md", "docs/guide.md"] {
            assert!(!matchers.is_match(path), "{path} must not be excluded");
        }
        // The notice names the pattern as expanded, not the glob that floated it.
        assert_eq!(
            matchers.matched_pattern("packages/app/node_modules/readme.md"),
            Some("node_modules/**")
        );
    }

    #[test]
    fn exclude_matchers_keep_a_pattern_containing_a_slash_anchored() {
        let matchers = ExcludeMatchers::new(&["docs/generated".to_string(), "sub/*.md".to_string()]);
        assert!(matchers.is_match("docs/generated/api.md"));
        assert!(!matchers.is_match("site/docs/generated/api.md"));
        assert!(matchers.is_match("sub/a.md"));
        assert!(!matchers.is_match("other/sub/a.md"));
    }

    #[test]
    fn exclude_matchers_exclude_the_contents_of_a_directory_a_wildcard_matches() {
        let matchers = ExcludeMatchers::new(&["draft?".to_string(), "sub/[gh]en".to_string()]);
        assert!(matchers.is_match("drafts/note.md"));
        assert!(
            matchers.is_match("blog/drafts/note.md"),
            "a slashless wildcard floats too"
        );
        assert!(matchers.is_match("sub/gen/x.md"));
        assert!(matchers.is_match("sub/hen/deep/x.md"));
        assert!(!matchers.is_match("sub/pen/x.md"));
        assert!(!matchers.is_match("other/sub/gen/x.md"));
    }

    #[test]
    fn exclude_matchers_report_an_invalid_pattern_once() {
        let matchers = ExcludeMatchers::new(&["[".to_string(), "docs".to_string()]);
        assert_eq!(matchers.invalid.len(), 1, "{:?}", matchers.invalid);
        assert!(matchers.is_match("docs/a.md"));
    }

    #[test]
    fn expand_home_prefix_expands_only_a_leading_tilde() {
        let home = Path::new("/home/dev");
        assert_eq!(
            expand_home_prefix_impl("~/.cursor/plans", Some(home)),
            "/home/dev/.cursor/plans"
        );
        assert_eq!(expand_home_prefix_impl("~", Some(home)), "/home/dev");
        assert_eq!(expand_home_prefix_impl("~/", Some(home)), "/home/dev");
    }

    #[test]
    fn expand_home_prefix_leaves_interior_tildes_alone() {
        let home = Path::new("/home/dev");
        // `~` is a legal filename character; only a leading `~/` is a home reference.
        for pattern in ["backup.md~", "docs/~drafts/**", "~user/docs", "**/*~", "!~/secret"] {
            assert_eq!(
                expand_home_prefix_impl(pattern, Some(home)),
                pattern,
                "{pattern:?} must be left as written"
            );
        }
    }

    #[test]
    fn expand_home_prefix_without_a_home_leaves_the_pattern_as_written() {
        assert_eq!(expand_home_prefix_impl("~/.cursor/plans", None), "~/.cursor/plans");
    }

    #[test]
    fn normalize_pattern_for_base_rewrites_absolute_patterns_under_the_base() {
        let temp = tempdir().unwrap();
        // Canonicalize the way production does, so the pattern has the shape an
        // expanded `~` produces (on Windows that means no verbatim prefix).
        let base = canonicalize_for_matching(temp.path()).unwrap();
        let pattern = format!("{}/docs/**", base.to_string_lossy().replace('\\', "/"));
        assert_eq!(normalize_pattern_for_base(&pattern, Some(&base)), "docs/**");
    }

    #[test]
    fn normalize_pattern_for_base_strips_through_a_non_canonical_base() {
        // The base as handed to us (a symlinked `/var` on macOS, a Windows 8.3
        // short name) must still strip.
        let temp = tempdir().unwrap();
        let canonical = canonicalize_for_matching(temp.path()).unwrap();
        let pattern = format!("{}/docs/**", canonical.to_string_lossy().replace('\\', "/"));
        assert_eq!(normalize_pattern_for_base(&pattern, Some(temp.path())), "docs/**");
    }

    #[test]
    fn normalize_pattern_for_base_leaves_other_patterns_alone() {
        let temp = tempdir().unwrap();
        let base = canonicalize_for_matching(temp.path()).unwrap();
        // Relative patterns are already base-relative.
        assert_eq!(normalize_pattern_for_base("docs/**", Some(&base)), "docs/**");
        // An absolute pattern outside the base stays absolute: nothing under
        // this walk can match it, which is the correct outcome.
        assert_eq!(
            normalize_pattern_for_base("/somewhere/else/**", Some(&base)),
            "/somewhere/else/**"
        );
        // With no base there is nothing to rewrite against.
        assert_eq!(normalize_pattern_for_base("/abs/docs/**", None), "/abs/docs/**");
    }

    #[test]
    fn strip_verbatim_prefix_unwraps_windows_canonical_paths() {
        // The exact shape `canonicalize` returns on Windows. Left unstripped it
        // normalizes to `//?/C:/...`, which matches nothing.
        assert_eq!(
            strip_verbatim_prefix(r"\\?\C:\Users\dev\AppData\Local\Temp\x"),
            r"C:\Users\dev\AppData\Local\Temp\x"
        );
        assert_eq!(strip_verbatim_prefix(r"\\?\C:\"), r"C:\");
        // UNC shares unwrap to their ordinary `\\server\share` form.
        assert_eq!(
            strip_verbatim_prefix(r"\\?\UNC\server\share\docs"),
            r"\\server\share\docs"
        );
    }

    #[test]
    fn strip_verbatim_prefix_unwraps_the_display_form() {
        // The same paths after display normalization has turned `\` into `/`.
        assert_eq!(
            strip_verbatim_prefix("//?/C:/Users/dev/AppData/Local/Temp/x"),
            "C:/Users/dev/AppData/Local/Temp/x"
        );
        assert_eq!(strip_verbatim_prefix("//?/C:/"), "C:/");
        assert_eq!(
            strip_verbatim_prefix("//?/UNC/server/share/docs"),
            "//server/share/docs"
        );
    }

    #[test]
    fn strip_verbatim_prefix_leaves_other_paths_alone() {
        for path in [
            "/home/dev/docs",
            r"C:\Users\dev",
            "C:/Users/dev",
            r"\\server\share",
            "//server/share",
            // A device namespace has no ordinary equivalent to unwrap to.
            r"\\?\Volume{b75e2c83-0000-0000-0000-602f00000000}\docs",
            "//?/Volume{b75e2c83-0000-0000-0000-602f00000000}/docs",
            r"\\?\",
            "//?/",
            // A prefix written in one separator does not unwrap in the other.
            r"\\?/C:/Users/dev",
            "",
        ] {
            assert_eq!(strip_verbatim_prefix(path), path, "{path:?} must be left as written");
        }
    }

    #[test]
    fn a_negated_exclude_pattern_is_invalid_and_matches_nothing() {
        let matchers = ExcludeMatchers::new(&[
            "!docs/a.md".to_string(),
            "!".to_string(),
            "!docs/a.md".to_string(),
            "docs/b.md".to_string(),
        ]);
        let reported: Vec<&str> = matchers.invalid.iter().map(|(pattern, _)| pattern.as_str()).collect();
        assert_eq!(reported, ["!docs/a.md", "!"], "each distinct negation once");
        assert!(
            matchers
                .invalid
                .iter()
                .all(|(_, error)| error == EXCLUDE_NEGATION_UNSUPPORTED)
        );
        assert!(!matchers.is_match("docs/a.md"));
        assert!(!matchers.is_match("!"));
        assert!(!matchers.is_match("!docs/a.md"));
        assert!(
            matchers.is_match("docs/b.md"),
            "an ordinary pattern beside it still applies"
        );
    }

    #[test]
    #[cfg(not(windows))]
    fn an_escaped_leading_bang_is_a_literal_exclude() {
        let matchers = ExcludeMatchers::new(&[r"\!bang.md".to_string()]);
        assert!(matchers.invalid.is_empty(), "{:?}", matchers.invalid);
        assert!(matchers.is_match("!bang.md"));
        assert!(exclude_pattern_compiles(r"\!bang.md"));
        assert!(!is_exclude_negation(r"\!bang.md"));
    }

    #[test]
    #[cfg(not(windows))]
    fn an_escaped_leading_bang_preserves_subsequent_escapes() {
        for pattern in [r"\!foo\[bar].md", r"\!foo\[bar\].md"] {
            let matchers = ExcludeMatchers::new(&[pattern.to_string()]);
            assert!(matchers.invalid.is_empty(), "{pattern}: {:?}", matchers.invalid);
            assert!(matchers.is_match("!foo[bar].md"), "{pattern}");
            assert!(matchers.is_match("docs/!foo[bar].md"), "{pattern}");
            assert!(!matchers.is_match("!foo/b.md"), "{pattern}");
            assert!(!matchers.is_match("!foob.md"), "{pattern}");
        }
    }

    /// Off Windows the escape reading must not depend on the platform default of
    /// `globset`; on Windows a `\` is a separator (see
    /// `windows_reads_a_backslash_as_a_separator_and_unix_as_an_escape`).
    #[test]
    #[cfg(not(windows))]
    fn exclude_glob_reads_a_backslash_as_an_escape_off_windows() {
        let escaped = exclude_glob(r"\!bang.md").unwrap().compile_matcher();
        assert!(escaped.is_match("!bang.md"));
        assert!(!escaped.is_match(r"\!bang.md"));

        // The reading the Windows default produces: no escape, so the pattern
        // does not name `!bang.md`. This is what `exclude_glob` must not inherit.
        let unescaped = GlobBuilder::new(r"\!bang.md")
            .literal_separator(true)
            .backslash_escape(false)
            .build()
            .unwrap()
            .compile_matcher();
        assert!(!unescaped.is_match("!bang.md"));
    }

    /// The post-walk matchers and the walker's `ignore` overrides read an
    /// escaped pattern the same way, so a name is excluded by both or by neither.
    #[test]
    #[cfg(not(windows))]
    fn exclude_matchers_and_walker_overrides_agree_on_escaped_patterns() {
        let names = ["!bang.md", "[x].md", "x.md", "a*b.md", "axb.md", r"\!bang.md"];
        for pattern in [r"\!bang.md", r"\[x\].md", r"a\*b.md"] {
            let matchers = ExcludeMatchers::new(&[pattern.to_string()]);
            assert!(matchers.invalid.is_empty(), "{pattern}: {:?}", matchers.invalid);
            let mut builder = ignore::overrides::OverrideBuilder::new(Path::new("/root"));
            builder.add(&exclude_override_rule(pattern)).unwrap();
            let walker = builder.build().unwrap();
            for name in names {
                let by_walker = walker.matched(Path::new("/root").join(name), false).is_ignore();
                assert_eq!(
                    matchers.is_match(name),
                    by_walker,
                    "pattern {pattern} on {name}: matchers vs walker"
                );
            }
            assert!(
                matchers.is_match(names[0]) || pattern != r"\!bang.md",
                "the escaped bang names the literal file"
            );
        }
    }

    #[test]
    fn exclude_pattern_compiles_rejects_a_negation() {
        assert!(!exclude_pattern_compiles("!x"));
        assert!(!exclude_pattern_compiles("!"));
        assert!(exclude_pattern_compiles("x"));
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
    fn expand_include_directory_adds_the_contents_with_the_patterns_own_anchoring() {
        // A bare name floats, so its contents are matched at any depth.
        assert_eq!(expand_include_directory("docs"), ["docs", "**/docs/**"]);
        // The pattern itself stays as written, so a trailing `/` keeps it directory-only.
        assert_eq!(expand_include_directory("docs/"), ["docs/", "**/docs/**"]);
        // A `/` anywhere but the end, or a leading one, anchors the contents too.
        assert_eq!(expand_include_directory("a/docs"), ["a/docs", "a/docs/**"]);
        assert_eq!(expand_include_directory("/docs"), ["/docs", "/docs/**"]);
        assert_eq!(expand_include_directory("/abs/docs/"), ["/abs/docs/", "/abs/docs/**"]);
        assert_eq!(expand_include_directory("**/docs"), ["**/docs", "**/docs/**"]);
    }

    #[test]
    fn expand_include_directory_leaves_other_patterns_alone() {
        for pattern in [
            "*.md",
            "docs/*",
            "docs/**",
            "docs/**/",
            "d?cs",
            "docs/[ab]",
            "docs/{public,private}",
            "!drafts",
            "#docs",
            "docs\\",
            "",
            " ",
            "\t",
        ] {
            assert_eq!(expand_include_directory(pattern), [pattern], "{pattern:?}");
        }
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
        let matchers = ExcludeMatchers::new(&["drafts".to_string()]);
        assert!(!matchers.excludes_file(None, Path::new("/home/dev/proj/drafts/note.md")));
        assert!(matchers.excludes_file(Some("drafts/note.md"), Path::new("/home/dev/proj/drafts/note.md")));

        // An absolute pattern makes the absolute path worth matching, and the
        // floating `drafts` must still not match a directory above the root.
        let matchers = ExcludeMatchers::new(&["drafts".to_string(), "/unrelated".to_string()]);
        let note = Path::new("/home/dev/drafts/site/note.md");
        assert_eq!(matchers.matched_pattern_for_file(None, note), None);
        assert_eq!(matchers.matched_pattern_for_file(Some("note.md"), note), None);
        assert_eq!(
            matchers.matched_pattern_for_file(Some("drafts/note.md"), Path::new("/home/dev/site/drafts/note.md")),
            Some("drafts/**")
        );
        assert_eq!(
            matchers.matched_pattern_for_file(None, Path::new("/unrelated/note.md")),
            Some("/unrelated/**")
        );
    }

    #[test]
    fn exclude_matchers_match_a_wildcard_within_one_path_component() {
        let matchers = ExcludeMatchers::new(&["sub/a*b".to_string(), "notes/a?b.md".to_string()]);
        assert!(matchers.is_match("sub/aXb/y.md"));
        assert!(matchers.is_match("sub/ab/y.md"));
        assert!(!matchers.is_match("sub/a/keep/b/x.md"));
        assert!(matchers.is_match("notes/aXb.md"));
        assert!(!matchers.is_match("notes/a/b.md"));

        let matchers = ExcludeMatchers::new(&["docs/**/draft.md".to_string()]);
        assert!(matchers.is_match("docs/draft.md"));
        assert!(matchers.is_match("docs/a/b/draft.md"));
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
        assert_eq!(
            path_relative_to(&base.join("docs/new/ghost.md"), &base).as_deref(),
            Some("docs/new/ghost.md"),
            "a file that does not exist yet relativizes the way it will once written"
        );
        assert_eq!(
            path_relative_to(&base.join("docs/a.md"), &base.join("missing")),
            None,
            "a base that does not exist contains nothing"
        );
    }

    #[test]
    fn resolve_for_matching_spells_a_missing_file_the_way_it_will_canonicalize() {
        let temp = tempdir().unwrap();
        let root = canonicalize_for_matching(temp.path()).unwrap();
        fs::create_dir_all(root.join("docs")).unwrap();
        fs::write(root.join("docs").join("x.md"), "# x").unwrap();

        // `temp.path()` is not canonical on macOS (`/var` -> `/private/var`), so
        // every row below also proves the existing ancestor was canonicalized.
        let cases = [
            ("docs/x.md", root.join("docs").join("x.md")),
            ("docs/ghost.md", root.join("docs").join("ghost.md")),
            (
                "docs/new/dir/ghost.md",
                root.join("docs").join("new").join("dir").join("ghost.md"),
            ),
            ("missing/../docs/./x.md", root.join("docs").join("x.md")),
            ("docs/missing/../../top.md", root.join("top.md")),
        ];
        for (relative, expected) in cases {
            assert_eq!(
                resolve_for_matching(&temp.path().join(relative)),
                expected,
                "{relative}"
            );
        }
    }

    #[test]
    fn resolve_for_matching_takes_a_relative_path_from_the_working_directory() {
        let cwd = canonicalize_for_matching(&std::env::current_dir().unwrap()).unwrap();
        assert_eq!(
            resolve_for_matching(Path::new("no-such-dir-for-matching/ghost.md")),
            cwd.join("no-such-dir-for-matching").join("ghost.md")
        );
        assert_eq!(resolve_for_matching(Path::new("./Cargo.toml")), cwd.join("Cargo.toml"));
    }

    #[cfg(unix)]
    #[test]
    fn resolve_for_matching_follows_a_symlink_reached_through_a_missing_directory() {
        let temp = tempdir().unwrap();
        let root = canonicalize_for_matching(temp.path()).unwrap();
        fs::create_dir_all(root.join("real")).unwrap();
        std::os::unix::fs::symlink(root.join("real"), root.join("link")).unwrap();

        assert_eq!(
            resolve_for_matching(&root.join("link").join("ghost.md")),
            root.join("real").join("ghost.md")
        );
        // Lexically `missing/../link/ghost.md` is `link/ghost.md`, and `link`
        // is only resolved by looking again once the `..` is gone.
        assert_eq!(
            resolve_for_matching(&root.join("missing").join("..").join("link").join("ghost.md")),
            root.join("real").join("ghost.md")
        );
    }

    fn sorted(mut patterns: Vec<String>) -> Vec<String> {
        patterns.sort();
        patterns
    }

    #[test]
    fn expand_braces_yields_every_alternative() {
        assert_eq!(
            sorted(expand_braces("/{var/folders,tmp}/**")),
            vec!["/tmp/**", "/var/folders/**"]
        );
        // Nesting expands too.
        assert_eq!(sorted(expand_braces("a{b,{c,d}}e")), vec!["abe", "ace", "ade"]);
        // An empty alternative is dropped, as globset drops it.
        assert_eq!(sorted(expand_braces("x{,y}")), vec!["xy"]);
        // Several groups multiply out.
        assert_eq!(
            sorted(expand_braces("/{a,b}/{c,d}.md")),
            vec!["/a/c.md", "/a/d.md", "/b/c.md", "/b/d.md"]
        );
    }

    #[test]
    fn expand_braces_leaves_patterns_it_cannot_split() {
        // Nothing to split.
        assert_eq!(expand_braces("/var/folders/**"), vec!["/var/folders/**"]);
        // An unclosed brace is not an alternation.
        assert_eq!(expand_braces("/var/{a,b/**"), vec!["/var/{a,b/**"]);
        // A comma inside a character class is literal.
        assert_eq!(expand_braces("/var/[a,b]/**"), vec!["/var/[a,b]/**"]);
        // An alternation of nothing but empty alternatives is not a split.
        assert_eq!(expand_braces("x{,}"), vec!["x{,}"]);
        // Past the expansion cap the pattern is left alone: 2^7 = 128 > 64.
        let wide = "/{a,b}{a,b}{a,b}{a,b}{a,b}{a,b}{a,b}/**";
        assert_eq!(expand_braces(wide), vec![wide]);
    }

    #[test]
    fn has_absolute_spelling_sees_past_a_leading_alternation() {
        assert!(has_absolute_spelling("/var/folders/**"));
        assert!(has_absolute_spelling("{/opt,/srv}/docs/**"));
        assert!(has_absolute_spelling("/{var/folders,tmp}/**"));
        assert!(!has_absolute_spelling("docs/**"));
        assert!(!has_absolute_spelling("{docs,notes}/**"));
    }

    #[test]
    fn expand_braces_agrees_with_globset() {
        // The expansion only discovers prefixes to canonicalize, but a
        // disagreement with globset would mean it is describing a different
        // pattern than the one that decides matches.
        let cases = [
            ("/{var/folders,tmp}/**", "/tmp/note.md"),
            ("/{var/folders,tmp}/**", "/var/folders/x/note.md"),
            ("/{var/folders,tmp}/**", "/opt/note.md"),
            ("/{a,b}/{c,d}.md", "/b/d.md"),
            ("/{a,b}/{c,d}.md", "/b/e.md"),
            ("x{,y}", "x"),
            ("x{,y}", "xy"),
            ("/var/[a,b]/**", "/var/a/n.md"),
            ("/var/[a,b]/**", "/var/,/n.md"),
            ("/var/folders/**", "/var/folders/n.md"),
        ];
        for (pattern, path) in cases {
            let direct = Glob::new(pattern).unwrap().compile_matcher().is_match(path);
            let expanded = expand_braces(pattern)
                .iter()
                .any(|p| Glob::new(p).unwrap().compile_matcher().is_match(path));
            assert_eq!(direct, expanded, "pattern {pattern} against {path}");
        }
    }

    #[test]
    fn literal_path_prefix_stops_at_the_first_wildcard() {
        assert_eq!(literal_path_prefix("/var/folders/**"), Some("/var/folders"));
        assert_eq!(literal_path_prefix("/var/log/app*.md"), Some("/var/log"));
        assert_eq!(literal_path_prefix("/var/note.md"), Some("/var/note.md"));
        assert_eq!(literal_path_prefix("/var/"), Some("/var"));
        assert_eq!(literal_path_prefix("docs/**"), Some("docs"));
        // Nothing literal to resolve.
        assert_eq!(literal_path_prefix("/**"), None);
        assert_eq!(literal_path_prefix("/{var,tmp}/**"), None);
        assert_eq!(literal_path_prefix("**/note.md"), None);
        // The result is always a prefix slice, so a remainder re-attaches by
        // byte offset.
        let pattern = "/var/folders/**";
        let prefix = literal_path_prefix(pattern).unwrap();
        assert_eq!(&pattern[prefix.len()..], "/**");
    }

    /// `(real directory, symlink to it)` under a fresh temp dir. The symlink is
    /// how a pattern spells the location; the real directory is where a file
    /// canonicalizes to.
    #[cfg(unix)]
    fn symlinked_dir(temp: &Path) -> (PathBuf, PathBuf) {
        let real = temp.join("real");
        fs::create_dir_all(real.join("notes")).unwrap();
        fs::write(real.join("notes/scratch.md"), "# Note\n").unwrap();
        let link = temp.join("link");
        std::os::unix::fs::symlink(&real, &link).unwrap();
        (canonicalize_for_matching(&real).unwrap(), link)
    }

    #[cfg(unix)]
    #[test]
    fn path_aliases_spell_a_path_the_way_a_pattern_named_it() {
        let temp = tempdir().unwrap();
        let (real, link) = symlinked_dir(temp.path());
        let pattern = format!("{}/notes/**", link.to_string_lossy());

        let aliases = PathAliases::new([pattern.as_str()]);
        assert!(!aliases.is_empty());
        assert_eq!(
            aliases.spellings_of(&real.join("notes/scratch.md")),
            vec![format!("{}/notes/scratch.md", link.to_string_lossy())]
        );
        // The alias is what makes the pattern match the canonical path.
        let matcher = Glob::new(&pattern).unwrap().compile_matcher();
        assert!(!matcher.is_match(real.join("notes/scratch.md")), "negative control");
        assert!(
            aliases
                .spellings_of(&real.join("notes/scratch.md"))
                .iter()
                .any(|alias| matcher.is_match(alias))
        );
        // A path outside the recorded prefix has no alias.
        assert!(aliases.spellings_of(Path::new("/somewhere/else/note.md")).is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn path_aliases_reach_through_a_brace_alternation() {
        // The prefix only exists once the alternation is expanded.
        let temp = tempdir().unwrap();
        let (real, link) = symlinked_dir(temp.path());
        let pattern = format!("{{/nowhere,{}}}/notes/**", link.to_string_lossy());

        let aliases = PathAliases::new([pattern.as_str()]);
        let matcher = Glob::new(&pattern).unwrap().compile_matcher();
        let note = real.join("notes/scratch.md");
        assert!(!matcher.is_match(&note), "negative control");
        assert!(aliases.spellings_of(&note).iter().any(|alias| matcher.is_match(alias)));
    }

    #[test]
    fn path_aliases_are_empty_without_a_symlinked_prefix() {
        let temp = tempdir().unwrap();
        let canonical = canonicalize_for_matching(temp.path()).unwrap();
        let canonical = canonical.to_string_lossy().replace('\\', "/");
        // Relative patterns, absolute patterns that already name their real
        // location, and prefixes that do not exist all record nothing.
        for pattern in ["docs/**", &format!("{canonical}/docs/**"), "/nonexistent/xyz/**"] {
            assert!(
                PathAliases::new([pattern]).is_empty(),
                "pattern {pattern} should record no alias"
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn exclude_matchers_match_a_file_a_pattern_named_through_a_symlink() {
        let temp = tempdir().unwrap();
        let (real, link) = symlinked_dir(temp.path());
        let note = real.join("notes/scratch.md");

        let matchers = ExcludeMatchers::new(&[format!("{}/notes/**", link.to_string_lossy())]);
        assert!(matchers.excludes_file(None, &note));

        // Negative controls: a sibling the pattern does not name, and a pattern
        // pointing somewhere else entirely.
        fs::write(real.join("other.md"), "# Other\n").unwrap();
        assert!(!matchers.excludes_file(None, &real.join("other.md")));
        let elsewhere = ExcludeMatchers::new(&[format!("{}/elsewhere/**", link.to_string_lossy())]);
        assert!(!elsewhere.excludes_file(None, &note));
    }

    #[cfg(unix)]
    #[test]
    fn normalize_pattern_for_base_strips_a_pattern_written_through_a_symlink() {
        let temp = tempdir().unwrap();
        let (real, link) = symlinked_dir(temp.path());
        let pattern = format!("{}/notes/*.md", link.to_string_lossy());

        assert_eq!(normalize_pattern_for_base(&pattern, Some(&real)), "notes/*.md");
        // A pattern naming a different location through the same symlink is
        // still outside a narrower base, and stays absolute.
        let outside = format!("{}/elsewhere/*.md", link.to_string_lossy());
        assert_eq!(normalize_pattern_for_base(&outside, Some(&real.join("notes"))), outside);
    }

    #[test]
    fn classify_file_reads_the_whole_name() {
        let cases = [
            ("README.md", FileKind::Markdown),
            ("README.MD", FileKind::Markdown),
            ("notes.txt", FileKind::Markdown),
            ("LICENSE", FileKind::Markdown),
            ("rules.mdc", FileKind::Markdown),
            ("page.html", FileKind::Markdown),
            ("lib.rs", FileKind::RustDocComments),
            // Only the spelling rustc and cargo use selects doc-comment linting.
            ("LIB.RS", FileKind::SourceCode),
            ("main.swift", FileKind::SourceCode),
            ("Main.KT", FileKind::SourceCode),
            ("config.yaml", FileKind::SourceCode),
            ("template.md.jinja", FileKind::Markdown),
            ("README.md.j2.in", FileKind::Markdown),
            ("README.md.swift", FileKind::Markdown),
            ("script.litcoffee", FileKind::Markdown),
            ("readme.swift.txt", FileKind::Markdown),
            // A leading dot names a hidden file: `.md` is not an extension here.
            (".md.swift", FileKind::SourceCode),
        ];
        for (name, expected) in cases {
            assert_eq!(classify_file(Path::new(name)), expected, "{name}");
            assert_eq!(
                classify_file(&Path::new("dir.md").join(name)),
                expected,
                "dir.md/{name}"
            );
        }
    }

    #[test]
    fn source_code_extensions_are_sorted_and_exclude_document_formats() {
        let extensions = crate::linguist_data::SOURCE_CODE_EXTENSIONS;
        assert!(extensions.windows(2).all(|pair| pair[0] < pair[1]));
        for document in ["md", "txt", "asc", "rst", "html", "mdx", "ipynb", "tex"] {
            assert!(!crate::linguist_data::is_source_code_extension(document), "{document}");
        }
        for source in ["rs", "swift", "kt", "dart", "java", "py", "json", "SWIFT"] {
            assert!(crate::linguist_data::is_source_code_extension(source), "{source}");
        }
    }

    /// The verdict `ignore` gives an `include` pattern on a top-level name, the
    /// way the walker builds its include override.
    #[cfg(not(windows))]
    fn include_override_matches(pattern: &str, name: &str) -> bool {
        let mut builder = ignore::overrides::OverrideBuilder::new(Path::new("/root"));
        builder.add(pattern).unwrap();
        builder
            .build()
            .unwrap()
            .matched(Path::new("/root").join(name), false)
            .is_whitelist()
    }

    /// An explicit include is matched twice, by the walker's override and by
    /// `ExplicitIncludeMatchers`. A backslash is an escape in the first on every
    /// platform, so it has to be one in the second, or a file the walk found is
    /// dropped (or one it skipped is kept) on Windows only.
    #[test]
    #[cfg(not(windows))]
    fn explicit_include_matchers_and_walker_overrides_agree_on_escaped_patterns() {
        let names = ["!bang.md", "[x].md", "x.md", "a*b.md", "axb.md", r"\!bang.md"];
        for pattern in [r"\!bang.md", r"\[x\].md", r"a\*b.md", "[x].md", "a*b.md"] {
            let matchers = ExplicitIncludeMatchers::new(&[pattern.to_string()]);
            assert!(!matchers.is_empty(), "{pattern} is an explicit file include");
            for name in names {
                assert_eq!(
                    matchers.matches_relative_path(name),
                    include_override_matches(pattern, name),
                    "include {pattern} on {name}: matchers vs walker"
                );
            }
        }
        // Positive controls: the escaped patterns do select their literal file.
        for (pattern, name) in [
            (r"\!bang.md", "!bang.md"),
            (r"\[x\].md", "[x].md"),
            (r"a\*b.md", "a*b.md"),
        ] {
            let matchers = ExplicitIncludeMatchers::new(&[pattern.to_string()]);
            assert!(matchers.matches_relative_path(name), "{pattern} should match {name}");
            assert!(include_override_matches(pattern, name), "walker: {pattern} on {name}");
        }
        // Negative controls: an escaped metacharacter is not a wildcard or a class.
        let escaped_class = ExplicitIncludeMatchers::new(&[r"\[x\].md".to_string()]);
        assert!(!escaped_class.matches_relative_path("x.md"));
        let escaped_star = ExplicitIncludeMatchers::new(&[r"a\*b.md".to_string()]);
        assert!(!escaped_star.matches_relative_path("axb.md"));
    }

    /// A backslash inside a character class is a literal there (`globset`), so
    /// the class `[a\]` ends at the `]` and the comma after it separates two
    /// alternatives. Skipping the byte after every backslash would read `\]` as an
    /// escape and swallow the rest of the pattern into the class.
    #[test]
    fn split_first_alternation_reads_a_backslash_inside_a_class_literally() {
        let (prefix, alternatives, suffix) = split_first_alternation(r"{[a\],/tmp}/notes/*.md").unwrap();
        assert_eq!(prefix, "");
        assert_eq!(alternatives, vec![r"[a\]", "/tmp"]);
        assert_eq!(suffix, "/notes/*.md");
        assert!(has_absolute_spelling(r"{[a\],/tmp}/notes/*.md"));
    }

    /// The first `]` after `[` (or `[!`) is a member of the class, not its end.
    #[test]
    fn split_first_alternation_reads_a_leading_close_bracket_as_a_class_member() {
        for pattern in [r"{[]a],/tmp}/*.md", r"{[!]a],/tmp}/*.md", r"{[^]a],/tmp}/*.md"] {
            let (_, alternatives, suffix) = split_first_alternation(pattern).unwrap();
            assert_eq!(alternatives.len(), 2, "{pattern}: {alternatives:?}");
            assert_eq!(alternatives[1], "/tmp", "{pattern}");
            assert_eq!(suffix, "/*.md", "{pattern}");
            assert!(has_absolute_spelling(pattern), "{pattern}");
            let glob = path_glob(pattern, false).unwrap().compile_matcher();
            assert!(glob.is_match("/tmp/x.md"), "{pattern}: globset agrees");
        }
        // A brace and a comma inside the class: only the leading `]` rule keeps
        // them from ending the alternation early.
        let pattern = "{[]},a],/tmp}/*.md";
        let (_, alternatives, suffix) = split_first_alternation(pattern).unwrap();
        assert_eq!(alternatives, vec!["[]},a]", "/tmp"]);
        assert_eq!(suffix, "/*.md");
        assert!(has_absolute_spelling(pattern));
        assert!(
            path_glob(pattern, false)
                .unwrap()
                .compile_matcher()
                .is_match("/tmp/x.md")
        );
    }

    /// Outside a class a backslash escapes the next character on every platform.
    #[test]
    fn split_first_alternation_treats_an_escaped_brace_as_a_literal() {
        assert!(
            split_first_alternation(r"a\{b,c}").is_none(),
            "an escaped brace opens nothing"
        );
        let (prefix, alternatives, suffix) = split_first_alternation(r"a\\{b,c}").unwrap();
        assert_eq!((prefix, alternatives, suffix), (r"a\\", vec!["b", "c"], ""));
        let (_, alternatives, _) = split_first_alternation(r"{a\,b,c}").unwrap();
        assert_eq!(alternatives, vec![r"a\,b", "c"], "an escaped comma does not split");
    }

    /// A component holding a backslash is not a plain literal on any platform:
    /// it is an escape sequence, and canonicalizing it as a path component would
    /// resolve the wrong name.
    #[test]
    fn literal_path_prefix_ends_at_a_component_holding_a_backslash() {
        assert_eq!(literal_path_prefix(r"/alias/foo\bar/x"), Some("/alias"));
        assert_eq!(literal_path_prefix(r"/alias/foo\[x\]/x"), Some("/alias"));
        assert_eq!(literal_path_prefix(r"foo\bar/x"), None);
        // A run that is only a drive designator names no directory.
        assert_eq!(literal_path_prefix(r"C:/foo\bar/*.tmpl"), None);
        assert_eq!(literal_path_prefix("/alias/plain/x"), Some("/alias/plain/x"));
        assert_eq!(literal_path_prefix("C:/**"), None);
        assert_eq!(literal_path_prefix("C:/proj/**"), Some("C:/proj"));
    }

    /// `path_glob` is the one construction every rumdl-side matcher shares, so
    /// its reading of `\` is pinned directly: an escape, never a separator.
    #[test]
    #[cfg(not(windows))]
    fn path_glob_reads_a_backslash_as_an_escape_and_keeps_the_separator_option() {
        let escaped = path_glob(r"\[x\].md", true).unwrap().compile_matcher();
        assert!(escaped.is_match("[x].md"));
        assert!(!escaped.is_match("x.md"));
        assert!(!escaped.is_match(r"\[x\].md"));
        assert!(!escaped.is_match("/[x/].md"), "a backslash is not a slash");

        let star = path_glob(r"a\*b.md", true).unwrap().compile_matcher();
        assert!(star.is_match("a*b.md"));
        assert!(!star.is_match("axb.md"));

        // `literal_separator` still decides whether `*` crosses a `/`.
        assert!(!path_glob("*.md", true).unwrap().compile_matcher().is_match("a/x.md"));
        assert!(path_glob("*.md", false).unwrap().compile_matcher().is_match("a/x.md"));

        // A dangling escape does not compile, as in the walker.
        assert!(path_glob(r"docs\", true).is_err());
    }

    #[test]
    fn trim_directory_slash_drops_an_escape_left_without_a_character() {
        assert_eq!(trim_directory_slash("docs"), "docs");
        assert_eq!(trim_directory_slash("docs/"), "docs");
        assert_eq!(trim_directory_slash("docs//"), "docs");
        assert_eq!(trim_directory_slash(r"docs\/"), "docs");
        // `ignore` drops one backslash unconditionally, leaving a dangling escape.
        assert_eq!(trim_directory_slash(r"docs\\/"), r"docs\");
        assert_eq!(trim_directory_slash(r"docs\\\/"), r"docs\\");
        // Several slashes: not the form `ignore` unescapes, so the backslash stays.
        assert_eq!(trim_directory_slash(r"docs\//"), r"docs\");
        // Without a trailing slash a dangling backslash is the caller's error.
        assert_eq!(trim_directory_slash(r"docs\"), r"docs\");
        assert_eq!(trim_directory_slash(r"a\/b/"), r"a\/b");
    }

    /// `docs\/` is a directory marker whose slash was escaped. The walker's
    /// overrides read it as the directory `docs`, so the matchers must too, or a
    /// named file under it is linted where the walk skipped it.
    #[test]
    #[cfg(not(windows))]
    fn an_escaped_trailing_slash_excludes_the_directory_like_the_walker() {
        let pattern = r"docs\/";
        assert!(exclude_pattern_compiles(pattern));
        let matchers = ExcludeMatchers::new(&[pattern.to_string()]);
        assert!(matchers.invalid.is_empty(), "{:?}", matchers.invalid);
        assert!(matchers.is_match("docs/x.md"));
        assert!(matchers.is_match("a/docs/x.md"));
        assert!(!matchers.is_match("other/x.md"), "negative control");

        let mut builder = ignore::overrides::OverrideBuilder::new(Path::new("/root"));
        for expanded in expand_directory_pattern(pattern) {
            builder.add(&exclude_override_rule(&expanded)).unwrap();
        }
        let walker = builder.build().unwrap();
        // The walk prunes an excluded directory, so the directory is what it
        // decides on; the matchers see the file beneath it.
        for (dir, file) in [
            ("docs", "docs/x.md"),
            ("a/docs", "a/docs/x.md"),
            ("other", "other/x.md"),
        ] {
            assert_eq!(
                matchers.is_match(file),
                walker.matched(Path::new("/root").join(dir), true).is_ignore(),
                "{dir}"
            );
        }
        assert_eq!(expand_directory_pattern(pattern), vec!["docs", "docs/**"]);
        // `ignore` drops one backslash with the slash, leaving a dangling escape it
        // rejects; the matchers reject the same patterns, wildcard or not.
        for dangling in [r"docs\\/", r"docs*\\/", r"docs\"] {
            let mut builder = ignore::overrides::OverrideBuilder::new(Path::new("/root"));
            assert!(
                builder.add(&exclude_override_rule(dangling)).is_err(),
                "{dangling}: walker"
            );
            assert!(!exclude_pattern_compiles(dangling), "{dangling}: pattern check");
            let matchers = ExcludeMatchers::new(&[dangling.to_string()]);
            assert!(!matchers.invalid.is_empty(), "{dangling}: matchers reject it");
            for file in [r"docs\/x.md", "docs/x.md", "docs/sub/x.md"] {
                assert!(!matchers.is_match(file), "{dangling}: matches nothing, not {file}");
            }
        }
        // After several slashes `ignore` keeps the backslash and accepts a rule
        // that can never match; here it is reported, and still excludes nothing.
        let several = ExcludeMatchers::new(&[r"docs\//".to_string()]);
        assert!(!several.invalid.is_empty());
        assert!(!several.is_match("docs/x.md") && !several.is_match(r"docs\/x.md"));
    }

    /// The walk's type filter is built with `globset`'s platform default, which
    /// reads `\` as a separator on Windows. A backslashed include name is
    /// therefore registered as `*`, on every platform, and the selector's own
    /// `keeps` makes the precise decision. With no include override in the way,
    /// the walk shows what the type filter alone admits.
    #[test]
    #[cfg(not(windows))]
    fn configure_types_admits_every_name_for_a_backslashed_explicit_include() {
        let dir = tempfile::tempdir().unwrap();
        for name in ["[x].txt", "x.txt", "y.md", "z.png"] {
            fs::write(dir.path().join(name), "").unwrap();
        }
        let walked = |includes: &[String]| {
            let selector = LintablePathSelector::new(Some(dir.path()), includes, LintableFileMode::Markdown);
            let mut builder = ignore::WalkBuilder::new(dir.path());
            selector.configure_types(&mut builder).unwrap();
            let mut names: Vec<String> = builder
                .build()
                .filter_map(Result::ok)
                .filter(|entry| entry.file_type().is_some_and(|kind| kind.is_file()))
                .map(|entry| entry.file_name().to_string_lossy().into_owned())
                .collect();
            names.sort();
            names
        };
        assert_eq!(
            walked(&[r"\[x\].txt".to_string()]),
            ["[x].txt", "x.txt", "y.md", "z.png"],
            "the escaped name widens the type filter to every file"
        );
        // Control: a plain name registers as itself and keeps the filter narrow.
        assert_eq!(walked(&["x.txt".to_string()]), ["x.txt", "y.md"]);
        // And `keeps` is the precise gate the widened filter hands over to.
        let selector = LintablePathSelector::new(
            Some(dir.path()),
            &[r"\[x\].txt".to_string()],
            LintableFileMode::Markdown,
        );
        assert!(selector.keeps(&dir.path().join("[x].txt")));
        assert!(!selector.keeps(&dir.path().join("x.txt")));
        assert!(!selector.keeps(&dir.path().join("z.png")));
    }

    #[test]
    #[cfg(not(windows))]
    fn type_filter_glob_widens_a_name_holding_a_backslash() {
        assert_eq!(type_filter_glob("*.md.jinja"), "*.md.jinja");
        assert_eq!(type_filter_glob(r"\[x\].txt"), "*");
        assert_eq!(type_filter_glob(r"a\*b.md"), "*");
    }

    /// The strip must agree with a path-component strip wherever the pattern
    /// holds no backslash, so the textual rewrite loses nothing the path-based
    /// one handled.
    #[test]
    fn strip_base_text_agrees_with_a_path_strip_when_there_is_no_backslash() {
        let legacy = |pattern: &str, base: &Path| {
            Path::new(pattern)
                .strip_prefix(base)
                .ok()
                .map(|rest| rest.to_string_lossy().into_owned())
        };
        let root = Path::new("/root");
        let cases = [
            "/root/docs/**",
            "/root/./docs",
            "/root//docs",
            "/root/docs/",
            "/root/docs/**/.",
            "/root/docs/../../.",
            "/./root/.//",
            "/root",
            "/root/",
            "/rooted/docs",
            "/other/docs",
            "docs/**",
            "/root/a/b/c.md",
        ];
        for pattern in cases {
            assert_eq!(
                strip_base_text(pattern, root).map(str::to_string),
                legacy(pattern, root),
                "{pattern}"
            );
        }
        for pattern in ["/docs/**", "/", "/docs/", "/docs/./x"] {
            assert_eq!(
                strip_base_text(pattern, Path::new("/")).map(str::to_string),
                legacy(pattern, Path::new("/")),
                "root base: {pattern}"
            );
        }
    }

    /// A backslash is never a separator for the base: `/root\[x\]` is a different
    /// name from `/root`, and an escape in the remainder survives the strip.
    #[test]
    fn strip_base_text_keeps_escapes_and_never_treats_a_backslash_as_a_boundary() {
        let root = Path::new("/root");
        assert_eq!(strip_base_text(r"/root\[x\]", root), None);
        assert_eq!(strip_base_text(r"/root\docs", root), None);
        assert_eq!(strip_base_text(r"/root/\[x\].md", root), Some(r"\[x\].md"));
        assert_eq!(
            strip_base_text(r"/root/docs/\[notes\].tmpl", root),
            Some(r"docs/\[notes\].tmpl")
        );
        assert_eq!(strip_base_text(r"/root/docs\/", root), Some("docs"));
        assert_eq!(
            strip_base_text(r"/root/docs\/.", root),
            Some(r"docs\/."),
            "an escaped slash is no component"
        );
    }

    #[test]
    #[cfg(not(windows))]
    fn normalize_pattern_for_base_keeps_escapes_in_the_rewritten_pattern() {
        let root = Path::new("/root");
        assert_eq!(
            normalize_pattern_for_base(r"/root/docs/\[notes\].tmpl", Some(root)),
            r"docs/\[notes\].tmpl"
        );
        assert_eq!(
            normalize_pattern_for_base(r"/root/\!bang.md", Some(root)),
            r"/\!bang.md"
        );
        // The rewritten include still selects the escaped name.
        let matcher = ExplicitIncludeMatchers::new(&[normalize_pattern_for_base("/root/./docs/*.tmpl", Some(root))]);
        assert!(matcher.matches_relative_path("docs/x.tmpl"));
        assert!(!matcher.matches_relative_path("other/x.tmpl"));
    }

    fn dots(pattern: &str) -> String {
        normalize_dot_segments(pattern, false).into_owned()
    }

    fn dots_windows(pattern: &str) -> String {
        normalize_dot_segments(pattern, true).into_owned()
    }

    #[test]
    fn dot_segments_drop_dot_components_and_cancel_literal_dot_dot() {
        for (pattern, expected) in [
            ("./dir", "dir"),
            ("dir/./x", "dir/x"),
            ("./dir/", "dir/"),
            ("dir/.", "dir"),
            ("dir/./", "dir/"),
            ("a/b/../c", "a/c"),
            ("a/../b/../c", "c"),
            ("a/b/../../dir/**", "dir/**"),
            ("nonexist/../dir/**", "dir/**"),
            ("a//b", "a/b"),
            ("a//../b", "b"),
            ("*/x/../y", "*/y"),
            ("**/./x", "**/x"),
            ("dir/plain", "dir/plain"),
        ] {
            assert_eq!(dots(pattern), expected, "{pattern}");
            assert_eq!(dots(expected), expected, "idempotent on {expected}");
        }
    }

    #[test]
    fn dot_segments_leave_what_cannot_be_resolved() {
        for pattern in [
            ".",
            "./",
            "a/..",
            "../x",
            "../../x",
            "*/../x",
            "dir/**/../x",
            "{a,b}/../x",
            r"a\*/../x",
            "x/[/../y",
            "x/{a/../y",
            "x/[/]/../y",
            "[!]/../x",
        ] {
            assert_eq!(dots(pattern), pattern, "{pattern}");
        }
        // A slash inside a class or a brace is not a component boundary.
        assert_eq!(dots("[a/b]/./x"), "[a/b]/x");
        assert_eq!(dots("{a/b,c}/./x"), "{a/b,c}/x");
    }

    #[test]
    fn dot_segments_resolve_an_absolute_prefix_lexically_only_when_it_is_missing() {
        let dots = |pattern: &str| normalize_dot_segments(pattern, cfg!(windows)).into_owned();
        let temp = tempdir().unwrap();
        let root = canonicalize_for_matching(temp.path()).unwrap();
        let root = root.to_string_lossy().replace('\\', "/");
        fs::create_dir_all(format!("{root}/a/b")).unwrap();

        assert_eq!(dots(&format!("{root}/./dir")), format!("{root}/dir"));
        assert_eq!(dots(&format!("{root}/nonexist/../dir")), format!("{root}/dir"));
        // The prefix exists, so the filesystem resolves the `..` (it may be a symlink).
        assert_eq!(dots(&format!("{root}/a/../dir")), format!("{root}/a/../dir"));
        assert_eq!(dots(&format!("{root}/a/b/../../dir")), format!("{root}/a/b/../../dir"));
        // After a wildcard or brace existence cannot be checked: every later `..` stays.
        for opaque in ["*", "{a,b}", r"\*"] {
            let pattern = format!("{root}/{opaque}/nonexist/../dir");
            assert_eq!(dots(&pattern), pattern, "{pattern}");
        }
        assert_eq!(dots("/zz-missing/x/../y"), "/zz-missing/y");
    }

    #[test]
    fn dot_segments_never_remove_a_root() {
        assert_eq!(dots("/.."), "/");
        assert_eq!(dots("/../x"), "/x");
        assert_eq!(dots("/zz-missing/../../x"), "/x");
        // Unix: a run of leading slashes is one root.
        assert_eq!(dots("//zz-missing/x/../y"), "/zz-missing/y");
        // Windows: a drive and a UNC share are roots; Unix reads `C:` as a name.
        assert_eq!(dots_windows("C:/proj/./docs"), "C:/proj/docs");
        assert_eq!(dots_windows("C:/../x"), "C:/x");
        assert_eq!(dots_windows("C:/.."), "C:/");
        assert_eq!(dots_windows("C:/."), "C:/");
        assert_eq!(dots_windows("C:/./"), "C:/");
        assert_eq!(dots_windows("C:/proj/.."), "C:/");
        assert_eq!(dots_windows("//server/share/../x"), "//server/share/x");
        assert_eq!(dots_windows("//?/C:/zz-missing/../x"), "//?/C:/x");
        assert_eq!(dots_windows("//server/share/./x"), "//server/share/x");
        assert_eq!(dots("C:/../x"), "x");
    }

    #[test]
    fn dot_segments_leave_a_symlinked_prefix_to_the_filesystem() {
        #[cfg(unix)]
        {
            let temp = tempdir().unwrap();
            let root = temp.path().canonicalize().unwrap();
            fs::create_dir_all(root.join("a/b")).unwrap();
            fs::create_dir_all(root.join("a/dir")).unwrap();
            std::os::unix::fs::symlink(root.join("a/b"), root.join("link")).unwrap();
            let pattern = format!("{}/link/../dir", root.display());
            assert_eq!(dots(&pattern), pattern, "link/.. is a/, not the root");
            let aliases = PathAliases::new([pattern.as_str()]);
            assert!(!aliases.is_empty(), "the filesystem still resolves it for the aliases");
            let matchers = ExcludeMatchers::new(&[format!("{}/link/../dir/**", root.display())]);
            fs::write(root.join("a/dir/e.md"), "x").unwrap();
            fs::create_dir_all(root.join("dir")).unwrap();
            fs::write(root.join("dir/f.md"), "x").unwrap();
            assert!(matchers.excludes_file(None, &root.join("a/dir/e.md")));
            assert!(!matchers.excludes_file(None, &root.join("dir/f.md")));
        }
    }

    #[test]
    fn a_dot_dot_that_cannot_resolve_still_matches_through_another_patterns_alias() {
        // The aliases are shared across patterns, so this pair excludes a file
        // neither excludes alone; unresolved `..` must not be rewritten away.
        #[cfg(unix)]
        {
            let temp = tempdir().unwrap();
            let root = temp.path().canonicalize().unwrap();
            fs::create_dir_all(root.join("a")).unwrap();
            fs::create_dir_all(root.join("docs")).unwrap();
            fs::write(root.join("docs/x.md"), "x").unwrap();
            let first = format!("{}/*/../docs/**", root.display());
            let second = format!("{}/a/../*.tmp.md", root.display());
            let file = root.join("docs/x.md");
            assert!(!ExcludeMatchers::new(std::slice::from_ref(&first)).excludes_file(None, &file));
            assert!(!ExcludeMatchers::new(std::slice::from_ref(&second)).excludes_file(None, &file));
            let both = ExcludeMatchers::new(&[first, second]);
            assert!(both.invalid.is_empty(), "{:?}", both.invalid);
            assert!(both.excludes_file(None, &file));
        }
    }

    #[test]
    fn a_dotted_exclude_stays_anchored_in_the_walker_and_the_matchers() {
        assert_eq!(expand_directory_pattern_for("./dir", false), ["/dir", "/dir/**"]);
        assert_eq!(expand_directory_pattern_for("./dir/", false), ["/dir", "/dir/**"]);
        assert_eq!(expand_directory_pattern_for("./a/b", false), ["a/b", "a/b/**"]);
        assert_eq!(expand_directory_pattern_for("dir/../dir", false), ["/dir", "/dir/**"]);
        assert_eq!(expand_directory_pattern_for("./*.md", false), ["/*.md"]);
        // Control: floating patterns are left to float.
        assert_eq!(expand_directory_pattern_for("dir", false), ["dir", "dir/**"]);
        assert_eq!(expand_directory_pattern_for("*.md", false), ["*.md"]);

        let names = [
            "top.md",
            "other/top.md",
            "dir/a.md",
            "other/dir/a.md",
            "dir/sub/b.md",
            "a/b/d.md",
        ];
        for pattern in [
            "./top.md",
            "top.md",
            "./dir",
            "dir/./sub",
            "a/../dir/",
            "./*.md",
            "./a/b",
        ] {
            let matchers = ExcludeMatchers::new(&[pattern.to_string()]);
            assert!(matchers.invalid.is_empty(), "{pattern}: {:?}", matchers.invalid);
            let mut builder = ignore::overrides::OverrideBuilder::new(Path::new("/root"));
            for entry in expand_directory_pattern(pattern) {
                builder.add(&exclude_override_rule(&entry)).unwrap();
            }
            let walker = builder.build().unwrap();
            for name in names {
                let by_walker = walker.matched(Path::new("/root").join(name), false).is_ignore();
                assert_eq!(
                    matchers.is_match(name),
                    by_walker,
                    "pattern {pattern} on {name}: matchers vs walker"
                );
            }
        }
        let anchored = ExcludeMatchers::new(&["./top.md".to_string()]);
        assert!(anchored.is_match("top.md"));
        assert!(!anchored.is_match("other/top.md"), "./top.md must not float");
        let floating = ExcludeMatchers::new(&["top.md".to_string()]);
        assert!(floating.is_match("other/top.md"));
    }

    #[test]
    fn include_normalization_keeps_anchoring_and_guards_override_lines() {
        let n = |pattern: &str| normalize_pattern_for_base_for(pattern, None, false);
        assert_eq!(n("./top.md"), "/top.md");
        assert_eq!(n("./dir/"), "/dir/");
        assert_eq!(n("dir/./x"), "dir/x");
        assert_eq!(n("./a/b"), "a/b");
        assert_eq!(n("top.md"), "top.md");
        assert_eq!(n("!./top.md"), "!/top.md");
        assert_eq!(n("!./dir/x"), "!dir/x");
        // A positive include that already is a comment stays one; a pattern that
        // would turn into a comment or a negation is left as written.
        assert_eq!(n("#dir/../docs/**"), "#dir/../docs/**");
        assert_eq!(n("a/../#x"), "a/../#x");
        assert_eq!(n("./!x"), "./!x");
        // Behind a negation marker `#` and `!` are ordinary name characters.
        assert_eq!(n("!#dir/../docs/**"), "!docs/**");
        assert_eq!(n("!./!x/**"), "!!x/**");
        for pattern in ["./top.md", "!./dir/x", "dir/../#x", "./dir/"] {
            let once = n(pattern);
            assert_eq!(n(&once), once, "idempotent on {pattern}");
        }
    }

    #[test]
    fn include_normalization_resolves_dots_in_an_absolute_pattern_under_the_base() {
        let temp = tempdir().unwrap();
        let root = canonicalize_for_matching(temp.path()).unwrap();
        fs::create_dir_all(root.join("docs")).unwrap();
        let text = root.to_string_lossy().replace('\\', "/");
        let n = |suffix: &str| normalize_pattern_for_base_for(&format!("{text}/{suffix}"), Some(&root), cfg!(windows));
        assert_eq!(n("./docs/**"), "docs/**");
        assert_eq!(n("nonexist/../docs/**"), "docs/**");
        assert_eq!(n("docs/../notes/**"), "notes/**");
        // The base strip drops a trailing `/`, as it always has.
        assert_eq!(n("docs/./../notes/"), "/notes");
        let explicit = ExplicitIncludeMatchers::new(&[n("x.tmpl")]);
        assert!(explicit.matches_relative_path("x.tmpl"));
        assert!(!explicit.matches_relative_path("a/x.tmpl"));
        // A single component keeps the root anchor an absolute pattern had.
        assert_eq!(n("top.md"), "/top.md");
        assert_eq!(n("docs/../top.md"), "/top.md");
        assert_eq!(n("*.md"), "/*.md");
        assert_eq!(n("dir/top.md"), "dir/top.md");
        // Outside the base: left absolute, nothing under the walk matches it.
        assert_eq!(n("../elsewhere/**"), format!("{text}/../elsewhere/**"));
        assert_eq!(n("docs/../../elsewhere/**"), format!("{text}/docs/../../elsewhere/**"));
    }

    #[test]
    fn include_normalization_resolves_dots_under_a_base_holding_metacharacters() {
        let temp = tempdir().unwrap();
        let base = canonicalize_for_matching(temp.path()).unwrap().join("project[1]");
        fs::create_dir_all(base.join("sub")).unwrap();
        let text = base.to_string_lossy().replace('\\', "/");
        let n = |suffix: &str| normalize_pattern_for_base_for(&format!("{text}/{suffix}"), Some(&base), cfg!(windows));
        assert_eq!(n("nonexist/../docs/**"), "docs/**");
        assert_eq!(n("sub/../docs/**"), "docs/**");
        assert_eq!(n("./docs/**"), "docs/**");
        assert_eq!(n("../out/**"), format!("{text}/../out/**"));
    }

    #[cfg(unix)]
    #[test]
    fn include_normalization_follows_a_symlink_before_dot_dot() {
        let temp = tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        fs::create_dir_all(root.join("x/y")).unwrap();
        std::os::unix::fs::symlink(root.join("x/y"), root.join("link")).unwrap();
        let pattern = format!("{}/link/../docs/**", root.display());
        assert_eq!(
            normalize_pattern_for_base_for(&pattern, Some(&root), false),
            "x/docs/**"
        );
    }

    #[test]
    fn per_file_keys_normalize_dots_and_keep_a_bang_as_a_name_character() {
        assert_eq!(prepare_pattern_for("./dir/x.md", false), "dir/x.md");
        assert_eq!(prepare_pattern_for("a/../dir/x.md", false), "dir/x.md");
        assert_eq!(prepare_pattern_for("!dir/../f.md", false), "f.md");
        // `!.` is an ordinary name here, not a `.` component.
        assert_eq!(prepare_pattern_for("!./f.md", false), "!./f.md");
        assert_eq!(prepare_pattern_for("a/!x/../f.md", false), "a/f.md");
        assert_eq!(prepare_pattern_for("../x.md", false), "../x.md");
    }

    #[test]
    fn windows_reads_a_backslash_as_a_separator_and_unix_as_an_escape() {
        // Windows: separators, so the directory forms work.
        assert_eq!(prepare_pattern_for(r"docs\**\", true), "docs/**/");
        assert_eq!(prepare_pattern_for(r"docs\sub\*.md", true), "docs/sub/*.md");
        assert_eq!(prepare_pattern_for(r"..\x", true), "../x");
        assert_eq!(prepare_pattern_for(r".\docs\..\notes", true), "notes");
        assert_eq!(prepare_pattern_for(r"C:\proj\.\docs", true), "C:/proj/docs");
        assert_eq!(expand_directory_pattern_for(r"docs\**\", true), ["docs/**/"]);
        assert_eq!(
            expand_directory_pattern_for(r"docs\sub", true),
            ["docs/sub", "docs/sub/**"]
        );
        assert_eq!(expand_directory_pattern_for(r".\dir", true), ["/dir", "/dir/**"]);
        assert_eq!(normalize_pattern_for_base_for(r".\docs\*.md", None, true), "docs/*.md");
        assert_eq!(normalize_pattern_for_base_for(r"!.\docs\x", None, true), "!docs/x");

        let separator = path_glob_for(r"docs\*.md", true, true).unwrap().compile_matcher();
        assert!(separator.is_match("docs/a.md"));
        assert!(
            path_glob_for(r"docs\", true, true).is_ok(),
            "no dangling escape on Windows"
        );
        let matchers = ExcludeMatchers::for_platform(&[r"docs\**".to_string()], true);
        assert!(matchers.invalid.is_empty(), "{:?}", matchers.invalid);
        assert!(matchers.is_match("docs/a.md"));
        assert!(matchers.is_match("docs/sub/b.md"));
        assert!(!matchers.is_match("other/a.md"));

        // A literal bang or metacharacter is spelled with a class or a brace.
        let bang = path_glob_for("{!}name.md", true, true).unwrap().compile_matcher();
        assert!(bang.is_match("!name.md"));
        let star = path_glob_for("a[*]b.md", true, true).unwrap().compile_matcher();
        assert!(star.is_match("a*b.md") && !star.is_match("axb.md"));
        // The walker's `ignore` overrides read the same spellings.
        let mut builder = ignore::overrides::OverrideBuilder::new(Path::new("/root"));
        for entry in expand_directory_pattern_for("{!}name.md", true)
            .into_iter()
            .chain(expand_directory_pattern_for("a[*]b.md", true))
        {
            builder.add(&exclude_override_rule(&entry)).unwrap();
        }
        let walker = builder.build().unwrap();
        for hit in ["!name.md", "a*b.md"] {
            assert!(walker.matched(Path::new("/root").join(hit), false).is_ignore(), "{hit}");
        }
        for miss in ["name.md", "axb.md"] {
            assert!(
                !walker.matched(Path::new("/root").join(miss), false).is_ignore(),
                "{miss}"
            );
        }

        // Unix: control, the backslash is still the escape.
        assert_eq!(prepare_pattern_for(r"docs\**", false), r"docs\**");
        assert_eq!(prepare_pattern_for(r"\!name", false), r"\!name");
        let escaped = path_glob_for(r"\!name.md", true, false).unwrap().compile_matcher();
        assert!(escaped.is_match("!name.md"));
        let unix = ExcludeMatchers::for_platform(&[r"docs\**".to_string()], false);
        assert!(!unix.is_match("docs/a.md"), "an escaped star names a file called docs*");
        assert!(path_glob_for(r"docs\", true, false).is_err());
    }

    #[test]
    fn windows_verbatim_patterns_are_unwrapped_before_glob_parsing() {
        for pattern in [r"\\?\C:\repo\docs\..\*.md", "//?/C:/repo/docs/../*.md"] {
            assert_eq!(prepare_pattern_for(pattern, true), "C:/repo/*.md");
            let matcher = path_glob_for(pattern, true, true).unwrap().compile_matcher();
            assert!(matcher.is_match("C:/repo/docs/../top.md"));
            assert!(!matcher.is_match("D:/repo/docs/../top.md"));
        }
        assert_eq!(
            prepare_pattern_for("//?/UNC/server/share/docs/../*.md", true),
            "//server/share/*.md"
        );
        assert_eq!(prepare_pattern_for("//?/C:/repo/./*.md", false), "/?/C:/repo/*.md");
    }

    #[test]
    fn path_aliases_see_through_a_dot_in_the_prefix() {
        #[cfg(unix)]
        {
            let temp = tempdir().unwrap();
            let root = temp.path().canonicalize().unwrap();
            fs::create_dir_all(root.join("real/docs")).unwrap();
            std::os::unix::fs::symlink(root.join("real"), root.join("alias")).unwrap();
            let aliases = PathAliases::new([format!("{}/./alias/docs/**", root.display()).as_str()]);
            assert_eq!(
                aliases.spellings_of(&root.join("real/docs/x.md")),
                vec![format!("{}/alias/docs/x.md", root.display())]
            );
        }
    }
}
