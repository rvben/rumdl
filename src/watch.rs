//! Watch mode functionality for continuous linting

use crate::check_runner::{CheckRunContext, perform_check_run};
use chrono::Local;
use colored::*;
use notify::{Config as NotifyConfig, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use rumdl_lib::config as rumdl_config;
use rumdl_lib::config::MARKDOWNLINT_CONFIG_FILES;
use std::io::{self, Write};
use std::path::Path;
use std::sync::mpsc::channel;
use std::time::{Duration, Instant};

pub enum ChangeKind {
    Configuration,
    SourceFile,
}

/// Detects what kind of change occurred based on the file extension.
///
/// `editorconfig` is the current config's opt-in. An `.editorconfig` counts as a
/// configuration file only while rumdl reads it, so a project that never opted in
/// is not re-linted for an edit that cannot change its result.
pub fn change_detected(event: &Event, editorconfig: bool) -> Option<ChangeKind> {
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
        if let Some(file_name) = path.file_name().and_then(|n| n.to_str())
            && (matches!(file_name, ".rumdl.toml" | "rumdl.toml" | "pyproject.toml")
                || MARKDOWNLINT_CONFIG_FILES.contains(&file_name)
                || (editorconfig && file_name == ".editorconfig"))
        {
            return Some(ChangeKind::Configuration);
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
pub fn run_watch_mode(
    args: &crate::CheckArgs,
    global_config_path: Option<&str>,
    isolated: bool,
    quiet: bool,
    inline_overrides: &[toml::Table],
) {
    // Always use current directory for config discovery to ensure config files are found
    // when pre-commit or other tools pass relative file paths
    let discovery_dir = None;

    // Load initial configuration
    let mut sourced = crate::load_config_with_cli_error_handling_with_dir(global_config_path, isolated, discovery_dir);

    // Apply inline `--config` rule overrides at CLI precedence
    crate::cli_config_override::apply_inline_overrides(&mut sourced, inline_overrides);

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

    // Convert to Config (watch mode doesn't need validation warnings). The
    // validated sourced form is kept alongside it for `.editorconfig` layering,
    // which needs each setting's provenance.
    let mut validated = sourced.clone().into_validated_unchecked();
    let mut config: rumdl_config::Config = validated.clone().into();

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
    let outcome = perform_check_run(&CheckRunContext {
        args,
        config: &config,
        sourced: &validated,
        quiet,
        cache: None,
        workspace_cache_dir: None,
        project_root: project_root.as_deref(),
        grouping_root: project_root.as_deref(),
        inline_overrides,
        explicit_config,
        isolated,
        // Watch never owns a process exit and re-runs continuously; the
        // --deny-config-warnings decision does not apply here.
        external_config_warning: false,
    });
    if !quiet {
        println!("\n{}", "Watching for file changes...".cyan());
    }

    // Whether an `.editorconfig` edit can change a result here. The opt-in may
    // live in a subdirectory config, which only the run itself resolves, so it
    // is read back from the run as well as from the root config: a run that
    // linted nothing answers for no subdirectory.
    let mut reads_editorconfig = config.global.editorconfig || outcome.reads_editorconfig;

    // Main watch loop with improved debouncing
    let debounce_duration = Duration::from_millis(100); // 100ms debounce - responsive while catching most duplicate events

    loop {
        match rx.recv() {
            Ok(event_result) => {
                match event_result {
                    Ok(first_event) => {
                        // Check what kind of change occurred
                        let Some(mut change_kind) = change_detected(&first_event, reads_editorconfig) else {
                            continue;
                        };

                        // Collect all events that occur within the debounce window
                        let start = Instant::now();
                        while start.elapsed() < debounce_duration {
                            // Try to receive more events with a short timeout
                            if let Ok(Ok(event)) = rx.recv_timeout(Duration::from_millis(10)) {
                                // If we get a config change, that takes priority
                                if let Some(kind) = change_detected(&event, reads_editorconfig)
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

                            // Re-apply inline `--config` rule overrides
                            crate::cli_config_override::apply_inline_overrides(&mut sourced, inline_overrides);

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
                            validated = sourced.clone().into_validated_unchecked();
                            config = validated.clone().into();
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
                        let outcome = perform_check_run(&CheckRunContext {
                            args,
                            config: &config,
                            sourced: &validated,
                            quiet,
                            cache: None,
                            workspace_cache_dir: None,
                            project_root: project_root.as_deref(),
                            grouping_root: project_root.as_deref(),
                            inline_overrides,
                            explicit_config,
                            isolated,
                            // Watch never owns a process exit; the flag does not apply.
                            external_config_warning: false,
                        });
                        reads_editorconfig = config.global.editorconfig || outcome.reads_editorconfig;
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

#[cfg(test)]
mod tests {
    use super::*;
    use notify::event::{CreateKind, ModifyKind};
    use std::path::PathBuf;

    fn modified(path: &str) -> Event {
        Event {
            kind: EventKind::Modify(ModifyKind::Any),
            paths: vec![PathBuf::from(path)],
            attrs: Default::default(),
        }
    }

    #[test]
    fn a_markdown_edit_is_a_source_change() {
        assert!(matches!(
            change_detected(&modified("docs/guide.md"), false),
            Some(ChangeKind::SourceFile)
        ));
    }

    #[test]
    fn a_rumdl_config_edit_is_a_configuration_change() {
        assert!(matches!(
            change_detected(&modified(".rumdl.toml"), false),
            Some(ChangeKind::Configuration)
        ));
    }

    #[test]
    fn an_editorconfig_edit_is_a_configuration_change_when_rumdl_reads_it() {
        assert!(
            matches!(
                change_detected(&modified("docs/.editorconfig"), true),
                Some(ChangeKind::Configuration)
            ),
            "an opted-in project must re-lint when its .editorconfig changes"
        );
    }

    #[test]
    fn an_editorconfig_edit_is_ignored_when_rumdl_does_not_read_it() {
        assert!(
            change_detected(&modified(".editorconfig"), false).is_none(),
            "without the opt-in the file cannot change the result, so it must not trigger a run"
        );
    }

    #[test]
    fn a_file_rumdl_never_lints_is_ignored() {
        assert!(change_detected(&modified("src/main.rs"), true).is_none());
    }

    #[test]
    fn a_new_editorconfig_counts_like_an_edited_one() {
        let event = Event {
            kind: EventKind::Create(CreateKind::File),
            paths: vec![PathBuf::from(".editorconfig")],
            attrs: Default::default(),
        };
        assert!(matches!(change_detected(&event, true), Some(ChangeKind::Configuration)));
    }
}
