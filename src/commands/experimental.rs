//! Thin CLI adapters for the experimental custom-checker interchange.
use clap::Subcommand;
use rumdl_lib::custom_rules::{self, MAX_RESPONSE_BYTES, MAX_SOURCE_BYTES};
use rumdl_lib::exit_codes::exit;
use std::io::{IsTerminal, Read};
use std::path::PathBuf;

#[derive(Debug, Subcommand)]
pub enum Action {
    /// Export raw code elements and their Markdown context as versioned JSON
    ExportElements {
        #[arg(required_unless_present = "stdin", conflicts_with = "stdin")]
        file: Option<PathBuf>,
        /// Read one UTF-8 document from stdin
        #[arg(long, requires = "stdin_filename")]
        stdin: bool,
        #[arg(long, requires = "stdin")]
        stdin_filename: Option<String>,
    },
    /// Validate diagnostic JSON from stdin against the exact source file
    ReportDiagnostics { file: PathBuf },
}

fn bounded(mut reader: impl Read, limit: usize) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    reader
        .by_ref()
        .take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > limit {
        return Err(format!("input exceeds {limit} bytes"));
    }
    Ok(bytes)
}

fn read_source(file: &std::path::Path) -> Result<String, String> {
    let input = std::fs::File::open(file).map_err(|e| format!("Cannot read {}: {e}", file.display()))?;
    String::from_utf8(bounded(input, MAX_SOURCE_BYTES).map_err(|e| format!("Cannot read {}: {e}", file.display()))?)
        .map_err(|e| format!("Source is not UTF-8: {e}"))
}

fn run(action: Action) -> Result<bool, String> {
    match action {
        Action::ExportElements {
            file,
            stdin,
            stdin_filename,
        } => {
            let (path, source) = if stdin {
                (
                    stdin_filename.unwrap(),
                    String::from_utf8(bounded(std::io::stdin().lock(), MAX_SOURCE_BYTES)?)
                        .map_err(|e| format!("Source is not UTF-8: {e}"))?,
                )
            } else {
                let file = file.unwrap();
                (
                    file.to_str().ok_or("Source path must be UTF-8")?.to_owned(),
                    read_source(&file)?,
                )
            };
            let document = custom_rules::export(&path, &source)?;
            println!("{}", serde_json::to_string(&document).map_err(|e| e.to_string())?);
            Ok(false)
        }
        Action::ReportDiagnostics { file } => {
            if std::io::stdin().is_terminal() {
                return Err("expected diagnostic JSON on stdin".into());
            }
            let path = file.to_str().ok_or("Source path must be UTF-8")?;
            let source = read_source(&file)?;
            let response = bounded(std::io::stdin().lock(), MAX_RESPONSE_BYTES)?;
            let warnings = custom_rules::validate(&source, &response)?;
            let formatted = rumdl_lib::output::OutputFormat::Concise
                .create_formatter()
                .format_warnings(&warnings, path);
            if !formatted.is_empty() {
                println!("{formatted}");
            }
            Ok(!warnings.is_empty())
        }
    }
}

pub fn handle(action: Action) {
    match run(action) {
        Ok(false) => {}
        Ok(true) => exit::violations_found(),
        Err(e) => {
            eprintln!("error: custom-rule protocol: {e}");
            exit::tool_error();
        }
    }
}
