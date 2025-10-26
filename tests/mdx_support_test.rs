use std::fs;
use std::path::Path;
use tempfile::TempDir;

#[test]
fn test_all_markdown_extensions_discovery() {
    // Create a temporary directory with all markdown extension variants
    let temp_dir = TempDir::new().unwrap();
    let temp_path = temp_dir.path();

    // Create test files for all supported extensions
    let extensions = vec!["md", "markdown", "mdx", "mkd", "mkdn", "mdown", "mdwn"];
    for ext in &extensions {
        let file_path = temp_path.join(format!("test.{}", ext));
        fs::write(&file_path, format!("# Test {}\n", ext.to_uppercase())).unwrap();
    }

    // Create CheckArgs for testing
    let args = rumdl::CheckArgs {
        paths: vec![temp_path.to_string_lossy().to_string()],
        verbose: false,
        quiet: false,
        fix: false,
        diff: false,
        include: None,
        exclude: None,
        no_exclude: false,
        enable: None,
        disable: None,
        extend_enable: None,
        extend_disable: None,
        respect_gitignore: false,
        output_format: None,
        statistics: false,
        config: None,
    };

    // Load default config
    let config = rumdl_lib::config::Config::default();

    // Find markdown files
    let files =
        rumdl::file_processor::find_markdown_files(&args.paths, &args, &config).expect("Failed to find markdown files");

    // Verify all extension types are discovered
    for ext in &extensions {
        assert!(
            files.iter().any(|f| f.ends_with(&format!("test.{}", ext))),
            "{} file should be discovered. Found files: {:?}",
            ext.to_uppercase(),
            files
        );
    }

    // Verify we found exactly the right number of files
    assert_eq!(
        files.len(),
        extensions.len(),
        "Should find exactly {} files, found {}. Files: {:?}",
        extensions.len(),
        files.len(),
        files
    );
}

#[test]
fn test_mdx_file_explicit_path() {
    // Create a temporary directory with an .mdx file
    let temp_dir = TempDir::new().unwrap();
    let temp_path = temp_dir.path();

    // Create test file
    let mdx_file = temp_path.join("test.mdx");
    fs::write(&mdx_file, "# Test MDX\n\n<Component />").unwrap();

    // Create CheckArgs with explicit path to .mdx file
    let args = rumdl::CheckArgs {
        paths: vec![mdx_file.to_string_lossy().to_string()],
        verbose: false,
        quiet: false,
        fix: false,
        diff: false,
        include: None,
        exclude: None,
        no_exclude: false,
        enable: None,
        disable: None,
        extend_enable: None,
        extend_disable: None,
        respect_gitignore: false,
        output_format: None,
        statistics: false,
        config: None,
    };

    // Load default config
    let config = rumdl_lib::config::Config::default();

    // Find markdown files
    let files =
        rumdl::file_processor::find_markdown_files(&args.paths, &args, &config).expect("Failed to find markdown files");

    // Verify the .mdx file is discovered
    assert_eq!(files.len(), 1, "Should find exactly one file");
    assert!(
        files[0].ends_with("test.mdx"),
        "Should find the MDX file. Found: {:?}",
        files[0]
    );
}

#[test]
fn test_alternative_extension_explicit_path() {
    // Test that alternative markdown extensions work with explicit paths
    let temp_dir = TempDir::new().unwrap();
    let temp_path = temp_dir.path();

    let alt_extensions = vec!["mkd", "mkdn", "mdown", "mdwn"];

    for ext in &alt_extensions {
        let file_path = temp_path.join(format!("test.{}", ext));
        fs::write(&file_path, format!("# Test {}\n", ext.to_uppercase())).unwrap();

        let args = rumdl::CheckArgs {
            paths: vec![file_path.to_string_lossy().to_string()],
            verbose: false,
            quiet: false,
            fix: false,
            diff: false,
            include: None,
            exclude: None,
            no_exclude: false,
            enable: None,
            disable: None,
            extend_enable: None,
            extend_disable: None,
            respect_gitignore: false,
            output_format: None,
            statistics: false,
            config: None,
        };

        let config = rumdl_lib::config::Config::default();
        let files = rumdl::file_processor::find_markdown_files(&args.paths, &args, &config)
            .expect("Failed to find markdown files");

        assert_eq!(files.len(), 1, "Should find exactly one {} file", ext.to_uppercase());
        assert!(
            files[0].ends_with(&format!("test.{}", ext)),
            "Should find the {} file. Found: {:?}",
            ext.to_uppercase(),
            files[0]
        );
    }
}

#[test]
fn test_mdx_file_can_be_linted() {
    // Create a temporary directory with an .mdx file
    let temp_dir = TempDir::new().unwrap();
    let temp_path = temp_dir.path();

    // Create test file with a linting issue (multiple blank lines)
    let mdx_file = temp_path.join("test.mdx");
    fs::write(&mdx_file, "# Test MDX\n\n\n\n<Component />").unwrap();

    // Create CheckArgs with explicit path to .mdx file
    let args = rumdl::CheckArgs {
        paths: vec![mdx_file.to_string_lossy().to_string()],
        verbose: false,
        quiet: false,
        fix: false,
        diff: false,
        include: None,
        exclude: None,
        no_exclude: false,
        enable: None,
        disable: None,
        extend_enable: None,
        extend_disable: None,
        respect_gitignore: false,
        output_format: None,
        statistics: false,
        config: None,
    };

    // Load default config
    let config = rumdl_lib::config::Config::default();

    // Get all rules
    let rules = rumdl_lib::rules::all_rules(&config);

    // Process the file
    let warnings = rumdl::file_processor::process_file_collect_warnings(
        &mdx_file.to_string_lossy(),
        &rules,
        false,
        false,
        true, // quiet mode
        &config,
        None,
    );

    // Verify that linting works (MD012 should catch multiple blank lines)
    assert!(
        warnings.iter().any(|w| w.rule_name.as_deref() == Some("MD012")),
        "Should detect MD012 (multiple blank lines) in MDX file. Warnings: {:?}",
        warnings
    );
}
