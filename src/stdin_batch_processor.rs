//! NUL-framed multi-document stdin processing for `rumdl check`.

use crate::check_runner::{CheckRunContext, CheckRunOutcome};
use colored::Colorize;
use rayon::prelude::*;
use rumdl_lib::doc_comment_lint::is_rust_source;
use rumdl_lib::output::{OutputFormat, OutputWriter};
use rumdl_lib::rule::{LintWarning, Severity};
use rumdl_lib::rules::{LinkResolution, LinkResolver};
use rumdl_lib::workspace_index::{FileIndex, WorkspaceIndex, link_target_candidates, workspace_key};
use std::collections::{HashMap, HashSet};
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::time::Instant;

#[derive(Debug)]
struct SuppliedDocument {
    path: String,
    /// The supplied text with every line ending normalized to LF.
    content: String,
    /// Where the supplied text had CRLF endings, so JSON fixes address its bytes.
    line_endings: rumdl_lib::utils::NormalizedLineEndingMap,
    encoding: SuppliedEncoding,
}

/// How a supplied document's bytes decoded. NUL frames the batch, so content
/// holding one never reaches here; a UTF-16 byte order mark still can.
#[derive(Debug, PartialEq)]
enum SuppliedEncoding {
    Utf8,
    /// Linted from a lossy decoding; MD094 reports these sequences.
    Lossy(Vec<rumdl_lib::encoding::InvalidSeq>),
    /// Not linted; MD094 reports the file once.
    Binary {
        utf16: bool,
    },
}

struct AnalyzedDocument {
    group_index: usize,
    normalized_path: PathBuf,
    /// The path per-file settings are matched against: the supplied path taken
    /// from the working directory, which need not exist on disk.
    config_path: PathBuf,
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
            let (content, encoding) = match rumdl_lib::encoding::decode(pair[1]) {
                rumdl_lib::encoding::Decoded::Utf8(content) => (content.to_string(), SuppliedEncoding::Utf8),
                rumdl_lib::encoding::Decoded::Lossy { text, invalid } => (text, SuppliedEncoding::Lossy(invalid)),
                rumdl_lib::encoding::Decoded::Binary { utf16 } => (String::new(), SuppliedEncoding::Binary { utf16 }),
            };
            Ok(SuppliedDocument {
                path: path.to_string(),
                line_endings: rumdl_lib::utils::NormalizedLineEndingMap::new(&content),
                content: rumdl_lib::utils::normalize_line_ending(&content, rumdl_lib::utils::LineEnding::Lf)
                    .into_owned(),
                encoding,
            })
        })
        .collect()
}

/// Paths declared to exist without being linted, split into files and
/// directories (entries with a trailing `/`), each deduplicated after
/// normalization.
#[derive(Debug, Default, PartialEq)]
struct DeclaredTargets {
    files: Vec<PathBuf>,
    directories: Vec<PathBuf>,
}

fn read_targets(file: &Path) -> Result<DeclaredTargets, String> {
    if file == Path::new("-") {
        return Err("'-' is not allowed because stdin carries the batch".to_string());
    }
    let input = std::fs::read(file).map_err(|error| format!("failed to read '{}': {error}", file.display()))?;
    parse_targets(&input)
}

fn parse_targets(input: &[u8]) -> Result<DeclaredTargets, String> {
    let mut targets = DeclaredTargets::default();
    if input.is_empty() {
        return Ok(targets);
    }
    if input.last() != Some(&0) {
        return Err("the list must end with a NUL byte".to_string());
    }
    let mut seen_files = HashSet::new();
    let mut seen_directories = HashSet::new();
    for entry in input[..input.len() - 1].split(|byte| *byte == 0) {
        let entry = std::str::from_utf8(entry).map_err(|_| "a path is not valid UTF-8".to_string())?;
        if entry.is_empty() {
            return Err("a path cannot be empty".to_string());
        }
        let is_separator = |c: char| c == '/' || (cfg!(windows) && c == '\\');
        let is_directory = entry.ends_with(is_separator);
        // Trailing separators are left to path parsing, which ignores them
        // but keeps a root (`/`, `C:\`) intact.
        let path = Path::new(entry);
        let key = rumdl_lib::workspace_index::normalize_relative_path(path);
        if is_directory {
            if seen_directories.insert(key) {
                targets.directories.push(path.to_path_buf());
            }
        } else if seen_files.insert(key) {
            targets.files.push(path.to_path_buf());
        }
    }
    Ok(targets)
}

pub fn process_stdin_batch(ctx: &CheckRunContext<'_>, output_format: OutputFormat) -> CheckRunOutcome {
    // Read before stdin so a bad list fails before the batch is consumed.
    let targets = match ctx.args.stdin_batch_targets.as_deref().map(read_targets) {
        Some(Ok(targets)) => targets,
        Some(Err(error)) => {
            if !ctx.args.silent {
                eprintln!("{}: invalid --stdin-batch-targets input: {error}", "Error".red().bold());
            }
            return CheckRunOutcome::tool_error();
        }
        None => DeclaredTargets::default(),
    };
    let documents = match read_documents() {
        Ok(documents) => documents,
        Err(error) => {
            if !ctx.args.silent {
                eprintln!("{}: invalid --stdin-batch input: {error}", "Error".red().bold());
            }
            return CheckRunOutcome::tool_error();
        }
    };

    // `docs/a.md` and its absolute spelling are one document, keyed once.
    let mut seen = HashSet::new();
    if let Some(duplicate) = documents
        .iter()
        .find(|document| !seen.insert(workspace_key(Path::new(&document.path))))
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

    // A supplied path the exclude patterns remove is a file this run does not
    // lint, exactly as `rumdl check <file>` skips it. It is still part of the
    // snapshot the caller described, so it stays a valid link target below.
    let exclude_matchers = crate::file_processor::run_exclude_matchers(ctx.args, ctx.config);
    let linted: Vec<&SuppliedDocument> = documents
        .iter()
        .filter(|document| {
            match crate::file_processor::named_file_exclude_pattern(
                &exclude_matchers,
                ctx.project_root,
                Path::new(&document.path),
            ) {
                Some(pattern) => {
                    crate::file_processor::report_named_file_excluded(ctx.args, &document.path, pattern);
                    false
                }
                None => true,
            }
        })
        .collect();
    let refused: Vec<&str> = linted
        .iter()
        .map(|document| document.path.as_str())
        .filter(|path| {
            rumdl_lib::discovery::classify_file(Path::new(path)) == rumdl_lib::discovery::FileKind::SourceCode
        })
        .collect();
    if !refused.is_empty() {
        for path in refused {
            crate::check_runner::report_source_code_refused(ctx.args, path);
        }
        return CheckRunOutcome::tool_error();
    }
    if linted.is_empty() && !documents.is_empty() {
        return crate::check_runner::report_empty_run(
            ctx.args,
            output_format,
            &crate::file_processor::EmptyDiscovery::all_named_files_excluded(documents.len()),
            false,
        );
    }

    let paths: Vec<String> = linted.iter().map(|document| document.path.clone()).collect();
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
    let mut analyzed = Vec::with_capacity(linted.len());
    let mut workspace_index = WorkspaceIndex::new();
    let supplied_document_paths = || documents.iter().map(|document| Path::new(&document.path));
    let link_target_policy = rumdl_lib::lint_context::LinkTargetPolicy::with_declared_targets(
        supplied_document_paths(),
        targets.files.iter().cloned(),
        targets.directories.iter().cloned(),
        !ctx.args.stdin_batch_closed_world,
    );
    // A path the caller declared (a listed file or directory, or a directory
    // a listed path implies) that is not a batch document has no content known
    // to rumdl. MD057 resolved the link to it, so MD051 must not substitute a
    // disk file for it: its headings are unknown, not read from disk.
    let batch_keys: HashSet<PathBuf> = documents
        .iter()
        .map(|document| workspace_key(Path::new(&document.path)))
        .collect();
    let is_declared_only = |path: &Path| {
        !batch_keys.contains(&workspace_key(path)) && link_target_policy.contains_with_markdown_extension(path)
    };

    // Validate inline configuration in input order so notices remain stable,
    // independent of the parallel lint pass below.
    for document in &linted {
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
        // A Rust file's inline configuration is read per doc comment, never
        // from the source as a whole, which can hold a lookalike in a string
        // literal. The file path does not validate it either.
        if is_rust_source(Path::new(&document.path)) {
            continue;
        }
        let group = &resolved.groups[group_index];
        let config_path = rumdl_lib::discovery::resolve_for_matching(Path::new(&document.path));
        let flavor = group.config.get_flavor_for_file(&config_path);
        let ignored_for_file = group.config.get_ignored_rules_for_file(&config_path);
        let mut inline_warnings = rumdl_lib::inline_config::validate_inline_config_rules(&document.content, flavor);
        let active_rules: HashSet<String> = group
            .rule_sets
            .document
            .iter()
            .map(|rule| rule.name().to_string())
            .collect();
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
    let analysis_results: Vec<Result<AnalyzedDocument, String>> = linted
        .par_iter()
        .map(|document| {
            let group_index = groups_by_path[document.path.as_str()];
            let group = &resolved.groups[group_index];
            let path = Path::new(&document.path);
            let config_path = rumdl_lib::discovery::resolve_for_matching(path);
            let rules = rumdl_lib::rules::filter_rules_for_file(&group.rule_sets.document, &group.config, &config_path);
            let invalid_utf8 = match &document.encoding {
                SuppliedEncoding::Lossy(invalid) => Some(invalid.as_slice()),
                _ => None,
            };
            let run = rumdl_lib::document_run::DocumentRun::new(&document.content, &rules, &group.config)
                .verbose(ctx.args.verbose)
                .config_path(Some(&config_path))
                .source_file(Some(path))
                .link_target_policy(&link_target_policy)
                .invalid_utf8(invalid_utf8);
            // A Rust file is read through its doc comments, as `rumdl check
            // lib.rs` reads it, and like that file it must be valid UTF-8: a
            // lossy rewrite of source code is not a document to lint.
            let (result, file_index) = if is_rust_source(path) {
                if document.encoding != SuppliedEncoding::Utf8 {
                    return Err(format!("{} is not valid UTF-8", document.path));
                }
                (
                    Ok(rumdl_lib::doc_comment_lint::check_doc_comment_blocks(
                        &document.content,
                        &rules,
                        &group.config,
                    )),
                    rumdl_lib::workspace_index::FileIndex::default(),
                )
            } else if let SuppliedEncoding::Binary { utf16 } = document.encoding {
                let binary =
                    rumdl_lib::encoding::detect_binary_for_rules(utf16, &rules, &group.config, Some(&config_path));
                (
                    Ok(binary.into_iter().collect()),
                    rumdl_lib::workspace_index::FileIndex::default(),
                )
            } else if let Some(conflict) = rumdl_lib::merge_conflict::detect_for_rules(
                &document.content,
                &rules,
                &group.config,
                Some(&config_path),
            ) {
                (Ok(vec![conflict]), rumdl_lib::workspace_index::FileIndex::default())
            } else {
                run.analyze_raw()
            };
            let warnings = result.map_err(|error| error.to_string())?;
            let display_path =
                crate::file_processor::resolve_display_path(&document.path, ctx.args.show_full_path, ctx.project_root);
            Ok(AnalyzedDocument {
                group_index,
                normalized_path: workspace_key(path),
                config_path,
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
    // Whether `candidate` is a Markdown file a workspace run would read. Loaded
    // on first use: most batches link only among the documents they supply.
    let mut is_scanned = |candidate: &Path| {
        let Some(canonical) = rumdl_lib::discovery::canonicalize_for_matching(candidate) else {
            return false;
        };
        scanned_files
            .get_or_insert_with(|| {
                crate::file_processor::find_markdown_files(&[], ctx.args, ctx.config, ctx.project_root)
                    .map(|discovered| {
                        discovered
                            .files
                            .iter()
                            .filter_map(|path| rumdl_lib::discovery::canonicalize_for_matching(Path::new(path)))
                            .collect()
                    })
                    .unwrap_or_default()
            })
            .contains(&canonical)
    };
    let mut resolvers: HashMap<usize, LinkResolver> = HashMap::new();
    for document in &analyzed {
        if ctx.args.stdin_batch_closed_world {
            break;
        }
        // Each link is followed to the file MD057 resolves it to, the file
        // MD051 then checks its fragment in.
        let resolver = resolvers
            .entry(document.group_index)
            .or_insert_with(|| LinkResolver::from_config(&resolved.groups[document.group_index].config));
        let document_links = resolver.for_document(&document.normalized_path);
        for link in &document.file_index.cross_file_links {
            if link.fragment.is_empty() {
                continue;
            }
            match document_links.resolve(&link.target_path, Some(&link_target_policy)) {
                LinkResolution::Target(target) => {
                    let key = workspace_key(&target);
                    if supplied_paths.contains(&key) || is_declared_only(&target) || !attempted.insert(target.clone()) {
                        continue;
                    }
                    if is_scanned(&target) {
                        disk_targets.push(target);
                    }
                }
                LinkResolution::Missing => {}
                LinkResolution::Unchecked => {
                    // A link the supplied set resolves to a declared target
                    // names content the run does not have, as MD051 reads it.
                    let candidates = link_target_candidates(&document.normalized_path, &link.target_path);
                    if candidates
                        .first()
                        .is_some_and(|base| link_target_policy.declared_target_for(base).is_some())
                    {
                        continue;
                    }
                    for candidate in candidates {
                        if supplied_paths.contains(&candidate) {
                            break;
                        }
                        if !attempted.insert(candidate.clone()) {
                            continue;
                        }
                        if is_scanned(&candidate) {
                            disk_targets.push(candidate);
                            break;
                        }
                    }
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
                let Ok(Some(content)) = rumdl_lib::encoding::read_markdown_lossy(&path) else {
                    continue;
                };
                let flavor = group.config.get_flavor_for_file(&path);
                let file_index = rumdl_lib::build_file_index_only_for_selection(
                    &content,
                    &group.rule_sets.document,
                    flavor,
                    Some(path.clone()),
                    &group.config,
                );
                workspace_index.insert_file(workspace_key(&path), file_index);
            }
        }
    }

    workspace_index.set_link_target_policy(link_target_policy.clone());

    // Every supplied document is indexed before cross-file checks begin. This
    // makes references resolve against the batch snapshot, including content
    // that differs from the file currently saved at the same path.
    for document in &mut analyzed {
        let group = &resolved.groups[document.group_index];
        let rules =
            rumdl_lib::rules::filter_rules_for_file(&group.rule_sets.document, &group.config, &document.config_path);
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

    for (document, analyzed_document) in linted.iter().zip(&analyzed) {
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
        let document_rules = crate::file_processor::rules_reconfigured_by_document(
            &group.rule_sets.document,
            &group.config,
            &document.content,
        );
        // Doc-comment findings carry no fix of their own (the fix pass rewrites
        // the comments), so a Rust file counts what its rules can fix.
        let rust_source = is_rust_source(Path::new(&document.path));
        total_fixable_issues += analyzed_document
            .warnings
            .iter()
            .filter(|warning| {
                (warning.fix.is_some() || rust_source)
                    && crate::file_processor::is_rule_cli_fixable_in(
                        &group.rule_sets.document,
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
        let mut warnings = analyzed_document.warnings.clone();
        if matches!(output_format, OutputFormat::Json) {
            rumdl_lib::output::formatters::json::remap_fix_ranges_to_original(&mut warnings, &document.line_endings);
        }
        batch_file_warnings.push((analyzed_document.display_path.clone(), warnings));
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
            total_fixable_issues,
            total_files_processed: linted.len(),
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
        files_changed: 0,
        had_tool_error: false,
        config_warning,
        reads_editorconfig: resolved.groups.iter().any(|group| group.config.global.editorconfig),
    }
}

#[cfg(test)]
mod tests {
    use super::{SuppliedEncoding, parse_documents, parse_targets};
    use std::path::PathBuf;

    #[test]
    fn targets_split_files_from_directories_and_collapse_duplicates() {
        let targets = parse_targets(b"a/b.pdf\0./a/b.pdf\0docs/\0docs//\0docs\0/\0").unwrap();
        assert_eq!(
            targets.files,
            vec![PathBuf::from("a/b.pdf"), PathBuf::from("docs")],
            "`docs` and `docs/` are two facts, so the file stays"
        );
        assert_eq!(targets.directories, vec![PathBuf::from("docs"), PathBuf::from("/")]);
    }

    #[test]
    fn targets_keep_root_separators_and_ignore_trailing_ones() {
        let targets = parse_targets(b"/\0dir/\0dir//\0").unwrap();
        assert!(targets.files.is_empty());
        assert_eq!(targets.directories.len(), 2, "`dir/` and `dir//` are one directory");
        assert_eq!(targets.directories[0], PathBuf::from("/"));
        assert_eq!(
            rumdl_lib::workspace_index::normalize_relative_path(&targets.directories[1]),
            PathBuf::from("dir")
        );
    }

    #[cfg(windows)]
    #[test]
    fn targets_keep_a_windows_drive_root() {
        for entry in [&b"C:/\0"[..], &b"C:\\\0"[..]] {
            let targets = parse_targets(entry).unwrap();
            assert_eq!(targets.directories.len(), 1);
            assert!(
                targets.directories[0].has_root(),
                "{:?} must stay absolute, not drive-relative `C:`",
                targets.directories[0]
            );
        }
    }

    #[test]
    fn targets_accept_empty_input_and_reject_malformed_lists() {
        assert_eq!(parse_targets(b"").unwrap(), super::DeclaredTargets::default());
        assert!(parse_targets(b"a.pdf").unwrap_err().contains("NUL"));
        assert!(parse_targets(b"\0").unwrap_err().contains("empty"));
        assert!(parse_targets(b"a\0\0b\0").unwrap_err().contains("empty"));
        assert!(parse_targets(b"\xff\0").unwrap_err().contains("UTF-8"));
    }

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
    fn parser_decodes_non_utf8_content_lossily() {
        let documents = parse_documents(b"a.md\0caf\xe9\r\n\0b.md\0\xff\xfe#\0").unwrap();
        assert_eq!(documents[0].content, "caf\u{FFFD}\n");
        let SuppliedEncoding::Lossy(invalid) = &documents[0].encoding else {
            panic!("expected lossy decoding: {:?}", documents[0]);
        };
        assert_eq!(invalid[0].bytes, vec![0xE9]);
        assert_eq!(documents[1].encoding, SuppliedEncoding::Binary { utf16: true });
        assert_eq!(documents[1].content, "");
    }
}
