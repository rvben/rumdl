//! File discovery, path utilities, and pattern expansion

use core::error::Error;
use ignore::WalkBuilder;
use ignore::overrides::OverrideBuilder;
use rumdl_config::{resolve_rule_name, resolve_rule_names};
use rumdl_lib::config as rumdl_config;
use rumdl_lib::rule::Rule;
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
