//! Watch mode functionality for continuous linting

use crate::check_runner::{CheckRunContext, perform_check_run};
use chrono::Local;
use colored::*;
use notify::{Config as NotifyConfig, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use rumdl_lib::config as rumdl_config;
use std::io::{self, Write};
use std::path::Path;
use std::sync::mpsc::channel;
use std::time::{Duration, Instant};

pub enum ChangeKind {
    Configuration,
    SourceFile,
}

/// Detects what kind of change occurred based on the file extension
pub fn change_detected(event: &Event) -> Option<ChangeKind> {
    // Skip access and other non-modification events
    if !matches!(
        event.kind,
        EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_)
    ) {
        return None;
    }

    let mut source_file = false;
    for path in &event.paths {
        // Check if this is a configuration file
        if let Some(file_name) = path.file_name().and_then(|n| n.to_str()) {
            // Check for rumdl-specific config files
            if matches!(
                file_name,
                ".rumdl.toml"
                    | "rumdl.toml"
                    | "pyproject.toml"
                    | ".markdownlint.json"
                    | ".markdownlint.jsonc"
                    | ".markdownlint.yaml"
                    | ".markdownlint.yml"
                    | "markdownlint.json"
                    | "markdownlint.jsonc"
                    | "markdownlint.yaml"
                    | "markdownlint.yml"
            ) {
                return Some(ChangeKind::Configuration);
            }
        }

        // Check for markdown files
        if let Some(extension) = path.extension()
            && matches!(extension.to_str(), Some("md" | "markdown" | "mdown" | "mkd" | "mdx"))
        {
            source_file = true;
        }
    }

    if source_file {
        Some(ChangeKind::SourceFile)
    } else {
        None
    }
}

/// Clear the terminal screen
pub fn clear_screen() {
    // ANSI escape sequence to clear screen and move cursor to top-left
    print!("\x1B[2J\x1B[1;1H");
    let _ = io::stdout().flush();
}

/// Run the linter in watch mode, re-running on file changes
pub fn run_watch_mode(args: &crate::CheckArgs, global_config_path: Option<&str>, isolated: bool, quiet: bool) {
    // Always use current directory for config discovery to ensure config files are found
    // when pre-commit or other tools pass relative file paths
    let discovery_dir = None;

    // Load initial configuration
    let mut sourced = crate::load_config_with_cli_error_handling_with_dir(global_config_path, isolated, discovery_dir);

    // Apply CLI argument overrides (e.g., --flavor)
    crate::apply_cli_overrides(&mut sourced, args);

    // Validate configuration
    let registry = rumdl_config::default_registry();
    let validation_warnings = rumdl_config::validate_config_sourced(&sourced, registry);
    if !validation_warnings.is_empty() && !args.silent {
        for warn in &validation_warnings {
            eprintln!("\x1b[33m[config warning]\x1b[0m {}", warn.message);
        }
    }

    // Extract project_root before converting to Config (for exclude pattern resolution)
    let mut project_root = sourced.project_root.clone();

    // Convert to Config (watch mode doesn't need validation warnings)
    let mut config: rumdl_config::Config = sourced.clone().into_validated_unchecked().into();

    // Configure the file watcher
    let (tx, rx) = channel();

    let mut watcher = match RecommendedWatcher::new(
        tx,
        NotifyConfig::default().with_poll_interval(Duration::from_millis(500)),
    ) {
        Ok(w) => w,
        Err(e) => {
            eprintln!("{}: Failed to create file watcher: {}", "Error".red().bold(), e);
            crate::exit::tool_error();
        }
    };

    // Watch directories for markdown and config files
    let watch_paths = if args.paths.is_empty() {
        vec![".".to_string()]
    } else {
        args.paths.clone()
    };

    for path_str in &watch_paths {
        let path = Path::new(path_str);
        if let Err(e) = watcher.watch(path, RecursiveMode::Recursive) {
            eprintln!("{}: Failed to watch {}: {}", "Warning".yellow().bold(), path_str, e);
        }
    }

    // Also watch configuration files
    if let Some(config_path) = global_config_path
        && let Err(e) = watcher.watch(Path::new(config_path), RecursiveMode::NonRecursive)
    {
        eprintln!("{}: Failed to watch config file: {}", "Warning".yellow().bold(), e);
    }

    // Perform initial run
    clear_screen();
    let timestamp = Local::now().format("%H:%M:%S");
    println!("[{}] {}...", timestamp, "Starting linter in watch mode".green().bold());
    println!("{}", "Press Ctrl-C to exit".cyan());
    println!();

    let explicit_config = global_config_path.is_some();
    let _has_issues = perform_check_run(&CheckRunContext {
        args,
        config: &config,
        quiet,
        cache: None,
        workspace_cache_dir: None,
        project_root: project_root.as_deref(),
        explicit_config,
        isolated,
    });
    if !quiet {
        println!("\n{}", "Watching for file changes...".cyan());
    }

    // Main watch loop with improved debouncing
    let debounce_duration = Duration::from_millis(100); // 100ms debounce - responsive while catching most duplicate events

    loop {
        match rx.recv() {
            Ok(event_result) => {
                match event_result {
                    Ok(first_event) => {
                        // Check what kind of change occurred
                        let Some(mut change_kind) = change_detected(&first_event) else {
                            continue;
                        };

                        // Collect all events that occur within the debounce window
                        let start = Instant::now();
                        while start.elapsed() < debounce_duration {
                            // Try to receive more events with a short timeout
                            if let Ok(Ok(event)) = rx.recv_timeout(Duration::from_millis(10)) {
                                // If we get a config change, that takes priority
                                if let Some(kind) = change_detected(&event)
                                    && matches!(kind, ChangeKind::Configuration)
                                {
                                    change_kind = ChangeKind::Configuration;
                                }
                            }
                        }

                        // Handle configuration changes if needed
                        if matches!(change_kind, ChangeKind::Configuration) {
                            // Reload configuration
                            sourced = crate::load_config_with_cli_error_handling_with_dir(
                                global_config_path,
                                isolated,
                                discovery_dir,
                            );

                            // Re-apply CLI argument overrides (e.g., --flavor)
                            crate::apply_cli_overrides(&mut sourced, args);

                            // Re-validate configuration
                            let validation_warnings = rumdl_config::validate_config_sourced(&sourced, registry);
                            if !validation_warnings.is_empty() && !args.silent {
                                for warn in &validation_warnings {
                                    eprintln!("\x1b[33m[config warning]\x1b[0m {}", warn.message);
                                }
                            }

                            // Update project_root from reloaded config
                            project_root = sourced.project_root.clone();
                            config = sourced.clone().into_validated_unchecked().into();
                        }

                        // Build the header message before clearing
                        let timestamp = chrono::Local::now().format("%H:%M:%S");
                        let header = match change_kind {
                            ChangeKind::Configuration => {
                                format!(
                                    "[{}] {}...\n\n",
                                    timestamp,
                                    "Configuration change detected".yellow().bold()
                                )
                            }
                            ChangeKind::SourceFile => {
                                format!("[{}] {}...\n\n", timestamp, "File change detected".cyan().bold())
                            }
                        };

                        // Clear and immediately print header
                        clear_screen();
                        print!("{header}");
                        let _ = io::stdout().flush();

                        // Re-run the check
                        let _has_issues = perform_check_run(&CheckRunContext {
                            args,
                            config: &config,
                            quiet,
                            cache: None,
                            workspace_cache_dir: None,
                            project_root: project_root.as_deref(),
                            explicit_config,
                            isolated,
                        });
                        if !quiet {
                            println!("\n{}", "Watching for file changes...".cyan());
                        }
                    }
                    Err(e) => {
                        eprintln!("{}: Watch error: {}", "Error".red().bold(), e);
                    }
                }
            }
            Err(e) => {
                eprintln!("{}: Failed to receive watch event: {}", "Error".red().bold(), e);
                crate::exit::tool_error();
            }
        }
    }
}
