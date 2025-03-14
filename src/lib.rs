pub mod rule;
pub mod rules;
pub mod config;
pub mod init;

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

// Function to check if a file path should be excluded
/// Check if a file path should be excluded based on the exclude patterns
pub fn should_exclude(file_path: &str, exclude_patterns: &[String]) -> bool {
    use glob::Pattern;
    
    // Normalize path by removing leading ./ if present
    let normalized_path = if file_path.starts_with("./") {
        &file_path[2..]
    } else {
        file_path
    };
    
    for pattern in exclude_patterns {
        // Handle directory patterns (ending with /)
        if pattern.ends_with('/') {
            let dir_prefix = &pattern[..pattern.len() - 1];
            if normalized_path == dir_prefix || normalized_path.starts_with(&format!("{}/", dir_prefix)) {
                return true;
            }
            continue;
        }
        
        // Try to compile the glob pattern
        match Pattern::new(pattern) {
            Ok(glob_pattern) => {
                // First, check direct match against normalized path
                if glob_pattern.matches(normalized_path) {
                    return true;
                }
                
                // Check if it's a directory pattern without wildcards
                if !pattern.contains('*') && normalized_path.starts_with(&format!("{}/", pattern)) {
                    return true;
                }
                
                // For single-star patterns (no **), we need to handle directory structure
                if pattern.contains('*') && !pattern.contains("**") {
                    let pattern_parts: Vec<&str> = pattern.split('/').collect();
                    let path_parts: Vec<&str> = normalized_path.split('/').collect();
                    
                    // Only match if the pattern and path have the same number of segments
                    if pattern_parts.len() == path_parts.len() {
                        let mut all_match = true;
                        for (i, pattern_part) in pattern_parts.iter().enumerate() {
                            // Create a pattern for this segment and check if it matches
                            if let Ok(part_pattern) = Pattern::new(pattern_part) {
                                if !part_pattern.matches(path_parts[i]) {
                                    all_match = false;
                                    break;
                                }
                            } else if pattern_part != &path_parts[i] {
                                all_match = false;
                                break;
                            }
                        }
                        
                        if all_match {
                            return true;
                        }
                    }
                }
                
                // For double-star patterns, match at any depth
                // This is already handled by the direct glob_pattern.matches() check
            },
            Err(_) => {
                // If pattern is invalid, just check for direct substring match
                if normalized_path.contains(pattern) {
                    return true;
                }
            }
        }
    }
    
    false
} 