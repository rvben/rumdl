#[cfg(test)]
mod exclude_tests {
    use rumdl::should_exclude;

    #[test]
    fn test_simple_file_exclusion() {
        let exclude_patterns = vec!["test.md".to_string()];
        assert!(should_exclude("test.md", &exclude_patterns));
        assert!(!should_exclude("other.md", &exclude_patterns));
    }

    #[test]
    fn test_directory_exclusion() {
        let exclude_patterns = vec!["docs/".to_string()];
        assert!(should_exclude("docs/test.md", &exclude_patterns));
        
        assert!(should_exclude("./docs/test.md", &exclude_patterns));
        
        assert!(!should_exclude("other/test.md", &exclude_patterns));
    }

    #[test]
    fn test_glob_pattern() {
        let exclude_patterns = vec!["*.md".to_string()];
        assert!(should_exclude("test.md", &exclude_patterns));
        assert!(should_exclude("other.md", &exclude_patterns));
        assert!(!should_exclude("test.txt", &exclude_patterns));
    }

    #[test]
    fn test_nested_glob_pattern() {
        let exclude_patterns = vec!["docs/*.md".to_string()];
        assert!(should_exclude("docs/test.md", &exclude_patterns));
        assert!(!should_exclude("other/test.md", &exclude_patterns));
        
        let double_star_patterns = vec!["docs/**/*.md".to_string()];
        let direct_subdir_pattern = vec!["docs/subdir/*.md".to_string()];
        
        assert!(should_exclude("docs/subdir/test.md", &double_star_patterns));
        
        assert!(should_exclude("docs/subdir/test.md", &direct_subdir_pattern));
        assert!(!should_exclude("docs/other-subdir/test.md", &direct_subdir_pattern));
    }

    #[test]
    fn test_double_star_glob() {
        let exclude_patterns = vec!["docs/**/*.md".to_string()];
        assert!(should_exclude("docs/test.md", &exclude_patterns));
        assert!(should_exclude("docs/subdir/test.md", &exclude_patterns));
        assert!(!should_exclude("other/test.md", &exclude_patterns));
    }

    #[test]
    fn test_multiple_patterns() {
        let exclude_patterns = vec![
            "test.md".to_string(),
            "docs/".to_string(),
            "temp_*.md".to_string(),
        ];
        assert!(should_exclude("test.md", &exclude_patterns));
        assert!(should_exclude("docs/file.md", &exclude_patterns));
        assert!(should_exclude("temp_1.md", &exclude_patterns));
        assert!(!should_exclude("other.md", &exclude_patterns));
    }

    #[test]
    fn test_invalid_patterns() {
        let exclude_patterns = vec!["[invalid".to_string()];
        assert!(should_exclude("file_with_[invalid_pattern.md", &exclude_patterns));
        assert!(!should_exclude("normal_file.md", &exclude_patterns));
    }
} 