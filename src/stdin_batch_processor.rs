//! NUL-framed multi-document stdin processing for `rumdl check`.

use crate::check_runner::{CheckRunContext, CheckRunOutcome};
use colored::Colorize;
use rayon::prelude::*;
use rumdl_lib::output::{OutputFormat, OutputWriter};
use rumdl_lib::rule::{LintWarning, Severity};
use rumdl_lib::workspace_index::{FileIndex, WorkspaceIndex, link_target_candidates, normalize_relative_path};
use std::collections::{HashMap, HashSet};
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::time::Instant;

#[derive(Debug)]
struct SuppliedDocument {
    path: String,
    content: String,
}

struct AnalyzedDocument {
    group_index: usize,
    normalized_path: PathBuf,
    display_path: String,
    warnings: Vec<LintWarning>,
    file_index: FileIndex,
}

fn read_documents() -> Result<Vec<SuppliedDocument>, String> {
    let mut input = Vec::new();
    io::stdin()
        .read_to_end(&mut input)
        .map_err(|error| format!("failed to read stdin: {error}"))?;

    parse_documents(&input)
}

fn parse_documents(input: &[u8]) -> Result<Vec<SuppliedDocument>, String> {
    if input.is_empty() {
        return Ok(Vec::new());
    }
    if input.last() != Some(&0) {
        return Err("batch input must end with a NUL byte".to_string());
    }

    let fields: Vec<&[u8]> = input[..input.len() - 1].split(|byte| *byte == 0).collect();
    if !fields.len().is_multiple_of(2) {
        return Err("batch input must contain NUL-delimited path/content pairs".to_string());
    }

    fields
        .chunks_exact(2)
        .map(|pair| {
            let path =
                std::str::from_utf8(pair[0]).map_err(|_| "batch document path is not valid UTF-8".to_string())?;
            if path.is_empty() {
                return Err("batch document path cannot be empty".to_string());
            }
            let content =
                std::str::from_utf8(pair[1]).map_err(|_| format!("batch content for '{path}' is not valid UTF-8"))?;
            Ok(SuppliedDocument {
                path: path.to_string(),
                content: rumdl_lib::utils::normalize_line_ending(content, rumdl_lib::utils::LineEnding::Lf)
                    .into_owned(),
            })
        })
        .collect()
}

pub fn process_stdin_batch(ctx: &CheckRunContext<'_>, output_format: OutputFormat) -> CheckRunOutcome {
    let documents = match read_documents() {
        Ok(documents) => documents,
        Err(error) => {
            if !ctx.args.silent {
                eprintln!("{}: invalid --stdin-batch input: {error}", "Error".red().bold());
            }
            return CheckRunOutcome::tool_error();
        }
    };

    let mut seen = HashSet::new();
    if let Some(duplicate) = documents
        .iter()
        .find(|document| !seen.insert(normalize_relative_path(Path::new(&document.path))))
    {
        if !ctx.args.silent {
            eprintln!(
                "{}: invalid --stdin-batch input: duplicate path '{}'",
                "Error".red().bold(),
                duplicate.path
            );
        }
        return CheckRunOutcome::tool_error();
    }

    let paths: Vec<String> = documents.iter().map(|document| document.path.clone()).collect();
    let resolved = crate::resolution::resolve_config_groups(
        &paths,
        &crate::resolution::RootConfig {
            config: ctx.config,
            sourced: ctx.sourced,
        },
        ctx.args,
        &crate::resolution::ResolutionRoots {
            grouping_root: ctx.grouping_root,
            project_root: ctx.project_root,
        },
        ctx.inline_overrides,
        &None,
        ctx.explicit_config || ctx.isolated,
    );

    let mut groups_by_path: HashMap<&str, usize> = HashMap::new();
    for (group_index, group) in resolved.groups.iter().enumerate() {
        for path in &group.files {
            groups_by_path.insert(path, group_index);
        }
    }
    let mut config_warning = resolved.config_warning;

    let start = Instant::now();
    let mut analyzed = Vec::with_capacity(documents.len());
    let mut workspace_index = WorkspaceIndex::new();
    let supplied_document_paths = || documents.iter().map(|document| Path::new(&document.path));
    let link_target_policy = if ctx.args.stdin_batch_closed_world {
        rumdl_lib::lint_context::LinkTargetPolicy::closed_world(supplied_document_paths())
    } else {
        rumdl_lib::lint_context::LinkTargetPolicy::open_world(supplied_document_paths())
    };

    // Validate inline configuration in input order so notices remain stable,
    // independent of the parallel lint pass below.
    for document in &documents {
        let Some(&group_index) = groups_by_path.get(document.path.as_str()) else {
            if !ctx.args.silent {
                eprintln!(
                    "{}: failed to resolve configuration for '{}'",
                    "Error".red().bold(),
                    document.path
                );
            }
            return CheckRunOutcome::tool_error();
        };
        let group = &resolved.groups[group_index];
        let path = Path::new(&document.path);
        let flavor = group.config.get_flavor_for_file(path);
        let ignored_for_file = group.config.get_ignored_rules_for_file(path);
        let mut inline_warnings = rumdl_lib::inline_config::validate_inline_config_rules(&document.content, flavor);
        let active_rules: HashSet<String> = group.rules.iter().map(|rule| rule.name().to_string()).collect();
        inline_warnings.extend(rumdl_lib::inline_config::validate_inline_enables_against_active_rules(
            &document.content,
            flavor,
            &active_rules,
            &ignored_for_file,
        ));
        config_warning |= !inline_warnings.is_empty();
        if !ctx.args.silent {
            for warning in inline_warnings {
                warning.print_warning(&document.path);
            }
        }
    }

    // Rayon preserves indexed-iterator collection order, so documents lint in
    // parallel without changing the caller's diagnostic order.
    let analysis_results: Vec<Result<AnalyzedDocument, String>> = documents
        .par_iter()
        .map(|document| {
            let group_index = groups_by_path[document.path.as_str()];
            let group = &resolved.groups[group_index];
            let path = Path::new(&document.path);
            let rules = rumdl_lib::rules::filter_rules_for_file(&group.rules, &group.config, path);
            let run = rumdl_lib::document_run::DocumentRun::new(&document.content, &rules, &group.config)
                .verbose(ctx.args.verbose)
                .file_path(path)
                .link_target_policy(&link_target_policy);
            let (result, file_index) = run.analyze_raw();
            let warnings = result.map_err(|error| error.to_string())?;
            let display_path =
                crate::file_processor::resolve_display_path(&document.path, ctx.args.show_full_path, ctx.project_root);
            Ok(AnalyzedDocument {
                group_index,
                normalized_path: normalize_relative_path(path),
                display_path,
                warnings,
                file_index,
            })
        })
        .collect();

    for result in analysis_results {
        let document = match result {
            Ok(document) => document,
            Err(error) => {
                if !ctx.args.silent {
                    eprintln!("{}: {error}", "Error".red().bold());
                }
                return CheckRunOutcome::tool_error();
            }
        };
        workspace_index.insert_file(document.normalized_path.clone(), document.file_index.clone());
        analyzed.push(document);
    }

    // Open-world batches only replace documents they explicitly supply. Resolve
    // other referenced Markdown documents through the same scanner as a normal
    // workspace run, so ignore rules and extension filtering stay identical.
    let supplied_paths: HashSet<PathBuf> = analyzed
        .iter()
        .map(|document| document.normalized_path.clone())
        .collect();
    let mut attempted = HashSet::new();
    let mut disk_targets = Vec::new();
    let mut scanned_files: Option<HashSet<PathBuf>> = None;
    for document in &analyzed {
        if ctx.args.stdin_batch_closed_world {
            break;
        }
        for link in &document.file_index.cross_file_links {
            if link.fragment.is_empty() {
                continue;
            }
            for candidate in link_target_candidates(&document.normalized_path, &link.target_path) {
                if supplied_paths.contains(&candidate) {
                    break;
                }
                if !attempted.insert(candidate.clone()) {
                    continue;
                }
                let Some(canonical) = rumdl_lib::discovery::canonicalize_for_matching(&candidate) else {
                    continue;
                };
                let scanned = scanned_files.get_or_insert_with(|| {
                    crate::file_processor::find_markdown_files(&[], ctx.args, ctx.config, ctx.project_root)
                        .map(|discovered| {
                            discovered
                                .files
                                .iter()
                                .filter_map(|path| rumdl_lib::discovery::canonicalize_for_matching(Path::new(path)))
                                .collect()
                        })
                        .unwrap_or_default()
                });
                if scanned.contains(&canonical) {
                    disk_targets.push(candidate);
                    break;
                }
            }
        }
    }

    if !disk_targets.is_empty() {
        let target_paths: Vec<String> = disk_targets
            .iter()
            .map(|path| path.to_string_lossy().into_owned())
            .collect();
        let disk_resolved = crate::resolution::resolve_config_groups(
            &target_paths,
            &crate::resolution::RootConfig {
                config: ctx.config,
                sourced: ctx.sourced,
            },
            ctx.args,
            &crate::resolution::ResolutionRoots {
                grouping_root: ctx.grouping_root,
                project_root: ctx.project_root,
            },
            ctx.inline_overrides,
            &None,
            ctx.explicit_config || ctx.isolated,
        );
        config_warning |= disk_resolved.config_warning;
        for group in &disk_resolved.groups {
            for target in &group.files {
                let path = PathBuf::from(target);
                let Ok(content) = std::fs::read_to_string(&path) else {
                    continue;
                };
                let flavor = group.config.get_flavor_for_file(&path);
                let file_index = rumdl_lib::build_file_index_only(&content, &group.rules, flavor, Some(path.clone()));
                workspace_index.insert_file(normalize_relative_path(&path), file_index);
            }
        }
    }

    // Every supplied document is indexed before cross-file checks begin. This
    // makes references resolve against the batch snapshot, including content
    // that differs from the file currently saved at the same path.
    for document in &mut analyzed {
        let group = &resolved.groups[document.group_index];
        let rules = rumdl_lib::rules::filter_rules_for_file(&group.rules, &group.config, &document.normalized_path);
        match rumdl_lib::run_cross_file_checks(
            &document.normalized_path,
            &document.file_index,
            &rules,
            &workspace_index,
            Some(&group.config),
        ) {
            Ok(warnings) => document.warnings.extend(warnings),
            Err(error) => {
                if !ctx.args.silent {
                    eprintln!("{}: {error}", "Error".red().bold());
                }
                return CheckRunOutcome::tool_error();
            }
        }
        document.warnings.sort_by_key(|warning| (warning.line, warning.column));
    }

    let writer = OutputWriter::new(ctx.args.stderr, ctx.args.silent);
    let formatter = output_format.create_formatter();
    let mut batch_file_warnings: Vec<(String, Vec<LintWarning>)> = Vec::new();
    let batch_all_files: Vec<String> = analyzed.iter().map(|document| document.display_path.clone()).collect();
    let mut files_with_issues = 0;
    let mut total_issues = 0;
    let mut total_fixable_issues = 0;
    let mut has_warnings = false;
    let mut has_errors = false;
    let mut all_warnings_for_stats = Vec::new();

    for (document, analyzed_document) in documents.iter().zip(&analyzed) {
        if analyzed_document.warnings.is_empty() {
            continue;
        }
        files_with_issues += 1;
        total_issues += analyzed_document.warnings.len();
        has_warnings |= analyzed_document
            .warnings
            .iter()
            .any(|warning| matches!(warning.severity, Severity::Warning | Severity::Error));
        has_errors |= analyzed_document
            .warnings
            .iter()
            .any(|warning| warning.severity == Severity::Error);

        let group = &resolved.groups[analyzed_document.group_index];
        let document_rules =
            crate::file_processor::rules_reconfigured_by_document(&group.rules, &group.config, &document.content);
        total_fixable_issues += analyzed_document
            .warnings
            .iter()
            .filter(|warning| {
                warning.fix.is_some()
                    && crate::file_processor::is_rule_cli_fixable_in(
                        &group.rules,
                        &document_rules,
                        &group.config,
                        warning.rule_name.as_deref().unwrap_or(""),
                    )
            })
            .count();
        if ctx.args.statistics {
            all_warnings_for_stats.extend(analyzed_document.warnings.clone());
        }

        if !output_format.is_batch() {
            let formatted = formatter.format_warnings_with_content(
                &analyzed_document.warnings,
                &analyzed_document.display_path,
                &document.content,
            );
            if !formatted.is_empty() {
                writer.writeln(&formatted).unwrap_or_else(|error| {
                    eprintln!("Error writing output: {error}");
                });
            }
        }
        batch_file_warnings.push((
            analyzed_document.display_path.clone(),
            analyzed_document.warnings.clone(),
        ));
    }

    if let Some(output) = output_format.format_batch(
        &batch_file_warnings,
        &batch_all_files,
        start.elapsed().as_millis() as u64,
    ) {
        writer.writeln(&output).unwrap_or_else(|error| {
            eprintln!("Error writing output: {error}");
        });
    }

    if !ctx.quiet && !ctx.args.silent && !output_format.is_batch() && !output_format.is_machine_readable() {
        crate::formatter::print_results_from_checkargs(crate::formatter::PrintResultsArgs {
            args: ctx.args,
            has_issues: total_issues > 0,
            files_with_issues,
            files_fixed: 0,
            total_issues,
            summary_issues_fixed: 0,
            total_issues_fixed: 0,
            total_fixable_issues,
            total_files_processed: documents.len(),
            duration_ms: start.elapsed().as_millis() as u64,
            had_tool_error: false,
        });
    }

    if ctx.args.statistics
        && !ctx.quiet
        && !ctx.args.silent
        && !output_format.is_batch()
        && !output_format.is_machine_readable()
        && !all_warnings_for_stats.is_empty()
    {
        crate::formatter::print_statistics(&all_warnings_for_stats);
    }

    CheckRunOutcome {
        has_issues: total_issues > 0,
        has_warnings,
        has_errors,
        total_issues_fixed: 0,
        had_tool_error: false,
        config_warning,
        reads_editorconfig: resolved.groups.iter().any(|group| group.config.global.editorconfig),
    }
}

#[cfg(test)]
mod tests {
    use super::parse_documents;

    #[test]
    fn parser_accepts_empty_content_and_empty_input() {
        assert!(parse_documents(b"").unwrap().is_empty());
        let documents = parse_documents(b"empty.md\0\0").unwrap();
        assert_eq!(documents.len(), 1);
        assert_eq!(documents[0].path, "empty.md");
        assert_eq!(documents[0].content, "");
    }

    #[test]
    fn parser_preserves_newlines_inside_content() {
        let documents = parse_documents(b"a.md\0# A\n\nBody\n\0b.md\0# B\n\0").unwrap();
        assert_eq!(documents.len(), 2);
        assert_eq!(documents[0].content, "# A\n\nBody\n");
        assert_eq!(documents[1].content, "# B\n");
    }

    #[test]
    fn parser_requires_complete_nul_terminated_pairs() {
        assert_eq!(
            parse_documents(b"a.md\0content").unwrap_err(),
            "batch input must end with a NUL byte"
        );
        assert_eq!(
            parse_documents(b"a.md\0content\0orphan.md\0").unwrap_err(),
            "batch input must contain NUL-delimited path/content pairs"
        );
    }

    #[test]
    fn parser_rejects_empty_and_non_utf8_paths() {
        assert_eq!(
            parse_documents(b"\0content\0").unwrap_err(),
            "batch document path cannot be empty"
        );
        assert_eq!(
            parse_documents(b"\xff\0content\0").unwrap_err(),
            "batch document path is not valid UTF-8"
        );
    }

    #[test]
    fn parser_rejects_non_utf8_content_with_its_path() {
        assert_eq!(
            parse_documents(b"a.md\0\xff\0").unwrap_err(),
            "batch content for 'a.md' is not valid UTF-8"
        );
    }
}
