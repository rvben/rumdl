use rumdl::should_include;
use rumdl::should_exclude;

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
} 