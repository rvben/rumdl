//! Handler for the `clean` command.

use colored::*;
use std::fs;
use std::io;
use std::path::Path;

use rumdl_lib::config as rumdl_config;
use rumdl_lib::exit_codes::exit;

/// Handle the clean command: clear the lint cache.
pub fn handle_clean(config_path: Option<&str>, no_config: bool, isolated: bool) {
    let cache_dir = resolve_cache_directory(config_path, no_config, isolated);

    // Check if cache directory exists
    if !cache_dir.exists() {
        println!(
            "{} {} ({})",
            "No cache found at".yellow().bold(),
            cache_dir.display(),
            "nothing to clean".dimmed()
        );
        return;
    }

    // Calculate cache stats before deletion
    match calculate_directory_stats(&cache_dir) {
        Ok((size, file_count)) => {
            if size == 0 && file_count == 0 {
                println!(
                    "{} {} ({})",
                    "Cache is empty at".yellow().bold(),
                    cache_dir.display(),
                    "nothing to clean".dimmed()
                );
                // Still remove the directory structure
                let cache_instance = crate::cache::LintCache::new(cache_dir.clone(), true);
                let _ = cache_instance.clear();
                return;
            }

            // Create cache instance and clear
            let cache_instance = crate::cache::LintCache::new(cache_dir.clone(), true);

            match cache_instance.clear() {
                Ok(()) => {
                    println!("{} {}", "Cleared cache:".green().bold(), cache_dir.display());
                    println!(
                        "  {} {} {} {}",
                        "Removed".dimmed(),
                        format_size(size).cyan(),
                        "across".dimmed(),
                        format!("{file_count} files").cyan()
                    );
                }
                Err(e) => {
                    eprintln!("{}: {}", "Error clearing cache".red().bold(), e);
                    eprintln!("  Cache location: {}", cache_dir.display());
                    exit::tool_error();
                }
            }
        }
        Err(e) => {
            eprintln!("{}: {}", "Error reading cache directory".red().bold(), e);
            eprintln!("  Cache location: {}", cache_dir.display());
            exit::tool_error();
        }
    }
}

/// Resolve cache directory with same logic as check command
fn resolve_cache_directory(config_path: Option<&str>, no_config: bool, isolated: bool) -> std::path::PathBuf {
    // Load config to get cache_dir setting
    let sourced = match rumdl_config::SourcedConfig::load_with_discovery(config_path, None, no_config || isolated) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("{}: {}", "Config error".red().bold(), e);
            exit::tool_error();
        }
    };

    // Get cache_dir from config
    let cache_dir_from_config = sourced
        .global
        .cache_dir
        .as_ref()
        .map(|sv| std::path::PathBuf::from(&sv.value));

    let project_root = sourced.project_root.clone();

    // Resolve cache directory with precedence: env var -> config -> default
    let mut cache_dir = std::env::var("RUMDL_CACHE_DIR")
        .ok()
        .map(std::path::PathBuf::from)
        .or(cache_dir_from_config)
        .unwrap_or_else(|| std::path::PathBuf::from(".rumdl_cache"));

    // If cache_dir is relative and we have a project root, resolve relative to project root
    if cache_dir.is_relative()
        && let Some(root) = project_root
    {
        cache_dir = root.join(&cache_dir);
    }

    cache_dir
}

/// Calculate total size and count of files in a directory recursively
fn calculate_directory_stats(path: &Path) -> io::Result<(u64, usize)> {
    let mut total_size = 0u64;
    let mut file_count = 0usize;

    fn visit_dir(path: &Path, total_size: &mut u64, file_count: &mut usize) -> io::Result<()> {
        if path.is_dir() {
            for entry in fs::read_dir(path)? {
                let entry = entry?;
                let path = entry.path();
                if path.is_dir() {
                    visit_dir(&path, total_size, file_count)?;
                } else if let Ok(metadata) = entry.metadata() {
                    *total_size += metadata.len();
                    *file_count += 1;
                }
            }
        }
        Ok(())
    }

    visit_dir(path, &mut total_size, &mut file_count)?;
    Ok((total_size, file_count))
}

/// Format bytes into human-readable size
fn format_size(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];
    let mut size = bytes as f64;
    let mut unit_index = 0;

    while size >= 1024.0 && unit_index < UNITS.len() - 1 {
        size /= 1024.0;
        unit_index += 1;
    }

    if unit_index == 0 {
        format!("{} {}", bytes, UNITS[0])
    } else {
        format!("{:.2} {}", size, UNITS[unit_index])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_calculate_directory_stats_empty() {
        let temp_dir = TempDir::new().unwrap();
        let (size, count) = calculate_directory_stats(temp_dir.path()).unwrap();
        assert_eq!(size, 0);
        assert_eq!(count, 0);
    }

    #[test]
    fn test_calculate_directory_stats_with_files() {
        let temp_dir = TempDir::new().unwrap();

        // Create some test files
        fs::write(temp_dir.path().join("file1.txt"), "hello").unwrap();
        fs::write(temp_dir.path().join("file2.txt"), "world!").unwrap();

        let (size, count) = calculate_directory_stats(temp_dir.path()).unwrap();
        assert_eq!(size, 11); // "hello" (5) + "world!" (6)
        assert_eq!(count, 2);
    }

    #[test]
    fn test_calculate_directory_stats_nested() {
        let temp_dir = TempDir::new().unwrap();

        // Create nested directories
        let nested = temp_dir.path().join("nested");
        fs::create_dir(&nested).unwrap();

        fs::write(temp_dir.path().join("file1.txt"), "abc").unwrap();
        fs::write(nested.join("file2.txt"), "defgh").unwrap();

        let (size, count) = calculate_directory_stats(temp_dir.path()).unwrap();
        assert_eq!(size, 8); // "abc" (3) + "defgh" (5)
        assert_eq!(count, 2);
    }

    #[test]
    fn test_calculate_directory_stats_deeply_nested() {
        let temp_dir = TempDir::new().unwrap();

        // Create deeply nested structure
        let level1 = temp_dir.path().join("level1");
        let level2 = level1.join("level2");
        let level3 = level2.join("level3");
        fs::create_dir_all(&level3).unwrap();

        fs::write(temp_dir.path().join("root.txt"), "1").unwrap();
        fs::write(level1.join("l1.txt"), "12").unwrap();
        fs::write(level2.join("l2.txt"), "123").unwrap();
        fs::write(level3.join("l3.txt"), "1234").unwrap();

        let (size, count) = calculate_directory_stats(temp_dir.path()).unwrap();
        assert_eq!(size, 10); // 1 + 2 + 3 + 4
        assert_eq!(count, 4);
    }

    #[test]
    fn test_format_size_bytes() {
        assert_eq!(format_size(0), "0 B");
        assert_eq!(format_size(1), "1 B");
        assert_eq!(format_size(42), "42 B");
        assert_eq!(format_size(1023), "1023 B");
    }

    #[test]
    fn test_format_size_kilobytes() {
        assert_eq!(format_size(1024), "1.00 KB");
        assert_eq!(format_size(1536), "1.50 KB");
        assert_eq!(format_size(2048), "2.00 KB");
        assert_eq!(format_size(1024 * 10), "10.00 KB");
    }

    #[test]
    fn test_format_size_megabytes() {
        assert_eq!(format_size(1024 * 1024), "1.00 MB");
        assert_eq!(format_size(1024 * 1024 + 512 * 1024), "1.50 MB");
        assert_eq!(format_size(1024 * 1024 * 5), "5.00 MB");
    }

    #[test]
    fn test_format_size_gigabytes() {
        assert_eq!(format_size(1024 * 1024 * 1024), "1.00 GB");
        assert_eq!(format_size(1024u64 * 1024 * 1024 * 2 + 512 * 1024 * 1024), "2.50 GB");
    }

    #[test]
    fn test_format_size_terabytes() {
        assert_eq!(format_size(1024u64 * 1024 * 1024 * 1024), "1.00 TB");
        assert_eq!(format_size(1024u64 * 1024 * 1024 * 1024 * 3), "3.00 TB");
    }

    #[test]
    fn test_format_size_edge_cases() {
        // Just under next unit
        assert_eq!(format_size(1023), "1023 B");
        assert_eq!(format_size(1024 * 1024 - 1), "1024.00 KB");

        // Exact boundaries
        assert_eq!(format_size(1024), "1.00 KB");
        assert_eq!(format_size(1024 * 1024), "1.00 MB");
    }

    #[test]
    fn test_format_size_realistic_cache_sizes() {
        // Small cache
        assert_eq!(format_size(458), "458 B");

        // Medium cache
        assert_eq!(format_size(156_234), "152.57 KB");

        // Large cache (like the Ruff issue)
        assert_eq!(format_size(1_500_000_000), "1.40 GB");
    }
}
