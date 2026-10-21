//! The explicit whole-directory asset scan, independent of document lint config.

use clap::{Args, ValueEnum};
use rumdl_lib::exit_codes::exit;
use rumdl_lib::unused_assets::{AssetScanOptions, scan};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, Default, ValueEnum)]
pub enum AssetOutput {
    #[default]
    Text,
    Json,
}

#[derive(Debug, Args)]
pub struct CheckAssetsArgs {
    /// Site root: leading-slash URLs and output paths are relative to this directory
    #[arg(long, value_name = "DIR")]
    pub root: PathBuf,
    /// Complete source directories to scan recursively (repeatable; no ignore filters)
    #[arg(long, required = true, value_name = "DIR")]
    pub source: Vec<PathBuf>,
    /// Asset directories to inspect recursively (repeatable; must be within --root)
    #[arg(long, required = true, value_name = "DIR")]
    pub assets: Vec<PathBuf>,
    /// Only consider asset paths matching a root-relative glob (repeatable)
    #[arg(long, value_name = "GLOB")]
    pub include: Vec<String>,
    /// Keep asset paths matching a root-relative glob (repeatable)
    #[arg(long, value_name = "GLOB")]
    pub keep: Vec<String>,
    /// Report format: human-readable text or versioned JSON
    #[arg(long, value_enum, default_value_t)]
    pub output_format: AssetOutput,
}

pub fn handle(args: CheckAssetsArgs) {
    let report = match scan(&AssetScanOptions {
        root: args.root,
        sources: args.source,
        assets: args.assets,
        include: args.include,
        keep: args.keep,
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
