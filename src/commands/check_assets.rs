//! The explicit whole-directory asset scan, independent of document lint config.

use crate::cli_types::{CheckArgs, Flavor};
use crate::resolution::{ResolutionRoots, RootConfig, resolve_config_groups};
use clap::{Args, Parser, ValueEnum};
use rumdl_lib::config::MarkdownFlavor;
use rumdl_lib::exit_codes::exit;
use rumdl_lib::unused_assets::{AssetScanOptions, scan};
use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, Default, ValueEnum)]
pub enum AssetOutput {
    #[default]
    Text,
    Json,
}

#[derive(Debug, Args)]
pub struct CheckAssetsArgs {
    /// URL root: a leading-slash link such as /img/a.png resolves from this directory, and reported paths are relative to it
    #[arg(long, value_name = "DIR")]
    pub root: PathBuf,
    /// Directory of Markdown files to read, scanned recursively without ignore filters (repeatable)
    #[arg(long, required = true, value_name = "DIR")]
    pub source: Vec<PathBuf>,
    /// Directory of asset files to report on, scanned recursively and within --root (repeatable; may be a --source directory)
    #[arg(long, required = true, value_name = "DIR")]
    pub assets: Vec<PathBuf>,
    /// Only consider asset paths matching a root-relative glob (repeatable)
    #[arg(long, value_name = "GLOB")]
    pub include: Vec<String>,
    /// Keep asset paths matching a root-relative glob (repeatable)
    #[arg(long, value_name = "GLOB")]
    pub keep: Vec<String>,
    /// Markdown flavor used to read sources (default: from configuration, then the file extension)
    #[arg(
        long,
        value_enum,
        help = "Markdown flavor used to read sources: standard (also accepts gfm/github/commonmark), mkdocs, mdx, pandoc, quarto, obsidian, kramdown, azure_devops (also accepts azure/ado), myst (also accepts mystmd), hugo (also accepts goldmark), mdg (also accepts markdown_with_gherkin), or gh-aw (preview)"
    )]
    pub flavor: Option<Flavor>,
    /// Report format: human-readable text or versioned JSON
    #[arg(long, value_enum, default_value_t)]
    pub output_format: AssetOutput,
}

/// The defaults `rumdl check` starts from, so config resolution sees the same
/// argument shape. Only the flavor override and the quiet/no-exclude switches differ.
fn resolution_args(flavor: Option<Flavor>) -> CheckArgs {
    #[derive(Parser)]
    struct Wrapper {
        #[command(flatten)]
        args: CheckArgs,
    }
    let mut args = Wrapper::parse_from(["rumdl"]).args;
    args.flavor = flavor;
    // Every Markdown file in the scan is read: ignores never hide a reference.
    args.no_exclude = true;
    args.silent = true;
    args
}

/// Resolve each source's flavor exactly as `rumdl check` would for the same
/// files: discovered or explicit configuration (flavor and `per-file-flavor`),
/// nested directory configuration, `--flavor`, then the file extension.
fn resolve_flavors(
    sources: &[PathBuf],
    scan_dirs: &[PathBuf],
    flavor: Option<Flavor>,
    config_path: Option<&str>,
    isolated: bool,
    inline_overrides: &[toml::Table],
) -> io::Result<Vec<MarkdownFlavor>> {
    let args = resolution_args(flavor);
    let strings: Vec<String> = scan_dirs.iter().map(|d| d.to_string_lossy().into_owned()).collect();
    let discovery_dir: Option<PathBuf> = match scan_dirs {
        [single] if single.is_dir() => Some(single.clone()),
        [_] => None,
        _ => crate::commands::check::common_ancestor_dir(&strings),
    };
    let multi_path_root = if scan_dirs.len() > 1 {
        discovery_dir.clone()
    } else {
        None
    };

    let mut sourced =
        crate::cli_utils::load_config_with_cli_error_handling_with_dir(config_path, isolated, discovery_dir.as_deref());
    crate::cli_config_override::apply_inline_overrides(&mut sourced, inline_overrides);
    crate::cli_utils::apply_cli_overrides(&mut sourced, &args);
    let project_root = sourced.project_root.clone();
    let grouping_root =
        crate::commands::check::grouping_root(project_root.clone(), multi_path_root, discovery_dir.as_deref());
    let sourced = sourced.into_validated_unchecked();
    let mut config: rumdl_lib::config::Config = sourced.clone().into();
    crate::cli_utils::apply_runtime_cli_overrides(&mut config, &args);

    let files: Vec<String> = sources.iter().map(|p| p.to_string_lossy().into_owned()).collect();
    let resolved = resolve_config_groups(
        &files,
        &RootConfig {
            config: &config,
            sourced: &sourced,
        },
        &args,
        &ResolutionRoots {
            grouping_root: grouping_root.as_deref(),
            project_root: project_root.as_deref(),
        },
        inline_overrides,
        &None,
        config_path.is_some() || isolated,
    );
    // A nested configuration that cannot be loaded would silently change the
    // flavor of the files it governs, so the scan fails instead.
    if let Some((path, reason)) = resolved.failed_configs.first() {
        return Err(io::Error::other(format!(
            "Cannot load configuration {}: {reason}",
            path.display()
        )));
    }
    let mut by_file = HashMap::new();
    for group in &resolved.groups {
        for file in &group.files {
            by_file.insert(file.clone(), group.config.get_flavor_for_file(Path::new(file)));
        }
    }
    files
        .iter()
        .map(|file| {
            by_file
                .get(file)
                .copied()
                .ok_or_else(|| io::Error::other(format!("No configuration resolved for source {file}")))
        })
        .collect()
}

pub fn handle(args: CheckAssetsArgs, config_path: Option<&str>, isolated: bool, inline_overrides: &[toml::Table]) {
    let flavors = |sources: &[PathBuf]| {
        resolve_flavors(
            sources,
            &args.source,
            args.flavor,
            config_path,
            isolated,
            inline_overrides,
        )
    };
    let report = match scan(&AssetScanOptions {
        root: args.root.clone(),
        sources: args.source.clone(),
        assets: args.assets.clone(),
        include: args.include.clone(),
        keep: args.keep.clone(),
        flavors: &flavors,
    }) {
        Ok(report) => report,
        Err(e) => {
            eprintln!("error: asset scan incomplete: {e}");
            exit::tool_error();
        }
    };
    match args.output_format {
        AssetOutput::Json => println!(
            "{}",
            serde_json::to_string_pretty(&report).expect("serializable asset report")
        ),
        AssetOutput::Text => {
            for path in &report.unreferenced {
                println!("{path}: unreferenced in scanned Markdown sources");
            }
            for path in &report.mentioned_but_unresolved {
                println!("{path}: mentioned but unresolved (kept)");
            }
            println!(
                "Scanned {} sources ({}), {} assets; {} unreferenced, {} mentioned but unresolved.",
                report.sources_scanned,
                report.source_extensions.join(", "),
                report.assets_considered,
                report.unreferenced.len(),
                report.mentioned_but_unresolved.len()
            );
        }
    }
    if !report.unreferenced.is_empty() {
        exit::violations_found();
    }
}
