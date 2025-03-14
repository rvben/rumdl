pub mod rule;
pub mod rules;
pub mod config;
pub mod init;
pub mod profiling;

#[cfg(feature = "python")]
pub mod python;

// Re-export commonly used types
pub use rules::heading_utils::{HeadingStyle, Heading};
pub use rules::*;


/// Collect patterns from .gitignore files
/// 
/// This function reads the closest .gitignore file and returns a list of patterns
/// that can be used to exclude files from linting.
pub fn collect_gitignore_patterns(start_dir: &str) -> Vec<String> {
    use std::path::Path;
    use std::fs;
    
    let mut patterns = Vec::new();
    
    // Start from the given directory and look for .gitignore files
    // going up to parent directories
    let path = Path::new(start_dir);
    let mut current_dir = if path.is_file() {
        path.parent().unwrap_or(Path::new(".")).to_path_buf()
    } else {
        path.to_path_buf()
    };
    
    // Track visited directories to avoid duplicates
    let mut visited_dirs = std::collections::HashSet::new();
    
    while visited_dirs.insert(current_dir.clone()) {
        let gitignore_path = current_dir.join(".gitignore");
        
        if gitignore_path.exists() && gitignore_path.is_file() {
            // Read the .gitignore file and process each pattern
            if let Ok(content) = fs::read_to_string(&gitignore_path) {
                for line in content.lines() {
                    // Skip comments and empty lines
                    let trimmed = line.trim();
                    if !trimmed.is_empty() && !trimmed.starts_with('#') {
                        // Normalize pattern to fit our exclude format
                        let pattern = normalize_gitignore_pattern(trimmed);
                        if !pattern.is_empty() {
                            patterns.push(pattern);
                        }
                    }
                }
            }
        }
        
        // Check for global gitignore in .git/info/exclude
        let git_dir = current_dir.join(".git");
        if git_dir.exists() && git_dir.is_dir() {
            let exclude_path = git_dir.join("info/exclude");
            if exclude_path.exists() && exclude_path.is_file() {
                if let Ok(content) = fs::read_to_string(&exclude_path) {
                    for line in content.lines() {
                        // Skip comments and empty lines
                        let trimmed = line.trim();
                        if !trimmed.is_empty() && !trimmed.starts_with('#') {
                            // Normalize pattern to fit our exclude format
                            let pattern = normalize_gitignore_pattern(trimmed);
                            if !pattern.is_empty() {
                                patterns.push(pattern);
                            }
                        }
                    }
                }
            }
        }
        
        // Go up to parent directory
        match current_dir.parent() {
            Some(parent) => current_dir = parent.to_path_buf(),
            None => break,
        }
    }
    
    // Add some common patterns that are usually in .gitignore files
    // but might not be in the specific project's .gitignore
    let common_patterns = vec![
        "node_modules",
        ".git",
        ".github",
        ".vscode",
        ".idea",
        "dist",
        "build",
        "target",
    ];
    
    for pattern in common_patterns {
        if !patterns.iter().any(|p| p == pattern) {
            patterns.push(pattern.to_string());
        }
    }
    
    patterns
}

/// Normalize a gitignore pattern to fit our exclude format
/// 
/// This function converts gitignore-style patterns to glob patterns
/// that can be used with the `should_exclude` function.
fn normalize_gitignore_pattern(pattern: &str) -> String {
    let mut normalized = pattern.trim().to_string();
    
    // Remove leading slash (gitignore uses it for absolute paths)
    if normalized.starts_with('/') {
        normalized = normalized[1..].to_string();
    }
    
    // Remove trailing slash (used in gitignore to specify directories)
    if normalized.ends_with('/') && normalized.len() > 1 {
        normalized = normalized[..normalized.len() - 1].to_string();
    }
    
    // Handle negated patterns (we don't support them currently)
    if normalized.starts_with('!') {
        return String::new();
    }
    
    // Convert ** pattern
    if normalized.contains("**") {
        return normalized;
    }
    
    // Add trailing / for directories
    if !normalized.contains('/') && !normalized.contains('*') {
        // This could be either a file or directory name, treat it as both
        normalized
    } else {
        normalized
    }
}

/// Should exclude a file based on patterns
///
/// This function checks if a file should be excluded based on a list of glob patterns.
pub fn should_exclude(file_path: &str, exclude_patterns: &[String]) -> bool {
    // Normalize the file path by removing leading ./ if present
    let normalized_path = if file_path.starts_with("./") {
        &file_path[2..]
    } else {
        file_path
    };

    for pattern in exclude_patterns {
        // Normalize the pattern by removing leading ./ if present
        let normalized_pattern = if pattern.starts_with("./") {
            &pattern[2..]
        } else {
            pattern
        };

        // Handle directory patterns (ending with /)
        if normalized_pattern.ends_with('/') {
            if normalized_path.starts_with(normalized_pattern) {
                return true;
            }
            continue;
        }

        // Handle glob patterns
        match glob::Pattern::new(normalized_pattern) {
            Ok(glob) => {
                if glob.matches(normalized_path) {
                    return true;
                }
            },
            Err(_) => {
                // For invalid glob patterns, fall back to simple substring matching
                if normalized_path.contains(normalized_pattern) {
                    return true;
                }
            }
        }
    }
    
    false
}

/// Lint a Markdown file
pub fn lint(content: &str, rules: &[Box<dyn rule::Rule>]) -> rule::LintResult {
    let _timer = profiling::ScopedTimer::new("lint_total");
    
    let mut warnings = Vec::new();
    
    for rule in rules {
        let _rule_timer = profiling::ScopedTimer::new(&format!("rule:{}", rule.name()));
        
        match rule.check(content) {
            Ok(rule_warnings) => {
                warnings.extend(rule_warnings);
            }
            Err(e) => {
                eprintln!("Error checking rule {}: {}", rule.name(), e);
            }
        }
    }
    
    // Force profiling to be enabled in debug mode
    #[cfg(debug_assertions)]
    {
        if !warnings.is_empty() {
            eprintln!("Found {} warnings", warnings.len());
        }
    }
    
    Ok(warnings)
}

/// Get the profiling report
pub fn get_profiling_report() -> String {
    profiling::get_report()
}

/// Reset the profiling data
pub fn reset_profiling() {
    profiling::reset()
} 