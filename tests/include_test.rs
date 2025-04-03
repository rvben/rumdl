use rumdl::should_include;
use rumdl::should_exclude;
use std::fs;

// Helper function to check if we can create test files
fn can_create_test_files() -> bool {
    if let Ok(temp_dir) = tempfile::tempdir() {
        let test_file = temp_dir.path().join("test_file");
        let result = fs::write(&test_file, "test").is_ok();
        temp_dir.close().unwrap();
        result
    } else {
        false
    }
}

#[cfg(test)]
mod include_tests {
    use super::*;

    #[test]
    fn test_empty_patterns() {
        let include_patterns = Vec::new();
        assert!(should_include("test.md", &include_patterns));
        assert!(should_include("docs/test.md", &include_patterns));
        assert!(should_include("any/path/file.md", &include_patterns));
    }

    #[test]
    fn test_simple_file_inclusion() {
        let include_patterns = vec!["test.md".to_string()];
        assert!(should_include("test.md", &include_patterns));
        assert!(!should_include("other.md", &include_patterns));
    }

    #[test]
    fn test_directory_inclusion() {
        let include_patterns = vec!["docs/".to_string()];
        assert!(should_include("docs/test.md", &include_patterns));
        assert!(should_include("./docs/test.md", &include_patterns));
        assert!(!should_include("other/test.md", &include_patterns));
    }

    #[test]
    fn test_glob_pattern() {
        let include_patterns = vec!["*.md".to_string()];
        assert!(should_include("test.md", &include_patterns));
        assert!(should_include("other.md", &include_patterns));
        assert!(!should_include("test.txt", &include_patterns));
    }

    #[test]
    fn test_nested_glob_pattern() {
        let include_patterns = vec!["docs/*.md".to_string()];
        assert!(should_include("docs/test.md", &include_patterns));
        assert!(!should_include("other/test.md", &include_patterns));

        let double_star_patterns = vec!["docs/**/*.md".to_string()];
        let direct_subdir_pattern = vec!["docs/subdir/*.md".to_string()];

        assert!(should_include("docs/subdir/test.md", &double_star_patterns));
        assert!(should_include("docs/subdir/test.md", &direct_subdir_pattern));
        assert!(!should_include("docs/other-subdir/test.md", &direct_subdir_pattern));
    }

    #[test]
    fn test_double_star_glob() {
        let include_patterns = vec!["docs/**/*.md".to_string()];
        assert!(should_include("docs/test.md", &include_patterns));
        assert!(should_include("docs/subdir/test.md", &include_patterns));
        assert!(!should_include("other/test.md", &include_patterns));
    }

    #[test]
    fn test_multiple_patterns() {
        let include_patterns = vec![
            "test.md".to_string(),
            "docs/".to_string(),
            "src/**/*.md".to_string(),
        ];
        assert!(should_include("test.md", &include_patterns));
        assert!(should_include("docs/file.md", &include_patterns));
        assert!(should_include("src/rules/test.md", &include_patterns));
        assert!(!should_include("other.md", &include_patterns));
    }

    #[test]
    fn test_invalid_patterns() {
        let include_patterns = vec!["[invalid".to_string()];
        assert!(should_include("file_with_[invalid_pattern.md", &include_patterns));
        assert!(!should_include("normal_file.md", &include_patterns));
    }

    #[test]
    fn test_include_exclude_interaction() {
        let include_patterns = vec!["docs/**/*.md".to_string()];
        let exclude_patterns = vec!["docs/temp/".to_string()];

        // File in docs but not in temp - should be included and not excluded
        assert!(should_include("docs/test.md", &include_patterns));
        assert!(!should_exclude("docs/test.md", &exclude_patterns, false));

        // File in docs/temp - should be included but also excluded
        assert!(should_include("docs/temp/test.md", &include_patterns));
        assert!(should_exclude("docs/temp/test.md", &exclude_patterns, false));

        // File outside docs - should not be included
        assert!(!should_include("src/test.md", &include_patterns));
        assert!(!should_exclude("src/test.md", &exclude_patterns, false));
    }

    #[test]
    fn test_include_patterns() {
        let exclude_patterns = vec!["docs/temp/".to_string()];
        assert!(!should_exclude("docs/test.md", &exclude_patterns, false));
        assert!(should_exclude("docs/temp/test.md", &exclude_patterns, false));
        assert!(!should_exclude("src/test.md", &exclude_patterns, false));
    }

    #[test]
    fn test_whitespace_in_patterns_and_filenames() {
        // Test patterns with spaces
        let include_patterns = vec!["file with spaces.md".to_string()];
        assert!(should_include("file with spaces.md", &include_patterns));
        assert!(!should_include("file_without_spaces.md", &include_patterns));
        
        // Test directory patterns with spaces
        let include_patterns = vec!["folder with spaces/*.md".to_string()];
        assert!(should_include("folder with spaces/document.md", &include_patterns));
        assert!(!should_include("folder_without_spaces/document.md", &include_patterns));
        
        // Test patterns with tabs and other whitespace
        let include_patterns = vec!["file\twith\ttabs.md".to_string()];
        assert!(should_include("file\twith\ttabs.md", &include_patterns));
        assert!(!should_include("file with tabs.md", &include_patterns));
    }

    #[test]
    fn test_unicode_filenames() {
        // Test basic Unicode characters
        let include_patterns = vec!["résumé.md".to_string()];
        assert!(should_include("résumé.md", &include_patterns));
        assert!(!should_include("resume.md", &include_patterns));
        
        // Test more complex Unicode in folders and files
        let include_patterns = vec!["中文/文件.md".to_string()];
        assert!(should_include("中文/文件.md", &include_patterns));
        assert!(!should_include("chinese/file.md", &include_patterns));
        
        // Test Unicode in glob patterns
        let include_patterns = vec!["emoji_📁/*.md".to_string()];
        assert!(should_include("emoji_📁/document.md", &include_patterns));
        assert!(!should_include("emoji_folder/document.md", &include_patterns));
    }

    #[test]
    fn test_symlinks() {
        // Skip this test if we can't create files (e.g., in CI environment)
        if !can_create_test_files() {
            return;
        }

        // Create temporary directory for test
        let temp_dir = tempfile::tempdir().unwrap();
        let base_path = temp_dir.path();
        
        // Create real file
        let real_file_path = base_path.join("real_file.md");
        fs::write(&real_file_path, "content").unwrap();
        
        // Create directory
        let dir_path = base_path.join("real_dir");
        fs::create_dir(&dir_path).unwrap();
        
        // Create file in directory
        let dir_file_path = dir_path.join("dir_file.md");
        fs::write(&dir_file_path, "content").unwrap();
        
        // Create symlink to file
        let symlink_file_path = base_path.join("symlink_file.md");
        #[cfg(unix)]
        std::os::unix::fs::symlink(&real_file_path, &symlink_file_path).unwrap_or_else(|_| {
            // If symlink creation fails, just create a regular file
            fs::write(&symlink_file_path, "content").unwrap();
        });
        #[cfg(windows)]
        fs::write(&symlink_file_path, "content").unwrap(); // Fallback for Windows
        
        // Create symlink to directory 
        let symlink_dir_path = base_path.join("symlink_dir");
        #[cfg(unix)]
        std::os::unix::fs::symlink(&dir_path, &symlink_dir_path).unwrap_or_else(|_| {
            // If symlink creation fails, just create a regular directory
            fs::create_dir(&symlink_dir_path).unwrap();
            fs::write(symlink_dir_path.join("dir_file.md"), "content").unwrap();
        });
        #[cfg(windows)]
        {
            fs::create_dir(&symlink_dir_path).unwrap();
            fs::write(symlink_dir_path.join("dir_file.md"), "content").unwrap();
        }
        
        // Test symlink to file
        let include_patterns = vec![symlink_file_path.to_string_lossy().to_string()];
        assert!(should_include(&symlink_file_path.to_string_lossy(), &include_patterns));
        
        // Test symlink to directory
        let include_patterns = vec![format!("{}/*.md", symlink_dir_path.to_string_lossy())];
        assert!(should_include(&format!("{}/dir_file.md", symlink_dir_path.to_string_lossy()), &include_patterns));
        
        // Clean up
        temp_dir.close().unwrap();
    }

    #[test]
    fn test_complex_glob_patterns() {
        // Test character classes
        let include_patterns = vec!["file[0-9].md".to_string()];
        assert!(should_include("file1.md", &include_patterns));
        assert!(should_include("file5.md", &include_patterns));
        assert!(!should_include("fileA.md", &include_patterns));
        
        // Test alternation with braces
        let include_patterns = vec!["*.{md,txt}".to_string()];
        assert!(should_include("document.md", &include_patterns));
        assert!(should_include("document.txt", &include_patterns));
        assert!(!should_include("document.docx", &include_patterns));
        
        // Test negated character classes
        let include_patterns = vec!["file[!0-9].md".to_string()];
        assert!(should_include("fileA.md", &include_patterns));
        assert!(!should_include("file1.md", &include_patterns));
        
        // Test complex pattern with multiple character classes and alternations
        let include_patterns = vec!["[a-z]_[0-9].{md,txt}".to_string()];
        assert!(should_include("a_1.md", &include_patterns));
        assert!(should_include("z_9.txt", &include_patterns));
        assert!(!should_include("A_1.md", &include_patterns));
        assert!(!should_include("a_1.docx", &include_patterns));
    }

    #[test]
    fn test_path_traversal_patterns() {
        // Test pattern with parent directory reference
        let include_patterns = vec!["../test.md".to_string()];
        
        // When in a subdirectory, this should match a file in the parent
        assert!(should_include("../test.md", &include_patterns));

        // Test pattern with complex traversal
        let include_patterns = vec!["../../docs/*.md".to_string()];
        assert!(should_include("../../docs/document.md", &include_patterns));
        assert!(!should_include("../../other/document.md", &include_patterns));
        
        // Test with relative path references
        let include_patterns = vec!["./docs/../src/*.md".to_string()];
        assert!(should_include("./docs/../src/file.md", &include_patterns));
        
        // This test is more complex as it depends on how path normalization is done
        // In the implementation, we need to normalize both paths the same way
        let include_patterns = vec!["src/*.md".to_string()];
        assert!(should_include("src/file.md", &include_patterns));
        
        // Test normalized path matches
        let include_patterns = vec!["./docs/../src/*.md".to_string()];
        assert!(should_include("src/file.md", &include_patterns));
        
        assert!(!should_include("docs/file.md", &include_patterns));
    }

    #[test]
    fn test_multiple_star_patterns() {
        // Test pattern with both ** and * wildcards
        let include_patterns = vec!["**/*.{md,txt}".to_string()];
        assert!(should_include("file.md", &include_patterns));
        assert!(should_include("docs/file.md", &include_patterns));
        assert!(should_include("docs/subdirectory/file.txt", &include_patterns));
        assert!(!should_include("file.docx", &include_patterns));
        
        // Test pattern with specific directory and multiple extensions
        let include_patterns = vec!["docs/**/*.{md,txt,json}".to_string()];
        assert!(should_include("docs/file.md", &include_patterns));
        assert!(should_include("docs/subdirectory/file.txt", &include_patterns));
        assert!(should_include("docs/deep/nested/config.json", &include_patterns));
        assert!(!should_include("src/file.md", &include_patterns));
        assert!(!should_include("docs/config.yaml", &include_patterns));
        
        // Test pattern with multiple directory wildcards
        let include_patterns = vec!["**/tests/**/*.md".to_string()];
        assert!(should_include("tests/file.md", &include_patterns));
        assert!(should_include("src/tests/file.md", &include_patterns));
        assert!(should_include("tests/unit/integration/file.md", &include_patterns));
        assert!(!should_include("docs/file.md", &include_patterns));
    }
} 