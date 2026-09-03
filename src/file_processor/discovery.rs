//! File discovery, path utilities, and pattern expansion

use core::error::Error;
use ignore::WalkBuilder;
use ignore::overrides::OverrideBuilder;
use rumdl_config::{WITHHELD, resolve_rule_names};
use rumdl_lib::config as rumdl_config;
use rumdl_lib::discovery::{
    ExcludeMatchers, LintableFileMode, LintablePathSelector, MarkdownWalkOptions, apply_markdown_walk_options,
    exclude_override_rule, expand_directory_pattern, has_markdown_extension, include_pattern_compiles,
    normalize_pattern_for_base, path_relative_to, strip_verbatim_prefix,
};
use rumdl_lib::rule::Rule;
use std::collections::HashSet;
use std::path::Path;

use crate::{CodeBlockToolsMode, FixMode};

/// Which auxiliary code-block-tool phases a command performs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AuxiliaryExecutionPlan {
    pub lint: bool,
    pub format: bool,
    pub relint: bool,
}

impl AuxiliaryExecutionPlan {
    /// The phases follow the command, not the mode: only mode decides which rules
    /// run over the outer document, and leaves the tool phases where the same
    /// command without the flag put them. Keeping the two independent is what
    /// makes an only-mode run a subset of the same command's normal output rather
    /// than a different pass that can hide a tool finding it cannot fix.
    fn from_args(args: &crate::CheckArgs) -> Self {
        if args.code_block_tools_mode() == CodeBlockToolsMode::Disabled {
            return Self {
                lint: false,
                format: false,
                relint: false,
            };
        }

        match (args.fix_mode, args.diff) {
            (FixMode::Check, false) => Self {
                lint: true,
                format: false,
                relint: false,
            },
            _ => Self {
                lint: true,
                format: true,
                relint: true,
            },
        }
    }

    pub fn cache_key(self) -> &'static str {
        match (self.lint, self.format, self.relint) {
            (false, false, false) => "none",
            (true, false, false) => "lint",
            (true, true, true) => "lint-format-relint",
            _ => "custom",
        }
    }
}

/// Rule sets with deliberately separate outer-document and embedded roles.
pub struct RuleSets {
    pub mode: CodeBlockToolsMode,
    pub document: Vec<Box<dyn Rule>>,
    pub embedded_markdown: Vec<Box<dyn Rule>>,
    pub auxiliary: AuxiliaryExecutionPlan,
}

impl RuleSets {
    /// Clone both roles after applying the file's per-file ignores.
    pub fn for_file(&self, ignored_rules: &HashSet<String>) -> Self {
        let filter = |rules: &[Box<dyn Rule>]| {
            rules
                .iter()
                .filter(|rule| !ignored_rules.contains(rule.name()))
                .map(|rule| dyn_clone::clone_box(&**rule))
                .collect()
        };

        Self {
            mode: self.mode,
            document: filter(&self.document),
            embedded_markdown: filter(&self.embedded_markdown),
            auxiliary: self.auxiliary,
        }
    }

    /// Rules for which an inline enable or rule-specific `.editorconfig`
    /// setting can observably affect this operation.
    pub fn configuration_relevant_rule_names(&self, config: &rumdl_config::Config) -> HashSet<String> {
        use rumdl_lib::rule::FixCapability;

        let mut names: HashSet<String> = self.document.iter().map(|rule| rule.name().to_string()).collect();

        if self.auxiliary.lint {
            names.extend(
                self.embedded_markdown
                    .iter()
                    .filter(|rule| !matches!(rule.name(), "MD041" | "MD047"))
                    .map(|rule| rule.name().to_string()),
            );
        }

        if self.auxiliary.format {
            names.extend(
                self.embedded_markdown
                    .iter()
                    // MD047 is passed through the formatter, but its newline
                    // change is restored at the fenced-content boundary.
                    .filter(|rule| rule.name() != "MD047")
                    .filter(|rule| super::processing::is_rule_actually_fixable(config, rule.name()))
                    .filter(|rule| rule.fix_capability() != FixCapability::Unfixable)
                    .map(|rule| rule.name().to_string()),
            );
        }

        names
    }
}

/// The rule-selection flags of one `check` or `fmt` invocation, as typed:
/// comma-separated rule IDs or aliases, with the `all` keyword.
pub struct RuleSelectionFlags<'a> {
    pub enable: Option<&'a str>,
    pub disable: Option<&'a str>,
    pub extend_enable: Option<&'a str>,
    pub extend_disable: Option<&'a str>,
}

impl<'a> From<&'a crate::cli_types::SharedCliArgs> for RuleSelectionFlags<'a> {
    fn from(args: &'a crate::cli_types::SharedCliArgs) -> Self {
        Self {
            enable: args.enable.as_deref(),
            disable: args.disable.as_deref(),
            extend_enable: args.extend_enable.as_deref(),
            extend_disable: args.extend_disable.as_deref(),
        }
    }
}

/// Canonical rule IDs named by a CLI list, in a stable order.
fn resolve_flag(list: Option<&str>) -> Vec<String> {
    let mut names: Vec<String> = list.map(resolve_rule_names).unwrap_or_default().into_iter().collect();
    names.sort_unstable();
    names
}

/// A config list with the rules a CLI flag names appended, first mention wins.
fn extended(base: &[String], flag: Option<&str>) -> Vec<String> {
    let mut names = base.to_vec();
    for name in resolve_flag(flag) {
        if !names.contains(&name) {
            names.push(name);
        }
    }
    names
}

/// Resolve the CLI flags against the config's own rule lists into the
/// `GlobalConfig` that `filter_rules` selects from, so the CLI, the LSP and
/// wasm all select rules with one function.
///
/// `--enable` has the semantics of ruff's `--select`: it replaces the config's
/// whole rule selection, and the other three flags then act within that
/// explicit scope alone. Without it, each CLI list extends the config's list
/// of the same name, so a rule disabled by either source stays disabled.
pub fn rule_selection(
    flags: &RuleSelectionFlags<'_>,
    config: &rumdl_config::GlobalConfig,
) -> rumdl_config::GlobalConfig {
    match flags.enable {
        Some(enable) => rumdl_config::GlobalConfig {
            enable: resolve_flag(Some(enable)),
            enable_is_explicit: true,
            disable: resolve_flag(flags.disable),
            extend_enable: resolve_flag(flags.extend_enable),
            extend_disable: resolve_flag(flags.extend_disable),
            ..config.clone()
        },
        None => rumdl_config::GlobalConfig {
            disable: extended(&config.disable, flags.disable),
            extend_enable: extended(&config.extend_enable, flags.extend_enable),
            extend_disable: extended(&config.extend_disable, flags.extend_disable),
            ..config.clone()
        },
    }
}

fn selected_rules(args: &crate::CheckArgs, config: &rumdl_config::Config) -> Vec<Box<dyn Rule>> {
    let selection = rule_selection(&RuleSelectionFlags::from(&args.shared), &config.global);
    let all_rules = rumdl_lib::rules::all_rules(config);
    rumdl_lib::rules::filter_rules(&all_rules, &selection)
}

fn print_rules(label: &str, rules: &[Box<dyn Rule>]) {
    println!("{label}:");
    for rule in rules {
        println!("  - {} ({})", rule.name(), rule.description());
    }
    println!();
}

pub fn get_enabled_rules_from_checkargs(args: &crate::CheckArgs, config: &rumdl_config::Config) -> Vec<Box<dyn Rule>> {
    let final_rules = selected_rules(args, config);

    if args.verbose {
        print_rules("Enabled rules", &final_rules);
    }

    final_rules
}

pub fn get_rule_sets_from_checkargs(args: &crate::CheckArgs, config: &rumdl_config::Config) -> RuleSets {
    let selected = selected_rules(args, config);
    let mode = args.code_block_tools_mode();
    let document = if mode == CodeBlockToolsMode::Only {
        Vec::new()
    } else {
        selected.to_vec()
    };
    let embedded_markdown = if rumdl_lib::embedded_lint::should_lint_embedded_markdown(&config.code_block_tools) {
        selected
    } else {
        Vec::new()
    };

    if args.verbose {
        // Preserve the established heading for the outer document. Only mode
        // makes this list empty and prints its fenced-Markdown rules separately.
        print_rules("Enabled rules", &document);
        if !embedded_markdown.is_empty() {
            print_rules("Enabled fenced-Markdown rules", &embedded_markdown);
        }
    }

    RuleSets {
        mode,
        document,
        embedded_markdown,
        auxiliary: AuxiliaryExecutionPlan::from_args(args),
    }
}

/// Canonicalize a file path to resolve symlinks and prevent duplicate linting.
///
/// Returns the canonical path if successful, or the original path if canonicalization
/// fails (e.g., file doesn't exist yet, permission denied, network path).
#[inline]
fn canonicalize_path_safe(path_str: &str) -> String {
    Path::new(path_str)
        .canonicalize()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| path_str.to_string())
}

/// Convert an absolute file path to a relative path for display purposes.
///
/// Tries to make the path relative to project_root first, then falls back to CWD.
/// If neither works, returns the original path unchanged.
///
/// This improves readability in CI logs and terminal output by showing
/// `docs/guide.md:12:5` instead of `/home/runner/work/myproj/docs/guide.md:12:5`.
pub fn to_display_path(file_path: &str, project_root: Option<&Path>) -> String {
    let path = Path::new(file_path);

    // Canonicalize the file path once (handles symlinks)
    let canonical_file = path.canonicalize().ok();
    let effective_path = canonical_file.as_deref().unwrap_or(path);

    // Try project root first (preferred for consistent output across the project)
    if let Some(root) = project_root
        && let Some(relative) = strip_base_prefix(effective_path, root)
    {
        return normalize_for_display(relative);
    }

    // Fall back to CWD-relative
    if let Ok(cwd) = std::env::current_dir()
        && let Some(relative) = strip_base_prefix(effective_path, &cwd)
    {
        return normalize_for_display(relative);
    }

    // If all else fails, return as-is
    normalize_for_display(file_path.to_string())
}

/// Resolve the path string to show in output for a file.
///
/// With `show_full_path` the path is shown in full (not relativized); otherwise
/// it is relativized via [`to_display_path`]. In both cases the result is
/// normalized by [`normalize_for_display`], so every output format shows the
/// same string for a file.
pub fn resolve_display_path(file_path: &str, show_full_path: bool, project_root: Option<&Path>) -> String {
    if show_full_path {
        normalize_for_display(file_path.to_string())
    } else {
        to_display_path(file_path, project_root)
    }
}

/// Normalize a path for output: `/` separators on every platform, and no Win32
/// verbatim prefix.
///
/// Only the platform's native separator is converted: on Windows `\` becomes `/`.
/// On Unix this is a no-op, where `\` is a legal filename character that must be
/// preserved. Windows paths also shed the `\\?\` prefix `canonicalize` adds, so
/// a file shown in full reads `C:/Users/dev/docs/guide.md`; the verbatim form is
/// an implementation detail of how the file was resolved, and read literally
/// its `//?/` opening names a host to any consumer treating the path as a URI.
fn normalize_for_display(path: String) -> String {
    if cfg!(windows) {
        windows_display_path(&path)
    } else {
        path
    }
}

/// The Windows half of [`normalize_for_display`]: pure string logic, so it is
/// tested on every platform.
pub(super) fn windows_display_path(path: &str) -> String {
    strip_verbatim_prefix(path).replace('\\', "/")
}

/// Try to strip a base path prefix from a file path.
/// Handles canonicalization of the base path to resolve symlinks.
pub(super) fn strip_base_prefix(file_path: &Path, base: &Path) -> Option<String> {
    // Canonicalize base to resolve symlinks (e.g., /tmp -> /private/tmp on macOS)
    let canonical_base = base.canonicalize().ok()?;

    // Try stripping the canonical base prefix
    if let Ok(relative) = file_path.strip_prefix(&canonical_base) {
        return Some(relative.to_string_lossy().to_string());
    }

    // Also try with non-canonical base (for cases where file_path wasn't canonicalized)
    if let Ok(relative) = file_path.strip_prefix(base) {
        return Some(relative.to_string_lossy().to_string());
    }

    None
}

/// Why a discovery walk produced no files to check.
///
/// "There is no markdown here" and "every markdown file was filtered out" are
/// different facts. Reporting both as a bare absence makes a misconfigured run
/// indistinguishable from a clean one, so each variant names which happened.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EmptyDiscovery {
    /// No lintable file was reachable at all, before any filtering.
    NoMarkdownFiles,
    /// Lintable files were reachable and every one of them was filtered out.
    AllFiltered {
        /// How many files existed but went unchecked. Always at least one, and
        /// at least the sum of the per-cause counts below.
        total: usize,
        /// Removed by an ignore file (the gitignore family).
        gitignore: usize,
        /// Removed by an `exclude` pattern.
        exclude: usize,
        /// Selected by no active `include` pattern.
        not_included: usize,
        /// One line per `include` pattern that no reachable file matches, so a
        /// pattern naming nothing is distinguished from one losing to another
        /// filter. Already written out, because a pattern read from an `extends`
        /// target may not be quoted and the line for it says something else
        /// entirely (see [`unmatched_include_lines`]).
        unmatched_includes: Vec<String>,
    },
}

impl EmptyDiscovery {
    /// The filtered-out tally, or [`Self::NoMarkdownFiles`] when it is zero.
    ///
    /// Keeps the invariant that [`Self::AllFiltered`] always accounts for at
    /// least one file, so its message can never read "all 0 were filtered out".
    ///
    /// `total` counts every file that existed and went unchecked, which is what
    /// the headline reports. The per-cause counts only cover files a cause was
    /// positively shown to have removed, so they may sum to less; a cause is
    /// never asserted by elimination.
    fn filtered(
        total: usize,
        gitignore: usize,
        exclude: usize,
        not_included: usize,
        unmatched_includes: Vec<String>,
    ) -> Self {
        if total == 0 {
            return Self::NoMarkdownFiles;
        }
        Self::AllFiltered {
            total,
            gitignore,
            exclude,
            not_included,
            unmatched_includes,
        }
    }

    /// Whether the emptiness points at a configuration problem rather than a
    /// directory that simply holds no markdown.
    pub fn is_misconfiguration(&self) -> bool {
        matches!(self, Self::AllFiltered { .. })
    }
}

impl std::fmt::Display for EmptyDiscovery {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoMarkdownFiles => write!(f, "No markdown files found to check."),
            Self::AllFiltered {
                total,
                gitignore,
                exclude,
                not_included,
                unmatched_includes,
            } => {
                let (noun, verb) = if *total == 1 {
                    ("file", "was")
                } else {
                    ("files", "were")
                };
                write!(
                    f,
                    "No markdown files left to check: {total} {noun} found {verb} filtered out."
                )?;
                if *gitignore > 0 {
                    write!(
                        f,
                        "\n  {gitignore} by ignore files (.gitignore, .ignore, .markdownlintignore); pass --respect-gitignore=false to keep them"
                    )?;
                }
                if *exclude > 0 {
                    write!(f, "\n  {exclude} by exclude patterns; pass --no-exclude to keep them")?;
                }
                if *not_included > 0 {
                    write!(f, "\n  {not_included} by include patterns")?;
                }
                for line in unmatched_includes {
                    write!(f, "\n  {line}")?;
                }
                Ok(())
            }
        }
    }
}

/// The files a discovery walk selected, and why it selected none.
pub struct Discovered {
    /// Files to check, canonicalized and deduplicated.
    pub files: Vec<String>,
    /// Why `files` is empty; `None` whenever at least one file was found.
    pub empty_reason: Option<EmptyDiscovery>,
}

/// Every file reachable from `roots`, streamed in the form the walker produces.
///
/// Vendor directories are traversed, because the discovery walk this explains
/// traverses them too: a markdown file under `node_modules` or `target` is one
/// rumdl checks, so a filter removing it removed a real file. Skipping them here
/// would report those files as never having existed, which is the same silent
/// absence this diagnosis exists to replace.
///
/// Paths come out exactly as the `ignore` walker yields them, which is the form
/// the pattern matchers are fed during the walk being explained. Canonicalizing
/// here would cost a syscall per file for a comparison almost no caller needs.
fn reachable_files(roots: &[&str], respect_gitignore: bool) -> impl Iterator<Item = std::path::PathBuf> + use<> {
    reachable_entries(roots, respect_gitignore)
        .filter(|entry| entry.file_type().is_some_and(|file_type| file_type.is_file()))
        .map(ignore::DirEntry::into_path)
}

/// Every entry the walk reaches, directories included.
///
/// A directory an ignore file hid is pruned before the walk descends, so which
/// directories were reached is what separates a file the walk declined from one
/// it never saw.
fn reachable_entries(roots: &[&str], respect_gitignore: bool) -> impl Iterator<Item = ignore::DirEntry> + use<> {
    let walk = roots.split_first().map(|(first, rest)| {
        let mut builder = WalkBuilder::new(first);
        for root in rest {
            builder.add(root);
        }
        apply_markdown_walk_options(
            &mut builder,
            roots,
            &MarkdownWalkOptions {
                respect_gitignore,
                skip_vendor_dirs: false,
            },
        );
        builder.build()
    });
    walk.into_iter().flatten().filter_map(Result::ok)
}

/// The override set for `rules` anchored at `base`.
///
/// Invalid rules are skipped; the discovery walk already warns about them. An
/// empty set matches nothing either way, which is what an inactive filter means.
fn overrides_from(base: &Path, rules: impl IntoIterator<Item = String>) -> ignore::overrides::Override {
    let mut builder = OverrideBuilder::new(base);
    for rule in rules {
        let _ = builder.add(&rule);
    }
    builder.build().unwrap_or_else(|_| ignore::overrides::Override::empty())
}

/// The path in the canonical form the walk records, from the raw form the walker
/// yields.
///
/// Costs a syscall, so callers reach for it only where a matcher needs the
/// canonical shape rather than the walker's.
fn canonical_walk_path(path: &Path) -> std::path::PathBuf {
    let raw = path.to_string_lossy();
    std::path::PathBuf::from(canonicalize_path_safe(raw.strip_prefix("./").unwrap_or(&raw)))
}

/// Everything that could have kept a file out of a discovery walk.
///
/// Carried as one value so the diagnosis below is built from the same
/// determination the walk used, rather than from a separately assembled set of
/// arguments that could describe a different run.
#[derive(Clone, Copy)]
struct DiscoveryFilters<'a> {
    /// The gate the walk applies to everything its filters let through.
    lintable: &'a LintablePathSelector,
    /// The post-walk exclude matchers, which also carry absolute patterns.
    exclude_matchers: &'a ExcludeMatchers,
    /// The exclude patterns as the walker's overrides see them.
    exclude_patterns: &'a [String],
    /// The include patterns as the walker's overrides see them.
    include_patterns: &'a [String],
    /// How to name the file those patterns came from when the diagnosis may not
    /// quote them, and `None` when it may (see [`unmatched_include_lines`]).
    include_source_withheld: Option<&'a str>,
    /// Files named on the command line that an exclude pattern already dropped.
    named_excluded: &'a [std::path::PathBuf],
    /// The directory the include and exclude patterns are anchored to.
    pattern_base: &'a Path,
    /// The project root in canonical form, for relative pattern matching.
    canonical_project_root: Option<&'a Path>,
    /// Whether the gitignore family applies.
    respect_gitignore: bool,
}

/// Explain why a walk over `roots` under `filters` selected no file.
///
/// Each cause is established positively: a file counts against a filter only
/// because that filter is shown to drop it, never because the other checks
/// happened not to claim it. A cause asserted by elimination would name the
/// wrong knob whenever this diagnosis and the real walk differ for a reason not
/// modelled here, and would do it with full confidence. Every verdict comes from
/// the matcher the walker itself consults, given the path in the form the walker
/// gives it, so the two cannot drift apart; a second walk could.
///
/// A file no cause claims still counts toward the total, so the headline stays
/// an accurate tally of what went unchecked while the breakdown stays verified.
///
/// Ignore files are what keep a walk small, so walking without them is by far
/// the expensive step. It runs only when nothing survived ignore handling, which
/// is the one case its answer can change.
fn diagnose_empty_discovery(roots: &[&str], filters: &DiscoveryFilters<'_>) -> EmptyDiscovery {
    let DiscoveryFilters {
        lintable,
        exclude_matchers,
        exclude_patterns,
        include_patterns,
        include_source_withheld,
        named_excluded,
        pattern_base,
        canonical_project_root,
        respect_gitignore,
    } = *filters;

    let excluded_by_pattern = overrides_from(pattern_base, exclude_patterns.iter().map(|p| exclude_override_rule(p)));
    let included_by_pattern = overrides_from(pattern_base, include_patterns.iter().cloned());
    // Kept apart from the combined set so an unmatched pattern can be named
    // individually: a pattern that selects nothing is a typo, while a pattern
    // losing to another filter is not.
    let per_include: Vec<ignore::overrides::Override> = include_patterns
        .iter()
        .map(|pattern| overrides_from(pattern_base, [pattern.clone()]))
        .collect();

    // A file rumdl would have had to check: markdown by default, plus whatever an
    // `include` pattern both selects and the walk's own gate then keeps. An
    // include that widens the walk without admitting a new file type reaches no
    // further, so a file it merely touches was never a candidate and must not be
    // reported as one the configuration removed.
    let is_lintable = |path: &Path| {
        if has_markdown_extension(path) {
            return true;
        }
        if !included_by_pattern.matched(path, false).is_whitelist() {
            return false;
        }
        lintable.keeps(&canonical_walk_path(path))
    };
    // Absolute patterns and paths outside the walk root reach the run only
    // through the post-walk matchers, which work on canonical paths.
    let excluded_after_walk = |path: &Path| {
        if exclude_matchers.is_empty() {
            return false;
        }
        let canonical = canonical_walk_path(path);
        let relative = canonical_project_root.and_then(|root| path_relative_to(&canonical, root));
        exclude_matchers.excludes_file(relative.as_deref(), &canonical)
    };
    // Overlapping roots (`rumdl check . docs`) hand the walker the same file
    // once per root, and the walk they explain reduces those to one file.
    // Recognising a repeat means canonicalizing, which costs a syscall per file,
    // so it is done only when there is more than one root to overlap.
    let mut seen_paths: HashSet<std::path::PathBuf> = HashSet::new();
    let mut is_repeat = move |path: &Path| roots.len() > 1 && !seen_paths.insert(canonical_walk_path(path));

    let named_excluded_paths: HashSet<&Path> = named_excluded.iter().map(std::path::PathBuf::as_path).collect();
    let already_counted = |path: &Path| {
        if named_excluded_paths.is_empty() {
            return false;
        }
        named_excluded_paths.contains(canonical_walk_path(path).as_path())
    };

    let mut matched_includes = vec![false; per_include.len()];
    let note_include_matches = |path: &Path, matched: &mut Vec<bool>| {
        for (pattern, seen) in per_include.iter().zip(matched.iter_mut()) {
            *seen = *seen || pattern.matched(path, false).is_whitelist();
        }
    };

    // What the include patterns say about a file. An include outranks the
    // ignore files, so this decides whether they could have applied at all, and
    // both walks below ask it first. With no include patterns at all the verdict
    // is neither, which leaves the ignore files free to act.
    let include_verdict = |path: &Path| included_by_pattern.matched(path, false);

    // Files that survived ignore handling. Whatever removed these is one of the
    // user's own patterns, so their causes are decided here. The directories
    // reached along the way are kept for the second walk.
    let (mut total, mut gitignore, mut exclude, mut not_included) = (0, 0, 0, 0);
    let mut reached_dirs: HashSet<std::path::PathBuf> = HashSet::new();
    for entry in reachable_entries(roots, respect_gitignore) {
        if entry.file_type().is_some_and(|file_type| file_type.is_dir()) {
            reached_dirs.insert(canonical_walk_path(entry.path()));
            continue;
        }
        if !entry.file_type().is_some_and(|file_type| file_type.is_file()) {
            continue;
        }
        let path = entry.into_path();
        if !is_lintable(&path) || is_repeat(&path) {
            continue;
        }
        note_include_matches(&path, &mut matched_includes);
        // A named file is tallied below from the command line, where its cause
        // was already established, so meeting it again here is a repeat.
        if already_counted(&path) {
            continue;
        }
        total += 1;
        if excluded_by_pattern.matched(&path, false).is_ignore() || excluded_after_walk(&path) {
            exclude += 1;
        } else if include_verdict(&path).is_ignore() {
            not_included += 1;
        }
    }

    // No candidate turned up at all, so this walk asks what ignore files hid. A
    // file named on the command line and excluded is a candidate whose cause is
    // already established, exactly like one the walk itself found, and either
    // makes this step redundant: an empty run needs one setting to undo, not a
    // census of everything else that went unchecked. Whether a file was named
    // or only walked must not change the diagnosis.
    //
    // The include patterns decide what the ignore files were even allowed to
    // do, so they are asked first. An include that selects nothing is why the
    // ignore files applied at all. One that selects a file the walk reached
    // overrides them, so the real walk did reach it too and only an exclude can
    // have dropped it; the run being empty is what makes that a conclusion
    // rather than a guess. An include cannot reach into a directory the walk
    // never entered, so a file whose directory an ignore file pruned is the
    // ignore file's doing however plainly an include names it. Without include
    // patterns the ignore files are the only thing left, and this walk finding a
    // file the first one could not see is the evidence. Their exclude patterns
    // are never asked: an ignored path never reached them, and answering for a
    // matcher that never ran would be a guess dressed as a finding.
    if total == 0 && named_excluded.is_empty() && respect_gitignore {
        let reached = |path: &Path| {
            path.parent()
                .is_some_and(|dir| reached_dirs.contains(&canonical_walk_path(dir)))
        };
        for path in reachable_files(roots, false) {
            if !is_lintable(&path) || is_repeat(&path) {
                continue;
            }
            note_include_matches(&path, &mut matched_includes);
            total += 1;
            let verdict = include_verdict(&path);
            if verdict.is_ignore() {
                not_included += 1;
            } else if verdict.is_whitelist() && reached(&path) {
                exclude += 1;
            } else {
                gitignore += 1;
            }
        }
    }

    // A pattern the overrides rejected never selected anything to begin with, so
    // "matches no file" would describe it as a glob that names nothing when it is
    // not a glob at all. The walk reports those itself, and reporting them here
    // again would also quote a pattern that walk deliberately did not.
    let unmatched: Vec<&str> = include_patterns
        .iter()
        .zip(&matched_includes)
        .filter(|(pattern, matched)| !**matched && include_pattern_compiles(pattern))
        .map(|(pattern, _)| pattern.as_str())
        .collect();
    let unmatched_includes = unmatched_include_lines(&unmatched, include_source_withheld);

    let named = named_excluded.len();
    EmptyDiscovery::filtered(
        total + named,
        gitignore,
        exclude + named,
        not_included,
        unmatched_includes,
    )
}

/// How an empty run describes the `include` patterns that selected nothing.
///
/// A pattern is quoted, which is what makes the notice actionable, unless it
/// came from an `extends` target: that is text out of a file the extending
/// project only pointed at, and a valid pattern is no less that file's own words
/// than one that does not compile. `withheld_source` then names the file
/// instead, once for the whole list, since lines that quote nothing carry
/// nothing to tell them apart.
fn unmatched_include_lines(unmatched: &[&str], withheld_source: Option<&str>) -> Vec<String> {
    match withheld_source {
        None => unmatched
            .iter()
            .map(|pattern| format!("include pattern '{pattern}' matches no file"))
            .collect(),
        Some(_) if unmatched.is_empty() => Vec::new(),
        Some(source) => {
            let (noun, verb) = if unmatched.len() == 1 {
                ("pattern", "matches")
            } else {
                ("patterns", "match")
            };
            vec![format!("{} include {noun} in {source} {verb} no file", unmatched.len())]
        }
    }
}

pub fn find_markdown_files(
    paths: &[String],
    args: &crate::CheckArgs,
    config: &rumdl_config::Config,
    project_root: Option<&std::path::Path>,
) -> Result<Discovered, Box<dyn Error>> {
    let mut file_paths = Vec::new();

    // Determine if running in discovery mode (e.g., "rumdl ." or "rumdl check ." or "rumdl check")
    let is_discovery_mode = paths.is_empty() || paths == ["."];

    // Track whether config-based include patterns are active in discovery mode
    let has_config_include = is_discovery_mode && !config.global.include.is_empty();

    // Include patterns are matched relative to the same base the walker's
    // overrides use, so `~` is expanded and an absolute pattern under that base
    // is rewritten relative to it (see `normalize_pattern_for_base`).
    let include_base = project_root
        .map(Path::to_path_buf)
        .or_else(|| std::env::current_dir().ok());
    let normalize_include = |pattern: &str| normalize_pattern_for_base(pattern, include_base.as_deref());
    let config_include: Vec<String> = config
        .global
        .include
        .iter()
        .map(|pattern| normalize_include(pattern))
        .collect();

    // --- Determine Effective Include/Exclude Patterns ---

    // Include patterns: CLI > Config (only in discovery mode) > Default (only in discovery mode)
    let final_include_patterns: Vec<String> = if let Some(cli_include) = args.include.as_deref() {
        // 1. CLI --include always wins
        cli_include
            .split(',')
            .map(|p| p.trim())
            .filter(|p| !p.is_empty())
            .map(normalize_include)
            .collect()
    } else if is_discovery_mode && !config.global.include.is_empty() {
        // 2. Config include is used ONLY in discovery mode if specified
        config_include.clone()
    } else if is_discovery_mode {
        // 3. Default: Don't add include patterns as overrides - the type filter already handles
        // selecting markdown files (lines 183-199). Using overrides here would bypass gitignore
        // because overrides take precedence over gitignore in the ignore crate.
        Vec::new()
    } else {
        // 4. Explicit path mode: No includes applied by default. Walk starts from explicit paths.
        Vec::new()
    };

    // How to name the file the include patterns came from when a message about one
    // may not quote it. A CLI `--include` is the user's own text and stays
    // quotable, so only the config branch above can carry the mark.
    let include_source_withheld: Option<&str> = args
        .include
        .is_none()
        .then_some(config.global.include_withheld.as_deref())
        .flatten();

    // Exclude patterns: CLI > Config (but disabled if --no-exclude is set)
    let raw_exclude_patterns: Vec<String> = if args.no_exclude {
        Vec::new() // Disable all exclusions
    } else if let Some(cli_exclude) = args.exclude.as_deref() {
        cli_exclude
            .split(',')
            .map(|p| p.trim().to_string())
            .filter(|p| !p.is_empty())
            .collect()
    } else {
        config.global.exclude.clone()
    };

    // Expand directory-only patterns to also match their contents (for the
    // walker overrides; ExcludeMatchers applies the same expansion itself)
    let final_exclude_patterns: Vec<String> = raw_exclude_patterns
        .iter()
        .flat_map(|p| expand_directory_pattern(p))
        .collect();

    // Debug: Log exclude patterns
    if args.verbose {
        eprintln!("Exclude patterns: {final_exclude_patterns:?}");
    }
    let exclude_matchers = ExcludeMatchers::new(&raw_exclude_patterns);
    for (pattern, error) in &exclude_matchers.invalid {
        eprintln!("Warning: Invalid exclude pattern '{pattern}': {error}");
    }
    let canonical_project_root = project_root.and_then(|root| root.canonicalize().ok());
    let selector_base = canonical_project_root.clone().or_else(|| std::env::current_dir().ok());
    let selector_includes = if has_config_include {
        config_include.as_slice()
    } else {
        &[]
    };
    let selector_mode = if args.include.is_some() {
        LintableFileMode::Any
    } else if has_config_include {
        LintableFileMode::MarkdownAndRust
    } else {
        LintableFileMode::Markdown
    };
    let lintable = LintablePathSelector::new(selector_base.as_deref(), selector_includes, selector_mode);
    // --- End Pattern Determination ---

    // --- Split explicit paths into named files and directory roots ---
    // A file named on the command line is trusted as-is: it is linted even
    // without a markdown extension and bypasses the walker's type/ignore
    // filters, subject only to exclude patterns (issue #99). Directory
    // arguments are walked below exactly like discovery-mode roots, so a
    // mixed invocation checks the union of both (issue #741).
    let mut explicit_files: Vec<String> = Vec::new();
    let mut explicit_dirs: Vec<&str> = Vec::new();
    // Named files an exclude pattern dropped, so an emptied explicit run can say
    // that its arguments were excluded rather than absent. Held as paths, not a
    // count, because a directory argument can rediscover the same file and the
    // diagnosis below must not tally it twice.
    let mut excluded_named_files: Vec<std::path::PathBuf> = Vec::new();

    if !is_discovery_mode {
        for path_str in paths {
            let path = Path::new(path_str);
            if !path.exists() {
                return Err(format!("File not found: {path_str}").into());
            }
            if !path.is_file() {
                explicit_dirs.push(path_str.as_str());
                continue;
            }
            // Convert to relative path for pattern matching
            // This ensures patterns like "docs/*" work with both relative and absolute paths
            let cleaned_path = if path.is_absolute() {
                // Try to make it relative to the current directory
                // Use canonicalized paths to handle symlinks (e.g., /tmp -> /private/tmp on macOS)
                if let Ok(cwd) = std::env::current_dir() {
                    // Canonicalize both paths to resolve symlinks
                    if let (Ok(canonical_cwd), Ok(canonical_path)) = (cwd.canonicalize(), path.canonicalize()) {
                        if let Ok(relative) = canonical_path.strip_prefix(&canonical_cwd) {
                            relative.to_string_lossy().to_string()
                        } else {
                            // Path is absolute but not under cwd, keep as-is
                            path_str.clone()
                        }
                    } else {
                        // Canonicalization failed, keep path as-is
                        path_str.clone()
                    }
                } else {
                    path_str.clone()
                }
            } else if let Some(stripped) = path_str.strip_prefix("./") {
                stripped.to_string()
            } else {
                path_str.clone()
            };

            // Check if this file should be excluded based on exclude patterns
            // This is the default behavior to match user expectations and avoid
            // duplication between rumdl config and pre-commit config (issue #99)
            if !exclude_matchers.is_empty() {
                // Compute path relative to project_root for pattern matching
                // This ensures patterns like "subdir/file.md" work regardless of cwd
                let path_for_matching = canonical_project_root
                    .as_deref()
                    .and_then(|root| path_relative_to(path, root))
                    .unwrap_or_else(|| cleaned_path.clone());
                // Absolute patterns (written literally or produced by `~`
                // expansion) match the absolute path instead.
                if let Some(pattern) = exclude_matchers.matched_pattern_for_file(Some(&path_for_matching), path) {
                    // Excluding an explicitly provided file is a deliberate config choice, so
                    // this is an informational notice, not a warning, and it is surfaced only
                    // under --verbose. This keeps explicit-path mode as quiet as discovery
                    // mode (which excludes silently) while still letting `--verbose` explain
                    // why a named file was skipped. --silent suppresses it entirely.
                    excluded_named_files.push(std::path::PathBuf::from(canonicalize_path_safe(&cleaned_path)));
                    if args.verbose && !args.silent {
                        let display_path = normalize_for_display(cleaned_path.clone());
                        eprintln!(
                            "{display_path} ignored because of exclude pattern '{pattern}'. Use --no-exclude to override"
                        );
                    }
                } else {
                    explicit_files.push(canonicalize_path_safe(&cleaned_path));
                }
            } else {
                explicit_files.push(canonicalize_path_safe(&cleaned_path));
            }
        }

        // One file can be named several times, or under spellings that resolve
        // to the same path. It is still one file, and a count of what an exclude
        // pattern removed has to say so.
        excluded_named_files.sort();
        excluded_named_files.dedup();

        // Nothing to walk when every argument is a file. Returns the explicit
        // set even if exclusions emptied it, so the caller reports "no files"
        // instead of silently falling back to a cwd walk.
        if explicit_dirs.is_empty() {
            explicit_files.sort();
            explicit_files.dedup();
            let excluded = excluded_named_files.len();
            let empty_reason = explicit_files
                .is_empty()
                .then(|| EmptyDiscovery::filtered(excluded, 0, excluded, 0, Vec::new()));
            return Ok(Discovered {
                files: explicit_files,
                empty_reason,
            });
        }
    }

    // --- Configure ignore::WalkBuilder over the directory roots ---
    // Discovery mode walks the cwd (`.`); explicit mode walks only the
    // directory arguments (named files were collected above).
    let walk_roots: Vec<&str> = if is_discovery_mode {
        vec![paths.first().map(String::as_str).unwrap_or(".")]
    } else {
        explicit_dirs.clone()
    };
    let mut walk_builder = {
        let (first, rest) = walk_roots.split_first().expect("a walk always has at least one root");
        let mut builder = WalkBuilder::new(first);
        for dir in rest {
            builder.add(dir);
        }
        builder
    };

    // The shared gate configures the coarse walker types; it is consulted again
    // after the walk for precise root-relative explicit-include matching.
    lintable.configure_types(&mut walk_builder)?;
    // -----------------------------------------

    // Apply overrides using the determined patterns
    if !final_include_patterns.is_empty() || !final_exclude_patterns.is_empty() {
        // Use project_root as the pattern base for OverrideBuilder
        // The walker paths are relative to the walk roots, but the ignore crate
        // handles the path matching internally when both are consistent directories
        let pattern_base = project_root.unwrap_or(Path::new("."));
        let mut override_builder = OverrideBuilder::new(pattern_base);

        // Add includes (these act as positive filters)
        for pattern in &final_include_patterns {
            // Important: In ignore crate, bare patterns act as includes if no exclude (!) is present.
            // If we add excludes later, these includes ensure *only* matching files are considered.
            // If no excludes are added, these effectively define the set of files to walk.
            match (override_builder.add(pattern), include_source_withheld) {
                (Ok(_), _) => {}
                // The error quotes the pattern it could not parse, which is text
                // out of a file this may not repeat. Which file pointed at it is
                // the extending config's own text, so that much can still be said.
                (Err(_), Some(source)) => {
                    eprintln!("Warning: Invalid include pattern in {source}: {WITHHELD}");
                }
                (Err(e), None) => eprintln!("Warning: Invalid include pattern '{pattern}': {e}"),
            }
        }

        // Add excludes (these filter *out* files) - MUST start with '!'
        for pattern in &final_exclude_patterns {
            let exclude_rule = exclude_override_rule(pattern);
            if let Err(e) = override_builder.add(&exclude_rule) {
                eprintln!("Warning: Invalid exclude pattern '{pattern}': {e}");
            }
        }

        // Build and apply the overrides
        match override_builder.build() {
            Ok(overrides) => {
                walk_builder.overrides(overrides);
            }
            Err(e) => {
                eprintln!("Error building path overrides: {e}");
            }
        };
    }

    // Configure ignore handling *SECOND*: gitignore family per config,
    // hidden files included, .markdownlintignore honored. Shared with the
    // LSP workspace scan so both walk the same files.
    apply_markdown_walk_options(
        &mut walk_builder,
        &walk_roots,
        &MarkdownWalkOptions {
            respect_gitignore: config.global.respect_gitignore,
            skip_vendor_dirs: false,
        },
    );

    // --- Execute Walk ---

    for result in walk_builder.build() {
        match result {
            Ok(entry) => {
                let path = entry.path();
                // We are primarily interested in files. ignore crate handles dir traversal.
                // Check if it's a file and if it wasn't explicitly excluded by overrides
                if entry.file_type().is_some_and(|file_type| file_type.is_file()) {
                    let file_path = path.to_string_lossy().to_string();
                    // Clean the path before pushing
                    let cleaned_path = if let Some(stripped) = file_path.strip_prefix("./") {
                        stripped.to_string()
                    } else {
                        file_path
                    };
                    file_paths.push(canonicalize_path_safe(&cleaned_path));
                }
            }
            Err(err) => {
                // Only show generic walking errors for directories, not for missing files
                if is_discovery_mode {
                    eprintln!("Error walking directory: {err}");
                }
            }
        }
    }

    // Remove duplicate paths if WalkBuilder might yield them (e.g. multiple input paths)
    file_paths.sort();
    file_paths.dedup();

    // --- Post-walk exclude pattern filtering ---
    // The ignore crate's overrides may not work correctly when the walker path prefix
    // differs from the config file location. Apply exclude patterns manually here.
    // This also carries absolute patterns, which the walker's overrides cannot
    // express: the ignore crate anchors a leading `/` to the walk root.
    if !exclude_matchers.is_empty() {
        file_paths.retain(|file_path| {
            let path = Path::new(file_path);
            // Compute path relative to project_root for pattern matching. Without
            // a project root, or outside it, only the absolute form applies.
            let path_for_matching = canonical_project_root
                .as_deref()
                .and_then(|root| path_relative_to(path, root));

            // Check if any exclude pattern matches
            !exclude_matchers.excludes_file(path_for_matching.as_deref(), path)
        });
    }

    // --- Final Lintable File Filter ---
    // Explicit include patterns are matched against the same base the walker
    // overrides use, so the full pattern path applies: a broad sibling pattern
    // must not inherit another pattern's allowance for files that merely share
    // its name.
    file_paths.retain(|path_str| lintable.keeps(Path::new(path_str)));
    // -------------------------------------

    // Union with the explicitly named files. Both sides hold canonicalized
    // paths, so a file that is both named and found by the directory walk
    // dedups away.
    file_paths.extend(explicit_files);
    file_paths.sort();
    file_paths.dedup();

    // Only an empty result pays for the diagnosis, and only it needs one.
    let empty_reason = file_paths.is_empty().then(|| {
        diagnose_empty_discovery(
            &walk_roots,
            &DiscoveryFilters {
                lintable: &lintable,
                exclude_matchers: &exclude_matchers,
                exclude_patterns: &final_exclude_patterns,
                include_patterns: &final_include_patterns,
                include_source_withheld,
                named_excluded: &excluded_named_files,
                pattern_base: project_root.unwrap_or(Path::new(".")),
                canonical_project_root: canonical_project_root.as_deref(),
                respect_gitignore: config.global.respect_gitignore,
            },
        )
    });

    Ok(Discovered {
        files: file_paths,
        empty_reason,
    })
}
