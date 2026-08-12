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

use globset::{Glob, GlobMatcher};
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
}

impl LintablePathSelector {
    pub fn new(base: Option<&Path>, includes: &[String], mode: LintableFileMode) -> Self {
        Self {
            base: base.map(Path::to_path_buf),
            explicit: ExplicitIncludeMatchers::new(includes),
            mode,
        }
    }

    /// Whether an included path is a source this adapter can interpret.
    pub fn keeps(&self, path: &Path) -> bool {
        if self.mode == LintableFileMode::Any {
            return true;
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

    /// Apply the corresponding coarse file-type filter to a discovery walk.
    /// [`Self::keeps`] remains the precise final gate because type filters only
    /// see file names, not root-relative include paths.
    pub fn configure_types(&self, builder: &mut ignore::WalkBuilder) -> Result<(), ignore::Error> {
        if self.mode == LintableFileMode::Any {
            return Ok(());
        }

        let mut types = ignore::types::TypesBuilder::new();
        types.add_defaults();
        for extension in MARKDOWN_EXTENSIONS {
            types.add("markdown", &any_case_extension_glob(extension))?;
        }
        types.select("markdown");
        if self.mode == LintableFileMode::MarkdownAndRust {
            types.add("rustdoc", "*.rs")?;
            types.select("rustdoc");
        }
        for glob in self.explicit.file_name_globs() {
            types.add("configinclude", glob)?;
        }
        if !self.explicit.is_empty() {
            types.select("configinclude");
        }
        builder.types(types.build()?);
        Ok(())
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
/// `.markdownlintignore` is honored for markdownlint compatibility.
///
/// The roots decide where gitignore reading stops, so a caller passes the same
/// ones it walks.
pub fn apply_markdown_walk_options<P: AsRef<Path>>(
    builder: &mut ignore::WalkBuilder,
    roots: &[P],
    options: &MarkdownWalkOptions,
) {
    let gitignore = options.respect_gitignore;
    builder
        .ignore(gitignore)
        .git_ignore(gitignore)
        .git_global(gitignore)
        .git_exclude(gitignore)
        .parents(gitignore)
        .hidden(false)
        // This setting does double duty in the walker: it gates gitignore
        // handling on a repository being present, and it is what stops the walk
        // reading gitignores above the repository root. Inside a repository both
        // are wanted. Outside one, requiring a repository would drop `.gitignore`
        // handling entirely, and there is no root to stop at in any case.
        .require_git(stops_at_repository_root(roots))
        .add_custom_ignore_filename(".markdownlintignore");

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
                if let Err(error) = builder.add(pattern) {
                    log::warn!("Invalid include pattern '{pattern}': {error}");
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
/// Pure string logic, compiled on every platform so it stays under test where
/// Windows is not available. Only the call sites are Windows-specific, and on
/// other platforms no path ever carries this prefix.
fn strip_verbatim_prefix(path: &str) -> Cow<'_, str> {
    // `\\?\UNC\server\share` -> `\\server\share`. The remainder already starts
    // with one separator, so restoring the UNC form needs one more prepended.
    if let Some(rest) = path.strip_prefix(r"\\?\UNC")
        && rest.starts_with('\\')
    {
        return Cow::Owned(format!(r"\{rest}"));
    }
    let Some(rest) = path.strip_prefix(r"\\?\") else {
        return Cow::Borrowed(path);
    };
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

/// The user's home directory, or `None` when it cannot be resolved.
///
/// Canonicalized for matching (see [`canonicalize_for_matching`]), falling
/// back to the path as reported when it cannot be canonicalized.
///
/// Wasm and WASI builds have no home directory to resolve, so patterns keep
/// their `~` there (see [`expand_home_prefix`]).
fn home_dir() -> Option<PathBuf> {
    #[cfg(feature = "native")]
    {
        use etcetera::{BaseStrategy, choose_base_strategy};
        choose_base_strategy()
            .ok()
            .map(|s| canonicalize_for_matching(s.home_dir()).unwrap_or_else(|| s.home_dir().to_path_buf()))
    }
    #[cfg(not(feature = "native"))]
    {
        None
    }
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
pub fn normalize_pattern_for_base(pattern: &str, base: Option<&Path>) -> String {
    let expanded = expand_home_prefix(pattern);
    let Some(base) = base else {
        return expanded.into_owned();
    };
    if !is_absolute_pattern(&expanded) {
        return expanded.into_owned();
    }

    // Try the base as given and canonicalized, so a symlinked or
    // non-canonical base (macOS `/var`, a Windows 8.3 short name) still strips.
    let path = Path::new(expanded.as_ref());
    let relative = path.strip_prefix(base).ok().or_else(|| {
        let canonical = canonicalize_for_matching(base)?;
        path.strip_prefix(canonical).ok()
    });
    match relative {
        Some(relative) => normalize_pattern_separators(relative.to_string_lossy()).into_owned(),
        None => expanded.into_owned(),
    }
}

/// Expands directory-style patterns to also match files within them.
/// Pattern "dir/path" becomes ["dir/path", "dir/path/**"] to match both
/// the directory itself and all contents recursively. A leading `~` is
/// expanded first (see [`expand_home_prefix`]).
///
/// The expansion is driven by the pattern's *final* component: it names a
/// directory only when it holds no wildcard. `docs/*` therefore stays as
/// written (it names direct children, and `docs/*/**` would newly exclude
/// nested contents), while `**/.cursor/plans` gains its contents-expansion
/// despite the wildcard earlier in the pattern.
pub fn expand_directory_pattern(pattern: &str) -> Vec<String> {
    let pattern = expand_home_prefix(pattern);
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

/// The `ignore` override rule that excludes `pattern`.
///
/// The crate spells exclusion with a leading `!`; a pattern already carrying one
/// passes through.
pub fn exclude_override_rule(pattern: &str) -> String {
    if pattern.starts_with('!') {
        pattern.to_string()
    } else {
        format!("!{pattern}")
    }
}

/// Whether every glob an `exclude` pattern turns into compiles.
///
/// An exclude pattern reaches two consumers: [`ExcludeMatchers`] compiles each
/// expansion with `globset`, and the walker adds each as an `ignore` override.
/// Both are mirrored here so a caller holding only the pattern can tell whether
/// either would reject it, which is also when either would print it.
pub fn exclude_pattern_compiles(pattern: &str) -> bool {
    expand_directory_pattern(pattern).iter().all(|expanded| {
        Glob::new(expanded).is_ok()
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
        .add(&expand_home_prefix(pattern))
        .is_ok()
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
pub fn is_absolute_pattern(pattern: &str) -> bool {
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
        let canonical = canonicalize_for_matching(absolute);
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
    fn strip_verbatim_prefix_leaves_other_paths_alone() {
        for path in [
            "/home/dev/docs",
            r"C:\Users\dev",
            r"\\server\share",
            // A device namespace has no ordinary equivalent to unwrap to.
            r"\\?\Volume{b75e2c83-0000-0000-0000-602f00000000}\docs",
            r"\\?\",
            "",
        ] {
            assert_eq!(strip_verbatim_prefix(path), path, "{path:?} must be left as written");
        }
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
