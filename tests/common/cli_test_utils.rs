/// CLI testing utilities to reduce subprocess execution overhead
use rumdl_lib::config::Config;
use rumdl_lib::lint_context::LintContext;
use rumdl_lib::rule::Rule;
use rumdl_lib::rules;
use std::path::{Path, PathBuf};
use std::collections::HashMap;

/// In-memory CLI simulator that avoids subprocess overhead
pub struct MockCli {
    pub files: HashMap<PathBuf, String>,
    pub config: Option<Config>,
    pub current_dir: PathBuf,
    pub verbose: bool,
}

impl MockCli {
    pub fn new() -> Self {
        Self {
            files: HashMap::new(),
            config: None,
            current_dir: PathBuf::from("/mock"),
            verbose: false,
        }
    }

    /// Set current directory for mock operations
    pub fn current_dir<P: AsRef<Path>>(&mut self, dir: P) -> &mut Self {
        self.current_dir = dir.as_ref().to_path_buf();
        self
    }

    /// Add file to mock filesystem
    pub fn add_file<P: AsRef<Path>, S: Into<String>>(&mut self, path: P, content: S) -> &mut Self {
        self.files.insert(path.as_ref().to_path_buf(), content.into());
        self
    }

    /// Set verbose mode
    pub fn verbose(&mut self, verbose: bool) -> &mut Self {
        self.verbose = verbose;
        self
    }

    /// Set configuration
    pub fn with_config(&mut self, config: Config) -> &mut Self {
        self.config = Some(config);
        self
    }

    /// Simulate "rumdl check" command without subprocess
    pub fn check<P: AsRef<Path>>(&self, path: P) -> MockCliResult {
        let path = path.as_ref();
        let mut all_warnings = Vec::new();
        let mut processed_files = Vec::new();

        // Find files to process (simulate glob matching)
        let files_to_process: Vec<&Path> = if path.to_string_lossy() == "." {
            // Process all .md files in current directory
            self.files.keys().filter(|p| p.extension().map_or(false, |e| e == "md")).map(|p| p.as_path()).collect()
        } else if path.is_file() || path.to_string_lossy().ends_with(".md") {
            // Process single file
            vec![path]
        } else {
            // Process directory - find all .md files in it
            self.files.keys()
                .filter(|p| p.starts_with(path) && p.extension().map_or(false, |e| e == "md"))
                .map(|p| p.as_path())
                .collect()
        };

        for file_path in files_to_process {
            if let Some(content) = self.files.get(file_path) {
                processed_files.push(file_path.to_path_buf());

                let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
                let default_config = Config::default();
                let config = self.config.as_ref().unwrap_or(&default_config);
                let all_rules = rules::all_rules(config);

                for rule in all_rules {
                    if let Ok(warnings) = rule.check(&ctx) {
                        all_warnings.extend(warnings);
                    }
                }
            }
        }

        let stdout = if self.verbose {
            processed_files.iter()
                .map(|p| format!("Processing file: {}", p.display()))
                .collect::<Vec<_>>()
                .join("\n")
        } else {
            String::new()
        };

        MockCliResult {
            success: true,
            warnings: all_warnings,
            processed_files,
            stdout,
            stderr: String::new(),
        }
    }

    /// Simulate "rumdl check --fix" command
    pub fn check_fix<P: AsRef<Path>>(&self, path: P) -> MockCliResult {
        let mut result = self.check(path);

        // In a real implementation, this would apply fixes
        // For testing purposes, we simulate successful fixing
        result.stdout.push_str("\nApplied fixes to processed files");
        result
    }

    /// Simulate include/exclude filtering
    pub fn check_with_filters<P: AsRef<Path>>(&self, path: P, include: &[&str], exclude: &[&str]) -> MockCliResult {
        let path = path.as_ref();
        let mut all_warnings = Vec::new();
        let mut processed_files = Vec::new();

        // Apply include/exclude filtering
        let mut files_to_process: Vec<&PathBuf> = self.files.keys().collect();

        if !include.is_empty() {
            files_to_process = files_to_process.into_iter()
                .filter(|p| include.iter().any(|pattern| p.to_string_lossy().contains(pattern)))
                .collect();
        }

        if !exclude.is_empty() {
            files_to_process = files_to_process.into_iter()
                .filter(|p| !exclude.iter().any(|pattern| p.to_string_lossy().contains(pattern)))
                .collect();
        }

        for file_path in files_to_process {
            if let Some(content) = self.files.get(file_path) {
                processed_files.push(file_path.to_path_buf());

                let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
                let default_config = Config::default();
                let config = self.config.as_ref().unwrap_or(&default_config);
                let all_rules = rules::all_rules(config);

                for rule in all_rules {
                    if let Ok(warnings) = rule.check(&ctx) {
                        all_warnings.extend(warnings);
                    }
                }
            }
        }

        let stdout = if self.verbose {
            processed_files.iter()
                .map(|p| format!("Processing file: {}", p.display()))
                .collect::<Vec<_>>()
                .join("\n")
        } else {
            String::new()
        };

        MockCliResult {
            success: true,
            warnings: all_warnings,
            processed_files,
            stdout,
            stderr: String::new(),
        }
    }
}

impl Default for MockCli {
    fn default() -> Self {
        Self::new()
    }
}

/// Result of a mock CLI operation
#[derive(Debug, Clone)]
pub struct MockCliResult {
    pub success: bool,
    pub warnings: Vec<rumdl_lib::rule::LintWarning>,
    pub processed_files: Vec<PathBuf>,
    pub stdout: String,
    pub stderr: String,
}

impl MockCliResult {
    /// Check if a specific file was processed
    pub fn processed_file<P: AsRef<Path>>(&self, path: P) -> bool {
        let path = path.as_ref();
        self.processed_files.iter().any(|p| p == path || p.file_name() == path.file_name())
    }

    /// Check if stdout contains a specific string
    pub fn stdout_contains(&self, text: &str) -> bool {
        self.stdout.contains(text)
    }

    /// Check if stderr contains a specific string
    pub fn stderr_contains(&self, text: &str) -> bool {
        self.stderr.contains(text)
    }

    /// Get number of warnings
    pub fn warning_count(&self) -> usize {
        self.warnings.len()
    }
}

/// Quick test setup helper
pub fn setup_test_workspace() -> MockCli {
    let mut cli = MockCli::new();
    cli.add_file("README.md", "# Test\n")
       .add_file("docs/doc1.md", "# Doc 1\n")
       .add_file("docs/temp/temp.md", "# Temp\n")
       .add_file("src/test.md", "# Source\n")
       .add_file("subfolder/README.md", "# Subfolder README\n");
    cli
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mock_cli_basic() {
        let mut cli = MockCli::new();
        cli.add_file("test.md", "# Test\n\nContent here.\n");

        let result = cli.check("test.md");
        assert!(result.success);
        assert!(result.processed_file("test.md"));
    }

    #[test]
    fn test_mock_cli_filtering() {
        let cli = setup_test_workspace();

        let result = cli.check_with_filters(".", &["docs/doc1.md"], &[]);
        assert!(result.success);
        assert!(result.processed_file("docs/doc1.md"));
        assert!(!result.processed_file("README.md"));
    }

    #[test]
    fn test_mock_cli_exclude() {
        let cli = setup_test_workspace();

        let result = cli.check_with_filters(".", &[], &["docs/temp"]);
        assert!(result.success);
        assert!(result.processed_file("README.md"));
        assert!(!result.processed_file("docs/temp/temp.md"));
    }
}
