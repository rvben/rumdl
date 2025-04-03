#[cfg(test)]
mod exclude_tests {
    use rumdl::should_exclude;

    #[test]
    fn test_simple_file_exclusion() {
        let exclude_patterns = vec!["test.md".to_string()];
        assert!(should_exclude("test.md", &exclude_patterns, false));
        assert!(!should_exclude("other.md", &exclude_patterns, false));
    }

    #[test]
    fn test_directory_exclusion() {
        let exclude_patterns = vec!["docs/".to_string()];
        assert!(should_exclude("docs/test.md", &exclude_patterns, false));
        assert!(!should_exclude("other/test.md", &exclude_patterns, false));
    }

    #[test]
    fn test_glob_pattern() {
        let exclude_patterns = vec!["*.md".to_string()];
        assert!(should_exclude("test.md", &exclude_patterns, false));
        assert!(!should_exclude("test.txt", &exclude_patterns, false));
    }

    #[test]
    fn test_nested_glob_pattern() {
        let exclude_patterns = vec!["docs/*.md".to_string()];
        assert!(should_exclude("docs/test.md", &exclude_patterns, false));
        assert!(!should_exclude("other/test.md", &exclude_patterns, false));

        let double_star_patterns = vec!["docs/**/*.md".to_string()];
        let direct_subdir_pattern = vec!["docs/subdir/*.md".to_string()];

        assert!(should_exclude(
            "docs/subdir/test.md",
            &double_star_patterns,
            false
        ));

        assert!(should_exclude(
            "docs/subdir/test.md",
            &direct_subdir_pattern,
            false
        ));
        assert!(!should_exclude(
            "docs/other-subdir/test.md",
            &direct_subdir_pattern,
            false
        ));
    }

    #[test]
    fn test_double_star_glob() {
        let double_star_patterns = vec!["docs/**/*.md".to_string()];
        let direct_subdir_pattern = vec!["docs/subdir/*.md".to_string()];

        // Double star pattern should match files in subdirectories
        assert!(should_exclude(
            "docs/subdir/test.md",
            &double_star_patterns,
            false
        ));

        // Direct subdir pattern should match files in that subdir
        assert!(should_exclude(
            "docs/subdir/test.md",
            &direct_subdir_pattern,
            false
        ));

        // Direct subdir pattern should not match files in other subdirs
        assert!(!should_exclude(
            "docs/other-subdir/test.md",
            &direct_subdir_pattern,
            false
        ));
    }

    #[test]
    fn test_multiple_patterns() {
        let exclude_patterns = vec![
            "docs/".to_string(),
            "test.md".to_string(),
            "*.txt".to_string(),
        ];
        assert!(should_exclude("docs/test.md", &exclude_patterns, false));
        assert!(should_exclude("test.md", &exclude_patterns, false));
        assert!(should_exclude("other.txt", &exclude_patterns, false));
        assert!(!should_exclude("other.md", &exclude_patterns, false));
    }

    #[test]
    fn test_invalid_patterns() {
        let exclude_patterns = vec!["[invalid".to_string()];
        assert!(should_exclude(
            "file_with_[invalid_pattern.md",
            &exclude_patterns,
            false
        ));
        assert!(!should_exclude("normal_file.md", &exclude_patterns, false));
    }
}
