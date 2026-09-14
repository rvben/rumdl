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
                let target_index = rumdl_lib::build_file_index_only(
                    &target_content,
                    &group.rule_sets.document,
                    flavor,
                    Some(target.clone()),
                );
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

/// Handle a piped document whose `--stdin-filename` the exclude patterns remove.
///
/// Nothing is linted, and the run reports the empty result `rumdl check <file>`
/// reports for an excluded file. The document is read either way, so the
/// process writing it is never cut off mid-write. Fix and format modes hand it
/// back byte for byte, since an editor or hook replaces its buffer with whatever
/// arrives on stdout, and an empty stdout would erase the file. A diff
/// (`--diff`, `fmt --check`) previews a rewrite instead of performing one, and
/// the diff of a document nothing formats is empty, so it prints nothing.
pub fn process_excluded_stdin(
    args: &crate::CheckArgs,
    output_format: rumdl_lib::output::OutputFormat,
    name: &str,
    pattern: &str,
) -> crate::check_runner::CheckRunOutcome {
    use std::io::Write;

    let mut content = Vec::new();
    if let Err(e) = io::stdin().read_to_end(&mut content) {
        if !args.silent {
            eprintln!("Error reading from stdin: {e}");
        }
        return crate::check_runner::CheckRunOutcome::tool_error();
    }

    file_processor::report_named_file_excluded(args, name, pattern);

    let passes_document_through = args.fix_mode != crate::FixMode::Check && !args.diff;
    if passes_document_through {
        let mut stdout = io::stdout().lock();
        if let Err(e) = stdout.write_all(&content).and_then(|()| stdout.flush()) {
            if !args.silent {
                eprintln!("Error writing output: {e}");
            }
            return crate::check_runner::CheckRunOutcome::tool_error();
        }
    }

    crate::check_runner::report_empty_run(
        args,
        output_format,
        &file_processor::EmptyDiscovery::all_named_files_excluded(1),
        passes_document_through,
    )
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
    // arguments, and a preview's diff goes with them. Fix and format modes put
    // the rewritten document on stdout instead and write their diagnostics
    // through a separate stderr writer.
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

    // Preserve the original bytes, including mixed endings, when refusing to format.
    if let Some(conflict) = rumdl_lib::merge_conflict::detect(&content) {
        let display_name = args.stdin_filename.as_deref().unwrap_or("<stdin>");
        let warnings = vec![conflict];
        let formatted = output_format
            .format_batch(
                &[(display_name.to_string(), warnings.clone())],
                &[display_name.to_string()],
                0,
            )
            .unwrap_or_else(|| {
                output_format
                    .create_formatter()
                    .format_warnings_with_content(&warnings, display_name, &content)
            });
        // Fix and format modes hand the document back untouched, since stdout is
        // the document there. A preview writes no document.
        let formatting = args.fix_mode != crate::FixMode::Check;
        if formatting && !args.diff {
            print!("{content}");
        }
        let writer = OutputWriter::new(formatting || args.stderr, silent);
        let _ = writer.writeln(&formatted);
        if args.deny_config_warnings && external_config_warning {
            exit::tool_error();
        }
        if args.fix_mode != crate::FixMode::Format && !matches!(args.fail_on_mode, crate::FailOn::Never) {
            exit::violations_found();
        }
        return;
    }

    // Detect original line ending and retain the byte mapping before internal
    // LF normalization so JSON fixes can address the caller's input.
    let line_ending_map = rumdl_lib::utils::NormalizedLineEndingMap::new(&content);
    let original_line_ending = rumdl_lib::utils::detect_line_ending_enum(&content);

    // Normalize to LF for all internal processing
    let original_content = content;
    let content = rumdl_lib::utils::normalize_line_ending(&original_content, rumdl_lib::utils::LineEnding::Lf);

    // Per-file settings (flavor, per-file-ignores) are keyed on the file the
    // piped text is, wherever the run was started from. A relative name is taken
    // from the working directory, as a path argument is, and the file need not
    // exist: an editor names an unsaved buffer the way it names a saved one.
    let config_path = args
        .stdin_filename
        .as_deref()
        .map(|name| rumdl_lib::discovery::resolve_for_matching(Path::new(name)));

    let flavor = config_path
        .as_deref()
        .map(|path| config.get_flavor_for_file(path))
        .unwrap_or_else(|| config.markdown_flavor());

    // `--stdin-filename lib.rs` says the piped text is that file, and this path
    // answers for it the way `rumdl check lib.rs` does: markdown inside `///`
    // and `//!`, and nothing else. Reading the source as markdown reports on the
    // Rust code itself, and `fmt` then rewrites it (`#[derive(Debug)]` is an
    // MD018 heading).
    let rust_source = args
        .stdin_filename
        .as_deref()
        .is_some_and(|name| rumdl_lib::doc_comment_lint::is_rust_source(std::path::Path::new(name)));

    // Detect unknown rule names in inline disable comments. Computed even under
    // --silent (which only suppresses the printed notices) so the flag can still
    // fail the run.
    //
    // A Rust file is exempt, as it is on the file path: an inline disable comment
    // there would have to be markdown inside a doc comment, and the block that
    // reads doc comments does not process them.
    let inline_config_warning = if rust_source {
        false
    } else {
        let mut inline_warnings = rumdl_lib::inline_config::validate_inline_config_rules(&content, flavor);
        let active_rules: std::collections::HashSet<String> = rules.iter().map(|r| r.name().to_string()).collect();
        // per-file-ignores is keyed on the same path the lint pass below uses, so
        // the two agree about what runs over this document.
        let ignored_for_file = config_path
            .as_deref()
            .map(|path| config.get_ignored_rules_for_file(path))
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
    let filtered_rules: Vec<Box<dyn Rule>> = match config_path.as_deref() {
        Some(path) => rumdl_lib::rules::filter_rules_for_file(rules, config, path),
        None => rules.to_vec(),
    };
    let effective_rules: &[Box<dyn Rule>] = &filtered_rules;

    // Lint through the same engine as the file path, so inline config
    // overrides, kramdown suppression, inline-disable ranges, and severity
    // overrides behave identically to `rumdl check <file>`. The piped document
    // and every fixed version of it are read the same way.
    let analyze = |text: &str| {
        if rust_source {
            // No index: a Rust file contributes no markdown links or headings to
            // the workspace, which is what `rumdl check lib.rs` indexes for it too.
            (
                Ok(rumdl_lib::doc_comment_lint::check_doc_comment_blocks(
                    text,
                    effective_rules,
                    config,
                )),
                FileIndex::new(),
            )
        } else {
            rumdl_lib::document_run::DocumentRun::new(text, effective_rules, config)
                .verbose(args.verbose)
                .config_path(config_path.as_deref())
                .source_file(source_file.as_deref())
                .analyze_raw()
        }
    };

    // The rewrite `fmt -` makes, shared by the fix modes and the preview of it.
    let fix_document = |text: &str, quiet: bool, silent: bool| {
        let mut fixed = text.to_string();
        file_processor::apply_document_fixes(
            effective_rules,
            &mut fixed,
            quiet,
            silent,
            config,
            config_path.as_deref(),
        );
        // What a Rust file gets instead: the document fixer above declines to
        // run over its source, so this is the whole fix pass for one, and it
        // rewrites exactly the markdown the lint pass reported on.
        if rust_source {
            file_processor::format_doc_comment_blocks(&mut fixed, effective_rules, config);
        }
        fixed
    };

    // The findings left once `fixed` replaces the document. Cross-file findings
    // carry no fix, so they survive the fix pass, and leaving them out would count
    // every one of them as fixed. An engine error here must not read as "0
    // remaining", so it is a tool error.
    let recheck = |fixed: &str| {
        let (result, fixed_file_index) = analyze(fixed);
        let mut remaining = match result {
            Ok(warnings) => warnings,
            Err(e) => {
                if !silent {
                    eprintln!("{}: failed to re-check fixed content: {}", "Error".red().bold(), e);
                }
                exit::tool_error();
            }
        };
        if let Some(path) = source_file.as_deref() {
            remaining.extend(
                cross_file_warnings(path, &fixed_file_index, effective_rules, config, args, workspace).warnings,
            );
        }
        remaining
    };

    let (lint_result, file_index) = analyze(&content);
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

    // A preview (`check --diff`, `fmt --diff`, `fmt --check`) writes no document,
    // so stdout carries what a run over the file prints: the findings the diff
    // leaves unfixed, the diff from the piped bytes to the bytes `fmt -` writes,
    // and a summary.
    if args.diff {
        let formats = args.fix_mode == crate::FixMode::Format;
        let fixed_content = if has_issues {
            fix_document(&content, true, true)
        } else {
            content.to_string()
        };
        let changed = fixed_content != *content;

        // A format with no room for a diff, one document or one JSON value per
        // line, gets every finding and nothing else.
        let findings_only = if output_format.carries_diff() {
            None
        } else {
            let mut warnings = all_warnings.clone();
            if matches!(output_format, OutputFormat::Json) {
                rumdl_lib::output::formatters::json::remap_fix_ranges_to_original(&mut warnings, &line_ending_map);
            }
            let file_warnings = [(display_filename.to_string(), warnings)];
            let batch = output_format.format_batch(&file_warnings, &[display_filename.to_string()], 0);
            Some(batch.unwrap_or_else(|| {
                let warnings = &file_warnings[0].1;
                if warnings.is_empty() {
                    String::new()
                } else {
                    output_format
                        .create_formatter()
                        .format_warnings_with_content(warnings, display_filename, &content)
                }
            }))
        };

        if let Some(output) = findings_only {
            if !output.is_empty() {
                output_writer.writeln(&output).unwrap_or_else(|e| {
                    eprintln!("Error writing output: {e}");
                });
            }
        } else {
            // Which findings the diff resolves and how many it leaves, read from
            // the document it produces.
            let reconcile = || {
                let remaining = if changed {
                    recheck(&fixed_content)
                } else {
                    all_warnings.clone()
                };
                (
                    file_processor::reconcile_fixed_warnings(&all_warnings, &remaining),
                    remaining.len(),
                )
            };

            // `fmt` reports findings only through its summary.
            if !formats && !silent {
                let unfixed = reconcile().0.unfixed(&all_warnings);
                if !unfixed.is_empty() {
                    let formatted = output_format.create_formatter().format_warnings_with_content(
                        &unfixed,
                        display_filename,
                        &content,
                    );
                    output_writer.writeln(&formatted).unwrap_or_else(|e| {
                        eprintln!("Error writing output: {e}");
                    });
                }
            }
            if changed {
                let fixed = rumdl_lib::utils::normalize_line_ending(&fixed_content, original_line_ending);
                let diff = crate::formatter::generate_diff(&original_content, &fixed, display_filename);
                output_writer.write(&diff).unwrap_or_else(|e| {
                    eprintln!("Error writing diff output: {e}");
                });
            }
            if !quiet && !output_format.is_machine_readable() {
                let summary = if !has_issues {
                    format!("No issues found in {display_filename}")
                } else if formats {
                    let (reconciliation, remaining) = reconcile();
                    format!(
                        "\n{} would be fixed, {} remaining",
                        crate::formatter::issues(reconciliation.fixed_count()),
                        crate::formatter::issues(remaining)
                    )
                } else {
                    format!(
                        "\nFound {} in {}",
                        crate::formatter::issues(all_warnings.len()),
                        display_filename
                    )
                };
                output_writer.writeln(&summary).ok();
            }
        }

        if deny_config {
            exit::tool_error();
        }
        let fails = if formats {
            args.check && changed
        } else {
            fails_on(args, &all_warnings)
        };
        if fails {
            exit::violations_found();
        }
        return;
    }

    // Apply fixes if requested
    if args.fix_mode != crate::FixMode::Check {
        if has_issues {
            let fixed_content = fix_document(&content, quiet, silent);

            // Denormalize back to original line ending before output (I/O boundary)
            let output_content =
                rumdl_lib::utils::normalize_line_ending(&fixed_content, original_line_ending).into_owned();

            // Output the fixed content to stdout
            print!("{output_content}");

            let remaining_warnings = recheck(&fixed_content);
            let reconciliation = file_processor::reconcile_fixed_warnings(&all_warnings, &remaining_warnings);

            // Diagnostics always go to stderr in fix mode (stdout has fixed content)
            let fix_writer = OutputWriter::new(true, silent);
            // Batch formats always emit a complete remaining-only document. An
            // empty report is still meaningful machine-readable output (`[]`,
            // an empty SARIF run, or a passing JUnit testcase).
            let batch_output = if output_format.is_batch() {
                let mut output_warnings = remaining_warnings.clone();
                if matches!(output_format, rumdl_lib::output::OutputFormat::Json) {
                    let output_line_endings = rumdl_lib::utils::NormalizedLineEndingMap::new(&output_content);
                    rumdl_lib::output::formatters::json::remap_fix_ranges_to_original(
                        &mut output_warnings,
                        &output_line_endings,
                    );
                }
                let batch_file_warnings = vec![(display_filename.to_string(), output_warnings)];
                let batch_all_files = vec![display_filename.to_string()];
                output_format.format_batch(&batch_file_warnings, &batch_all_files, 0)
            } else {
                None
            };

            if let Some(output) = batch_output {
                fix_writer.writeln(&output).unwrap_or_else(|e| {
                    eprintln!("Error writing output: {e}");
                });
            } else {
                match output_format {
                    // Human-readable text formats: what was fixed, at the position it
                    // was fixed at, alongside what is left, at its position in the
                    // document now on stdout.
                    OutputFormat::Text | OutputFormat::Full => {
                        let mut entries: Vec<(&LintWarning, bool)> = all_warnings
                            .iter()
                            .zip(reconciliation.per_warning())
                            .filter(|&(_, &was_fixed)| was_fixed)
                            .map(|(warning, _)| (warning, true))
                            .chain(remaining_warnings.iter().map(|warning| (warning, false)))
                            .collect();
                        entries.sort_by_key(|(warning, _)| (warning.line, warning.column));

                        let mut output = String::new();
                        for (warning, was_fixed) in entries {
                            let rule_name = warning.rule_name.as_deref().unwrap_or("unknown");

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
                        if !output.is_empty() {
                            fix_writer.writeln(&output).unwrap_or_else(|e| {
                                eprintln!("Error writing output: {e}");
                            });
                        }
                    }
                    // Other streaming formats: use their formatter with remaining-only
                    _ => {
                        if !remaining_warnings.is_empty() {
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
            }
            // Stdout holds the rewritten document here, so this stream is
            // where a machine-readable format is read from, and prose ends
            // it the same way it would end stdout in check mode. A run that
            // fixed everything still reports what it fixed: the alternative
            // is a `fmt` that rewrites the document and says nothing.
            if !quiet && !output_format.is_machine_readable() {
                fix_writer
                    .writeln(&format!(
                        "\n{} fixed, {} remaining",
                        crate::formatter::issues(reconciliation.fixed_count()),
                        crate::formatter::issues(remaining_warnings.len())
                    ))
                    .ok();
            }

            // Config problem outranks the fix-mode --fail-on exit below (and the
            // Format-mode fall-through), for `check --fix --stdin` and
            // `fmt --stdin` alike.
            if deny_config {
                exit::tool_error();
            }

            if args.fix_mode != crate::FixMode::Format && fails_on(args, &remaining_warnings) {
                exit::violations_found();
            }
        } else {
            print!("{original_content}");
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
    if matches!(output_format, rumdl_lib::output::OutputFormat::Json) {
        rumdl_lib::output::formatters::json::remap_fix_ranges_to_original(&mut all_warnings, &line_ending_map);
    }
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
                        "\nFound {} in {}",
                        crate::formatter::issues(all_warnings.len()),
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

    if fails_on(args, &batch_file_warnings[0].1) {
        exit::violations_found();
    }
}

/// Whether `warnings` fail the run under `--fail-on`.
fn fails_on(args: &crate::CheckArgs, warnings: &[LintWarning]) -> bool {
    match args.fail_on_mode {
        crate::FailOn::Never => false,
        crate::FailOn::Error => warnings.iter().any(|w| w.severity == Severity::Error),
        crate::FailOn::Warning => warnings
            .iter()
            .any(|w| matches!(w.severity, Severity::Warning | Severity::Error)),
        crate::FailOn::Any => !warnings.is_empty(),
    }
}
