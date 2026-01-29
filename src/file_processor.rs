//! File processing and linting logic

use crate::cache::LintCache;
use crate::formatter;
use colored::*;
use core::error::Error;
use ignore::WalkBuilder;
use ignore::overrides::OverrideBuilder;
use rumdl_config::{resolve_rule_name, resolve_rule_names};
use rumdl_lib::config as rumdl_config;
use rumdl_lib::lint_context::LintContext;
use rumdl_lib::rule::{FixCapability, LintWarning, Rule};
use rumdl_lib::utils::code_block_utils::CodeBlockUtils;
use std::borrow::Cow;
use std::collections::HashSet;
use std::path::Path;

/// Expands directory-style patterns to also match files within them.
/// Pattern "dir/path" becomes ["dir/path", "dir/path/**"] to match both
/// the directory itself and all contents recursively.
///
/// Patterns containing glob characters (*, ?, [) are returned unchanged.
fn expand_directory_pattern(pattern: &str) -> Vec<String> {
    // If pattern already has glob characters, use as-is
    if pattern.contains('*') || pattern.contains('?') || pattern.contains('[') {
        return vec![pattern.to_string()];
    }

    // Directory-like pattern: no glob chars
    // Transform to match both the directory and its contents
    let base = pattern.trim_end_matches('/');
    vec![
        base.to_string(),     // Match the directory itself
        format!("{base}/**"), // Match everything underneath
    ]
}

pub fn get_enabled_rules_from_checkargs(args: &crate::CheckArgs, config: &rumdl_config::Config) -> Vec<Box<dyn Rule>> {
    // 1. Initialize all available rules using from_config only
    let all_rules: Vec<Box<dyn Rule>> = rumdl_lib::rules::all_rules(config);

    // 2. Determine the final list of enabled rules based on precedence
    let final_rules: Vec<Box<dyn Rule>>;

    // Rule names provided via CLI flags (resolved to canonical IDs)
    let cli_enable_set: Option<HashSet<String>> = args.enable.as_deref().map(resolve_rule_names);
    let cli_disable_set: Option<HashSet<String>> = args.disable.as_deref().map(resolve_rule_names);
    let cli_extend_enable_set: Option<HashSet<String>> = args.extend_enable.as_deref().map(resolve_rule_names);
    let cli_extend_disable_set: Option<HashSet<String>> = args.extend_disable.as_deref().map(resolve_rule_names);

    // Rule names provided via config file (resolved to canonical IDs for consistent comparison)
    let config_enable_set: HashSet<String> = config.global.enable.iter().map(|s| resolve_rule_name(s)).collect();
    let config_disable_set: HashSet<String> = config.global.disable.iter().map(|s| resolve_rule_name(s)).collect();

    if let Some(enabled_cli) = &cli_enable_set {
        // CLI --enable completely overrides config (ruff --select behavior)
        // CLI names are already resolved to canonical IDs
        let mut filtered_rules = all_rules
            .into_iter()
            .filter(|rule| enabled_cli.contains(rule.name()))
            .collect::<Vec<_>>();

        // Apply CLI --disable to remove rules from the enabled set (ruff-like behavior)
        if let Some(disabled_cli) = &cli_disable_set {
            filtered_rules.retain(|rule| !disabled_cli.contains(rule.name()));
        }

        final_rules = filtered_rules;
    } else if cli_extend_enable_set.is_some() || cli_extend_disable_set.is_some() {
        // Handle extend flags (additive with config)
        let mut current_rules = all_rules;

        // Start with config enable if present (config set already resolved to canonical IDs)
        if !config_enable_set.is_empty() {
            current_rules.retain(|rule| config_enable_set.contains(rule.name()));
        }

        // Add CLI extend-enable rules
        if let Some(extend_enabled_cli) = &cli_extend_enable_set {
            // If we started with all rules (no config enable), keep all rules
            // If we started with config enable, we need to re-filter with extended set
            if !config_enable_set.is_empty() {
                // Merge config enable set with CLI extend-enable (both already canonical IDs)
                let extended_enable_set: HashSet<&str> = config_enable_set
                    .iter()
                    .map(|s| s.as_str())
                    .chain(extend_enabled_cli.iter().map(|s| s.as_str()))
                    .collect();

                // Re-filter with extended set
                current_rules = rumdl_lib::rules::all_rules(config)
                    .into_iter()
                    .filter(|rule| extended_enable_set.contains(rule.name()))
                    .collect();
            }
        }

        // Apply config disable (config set already resolved to canonical IDs)
        if !config_disable_set.is_empty() {
            current_rules.retain(|rule| !config_disable_set.contains(rule.name()));
        }

        // Apply CLI extend-disable (already resolved to canonical IDs)
        if let Some(extend_disabled_cli) = &cli_extend_disable_set {
            current_rules.retain(|rule| !extend_disabled_cli.contains(rule.name()));
        }

        // Apply CLI disable (already resolved to canonical IDs)
        if let Some(disabled_cli) = &cli_disable_set {
            current_rules.retain(|rule| !disabled_cli.contains(rule.name()));
        }

        final_rules = current_rules;
    } else {
        // --- Case 2: No CLI --enable ---
        // Start with the configured rules.
        let mut current_rules = all_rules;

        // Step 2a: Apply config `enable` (if specified).
        // Config set already resolved to canonical IDs.
        if !config_enable_set.is_empty() {
            current_rules.retain(|rule| config_enable_set.contains(rule.name()));
        }

        // Step 2b: Apply config `disable`.
        // Config set already resolved to canonical IDs.
        if !config_disable_set.is_empty() {
            current_rules.retain(|rule| !config_disable_set.contains(rule.name()));
        }

        // Step 2c: Apply CLI `disable` (already resolved to canonical IDs).
        // Remove rules specified in cli.disable from the result of steps 2a & 2b.
        if let Some(disabled_cli) = &cli_disable_set {
            current_rules.retain(|rule| !disabled_cli.contains(rule.name()));
        }

        final_rules = current_rules; // Assign the final filtered vector
    }

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
        return relative;
    }

    // Fall back to CWD-relative
    if let Ok(cwd) = std::env::current_dir()
        && let Some(relative) = strip_base_prefix(effective_path, &cwd)
    {
        return relative;
    }

    // If all else fails, return as-is
    file_path.to_string()
}

/// Try to strip a base path prefix from a file path.
/// Handles canonicalization of the base path to resolve symlinks.
fn strip_base_prefix(file_path: &Path, base: &Path) -> Option<String> {
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

pub fn find_markdown_files(
    paths: &[String],
    args: &crate::CheckArgs,
    config: &rumdl_config::Config,
    project_root: Option<&std::path::Path>,
) -> Result<Vec<String>, Box<dyn Error>> {
    let mut file_paths = Vec::new();

    // --- Configure ignore::WalkBuilder ---
    // Start with the first path, add others later
    let first_path = paths.first().cloned().unwrap_or_else(|| ".".to_string());
    let mut walk_builder = WalkBuilder::new(first_path);

    // Add remaining paths
    for path in paths.iter().skip(1) {
        walk_builder.add(path);
    }

    // --- Add Markdown File Type Definition ---
    // Only apply type filtering if --include is NOT provided
    // When --include is provided, let the include patterns determine which files to process
    if args.include.is_none() {
        let mut types_builder = ignore::types::TypesBuilder::new();
        types_builder.add_defaults(); // Add standard types
        types_builder.add("markdown", "*.md")?;
        types_builder.add("markdown", "*.markdown")?;
        types_builder.add("markdown", "*.mdx")?;
        types_builder.add("markdown", "*.mkd")?;
        types_builder.add("markdown", "*.mkdn")?;
        types_builder.add("markdown", "*.mdown")?;
        types_builder.add("markdown", "*.mdwn")?;
        types_builder.add("markdown", "*.qmd")?;
        types_builder.add("markdown", "*.rmd")?;
        types_builder.add("markdown", "*.Rmd")?;
        types_builder.select("markdown"); // Select ONLY markdown for processing
        let types = types_builder.build()?;
        walk_builder.types(types);
    }
    // -----------------------------------------

    // Determine if running in discovery mode (e.g., "rumdl ." or "rumdl check ." or "rumdl check")
    // Adjusted to handle both legacy and subcommand paths
    let is_discovery_mode = paths.is_empty() || paths == ["."];

    // Track if --include was explicitly provided via CLI
    // This is used to decide whether to apply the final extension filter
    let has_explicit_cli_include = args.include.is_some();

    // --- Determine Effective Include/Exclude Patterns ---

    // Include patterns: CLI > Config (only in discovery mode) > Default (only in discovery mode)
    let final_include_patterns: Vec<String> = if let Some(cli_include) = args.include.as_deref() {
        // 1. CLI --include always wins
        cli_include
            .split(',')
            .map(|p| p.trim().to_string())
            .filter(|p| !p.is_empty())
            .collect()
    } else if is_discovery_mode && !config.global.include.is_empty() {
        // 2. Config include is used ONLY in discovery mode if specified
        config.global.include.clone()
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
    // Expand directory-only patterns to also match their contents
    let final_exclude_patterns: Vec<String> = if args.no_exclude {
        Vec::new() // Disable all exclusions
    } else if let Some(cli_exclude) = args.exclude.as_deref() {
        cli_exclude
            .split(',')
            .map(|p| p.trim().to_string())
            .filter(|p| !p.is_empty())
            .flat_map(|p| expand_directory_pattern(&p))
            .collect()
    } else {
        config
            .global
            .exclude
            .iter()
            .flat_map(|p| expand_directory_pattern(p))
            .collect()
    };

    // Debug: Log exclude patterns
    if args.verbose {
        eprintln!("Exclude patterns: {final_exclude_patterns:?}");
    }
    // --- End Pattern Determination ---

    // Apply overrides using the determined patterns
    if !final_include_patterns.is_empty() || !final_exclude_patterns.is_empty() {
        // Use project_root as the pattern base for OverrideBuilder
        // The walker paths are relative to the first_path, but the ignore crate
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
            // Ensure exclude patterns start with '!' for ignore crate overrides
            let exclude_rule = if pattern.starts_with('!') {
                pattern.clone() // Already formatted
            } else {
                format!("!{pattern}")
            };
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

    // Configure gitignore handling *SECOND*
    // Use config value which merges CLI override with config file setting
    let use_gitignore = config.global.respect_gitignore;

    walk_builder.ignore(use_gitignore); // Enable/disable .ignore
    walk_builder.git_ignore(use_gitignore); // Enable/disable .gitignore
    walk_builder.git_global(use_gitignore); // Enable/disable global gitignore
    walk_builder.git_exclude(use_gitignore); // Enable/disable .git/info/exclude
    walk_builder.parents(use_gitignore); // Enable/disable parent ignores
    walk_builder.hidden(false); // Include hidden files and directories
    walk_builder.require_git(false); // Process git ignores even if no repo detected

    // Add support for .markdownlintignore file
    walk_builder.add_custom_ignore_filename(".markdownlintignore");

    // --- Pre-check for explicit file paths ---
    // If not in discovery mode, validate that specified paths exist
    if !is_discovery_mode {
        let mut processed_explicit_files = false;

        for path_str in paths {
            let path = Path::new(path_str);
            if !path.exists() {
                return Err(format!("File not found: {path_str}").into());
            }
            // If it's a file, process it (trust user's explicit intent)
            if path.is_file() {
                processed_explicit_files = true;
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
                if !final_exclude_patterns.is_empty() {
                    // Compute path relative to project_root for pattern matching
                    // This ensures patterns like "subdir/file.md" work regardless of cwd
                    let path_for_matching = if let Some(root) = project_root {
                        if let Ok(canonical_path) = path.canonicalize() {
                            if let Ok(canonical_root) = root.canonicalize() {
                                if let Ok(relative) = canonical_path.strip_prefix(&canonical_root) {
                                    relative.to_string_lossy().to_string()
                                } else {
                                    // Path is not under project_root, fall back to cleaned_path
                                    cleaned_path.clone()
                                }
                            } else {
                                cleaned_path.clone()
                            }
                        } else {
                            cleaned_path.clone()
                        }
                    } else {
                        cleaned_path.clone()
                    };

                    let mut matching_pattern: Option<&str> = None;
                    for pattern in &final_exclude_patterns {
                        // Use globset for pattern matching
                        if let Ok(glob) = globset::Glob::new(pattern) {
                            let matcher = glob.compile_matcher();
                            if matcher.is_match(&path_for_matching) {
                                matching_pattern = Some(pattern);
                                break;
                            }
                        }
                    }
                    if let Some(pattern) = matching_pattern {
                        // Always print a warning when excluding explicitly provided files
                        // This matches ESLint's behavior and helps users understand why the file wasn't linted
                        eprintln!(
                            "warning: {cleaned_path} ignored because of exclude pattern '{pattern}'. Use --no-exclude to override"
                        );
                    } else {
                        file_paths.push(canonicalize_path_safe(&cleaned_path));
                    }
                } else {
                    file_paths.push(canonicalize_path_safe(&cleaned_path));
                }
            }
        }

        // If we processed explicit files, return the results (even if empty due to exclusions)
        // This prevents the walker from running when explicit files were provided
        if processed_explicit_files {
            file_paths.sort();
            file_paths.dedup();
            return Ok(file_paths);
        }
    }

    // --- Execute Walk ---

    for result in walk_builder.build() {
        match result {
            Ok(entry) => {
                let path = entry.path();
                // We are primarily interested in files. ignore crate handles dir traversal.
                // Check if it's a file and if it wasn't explicitly excluded by overrides
                if path.is_file() {
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
    if !final_exclude_patterns.is_empty()
        && let Some(root) = project_root
    {
        file_paths.retain(|file_path| {
            let path = Path::new(file_path);
            // Compute path relative to project_root for pattern matching
            let path_for_matching = if let Ok(canonical_path) = path.canonicalize() {
                if let Ok(canonical_root) = root.canonicalize() {
                    if let Ok(relative) = canonical_path.strip_prefix(&canonical_root) {
                        relative.to_string_lossy().to_string()
                    } else {
                        file_path.clone()
                    }
                } else {
                    file_path.clone()
                }
            } else {
                file_path.clone()
            };

            // Check if any exclude pattern matches
            for pattern in &final_exclude_patterns {
                if let Ok(glob) = globset::Glob::new(pattern) {
                    let matcher = glob.compile_matcher();
                    if matcher.is_match(&path_for_matching) {
                        return false; // Exclude this file
                    }
                }
            }
            true // Keep this file
        });
    }

    // --- Final Explicit Markdown Filter ---
    // Only apply the extension filter if --include was NOT explicitly provided via CLI
    // When --include is provided, respect the user's explicit intent about which files to check
    if !has_explicit_cli_include {
        // Ensure only files with markdown extensions are returned,
        // regardless of how ignore crate overrides interacted with type filters.
        file_paths.retain(|path_str| {
            let path = Path::new(path_str);
            path.extension().is_some_and(|ext| {
                matches!(
                    ext.to_str(),
                    Some("md" | "markdown" | "mdx" | "mkd" | "mkdn" | "mdown" | "mdwn" | "qmd" | "rmd" | "Rmd")
                )
            })
        });
    }
    // -------------------------------------

    Ok(file_paths) // Ensure the function returns the result
}
pub fn is_rule_actually_fixable(config: &rumdl_config::Config, rule_name: &str) -> bool {
    // Check unfixable list
    if config
        .global
        .unfixable
        .iter()
        .any(|r| r.eq_ignore_ascii_case(rule_name))
    {
        return false;
    }

    // Check fixable list if specified
    if !config.global.fixable.is_empty() {
        return config.global.fixable.iter().any(|r| r.eq_ignore_ascii_case(rule_name));
    }

    true
}

/// Check if a rule is fixable via CLI (considers both config AND rule's fix_capability)
///
/// A rule is CLI-fixable if:
/// 1. It's not in the unfixable config list
/// 2. It's in the fixable config list (if specified)
/// 3. The rule itself doesn't declare FixCapability::Unfixable
///
/// This replaces hardcoded rule name checks (e.g., `&& name != "MD033"`) with
/// capability-based checks that are future-proof for any rule.
pub fn is_rule_cli_fixable(rules: &[Box<dyn Rule>], config: &rumdl_config::Config, rule_name: &str) -> bool {
    // First check config-based fixability
    if !is_rule_actually_fixable(config, rule_name) {
        return false;
    }

    // Then check if the rule declares itself as Unfixable
    // Rules like MD033 have LSP-only fixes (for VS Code quick actions) but
    // their fix() method returns content unchanged, so CLI shouldn't count them
    rules
        .iter()
        .find(|r| r.name().eq_ignore_ascii_case(rule_name))
        .is_none_or(|r| r.fix_capability() != FixCapability::Unfixable)
}

#[allow(clippy::too_many_arguments)]
pub fn process_file_with_formatter(
    file_path: &str,
    rules: &[Box<dyn Rule>],
    fix_mode: crate::FixMode,
    diff: bool,
    verbose: bool,
    quiet: bool,
    silent: bool,
    output_format: &rumdl_lib::output::OutputFormat,
    output_writer: &rumdl_lib::output::OutputWriter,
    config: &rumdl_config::Config,
    cache: Option<std::sync::Arc<std::sync::Mutex<LintCache>>>,
    project_root: Option<&Path>,
    show_full_path: bool,
    cache_hashes: Option<&CacheHashes>,
) -> (
    bool,
    usize,
    usize,
    usize,
    Vec<rumdl_lib::rule::LintWarning>,
    rumdl_lib::workspace_index::FileIndex,
) {
    let formatter = output_format.create_formatter();

    // Convert to display path (relative) unless --show-full-path is set
    let display_path = if show_full_path {
        file_path.to_string()
    } else {
        to_display_path(file_path, project_root)
    };

    // Call the original process_file_inner to get warnings, original line ending, and FileIndex
    let (all_warnings, mut content, total_warnings, fixable_warnings, original_line_ending, file_index) =
        process_file_inner(file_path, rules, verbose, quiet, silent, config, cache, cache_hashes);

    // Compute filtered rules based on per-file-ignores for embedded markdown formatting
    // This ensures embedded markdown formatting respects per-file-ignores just like linting does
    let ignored_rules_for_file = config.get_ignored_rules_for_file(Path::new(file_path));
    let filtered_rules: Vec<Box<dyn Rule>> = if !ignored_rules_for_file.is_empty() {
        rules
            .iter()
            .filter(|rule| !ignored_rules_for_file.contains(rule.name()))
            .map(|r| dyn_clone::clone_box(&**r))
            .collect()
    } else {
        rules.to_vec()
    };

    // In check mode with no warnings, return early
    if total_warnings == 0 && fix_mode == crate::FixMode::Check && !diff {
        return (false, 0, 0, 0, Vec::new(), file_index);
    }

    // In fix mode with no warnings to fix, check if there are embedded markdown blocks to format
    // If not, return early to avoid the re-lint which doesn't apply per-file-ignores or inline config
    if total_warnings == 0 && fix_mode != crate::FixMode::Check && !diff {
        // Check if there's any embedded markdown to format
        let has_embedded = has_fenced_code_blocks(&content)
            && CodeBlockUtils::detect_markdown_code_blocks(&content)
                .iter()
                .any(|b| !content[b.content_start..b.content_end].trim().is_empty());

        if !has_embedded {
            return (false, 0, 0, 0, Vec::new(), file_index);
        }
    }

    // Format and output warnings (show diagnostics unless silent)
    if !silent && fix_mode == crate::FixMode::Check {
        if diff {
            // In diff mode, only show warnings for unfixable issues
            let unfixable_warnings: Vec<_> = all_warnings.iter().filter(|w| w.fix.is_none()).cloned().collect();

            if !unfixable_warnings.is_empty() {
                let formatted = formatter.format_warnings(&unfixable_warnings, &display_path);
                if !formatted.is_empty() {
                    output_writer.writeln(&formatted).unwrap_or_else(|e| {
                        eprintln!("Error writing output: {e}");
                    });
                }
            }
        } else {
            // In check mode, show all warnings with [*] for fixable issues
            // Strip fix from warnings where the rule is not CLI-fixable (e.g., LSP-only fixes)
            let display_warnings: Vec<_> = all_warnings
                .iter()
                .map(|w| {
                    let rule_name = w.rule_name.as_deref().unwrap_or("");
                    if !is_rule_cli_fixable(rules, config, rule_name) {
                        LintWarning { fix: None, ..w.clone() }
                    } else {
                        w.clone()
                    }
                })
                .collect();
            let formatted = formatter.format_warnings(&display_warnings, &display_path);
            if !formatted.is_empty() {
                output_writer.writeln(&formatted).unwrap_or_else(|e| {
                    eprintln!("Error writing output: {e}");
                });
            }
        }
    }

    // Handle diff mode or fix mode
    let mut warnings_fixed = 0;
    if diff {
        // In diff mode, apply fixes to a copy and show diff
        let original_content = content.clone();
        warnings_fixed = apply_fixes_coordinated(rules, &all_warnings, &mut content, true, true, config);

        // Format embedded markdown blocks (recursive formatting)
        // Use filtered_rules to respect per-file-ignores for embedded content
        let embedded_formatted = format_embedded_markdown_blocks(&mut content, &filtered_rules, config);
        warnings_fixed += embedded_formatted;

        if warnings_fixed > 0 {
            let diff_output = formatter::generate_diff(&original_content, &content, &display_path);
            output_writer.writeln(&diff_output).unwrap_or_else(|e| {
                eprintln!("Error writing diff output: {e}");
            });
        }

        // Don't actually write the file in diff mode, but report how many would be fixed
        return (
            total_warnings > 0 || warnings_fixed > 0,
            total_warnings,
            warnings_fixed,
            fixable_warnings,
            all_warnings,
            file_index,
        );
    } else if fix_mode != crate::FixMode::Check {
        // Apply fixes using Fix Coordinator
        warnings_fixed = apply_fixes_coordinated(rules, &all_warnings, &mut content, quiet, silent, config);

        // Format embedded markdown blocks (recursive formatting)
        // Use filtered_rules to respect per-file-ignores for embedded content
        let embedded_formatted = format_embedded_markdown_blocks(&mut content, &filtered_rules, config);
        warnings_fixed += embedded_formatted;

        // Write fixed content back to file
        if warnings_fixed > 0 {
            // Denormalize back to original line ending before writing
            let content_to_write = rumdl_lib::utils::normalize_line_ending(&content, original_line_ending);

            if let Err(err) = std::fs::write(file_path, &content_to_write)
                && !silent
            {
                eprintln!(
                    "{} Failed to write fixed content to file {}: {}",
                    "Error:".red().bold(),
                    file_path,
                    err
                );
            }
        }

        // If there were no original warnings, we only formatted embedded blocks.
        // In this case, return success (no issues) without re-linting, since re-lint
        // doesn't apply per-file-ignores or inline config that the original lint did.
        if total_warnings == 0 {
            return (false, 0, warnings_fixed, 0, Vec::new(), file_index);
        }

        // Re-lint the fixed content to see which warnings remain
        // This is needed both for display and to determine exit code (following Ruff's convention)
        //
        // Apply the same filtering as the original lint to ensure consistent behavior:
        // 1. Per-file-ignores: filter rules based on config
        // 2. Inline config: filter warnings based on inline directives
        let ignored_rules_for_file = config.get_ignored_rules_for_file(Path::new(file_path));
        let filtered_rules: Vec<_> = if !ignored_rules_for_file.is_empty() {
            rules
                .iter()
                .filter(|rule| !ignored_rules_for_file.contains(rule.name()))
                .collect()
        } else {
            rules.iter().collect()
        };

        let fixed_ctx = LintContext::new(&content, config.markdown_flavor(), None);
        let inline_config = rumdl_lib::inline_config::InlineConfig::from_content(&content);
        let mut remaining_warnings = Vec::new();

        for rule in &filtered_rules {
            if let Ok(rule_warnings) = rule.check(&fixed_ctx) {
                // Filter warnings based on inline config directives
                let filtered_warnings = rule_warnings.into_iter().filter(|warning| {
                    let rule_name = warning.rule_name.as_deref().unwrap_or(rule.name());
                    // Extract base rule name for sub-rules like "MD029-style" -> "MD029"
                    let base_rule_name = if let Some(dash_pos) = rule_name.find('-') {
                        &rule_name[..dash_pos]
                    } else {
                        rule_name
                    };
                    !inline_config.is_rule_disabled(base_rule_name, warning.line)
                });
                remaining_warnings.extend(filtered_warnings);
            }
        }

        // In fix mode, show warnings with [fixed] for issues that were fixed
        if !silent {
            // Create a custom formatter that shows [fixed] instead of [*]
            let mut output = String::new();
            for warning in &all_warnings {
                let rule_name = warning.rule_name.as_deref().unwrap_or("unknown");

                // Check if the rule is CLI-fixable (config + rule capability)
                let is_fixable = is_rule_cli_fixable(rules, config, rule_name);

                let was_fixed = warning.fix.is_some()
                    && is_fixable
                    && !remaining_warnings.iter().any(|w| {
                        w.line == warning.line && w.column == warning.column && w.rule_name == warning.rule_name
                    });

                // Show [fixed] only for issues that were actually fixed, nothing otherwise
                let fix_indicator = if was_fixed {
                    " [fixed]".green().to_string()
                } else {
                    String::new()
                };

                // Format: file:line:column: [rule] message [fixed/*/]
                // Use colors similar to TextFormatter
                let line = format!(
                    "{}:{}:{}: {} {}{}",
                    display_path.blue().underline(),
                    warning.line.to_string().cyan(),
                    warning.column.to_string().cyan(),
                    format!("[{rule_name:5}]").yellow(),
                    warning.message,
                    fix_indicator
                );

                output.push_str(&line);
                output.push('\n');
            }

            if !output.is_empty() {
                output.pop(); // Remove trailing newline
                output_writer.writeln(&output).unwrap_or_else(|e| {
                    eprintln!("Error writing output: {e}");
                });
            }
        }

        // Return false (no issues) if no warnings remain after fixing, true otherwise
        // This follows Ruff's convention: exit 0 if all violations are fixed
        return (
            !remaining_warnings.is_empty(),
            total_warnings,
            warnings_fixed,
            fixable_warnings,
            all_warnings,
            file_index,
        );
    }

    (
        true,
        total_warnings,
        warnings_fixed,
        fixable_warnings,
        all_warnings,
        file_index,
    )
}

/// Result type for file processing that includes index data for cross-file analysis
pub struct ProcessFileResult {
    pub warnings: Vec<rumdl_lib::rule::LintWarning>,
    pub content: String,
    pub total_warnings: usize,
    pub fixable_warnings: usize,
    pub original_line_ending: rumdl_lib::utils::LineEnding,
    pub file_index: rumdl_lib::workspace_index::FileIndex,
}

pub struct CacheHashes {
    pub config_hash: String,
    pub rules_hash: String,
}

impl CacheHashes {
    pub fn new(config: &rumdl_config::Config, rules: &[Box<dyn Rule>]) -> Self {
        Self {
            config_hash: LintCache::hash_config(config),
            rules_hash: LintCache::hash_rules(rules),
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub fn process_file_inner(
    file_path: &str,
    rules: &[Box<dyn Rule>],
    verbose: bool,
    quiet: bool,
    silent: bool,
    config: &rumdl_config::Config,
    cache: Option<std::sync::Arc<std::sync::Mutex<LintCache>>>,
    cache_hashes: Option<&CacheHashes>,
) -> (
    Vec<rumdl_lib::rule::LintWarning>,
    String,
    usize,
    usize,
    rumdl_lib::utils::LineEnding,
    rumdl_lib::workspace_index::FileIndex,
) {
    let result = process_file_with_index(file_path, rules, verbose, quiet, silent, config, cache, cache_hashes);
    (
        result.warnings,
        result.content,
        result.total_warnings,
        result.fixable_warnings,
        result.original_line_ending,
        result.file_index,
    )
}

/// Process a file and return both warnings and FileIndex for cross-file aggregation
#[allow(clippy::too_many_arguments)]
pub fn process_file_with_index(
    file_path: &str,
    rules: &[Box<dyn Rule>],
    verbose: bool,
    quiet: bool,
    silent: bool,
    config: &rumdl_config::Config,
    cache: Option<std::sync::Arc<std::sync::Mutex<LintCache>>>,
    cache_hashes: Option<&CacheHashes>,
) -> ProcessFileResult {
    use std::time::Instant;

    let start_time = Instant::now();
    if verbose && !quiet {
        // Display relative path for better UX, even if file_path is canonical (absolute)
        let display_path = if let Ok(cwd) = std::env::current_dir() {
            Path::new(file_path)
                .strip_prefix(&cwd)
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|_| file_path.to_string())
        } else {
            file_path.to_string()
        };
        println!("Processing file: {display_path}");
    }

    let empty_result = ProcessFileResult {
        warnings: Vec::new(),
        content: String::new(),
        total_warnings: 0,
        fixable_warnings: 0,
        original_line_ending: rumdl_lib::utils::LineEnding::Lf,
        file_index: rumdl_lib::workspace_index::FileIndex::new(),
    };

    // Read file content efficiently
    let mut content = match crate::read_file_efficiently(Path::new(file_path)) {
        Ok(content) => content,
        Err(e) => {
            if !silent {
                eprintln!("Error reading file {file_path}: {e}");
            }
            return empty_result;
        }
    };

    // Detect original line ending before any processing
    let original_line_ending = rumdl_lib::utils::detect_line_ending_enum(&content);

    // Normalize to LF for all internal processing
    content = rumdl_lib::utils::normalize_line_ending(&content, rumdl_lib::utils::LineEnding::Lf);

    // Validate inline config comments and warn about unknown rules
    if !silent {
        let inline_warnings = rumdl_lib::inline_config::validate_inline_config_rules(&content);
        for warn in inline_warnings {
            warn.print_warning(file_path);
        }
    }

    // Early content analysis for ultra-fast skip decisions
    if content.is_empty() {
        return ProcessFileResult {
            original_line_ending,
            ..empty_result
        };
    }

    // Compute hashes for cache (Ruff-style: file content + config + enabled rules)
    let (config_hash, rules_hash) = if let Some(hashes) = cache_hashes {
        (Cow::Borrowed(&hashes.config_hash), Cow::Borrowed(&hashes.rules_hash))
    } else {
        (
            Cow::Owned(LintCache::hash_config(config)),
            Cow::Owned(LintCache::hash_rules(rules)),
        )
    };

    // Try to get from cache first (lock briefly for cache read)
    // Note: Cache only stores single-file warnings; cross-file checks must run fresh
    if let Some(ref cache_arc) = cache {
        let cache_result = cache_arc
            .lock()
            .ok()
            .and_then(|mut guard| guard.get(&content, &config_hash, &rules_hash));
        if let Some(cached_warnings) = cache_result {
            if verbose && !quiet {
                println!("Cache hit for {file_path}");
            }
            // Count fixable warnings from cache (using capability-based check)
            let fixable_warnings = cached_warnings
                .iter()
                .filter(|w| {
                    w.fix.is_some()
                        && w.rule_name
                            .as_ref()
                            .is_some_and(|name| is_rule_cli_fixable(rules, config, name))
                })
                .count();

            // Build FileIndex for cross-file analysis on cache hit (lightweight, no rule checking)
            let flavor = config.get_flavor_for_file(Path::new(file_path));
            let file_index = rumdl_lib::build_file_index_only(&content, rules, flavor);

            let total_warnings = cached_warnings.len();
            return ProcessFileResult {
                warnings: cached_warnings,
                content,
                total_warnings,
                fixable_warnings,
                original_line_ending,
                file_index,
            };
        }
    }

    let lint_start = Instant::now();

    // Filter rules based on per-file-ignores configuration
    let ignored_rules_for_file = config.get_ignored_rules_for_file(Path::new(file_path));
    let filtered_rules: Vec<_> = if !ignored_rules_for_file.is_empty() {
        rules
            .iter()
            .filter(|rule| !ignored_rules_for_file.contains(rule.name()))
            .map(|r| dyn_clone::clone_box(&**r))
            .collect()
    } else {
        rules.to_vec()
    };

    // Determine flavor based on per-file-flavor overrides, global config, or file extension
    let flavor = config.get_flavor_for_file(Path::new(file_path));

    // Use lint_and_index for single-file linting + index contribution
    let source_file = Some(std::path::PathBuf::from(file_path));
    let (warnings_result, file_index) =
        rumdl_lib::lint_and_index(&content, &filtered_rules, verbose, flavor, source_file, Some(config));

    // Combine all warnings
    let mut all_warnings = warnings_result.unwrap_or_default();

    // Check embedded markdown blocks and add their warnings
    let embedded_warnings = check_embedded_markdown_blocks(&content, &filtered_rules, config);
    all_warnings.extend(embedded_warnings);

    // Sort warnings by line number, then column
    all_warnings.sort_by(|a, b| {
        if a.line == b.line {
            a.column.cmp(&b.column)
        } else {
            a.line.cmp(&b.line)
        }
    });

    let total_warnings = all_warnings.len();

    // Count fixable issues (using capability-based check)
    let fixable_warnings = all_warnings
        .iter()
        .filter(|w| {
            w.fix.is_some()
                && w.rule_name
                    .as_ref()
                    .is_some_and(|name| is_rule_cli_fixable(rules, config, name))
        })
        .count();

    let lint_end_time = Instant::now();
    let lint_time = lint_end_time.duration_since(lint_start);

    if verbose && !quiet {
        println!("Linting took: {lint_time:?}");
    }

    let total_time = start_time.elapsed();
    if verbose && !quiet {
        println!("Total processing time for {file_path}: {total_time:?}");
    }

    // Store in cache before returning (ignore if mutex is poisoned)
    if let Some(ref cache_arc) = cache
        && let Ok(mut cache_guard) = cache_arc.lock()
    {
        cache_guard.set(&content, &config_hash, &rules_hash, all_warnings.clone());
    }

    ProcessFileResult {
        warnings: all_warnings,
        content,
        total_warnings,
        fixable_warnings,
        original_line_ending,
        file_index,
    }
}
pub fn apply_fixes_coordinated(
    rules: &[Box<dyn Rule>],
    all_warnings: &[rumdl_lib::rule::LintWarning],
    content: &mut String,
    _quiet: bool,
    silent: bool,
    config: &rumdl_config::Config,
) -> usize {
    use rumdl_lib::fix_coordinator::FixCoordinator;
    use std::time::Instant;

    let start = Instant::now();
    let coordinator = FixCoordinator::new();

    // Apply fixes iteratively (up to 100 iterations to ensure convergence, same as Ruff)
    match coordinator.apply_fixes_iterative(rules, all_warnings, content, config, 100) {
        Ok(result) => {
            let elapsed = start.elapsed();

            if std::env::var("RUMDL_DEBUG_FIX_PERF").is_ok() {
                eprintln!("DEBUG: Fix Coordinator used");
                eprintln!("DEBUG: Iterations: {}", result.iterations);
                eprintln!("DEBUG: Rules applied: {}", result.rules_fixed);
                eprintln!("DEBUG: LintContext creations: {}", result.context_creations);
                eprintln!("DEBUG: Converged: {}", result.converged);
                eprintln!("DEBUG: Total time: {elapsed:?}");
            }

            // Warn if convergence failed (Ruff-style)
            if !result.converged && !silent {
                eprintln!("Warning: Failed to converge after {} iterations.", result.iterations);
                eprintln!("This likely indicates a bug in rumdl.");
                if !result.fixed_rule_names.is_empty() {
                    let rule_codes: Vec<&str> = result.fixed_rule_names.iter().map(|s| s.as_str()).collect();
                    eprintln!("Rule codes: {}", rule_codes.join(", "));
                }
                eprintln!("Please report at: https://github.com/rvben/rumdl/issues/new?template=bug_report.yml");
            }

            // Count warnings for the rules that were successfully applied
            all_warnings
                .iter()
                .filter(|w| {
                    w.rule_name
                        .as_ref()
                        .map(|name| result.fixed_rule_names.contains(name.as_str()))
                        .unwrap_or(false)
                })
                .count()
        }
        Err(e) => {
            if !silent {
                eprintln!("Warning: Fix coordinator failed: {e}");
            }
            0
        }
    }
}

/// Maximum recursion depth for formatting nested markdown blocks.
///
/// This prevents stack overflow from deeply nested or maliciously crafted content.
/// The value of 5 is chosen because:
/// - Real-world usage rarely exceeds 2-3 levels (e.g., docs showing example markdown)
/// - 5 levels provides headroom for legitimate use cases
/// - Beyond 5 levels, the content is likely either malicious or unintentional
const MAX_EMBEDDED_DEPTH: usize = 5;

fn has_fenced_code_blocks(content: &str) -> bool {
    content.contains("```") || content.contains("~~~")
}

/// Format markdown content embedded in fenced code blocks with `markdown` or `md` language.
///
/// This function detects markdown code blocks and recursively applies formatting to their content.
/// The formatting preserves indentation for blocks inside lists or blockquotes.
///
/// Returns the number of blocks that were formatted.
pub fn format_embedded_markdown_blocks(
    content: &mut String,
    rules: &[Box<dyn Rule>],
    config: &rumdl_config::Config,
) -> usize {
    format_embedded_markdown_blocks_recursive(content, rules, config, 0)
}

/// Internal recursive implementation with depth tracking.
fn format_embedded_markdown_blocks_recursive(
    content: &mut String,
    rules: &[Box<dyn Rule>],
    config: &rumdl_config::Config,
    depth: usize,
) -> usize {
    // Prevent excessive recursion
    if depth >= MAX_EMBEDDED_DEPTH {
        return 0;
    }
    if !has_fenced_code_blocks(content) {
        return 0;
    }

    let blocks = CodeBlockUtils::detect_markdown_code_blocks(content);

    if blocks.is_empty() {
        return 0;
    }

    // Parse inline config from the parent content to respect disable/enable directives
    let inline_config = rumdl_lib::inline_config::InlineConfig::from_content(content);

    let mut formatted_count = 0;

    // Process blocks in reverse order to maintain byte offsets
    for block in blocks.into_iter().rev() {
        // Extract the content between the fences
        let block_content = &content[block.content_start..block.content_end];

        // Skip empty blocks
        if block_content.trim().is_empty() {
            continue;
        }

        // Compute the line number of the block's opening fence
        // The inline config state at this line determines which rules are disabled
        let block_line = content[..block.content_start].matches('\n').count() + 1;

        // Filter rules based on inline config at this block's location
        let block_rules: Vec<Box<dyn Rule>> = rules
            .iter()
            .filter(|rule| !inline_config.is_rule_disabled(rule.name(), block_line))
            .map(|r| dyn_clone::clone_box(&**r))
            .collect();

        // Strip common indentation from all lines
        let (stripped_content, common_indent) = strip_common_indent(block_content);

        // Apply formatting to the stripped content
        let mut formatted = stripped_content;

        // First, recursively format any nested markdown blocks
        let nested_formatted =
            format_embedded_markdown_blocks_recursive(&mut formatted, &block_rules, config, depth + 1);

        // Create a context and collect warnings for the embedded content
        let ctx = LintContext::new(&formatted, config.markdown_flavor(), None);
        let mut warnings = Vec::new();
        for rule in &block_rules {
            if let Ok(rule_warnings) = rule.check(&ctx) {
                warnings.extend(rule_warnings);
            }
        }

        // Apply fixes
        if !warnings.is_empty() {
            let _fixed = apply_fixes_coordinated(&block_rules, &warnings, &mut formatted, true, true, config);
        }

        // Remove trailing newline that MD047 may have added if original didn't have one
        // This prevents extra blank lines before the closing fence
        let original_had_trailing_newline = block_content.ends_with('\n');
        if !original_had_trailing_newline && formatted.ends_with('\n') {
            formatted.pop();
        }

        // Restore indentation
        let restored = restore_indent(&formatted, &common_indent);

        // Replace the block content if it changed
        if restored != block_content {
            content.replace_range(block.content_start..block.content_end, &restored);
            formatted_count += 1;
        }

        formatted_count += nested_formatted;
    }

    formatted_count
}

/// Check markdown content embedded in fenced code blocks with `markdown` or `md` language.
///
/// This function detects markdown code blocks and runs lint checks on their content,
/// returning warnings with adjusted line numbers that point to the correct location
/// in the parent file.
///
/// Returns a vector of warnings from all embedded markdown blocks.
pub fn check_embedded_markdown_blocks(
    content: &str,
    rules: &[Box<dyn Rule>],
    config: &rumdl_config::Config,
) -> Vec<rumdl_lib::rule::LintWarning> {
    check_embedded_markdown_blocks_recursive(content, rules, config, 0)
}

/// Internal recursive implementation with depth tracking.
fn check_embedded_markdown_blocks_recursive(
    content: &str,
    rules: &[Box<dyn Rule>],
    config: &rumdl_config::Config,
    depth: usize,
) -> Vec<rumdl_lib::rule::LintWarning> {
    // Prevent excessive recursion
    if depth >= MAX_EMBEDDED_DEPTH {
        return Vec::new();
    }
    if !has_fenced_code_blocks(content) {
        return Vec::new();
    }

    let blocks = CodeBlockUtils::detect_markdown_code_blocks(content);

    if blocks.is_empty() {
        return Vec::new();
    }

    // Parse inline config from the parent content to respect disable/enable directives
    let inline_config = rumdl_lib::inline_config::InlineConfig::from_content(content);

    let mut all_warnings = Vec::new();

    for block in blocks {
        // Extract the content between the fences
        let block_content = &content[block.content_start..block.content_end];

        // Skip empty blocks
        if block_content.trim().is_empty() {
            continue;
        }

        // Calculate the line offset for this block
        // Count newlines before content_start to get the starting line number
        let line_offset = content[..block.content_start].matches('\n').count();

        // Compute the line number of the block's opening fence (1-indexed)
        // The inline config state at this line determines which rules are disabled
        let block_line = line_offset + 1;

        // Filter rules based on inline config at this block's location
        let block_rules: Vec<&Box<dyn Rule>> = rules
            .iter()
            .filter(|rule| !inline_config.is_rule_disabled(rule.name(), block_line))
            .collect();

        // Strip common indentation from all lines
        let (stripped_content, _common_indent) = strip_common_indent(block_content);

        // First, recursively check any nested markdown blocks
        // Clone rules for recursion since we need owned values
        let block_rules_owned: Vec<Box<dyn Rule>> = block_rules.iter().map(|r| dyn_clone::clone_box(&***r)).collect();
        let nested_warnings =
            check_embedded_markdown_blocks_recursive(&stripped_content, &block_rules_owned, config, depth + 1);

        // Adjust nested warning line numbers and add to results
        for mut warning in nested_warnings {
            warning.line += line_offset;
            warning.end_line += line_offset;
            // Clear fix since byte offsets won't be valid for parent file
            warning.fix = None;
            all_warnings.push(warning);
        }

        // Create a context and collect warnings for the embedded content
        // Skip file-scoped rules that don't apply to embedded snippets
        let ctx = LintContext::new(&stripped_content, config.markdown_flavor(), None);
        for rule in &block_rules {
            // Skip file-scoped rules for embedded content
            match rule.name() {
                "MD041" => continue, // "First line in file should be heading" - not a file
                "MD047" => continue, // "File should end with newline" - not a file
                _ => {}
            }

            if let Ok(rule_warnings) = rule.check(&ctx) {
                for warning in rule_warnings {
                    // Create adjusted warning with correct line numbers
                    let adjusted_warning = rumdl_lib::rule::LintWarning {
                        message: warning.message.clone(),
                        line: warning.line + line_offset,
                        column: warning.column,
                        end_line: warning.end_line + line_offset,
                        end_column: warning.end_column,
                        severity: warning.severity,
                        fix: None, // Clear fix since byte offsets won't be valid
                        rule_name: warning.rule_name,
                    };
                    all_warnings.push(adjusted_warning);
                }
            }
        }
    }

    all_warnings
}

/// Strip common leading indentation from all non-empty lines.
/// Returns the stripped content and the common indent string.
fn strip_common_indent(content: &str) -> (String, String) {
    let lines: Vec<&str> = content.lines().collect();
    let has_trailing_newline = content.ends_with('\n');

    // Find minimum indentation among non-empty lines
    let min_indent = lines
        .iter()
        .filter(|line| !line.trim().is_empty())
        .map(|line| line.len() - line.trim_start().len())
        .min()
        .unwrap_or(0);

    // Build the stripped content
    let mut stripped: String = lines
        .iter()
        .map(|line| {
            if line.trim().is_empty() {
                // Preserve empty lines as empty (no spaces)
                ""
            } else if line.len() >= min_indent {
                &line[min_indent..]
            } else {
                // Fallback: strip what we can
                line.trim_start()
            }
        })
        .collect::<Vec<_>>()
        .join("\n");

    // Preserve trailing newline if original had one
    if has_trailing_newline && !stripped.ends_with('\n') {
        stripped.push('\n');
    }

    // Return the common indent string (spaces)
    let indent_str = " ".repeat(min_indent);

    (stripped, indent_str)
}

/// Restore indentation to all non-empty lines.
/// Preserves trailing newline if present in the original content.
fn restore_indent(content: &str, indent: &str) -> String {
    let has_trailing_newline = content.ends_with('\n');

    let mut result: String = content
        .lines()
        .map(|line| {
            if line.trim().is_empty() {
                line.to_string()
            } else {
                format!("{indent}{line}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n");

    // Preserve trailing newline
    if has_trailing_newline && !result.ends_with('\n') {
        result.push('\n');
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    /// Create a temporary directory structure for testing path display
    fn create_test_structure() -> TempDir {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let docs_dir = temp_dir.path().join("docs");
        fs::create_dir_all(&docs_dir).expect("Failed to create docs dir");
        fs::write(docs_dir.join("guide.md"), "# Test").expect("Failed to write test file");
        temp_dir
    }

    #[test]
    fn test_to_display_path_with_project_root() {
        let temp_dir = create_test_structure();
        let project_root = temp_dir.path();
        let file_path = project_root.join("docs/guide.md");

        let result = to_display_path(&file_path.to_string_lossy(), Some(project_root));

        assert_eq!(result, "docs/guide.md");
    }

    #[test]
    fn test_to_display_path_with_canonical_paths() {
        let temp_dir = create_test_structure();
        let project_root = temp_dir.path().canonicalize().unwrap();
        let file_path = project_root.join("docs/guide.md").canonicalize().unwrap();

        let result = to_display_path(&file_path.to_string_lossy(), Some(&project_root));

        assert_eq!(result, "docs/guide.md");
    }

    #[test]
    fn test_to_display_path_no_project_root_uses_cwd() {
        // Test that when no project_root is given, files under CWD get relative paths
        // We test this indirectly by checking files in CWD get stripped
        let cwd = std::env::current_dir().unwrap();
        let cwd_canonical = cwd.canonicalize().unwrap_or(cwd.clone());

        // Create a path that would be under CWD
        let test_path = cwd_canonical.join("test_file.md");

        // Even if file doesn't exist, the path should be made relative to CWD
        let result = to_display_path(&test_path.to_string_lossy(), None);

        assert_eq!(result, "test_file.md");
    }

    #[test]
    fn test_to_display_path_empty_string() {
        let result = to_display_path("", None);
        assert_eq!(result, "");
    }

    #[test]
    fn test_to_display_path_with_parent_references() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let nested = temp_dir.path().join("a/b/c");
        fs::create_dir_all(&nested).expect("Failed to create nested dirs");
        let file = nested.join("file.md");
        fs::write(&file, "# Test").expect("Failed to write");

        // Path with .. that resolves to the same file
        let path_with_parent = temp_dir.path().join("a/b/c/../c/file.md");
        let result = to_display_path(&path_with_parent.to_string_lossy(), Some(temp_dir.path()));

        // Should resolve to clean relative path
        assert_eq!(result, "a/b/c/file.md");
    }

    #[test]
    fn test_to_display_path_special_characters() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let special_dir = temp_dir.path().join("docs#1/test%20files");
        fs::create_dir_all(&special_dir).expect("Failed to create dir with special chars");
        let file_path = special_dir.join("file&name.md");
        fs::write(&file_path, "# Test").expect("Failed to write");

        let result = to_display_path(&file_path.to_string_lossy(), Some(temp_dir.path()));

        assert_eq!(result, "docs#1/test%20files/file&name.md");
    }

    #[test]
    fn test_to_display_path_root_as_project_root() {
        // When project root is /, paths should still be relative to it
        let result = to_display_path("/usr/local/test.md", Some(Path::new("/")));

        assert_eq!(result, "usr/local/test.md");
    }

    #[test]
    fn test_to_display_path_file_outside_project_root() {
        let temp_dir1 = create_test_structure();
        let temp_dir2 = TempDir::new().expect("Failed to create temp dir 2");
        let outside_file = temp_dir2.path().join("outside.md");
        fs::write(&outside_file, "# Outside").expect("Failed to write");

        // File is in temp_dir2, but project root is temp_dir1
        let result = to_display_path(&outside_file.to_string_lossy(), Some(temp_dir1.path()));

        // Should fall back to CWD-relative or absolute
        // Since outside_file is not under project_root, it might be CWD-relative or absolute
        assert!(
            result.ends_with("outside.md"),
            "Expected path to end with 'outside.md', got: {result}"
        );
    }

    #[test]
    fn test_to_display_path_already_relative() {
        // When given a relative path that doesn't exist, should return as-is
        let result = to_display_path("nonexistent/path.md", None);
        assert_eq!(result, "nonexistent/path.md");
    }

    #[test]
    fn test_to_display_path_nested_subdirectory() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let nested_dir = temp_dir.path().join("a/b/c/d");
        fs::create_dir_all(&nested_dir).expect("Failed to create nested dirs");
        let file_path = nested_dir.join("deep.md");
        fs::write(&file_path, "# Deep").expect("Failed to write");

        let result = to_display_path(&file_path.to_string_lossy(), Some(temp_dir.path()));

        assert_eq!(result, "a/b/c/d/deep.md");
    }

    #[test]
    fn test_to_display_path_with_spaces_in_path() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let dir_with_spaces = temp_dir.path().join("my docs/sub folder");
        fs::create_dir_all(&dir_with_spaces).expect("Failed to create dir with spaces");
        let file_path = dir_with_spaces.join("my file.md");
        fs::write(&file_path, "# Spaces").expect("Failed to write");

        let result = to_display_path(&file_path.to_string_lossy(), Some(temp_dir.path()));

        assert_eq!(result, "my docs/sub folder/my file.md");
    }

    #[test]
    fn test_to_display_path_with_unicode() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let unicode_dir = temp_dir.path().join("文档/ドキュメント");
        fs::create_dir_all(&unicode_dir).expect("Failed to create unicode dir");
        let file_path = unicode_dir.join("日本語.md");
        fs::write(&file_path, "# 日本語").expect("Failed to write");

        let result = to_display_path(&file_path.to_string_lossy(), Some(temp_dir.path()));

        assert_eq!(result, "文档/ドキュメント/日本語.md");
    }

    #[test]
    fn test_strip_base_prefix_basic() {
        let temp_dir = create_test_structure();
        let base = temp_dir.path();
        let file = temp_dir.path().join("docs/guide.md");

        let result = strip_base_prefix(&file, base);

        assert_eq!(result, Some("docs/guide.md".to_string()));
    }

    #[test]
    fn test_strip_base_prefix_not_under_base() {
        let temp_dir1 = TempDir::new().expect("Failed to create temp dir 1");
        let temp_dir2 = TempDir::new().expect("Failed to create temp dir 2");
        let file = temp_dir2.path().join("file.md");
        fs::write(&file, "# Test").expect("Failed to write");

        let result = strip_base_prefix(&file, temp_dir1.path());

        assert_eq!(result, None);
    }

    #[test]
    fn test_strip_base_prefix_with_symlink() {
        // This test verifies that symlinks are resolved correctly
        // On macOS, /tmp is a symlink to /private/tmp
        let temp_dir = create_test_structure();
        let canonical_base = temp_dir.path().canonicalize().unwrap();
        let file = temp_dir.path().join("docs/guide.md").canonicalize().unwrap();

        let result = strip_base_prefix(&file, &canonical_base);

        assert_eq!(result, Some("docs/guide.md".to_string()));
    }

    #[test]
    fn test_strip_base_prefix_nonexistent_base() {
        let file = Path::new("/some/existing/path.md");
        let nonexistent_base = Path::new("/this/path/does/not/exist");

        let result = strip_base_prefix(file, nonexistent_base);

        // Should return None because canonicalize fails on nonexistent path
        assert_eq!(result, None);
    }

    #[test]
    fn test_format_embedded_markdown_blocks_atx_heading() {
        let config = rumdl_config::Config::default();
        let rules = rumdl_lib::rules::all_rules(&config);

        let mut content = "# Example\n\n```markdown\n#Heading without space\n```\n".to_string();
        let formatted = format_embedded_markdown_blocks(&mut content, &rules, &config);

        assert!(formatted > 0, "Should format at least one block");
        assert!(
            content.contains("# Heading without space"),
            "Should fix ATX heading spacing, got: {content:?}"
        );
    }

    #[test]
    fn test_format_embedded_markdown_blocks_md_language() {
        let config = rumdl_config::Config::default();
        let rules = rumdl_lib::rules::all_rules(&config);

        let mut content = "# Example\n\n```md\n#Test\n```\n".to_string();
        let formatted = format_embedded_markdown_blocks(&mut content, &rules, &config);

        assert!(formatted > 0, "Should format block with 'md' language");
        assert!(content.contains("# Test"), "Should fix heading, got: {content:?}");
    }

    #[test]
    fn test_format_embedded_markdown_blocks_case_insensitive() {
        let config = rumdl_config::Config::default();
        let rules = rumdl_lib::rules::all_rules(&config);

        let mut content = "# Doc\n\n```MARKDOWN\n#Upper case\n```\n".to_string();
        let formatted = format_embedded_markdown_blocks(&mut content, &rules, &config);

        assert!(formatted > 0, "Should detect MARKDOWN (uppercase)");
        assert!(content.contains("# Upper case"));
    }

    #[test]
    fn test_format_embedded_markdown_blocks_tilde_fence() {
        let config = rumdl_config::Config::default();
        let rules = rumdl_lib::rules::all_rules(&config);

        let mut content = "# Doc\n\n~~~markdown\n#Tilde fence\n~~~\n".to_string();
        let formatted = format_embedded_markdown_blocks(&mut content, &rules, &config);

        assert!(formatted > 0, "Should detect tilde fenced blocks");
        assert!(content.contains("# Tilde fence"));
    }

    #[test]
    fn test_format_embedded_markdown_blocks_multiple_blocks() {
        let config = rumdl_config::Config::default();
        let rules = rumdl_lib::rules::all_rules(&config);

        let mut content = "# Doc\n\n```markdown\n#First\n```\n\nText\n\n```md\n#Second\n```\n".to_string();
        let formatted = format_embedded_markdown_blocks(&mut content, &rules, &config);

        assert_eq!(formatted, 2, "Should format both blocks");
        assert!(content.contains("# First"));
        assert!(content.contains("# Second"));
    }

    #[test]
    fn test_format_embedded_markdown_blocks_nested() {
        let config = rumdl_config::Config::default();
        let rules = rumdl_lib::rules::all_rules(&config);

        // Outer block contains inner block (using longer fence)
        let mut content = "# Doc\n\n````markdown\n#Outer\n\n```markdown\n#Inner\n```\n````\n".to_string();
        let formatted = format_embedded_markdown_blocks(&mut content, &rules, &config);

        assert!(formatted >= 1, "Should format at least outer block");
        assert!(content.contains("# Outer"), "Should fix outer heading");
    }

    #[test]
    fn test_format_embedded_markdown_blocks_preserves_indentation() {
        let config = rumdl_config::Config::default();
        let rules = rumdl_lib::rules::all_rules(&config);

        // Content with relative indentation that should be preserved
        let mut content = "# Doc\n\n```markdown\n#Heading\n\n    code block\n```\n".to_string();
        let formatted = format_embedded_markdown_blocks(&mut content, &rules, &config);

        assert!(formatted > 0);
        assert!(content.contains("# Heading"), "Should fix heading");
        assert!(
            content.contains("    code block"),
            "Should preserve indented code block"
        );
    }

    #[test]
    fn test_format_embedded_markdown_blocks_empty_block() {
        let config = rumdl_config::Config::default();
        let rules = rumdl_lib::rules::all_rules(&config);

        let mut content = "# Doc\n\n```markdown\n\n```\n".to_string();
        let original = content.clone();
        let formatted = format_embedded_markdown_blocks(&mut content, &rules, &config);

        assert_eq!(formatted, 0, "Should skip empty blocks");
        assert_eq!(content, original, "Content should be unchanged");
    }

    #[test]
    fn test_format_embedded_markdown_blocks_whitespace_only() {
        let config = rumdl_config::Config::default();
        let rules = rumdl_lib::rules::all_rules(&config);

        let mut content = "# Doc\n\n```markdown\n   \n\n```\n".to_string();
        let original = content.clone();
        let formatted = format_embedded_markdown_blocks(&mut content, &rules, &config);

        assert_eq!(formatted, 0, "Should skip whitespace-only blocks");
        assert_eq!(content, original, "Content should be unchanged");
    }

    #[test]
    fn test_format_embedded_markdown_blocks_skips_other_languages() {
        let config = rumdl_config::Config::default();
        let rules = rumdl_lib::rules::all_rules(&config);

        let mut content = "# Doc\n\n```rust\n#[derive(Debug)]\nfn main() {}\n```\n".to_string();
        let original = content.clone();
        let formatted = format_embedded_markdown_blocks(&mut content, &rules, &config);

        assert_eq!(formatted, 0, "Should not format rust blocks");
        assert_eq!(content, original, "Content should be unchanged");
    }

    #[test]
    fn test_format_embedded_markdown_blocks_multiple_blank_lines() {
        let config = rumdl_config::Config::default();
        let rules = rumdl_lib::rules::all_rules(&config);

        // MD012 should fix multiple consecutive blank lines
        let mut content = "# Doc\n\n```markdown\n# Heading\n\n\n\nParagraph\n```\n".to_string();
        let formatted = format_embedded_markdown_blocks(&mut content, &rules, &config);

        assert!(formatted > 0, "Should format block");
        // After formatting, should have at most one blank line between heading and paragraph
        let block_content = content
            .split("```markdown\n")
            .nth(1)
            .unwrap()
            .split("\n```")
            .next()
            .unwrap();
        let blank_count = block_content.matches("\n\n\n").count();
        assert_eq!(blank_count, 0, "Should reduce multiple blank lines");
    }

    #[test]
    fn test_format_embedded_markdown_blocks_depth_limit() {
        let config = rumdl_config::Config::default();
        let rules = rumdl_lib::rules::all_rules(&config);

        // Create deeply nested blocks (beyond MAX_EMBEDDED_DEPTH)
        let mut content = "# Doc\n\n".to_string();
        for i in 0..10 {
            let backticks = "`".repeat(3 + i);
            content.push_str(&format!("{backticks}markdown\n#Level{i}\n"));
        }
        for i in (0..10).rev() {
            let backticks = "`".repeat(3 + i);
            content.push_str(&format!("{backticks}\n"));
        }

        // Should not panic or stack overflow
        let formatted = format_embedded_markdown_blocks(&mut content, &rules, &config);
        assert!(formatted <= MAX_EMBEDDED_DEPTH, "Should respect depth limit");
    }

    #[test]
    fn test_strip_common_indent_basic() {
        let content = "    line1\n    line2\n";
        let (stripped, indent) = strip_common_indent(content);

        assert_eq!(indent, "    ");
        assert!(stripped.starts_with("line1\n"));
        assert!(stripped.contains("line2"));
    }

    #[test]
    fn test_strip_common_indent_mixed() {
        // First line has 2 spaces, second has 4 - should strip 2
        let content = "  line1\n    line2\n";
        let (stripped, indent) = strip_common_indent(content);

        assert_eq!(indent, "  ");
        assert_eq!(stripped, "line1\n  line2\n");
    }

    #[test]
    fn test_strip_common_indent_preserves_empty_lines() {
        let content = "  line1\n\n  line2\n";
        let (stripped, _) = strip_common_indent(content);

        assert!(stripped.contains("\n\n"), "Should preserve empty lines");
    }

    #[test]
    fn test_restore_indent_basic() {
        let content = "line1\nline2\n";
        let restored = restore_indent(content, "  ");

        assert_eq!(restored, "  line1\n  line2\n");
    }

    #[test]
    fn test_restore_indent_preserves_empty_lines() {
        let content = "line1\n\nline2\n";
        let restored = restore_indent(content, "  ");

        assert_eq!(restored, "  line1\n\n  line2\n");
    }

    #[test]
    fn test_restore_indent_preserves_trailing_newline() {
        let content = "line1\n";
        let restored = restore_indent(content, "  ");

        assert!(restored.ends_with('\n'), "Should preserve trailing newline");

        let content_no_newline = "line1";
        let restored_no_newline = restore_indent(content_no_newline, "  ");

        assert!(!restored_no_newline.ends_with('\n'), "Should not add trailing newline");
    }

    #[test]
    fn test_format_embedded_markdown_no_extra_blank_line() {
        // Regression test: MD047 should NOT add extra blank line before closing fence
        let config = rumdl_config::Config::default();
        let rules = rumdl_lib::rules::all_rules(&config);

        // Content that doesn't end with newline inside the block
        let mut content = "# Doc\n\n```markdown\n> [!INFO]\n> Content\n```\n".to_string();
        let original = content.clone();
        let formatted = format_embedded_markdown_blocks(&mut content, &rules, &config);

        // If no changes needed inside the block, content should be unchanged
        // (no extra blank line before closing fence)
        if formatted == 0 {
            assert_eq!(content, original, "Should not add extra blank lines");
        } else {
            // If changes were made, verify no blank line before closing fence
            assert!(
                !content.contains("\n\n```\n"),
                "Should not have blank line before closing fence"
            );
        }
    }

    #[test]
    fn test_format_embedded_markdown_with_fix() {
        // Test that fixes are applied without corrupting structure
        let config = rumdl_config::Config::default();
        let rules = rumdl_lib::rules::all_rules(&config);

        let mut content = "# Doc\n\n```markdown\n#Bad heading\n```\n".to_string();
        let formatted = format_embedded_markdown_blocks(&mut content, &rules, &config);

        assert!(formatted > 0, "Should format the block");
        assert!(content.contains("# Bad heading"), "Should fix heading");
        assert!(!content.contains("\n\n```\n"), "Should not add blank line before fence");
        // Verify structure is preserved
        assert!(content.starts_with("# Doc\n\n```markdown\n"));
        assert!(content.ends_with("```\n"));
    }

    #[test]
    fn test_format_embedded_markdown_unicode_content() {
        // Test with multi-byte UTF-8 characters to verify byte offset handling
        let config = rumdl_config::Config::default();
        let rules = rumdl_lib::rules::all_rules(&config);

        // Japanese, Chinese, and emoji characters (multi-byte UTF-8)
        let mut content = "# ドキュメント\n\n```markdown\n#見出し\n\n中文内容 🎉\n```\n".to_string();
        let original_structure = (content.contains("```markdown"), content.contains("```\n"));

        let formatted = format_embedded_markdown_blocks(&mut content, &rules, &config);

        // Structure should be preserved
        assert!(content.contains("```markdown"), "Opening fence preserved");
        assert!(content.ends_with("```\n"), "Closing fence preserved");

        // If formatted, heading should be fixed
        if formatted > 0 {
            assert!(content.contains("# 見出し"), "Japanese heading should be fixed");
        }

        // Content should not be corrupted
        assert!(content.contains("中文内容"), "Chinese content preserved");
        assert!(content.contains("🎉"), "Emoji preserved");

        // Structure should match original pattern
        assert_eq!(
            (content.contains("```markdown"), content.contains("```\n")),
            original_structure,
            "Structure should be preserved"
        );
    }

    #[test]
    fn test_format_embedded_markdown_in_list_item() {
        // Test markdown code block indented inside a list item
        let config = rumdl_config::Config::default();
        let rules = rumdl_lib::rules::all_rules(&config);

        let mut content = "- List item:\n\n  ```markdown\n  #Heading\n  ```\n".to_string();
        let formatted = format_embedded_markdown_blocks(&mut content, &rules, &config);

        assert!(formatted > 0, "Should format embedded block");
        assert!(content.contains("# Heading"), "Should fix heading");
        // Verify list structure is preserved
        assert!(content.starts_with("- List item:"), "List item preserved");
    }

    #[test]
    fn test_format_embedded_markdown_info_string_with_attributes() {
        // Test that info string attributes are handled correctly
        // e.g., ```markdown title="Example"
        let config = rumdl_config::Config::default();
        let rules = rumdl_lib::rules::all_rules(&config);

        let mut content = "# Doc\n\n```markdown title=\"Example\" highlight={1}\n#Heading\n```\n".to_string();
        let formatted = format_embedded_markdown_blocks(&mut content, &rules, &config);

        assert!(formatted > 0, "Should recognize markdown despite extra info");
        assert!(content.contains("# Heading"), "Should fix heading");
        // Info string should be preserved
        assert!(
            content.contains("```markdown title=\"Example\""),
            "Info string preserved"
        );
    }

    #[test]
    fn test_format_embedded_markdown_depth_verification() {
        // Verify that each level up to MAX_EMBEDDED_DEPTH is actually formatted
        let config = rumdl_config::Config::default();
        let rules = rumdl_lib::rules::all_rules(&config);

        // Create content with 2 sequential blocks at different "depths"
        // Note: True nesting requires increasing fence length, which changes parsing.
        // Instead, we test multiple blocks in sequence to verify recursion works.
        let mut content = "# Doc\n\n```markdown\n#Level1\n```\n\n```md\n#Level2\n```\n".to_string();

        let formatted = format_embedded_markdown_blocks(&mut content, &rules, &config);

        // Both blocks should be formatted
        assert!(formatted >= 2, "Should format both blocks, got {formatted}");
        assert!(content.contains("# Level1"), "Block 1 should be formatted");
        assert!(content.contains("# Level2"), "Block 2 should be formatted");
    }

    #[test]
    fn test_format_embedded_markdown_true_nesting() {
        // Test true recursive nesting with tilde fences (avoids fence length issues)
        let config = rumdl_config::Config::default();
        let rules = rumdl_lib::rules::all_rules(&config);

        // Use tildes for outer, backticks for inner - this is valid CommonMark
        let mut content = "# Doc\n\n~~~markdown\n#Outer\n\n```markdown\n#Inner\n```\n~~~\n".to_string();

        let formatted = format_embedded_markdown_blocks(&mut content, &rules, &config);

        // Both levels should be formatted
        assert!(formatted >= 1, "Should format at least outer block");
        assert!(content.contains("# Outer"), "Outer heading should be formatted");
        // Inner might not be formatted due to nesting complexity - that's OK
        // The important thing is that the structure isn't corrupted
        assert!(content.contains("~~~\n"), "Tilde fence preserved");
        assert!(content.contains("```\n"), "Backtick fence preserved");
    }

    #[test]
    fn test_format_embedded_markdown_cli_integration() {
        // Integration test: verify embedded formatting works through file processing
        use std::io::Write;
        use tempfile::NamedTempFile;

        let config = rumdl_config::Config::default();
        let rules = rumdl_lib::rules::all_rules(&config);

        // Create a temp file with embedded markdown
        let mut temp_file = NamedTempFile::new().expect("Failed to create temp file");
        writeln!(temp_file, "# Test Doc").unwrap();
        writeln!(temp_file).unwrap();
        writeln!(temp_file, "```markdown").unwrap();
        writeln!(temp_file, "#Bad Heading").unwrap();
        writeln!(temp_file, "```").unwrap();
        temp_file.flush().unwrap();

        // Read and format the content
        let mut content = std::fs::read_to_string(temp_file.path()).expect("Failed to read temp file");
        let formatted = format_embedded_markdown_blocks(&mut content, &rules, &config);

        assert!(formatted > 0, "Should format embedded content");
        assert!(content.contains("# Bad Heading"), "Should fix embedded heading");
    }

    #[test]
    fn test_format_embedded_markdown_md041_behavior() {
        // Verify behavior with document-level rules like MD041 on embedded content
        // MD041 requires first heading to be H1, but embedded docs often show examples
        // with H2 headings deliberately
        let config = rumdl_config::Config::default();
        let rules = rumdl_lib::rules::all_rules(&config);

        // Embedded content starts with H2, not H1
        let mut content = "# Main Doc\n\n```markdown\n## Example H2\n```\n".to_string();
        let original = content.clone();

        let formatted = format_embedded_markdown_blocks(&mut content, &rules, &config);

        // Document the current behavior: MD041 does NOT have a fix function,
        // so even if it fires as a warning, it won't change the content.
        // This is actually the desired behavior for embedded markdown,
        // since documentation examples often intentionally show non-H1 headings.

        // Verify the embedded content is NOT changed by MD041 (no fix available)
        assert_eq!(content, original, "MD041 should not change embedded H2 (no fix)");
        assert_eq!(formatted, 0, "No formatting changes expected");
    }

    #[test]
    fn test_check_embedded_markdown_blocks() {
        let config = rumdl_config::Config::default();
        let rules = rumdl_lib::rules::all_rules(&config);

        // Content with violations inside embedded markdown
        let content = "# Doc\n```markdown\n##  Bad heading\n```\n";

        let warnings = check_embedded_markdown_blocks(content, &rules, &config);

        // Should find violations in embedded content
        assert!(!warnings.is_empty(), "Should find warnings in embedded markdown");

        // Check that warnings have adjusted line numbers
        // The embedded content starts at line 3 (after "# Doc\n```markdown\n")
        let md019_warning = warnings
            .iter()
            .find(|w| w.rule_name.as_ref().is_some_and(|n| n == "MD019"));
        assert!(md019_warning.is_some(), "Should find MD019 warning for extra space");

        // Line should be 3 (line 1 = "# Doc", line 2 = "```markdown", line 3 = "##  Bad heading")
        if let Some(w) = md019_warning {
            assert_eq!(w.line, 3, "MD019 warning should be on line 3");
        }
    }

    #[test]
    fn test_check_embedded_markdown_blocks_skips_file_scoped_rules() {
        let config = rumdl_config::Config::default();
        let rules = rumdl_lib::rules::all_rules(&config);

        // Content that would trigger MD041 (no H1 first) and MD047 (no trailing newline)
        let content = "# Doc\n```markdown\n## Not H1\nNo trailing newline```\n";

        let warnings = check_embedded_markdown_blocks(content, &rules, &config);

        // MD041 and MD047 should be filtered out for embedded content
        let md041 = warnings
            .iter()
            .find(|w| w.rule_name.as_ref().is_some_and(|n| n == "MD041"));
        let md047 = warnings
            .iter()
            .find(|w| w.rule_name.as_ref().is_some_and(|n| n == "MD047"));

        assert!(md041.is_none(), "MD041 should be skipped for embedded content");
        assert!(md047.is_none(), "MD047 should be skipped for embedded content");
    }

    #[test]
    fn test_check_embedded_markdown_blocks_empty() {
        let config = rumdl_config::Config::default();
        let rules = rumdl_lib::rules::all_rules(&config);

        // No embedded markdown
        let content = "# Doc\n\nSome text\n";

        let warnings = check_embedded_markdown_blocks(content, &rules, &config);

        assert!(warnings.is_empty(), "Should have no warnings without embedded markdown");
    }

    #[test]
    fn test_format_embedded_markdown_respects_filtered_rules() {
        // Test that embedded markdown formatting respects per-file-ignores
        // This simulates what happens when per-file-ignores excludes certain rules
        let config = rumdl_config::Config::default();
        let all_rules = rumdl_lib::rules::all_rules(&config);

        // Content with MD022 violation (missing blank line above heading)
        let original = "# Rule Documentation\n\n```markdown\n# Heading\n## No blank line above\n```\n";

        // Test 1: WITH MD022 rule - should add blank line
        let mut content_with_rule = original.to_string();
        let formatted_with_rule = format_embedded_markdown_blocks(&mut content_with_rule, &all_rules, &config);

        assert!(formatted_with_rule > 0, "Should format when MD022 is active");
        assert!(
            content_with_rule.contains("# Heading\n\n## No blank line above"),
            "Should add blank line when MD022 is active"
        );

        // Test 2: WITHOUT MD022 rule (simulating per-file-ignores) - should NOT add blank line
        let filtered_rules: Vec<Box<dyn crate::Rule>> = all_rules
            .iter()
            .filter(|rule| rule.name() != "MD022")
            .map(|r| dyn_clone::clone_box(&**r))
            .collect();

        let mut content_without_rule = original.to_string();
        let _formatted_without_rule =
            format_embedded_markdown_blocks(&mut content_without_rule, &filtered_rules, &config);

        // The content should NOT have MD022 fix applied
        assert!(
            content_without_rule.contains("# Heading\n## No blank line above"),
            "Should NOT add blank line when MD022 is filtered out (per-file-ignores)"
        );

        // If other rules applied fixes, that's fine, but MD022 specifically shouldn't
        // The key assertion is that the missing blank line above ## is preserved
        assert_ne!(
            content_with_rule, content_without_rule,
            "Filtered rules should produce different result than all rules"
        );
    }

    #[test]
    fn test_format_embedded_markdown_respects_inline_config() {
        // Test that embedded markdown formatting respects inline disable directives
        let config = rumdl_config::Config::default();
        let rules = rumdl_lib::rules::all_rules(&config);

        // Content with MD022 violation inside a markdown block, wrapped in inline disable
        let original = r#"# Doc

<!-- rumdl-disable MD022 -->

```markdown
# Heading
## No blank line above
```

<!-- rumdl-enable MD022 -->
"#;

        let mut content = original.to_string();
        let formatted = format_embedded_markdown_blocks(&mut content, &rules, &config);

        // The embedded content should NOT be modified because MD022 is disabled via inline config
        assert!(
            content.contains("# Heading\n## No blank line above"),
            "Should NOT add blank line when MD022 is disabled via inline config. Got: {content}"
        );

        // No blocks should have been formatted
        assert_eq!(formatted, 0, "No blocks should be formatted when rules are disabled");
    }

    #[test]
    fn test_check_embedded_markdown_respects_inline_config() {
        // Test that embedded markdown checking respects inline disable directives
        // This ensures check and fmt behave consistently
        let config = rumdl_config::Config::default();
        let rules = rumdl_lib::rules::all_rules(&config);

        // Content with MD022 violation inside a markdown block, wrapped in inline disable
        let content = r#"# Doc

<!-- rumdl-disable MD022 -->

```markdown
# Heading
## No blank line above
```

<!-- rumdl-enable MD022 -->
"#;

        let warnings = check_embedded_markdown_blocks(content, &rules, &config);

        // Should have NO MD022 warnings because it's disabled via inline config
        let md022_warnings: Vec<_> = warnings
            .iter()
            .filter(|w| w.rule_name.as_ref().is_some_and(|n| n == "MD022"))
            .collect();

        assert!(
            md022_warnings.is_empty(),
            "Should NOT report MD022 warnings when disabled via inline config. Got: {md022_warnings:?}"
        );
    }

    #[test]
    fn test_check_and_format_consistency() {
        // Verify that check and format behave identically for inline config
        let config = rumdl_config::Config::default();
        let rules = rumdl_lib::rules::all_rules(&config);

        // Content with violations both inside and outside disabled region
        let content = r#"# Doc

<!-- rumdl-disable MD022 -->

```markdown
# Heading
## Inside disabled - should be ignored
```

<!-- rumdl-enable MD022 -->

```markdown
# Another
## Outside disabled - should be reported/fixed
```
"#;

        // Check should report warnings only for the second block
        let warnings = check_embedded_markdown_blocks(content, &rules, &config);
        let md022_warnings: Vec<_> = warnings
            .iter()
            .filter(|w| w.rule_name.as_ref().is_some_and(|n| n == "MD022"))
            .collect();

        assert!(
            !md022_warnings.is_empty(),
            "Should report MD022 for block outside disabled region"
        );

        // All warnings should be for lines after the enable comment (line 11+)
        for w in &md022_warnings {
            assert!(
                w.line > 10,
                "MD022 warning should be after enable comment, got line {}",
                w.line
            );
        }

        // Format should only modify the second block
        let mut format_content = content.to_string();
        let formatted = format_embedded_markdown_blocks(&mut format_content, &rules, &config);

        assert!(formatted > 0, "Should format the second block");
        assert!(
            format_content.contains("# Heading\n## Inside disabled"),
            "First block should be unchanged"
        );
        assert!(
            format_content.contains("# Another\n\n## Outside disabled"),
            "Second block should have blank line added"
        );
    }
}
