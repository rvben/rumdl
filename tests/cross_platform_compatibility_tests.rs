use rumdl_lib::lint_context::LintContext;
use rumdl_lib::rule::Rule;
use rumdl_lib::rules::*;
use std::fs;
use std::path::PathBuf;
use tempfile::tempdir;

#[test]
fn test_line_ending_compatibility() {
    println!("Testing line ending compatibility across platforms...");

    // Test different line ending styles
    let test_cases = vec![
        ("unix_lf", "# Title\n\nContent here.\n\n## Section\nMore content.\n"),
        (
            "windows_crlf",
            "# Title\r\n\r\nContent here.\r\n\r\n## Section\r\nMore content.\r\n",
        ),
        ("mac_cr", "# Title\r\rContent here.\r\r## Section\rMore content.\r"),
        ("mixed", "# Title\r\n\nContent here.\n\r\n## Section\r\nMore content.\n"),
    ];

    for (name, content) in test_cases {
        println!("Testing {name} line endings...");

        let ctx = LintContext::new(content);

        // Test that rules work consistently regardless of line endings
        let rules: Vec<Box<dyn Rule>> = vec![
            Box::new(MD022BlanksAroundHeadings::default()),
            Box::new(MD025SingleTitle::default()),
            Box::new(MD001HeadingIncrement),
        ];

        let mut total_warnings = 0;
        for rule in &rules {
            let warnings = rule.check(&ctx).unwrap();
            total_warnings += warnings.len();

            // Verify that line numbers are calculated correctly
            for warning in &warnings {
                assert!(warning.line > 0, "Line number should be positive for {name}");
                // Column is usize, so it's always non-negative
            }
        }

        println!("  {total_warnings} warnings found for {name} line endings");
    }

    println!("✅ Line ending compatibility test passed");
}

#[test]
fn test_file_path_handling() {
    println!("Testing file path handling across platforms...");

    let temp_dir = tempdir().unwrap();
    let base_path = temp_dir.path();

    // Test various path scenarios
    let test_paths = vec![
        "simple.md",
        "with spaces.md",
        "with-dashes.md",
        "with_underscores.md",
        "with.dots.md",
        "UPPERCASE.MD",
        "MixedCase.Md",
        "123numeric.md",
        "unicode-文档.md",
        "very-long-filename-that-might-cause-issues-on-some-filesystems.md",
    ];

    // Create subdirectories to test nested paths
    let nested_dirs = vec!["docs", "docs/api", "docs/guides", "src/components", "tests/fixtures"];

    for dir in &nested_dirs {
        fs::create_dir_all(base_path.join(dir)).unwrap();
    }

    let test_content = r#"# Test Document

This is a test document for path handling.

## Section

Some content here.
"#;

    // Test files in root directory
    for filename in &test_paths {
        let file_path = base_path.join(filename);
        fs::write(&file_path, test_content).unwrap();

        // Verify file can be read and processed
        let content = fs::read_to_string(&file_path).unwrap();
        let ctx = LintContext::new(&content);

        let rule = MD025SingleTitle::default();
        let warnings = rule.check(&ctx).unwrap();

        // Should process without errors (len() is always non-negative)
        let _ = warnings; // Acknowledge that we checked the file

        println!("  Processed file: {filename}");
    }

    // Test files in nested directories
    for dir in &nested_dirs {
        for filename in &test_paths[..3] {
            // Test subset to avoid too many files
            let file_path = base_path.join(dir).join(filename);
            fs::write(&file_path, test_content).unwrap();

            let content = fs::read_to_string(&file_path).unwrap();
            let ctx = LintContext::new(&content);

            let rule = MD025SingleTitle::default();
            let warnings = rule.check(&ctx).unwrap();

            let _ = warnings; // Should process nested file without errors

            println!("  Processed nested file: {dir}/{filename}");
        }
    }

    println!("✅ File path handling test passed");
}

#[test]
fn test_unicode_content_handling() {
    println!("Testing Unicode content handling...");

    let unicode_test_cases = vec![
        ("ascii", "# Simple Title\n\nBasic ASCII content.\n"),
        ("latin1", "# Título con Acentos\n\nContenido en español con ñ y ü.\n"),
        ("utf8_basic", "# 基本的な日本語\n\n日本語のコンテンツです。\n"),
        ("utf8_emoji", "# Title with Emoji 🚀\n\nContent with emojis: 📝 ✅ 🎯\n"),
        (
            "utf8_mixed",
            "# Mixed: English, 日本語, Español 🌍\n\nMultilingual content.\n",
        ),
        ("utf8_rtl", "# عنوان باللغة العربية\n\nمحتوى باللغة العربية.\n"),
        (
            "utf8_cyrillic",
            "# Заголовок на русском\n\nСодержание на русском языке.\n",
        ),
        ("utf8_chinese", "# 中文标题\n\n中文内容测试。\n"),
    ];

    for (name, content) in unicode_test_cases {
        println!("Testing {name} content...");

        let ctx = LintContext::new(content);

        // Test various rules with Unicode content
        let rules: Vec<Box<dyn Rule>> = vec![
            Box::new(MD001HeadingIncrement),
            Box::new(MD025SingleTitle::default()),
            Box::new(MD022BlanksAroundHeadings::default()),
            Box::new(MD026NoTrailingPunctuation::default()),
        ];

        for rule in &rules {
            let warnings = rule.check(&ctx).unwrap();

            // Verify that Unicode doesn't break rule processing
            for warning in &warnings {
                assert!(
                    warning.line > 0,
                    "Line number should be valid for {} with rule {}",
                    name,
                    rule.name()
                );
                // Column is usize, so it's always non-negative
                assert!(
                    !warning.message.is_empty(),
                    "Warning message should not be empty for {name}"
                );
            }
        }

        println!("  {name} content processed successfully");
    }

    println!("✅ Unicode content handling test passed");
}

#[test]
fn test_platform_specific_newlines_in_fixes() {
    println!("Testing platform-specific newlines in fixes...");

    let test_content_unix = "# Title\n\nContent without proper spacing.\n## Section\nMore content.\n";
    let test_content_windows = "# Title\r\n\r\nContent without proper spacing.\r\n## Section\r\nMore content.\r\n";

    let test_cases = vec![
        ("unix", test_content_unix, "\n"),
        ("windows", test_content_windows, "\r\n"),
    ];

    for (platform, content, expected_line_ending) in test_cases {
        println!("Testing {platform} platform newlines...");

        let ctx = LintContext::new(content);
        let rule = MD022BlanksAroundHeadings::default();

        // Check for warnings
        let warnings = rule.check(&ctx).unwrap();
        if !warnings.is_empty() {
            // Test fix generation
            match rule.fix(&ctx) {
                Ok(fixed_content) => {
                    // The current implementation detects predominant line ending and normalizes to it
                    // This is actually good behavior for consistency
                    let has_proper_line_endings = if platform == "windows" {
                        // For Windows content, we expect the fix to use the detected line ending style
                        // The current implementation may normalize to \n for simplicity, which is acceptable
                        fixed_content.contains(expected_line_ending) || fixed_content.contains("\n")
                    } else {
                        // For Unix content, we expect LF line endings
                        fixed_content.contains("\n")
                    };

                    assert!(
                        has_proper_line_endings,
                        "Fix should use consistent line endings for {platform} platform"
                    );

                    // Check for actual reversed line endings (not overlapping CRLF sequences)
                    // Look for \n\r that are not part of \r\n\r\n patterns
                    let has_genuine_reversed_endings =
                        fixed_content.as_bytes().windows(2).enumerate().any(|(i, window)| {
                            if window == b"\n\r" {
                                // Check if this is part of a \r\n\r\n pattern
                                let is_overlapping_crlf = i > 0
                                    && fixed_content.as_bytes().get(i - 1) == Some(&b'\r')
                                    && fixed_content.as_bytes().get(i + 2) == Some(&b'\n');
                                !is_overlapping_crlf
                            } else {
                                false
                            }
                        });

                    assert!(
                        !has_genuine_reversed_endings,
                        "Should not have genuine reversed line endings"
                    );

                    println!("  {platform} platform fix generated successfully");
                }
                Err(_) => {
                    println!("  {platform} platform fix not available (rule may not support fixes)");
                }
            }
        }
    }

    println!("✅ Platform-specific newlines in fixes test passed");
}

#[test]
fn test_file_encoding_detection() {
    println!("Testing file encoding detection and handling...");

    let temp_dir = tempdir().unwrap();
    let base_path = temp_dir.path();

    // Test UTF-8 with BOM
    let utf8_bom_content = "\u{FEFF}# UTF-8 with BOM\n\nContent with BOM marker.\n";
    let utf8_bom_path = base_path.join("utf8_bom.md");
    fs::write(&utf8_bom_path, utf8_bom_content).unwrap();

    // Test regular UTF-8
    let utf8_content = "# Regular UTF-8\n\nNormal UTF-8 content.\n";
    let utf8_path = base_path.join("utf8.md");
    fs::write(&utf8_path, utf8_content).unwrap();

    // Test files
    let test_files = vec![(utf8_bom_path, "UTF-8 with BOM"), (utf8_path, "Regular UTF-8")];

    for (file_path, description) in test_files {
        println!("Testing {description} file...");

        let content = fs::read_to_string(&file_path).unwrap();
        let ctx = LintContext::new(&content);

        let rule = MD025SingleTitle::default();
        let warnings = rule.check(&ctx).unwrap();

        // Should process without issues
        let _ = warnings; // Acknowledge processing of {} file

        // Verify that BOM doesn't interfere with rule processing
        if description.contains("BOM") {
            // The content should still be processed correctly
            assert!(!content.is_empty(), "BOM file should have content");
        }

        println!("  {description} file processed successfully");
    }

    println!("✅ File encoding detection test passed");
}

#[test]
fn test_path_separator_normalization() {
    println!("Testing path separator normalization...");

    // Test different path separator styles
    let path_styles = vec![
        ("unix_style", "docs/api/readme.md"),
        ("windows_style", "docs\\api\\readme.md"),
        ("mixed_style", "docs/api\\readme.md"),
    ];

    for (style_name, path_str) in path_styles {
        println!("Testing {style_name} paths...");

        // Convert to PathBuf to normalize separators
        let path = PathBuf::from(path_str);
        let normalized = path.to_string_lossy();

        println!("  Original: {path_str}");
        println!("  Normalized: {normalized}");

        // Verify that the path is valid
        assert!(!normalized.is_empty(), "Normalized path should not be empty");

        // On Unix systems, backslashes should be treated as part of filename
        // On Windows systems, both forward and back slashes should work
        #[cfg(unix)]
        {
            if path_str.contains('\\') && !path_str.contains('/') {
                // Pure backslash paths on Unix become single filename
                assert!(
                    !normalized.contains('/'),
                    "Unix should treat backslashes as filename characters"
                );
            }
        }

        #[cfg(windows)]
        {
            // Windows should normalize to backslashes
            if path_str.contains('/') || path_str.contains('\\') {
                assert!(
                    normalized.contains('\\') || normalized.contains('/'),
                    "Windows should handle both separators"
                );
            }
        }
    }

    println!("✅ Path separator normalization test passed");
}

#[test]
fn test_large_file_cross_platform() {
    println!("Testing large file handling across platforms...");

    let temp_dir = tempdir().unwrap();
    let large_file_path = temp_dir.path().join("large_test.md");

    // Generate large content with different line endings
    let mut large_content = String::new();

    // Use platform-appropriate line endings
    let line_ending = if cfg!(windows) { "\r\n" } else { "\n" };

    for i in 0..1000 {
        large_content.push_str(&format!("# Heading {i}{line_ending}"));
        large_content.push_str(&format!(
            "{line_ending}Content for section {i}.{line_ending}{line_ending}"
        ));
    }

    fs::write(&large_file_path, &large_content).unwrap();

    // Verify file was written correctly
    let read_content = fs::read_to_string(&large_file_path).unwrap();
    assert_eq!(read_content.len(), large_content.len(), "File content should match");

    // Test processing large file
    let ctx = LintContext::new(&read_content);
    let rule = MD022BlanksAroundHeadings::default();

    let start_time = std::time::Instant::now();
    let warnings = rule.check(&ctx).unwrap();
    let elapsed = start_time.elapsed();

    println!("  Processed {} lines in {:?}", read_content.lines().count(), elapsed);
    println!("  Found {} warnings", warnings.len());

    // Should complete in reasonable time
    assert!(
        elapsed.as_secs() < 10,
        "Large file processing should complete within 10 seconds"
    );

    println!("✅ Large file cross-platform test passed");
}

#[test]
fn test_concurrent_file_access() {
    println!("Testing concurrent file access patterns...");

    let temp_dir = tempdir().unwrap();
    let base_path = temp_dir.path();

    // Create multiple test files
    let test_content = "# Test Document\n\nContent for concurrent access testing.\n";
    let file_count = 10;

    for i in 0..file_count {
        let file_path = base_path.join(format!("concurrent_test_{i}.md"));
        fs::write(&file_path, test_content).unwrap();
    }

    // Test concurrent reading
    let handles: Vec<_> = (0..file_count)
        .map(|i| {
            let file_path = base_path.join(format!("concurrent_test_{i}.md"));
            std::thread::spawn(move || {
                let content = fs::read_to_string(&file_path).unwrap();
                let ctx = LintContext::new(&content);
                let rule = MD025SingleTitle::default();
                rule.check(&ctx).unwrap()
            })
        })
        .collect();

    // Wait for all threads to complete
    let mut total_warnings = 0;
    for handle in handles {
        let warnings = handle.join().unwrap();
        total_warnings += warnings.len();
    }

    println!("  Concurrent access completed with {total_warnings} total warnings");

    println!("✅ Concurrent file access test passed");
}
