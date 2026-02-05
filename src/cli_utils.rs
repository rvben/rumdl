//! Shared CLI utility functions used across command handlers and watch mode.

use colored::*;
use core::error::Error;
use memmap2::Mmap;
use std::fs;
use std::path::Path;
use std::str::FromStr;

use rumdl_lib::config as rumdl_config;
use rumdl_lib::exit_codes::exit;

use crate::CheckArgs;

/// Threshold for using memory-mapped I/O (1MB)
pub const MMAP_THRESHOLD: u64 = 1024 * 1024;

/// Apply CLI argument overrides to a sourced config.
/// This centralizes the logic for CLI args overriding config values,
/// ensuring consistency between regular check and watch mode.
pub fn apply_cli_overrides(sourced: &mut rumdl_config::SourcedConfig, args: &CheckArgs) {
    // Apply --flavor override if provided
    if let Some(ref flavor_str) = args.flavor
        && let Ok(flavor) = rumdl_config::MarkdownFlavor::from_str(flavor_str)
    {
        sourced.global.flavor = rumdl_config::SourcedValue::new(flavor, rumdl_config::ConfigSource::Cli);
    }

    // Apply --respect-gitignore override if provided
    // This allows CLI to override config file setting
    if let Some(respect_gitignore) = args.respect_gitignore {
        sourced.global.respect_gitignore =
            rumdl_config::SourcedValue::new(respect_gitignore, rumdl_config::ConfigSource::Cli);
    }
}

/// Efficiently read file content using memory mapping for large files
pub fn read_file_efficiently(path: &Path) -> Result<String, Box<dyn Error>> {
    // Get file metadata first
    let metadata = fs::metadata(path)?;
    let file_size = metadata.len();

    if file_size > MMAP_THRESHOLD {
        // Use memory mapping for large files
        let file = fs::File::open(path)?;
        let mmap = unsafe { Mmap::map(&file)? };

        // Convert to string - this is still a copy but more efficient for large files
        String::from_utf8(mmap.to_vec()).map_err(|e| format!("Invalid UTF-8 in file {}: {}", path.display(), e).into())
    } else {
        // Use regular reading for small files
        fs::read_to_string(path).map_err(|e| format!("Failed to read file {}: {}", path.display(), e).into())
    }
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
