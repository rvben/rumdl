//! Stdin processing for markdown linting

use crate::file_processor;
use colored::*;
use rumdl_lib::config as rumdl_config;
use rumdl_lib::exit_codes::exit;
use rumdl_lib::rule::{LintWarning, Rule, Severity};
use rumdl_lib::workspace_index::{FileIndex, WorkspaceIndex, link_target_candidates, normalize_relative_path};
use std::collections::HashSet;
use std::io::{self, Read};
use std::path::{Path, PathBuf};

/// Cross-file findings for a document read from stdin.
///
/// A run over files indexes the whole workspace before resolving cross-file
/// references; a piped document has no workspace, so the files its links name are
/// read from disk here. That is the same disk MD057 already resolves link targets
/// against on this path, and it reads only the targets this document actually
/// references. Nothing from a target's content is reported: a finding names the
/// fragment and the destination as the piped document wrote them.
///
/// Returns nothing without `--stdin-filename`, which is what gives a relative
/// destination a directory to resolve against. MD057 is already silent there for
/// the same reason.
fn cross_file_warnings(
    file_path: &Path,
    file_index: &FileIndex,
    rules: &[Box<dyn Rule>],
    config: &rumdl_config::Config,
    args: &crate::CheckArgs,
    workspace: &StdinWorkspace<'_>,
) -> CrossFileResult {
    let mut workspace_index = WorkspaceIndex::new();
    let mut attempted: HashSet<PathBuf> = HashSet::new();
    // Resolved on the first target that exists, so a document naming none pays
    // nothing for it.
    let mut scanned: Option<HashSet<PathBuf>> = None;
    // The targets to read, in the order the document names them. A file is
    // resolved here and read below, because which config governs it is a
    // question about the whole set.
    let mut targets: Vec<String> = Vec::new();
    let mut resolved_targets: HashSet<PathBuf> = HashSet::new();
    // Spelled the way the candidates are, so a self-reference is recognizable
    // whichever way `--stdin-filename` was written.
    let self_path = normalize_relative_path(file_path);

    for link in &file_index.cross_file_links {
        // A destination with no fragment names a file, which MD057 checks; there
        // is nothing to resolve against the target's headings.
        if link.fragment.is_empty() {
            continue;
        }

        for candidate in link_target_candidates(file_path, &link.target_path) {
            // Two links naming the same target resolve to it once. Testing the
            // resolved set rather than `attempted` is what stops the second link
            // from walking past an already-resolved candidate onto another
            // extension.
            if resolved_targets.contains(&candidate) {
                break;
            }
            // A document that links to itself is answered by the text being
            // linted, not by whatever is saved under that name. The two differ
            // whenever an editor pipes an unsaved buffer, which is the case
            // `--stdin` exists for. This is also why the piped document answers
            // for itself whatever it is named: it is the file this run was given,
            // exactly as `rumdl check notes.txt` lints the file it was handed.
            if candidate == self_path {
                resolved_targets.insert(candidate.clone());
                workspace_index.insert_file(candidate, file_index.clone());
                break;
            }
            if !attempted.insert(candidate.clone()) {
                continue;
            }
            // A destination that names nothing on disk resolves to no file, so
            // there is no question of whether a scan would reach it. Answering
            // that first is also what keeps a document whose links all dangle
            // from paying for the scan below.
            let Some(resolved) = rumdl_lib::discovery::canonicalize_for_matching(&candidate) else {
                continue;
            };
            // Every other file a run knows about, it found by scanning, so this
            // asks the scanner. Extension, gitignore, `.markdownlintignore`, and
            // the configured include and exclude patterns all decide whether a
            // file is in the workspace, and a target this run reads but a scan
            // would not index is a finding `rumdl check` never reports.
            let scanned = scanned.get_or_insert_with(|| scanned_files(args, config, workspace.roots.project_root));
            if !scanned.contains(&resolved) {
                continue;
            }
            targets.push(candidate.to_string_lossy().into_owned());
            resolved_targets.insert(candidate);
            break;
        }
    }

    // A scan indexes each file under the config that governs it, so a target in a
    // directory with its own rumdl config is read under that one. Settings that
    // decide what a heading's anchor is live there, so indexing every target
    // under the piped document's config would answer a different question than
    // `rumdl check` does and disagree with it.
    let mut config_warning = false;
    if !targets.is_empty() {
        let resolved = crate::resolution::resolve_config_groups(
            &targets,
            &workspace.root,
            args,
            &workspace.roots,
            workspace.inline_overrides,
            &None,
            workspace.bypass_discovery,
        );
        config_warning = resolved.config_warning;
        for group in &resolved.groups {
            for target in &group.files {
                let target = PathBuf::from(target);
                // A destination that is not readable text (an unreadable file, or
                // one that is not UTF-8) simply contributes nothing, exactly as a
                // workspace scan that failed to index it would.
                let Ok(target_content) = std::fs::read_to_string(&target) else {
                    continue;
                };
                let flavor = group.config.get_flavor_for_file(&target);
                let target_index =
                    rumdl_lib::build_file_index_only(&target_content, &group.rules, flavor, Some(target.clone()));
                workspace_index.insert_file(target, target_index);
            }
        }
    }

    if workspace_index.file_count() == 0 {
        return CrossFileResult {
            warnings: Vec::new(),
            config_warning,
        };
    }

    CrossFileResult {
        warnings: rumdl_lib::run_cross_file_checks(file_path, file_index, rules, &workspace_index, Some(config))
            .unwrap_or_default(),
        config_warning,
    }
}

/// What resolving a piped document's cross-file references turned up.
struct CrossFileResult {
    warnings: Vec<LintWarning>,
    /// Set when a config governing one of the targets failed to load, so the
    /// anchors it was indexed against are not the ones its author configured.
    /// Counted by `--deny-config-warnings` like every other config warning.
    config_warning: bool,
}

/// The project the piped document belongs to, as far as resolving its cross-file
/// references needs to know it: which files a scan would reach, and which config
/// governs each of them.
pub struct StdinWorkspace<'a> {
    pub root: crate::resolution::RootConfig<'a>,
    pub roots: crate::resolution::ResolutionRoots<'a>,
    pub inline_overrides: &'a [toml::Table],
    /// `--config` and `--isolated` pin every file to the one config, exactly as
    /// they do for a run over paths.
    pub bypass_discovery: bool,
}

/// Every file a directory scan of this run's project would index.
///
/// This is the scan itself, not a second opinion about what it would do. Which
/// files a run knows about is decided by the ignore files, the configured
/// include and exclude patterns and the walk's own extension filter, all of
/// which interact, so the answer is taken from the function that produces it for
/// a run over paths. No path is passed, which is the same discovery mode a bare
/// `rumdl check` walks with, and the piped document is the same project's.
///
/// A scan that cannot run answers with nothing, so a target is left unread
/// rather than read on a guess.
fn scanned_files(
    args: &crate::CheckArgs,
    config: &rumdl_config::Config,
    project_root: Option<&Path>,
) -> HashSet<PathBuf> {
    let Ok(discovered) = crate::file_processor::find_markdown_files(&[], args, config, project_root) else {
        return HashSet::new();
    };
    discovered
        .files
        .iter()
        .filter_map(|file| rumdl_lib::discovery::canonicalize_for_matching(Path::new(file)))
        .collect()
}

/// Process markdown content from stdin.
///
/// `external_config_warning` reports whether a config-file, CLI-flag, or
/// discovery config warning was already seen (the classes decided in
/// `run_check`); combined with inline-comment detection here it drives the
/// `--deny-config-warnings` exit, which this function owns for the stdin path.
pub fn process_stdin(
    rules: &[Box<dyn Rule>],
    args: &crate::CheckArgs,
    config: &rumdl_config::Config,
    external_config_warning: bool,
    workspace: &StdinWorkspace<'_>,
) {
    use rumdl_lib::output::{OutputFormat, OutputWriter};

    let quiet = args.quiet;
    let silent = args.silent;

    // Diagnostics are what `check` was asked to produce, so they go to stdout
    // unless --stderr moves them, exactly as they do for a run over file
    // arguments. Fix and format modes put the rewritten document on stdout
    // instead and write their diagnostics through a separate stderr writer.
    let output_writer = OutputWriter::new(args.stderr, silent);

    let output_format = match crate::cli_utils::resolve_output_format(args, config) {
        Ok(fmt) => fmt,
        Err(e) => {
            eprintln!("{}: {}", "Error".red().bold(), e);
            exit::tool_error();
        }
    };

    // Read all content from stdin
    let mut content = String::new();
    if let Err(e) = io::stdin().read_to_string(&mut content) {
        if !args.silent {
            eprintln!("Error reading from stdin: {e}");
        }
        exit::violations_found();
    }

    // Detect original line ending before any processing (I/O boundary)
    let original_line_ending = rumdl_lib::utils::detect_line_ending_enum(&content);

    // Normalize to LF for all internal processing
    content = rumdl_lib::utils::normalize_line_ending(&content, rumdl_lib::utils::LineEnding::Lf).into_owned();

    // Use per-file flavor if stdin_filename is provided
    let flavor = args
        .stdin_filename
        .as_ref()
        .map(|f| config.get_flavor_for_file(std::path::Path::new(f)))
        .unwrap_or_else(|| config.markdown_flavor());

    // Detect unknown rule names in inline disable comments. Computed even under
    // --silent (which only suppresses the printed notices) so the flag can still
    // fail the run.
    let inline_config_warning = {
        let mut inline_warnings = rumdl_lib::inline_config::validate_inline_config_rules(&content, flavor);
        let active_rules: std::collections::HashSet<String> = rules.iter().map(|r| r.name().to_string()).collect();
        // per-file-ignores is keyed on the stdin filename, the same key the lint
        // pass below uses, so the two agree about what runs over this document.
        let ignored_for_file = args
            .stdin_filename
            .as_deref()
            .map(|name| config.get_ignored_rules_for_file(std::path::Path::new(name)))
            .unwrap_or_default();
        inline_warnings.extend(rumdl_lib::inline_config::validate_inline_enables_against_active_rules(
            &content,
            flavor,
            &active_rules,
            &ignored_for_file,
        ));
        let had_any = !inline_warnings.is_empty();
        if !silent {
            let display_name = args.stdin_filename.as_deref().unwrap_or("<stdin>");
            for warn in inline_warnings {
                warn.print_warning(display_name);
            }
        }
        had_any
    };

    // A configuration problem is a tooling error (exit 2) that outranks Markdown
    // violations (exit 1). Computed once, checked at every exit path below so
    // fix/format mode cannot bypass it.
    let mut deny_config = args.deny_config_warnings && (external_config_warning || inline_config_warning);

    // Determine the filename to use for display and context
    let display_filename = args.stdin_filename.as_deref().unwrap_or("<stdin>");

    // Convert stdin-filename to PathBuf for LintContext
    let source_file = args.stdin_filename.as_ref().map(std::path::PathBuf::from);

    // Apply per-file-ignores keyed on the stdin filename, so piping a file's
    // content (as pre-commit hooks and editors do) honors `[per-file-ignores]`
    // exactly like `rumdl check/fmt <file>`. Without this, linting would report
    // rules the file has excluded; the fix coordinator enforces the same
    // exclusion on the fix pass, so check and fix stay consistent.
    let filtered_rules: Vec<Box<dyn Rule>> = match args.stdin_filename.as_deref() {
        Some(name) => rumdl_lib::rules::filter_rules_for_file(rules, config, std::path::Path::new(name)),
        None => rules.to_vec(),
    };
    let effective_rules: &[Box<dyn Rule>] = &filtered_rules;

    // The rules this document configures itself, which is what decides whether its
    // warnings carry a fix the CLI will apply.
    let document_rules = file_processor::rules_reconfigured_by_document(rules, config, &content);

    // Lint through the same engine as the file path, so inline config
    // overrides, kramdown suppression, inline-disable ranges, and severity
    // overrides behave identically to `rumdl check <file>`.
    let run = rumdl_lib::document_run::DocumentRun::new(&content, effective_rules, config).verbose(args.verbose);
    let run = match source_file.as_deref() {
        Some(path) => run.file_path(path),
        None => run,
    };
    let (lint_result, file_index) = run.analyze_raw();
    let mut all_warnings = match lint_result {
        Ok(warnings) => warnings,
        Err(e) => {
            if !silent {
                eprintln!("{}: {}", "Error".red().bold(), e);
            }
            exit::tool_error();
        }
    };

    // Resolve this document's cross-file references against the files they name,
    // so a piped document reports what `rumdl check <file>` reports.
    if let Some(path) = source_file.as_deref() {
        let cross = cross_file_warnings(path, &file_index, effective_rules, config, args, workspace);
        // A target read under a config that failed to load is checked against
        // anchors its author did not configure, which is the same problem
        // `--deny-config-warnings` fails a run over files for.
        deny_config = deny_config || (args.deny_config_warnings && cross.config_warning);
        all_warnings.extend(cross.warnings);
    }
    let deny_config = deny_config;

    // Sort warnings by line/column
    all_warnings.sort_by(|a, b| {
        if a.line == b.line {
            a.column.cmp(&b.column)
        } else {
            a.line.cmp(&b.line)
        }
    });

    let has_issues = !all_warnings.is_empty();
    let has_warnings = all_warnings
        .iter()
        .any(|w| matches!(w.severity, Severity::Warning | Severity::Error));
    let has_errors = all_warnings.iter().any(|w| w.severity == Severity::Error);

    // Apply fixes if requested
    if args.fix_mode != crate::FixMode::Check {
        if has_issues {
            let mut fixed_content = content.clone();
            let file_path = args.stdin_filename.as_ref().map(std::path::Path::new);
            let _warnings_fixed = file_processor::apply_fixes_coordinated(
                effective_rules,
                &all_warnings,
                &mut fixed_content,
                quiet,
                silent,
                config,
                file_path,
            );

            // Denormalize back to original line ending before output (I/O boundary)
            let output_content =
                rumdl_lib::utils::normalize_line_ending(&fixed_content, original_line_ending).into_owned();

            // Output the fixed content to stdout
            print!("{output_content}");

            // Re-check the fixed content through the same engine to see if
            // any issues remain. Use same per-file flavor as initial lint.
            // The fixed content is already on stdout; an engine error here
            // must not be reported as "0 remaining", so signal a tool error.
            let recheck = rumdl_lib::document_run::DocumentRun::new(&fixed_content, effective_rules, config)
                .verbose(args.verbose);
            let recheck = match source_file.as_deref() {
                Some(path) => recheck.file_path(path),
                None => recheck,
            };
            let (recheck_result, fixed_file_index) = recheck.analyze_raw();
            let mut remaining_warnings = match recheck_result {
                Ok(warnings) => warnings,
                Err(e) => {
                    if !silent {
                        eprintln!("{}: failed to re-check fixed content: {}", "Error".red().bold(), e);
                    }
                    exit::tool_error();
                }
            };

            // Cross-file findings carry no fix, so they survive the fix pass. Leaving
            // them out of the re-check would count every one of them as fixed.
            if let Some(path) = source_file.as_deref() {
                remaining_warnings.extend(
                    cross_file_warnings(path, &fixed_file_index, effective_rules, config, args, workspace).warnings,
                );
            }
            let remaining_warnings = remaining_warnings;
            let actual_warnings_fixed = file_processor::count_actually_fixed_warnings(
                rules,
                &document_rules,
                config,
                &all_warnings,
                &remaining_warnings,
            );

            // Diagnostics always go to stderr in fix mode (stdout has fixed content)
            let fix_writer = OutputWriter::new(true, silent);
            if !remaining_warnings.is_empty() {
                // Batch formats: remaining-only warnings
                let batch_file_warnings = vec![(display_filename.to_string(), remaining_warnings.clone())];
                let batch_all_files = vec![display_filename.to_string()];
                if let Some(output) = output_format.format_batch(&batch_file_warnings, &batch_all_files, 0) {
                    fix_writer.writeln(&output).unwrap_or_else(|e| {
                        eprintln!("Error writing output: {e}");
                    });
                } else {
                    match output_format {
                        // Human-readable text formats: all warnings with [fixed] labels
                        OutputFormat::Text | OutputFormat::Full => {
                            let mut output = String::new();
                            for warning in &all_warnings {
                                let rule_name = warning.rule_name.as_deref().unwrap_or("unknown");
                                let was_fixed =
                                    file_processor::is_rule_cli_fixable_in(rules, &document_rules, config, rule_name)
                                        && warning.fix.is_some()
                                        && !remaining_warnings.iter().any(|w| {
                                            w.line == warning.line
                                                && w.column == warning.column
                                                && w.rule_name == warning.rule_name
                                                && w.message == warning.message
                                        });

                                let fix_indicator = if was_fixed {
                                    " [fixed]".green().to_string()
                                } else {
                                    String::new()
                                };

                                use std::fmt::Write;
                                writeln!(
                                    output,
                                    "{}:{}:{}: {} {}{}",
                                    display_filename.blue().underline(),
                                    warning.line.to_string().cyan(),
                                    warning.column.to_string().cyan(),
                                    format!("[{rule_name:5}]").yellow(),
                                    warning.message,
                                    fix_indicator
                                )
                                .ok();
                            }

                            if output.ends_with('\n') {
                                output.pop();
                            }
                            fix_writer.writeln(&output).unwrap_or_else(|e| {
                                eprintln!("Error writing output: {e}");
                            });
                        }
                        // Other streaming formats: use their formatter with remaining-only
                        _ => {
                            let formatter = output_format.create_formatter();
                            let formatted = formatter.format_warnings_with_content(
                                &remaining_warnings,
                                display_filename,
                                &fixed_content,
                            );
                            fix_writer.writeln(&formatted).unwrap_or_else(|e| {
                                eprintln!("Error writing output: {e}");
                            });
                        }
                    }
                }
                // Stdout holds the rewritten document here, so this stream is
                // where a machine-readable format is read from, and prose ends
                // it the same way it would end stdout in check mode.
                if !quiet && !output_format.is_machine_readable() {
                    fix_writer
                        .writeln(&format!(
                            "\n{} issue(s) fixed, {} issue(s) remaining",
                            actual_warnings_fixed,
                            remaining_warnings.len()
                        ))
                        .ok();
                }
            }

            // Config problem outranks the fix-mode --fail-on exit below (and the
            // Format-mode fall-through), for `check --fix --stdin` and
            // `fmt --stdin` alike.
            if deny_config {
                exit::tool_error();
            }

            if args.fix_mode != crate::FixMode::Format {
                let remaining_has_warnings = remaining_warnings
                    .iter()
                    .any(|w| matches!(w.severity, Severity::Warning | Severity::Error));
                let remaining_has_errors = remaining_warnings.iter().any(|w| w.severity == Severity::Error);
                let should_fail = match args.fail_on_mode {
                    crate::FailOn::Never => false,
                    crate::FailOn::Error => remaining_has_errors,
                    crate::FailOn::Warning => remaining_has_warnings,
                    crate::FailOn::Any => !remaining_warnings.is_empty(),
                };
                if should_fail {
                    exit::violations_found();
                }
            }
        } else {
            print!("{content}");
        }

        // Covers the no-issues sub-branch (which skips the gate above).
        if deny_config {
            exit::tool_error();
        }

        return;
    }

    // Normal check mode (no fix) - output diagnostics.
    // Batch formats emit one document with all warnings; streaming formats
    // emit per-warning lines plus a human-readable summary.
    let batch_file_warnings = vec![(display_filename.to_string(), all_warnings)];
    let batch_all_files = vec![display_filename.to_string()];
    if let Some(output) = output_format.format_batch(&batch_file_warnings, &batch_all_files, 0) {
        output_writer.writeln(&output).unwrap_or_else(|e| {
            eprintln!("Error writing output: {e}");
        });
    } else {
        let all_warnings = &batch_file_warnings[0].1;
        // Use formatter for line-by-line output
        let formatter = output_format.create_formatter();
        if !all_warnings.is_empty() {
            let formatted = formatter.format_warnings_with_content(all_warnings, display_filename, &content);
            output_writer.writeln(&formatted).unwrap_or_else(|e| {
                eprintln!("Error writing output: {e}");
            });
        }

        // The summary is a sentence for a person, so it is emitted only for the
        // formats a person reads. A streaming machine-readable format shares
        // stdout with the diagnostics it just wrote, and appending prose there
        // makes the document unparseable, exactly as it would for a run over
        // file arguments.
        if !quiet && !output_format.is_machine_readable() {
            if has_issues {
                output_writer
                    .writeln(&format!(
                        "\nFound {} issue(s) in {}",
                        all_warnings.len(),
                        display_filename
                    ))
                    .ok();
            } else {
                output_writer
                    .writeln(&format!("No issues found in {display_filename}"))
                    .ok();
            }
        }
    }

    // A config problem outranks the check-mode --fail-on exit.
    if deny_config {
        exit::tool_error();
    }

    // Exit with error code based on --fail-on setting
    let should_fail = match args.fail_on_mode {
        crate::FailOn::Never => false,
        crate::FailOn::Error => has_errors,
        crate::FailOn::Warning => has_warnings,
        crate::FailOn::Any => has_issues,
    };
    if should_fail {
        exit::violations_found();
    }
}
