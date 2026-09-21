//! Shared CLI utility functions used across command handlers and watch mode.

use colored::*;
use core::error::Error;
use std::fs::{self, File};
use std::io::{BufReader, Read};
use std::path::Path;

use rumdl_lib::config as rumdl_config;
use rumdl_lib::exit_codes::exit;

use crate::{CheckArgs, CodeBlockToolsMode};

/// Apply CLI argument overrides to a sourced config.
/// This centralizes the logic for CLI args overriding config values,
/// ensuring consistency between regular check and watch mode.
pub fn apply_cli_overrides(sourced: &mut rumdl_config::SourcedConfig, args: &CheckArgs) {
    // Apply --flavor override if provided
    if let Some(flavor) = args.flavor {
        sourced.global.flavor = rumdl_config::SourcedValue::new(flavor.into(), rumdl_config::ConfigSource::Cli);
    }

    // Apply --respect-gitignore override if provided
    // This allows CLI to override config file setting
    if let Some(respect_gitignore) = args.respect_gitignore {
        sourced.global.respect_gitignore =
            rumdl_config::SourcedValue::new(respect_gitignore, rumdl_config::ConfigSource::Cli);
    }

    // Apply --fixable override if provided
    if let Some(ref fixable) = args.fixable {
        let rules: Vec<String> = fixable
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        sourced.global.fixable = rumdl_config::SourcedValue::new(rules, rumdl_config::ConfigSource::Cli);
    }

    // Apply --unfixable override if provided
    if let Some(ref unfixable) = args.unfixable {
        let rules: Vec<String> = unfixable
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        sourced.global.unfixable = rumdl_config::SourcedValue::new(rules, rumdl_config::ConfigSource::Cli);
    }
}

/// Apply invocation-level overrides to an effective runtime configuration.
///
/// This must run after every conversion from `SourcedConfig` to `Config`,
/// including subdirectory, `.editorconfig`, and watch-mode reload paths. Keeping
/// the code-block-tools mode here makes it a final execution overlay without
/// discarding any loaded tool definitions or unrelated settings.
pub fn apply_runtime_cli_overrides(config: &mut rumdl_config::Config, args: &CheckArgs) {
    if let Some(flavor) = args.flavor {
        config.global.flavor = flavor.into();
    }

    if let Some(respect_gitignore) = args.respect_gitignore {
        config.global.respect_gitignore = respect_gitignore;
    }

    match args.code_block_tools_mode() {
        CodeBlockToolsMode::Configured => {}
        CodeBlockToolsMode::Disabled => config.code_block_tools.enabled = false,
        CodeBlockToolsMode::Only => config.code_block_tools.enabled = true,
    }
}

/// Resolve the lint output format with the standard precedence:
/// CLI `--output-format` → `RUMDL_OUTPUT_FORMAT` env var → config
/// `output_format` → legacy `--output json` → `text`.
///
/// Returns the parse error message for an unrecognized format name so each
/// caller can report it through its own channel.
pub fn resolve_output_format(
    args: &CheckArgs,
    config: &rumdl_config::Config,
) -> Result<rumdl_lib::output::OutputFormat, String> {
    use std::str::FromStr;

    if let Some(fmt) = args.output_format {
        return Ok(fmt.into());
    }

    let env_output_format = std::env::var("RUMDL_OUTPUT_FORMAT").ok();
    let output_format_str = env_output_format
        .as_deref()
        .or(config.global.output_format.as_deref())
        .or({
            // Legacy support: map --output json to --output-format json
            match args.output {
                crate::cli_types::Output::Json => Some("json"),
                crate::cli_types::Output::Text => None,
            }
        })
        .unwrap_or("text");

    rumdl_lib::output::OutputFormat::from_str(output_format_str).map_err(|e| e.to_string())
}

/// Read file content as a UTF-8 string.
pub fn read_file_efficiently(path: &Path) -> Result<String, Box<dyn Error>> {
    fs::read_to_string(path).map_err(|e| format!("Failed to read file {}: {}", path.display(), e).into())
}

/// Content read from a file after invalid UTF-8 sequences have been replaced.
pub struct DecodedFileContent {
    pub content: String,
    /// One-based `(line, column)` positions where invalid UTF-8 sequences began.
    pub invalid_positions: Vec<(usize, usize)>,
    /// Percentage of the file's bytes that were invalid UTF-8.
    pub invalid_byte_ratio: f64,
    /// Whether the file exceeded the configured invalid-byte threshold and
    /// should be excluded from linting.
    pub skip_linting: bool,
}

/// Read a file as UTF-8 while tolerating invalid byte sequences.
///
/// Invalid sequences are replaced with `U+FFFD` and their one-based source
/// positions are recorded. Reading stops early when invalid bytes exceed
/// `threshold_percent` of the file's total byte size; in that case the
/// returned content is partial and `skip_linting` is set.
pub fn read_non_utf8_file_content(path: &Path, threshold_percent: f64) -> Result<DecodedFileContent, Box<dyn Error>> {
    let file_size = fs::metadata(path)?.len() as usize;
    let file = File::open(path)?;
    let mut reader = BufReader::new(file);
    let mut content = String::with_capacity(file_size);
    let mut invalid_positions = Vec::new();
    let mut line = 1;
    let mut column = 1;
    let mut invalid_bytes = 0;
    let mut pending = Vec::new();
    let mut buffer = [0; 8192];
    let mut eof = false;

    loop {
        if !eof {
            let read = reader.read(&mut buffer)?;
            if read == 0 {
                eof = true;
            } else {
                pending.extend_from_slice(&buffer[..read]);
            }
        }

        loop {
            match std::str::from_utf8(&pending) {
                Ok(valid) => {
                    content.push_str(valid);
                    pending.clear();
                    break;
                }
                Err(error) => {
                    let valid_length = error.valid_up_to();
                    let valid = std::str::from_utf8(&pending[..valid_length]).expect("valid UTF-8 prefix");
                    content.push_str(valid);
                    for character in valid.chars() {
                        if character == '\n' {
                            line += 1;
                            column = 1;
                        } else {
                            column += 1;
                        }
                    }
                    pending.drain(..valid_length);

                    let invalid_length = match error.error_len() {
                        Some(length) => length,
                        None => {
                            if !eof {
                                break;
                            }
                            1
                        }
                    };

                    invalid_bytes += invalid_length;
                    invalid_positions.push((line, column));
                    let invalid_byte_percent = (invalid_bytes as f64 / file_size.max(1) as f64) * 100.0;
                    if invalid_byte_percent > threshold_percent {
                        return Ok(DecodedFileContent {
                            content,
                            invalid_positions,
                            invalid_byte_ratio: invalid_byte_percent,
                            skip_linting: true,
                        });
                    }
                    content.push('\u{FFFD}');
                    column += 1;
                    pending.drain(..invalid_length);
                }
            }
        }

        if eof {
            break;
        }
    }

    Ok(DecodedFileContent {
        content,
        invalid_positions,
        invalid_byte_ratio: (invalid_bytes as f64 / file_size.max(1) as f64) * 100.0,
        skip_linting: false,
    })
}

/// Load configuration with standard CLI error handling.
pub fn load_config_with_cli_error_handling(config_path: Option<&str>, isolated: bool) -> rumdl_config::SourcedConfig {
    load_config_with_cli_error_handling_with_dir(config_path, isolated, None)
}

/// Load configuration with standard CLI error handling, optionally using a discovery directory.
pub fn load_config_with_cli_error_handling_with_dir(
    config_path: Option<&str>,
    isolated: bool,
    discovery_dir: Option<&Path>,
) -> rumdl_config::SourcedConfig {
    let result = if let Some(dir) = discovery_dir {
        // Canonicalize config path before changing directory
        // Otherwise relative paths will be resolved from the wrong directory
        let absolute_config_path = config_path.map(|p| {
            let path = Path::new(p);
            if path.is_absolute() {
                p.to_string()
            } else if let Ok(canonical) = std::fs::canonicalize(path) {
                canonical.to_string_lossy().to_string()
            } else {
                // If file doesn't exist yet, make it absolute relative to current dir
                std::env::current_dir()
                    .map(|cwd| cwd.join(p).to_string_lossy().to_string())
                    .unwrap_or_else(|_| p.to_string())
            }
        });

        // Temporarily change working directory for config discovery
        let original_dir = std::env::current_dir().ok();

        // Change to the discovery directory if it exists
        if dir.is_dir() {
            let _ = std::env::set_current_dir(dir);
        } else if let Some(parent) = dir.parent() {
            let _ = std::env::set_current_dir(parent);
        }

        let config_result =
            rumdl_config::SourcedConfig::load_with_discovery(absolute_config_path.as_deref(), None, isolated);

        // Restore original directory
        if let Some(orig) = original_dir {
            let _ = std::env::set_current_dir(orig);
        }

        config_result
    } else {
        rumdl_config::SourcedConfig::load_with_discovery(config_path, None, isolated)
    };

    match result {
        Ok(config) => config,
        Err(e) => {
            eprintln!("{}: {}", "Config error".red().bold(), e);
            exit::tool_error();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_non_utf8_file_content_reports_positions() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("invalid.md");
        let mut bytes = b"# title\ntext \xff and \xfe\n".to_vec();
        bytes.extend(std::iter::repeat_n(b'a', 100));
        fs::write(&path, bytes).unwrap();

        let decoded = read_non_utf8_file_content(&path, 4.0).unwrap();

        assert_eq!(decoded.invalid_positions, vec![(2, 6), (2, 12)]);
        assert!(!decoded.skip_linting);
        assert!(decoded.content.contains('\u{FFFD}'));
    }

    #[test]
    fn read_non_utf8_file_content_aborts_above_threshold() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("mostly-invalid.md");
        fs::write(&path, b"a\xff\xff\xff\xff\xff\xff\xff\xff\xff\xff").unwrap();

        let decoded = read_non_utf8_file_content(&path, 4.0).unwrap();

        assert!(decoded.skip_linting);
        assert!((decoded.invalid_byte_ratio - 9.0909).abs() < 0.001);
        assert!(decoded.content.len() < fs::metadata(&path).unwrap().len() as usize);
    }
}
