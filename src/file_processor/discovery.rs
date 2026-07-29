//! File discovery, path utilities, and pattern expansion

use core::error::Error;
use ignore::WalkBuilder;
use ignore::overrides::OverrideBuilder;
use rumdl_config::resolve_rule_names;
use rumdl_lib::config as rumdl_config;
use rumdl_lib::discovery::{
    ExcludeMatchers, ExplicitIncludeMatchers, MARKDOWN_EXTENSIONS, MarkdownWalkOptions, any_case_extension_glob,
    apply_markdown_walk_options, expand_directory_pattern, has_markdown_extension, normalize_pattern_for_base,
    path_relative_to,
};
use rumdl_lib::rule::Rule;
use std::collections::HashSet;
use std::path::Path;

pub fn get_enabled_rules_from_checkargs(args: &crate::CheckArgs, config: &rumdl_config::Config) -> Vec<Box<dyn Rule>> {
    // 1. Initialize all available rules using from_config only
    let all_rules: Vec<Box<dyn Rule>> = rumdl_lib::rules::all_rules(config);

    // 2. Determine the final list of enabled rules based on precedence
    let final_rules: Vec<Box<dyn Rule>>;

    // CLI flags (resolved to canonical IDs)
    let cli_enable_set: Option<HashSet<String>> = args.enable.as_deref().map(resolve_rule_names);
    let cli_disable_set: Option<HashSet<String>> = args.disable.as_deref().map(resolve_rule_names);
    let cli_extend_enable_set: Option<HashSet<String>> = args.extend_enable.as_deref().map(resolve_rule_names);
    let cli_extend_disable_set: Option<HashSet<String>> = args.extend_disable.as_deref().map(resolve_rule_names);

    // CLI --enable acts like Ruff --select: explicit selection overrides config
    // rule selection, including config `enable` and config `extend-enable`.
    // CLI extend flags remain additive/subtractive within this explicit scope.
    if let Some(enabled_cli) = &cli_enable_set {
        let cli_enable_all = enabled_cli.iter().any(|v| v.eq_ignore_ascii_case("all"));
        let cli_extend_enable_all = cli_extend_enable_set
            .as_ref()
            .is_some_and(|s| s.iter().any(|v| v.eq_ignore_ascii_case("all")));
        let cli_extend_disable_all = cli_extend_disable_set
            .as_ref()
            .is_some_and(|s| s.iter().any(|v| v.eq_ignore_ascii_case("all")));

        let mut current_rules = if cli_enable_all || cli_extend_enable_all {
            all_rules
        } else {
            all_rules
                .into_iter()
                .filter(|rule| enabled_cli.contains(rule.name()))
                .collect::<Vec<_>>()
        };

        if !cli_extend_enable_all && let Some(extend_enabled_cli) = &cli_extend_enable_set {
            let already_enabled: HashSet<&str> = current_rules.iter().map(|r| r.name()).collect();
            let additional: Vec<Box<dyn Rule>> = rumdl_lib::rules::all_rules(config)
                .into_iter()
                .filter(|rule| extend_enabled_cli.contains(rule.name()) && !already_enabled.contains(rule.name()))
                .collect();
            current_rules.extend(additional);
        }

        if cli_extend_disable_all {
            current_rules.clear();
        } else {
            if let Some(extend_disabled_cli) = &cli_extend_disable_set {
                current_rules.retain(|rule| !extend_disabled_cli.contains(rule.name()));
            }
            if let Some(disabled_cli) = &cli_disable_set {
                current_rules.retain(|rule| !disabled_cli.contains(rule.name()));
            }
        }

        final_rules = current_rules;

        // 4. Print enabled rules if verbose
        if args.verbose {
            println!("Enabled rules:");
            for rule in &final_rules {
                println!("  - {} ({})", rule.name(), rule.description());
            }
            println!();
        }

        return final_rules;
    }

    // Config rule lists are guaranteed canonical by the runtime invariant
    // enforced in `Config::canonicalize_rule_lists` (see `src/config/types.rs`),
    // so a plain string set suffices here. CLI flags are still resolved above
    // because they come from raw user input that hasn't been canonicalised.
    let config_enable_set: HashSet<String> = config.global.enable.iter().cloned().collect();
    let config_disable_set: HashSet<String> = config.global.disable.iter().cloned().collect();
    let config_extend_enable_set: HashSet<String> = config.global.extend_enable.iter().cloned().collect();
    let config_extend_disable_set: HashSet<String> = config.global.extend_disable.iter().cloned().collect();

    let config_enable_all = config.global.enable.iter().any(|s| s.eq_ignore_ascii_case("all"));
    let opt_in_set = rumdl_lib::rules::opt_in_rules();

    // Combine all extend-enable sources (config + CLI) into one set
    let mut combined_extend_enable: HashSet<String> = config_extend_enable_set;
    if let Some(ref cli_ee) = cli_extend_enable_set {
        combined_extend_enable.extend(cli_ee.iter().cloned());
    }

    // Combine all extend-disable sources (config + CLI) into one set
    let mut combined_extend_disable: HashSet<String> = config_extend_disable_set;
    if let Some(ref cli_ed) = cli_extend_disable_set {
        combined_extend_disable.extend(cli_ed.iter().cloned());
    }

    // Check for "ALL" keyword in extend-enable (case-insensitive)
    let extend_enable_all = combined_extend_enable.iter().any(|s| s.eq_ignore_ascii_case("all"));
    // Check for "all" keyword in extend-disable (case-insensitive)
    let extend_disable_all = combined_extend_disable.iter().any(|s| s.eq_ignore_ascii_case("all"));

    // Step 1: Determine the base rule set
    let mut current_rules = if extend_enable_all {
        // extend-enable: ["ALL"] → all rules including opt-in
        all_rules
    } else if config_enable_all {
        // enable: ["ALL"] → all rules including opt-in
        all_rules
    } else if !config_enable_set.is_empty() || config.global.enable_is_explicit {
        // Explicit enable list (possibly empty) → only those rules
        all_rules
            .into_iter()
            .filter(|rule| config_enable_set.contains(rule.name()))
            .collect::<Vec<_>>()
    } else {
        // No explicit enable → all non-opt-in rules
        all_rules
            .into_iter()
            .filter(|rule| !opt_in_set.contains(rule.name()))
            .collect::<Vec<_>>()
    };

    // Step 2: Apply additive extend-enable (add rules not already present)
    // Skip if extend_enable_all was already handled in step 1
    if !extend_enable_all && !combined_extend_enable.is_empty() {
        let already_enabled: HashSet<&str> = current_rules.iter().map(|r| r.name()).collect();
        let additional: Vec<Box<dyn Rule>> = rumdl_lib::rules::all_rules(config)
            .into_iter()
            .filter(|rule| combined_extend_enable.contains(rule.name()) && !already_enabled.contains(rule.name()))
            .collect();
        current_rules.extend(additional);
    }

    // Step 3: Apply disables (subtractive, all sources)
    if extend_disable_all {
        current_rules.clear();
    } else {
        if !config_disable_set.is_empty() {
            current_rules.retain(|rule| !config_disable_set.contains(rule.name()));
        }
        if !combined_extend_disable.is_empty() {
            current_rules.retain(|rule| !combined_extend_disable.contains(rule.name()));
        }
        if let Some(disabled_cli) = &cli_disable_set {
            current_rules.retain(|rule| !disabled_cli.contains(rule.name()));
        }
    }

    final_rules = current_rules;

    // 4. Print enabled rules if verbose
    if args.verbose {
        println!("Enabled rules:");
        for rule in &final_rules {
            println!("  - {} ({})", rule.name(), rule.description());
        }
        println!();
    }

    final_rules
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
        return normalize_separators(relative);
    }

    // Fall back to CWD-relative
    if let Ok(cwd) = std::env::current_dir()
        && let Some(relative) = strip_base_prefix(effective_path, &cwd)
    {
        return normalize_separators(relative);
    }

    // If all else fails, return as-is
    normalize_separators(file_path.to_string())
}

/// Resolve the path string to show in output for a file.
///
/// With `show_full_path` the path is shown as-is (not relativized); otherwise it
/// is relativized via [`to_display_path`]. In both cases the result uses `/`
/// separators for consistent output across platforms.
pub fn resolve_display_path(file_path: &str, show_full_path: bool, project_root: Option<&Path>) -> String {
    if show_full_path {
        normalize_separators(file_path.to_string())
    } else {
        to_display_path(file_path, project_root)
    }
}

/// Normalize path separators to `/` for consistent cross-platform output.
///
/// Only the platform's native separator is converted: on Windows `\` becomes `/`.
/// On Unix this is a no-op, where `\` is a legal filename character that must be
/// preserved.
fn normalize_separators(path: String) -> String {
    if cfg!(windows) { path.replace('\\', "/") } else { path }
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
        /// `include` patterns that no reachable file matches, so they are a
        /// pattern that names nothing rather than one losing to another filter.
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
                for pattern in unmatched_includes {
                    write!(f, "\n  include pattern '{pattern}' matches no file")?;
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
    let walk = roots.split_first().map(|(first, rest)| {
        let mut builder = WalkBuilder::new(first);
        for root in rest {
            builder.add(root);
        }
        apply_markdown_walk_options(
            &mut builder,
            &MarkdownWalkOptions {
                respect_gitignore,
                skip_vendor_dirs: false,
            },
        );
        builder.build()
    });
    walk.into_iter()
        .flatten()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_some_and(|file_type| file_type.is_file()))
        .map(ignore::DirEntry::into_path)
}

/// The `ignore` override rule that excludes `pattern`.
///
/// The crate spells exclusion with a leading `!`; a pattern already carrying one
/// passes through.
fn exclude_override_rule(pattern: &str) -> String {
    if pattern.starts_with('!') {
        pattern.to_string()
    } else {
        format!("!{pattern}")
    }
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

/// Whether a discovery walk keeps a file once its filters have run.
///
/// A CLI `--include` replaces the extension gate outright, so the walk keeps
/// whatever the pattern selected. Config include patterns do not: they widen the
/// walk, but the gate still applies afterwards, so a pattern pinning no extension
/// (`docs/**`) admits no new file type. The walk and the diagnosis share this so
/// the diagnosis cannot report a file as filtered out that the walk would have
/// dropped for never being lintable.
#[derive(Clone, Copy)]
struct LintableFilter<'a> {
    /// Whether a CLI `--include` is active, which removes the gate entirely.
    cli_include_active: bool,
    /// Whether config include patterns are active, which also admit Rust sources.
    has_config_include: bool,
    /// Config include patterns naming files beyond the markdown extensions.
    explicit_includes: &'a ExplicitIncludeMatchers,
    /// The base those patterns are matched relative to.
    base: Option<&'a Path>,
}

impl LintableFilter<'_> {
    /// Whether `path`, given in the canonical form the walk records, is lintable.
    fn keeps(&self, path: &Path) -> bool {
        if self.cli_include_active {
            return true;
        }
        let is_rust = self.has_config_include && path.extension().is_some_and(|ext| ext.to_str() == Some("rs"));
        if has_markdown_extension(path) || is_rust {
            return true;
        }
        if self.explicit_includes.is_empty() {
            return false;
        }
        match self.base.and_then(|base| path_relative_to(path, base)) {
            Some(relative) => self.explicit_includes.matches_relative_path(&relative),
            // Outside the pattern base only unanchored patterns can still apply;
            // matching the full path covers those.
            None => self.explicit_includes.matches_relative_path(&path.to_string_lossy()),
        }
    }
}

/// Everything that could have kept a file out of a discovery walk.
///
/// Carried as one value so the diagnosis below is built from the same
/// determination the walk used, rather than from a separately assembled set of
/// arguments that could describe a different run.
#[derive(Clone, Copy)]
struct DiscoveryFilters<'a> {
    /// The gate the walk applies to everything its filters let through.
    lintable: LintableFilter<'a>,
    /// The post-walk exclude matchers, which also carry absolute patterns.
    exclude_matchers: &'a ExcludeMatchers,
    /// The exclude patterns as the walker's overrides see them.
    exclude_patterns: &'a [String],
    /// The include patterns as the walker's overrides see them.
    include_patterns: &'a [String],
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
    // user's own patterns, so their causes are decided here.
    let (mut total, mut gitignore, mut exclude, mut not_included) = (0, 0, 0, 0);
    for path in reachable_files(roots, respect_gitignore) {
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
    // ignore files applied at all. One that selects the file overrides them, so
    // the real walk did reach it and only an exclude can have dropped it; the
    // run being empty is what makes that a conclusion rather than a guess.
    // Without include patterns the ignore files are the only thing left, and
    // this walk finding a file the first one could not see is the evidence.
    // Their exclude patterns are never asked: an ignored path never reached
    // them, and answering for a matcher that never ran would be a guess dressed
    // as a finding.
    if total == 0 && named_excluded.is_empty() && respect_gitignore {
        for path in reachable_files(roots, false) {
            if !is_lintable(&path) || is_repeat(&path) {
                continue;
            }
            note_include_matches(&path, &mut matched_includes);
            total += 1;
            let verdict = include_verdict(&path);
            if verdict.is_ignore() {
                not_included += 1;
            } else if verdict.is_whitelist() {
                exclude += 1;
            } else {
                gitignore += 1;
            }
        }
    }

    let unmatched_includes = include_patterns
        .iter()
        .zip(&matched_includes)
        .filter(|(_, matched)| !**matched)
        .map(|(pattern, _)| pattern.clone())
        .collect();

    let named = named_excluded.len();
    EmptyDiscovery::filtered(
        total + named,
        gitignore,
        exclude + named,
        not_included,
        unmatched_includes,
    )
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

    // Config include patterns that explicitly name files beyond the standard
    // markdown extensions (e.g. `**/*.md.jinja`). These widen both the
    // walker's type filter and the final lintable-file filter below, so that
    // config include reaches the same files the equivalent CLI --include does.
    let explicit_includes = if has_config_include {
        ExplicitIncludeMatchers::new(&config_include)
    } else {
        ExplicitIncludeMatchers::new(&[])
    };

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
                        let display_path = normalize_separators(cleaned_path.clone());
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

    // --- Add Lintable File Type Filter ---
    // CLI --include: no type filter (user controls which files to process)
    // Config include: expanded filter (markdown + rust + explicitly named
    // files, since the user spelled those out)
    // Default: markdown-only filter
    if args.include.is_none() {
        let mut types_builder = ignore::types::TypesBuilder::new();
        types_builder.add_defaults();
        for ext in MARKDOWN_EXTENSIONS {
            types_builder.add("markdown", &any_case_extension_glob(ext))?;
        }
        types_builder.select("markdown");
        if has_config_include {
            // Config include is active: also allow Rust files for doc comment linting
            types_builder.add("rustdoc", "*.rs")?;
            types_builder.select("rustdoc");
        }
        if !explicit_includes.is_empty() {
            // Type names must be purely alphanumeric in the ignore crate.
            for glob in explicit_includes.file_name_globs() {
                types_builder.add("configinclude", glob)?;
            }
            types_builder.select("configinclude");
        }
        let types = types_builder.build()?;
        walk_builder.types(types);
    }
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
            if let Err(e) = override_builder.add(pattern) {
                eprintln!("Warning: Invalid include pattern '{pattern}': {e}");
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
    let explicit_include_base = canonical_project_root.clone().or_else(|| std::env::current_dir().ok());
    let lintable = LintableFilter {
        cli_include_active: args.include.is_some(),
        has_config_include,
        explicit_includes: &explicit_includes,
        base: explicit_include_base.as_deref(),
    };
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
                lintable,
                exclude_matchers: &exclude_matchers,
                exclude_patterns: &final_exclude_patterns,
                include_patterns: &final_include_patterns,
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
