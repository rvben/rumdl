//! Core file processing, fix application, and fixability checks.

use crate::cache::{DependencyFingerprint, LintCache};
use crate::formatter;
use colored::*;
use rumdl_lib::config as rumdl_config;
use rumdl_lib::doc_comment_lint::is_rust_source;
use rumdl_lib::rule::{FixCapability, LintWarning, Rule};
use rumdl_lib::utils::code_block_utils::CodeBlockUtils;
use std::borrow::Cow;
use std::path::{Path, PathBuf};

use rumdl_lib::code_block_tools::executor::ExecutorError;
use rumdl_lib::code_block_tools::processor::ProcessorError;

use super::discovery::{AuxiliaryExecutionPlan, RuleSets, resolve_display_path, to_display_path};
use super::embedded::{
    check_embedded_markdown_blocks, format_embedded_markdown_blocks, has_fenced_code_blocks,
    should_lint_embedded_markdown,
};
use super::fix_reporting::reconcile_fixed_warnings;

fn warnings_for_output(
    mut warnings: Vec<LintWarning>,
    output_format: &rumdl_lib::output::OutputFormat,
    line_endings: &rumdl_lib::utils::NormalizedLineEndingMap,
) -> Vec<LintWarning> {
    if matches!(output_format, rumdl_lib::output::OutputFormat::Json) {
        rumdl_lib::output::formatters::json::remap_fix_ranges_to_original(&mut warnings, line_endings);
    }
    warnings
}

/// Result of processing a file through lint and optional fix passes.
pub struct FileProcessResult {
    pub has_issues: bool,
    pub issues_found: usize,
    /// Whether the fix pass rewrote the file (or, in diff mode, would have).
    pub content_changed: bool,
    /// How many of the file's warnings the fix pass resolved.
    pub summary_issues_fixed: usize,
    pub fixable_issues: usize,
    /// In fix mode, contains only remaining (unfixed) warnings.
    /// In check mode, contains all warnings.
    pub warnings: Vec<rumdl_lib::rule::LintWarning>,
    pub file_index: rumdl_lib::workspace_index::FileIndex,
    pub file_index_reused: bool,
    /// The file could not be read. A tool error (exit code 2), distinct from a
    /// lint violation (exit code 1).
    pub errored: bool,
    /// An inline disable comment referenced an unknown rule name.
    pub config_warning: bool,
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
    is_rule_cli_fixable_in(rules, &[], config, rule_name)
}

/// `is_rule_cli_fixable` for a document that configures some of its rules itself.
///
/// `document_rules` holds those rules as the document asked for them, and answers
/// for the names it covers; every other name is answered from `rules`.
pub fn is_rule_cli_fixable_in(
    rules: &[Box<dyn Rule>],
    document_rules: &[Box<dyn Rule>],
    config: &rumdl_config::Config,
    rule_name: &str,
) -> bool {
    // First check config-based fixability
    if !is_rule_actually_fixable(config, rule_name) {
        return false;
    }

    // Then check if the rule declares itself as Unfixable
    // Rules like MD033 have LSP-only fixes (for VS Code quick actions) but
    // their fix() method returns content unchanged, so CLI shouldn't count them
    document_rules
        .iter()
        .chain(rules)
        .find(|r| r.name().eq_ignore_ascii_case(rule_name))
        .is_none_or(|r| r.fix_capability() != FixCapability::Unfixable)
}

/// The rules a document reconfigures, built with the settings it asks for.
///
/// A rule's fix capability can depend on its settings, and an inline
/// `rumdl-configure-file` comment changes those settings for one file. The fixer
/// already runs the reconfigured rule, so whatever reports what a run fixed has to
/// read the capability from the same instance. Empty for the documents that carry
/// no inline configuration, which is nearly all of them.
pub fn rules_reconfigured_by_document(
    rules: &[Box<dyn Rule>],
    config: &rumdl_config::Config,
    content: &str,
) -> Vec<Box<dyn Rule>> {
    let inline_config = rumdl_lib::inline_config::InlineConfig::from_content(content);
    if inline_config.get_all_rule_configs().is_empty() {
        return Vec::new();
    }

    let merged = config.merge_with_inline_config(&inline_config);
    rules
        .iter()
        .filter(|rule| inline_config.get_rule_config(rule.name()).is_some())
        .filter_map(|rule| rumdl_lib::rules::create_rule_by_name(rule.name(), &merged))
        .collect()
}

#[allow(clippy::too_many_arguments)]
pub fn process_file_with_formatter(
    file_path: &str,
    rule_sets: &RuleSets,
    fix_mode: crate::FixMode,
    diff: bool,
    verbose: bool,
    quiet: bool,
    silent: bool,
    output_format: &rumdl_lib::output::OutputFormat,
    output_writer: &rumdl_lib::output::OutputWriter,
    config: &rumdl_config::Config,
    cache: Option<std::sync::Arc<LintCache>>,
    workspace_index: Option<std::sync::Arc<rumdl_lib::workspace_index::WorkspaceIndex>>,
    project_root: Option<&Path>,
    show_full_path: bool,
    cache_hashes: Option<&CacheHashes>,
) -> FileProcessResult {
    let formatter = output_format.create_formatter();

    // The same display path the batch formats show for this file: relative
    // unless --show-full-path is set, normalized either way.
    let display_path = resolve_display_path(file_path, show_full_path, project_root);

    // Call the original process_file_inner to get warnings, original line ending, and FileIndex
    let (
        all_warnings,
        mut content,
        total_warnings,
        fixable_warnings,
        original_line_ending,
        line_ending_map,
        file_index,
        file_index_reused,
        errored,
        inline_config_warning,
    ) = process_file_inner(
        file_path,
        rule_sets,
        verbose,
        quiet,
        silent,
        config,
        cache,
        workspace_index,
        cache_hashes,
    );

    // The file could not be read: report it as a tool error (exit code 2) and
    // do not treat the empty content as a clean, issue-free file.
    if errored {
        return FileProcessResult {
            has_issues: false,
            issues_found: 0,
            content_changed: false,
            summary_issues_fixed: 0,
            fixable_issues: 0,
            warnings: Vec::new(),
            file_index,
            file_index_reused,
            errored: true,
            config_warning: false,
        };
    }

    // The rules this document configures itself, which is what decides whether its
    // warnings carry a fix the CLI will apply.
    let document_rules = rules_reconfigured_by_document(&rule_sets.document, config, &content);

    // Compute filtered rules based on per-file-ignores. The fix coordinator, embedded
    // markdown formatting, and Rust doc-comment formatting all run against this set so
    // `fmt` never applies a per-file-ignored rule - matching what linting already does.
    // Passing the unfiltered `rules` here would let the coordinator re-check and re-fix
    // an ignored rule as a side effect of fixing a non-ignored one (issue #707).
    let ignored_rules_for_file = config.get_ignored_rules_for_file(Path::new(file_path));
    let filtered_rule_sets = rule_sets.for_file(&ignored_rules_for_file);

    // In check mode with no warnings, return early
    if total_warnings == 0 && fix_mode == crate::FixMode::Check && !diff {
        return FileProcessResult {
            has_issues: false,
            issues_found: 0,
            content_changed: false,
            summary_issues_fixed: 0,
            fixable_issues: 0,
            warnings: Vec::new(),
            file_index,
            file_index_reused,
            errored: false,
            config_warning: inline_config_warning,
        };
    }

    // In fix mode with no warnings to fix, check if there are embedded markdown blocks to format
    // or code block tools to run. If not, return early.
    if total_warnings == 0 && fix_mode != crate::FixMode::Check && !diff {
        // Check if there's any embedded markdown to format
        let has_embedded = rule_sets.auxiliary.format
            && !filtered_rule_sets.embedded_markdown.is_empty()
            && has_fenced_code_blocks(&content)
            && CodeBlockUtils::detect_markdown_code_blocks(&content)
                .iter()
                .any(|b| !content[b.content_start..b.content_end].trim().is_empty());

        // Check if code block tools are enabled
        let has_code_block_tools =
            rule_sets.auxiliary.format && config.code_block_tools.enabled && !is_rust_source(Path::new(file_path));

        if !has_embedded && !has_code_block_tools {
            return FileProcessResult {
                has_issues: false,
                issues_found: 0,
                content_changed: false,
                summary_issues_fixed: 0,
                fixable_issues: 0,
                warnings: Vec::new(),
                file_index,
                file_index_reused,
                errored: false,
                config_warning: inline_config_warning,
            };
        }
    }

    // Format and output warnings (show diagnostics unless silent)
    if !silent && fix_mode == crate::FixMode::Check {
        if diff {
            // In diff mode, only show warnings for unfixable issues
            let unfixable_warnings: Vec<_> = all_warnings.iter().filter(|w| w.fix.is_none()).cloned().collect();

            if !unfixable_warnings.is_empty() {
                let formatted = formatter.format_warnings_with_content(&unfixable_warnings, &display_path, &content);
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
                    if !is_rule_cli_fixable_in(&rule_sets.document, &document_rules, config, rule_name) {
                        LintWarning { fix: None, ..w.clone() }
                    } else {
                        w.clone()
                    }
                })
                .collect();
            let formatted = formatter.format_warnings_with_content(&display_warnings, &display_path, &content);
            if !formatted.is_empty() {
                output_writer.writeln(&formatted).unwrap_or_else(|e| {
                    eprintln!("Error writing output: {e}");
                });
            }
        }
    }

    // Handle diff mode or fix mode
    if diff {
        // In diff mode, apply fixes to a copy and show diff
        let original_content = content.clone();
        let document_changed = apply_document_fixes(
            &filtered_rule_sets.document,
            &mut content,
            true,
            true,
            config,
            Some(Path::new(file_path)),
        );
        // A diff is a preview and writes nothing, but an external formatter that
        // could not run is a fact about this run either way, so `--silent` is what
        // decides whether the user hears about it.
        let auxiliary = apply_auxiliary_fixes(
            &mut content,
            file_path,
            &display_path,
            &filtered_rule_sets,
            config,
            silent,
        );
        let blocks_formatted = auxiliary.blocks_formatted;

        let content_changed = document_changed || blocks_formatted > 0;

        if content_changed {
            let diff_output = formatter::generate_diff(&original_content, &content, &display_path);
            output_writer.writeln(&diff_output).unwrap_or_else(|e| {
                eprintln!("Error writing diff output: {e}");
            });
        }

        let summary_issues_fixed = if total_warnings > 0 {
            let remaining_warnings = remaining_after_fixes(
                &content,
                file_path,
                &filtered_rule_sets,
                config,
                &all_warnings,
                content_changed,
            );
            reconcile_fixed_warnings(&all_warnings, &remaining_warnings).fixed_count()
        } else {
            blocks_formatted
        };

        // Don't actually write the file in diff mode, but report how many would be fixed
        return FileProcessResult {
            has_issues: total_warnings > 0 || content_changed,
            issues_found: total_warnings,
            content_changed,
            summary_issues_fixed,
            fixable_issues: fixable_warnings,
            warnings: warnings_for_output(all_warnings, output_format, &line_ending_map),
            file_index,
            file_index_reused,
            errored: auxiliary.tool_failed,
            config_warning: inline_config_warning,
        };
    } else if fix_mode != crate::FixMode::Check {
        // Apply fixes using Fix Coordinator
        let document_changed = apply_document_fixes(
            &filtered_rule_sets.document,
            &mut content,
            quiet,
            silent,
            config,
            Some(Path::new(file_path)),
        );

        let auxiliary = apply_auxiliary_fixes(
            &mut content,
            file_path,
            &display_path,
            &filtered_rule_sets,
            config,
            silent,
        );
        let blocks_formatted = auxiliary.blocks_formatted;

        let content_changed = document_changed || blocks_formatted > 0;

        // Write fixed content back to file
        if content_changed {
            // Denormalize back to original line ending before writing
            let content_to_write = rumdl_lib::utils::normalize_line_ending(&content, original_line_ending).into_owned();

            // Write atomically (temp file + rename) so an interrupted or failed
            // write can never truncate the user's file: the original is only
            // ever replaced wholesale, never edited in place.
            if let Err(err) =
                rumdl_lib::utils::atomic_write::write_atomically(Path::new(file_path), content_to_write.as_bytes())
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
            return FileProcessResult {
                has_issues: false,
                issues_found: 0,
                content_changed,
                summary_issues_fixed: blocks_formatted,
                fixable_issues: 0,
                warnings: Vec::new(),
                file_index,
                file_index_reused,
                errored: auxiliary.tool_failed,
                config_warning: inline_config_warning,
            };
        }

        // Re-lint the fixed content to see which warnings remain.
        let remaining_warnings = remaining_after_fixes(
            &content,
            file_path,
            &filtered_rule_sets,
            config,
            &all_warnings,
            content_changed,
        );

        let reconciliation = reconcile_fixed_warnings(&all_warnings, &remaining_warnings);
        let summary_issues_fixed = reconciliation.fixed_count();

        // Show fix results in streaming output
        if !silent {
            use rumdl_lib::output::OutputFormat;
            match output_format {
                // Human-readable text formats: show what was fixed alongside what is
                // left. A fixed warning is reported where it was, since that is the
                // only place it ever existed; everything else is reported from the
                // re-lint, so its position belongs to the file now on disk.
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
                // Batch formats are handled by check_runner (silent=true suppresses this path)
                OutputFormat::Json | OutputFormat::GitLab | OutputFormat::Sarif | OutputFormat::Junit => {}
                // Other streaming formats: use their formatter with remaining-only warnings
                _ => {
                    if !remaining_warnings.is_empty() {
                        let formatted =
                            formatter.format_warnings_with_content(&remaining_warnings, &display_path, &content);
                        if !formatted.is_empty() {
                            output_writer.writeln(&formatted).unwrap_or_else(|e| {
                                eprintln!("Error writing output: {e}");
                            });
                        }
                    }
                }
            }
        }

        // Return remaining warnings for batch format collection
        // Exit 0 if all violations are fixed (Ruff convention)
        let fixed_line_ending_map = if content_changed {
            let output_content = rumdl_lib::utils::normalize_line_ending(&content, original_line_ending).into_owned();
            rumdl_lib::utils::NormalizedLineEndingMap::new(&output_content)
        } else {
            line_ending_map.clone()
        };

        return FileProcessResult {
            has_issues: !remaining_warnings.is_empty(),
            issues_found: total_warnings,
            content_changed,
            summary_issues_fixed,
            fixable_issues: fixable_warnings,
            warnings: warnings_for_output(remaining_warnings, output_format, &fixed_line_ending_map),
            file_index,
            file_index_reused,
            errored: auxiliary.tool_failed,
            config_warning: inline_config_warning,
        };
    }

    FileProcessResult {
        has_issues: true,
        issues_found: total_warnings,
        content_changed: false,
        summary_issues_fixed: 0,
        fixable_issues: fixable_warnings,
        warnings: warnings_for_output(all_warnings, output_format, &line_ending_map),
        file_index,
        file_index_reused,
        errored: false,
        config_warning: inline_config_warning,
    }
}

/// Lint the fixed content to see which warnings remain.
///
/// `rule_sets` are the ones the fix pass ran, already filtered by per-file ignores.
/// Going through the same entry point the pre-fix pass uses keeps the two sets of
/// warnings comparable: inline config, kramdown blocks, severity overrides and the
/// rules that report on a whole document are all handled identically.
fn relint_fixed_file_content(
    content: &str,
    file_path: &str,
    rule_sets: &RuleSets,
    config: &rumdl_config::Config,
) -> Vec<rumdl_lib::rule::LintWarning> {
    // A Rust file is linted through the markdown in its doc comments and nothing
    // else. Handing its source to the markdown linter reports on the Rust code
    // itself, and those findings are not in the file: an `.rs` file whose doc
    // comment was fixed gained an MD041 for its first line of code.
    if is_rust_source(Path::new(file_path)) {
        return rumdl_lib::doc_comment_lint::check_doc_comment_blocks(content, &rule_sets.document, config);
    }

    let flavor = config.get_flavor_for_file(Path::new(file_path));
    let mut warnings = rumdl_lib::lint(
        content,
        &rule_sets.document,
        false,
        flavor,
        Some(PathBuf::from(file_path)),
        Some(config),
    )
    .unwrap_or_default();
    let relint_plan = AuxiliaryExecutionPlan {
        lint: rule_sets.auxiliary.relint,
        format: false,
        relint: false,
    };
    warnings.extend(auxiliary_warnings(
        content,
        file_path,
        &rule_sets.embedded_markdown,
        relint_plan,
        config,
    ));
    warnings
}

/// The warnings a file still has once its fix run is done.
///
/// A run that changed no bytes fixed nothing, so the file still says exactly what
/// it was reported to say and the answer is already in hand. Asking again is a
/// second full lint of every file a run left alone, external code-block tools
/// included, to be told what it was told before the fix pass.
fn remaining_after_fixes(
    content: &str,
    file_path: &str,
    rule_sets: &RuleSets,
    config: &rumdl_config::Config,
    all_warnings: &[rumdl_lib::rule::LintWarning],
    content_changed: bool,
) -> Vec<rumdl_lib::rule::LintWarning> {
    if content_changed {
        relint_fixed_file_content(content, file_path, rule_sets, config)
    } else {
        all_warnings.to_vec()
    }
}

/// What `apply_auxiliary_fixes` managed to do.
#[derive(Default)]
struct AuxiliaryFixOutcome {
    /// Number of blocks that were rewritten.
    blocks_formatted: usize,
    /// A configured code-block tool could not run and the setting for that case
    /// is `fail`. The document was formatted only in part, which is a tool-level
    /// error rather than a lint finding: it must surface as exit code 2, the same
    /// way the lint path reports it as a violation. Messages collected under
    /// `on-error = "warn"` leave this false.
    tool_failed: bool,
}

impl From<usize> for AuxiliaryFixOutcome {
    fn from(blocks_formatted: usize) -> Self {
        Self {
            blocks_formatted,
            tool_failed: false,
        }
    }
}

/// Run the fixers that work beside the document's own fix pass.
///
/// The counterpart of `auxiliary_warnings`: one funnel per direction, so a source
/// cannot be linted without being fixed or fixed without being linted.
fn apply_auxiliary_fixes(
    content: &mut String,
    file_path: &str,
    display_path: &str,
    rule_sets: &RuleSets,
    config: &rumdl_config::Config,
    silent: bool,
) -> AuxiliaryFixOutcome {
    // Rust sources are treated solely as containers for Markdown doc comments.
    // This is regular rumdl document formatting rather than a code-block tool,
    // so `--no-code-block-tools` preserves it. Only mode supplies no document
    // rules and therefore changes nothing. Never hand the Rust code itself to
    // configured fenced-code tools.
    if is_rust_source(Path::new(file_path)) {
        return super::doc_comments::format_doc_comment_blocks(content, &rule_sets.document, config).into();
    }

    if !rule_sets.auxiliary.format {
        return AuxiliaryFixOutcome::default();
    }

    let mut blocks_formatted = 0;
    let mut tool_failed = false;

    // Format embedded markdown blocks (recursive formatting). This is opt-in
    // via code-block-tools (`[code-block-tools.languages.markdown] lint = ["rumdl"]`)
    // and gated identically to the check path, so `--fix` never rewrites the
    // contents of a markdown code block that `check` did not report on.
    // `embedded_markdown` respects per-file-ignores for the embedded content.
    if !rule_sets.embedded_markdown.is_empty() && should_lint_embedded_markdown(&config.code_block_tools) {
        blocks_formatted += format_embedded_markdown_blocks(content, &rule_sets.embedded_markdown, config);
    }

    // Format code blocks using external tools if enabled
    if config.code_block_tools.enabled {
        let processor = rumdl_lib::code_block_tools::CodeBlockToolProcessor::new(
            &config.code_block_tools,
            config.get_flavor_for_file(Path::new(file_path)),
        );
        match processor.format(content) {
            Ok(output) => {
                if output.content != *content {
                    *content = output.content;
                    blocks_formatted += 1;
                }
                // Report any errors that occurred during formatting
                if output.had_errors && !silent {
                    for msg in &output.error_messages {
                        eprintln!("Warning: {}", format_tool_warning(msg, display_path));
                    }
                }
                tool_failed |= output.failed;
            }
            // `format` only returns Err for the settings that ask to stop on the
            // first failure (`fail-fast`, `on-error = "fail"`), so the document
            // was left unformatted from that block onwards.
            Err(e) => {
                if !silent {
                    eprintln!("Warning: {}", format_tool_error(&e, display_path));
                }
                tool_failed = true;
            }
        }
    }

    AuxiliaryFixOutcome {
        blocks_formatted,
        tool_failed,
    }
}

/// The warnings a file has from the sources beside its own document lint.
///
/// Markdown embedded in a fenced code block, and a code block handed to an
/// external tool, each produce findings `rumdl_lib::lint` knows nothing about.
/// The check pass adds both, so the re-lint a fix run reconciles against has to
/// add them on the same terms: a source present on one side and missing on the
/// other is a warning that leaves the report without anyone having fixed it.
fn auxiliary_warnings(
    content: &str,
    file_path: &str,
    embedded_markdown_rules: &[Box<dyn Rule>],
    plan: AuxiliaryExecutionPlan,
    config: &rumdl_config::Config,
) -> Vec<rumdl_lib::rule::LintWarning> {
    if !plan.lint || is_rust_source(Path::new(file_path)) {
        return Vec::new();
    }

    let mut warnings = Vec::new();

    // An embedded block is part of this file, so its findings are this file's and
    // the caller's per-file-ignores decides which of them are reported.
    if !embedded_markdown_rules.is_empty() && should_lint_embedded_markdown(&config.code_block_tools) {
        warnings.extend(rumdl_lib::time_function!(
            "file: embedded markdown blocks",
            check_embedded_markdown_blocks(content, embedded_markdown_rules, config)
        ));
    }

    if config.code_block_tools.enabled {
        rumdl_lib::time_section!("file: code block tools", {
            let processor = rumdl_lib::code_block_tools::CodeBlockToolProcessor::new(
                &config.code_block_tools,
                config.get_flavor_for_file(Path::new(file_path)),
            );
            match processor.lint(content) {
                Ok(diagnostics) => warnings.extend(diagnostics.iter().map(|d| d.to_lint_warning())),
                Err(e) => {
                    // Convert processor error to a warning so it counts toward exit code
                    warnings.push(rumdl_lib::rule::LintWarning {
                        message: e.to_string(),
                        line: 1,
                        column: 1,
                        end_line: 1,
                        end_column: 1,
                        severity: rumdl_lib::rule::Severity::Error,
                        fix: None,
                        rule_name: Some(CODE_BLOCK_TOOLS_DIAGNOSTIC_NAME.to_string()),
                    });
                }
            }
        });
    }

    warnings
}

/// The name a code block tools processor error is reported under.
///
/// Not a rule name and not a tool id: it names the class of problem, so nothing
/// looks it up in the rule registry.
const CODE_BLOCK_TOOLS_DIAGNOSTIC_NAME: &str = "code-block-tools";

/// Result type for file processing that includes index data for cross-file analysis
pub struct ProcessFileResult {
    pub warnings: Vec<rumdl_lib::rule::LintWarning>,
    pub content: String,
    pub total_warnings: usize,
    pub fixable_warnings: usize,
    pub original_line_ending: rumdl_lib::utils::LineEnding,
    pub line_ending_map: rumdl_lib::utils::NormalizedLineEndingMap,
    pub file_index: rumdl_lib::workspace_index::FileIndex,
    pub file_index_reused: bool,
    /// The file could not be read (missing, unreadable, or not valid UTF-8).
    /// A tool-level error, not a lint finding: it must surface as exit code 2.
    pub errored: bool,
    /// An inline disable comment referenced an unknown rule name.
    pub inline_config_warning: bool,
}

pub struct CacheHashes {
    pub config_hash: String,
    pub rules_hash: String,
}

impl CacheHashes {
    pub fn new(config: &rumdl_config::Config, rule_sets: &RuleSets) -> Self {
        Self {
            config_hash: LintCache::hash_config(config),
            rules_hash: Self::hash_rule_sets(rule_sets),
        }
    }

    fn hash_rule_sets(rule_sets: &RuleSets) -> String {
        let material = format!(
            "code-block-tool-modes-v1\0{}\0{}\0{}\0{}",
            rule_sets.mode.as_str(),
            rule_sets.auxiliary.cache_key(),
            LintCache::hash_rules(&rule_sets.document),
            LintCache::hash_rules(&rule_sets.embedded_markdown),
        );
        blake3::hash(material.as_bytes()).to_hex().to_string()
    }
}

#[allow(clippy::too_many_arguments)]
pub fn process_file_inner(
    file_path: &str,
    rule_sets: &RuleSets,
    verbose: bool,
    quiet: bool,
    silent: bool,
    config: &rumdl_config::Config,
    cache: Option<std::sync::Arc<LintCache>>,
    workspace_index: Option<std::sync::Arc<rumdl_lib::workspace_index::WorkspaceIndex>>,
    cache_hashes: Option<&CacheHashes>,
) -> (
    Vec<rumdl_lib::rule::LintWarning>,
    String,
    usize,
    usize,
    rumdl_lib::utils::LineEnding,
    rumdl_lib::utils::NormalizedLineEndingMap,
    rumdl_lib::workspace_index::FileIndex,
    bool,
    bool,
    bool,
) {
    let result = process_file_with_index(
        file_path,
        rule_sets,
        verbose,
        quiet,
        silent,
        config,
        cache,
        workspace_index,
        cache_hashes,
    );
    (
        result.warnings,
        result.content,
        result.total_warnings,
        result.fixable_warnings,
        result.original_line_ending,
        result.line_ending_map,
        result.file_index,
        result.file_index_reused,
        result.errored,
        result.inline_config_warning,
    )
}

/// Process a file and return both warnings and FileIndex for cross-file aggregation
#[allow(clippy::too_many_arguments)]
pub fn process_file_with_index(
    file_path: &str,
    rule_sets: &RuleSets,
    verbose: bool,
    quiet: bool,
    silent: bool,
    config: &rumdl_config::Config,
    cache: Option<std::sync::Arc<LintCache>>,
    workspace_index: Option<std::sync::Arc<rumdl_lib::workspace_index::WorkspaceIndex>>,
    cache_hashes: Option<&CacheHashes>,
) -> ProcessFileResult {
    use std::time::Instant;

    let start_time = Instant::now();
    if verbose && !quiet {
        // Display a relative path for better UX, even if file_path is canonical
        // (absolute). to_display_path canonicalizes both the file and the base
        // before stripping, so it relativizes correctly on Windows where the
        // discovered path carries a `\\?\` verbatim prefix and a long name while
        // the cwd may be an 8.3 short name. It also normalizes separators to `/`.
        let display_path = to_display_path(file_path, None);
        println!("Processing file: {display_path}");
    }

    let empty_result = ProcessFileResult {
        warnings: Vec::new(),
        content: String::new(),
        total_warnings: 0,
        fixable_warnings: 0,
        original_line_ending: rumdl_lib::utils::LineEnding::Lf,
        line_ending_map: rumdl_lib::utils::NormalizedLineEndingMap::default(),
        file_index: rumdl_lib::workspace_index::FileIndex::new(),
        file_index_reused: false,
        errored: false,
        // Inline-comment detection has not run at this point (the errored and
        // empty-content early returns spread this template); those paths have no
        // inline warning.
        inline_config_warning: false,
    };

    // Read file content efficiently
    let mut content =
        match rumdl_lib::time_function!("file: read content", crate::read_file_efficiently(Path::new(file_path))) {
            Ok(content) => content,
            Err(e) => {
                if !silent {
                    eprintln!("Error reading file {file_path}: {e}");
                }
                // A read failure is a tool error, not a clean result: flag it so
                // the run exits with the tool-error code instead of reporting
                // the file as having no issues.
                return ProcessFileResult {
                    errored: true,
                    ..empty_result
                };
            }
        };

    // Detect original line ending and retain a mapping back to the original
    // byte boundaries before any processing.
    let line_ending_map = rumdl_lib::utils::NormalizedLineEndingMap::new(&content);
    let original_line_ending = rumdl_lib::time_function!(
        "file: detect line endings",
        rumdl_lib::utils::detect_line_ending_enum(&content)
    );

    // Normalize to LF for all internal processing
    content = rumdl_lib::time_function!(
        "file: normalize line endings",
        rumdl_lib::utils::normalize_line_ending(&content, rumdl_lib::utils::LineEnding::Lf).into_owned()
    );

    // Route Rust files to doc comment linting instead of regular markdown linting
    if is_rust_source(Path::new(file_path)) {
        return process_rust_file_doc_comments(
            file_path,
            &content,
            &rule_sets.document,
            config,
            original_line_ending,
            line_ending_map,
        );
    }

    // The rules per-file-ignores takes away for this file. Resolved here rather
    // than at the filtering site below because the inline-config validation
    // needs it too, and that runs ahead of the cache lookup.
    let ignored_rules_for_file = config.get_ignored_rules_for_file(Path::new(file_path));

    // Detect unknown rule names in inline disable comments. The result feeds the
    // exit code under --deny-config-warnings, so it is computed even when
    // --silent suppresses the printed notices.
    let inline_config_warning = rumdl_lib::time_section!("file: validate inline config", {
        // The document's own flavor decides what its indentation means, so a
        // directive in a container body is judged as the configuration it is.
        let flavor = config.get_flavor_for_file(Path::new(file_path));
        let mut inline_warnings = rumdl_lib::inline_config::validate_inline_config_rules(&content, flavor);
        // Also flag inline enables that cannot take effect in either the outer
        // document or configured fenced Markdown during this operation.
        let active_rules = rule_sets.configuration_relevant_rule_names(config);
        inline_warnings.extend(rumdl_lib::inline_config::validate_inline_enables_against_active_rules(
            &content,
            flavor,
            &active_rules,
            &ignored_rules_for_file,
        ));
        let had_any = !inline_warnings.is_empty();
        if !silent {
            // The same relative form the findings for this file carry, so both
            // name the file the way the user typed it.
            let display_path = to_display_path(file_path, None);
            for warn in inline_warnings {
                warn.print_warning(&display_path);
            }
        }
        had_any
    });

    // Early content analysis for ultra-fast skip decisions
    if content.is_empty() {
        return ProcessFileResult {
            original_line_ending,
            line_ending_map,
            ..empty_result
        };
    }

    // The rules this document configures itself, which is what decides whether its
    // warnings carry a fix the CLI will apply.
    let document_rules = rules_reconfigured_by_document(&rule_sets.document, config, &content);

    // Compute hashes for cache (Ruff-style: file content + config + enabled rules)
    let (config_hash, rules_hash) = if let Some(hashes) = cache_hashes {
        (Cow::Borrowed(&hashes.config_hash), Cow::Borrowed(&hashes.rules_hash))
    } else {
        (
            Cow::Owned(LintCache::hash_config(config)),
            Cow::Owned(CacheHashes::hash_rule_sets(rule_sets)),
        )
    };
    let file_hash = LintCache::hash_content(&content);
    let md057_rule = if ignored_rules_for_file.contains("MD057") {
        None
    } else {
        rule_sets.document.iter().find_map(|rule| {
            rule.as_any()
                .downcast_ref::<rumdl_lib::rules::MD057ExistingRelativeLinks>()
        })
    };

    // Try to get from cache first (lock briefly for cache read)
    // Note: Cache only stores single-file warnings; cross-file checks must run fresh
    if let Some(ref cache_arc) = cache {
        let flavor = config.get_flavor_for_file(Path::new(file_path));
        let canonical_path = std::fs::canonicalize(file_path).unwrap_or_else(|_| PathBuf::from(file_path));
        let cached_file_index = workspace_index
            .as_deref()
            .and_then(|index| index.get_file(&canonical_path))
            .filter(|cached| cached.content_hash == file_hash);
        let dependency_fingerprint = md057_rule.map(|rule| {
            cached_file_index
                .map(|file_index| rule.cache_dependency_fingerprint(Path::new(file_path), flavor, file_index))
        });
        let dependency_state = match dependency_fingerprint.as_ref() {
            None => DependencyFingerprint::NotRequired,
            Some(None) => DependencyFingerprint::Unavailable,
            Some(Some(fingerprint)) => DependencyFingerprint::Current(fingerprint),
        };
        match rumdl_lib::time_function!(
            "cache: lookup total",
            cache_arc.get_with_reason_for_hash_and_dependencies(
                &file_hash,
                &config_hash,
                &rules_hash,
                dependency_state,
            )
        ) {
            Ok(cached_warnings) => {
                if verbose && !quiet {
                    println!("Cache hit for {file_path}");
                }
                // Count fixable warnings from cache (using capability-based check)
                let fixable_warnings = rumdl_lib::time_function!(
                    "cache hit: count fixable warnings",
                    cached_warnings
                        .iter()
                        .filter(|w| {
                            w.fix.is_some()
                                && w.rule_name.as_ref().is_some_and(|name| {
                                    is_rule_cli_fixable_in(&rule_sets.document, &document_rules, config, name)
                                })
                        })
                        .count()
                );

                // Build FileIndex for cross-file analysis on cache hit (lightweight, no rule checking)
                let (file_index, file_index_reused) = if let Some(file_index) = cached_file_index {
                    (file_index.clone(), true)
                } else {
                    (
                        rumdl_lib::time_function!(
                            "cache hit: build file index",
                            rumdl_lib::build_file_index_only(
                                &content,
                                &rule_sets.document,
                                flavor,
                                Some(std::path::PathBuf::from(file_path)),
                            )
                        ),
                        false,
                    )
                };

                let total_warnings = cached_warnings.len();
                return ProcessFileResult {
                    warnings: cached_warnings,
                    content,
                    total_warnings,
                    fixable_warnings,
                    original_line_ending,
                    line_ending_map,
                    file_index,
                    file_index_reused,
                    errored: false,
                    inline_config_warning,
                };
            }
            Err(reason) => {
                if verbose && !quiet {
                    println!("Cache miss for {file_path}: {reason}");
                }
            }
        }
    }

    let lint_start = Instant::now();

    // Use lint_and_index for single-file linting + index contribution.
    //
    // The full rule set goes in: `lint_and_index` applies this file's
    // per-file-ignores to what it reports, and keeps every cross-file rule for the
    // index it builds. Handing it a pre-filtered set instead erased this file's
    // headings from the workspace, so a link elsewhere pointing at one of them
    // reported as broken.
    let (warnings_result, file_index) = rumdl_lib::time_function!(
        "file: lint and index",
        rumdl_lib::document_run::DocumentRun::new(&content, &rule_sets.document, config)
            .file_path(Path::new(file_path))
            .verbose(verbose)
            .analyze_raw()
    );

    // Combine all warnings
    let mut all_warnings = warnings_result.unwrap_or_default();

    // Warnings from the sources beside the document lint: markdown embedded in a
    // fenced block, and code blocks handed to external tools. Both go through the
    // funnel the re-lint uses, so a fix run reconciles like against like.
    {
        // An embedded block is part of this file, so its findings are this file's
        // and per-file-ignores decides which of them are reported.
        let filtered_rule_sets =
            rumdl_lib::time_function!("file: filter rules", rule_sets.for_file(&ignored_rules_for_file));
        all_warnings.extend(auxiliary_warnings(
            &content,
            file_path,
            &filtered_rule_sets.embedded_markdown,
            filtered_rule_sets.auxiliary,
            config,
        ));
    }

    // Sort warnings by line number, then column
    rumdl_lib::time_section!("file: sort warnings", {
        all_warnings.sort_by(|a, b| {
            if a.line == b.line {
                a.column.cmp(&b.column)
            } else {
                a.line.cmp(&b.line)
            }
        });
    });

    let total_warnings = all_warnings.len();

    // Count fixable issues (using capability-based check)
    let fixable_warnings = all_warnings
        .iter()
        .filter(|w| {
            w.fix.is_some()
                && w.rule_name
                    .as_ref()
                    .is_some_and(|name| is_rule_cli_fixable_in(&rule_sets.document, &document_rules, config, name))
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
    if let Some(ref cache_arc) = cache {
        rumdl_lib::time_section!("cache: store total", {
            let dependency_fingerprint = md057_rule.map(|rule| {
                rule.cache_dependency_fingerprint(
                    Path::new(file_path),
                    config.get_flavor_for_file(Path::new(file_path)),
                    &file_index,
                )
            });
            cache_arc.set_with_hash_and_dependencies(
                &file_hash,
                &config_hash,
                &rules_hash,
                all_warnings.clone(),
                dependency_fingerprint,
            );
        });
    }

    ProcessFileResult {
        warnings: all_warnings,
        content,
        total_warnings,
        fixable_warnings,
        original_line_ending,
        line_ending_map,
        file_index,
        file_index_reused: false,
        errored: false,
        inline_config_warning,
    }
}

/// Apply the fixes a file's own markdown produces.
///
/// The fix-side counterpart of the lint pass, and it has to read a file the way
/// that pass read it. A Rust file is markdown only inside its doc comments, so
/// its source is never handed to the markdown fixer: every edit that produced
/// would be a byte no `check` of the same file ever reported, and the edits are
/// not hypothetical (`#[derive(Debug)]` is an MD018 heading, so the fixer writes
/// `# [derive(Debug)]` and the file stops being Rust). What such a file gets
/// instead is `format_doc_comment_blocks`, reached through `apply_auxiliary_fixes`
/// on the file path and called directly on the stdin path.
///
/// Returns whether the content was rewritten.
pub fn apply_document_fixes(
    rules: &[Box<dyn Rule>],
    content: &mut String,
    quiet: bool,
    silent: bool,
    config: &rumdl_config::Config,
    file_path: Option<&std::path::Path>,
) -> bool {
    if file_path.is_some_and(is_rust_source) {
        return false;
    }

    apply_fixes_coordinated(rules, content, quiet, silent, config, file_path)
}

/// Apply every rule's fix to a document, iterating until the result is stable.
///
/// Reports whether the document changed, and nothing about which warnings that
/// resolved: the coordinator works rule by rule on whole documents, and one
/// rule's rewrite routinely resolves another's finding. Which warnings a fix run
/// resolved is settled afterwards, by re-linting and reconciling (see
/// `fix_reporting`).
///
/// Takes the content it is given as markdown. Callers holding a path go through
/// `apply_document_fixes`, which decides whether the file is markdown at all.
pub fn apply_fixes_coordinated(
    rules: &[Box<dyn Rule>],
    content: &mut String,
    _quiet: bool,
    silent: bool,
    config: &rumdl_config::Config,
    file_path: Option<&std::path::Path>,
) -> bool {
    use std::time::Instant;

    let start = Instant::now();
    let run = rumdl_lib::document_run::DocumentRun::new(content, rules, config);
    let run = match file_path {
        Some(path) => run.file_path(path),
        None => run,
    };

    // Apply fixes iteratively (up to 100 iterations to ensure convergence, same as Ruff).
    match run.fix(100) {
        Ok((fixed_content, result)) => {
            *content = fixed_content;
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
                for line in build_non_convergence_warning_lines(&result, file_path) {
                    eprintln!("{line}");
                }
            }

            !result.fixed_rule_names.is_empty()
        }
        Err(e) => {
            if !silent {
                eprintln!("Warning: Fix coordinator failed: {e}");
            }
            false
        }
    }
}

/// Format an error_messages string (from OnError::Warn path) for user display.
///
/// Input format: `"line 15 (shell): Tool 'shfmt' failed: Exit code 1: <standard input>:3:27: msg"`
/// Output format: `"docs/guide.md:18:27: [shfmt] msg"`
fn format_tool_warning(msg: &str, display_path: &str) -> String {
    // Parse "line N (lang): rest" prefix
    let Some(rest) = msg.strip_prefix("line ") else {
        return format!("{display_path}: {msg}");
    };
    let Some(space_pos) = rest.find(' ') else {
        return format!("{display_path}: {msg}");
    };
    let Ok(fence_line) = rest[..space_pos].parse::<usize>() else {
        return format!("{display_path}: {msg}");
    };
    // Extract "(lang): rest_of_message"
    let after_line = &rest[space_pos + 1..];
    let Some(paren_end) = after_line.find("): ") else {
        return format!("{display_path}: {msg}");
    };
    let error_msg = &after_line[paren_end + 3..];

    // Extract tool name from "Tool 'name' failed: ..." and strip boilerplate
    let (tool_bracket, clean_error) = if let Some(tool_start) = error_msg.find("Tool '") {
        let name_start = tool_start + 6;
        if let Some(name_end) = error_msg[name_start..].find("' failed: ") {
            let tool = &error_msg[name_start..name_start + name_end];
            let after_failed = &error_msg[name_start + name_end + 10..];
            let stripped = strip_exit_code_prefix(after_failed);
            (format!("[{tool}]"), stripped.to_string())
        } else {
            (String::new(), error_msg.to_string())
        }
    } else {
        (String::new(), error_msg.to_string())
    };

    let (location, cleaned) = extract_stdin_location(&clean_error, fence_line);
    let loc = location.unwrap_or_else(|| format!("{fence_line}"));
    if tool_bracket.is_empty() {
        format!("{display_path}:{loc}: {cleaned}")
    } else {
        format!("{display_path}:{loc}: {tool_bracket} {cleaned}")
    }
}

/// Format a code-block-tools ProcessorError for user display.
///
/// For `ToolErrorAt` errors, produces `file:line:col: [tool] message` format matching
/// rumdl's own lint output style. Translates `<standard input>:N:` references to
/// absolute file line numbers and strips boilerplate like exit codes.
fn format_tool_error(err: &ProcessorError, display_path: &str) -> String {
    match err {
        ProcessorError::ToolErrorAt {
            error,
            line: fence_line,
            ..
        } => match error {
            ExecutorError::ExecutionFailed { tool, message } => {
                let stripped = strip_exit_code_prefix(message);
                let (location, cleaned) = extract_stdin_location(stripped, *fence_line);
                let loc = location.unwrap_or_else(|| format!("{fence_line}"));
                format!("{display_path}:{loc}: [{tool}] {cleaned}")
            }
            ExecutorError::Timeout { tool, timeout_ms } => {
                format!("{display_path}:{fence_line}: [{tool}] timed out after {timeout_ms}ms")
            }
            ExecutorError::RepeatedTimeouts {
                tool,
                timeout_ms,
                timeouts,
            } => {
                format!(
                    "{display_path}:{fence_line}: [{tool}] skipped after timing out {timeouts} times at {timeout_ms}ms; the tool is likely not reading stdin"
                )
            }
            ExecutorError::ToolNotFound { tool } => {
                format!("{display_path}:{fence_line}: [{tool}] not found in PATH")
            }
            ExecutorError::IoError { message } => {
                format!("{display_path}:{fence_line}: I/O error: {message}")
            }
        },
        _ => format!("{display_path}: {err}"),
    }
}

/// Strip "Exit code N: " prefix from tool error messages.
fn strip_exit_code_prefix(message: &str) -> &str {
    if let Some(rest) = message.strip_prefix("Exit code ")
        && let Some(colon_pos) = rest.find(": ")
    {
        return &rest[colon_pos + 2..];
    }
    message
}

/// Extract `<standard input>:N:M:` from a tool error message, returning the absolute
/// `line:col` string and the cleaned-up message with the stdin reference removed.
///
/// Returns `(Some("18:27"), "Tool 'shfmt' failed: Exit code 1: `>` must be...")` on
/// success, or `(None, original_message)` if no stdin reference is found.
fn extract_stdin_location(message: &str, fence_line: usize) -> (Option<String>, String) {
    const STDIN_PREFIX: &str = "<standard input>:";
    let Some(pos) = message.find(STDIN_PREFIX) else {
        return (None, message.to_string());
    };
    let after = &message[pos + STDIN_PREFIX.len()..];
    // Parse line number
    let Some(first_colon) = after.find(':') else {
        return (None, message.to_string());
    };
    let Ok(tool_line) = after[..first_colon].parse::<usize>() else {
        return (None, message.to_string());
    };
    let absolute_line = fence_line + tool_line;

    // Try to parse column number
    let rest_after_line = &after[first_colon + 1..];
    let (location, remaining_start) = if let Some(second_colon) = rest_after_line.find(':')
        && let Ok(col) = rest_after_line[..second_colon].parse::<usize>()
    {
        // Have both line and column
        let skip = pos + STDIN_PREFIX.len() + first_colon + 1 + second_colon + 1;
        (format!("{absolute_line}:{col}"), skip)
    } else {
        // Only line number
        let skip = pos + STDIN_PREFIX.len() + first_colon + 1;
        (format!("{absolute_line}"), skip)
    };

    // Reconstruct message: everything before the stdin ref + everything after line:col:
    let before = message[..pos].trim_end();
    let after_ref = message[remaining_start..].trim_start();
    let cleaned = if before.is_empty() {
        after_ref.to_string()
    } else if after_ref.is_empty() {
        before.to_string()
    } else {
        format!("{before} {after_ref}")
    };
    (Some(location), cleaned)
}

fn format_loop(cycle: &[String]) -> Option<String> {
    if cycle.is_empty() {
        return None;
    }

    let mut parts = cycle.to_vec();
    if let Some(first) = parts.first().cloned() {
        parts.push(first);
    }
    Some(parts.join(" -> "))
}

fn build_non_convergence_warning_lines(
    result: &rumdl_lib::fix_coordinator::FixResult,
    file_path: Option<&Path>,
) -> Vec<String> {
    let mut lines = Vec::new();
    let location = file_path.map(|p| format!(" for {}", p.display())).unwrap_or_default();

    if !result.conflicting_rules.is_empty() {
        let mut rules = result.conflicting_rules.clone();
        rules.sort();
        let rule_list = rules.join(", ");
        let primary_rule = rules[0].clone();

        lines.push(format!(
            "Warning: Auto-fix detected a rule conflict loop after {} iterations{}.",
            result.iterations, location
        ));
        lines.push(format!("Conflicting rules: {rule_list}"));
        if let Some(loop_str) = format_loop(&result.conflict_cycle) {
            lines.push(format!("Observed cycle: {loop_str}"));
        }
        lines.push("Actionable options:".to_string());
        lines.push(format!(
            "  - Keep linting but stop auto-fixing one rule: [global] unfixable = [\"{primary_rule}\"]"
        ));
        lines.push(format!(
            "  - Disable one rule entirely for this run: rumdl check --fix --disable {primary_rule}"
        ));
        lines.push(format!(
            "  - Disable one rule in config: [global] disable = [\"{primary_rule}\"]"
        ));
        lines.push(
            "If this looks wrong, please report it: https://github.com/rvben/rumdl/issues/new?template=bug_report.yml"
                .to_string(),
        );
        return lines;
    }

    let mut fixed_rules: Vec<String> = result.fixed_rule_names.iter().cloned().collect();
    fixed_rules.sort();
    let fixed_rules_list = if fixed_rules.is_empty() {
        "(none)".to_string()
    } else {
        fixed_rules.join(", ")
    };

    lines.push(format!(
        "Warning: Auto-fix did not converge after {} iterations{}.",
        result.iterations, location
    ));
    lines.push("No repeatable cycle was detected; this is likely a convergence bug.".to_string());
    lines.push(format!("Rules that changed content: {fixed_rules_list}"));
    if !fixed_rules.is_empty() {
        let quoted_rules = fixed_rules
            .iter()
            .map(|r| format!("\"{r}\""))
            .collect::<Vec<_>>()
            .join(", ");
        lines.push(format!(
            "Try narrowing auto-fix scope: [global] fixable = [{quoted_rules}]"
        ));
    }
    lines.push("Please report it: https://github.com/rvben/rumdl/issues/new?template=bug_report.yml".to_string());
    lines
}

/// Process a Rust source file by linting markdown in doc comments.
///
/// Returns a `ProcessFileResult` with warnings remapped to their original file
/// positions. No cross-file analysis is performed for doc comments.
fn process_rust_file_doc_comments(
    file_path: &str,
    content: &str,
    rules: &[Box<dyn Rule>],
    config: &rumdl_config::Config,
    original_line_ending: rumdl_lib::utils::LineEnding,
    line_ending_map: rumdl_lib::utils::NormalizedLineEndingMap,
) -> ProcessFileResult {
    // Filter rules based on per-file-ignores configuration
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

    let all_warnings = rumdl_lib::doc_comment_lint::check_doc_comment_blocks(content, &filtered_rules, config);

    let total_warnings = all_warnings.len();
    // Doc comment warnings have fix stripped (fix: None) in check mode, so
    // determine fixability by checking the rule's fix capability instead.
    let fixable_warnings = all_warnings
        .iter()
        .filter(|w| {
            w.rule_name
                .as_ref()
                .is_some_and(|name| is_rule_cli_fixable(rules, config, name))
        })
        .count();

    ProcessFileResult {
        warnings: all_warnings,
        content: content.to_string(),
        total_warnings,
        fixable_warnings,
        original_line_ending,
        line_ending_map,
        file_index: rumdl_lib::workspace_index::FileIndex::new(),
        file_index_reused: false,
        errored: false,
        // Rust doc-comment linting does not process markdown inline disable
        // comments (the rust path returns before that detection runs).
        inline_config_warning: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rumdl_lib::fix_coordinator::FixResult;
    use std::collections::HashSet;

    #[test]
    fn test_build_non_convergence_warning_lines_conflict_loop() {
        let result = FixResult {
            rules_fixed: 3,
            iterations: 6,
            context_creations: 6,
            fixed_rule_names: ["MD044".to_string(), "MD063".to_string()].into_iter().collect(),
            converged: false,
            conflicting_rules: vec!["MD063".to_string(), "MD044".to_string()],
            conflict_cycle: vec!["MD044".to_string(), "MD063".to_string()],
        };

        let lines = build_non_convergence_warning_lines(&result, Some(Path::new("docs/guide.md")));
        let rendered = lines.join("\n");

        assert!(rendered.contains("rule conflict loop"));
        assert!(rendered.contains("for docs/guide.md"));
        assert!(rendered.contains("Conflicting rules: MD044, MD063"));
        assert!(rendered.contains("Observed cycle: MD044 -> MD063 -> MD044"));
        assert!(rendered.contains("[global] unfixable = [\"MD044\"]"));
        assert!(rendered.contains("rumdl check --fix --disable MD044"));
    }

    #[test]
    fn test_build_non_convergence_warning_lines_max_iterations() {
        let result = FixResult {
            rules_fixed: 10,
            iterations: 100,
            context_creations: 100,
            fixed_rule_names: ["MD009".to_string(), "MD012".to_string()].into_iter().collect(),
            converged: false,
            conflicting_rules: Vec::new(),
            conflict_cycle: Vec::new(),
        };

        let lines = build_non_convergence_warning_lines(&result, None);
        let rendered = lines.join("\n");

        assert!(rendered.contains("did not converge after 100 iterations"));
        assert!(rendered.contains("Rules that changed content: MD009, MD012"));
        assert!(rendered.contains("[global] fixable = [\"MD009\", \"MD012\"]"));
        assert!(rendered.contains("Please report it"));
    }

    #[test]
    fn test_format_loop_renders_closed_cycle() {
        let cycle = vec!["MD044".to_string(), "MD063".to_string()];
        assert_eq!(format_loop(&cycle).as_deref(), Some("MD044 -> MD063 -> MD044"));
    }

    #[test]
    fn test_format_loop_empty() {
        assert!(format_loop(&[]).is_none());
    }

    #[test]
    fn test_build_non_convergence_warning_lines_handles_empty_rule_set() {
        let result = FixResult {
            rules_fixed: 0,
            iterations: 100,
            context_creations: 100,
            fixed_rule_names: HashSet::new(),
            converged: false,
            conflicting_rules: Vec::new(),
            conflict_cycle: Vec::new(),
        };

        let lines = build_non_convergence_warning_lines(&result, Some(Path::new("README.md")));
        let rendered = lines.join("\n");

        assert!(rendered.contains("for README.md"));
        assert!(rendered.contains("Rules that changed content: (none)"));
    }

    #[test]
    fn extract_stdin_location_with_line_and_col() {
        let msg = "<standard input>:3:27: `>` must be followed by a word";
        let (loc, cleaned) = super::extract_stdin_location(msg, 15);
        assert_eq!(loc.as_deref(), Some("18:27"));
        assert_eq!(cleaned, "`>` must be followed by a word");
    }

    #[test]
    fn extract_stdin_location_line_only() {
        let msg = "<standard input>:5: syntax error";
        let (loc, cleaned) = super::extract_stdin_location(msg, 10);
        assert_eq!(loc.as_deref(), Some("15"));
        assert_eq!(cleaned, "syntax error");
    }

    #[test]
    fn extract_stdin_location_no_stdin_ref() {
        let msg = "Unknown option --foo";
        let (loc, cleaned) = super::extract_stdin_location(msg, 10);
        assert!(loc.is_none());
        assert_eq!(cleaned, msg);
    }

    #[test]
    fn extract_stdin_location_mid_string() {
        let msg = "some prefix <standard input>:3:27: error text";
        let (loc, cleaned) = super::extract_stdin_location(msg, 15);
        assert_eq!(loc.as_deref(), Some("18:27"));
        assert_eq!(cleaned, "some prefix error text");
    }

    #[test]
    fn strip_exit_code_prefix_present() {
        assert_eq!(super::strip_exit_code_prefix("Exit code 1: some error"), "some error");
        assert_eq!(super::strip_exit_code_prefix("Exit code 127: not found"), "not found");
    }

    #[test]
    fn strip_exit_code_prefix_absent() {
        assert_eq!(
            super::strip_exit_code_prefix("some error without prefix"),
            "some error without prefix"
        );
    }

    #[test]
    fn format_tool_error_execution_failed_with_stdin() {
        use rumdl_lib::code_block_tools::executor::ExecutorError;
        use rumdl_lib::code_block_tools::processor::ProcessorError;
        let err = ProcessorError::ToolErrorAt {
            error: ExecutorError::ExecutionFailed {
                tool: "shfmt".to_string(),
                message: "Exit code 1: <standard input>:3:27: `>` must be followed by a word".to_string(),
            },
            line: 15,
            language: "shell".to_string(),
        };
        assert_eq!(
            super::format_tool_error(&err, "docs/guide.md"),
            "docs/guide.md:18:27: [shfmt] `>` must be followed by a word"
        );
    }

    #[test]
    fn format_tool_error_execution_failed_without_stdin() {
        use rumdl_lib::code_block_tools::executor::ExecutorError;
        use rumdl_lib::code_block_tools::processor::ProcessorError;
        let err = ProcessorError::ToolErrorAt {
            error: ExecutorError::ExecutionFailed {
                tool: "black".to_string(),
                message: "Exit code 1: cannot format".to_string(),
            },
            line: 15,
            language: "python".to_string(),
        };
        assert_eq!(
            super::format_tool_error(&err, "readme.md"),
            "readme.md:15: [black] cannot format"
        );
    }

    #[test]
    fn format_tool_error_timeout() {
        use rumdl_lib::code_block_tools::executor::ExecutorError;
        use rumdl_lib::code_block_tools::processor::ProcessorError;
        let err = ProcessorError::ToolErrorAt {
            error: ExecutorError::Timeout {
                tool: "prettier".to_string(),
                timeout_ms: 5000,
            },
            line: 20,
            language: "javascript".to_string(),
        };
        assert_eq!(
            super::format_tool_error(&err, "test.md"),
            "test.md:20: [prettier] timed out after 5000ms"
        );
    }

    #[test]
    fn format_tool_warning_with_stdin_ref() {
        let msg = "line 15 (shell): Tool 'shfmt' failed: Exit code 1: <standard input>:3:27: bad syntax";
        let result = super::format_tool_warning(msg, "docs/guide.md");
        assert_eq!(result, "docs/guide.md:18:27: [shfmt] bad syntax");
    }

    #[test]
    fn format_tool_warning_without_stdin_ref() {
        let msg = "line 15 (python): Tool 'black' failed: Exit code 1: cannot format";
        let result = super::format_tool_warning(msg, "readme.md");
        assert_eq!(result, "readme.md:15: [black] cannot format");
    }

    #[test]
    fn format_tool_warning_no_prefix() {
        let msg = "No format tools configured for language 'ruby' at line 5";
        let result = super::format_tool_warning(msg, "test.md");
        assert_eq!(
            result,
            "test.md: No format tools configured for language 'ruby' at line 5"
        );
    }
}
