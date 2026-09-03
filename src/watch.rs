//! Watch mode functionality for continuous linting

use crate::check_runner::{CheckRunContext, perform_check_run};
use chrono::Local;
use colored::*;
use notify::{Config as NotifyConfig, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use rumdl_lib::config as rumdl_config;
use rumdl_lib::config::MARKDOWNLINT_CONFIG_FILES;
use std::collections::BTreeSet;
use std::ffi::OsStr;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::mpsc::channel;
use std::time::{Duration, Instant};

pub enum ChangeKind {
    Configuration,
    SourceFile,
}

/// What the watcher subscribed to, and therefore what an event can mean.
#[derive(Default)]
pub struct WatchScope {
    /// Whether an `.editorconfig` edit can change this run's result. The file
    /// supplies settings only while a config opts into reading it, so a project
    /// that never opted in is not re-linted for an edit that cannot matter.
    pub reads_editorconfig: bool,
    /// Directories subscribed for the configs they hold rather than for their
    /// contents, because they sit above the paths the run was given.
    config_dirs: BTreeSet<PathBuf>,
    /// The paths the run was pointed at that are single files. One of those can
    /// live in a configuration directory, and it is still part of the run even
    /// though its neighbours there are not.
    watched_files: BTreeSet<PathBuf>,
}

impl WatchScope {
    /// Whether a path is one this run only reads configuration from.
    ///
    /// The directories are canonical, so a path in another representation simply
    /// does not match and is treated as part of the run: an extra re-lint costs a
    /// redraw, while a missed one leaves stale output on screen.
    fn config_only(&self, path: &Path) -> bool {
        path.parent().is_some_and(|parent| self.config_dirs.contains(parent)) && !self.watched_files.contains(path)
    }
}

/// Detects what kind of change occurred based on the file extension.
pub fn change_detected(event: &Event, scope: &WatchScope) -> Option<ChangeKind> {
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
                || (scope.reads_editorconfig && file_name == ".editorconfig"))
        {
            return Some(ChangeKind::Configuration);
        }

        // Check for markdown files
        if let Some(extension) = path.extension()
            && matches!(extension.to_str(), Some("md" | "markdown" | "mdown" | "mkd" | "mdx"))
            && !scope.config_only(path)
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

/// The directories to subscribe to beyond the watched paths themselves, and the
/// watched paths that are single files.
///
/// A run reads the configs above the paths it was given, so watching a
/// subdirectory or a single file still has to notice an edit to a config that
/// sits higher up. Those directories are subscribed non-recursively, up to and
/// including the project root; without a project root only the directory holding
/// each watched path is, since nothing says how far up the project reaches.
///
/// The files come back canonicalized because one of them can sit in a directory
/// subscribed this way, where it is the only thing the run lints.
fn config_directories(watch_paths: &[String], project_root: Option<&Path>) -> (BTreeSet<PathBuf>, BTreeSet<PathBuf>) {
    let root = project_root.and_then(|root| root.canonicalize().ok());

    let mut watched_dirs: Vec<PathBuf> = Vec::new();
    let mut watched_files: BTreeSet<PathBuf> = BTreeSet::new();
    for path in watch_paths {
        let Ok(canonical) = Path::new(path).canonicalize() else {
            continue;
        };
        if canonical.is_dir() {
            watched_dirs.push(canonical);
        } else {
            watched_files.insert(canonical);
        }
    }

    let mut subscribe: BTreeSet<PathBuf> = BTreeSet::new();
    let starts = watched_dirs
        .iter()
        .map(PathBuf::as_path)
        .chain(watched_files.iter().filter_map(|file| file.parent()));
    for start in starts {
        subscribe.extend(ancestors_up_to(start, root.as_deref()));
    }
    // A project can keep its config in a `.config` subdirectory, which a
    // non-recursive subscription to the directory above it does not reach.
    let nested: Vec<PathBuf> = subscribe
        .iter()
        .map(|dir| dir.join(".config"))
        .filter(|dir| dir.is_dir())
        .collect();
    subscribe.extend(nested);

    // Anything inside a watched directory already arrives through that
    // directory's recursive subscription. Subscribing it again here would
    // shallow it: notify records recursion per exact path, so a non-recursive
    // watch on a directory below a recursive one stops the subdirectories
    // created there later from being watched at all.
    subscribe.retain(|dir| !watched_dirs.iter().any(|watched| dir.starts_with(watched)));

    (subscribe, watched_files)
}

/// The `.config` directories an event brought into scope: ones that now exist
/// under a directory watched for configuration, and that are not watched yet.
///
/// Such a directory holds a config from the moment it exists, and the directory
/// above it is watched non-recursively, so a file written into it would deliver
/// no event until the watcher subscribes to it as well.
///
/// The event kind is not consulted. A directory arrives either created in place
/// or moved in, and the kinds a platform reports for a move differ; asking
/// whether the path is a directory the watcher lacks answers for both.
fn appeared_config_dirs<'a>(event: &'a Event, watched: &BTreeSet<PathBuf>) -> Vec<&'a Path> {
    event
        .paths
        .iter()
        .filter(|path| {
            path.file_name() == Some(OsStr::new(".config"))
                && !watched.contains(path.as_path())
                && path.parent().is_some_and(|parent| watched.contains(parent))
                && path.is_dir()
        })
        .map(PathBuf::as_path)
        .collect()
}

/// Subscribe to the config directories an event brought into scope, and answer
/// whether any appeared.
fn subscribe_config_dirs(watcher: &mut RecommendedWatcher, scope: &mut WatchScope, event: &Event) -> bool {
    let mut appeared = false;
    for dir in appeared_config_dirs(event, &scope.config_dirs) {
        match watcher.watch(dir, RecursiveMode::NonRecursive) {
            Ok(()) => {
                scope.config_dirs.insert(dir.to_path_buf());
                appeared = true;
            }
            Err(e) => eprintln!(
                "{}: Failed to watch {}: {}",
                "Warning".yellow().bold(),
                dir.display(),
                e
            ),
        }
    }
    appeared
}

/// `start` and each directory above it, up to and including `root`.
///
/// Stops at `start` when there is no root, or when `start` lies outside it.
fn ancestors_up_to(start: &Path, root: Option<&Path>) -> Vec<PathBuf> {
    let mut dirs = vec![start.to_path_buf()];
    let Some(root) = root.filter(|root| start.starts_with(root)) else {
        return dirs;
    };

    let mut dir = start;
    while dir != root
        && let Some(parent) = dir.parent()
    {
        dirs.push(parent.to_path_buf());
        dir = parent;
    }
    dirs
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
    crate::apply_runtime_cli_overrides(&mut config, args);

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

    // And the directories above them, which hold configs the run reads without
    // linting anything in them.
    let (config_dirs, watched_files) = config_directories(&watch_paths, project_root.as_deref());
    for dir in &config_dirs {
        if let Err(e) = watcher.watch(dir, RecursiveMode::NonRecursive) {
            eprintln!(
                "{}: Failed to watch {}: {}",
                "Warning".yellow().bold(),
                dir.display(),
                e
            );
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
    let mut scope = WatchScope {
        reads_editorconfig: config.global.editorconfig || outcome.reads_editorconfig,
        config_dirs,
        watched_files,
    };

    // Main watch loop with improved debouncing
    let debounce_duration = Duration::from_millis(100); // 100ms debounce - responsive while catching most duplicate events

    loop {
        match rx.recv() {
            Ok(event_result) => {
                match event_result {
                    Ok(first_event) => {
                        // Check what kind of change occurred. A config directory
                        // that just appeared is one, whatever it holds already.
                        let appeared = subscribe_config_dirs(&mut watcher, &mut scope, &first_event);
                        let detected = change_detected(&first_event, &scope);
                        let Some(mut change_kind) = (if appeared {
                            Some(ChangeKind::Configuration)
                        } else {
                            detected
                        }) else {
                            continue;
                        };

                        // Collect all events that occur within the debounce window
                        let start = Instant::now();
                        while start.elapsed() < debounce_duration {
                            // Try to receive more events with a short timeout
                            if let Ok(Ok(event)) = rx.recv_timeout(Duration::from_millis(10)) {
                                // If we get a config change, that takes priority
                                if subscribe_config_dirs(&mut watcher, &mut scope, &event) {
                                    change_kind = ChangeKind::Configuration;
                                }
                                if let Some(kind) = change_detected(&event, &scope)
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
                            crate::apply_runtime_cli_overrides(&mut config, args);
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
                        scope.reads_editorconfig = config.global.editorconfig || outcome.reads_editorconfig;
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
    use notify::event::{CreateKind, ModifyKind, RenameMode};
    use std::path::PathBuf;

    fn modified(path: &str) -> Event {
        Event {
            kind: EventKind::Modify(ModifyKind::Any),
            paths: vec![PathBuf::from(path)],
            attrs: Default::default(),
        }
    }

    /// A scope over paths that are all part of the run.
    fn scope(reads_editorconfig: bool) -> WatchScope {
        WatchScope {
            reads_editorconfig,
            ..Default::default()
        }
    }

    /// A scope whose one configuration directory holds `watched_files`.
    fn config_dir_scope(dir: &str, watched_files: &[&str]) -> WatchScope {
        WatchScope {
            reads_editorconfig: true,
            config_dirs: BTreeSet::from([PathBuf::from(dir)]),
            watched_files: watched_files.iter().map(PathBuf::from).collect(),
        }
    }

    #[test]
    fn a_markdown_edit_is_a_source_change() {
        assert!(matches!(
            change_detected(&modified("docs/guide.md"), &scope(false)),
            Some(ChangeKind::SourceFile)
        ));
    }

    #[test]
    fn a_rumdl_config_edit_is_a_configuration_change() {
        assert!(matches!(
            change_detected(&modified(".rumdl.toml"), &scope(false)),
            Some(ChangeKind::Configuration)
        ));
    }

    #[test]
    fn an_editorconfig_edit_is_a_configuration_change_when_rumdl_reads_it() {
        assert!(
            matches!(
                change_detected(&modified("docs/.editorconfig"), &scope(true)),
                Some(ChangeKind::Configuration)
            ),
            "an opted-in project must re-lint when its .editorconfig changes"
        );
    }

    #[test]
    fn an_editorconfig_edit_is_ignored_when_rumdl_does_not_read_it() {
        assert!(
            change_detected(&modified(".editorconfig"), &scope(false)).is_none(),
            "without the opt-in the file cannot change the result, so it must not trigger a run"
        );
    }

    #[test]
    fn a_file_rumdl_never_lints_is_ignored() {
        assert!(change_detected(&modified("src/main.rs"), &scope(true)).is_none());
    }

    #[test]
    fn a_new_editorconfig_counts_like_an_edited_one() {
        let event = Event {
            kind: EventKind::Create(CreateKind::File),
            paths: vec![PathBuf::from(".editorconfig")],
            attrs: Default::default(),
        };
        assert!(matches!(
            change_detected(&event, &scope(true)),
            Some(ChangeKind::Configuration)
        ));
    }

    #[test]
    fn a_config_edit_above_the_watched_path_is_a_configuration_change() {
        assert!(
            matches!(
                change_detected(&modified("/project/.rumdl.toml"), &config_dir_scope("/project", &[])),
                Some(ChangeKind::Configuration)
            ),
            "the run reads that config, so an edit to it has to re-run the check"
        );
    }

    #[test]
    fn a_markdown_edit_above_the_watched_path_is_not_a_source_change() {
        assert!(
            change_detected(&modified("/project/README.md"), &config_dir_scope("/project", &[])).is_none(),
            "that directory is watched for its configs; the file itself is not being linted"
        );
    }

    #[test]
    fn a_watched_file_is_a_source_change_but_the_neighbours_it_shares_a_directory_with_are_not() {
        let watch = config_dir_scope("/project", &["/project/doc.md"]);
        assert!(
            matches!(
                change_detected(&modified("/project/doc.md"), &watch),
                Some(ChangeKind::SourceFile)
            ),
            "the run was pointed at this file, so its edits are the point of watching"
        );
        assert!(
            change_detected(&modified("/project/other.md"), &watch).is_none(),
            "its directory is subscribed for configs; the run never lints the file next to it"
        );
    }

    #[test]
    fn the_directories_watched_for_configs_reach_the_project_root() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        std::fs::create_dir_all(root.join("docs/guide")).unwrap();

        let (subscribe, watched_files) =
            config_directories(&[root.join("docs/guide").to_string_lossy().into_owned()], Some(&root));

        assert_eq!(
            subscribe,
            BTreeSet::from([root.clone(), root.join("docs")]),
            "the watched directory arrives recursively; everything above it up to the root does not"
        );
        assert!(
            watched_files.is_empty(),
            "and a directory target names no file that has to stay in scope"
        );
    }

    #[test]
    fn a_dot_config_directory_above_the_watched_path_is_watched_too() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        std::fs::create_dir_all(root.join("docs")).unwrap();
        std::fs::create_dir_all(root.join(".config")).unwrap();
        std::fs::write(root.join(".config/rumdl.toml"), "").unwrap();

        let (subscribe, _) = config_directories(&[root.join("docs").to_string_lossy().into_owned()], Some(&root));

        assert!(
            subscribe.contains(&root.join(".config")),
            "a config kept there is discovered, so it has to be watched, got {subscribe:?}"
        );
    }

    /// Every subscription is non-recursive, and notify records recursion per
    /// exact path, so one landing inside a recursively watched directory would
    /// stop subdirectories created there later from being watched at all.
    #[test]
    fn nothing_inside_a_watched_tree_is_subscribed_again() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        std::fs::create_dir_all(root.join("docs/guide")).unwrap();
        std::fs::create_dir_all(root.join("docs/.config")).unwrap();
        std::fs::write(root.join("docs/guide/doc.md"), "# Title\n").unwrap();

        for below in ["docs/guide", "docs/guide/doc.md"] {
            let (subscribe, _) = config_directories(
                &[
                    root.join("docs").to_string_lossy().into_owned(),
                    root.join(below).to_string_lossy().into_owned(),
                ],
                Some(&root),
            );

            assert_eq!(
                subscribe,
                BTreeSet::from([root.clone()]),
                "the recursive watch on docs covers everything under it, including its .config, \
                 so watching {below} too may add nothing below docs; only the root above it stays"
            );
        }
    }

    #[test]
    fn a_dot_config_directory_that_appears_while_watching_is_picked_up() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        std::fs::create_dir_all(root.join(".config")).unwrap();
        let watched = BTreeSet::from([root.clone()]);

        // Created in place, and moved in from elsewhere, which platforms report
        // as anything from a create to a rename to a plain modification.
        for kind in [
            EventKind::Create(CreateKind::Folder),
            EventKind::Modify(ModifyKind::Name(RenameMode::To)),
            EventKind::Modify(ModifyKind::Any),
        ] {
            let event = Event {
                kind,
                paths: vec![root.join(".config")],
                attrs: Default::default(),
            };
            assert_eq!(
                appeared_config_dirs(&event, &watched),
                vec![root.join(".config")],
                "a config written into it afterwards has to be noticed, whatever {kind:?} says"
            );
        }

        let created = Event {
            kind: EventKind::Create(CreateKind::Folder),
            paths: vec![root.join(".config")],
            attrs: Default::default(),
        };
        assert!(
            appeared_config_dirs(&created, &BTreeSet::from([root.join("elsewhere")])).is_empty(),
            "a directory this run does not read configuration from stays out of scope"
        );
        assert!(
            appeared_config_dirs(&created, &BTreeSet::from([root.clone(), root.join(".config")])).is_empty(),
            "and one the watcher already follows is not resubscribed on every event"
        );
    }

    #[test]
    fn the_directory_holding_a_watched_file_is_watched_and_the_file_stays_in_scope() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        std::fs::write(root.join("doc.md"), "# Title\n").unwrap();

        let (subscribe, watched_files) =
            config_directories(&[root.join("doc.md").to_string_lossy().into_owned()], None);

        assert_eq!(
            subscribe,
            BTreeSet::from([root.clone()]),
            "a config beside the watched file still has to be noticed"
        );
        assert_eq!(
            watched_files,
            BTreeSet::from([root.join("doc.md")]),
            "and the watched file itself lives there, so its edits still count"
        );
    }

    #[test]
    fn a_nested_watched_directory_adds_no_subscription_of_its_own() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        std::fs::create_dir_all(root.join("docs")).unwrap();

        let (subscribe, watched_files) = config_directories(
            &[
                root.to_string_lossy().into_owned(),
                root.join("docs").to_string_lossy().into_owned(),
            ],
            Some(&root),
        );

        assert!(
            subscribe.is_empty(),
            "both are watched recursively already, got {subscribe:?}"
        );
        assert!(watched_files.is_empty(), "and neither target is a single file");
    }
}
