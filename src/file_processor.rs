//! File processing and linting logic

use crate::formatter;
use colored::*;
use ignore::WalkBuilder;
use ignore::overrides::OverrideBuilder;
use rumdl_config::normalize_key;
use rumdl_lib::config as rumdl_config;
use rumdl_lib::lint_context::LintContext;
use rumdl_lib::rule::Rule;
use std::collections::HashSet;
use std::error::Error;
use std::path::Path;

pub fn get_enabled_rules_from_checkargs(args: &crate::CheckArgs, config: &rumdl_config::Config) -> Vec<Box<dyn Rule>> {
    // 1. Initialize all available rules using from_config only
    let all_rules: Vec<Box<dyn Rule>> = rumdl_lib::rules::all_rules(config);

    // 2. Determine the final list of enabled rules based on precedence
    let final_rules: Vec<Box<dyn Rule>>;

    // Rule names provided via CLI flags
    let cli_enable_set: Option<HashSet<&str>> = args
        .enable
        .as_deref()
        .map(|s| s.split(',').map(|r| r.trim()).filter(|r| !r.is_empty()).collect());
    let cli_disable_set: Option<HashSet<&str>> = args
        .disable
        .as_deref()
        .map(|s| s.split(',').map(|r| r.trim()).filter(|r| !r.is_empty()).collect());
    let cli_extend_enable_set: Option<HashSet<&str>> = args
        .extend_enable
        .as_deref()
        .map(|s| s.split(',').map(|r| r.trim()).filter(|r| !r.is_empty()).collect());
    let cli_extend_disable_set: Option<HashSet<&str>> = args
        .extend_disable
        .as_deref()
        .map(|s| s.split(',').map(|r| r.trim()).filter(|r| !r.is_empty()).collect());

    // Rule names provided via config file
    let config_enable_set: HashSet<&str> = config.global.enable.iter().map(|s| s.as_str()).collect();

    let config_disable_set: HashSet<&str> = config.global.disable.iter().map(|s| s.as_str()).collect();

    if let Some(enabled_cli) = &cli_enable_set {
        // CLI --enable completely overrides config (ruff --select behavior)
        let enabled_cli_normalized: HashSet<String> = enabled_cli.iter().map(|s| normalize_key(s)).collect();
        let _all_rule_names: Vec<String> = all_rules.iter().map(|r| normalize_key(r.name())).collect();
        let mut filtered_rules = all_rules
            .into_iter()
            .filter(|rule| enabled_cli_normalized.contains(&normalize_key(rule.name())))
            .collect::<Vec<_>>();

        // Apply CLI --disable to remove rules from the enabled set (ruff-like behavior)
        if let Some(disabled_cli) = &cli_disable_set {
            filtered_rules.retain(|rule| {
                let rule_name_upper = rule.name();
                let rule_name_lower = normalize_key(rule_name_upper);
                !disabled_cli.contains(rule_name_upper) && !disabled_cli.contains(rule_name_lower.as_str())
            });
        }

        final_rules = filtered_rules;
    } else if cli_extend_enable_set.is_some() || cli_extend_disable_set.is_some() {
        // Handle extend flags (additive with config)
        let mut current_rules = all_rules;

        // Start with config enable if present
        if !config_enable_set.is_empty() {
            current_rules.retain(|rule| {
                let normalized_rule_name = normalize_key(rule.name());
                config_enable_set.contains(normalized_rule_name.as_str())
            });
        }

        // Add CLI extend-enable rules
        if let Some(extend_enabled_cli) = &cli_extend_enable_set {
            // If we started with all rules (no config enable), keep all rules
            // If we started with config enable, we need to re-filter with extended set
            if !config_enable_set.is_empty() {
                let mut extended_enable_set = config_enable_set.clone();
                for rule in extend_enabled_cli {
                    extended_enable_set.insert(rule);
                }

                // Re-filter with extended set
                current_rules = rumdl_lib::rules::all_rules(config)
                    .into_iter()
                    .filter(|rule| {
                        let normalized_rule_name = normalize_key(rule.name());
                        extended_enable_set.contains(normalized_rule_name.as_str())
                    })
                    .collect();
            }
        }

        // Apply config disable
        if !config_disable_set.is_empty() {
            current_rules.retain(|rule| {
                let normalized_rule_name = normalize_key(rule.name());
                !config_disable_set.contains(normalized_rule_name.as_str())
            });
        }

        // Apply CLI extend-disable
        if let Some(extend_disabled_cli) = &cli_extend_disable_set {
            current_rules.retain(|rule| {
                let rule_name_upper = rule.name();
                let rule_name_lower = normalize_key(rule_name_upper);
                !extend_disabled_cli.contains(rule_name_upper)
                    && !extend_disabled_cli.contains(rule_name_lower.as_str())
            });
        }

        // Apply CLI disable
        if let Some(disabled_cli) = &cli_disable_set {
            current_rules.retain(|rule| {
                let rule_name_upper = rule.name();
                let rule_name_lower = normalize_key(rule_name_upper);
                !disabled_cli.contains(rule_name_upper) && !disabled_cli.contains(rule_name_lower.as_str())
            });
        }

        final_rules = current_rules;
    } else {
        // --- Case 2: No CLI --enable ---
        // Start with the configured rules.
        let mut current_rules = all_rules;

        // Step 2a: Apply config `enable` (if specified).
        // If config.enable is not empty, it acts as an *exclusive* list.
        if !config_enable_set.is_empty() {
            current_rules.retain(|rule| {
                let normalized_rule_name = normalize_key(rule.name());
                config_enable_set.contains(normalized_rule_name.as_str())
            });
        }

        // Step 2b: Apply config `disable`.
        // Remove rules specified in config.disable from the current set.
        if !config_disable_set.is_empty() {
            current_rules.retain(|rule| {
                let normalized_rule_name = normalize_key(rule.name());
                let is_disabled = config_disable_set.contains(normalized_rule_name.as_str());
                !is_disabled // Keep if NOT disabled
            });
        }

        // Step 2c: Apply CLI `disable`.
        // Remove rules specified in cli.disable from the result of steps 2a & 2b.
        if let Some(disabled_cli) = &cli_disable_set {
            current_rules.retain(|rule| {
                let rule_name_upper = rule.name();
                let rule_name_lower = normalize_key(rule_name_upper);
                !disabled_cli.contains(rule_name_upper) && !disabled_cli.contains(rule_name_lower.as_str())
            });
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
pub fn find_markdown_files(
    paths: &[String],
    args: &crate::CheckArgs,
    config: &rumdl_config::Config,
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
    let mut types_builder = ignore::types::TypesBuilder::new();
    types_builder.add_defaults(); // Add standard types
    types_builder.add("markdown", "*.md").unwrap();
    types_builder.add("markdown", "*.markdown").unwrap();
    types_builder.select("markdown"); // Select ONLY markdown for processing
    let types = types_builder.build().unwrap();
    walk_builder.types(types);
    // -----------------------------------------

    // Determine if running in discovery mode (e.g., "rumdl ." or "rumdl check ." or "rumdl check")
    // Adjusted to handle both legacy and subcommand paths
    let is_discovery_mode = paths.is_empty() || paths == ["."];

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
        // 3. Default include (*.md, *.markdown) ONLY in discovery mode if no CLI/Config include
        vec!["*.md".to_string(), "*.markdown".to_string()]
    } else {
        // 4. Explicit path mode: No includes applied by default. Walk starts from explicit paths.
        Vec::new()
    };

    // Exclude patterns: CLI > Config (but disabled if --no-exclude is set)
    let final_exclude_patterns: Vec<String> = if args.no_exclude {
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

    // Debug: Log exclude patterns
    if args.verbose {
        eprintln!("Exclude patterns: {final_exclude_patterns:?}");
    }
    // --- End Pattern Determination ---

    // Apply overrides using the determined patterns
    if !final_include_patterns.is_empty() || !final_exclude_patterns.is_empty() {
        let mut override_builder = OverrideBuilder::new("."); // Root context

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
    let use_gitignore = if args.respect_gitignore {
        true // If respect is true, always include gitignore
    } else {
        false // If respect is false, always exclude gitignore
    };

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
            // If it's a file, check if it's a markdown file
            if path.is_file()
                && let Some(ext) = path.extension()
                && (ext == "md" || ext == "markdown")
            {
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
                    let mut matching_pattern: Option<&str> = None;
                    for pattern in &final_exclude_patterns {
                        // Use globset for pattern matching
                        if let Ok(glob) = globset::Glob::new(pattern) {
                            let matcher = glob.compile_matcher();
                            if matcher.is_match(&cleaned_path) {
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
                        file_paths.push(cleaned_path);
                    }
                } else {
                    // No exclude patterns, add the file
                    file_paths.push(cleaned_path);
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
                    file_paths.push(cleaned_path);
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

    // --- Final Explicit Markdown Filter ---
    // Ensure only files with .md or .markdown extensions are returned,
    // regardless of how ignore crate overrides interacted with type filters.
    file_paths.retain(|path_str| {
        let path = Path::new(path_str);
        path.extension().is_some_and(|ext| ext == "md" || ext == "markdown")
    });
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

#[allow(clippy::too_many_arguments)]
pub fn process_file_with_formatter(
    file_path: &str,
    rules: &[Box<dyn Rule>],
    _fix: bool,
    diff: bool,
    verbose: bool,
    quiet: bool,
    output_format: &rumdl_lib::output::OutputFormat,
    output_writer: &rumdl_lib::output::OutputWriter,
    config: &rumdl_config::Config,
) -> (bool, usize, usize, usize, Vec<rumdl_lib::rule::LintWarning>) {
    let formatter = output_format.create_formatter();

    // Call the original process_file_inner to get warnings and original line ending
    let (all_warnings, mut content, total_warnings, fixable_warnings, original_line_ending) =
        process_file_inner(file_path, rules, verbose, quiet, config);

    if total_warnings == 0 {
        return (false, 0, 0, 0, Vec::new());
    }

    // Format and output warnings
    if !quiet && !_fix {
        if diff {
            // In diff mode, only show warnings for unfixable issues
            let unfixable_warnings: Vec<_> = all_warnings.iter().filter(|w| w.fix.is_none()).cloned().collect();

            if !unfixable_warnings.is_empty() {
                let formatted = formatter.format_warnings(&unfixable_warnings, file_path);
                if !formatted.is_empty() {
                    output_writer.writeln(&formatted).unwrap_or_else(|e| {
                        eprintln!("Error writing output: {e}");
                    });
                }
            }
        } else {
            // In check mode, show all warnings with [*] for fixable issues
            let formatted = formatter.format_warnings(&all_warnings, file_path);
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
        warnings_fixed = apply_fixes_coordinated(rules, &all_warnings, &mut content, true, config);

        if warnings_fixed > 0 {
            let diff_output = formatter::generate_diff(&original_content, &content, file_path);
            output_writer.writeln(&diff_output).unwrap_or_else(|e| {
                eprintln!("Error writing diff output: {e}");
            });
        }

        // Don't actually write the file in diff mode
        return (total_warnings > 0, total_warnings, 0, fixable_warnings, all_warnings);
    } else if _fix {
        // Apply fixes using Fix Coordinator
        warnings_fixed = apply_fixes_coordinated(rules, &all_warnings, &mut content, quiet, config);

        // Write fixed content back to file
        if warnings_fixed > 0 {
            // Denormalize back to original line ending before writing
            let content_to_write = rumdl_lib::utils::normalize_line_ending(&content, original_line_ending);

            if let Err(err) = std::fs::write(file_path, &content_to_write)
                && !quiet
            {
                eprintln!(
                    "{} Failed to write fixed content to file {}: {}",
                    "Error:".red().bold(),
                    file_path,
                    err
                );
            }
        }

        // In fix mode, show warnings with [fixed] for issues that were fixed
        if !quiet {
            // Re-lint the fixed content to see which warnings remain
            let fixed_ctx = LintContext::new(&content, config.markdown_flavor());
            let mut remaining_warnings = Vec::new();

            for rule in rules {
                if let Ok(rule_warnings) = rule.check(&fixed_ctx) {
                    remaining_warnings.extend(rule_warnings);
                }
            }

            // Create a custom formatter that shows [fixed] instead of [*]
            let mut output = String::new();
            for warning in &all_warnings {
                let rule_name = warning.rule_name.unwrap_or("unknown");

                // Check if the rule is actually fixable based on configuration
                let is_fixable = is_rule_actually_fixable(config, rule_name);

                let was_fixed = warning.fix.is_some()
                    && is_fixable
                    && !remaining_warnings.iter().any(|w| {
                        w.line == warning.line && w.column == warning.column && w.rule_name == warning.rule_name
                    });

                let fix_indicator = if warning.fix.is_some() {
                    if !is_fixable {
                        " [unfixable]".yellow().to_string()
                    } else if was_fixed {
                        " [fixed]".green().to_string()
                    } else {
                        String::new()
                    }
                } else {
                    String::new()
                };

                // Format: file:line:column: [rule] message [fixed/*/]
                // Use colors similar to TextFormatter
                let line = format!(
                    "{}:{}:{}: {} {}{}",
                    file_path.blue().underline(),
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
    }

    (true, total_warnings, warnings_fixed, fixable_warnings, all_warnings)
}
pub fn process_file_inner(
    file_path: &str,
    rules: &[Box<dyn Rule>],
    verbose: bool,
    quiet: bool,
    config: &rumdl_config::Config,
) -> (
    Vec<rumdl_lib::rule::LintWarning>,
    String,
    usize,
    usize,
    rumdl_lib::utils::LineEnding,
) {
    use std::time::Instant;

    let start_time = Instant::now();
    if verbose && !quiet {
        println!("Processing file: {file_path}");
    }

    // Read file content efficiently
    let mut content = match crate::read_file_efficiently(Path::new(file_path)) {
        Ok(content) => content,
        Err(e) => {
            if !quiet {
                eprintln!("Error reading file {file_path}: {e}");
            }
            return (Vec::new(), String::new(), 0, 0, rumdl_lib::utils::LineEnding::Lf);
        }
    };

    // Detect original line ending before any processing
    let original_line_ending = rumdl_lib::utils::detect_line_ending_enum(&content);

    // Normalize to LF for all internal processing
    content = rumdl_lib::utils::normalize_line_ending(&content, rumdl_lib::utils::LineEnding::Lf);

    // Early content analysis for ultra-fast skip decisions
    if content.is_empty() {
        return (Vec::new(), String::new(), 0, 0, original_line_ending);
    }

    let lint_start = Instant::now();
    // Set the environment variable for the file path
    // This allows rules like MD057 to know which file is being processed
    unsafe {
        std::env::set_var("RUMDL_FILE_PATH", file_path);
    }

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

    // Use the standard lint function with the configured flavor
    let warnings_result = rumdl_lib::lint(&content, &filtered_rules, verbose, config.markdown_flavor());

    // Clear the environment variable after processing
    unsafe {
        std::env::remove_var("RUMDL_FILE_PATH");
    }

    // Combine all warnings
    let mut all_warnings = warnings_result.unwrap_or_default();

    // Sort warnings by line number, then column
    all_warnings.sort_by(|a, b| {
        if a.line == b.line {
            a.column.cmp(&b.column)
        } else {
            a.line.cmp(&b.line)
        }
    });

    let total_warnings = all_warnings.len();

    // Count fixable issues (excluding unfixable rules)
    let fixable_warnings = all_warnings
        .iter()
        .filter(|w| w.fix.is_some() && w.rule_name.is_some_and(|name| is_rule_actually_fixable(config, name)))
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

    (
        all_warnings,
        content,
        total_warnings,
        fixable_warnings,
        original_line_ending,
    )
}
pub fn apply_fixes_coordinated(
    rules: &[Box<dyn Rule>],
    all_warnings: &[rumdl_lib::rule::LintWarning],
    content: &mut String,
    quiet: bool,
    config: &rumdl_config::Config,
) -> usize {
    use rumdl_lib::fix_coordinator::FixCoordinator;
    use std::time::Instant;

    let start = Instant::now();
    let coordinator = FixCoordinator::new();

    // Apply fixes iteratively (up to 10 iterations to ensure convergence)
    match coordinator.apply_fixes_iterative(rules, all_warnings, content, config, 10) {
        Ok((rules_applied, iterations, ctx_creations, fixed_rule_names)) => {
            let elapsed = start.elapsed();

            if std::env::var("RUMDL_DEBUG_FIX_PERF").is_ok() {
                eprintln!("DEBUG: Fix Coordinator used");
                eprintln!("DEBUG: Iterations: {iterations}");
                eprintln!("DEBUG: Rules applied: {rules_applied}");
                eprintln!("DEBUG: LintContext creations: {ctx_creations}");
                eprintln!("DEBUG: Total time: {elapsed:?}");
            }

            // Count warnings for the rules that were successfully applied
            all_warnings
                .iter()
                .filter(|w| w.rule_name.map(|name| fixed_rule_names.contains(name)).unwrap_or(false))
                .count()
        }
        Err(e) => {
            if !quiet {
                eprintln!("Warning: Fix coordinator failed: {e}");
            }
            0
        }
    }
}
pub fn process_file_collect_warnings(
    file_path: &str,
    rules: &[Box<dyn Rule>],
    _fix: bool,
    verbose: bool,
    quiet: bool,
    config: &rumdl_config::Config,
) -> Vec<rumdl_lib::rule::LintWarning> {
    if verbose && !quiet {
        println!("Processing file: {file_path}");
    }

    // Read file content efficiently
    let content = match crate::read_file_efficiently(Path::new(file_path)) {
        Ok(content) => content,
        Err(e) => {
            if !quiet {
                eprintln!("Error reading file {file_path}: {e}");
            }
            return Vec::new();
        }
    };

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

    unsafe {
        std::env::set_var("RUMDL_FILE_PATH", file_path);
    }
    let warnings_result = rumdl_lib::lint(&content, &filtered_rules, verbose, config.markdown_flavor());
    unsafe {
        std::env::remove_var("RUMDL_FILE_PATH");
    }
    let mut all_warnings = warnings_result.unwrap_or_default();
    all_warnings.sort_by(|a, b| {
        if a.line == b.line {
            a.column.cmp(&b.column)
        } else {
            a.line.cmp(&b.line)
        }
    });
    all_warnings
}
