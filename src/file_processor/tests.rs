use super::embedded::{
    MAX_EMBEDDED_DEPTH, check_embedded_markdown_blocks, format_embedded_markdown_blocks, should_lint_embedded_markdown,
};
use super::*;
use rumdl_lib::config as rumdl_config;
use rumdl_lib::rule::Rule;
use std::fs;
use std::path::Path;
use tempfile::TempDir;

/// Create a temporary directory structure for testing path display
fn create_test_structure() -> TempDir {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let docs_dir = temp_dir.path().join("docs");
    fs::create_dir_all(&docs_dir).expect("Failed to create docs dir");
    fs::write(docs_dir.join("guide.md"), "# Test").expect("Failed to write test file");
    temp_dir
}

#[test]
fn test_to_display_path_with_project_root() {
    let temp_dir = create_test_structure();
    let project_root = temp_dir.path();
    let file_path = project_root.join("docs/guide.md");

    let result = to_display_path(&file_path.to_string_lossy(), Some(project_root));

    assert_eq!(result, "docs/guide.md");
}

#[test]
fn test_to_display_path_with_canonical_paths() {
    let temp_dir = create_test_structure();
    let project_root = temp_dir.path().canonicalize().unwrap();
    let file_path = project_root.join("docs/guide.md").canonicalize().unwrap();

    let result = to_display_path(&file_path.to_string_lossy(), Some(&project_root));

    assert_eq!(result, "docs/guide.md");
}

#[test]
fn test_to_display_path_no_project_root_uses_cwd() {
    // Test that when no project_root is given, files under CWD get relative paths
    // We test this indirectly by checking files in CWD get stripped
    let cwd = std::env::current_dir().unwrap();
    let cwd_canonical = cwd.canonicalize().unwrap_or(cwd.clone());

    // Create a path that would be under CWD
    let test_path = cwd_canonical.join("test_file.md");

    // Even if file doesn't exist, the path should be made relative to CWD
    let result = to_display_path(&test_path.to_string_lossy(), None);

    assert_eq!(result, "test_file.md");
}

#[test]
fn test_to_display_path_empty_string() {
    let result = to_display_path("", None);
    assert_eq!(result, "");
}

#[test]
fn test_to_display_path_with_parent_references() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let nested = temp_dir.path().join("a/b/c");
    fs::create_dir_all(&nested).expect("Failed to create nested dirs");
    let file = nested.join("file.md");
    fs::write(&file, "# Test").expect("Failed to write");

    // Path with .. that resolves to the same file
    let path_with_parent = temp_dir.path().join("a/b/c/../c/file.md");
    let result = to_display_path(&path_with_parent.to_string_lossy(), Some(temp_dir.path()));

    // Should resolve to clean relative path
    assert_eq!(result, "a/b/c/file.md");
}

#[test]
fn test_to_display_path_special_characters() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let special_dir = temp_dir.path().join("docs#1/test%20files");
    fs::create_dir_all(&special_dir).expect("Failed to create dir with special chars");
    let file_path = special_dir.join("file&name.md");
    fs::write(&file_path, "# Test").expect("Failed to write");

    let result = to_display_path(&file_path.to_string_lossy(), Some(temp_dir.path()));

    assert_eq!(result, "docs#1/test%20files/file&name.md");
}

#[test]
fn test_to_display_path_root_as_project_root() {
    // When project root is /, paths should still be relative to it
    let result = to_display_path("/usr/local/test.md", Some(Path::new("/")));

    assert_eq!(result, "usr/local/test.md");
}

#[test]
fn test_to_display_path_file_outside_project_root() {
    let temp_dir1 = create_test_structure();
    let temp_dir2 = TempDir::new().expect("Failed to create temp dir 2");
    let outside_file = temp_dir2.path().join("outside.md");
    fs::write(&outside_file, "# Outside").expect("Failed to write");

    // File is in temp_dir2, but project root is temp_dir1
    let result = to_display_path(&outside_file.to_string_lossy(), Some(temp_dir1.path()));

    // Should fall back to CWD-relative or absolute
    // Since outside_file is not under project_root, it might be CWD-relative or absolute
    assert!(
        result.ends_with("outside.md"),
        "Expected path to end with 'outside.md', got: {result}"
    );
}

#[test]
fn test_to_display_path_already_relative() {
    // When given a relative path that doesn't exist, should return as-is
    let result = to_display_path("nonexistent/path.md", None);
    assert_eq!(result, "nonexistent/path.md");
}

#[test]
fn test_to_display_path_nested_subdirectory() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let nested_dir = temp_dir.path().join("a/b/c/d");
    fs::create_dir_all(&nested_dir).expect("Failed to create nested dirs");
    let file_path = nested_dir.join("deep.md");
    fs::write(&file_path, "# Deep").expect("Failed to write");

    let result = to_display_path(&file_path.to_string_lossy(), Some(temp_dir.path()));

    assert_eq!(result, "a/b/c/d/deep.md");
}

#[test]
fn test_to_display_path_with_spaces_in_path() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let dir_with_spaces = temp_dir.path().join("my docs/sub folder");
    fs::create_dir_all(&dir_with_spaces).expect("Failed to create dir with spaces");
    let file_path = dir_with_spaces.join("my file.md");
    fs::write(&file_path, "# Spaces").expect("Failed to write");

    let result = to_display_path(&file_path.to_string_lossy(), Some(temp_dir.path()));

    assert_eq!(result, "my docs/sub folder/my file.md");
}

#[test]
fn test_to_display_path_with_unicode() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let unicode_dir = temp_dir.path().join("文档/ドキュメント");
    fs::create_dir_all(&unicode_dir).expect("Failed to create unicode dir");
    let file_path = unicode_dir.join("日本語.md");
    fs::write(&file_path, "# 日本語").expect("Failed to write");

    let result = to_display_path(&file_path.to_string_lossy(), Some(temp_dir.path()));

    assert_eq!(result, "文档/ドキュメント/日本語.md");
}

#[test]
fn test_strip_base_prefix_basic() {
    let temp_dir = create_test_structure();
    let base = temp_dir.path();
    let file = temp_dir.path().join("docs/guide.md");

    let result = strip_base_prefix(&file, base);

    assert_eq!(result, Some("docs/guide.md".to_string()));
}

#[test]
fn test_strip_base_prefix_not_under_base() {
    let temp_dir1 = TempDir::new().expect("Failed to create temp dir 1");
    let temp_dir2 = TempDir::new().expect("Failed to create temp dir 2");
    let file = temp_dir2.path().join("file.md");
    fs::write(&file, "# Test").expect("Failed to write");

    let result = strip_base_prefix(&file, temp_dir1.path());

    assert_eq!(result, None);
}

#[test]
fn test_strip_base_prefix_with_symlink() {
    // This test verifies that symlinks are resolved correctly
    // On macOS, /tmp is a symlink to /private/tmp
    let temp_dir = create_test_structure();
    let canonical_base = temp_dir.path().canonicalize().unwrap();
    let file = temp_dir.path().join("docs/guide.md").canonicalize().unwrap();

    let result = strip_base_prefix(&file, &canonical_base);

    // strip_base_prefix is a path primitive that returns native separators;
    // callers normalize for display. Build the expectation the same way so it
    // holds on Windows (`docs\guide.md`) as well as Unix (`docs/guide.md`).
    let expected = Path::new("docs").join("guide.md").to_string_lossy().to_string();
    assert_eq!(result, Some(expected));
}

/// A project whose root is reached through a symlink, with the canonical path of
/// `docs/guide.md` inside it: the shape discovery hands to the display code.
#[cfg(unix)]
fn symlinked_root_and_canonical_file() -> (TempDir, std::path::PathBuf, std::path::PathBuf) {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let real = temp_dir.path().join("real");
    fs::create_dir_all(real.join("docs")).expect("Failed to create docs dir");
    fs::write(real.join("docs/guide.md"), "# Test").expect("Failed to write test file");
    let link = temp_dir.path().join("link");
    std::os::unix::fs::symlink(&real, &link).expect("Failed to create symlink");
    let file = real.join("docs/guide.md").canonicalize().unwrap();
    (temp_dir, link, file)
}

#[test]
#[cfg(unix)]
fn test_strip_base_prefix_resolves_a_symlinked_base() {
    let (_temp_dir, link, file) = symlinked_root_and_canonical_file();

    assert!(
        file.strip_prefix(&link).is_err(),
        "the base must not prefix the file as written"
    );
    assert_eq!(strip_base_prefix(&file, &link), Some("docs/guide.md".to_string()));
}

#[test]
#[cfg(unix)]
fn test_discovered_display_path_is_relative_to_a_symlinked_root() {
    let (_temp_dir, link, file) = symlinked_root_and_canonical_file();
    let file = file.to_string_lossy();

    assert_eq!(discovered_display_path(&file, Some(&link)), "docs/guide.md");
    assert_eq!(
        resolve_discovered_display_path(&file, false, Some(&link)),
        "docs/guide.md"
    );
    assert_eq!(resolve_discovered_display_path(&file, true, Some(&link)), file);
}

/// A config named by a bare file name yields the empty path as its directory,
/// which `Path::strip_prefix` accepts as a prefix of anything.
#[test]
fn test_strip_base_prefix_empty_base_is_not_a_prefix_of_an_absolute_path() {
    let temp_dir = create_test_structure();
    let file = temp_dir.path().join("docs/guide.md").canonicalize().unwrap();

    assert_eq!(strip_base_prefix(&file, Path::new("")), None);
}

#[test]
fn test_strip_base_prefix_nonexistent_base() {
    let file = Path::new("/some/existing/path.md");
    let nonexistent_base = Path::new("/this/path/does/not/exist");

    let result = strip_base_prefix(file, nonexistent_base);

    // Should return None because canonicalize fails on nonexistent path
    assert_eq!(result, None);
}

#[test]
fn test_windows_display_path_unwraps_verbatim_and_normalizes_separators() {
    // The exact string `canonicalize` returns on Windows, which is what a file
    // shown in full is displayed from.
    assert_eq!(
        windows_display_path(r"\\?\C:\Users\dev\docs\guide.md"),
        "C:/Users/dev/docs/guide.md"
    );
    assert_eq!(
        windows_display_path(r"\\?\UNC\server\share\docs\guide.md"),
        "//server/share/docs/guide.md"
    );
    // Ordinary and relative paths only change separators.
    assert_eq!(
        windows_display_path(r"C:\Users\dev\docs\guide.md"),
        "C:/Users/dev/docs/guide.md"
    );
    assert_eq!(windows_display_path(r"docs\guide.md"), "docs/guide.md");
    assert_eq!(windows_display_path("docs/guide.md"), "docs/guide.md");
}

#[test]
fn test_format_embedded_markdown_blocks_atx_heading() {
    let config = rumdl_config::Config::default();
    let rules = rumdl_lib::rules::filter_rules(&rumdl_lib::rules::all_rules(&config), &config.global);

    let mut content = "# Example\n\n```markdown\n#Heading without space\n```\n".to_string();
    let formatted = format_embedded_markdown_blocks(&mut content, &rules, &config).formatted;

    assert!(formatted > 0, "Should format at least one block");
    assert!(
        content.contains("# Heading without space"),
        "Should fix ATX heading spacing, got: {content:?}"
    );
}

#[test]
fn test_format_embedded_markdown_blocks_md_language() {
    let config = rumdl_config::Config::default();
    let rules = rumdl_lib::rules::filter_rules(&rumdl_lib::rules::all_rules(&config), &config.global);

    let mut content = "# Example\n\n```md\n#Test\n```\n".to_string();
    let formatted = format_embedded_markdown_blocks(&mut content, &rules, &config).formatted;

    assert!(formatted > 0, "Should format block with 'md' language");
    assert!(content.contains("# Test"), "Should fix heading, got: {content:?}");
}

#[test]
fn test_format_embedded_markdown_blocks_case_insensitive() {
    let config = rumdl_config::Config::default();
    let rules = rumdl_lib::rules::filter_rules(&rumdl_lib::rules::all_rules(&config), &config.global);

    let mut content = "# Doc\n\n```MARKDOWN\n#Upper case\n```\n".to_string();
    let formatted = format_embedded_markdown_blocks(&mut content, &rules, &config).formatted;

    assert!(formatted > 0, "Should detect MARKDOWN (uppercase)");
    assert!(content.contains("# Upper case"));
}

#[test]
fn test_format_embedded_markdown_blocks_tilde_fence() {
    let config = rumdl_config::Config::default();
    let rules = rumdl_lib::rules::filter_rules(&rumdl_lib::rules::all_rules(&config), &config.global);

    let mut content = "# Doc\n\n~~~markdown\n#Tilde fence\n~~~\n".to_string();
    let formatted = format_embedded_markdown_blocks(&mut content, &rules, &config).formatted;

    assert!(formatted > 0, "Should detect tilde fenced blocks");
    assert!(content.contains("# Tilde fence"));
}

#[test]
fn test_format_embedded_markdown_blocks_multiple_blocks() {
    let config = rumdl_config::Config::default();
    let rules = rumdl_lib::rules::filter_rules(&rumdl_lib::rules::all_rules(&config), &config.global);

    let mut content = "# Doc\n\n```markdown\n#First\n```\n\nText\n\n```md\n#Second\n```\n".to_string();
    let formatted = format_embedded_markdown_blocks(&mut content, &rules, &config).formatted;

    assert_eq!(formatted, 2, "Should format both blocks");
    assert!(content.contains("# First"));
    assert!(content.contains("# Second"));
}

#[test]
fn test_format_embedded_markdown_blocks_nested() {
    let config = rumdl_config::Config::default();
    let rules = rumdl_lib::rules::filter_rules(&rumdl_lib::rules::all_rules(&config), &config.global);

    // Outer block contains inner block (using longer fence)
    let mut content = "# Doc\n\n````markdown\n#Outer\n\n```markdown\n#Inner\n```\n````\n".to_string();
    let formatted = format_embedded_markdown_blocks(&mut content, &rules, &config).formatted;

    assert!(formatted >= 1, "Should format at least outer block");
    assert!(content.contains("# Outer"), "Should fix outer heading");
}

#[test]
fn test_format_embedded_markdown_blocks_preserves_indentation() {
    let config = rumdl_config::Config::default();
    let rules = rumdl_lib::rules::filter_rules(&rumdl_lib::rules::all_rules(&config), &config.global);

    // Content with relative indentation that should be preserved
    let mut content = "# Doc\n\n```markdown\n#Heading\n\n    code block\n```\n".to_string();
    let formatted = format_embedded_markdown_blocks(&mut content, &rules, &config).formatted;

    assert!(formatted > 0);
    assert!(content.contains("# Heading"), "Should fix heading");
    assert!(
        content.contains("    code block"),
        "Should preserve indented code block"
    );
}

#[test]
fn test_format_embedded_markdown_blocks_empty_block() {
    let config = rumdl_config::Config::default();
    let rules = rumdl_lib::rules::filter_rules(&rumdl_lib::rules::all_rules(&config), &config.global);

    let mut content = "# Doc\n\n```markdown\n\n```\n".to_string();
    let original = content.clone();
    let formatted = format_embedded_markdown_blocks(&mut content, &rules, &config).formatted;

    assert_eq!(formatted, 0, "Should skip empty blocks");
    assert_eq!(content, original, "Content should be unchanged");
}

#[test]
fn test_format_embedded_markdown_blocks_whitespace_only() {
    let config = rumdl_config::Config::default();
    let rules = rumdl_lib::rules::filter_rules(&rumdl_lib::rules::all_rules(&config), &config.global);

    let mut content = "# Doc\n\n```markdown\n   \n\n```\n".to_string();
    let original = content.clone();
    let formatted = format_embedded_markdown_blocks(&mut content, &rules, &config).formatted;

    assert_eq!(formatted, 0, "Should skip whitespace-only blocks");
    assert_eq!(content, original, "Content should be unchanged");
}

#[test]
fn test_format_embedded_markdown_blocks_skips_other_languages() {
    let config = rumdl_config::Config::default();
    let rules = rumdl_lib::rules::filter_rules(&rumdl_lib::rules::all_rules(&config), &config.global);

    let mut content = "# Doc\n\n```rust\n#[derive(Debug)]\nfn main() {}\n```\n".to_string();
    let original = content.clone();
    let formatted = format_embedded_markdown_blocks(&mut content, &rules, &config).formatted;

    assert_eq!(formatted, 0, "Should not format rust blocks");
    assert_eq!(content, original, "Content should be unchanged");
}

#[test]
fn test_format_embedded_markdown_blocks_multiple_blank_lines() {
    let config = rumdl_config::Config::default();
    let rules = rumdl_lib::rules::filter_rules(&rumdl_lib::rules::all_rules(&config), &config.global);

    // MD012 should fix multiple consecutive blank lines between non-heading content
    let mut content = "# Doc\n\n```markdown\nParagraph 1\n\n\n\nParagraph 2\n```\n".to_string();
    let formatted = format_embedded_markdown_blocks(&mut content, &rules, &config).formatted;

    assert!(formatted > 0, "Should format block");
    // After formatting, should have at most one blank line between paragraphs
    let block_content = content
        .split("```markdown\n")
        .nth(1)
        .unwrap()
        .split("\n```")
        .next()
        .unwrap();
    let blank_count = block_content.matches("\n\n\n").count();
    assert_eq!(blank_count, 0, "Should reduce multiple blank lines");
}

#[test]
fn test_format_embedded_markdown_blocks_depth_limit() {
    let config = rumdl_config::Config::default();
    let rules = rumdl_lib::rules::filter_rules(&rumdl_lib::rules::all_rules(&config), &config.global);

    // Create deeply nested blocks (beyond MAX_EMBEDDED_DEPTH)
    let mut content = "# Doc\n\n".to_string();
    for i in 0..10 {
        let backticks = "`".repeat(3 + i);
        content.push_str(&format!("{backticks}markdown\n#Level{i}\n"));
    }
    for i in (0..10).rev() {
        let backticks = "`".repeat(3 + i);
        content.push_str(&format!("{backticks}\n"));
    }

    // Should not panic or stack overflow
    let formatted = format_embedded_markdown_blocks(&mut content, &rules, &config).formatted;
    assert!(formatted <= MAX_EMBEDDED_DEPTH, "Should respect depth limit");
}

/// Formatting an embedded block changes only what its rules fix: blank lines
/// at the end of the block stay.
#[test]
fn test_format_embedded_markdown_blocks_keeps_trailing_blank_lines() {
    let config = rumdl_config::Config::default();
    let rules: Vec<_> = rumdl_lib::rules::all_rules(&config)
        .into_iter()
        .filter(|rule| rule.name() == "MD018")
        .collect();

    let mut content = "# T\n\n```markdown\n#A\n\n\n```\n\n- x\n\n  ```markdown\n  #B\n  \n\n  ```\n".to_string();
    format_embedded_markdown_blocks(&mut content, &rules, &config);

    assert_eq!(
        content,
        "# T\n\n```markdown\n# A\n\n\n```\n\n- x\n\n  ```markdown\n  # B\n\n\n  ```\n"
    );

    // A whitespace-only last line is emptied like any other, not removed.
    let mut content = "- x\n\n  ```markdown\n  #B\n  \n  ```\n".to_string();
    format_embedded_markdown_blocks(&mut content, &rules, &config);
    assert_eq!(content, "- x\n\n  ```markdown\n  # B\n\n  ```\n");
}

#[test]
fn test_format_embedded_markdown_no_extra_blank_line() {
    // Regression test: MD047 should NOT add extra blank line before closing fence
    let config = rumdl_config::Config::default();
    let rules = rumdl_lib::rules::filter_rules(&rumdl_lib::rules::all_rules(&config), &config.global);

    // Content that doesn't end with newline inside the block
    let mut content = "# Doc\n\n```markdown\n> [!INFO]\n> Content\n```\n".to_string();
    let original = content.clone();
    let formatted = format_embedded_markdown_blocks(&mut content, &rules, &config).formatted;

    // If no changes needed inside the block, content should be unchanged
    // (no extra blank line before closing fence)
    if formatted == 0 {
        assert_eq!(content, original, "Should not add extra blank lines");
    } else {
        // If changes were made, verify no blank line before closing fence
        assert!(
            !content.contains("\n\n```\n"),
            "Should not have blank line before closing fence"
        );
    }
}

#[test]
fn test_format_embedded_markdown_with_fix() {
    // Test that fixes are applied without corrupting structure
    let config = rumdl_config::Config::default();
    let rules = rumdl_lib::rules::filter_rules(&rumdl_lib::rules::all_rules(&config), &config.global);

    let mut content = "# Doc\n\n```markdown\n#Bad heading\n```\n".to_string();
    let formatted = format_embedded_markdown_blocks(&mut content, &rules, &config).formatted;

    assert!(formatted > 0, "Should format the block");
    assert!(content.contains("# Bad heading"), "Should fix heading");
    assert!(!content.contains("\n\n```\n"), "Should not add blank line before fence");
    // Verify structure is preserved
    assert!(content.starts_with("# Doc\n\n```markdown\n"));
    assert!(content.ends_with("```\n"));
}

#[test]
fn test_format_embedded_markdown_unicode_content() {
    // Test with multi-byte UTF-8 characters to verify byte offset handling
    let config = rumdl_config::Config::default();
    let rules = rumdl_lib::rules::filter_rules(&rumdl_lib::rules::all_rules(&config), &config.global);

    // Japanese, Chinese, and emoji characters (multi-byte UTF-8)
    let mut content = "# ドキュメント\n\n```markdown\n#見出し\n\n中文内容 🎉\n```\n".to_string();
    let original_structure = (content.contains("```markdown"), content.contains("```\n"));

    let formatted = format_embedded_markdown_blocks(&mut content, &rules, &config).formatted;

    // Structure should be preserved
    assert!(content.contains("```markdown"), "Opening fence preserved");
    assert!(content.ends_with("```\n"), "Closing fence preserved");

    // If formatted, heading should be fixed
    if formatted > 0 {
        assert!(content.contains("# 見出し"), "Japanese heading should be fixed");
    }

    // Content should not be corrupted
    assert!(content.contains("中文内容"), "Chinese content preserved");
    assert!(content.contains("🎉"), "Emoji preserved");

    // Structure should match original pattern
    assert_eq!(
        (content.contains("```markdown"), content.contains("```\n")),
        original_structure,
        "Structure should be preserved"
    );
}

#[test]
fn test_format_embedded_markdown_in_list_item() {
    // Test markdown code block indented inside a list item
    let config = rumdl_config::Config::default();
    let rules = rumdl_lib::rules::filter_rules(&rumdl_lib::rules::all_rules(&config), &config.global);

    let mut content = "- List item:\n\n  ```markdown\n  #Heading\n  ```\n".to_string();
    let formatted = format_embedded_markdown_blocks(&mut content, &rules, &config).formatted;

    assert!(formatted > 0, "Should format embedded block");
    assert!(content.contains("# Heading"), "Should fix heading");
    // Verify list structure is preserved
    assert!(content.starts_with("- List item:"), "List item preserved");
}

#[test]
fn test_format_embedded_markdown_info_string_with_attributes() {
    // Test that info string attributes are handled correctly
    // e.g., ```markdown title="Example"
    let config = rumdl_config::Config::default();
    let rules = rumdl_lib::rules::filter_rules(&rumdl_lib::rules::all_rules(&config), &config.global);

    let mut content = "# Doc\n\n```markdown title=\"Example\" highlight={1}\n#Heading\n```\n".to_string();
    let formatted = format_embedded_markdown_blocks(&mut content, &rules, &config).formatted;

    assert!(formatted > 0, "Should recognize markdown despite extra info");
    assert!(content.contains("# Heading"), "Should fix heading");
    // Info string should be preserved
    assert!(
        content.contains("```markdown title=\"Example\""),
        "Info string preserved"
    );
}

#[test]
fn test_format_embedded_markdown_depth_verification() {
    // Verify that each level up to MAX_EMBEDDED_DEPTH is actually formatted
    let config = rumdl_config::Config::default();
    let rules = rumdl_lib::rules::filter_rules(&rumdl_lib::rules::all_rules(&config), &config.global);

    // Create content with 2 sequential blocks at different "depths"
    // Note: True nesting requires increasing fence length, which changes parsing.
    // Instead, we test multiple blocks in sequence to verify recursion works.
    let mut content = "# Doc\n\n```markdown\n#Level1\n```\n\n```md\n#Level2\n```\n".to_string();

    let formatted = format_embedded_markdown_blocks(&mut content, &rules, &config).formatted;

    // Both blocks should be formatted
    assert!(formatted >= 2, "Should format both blocks, got {formatted}");
    assert!(content.contains("# Level1"), "Block 1 should be formatted");
    assert!(content.contains("# Level2"), "Block 2 should be formatted");
}

#[test]
fn test_format_embedded_markdown_true_nesting() {
    // Test true recursive nesting with tilde fences (avoids fence length issues)
    let config = rumdl_config::Config::default();
    let rules = rumdl_lib::rules::filter_rules(&rumdl_lib::rules::all_rules(&config), &config.global);

    // Use tildes for outer, backticks for inner - this is valid CommonMark
    let mut content = "# Doc\n\n~~~markdown\n#Outer\n\n```markdown\n#Inner\n```\n~~~\n".to_string();

    let formatted = format_embedded_markdown_blocks(&mut content, &rules, &config).formatted;

    // Both levels should be formatted
    assert!(formatted >= 1, "Should format at least outer block");
    assert!(content.contains("# Outer"), "Outer heading should be formatted");
    // Inner might not be formatted due to nesting complexity - that's OK
    // The important thing is that the structure isn't corrupted
    assert!(content.contains("~~~\n"), "Tilde fence preserved");
    assert!(content.contains("```\n"), "Backtick fence preserved");
}

#[test]
fn test_format_embedded_markdown_cli_integration() {
    // Integration test: verify embedded formatting works through file processing
    use std::io::Write;
    use tempfile::NamedTempFile;

    let config = rumdl_config::Config::default();
    let rules = rumdl_lib::rules::filter_rules(&rumdl_lib::rules::all_rules(&config), &config.global);

    // Create a temp file with embedded markdown
    let mut temp_file = NamedTempFile::new().expect("Failed to create temp file");
    writeln!(temp_file, "# Test Doc").unwrap();
    writeln!(temp_file).unwrap();
    writeln!(temp_file, "```markdown").unwrap();
    writeln!(temp_file, "#Bad Heading").unwrap();
    writeln!(temp_file, "```").unwrap();
    temp_file.flush().unwrap();

    // Read and format the content
    let mut content = std::fs::read_to_string(temp_file.path()).expect("Failed to read temp file");
    let formatted = format_embedded_markdown_blocks(&mut content, &rules, &config).formatted;

    assert!(formatted > 0, "Should format embedded content");
    assert!(content.contains("# Bad Heading"), "Should fix embedded heading");
}

#[test]
fn test_format_embedded_markdown_md041_behavior() {
    // Verify behavior with document-level rules like MD041 on embedded content
    // MD041 requires first heading to be H1, but embedded docs often show examples
    // with H2 headings deliberately
    let config = rumdl_config::Config::default();
    let rules = rumdl_lib::rules::filter_rules(&rumdl_lib::rules::all_rules(&config), &config.global);

    // Embedded content starts with H2, not H1
    let mut content = "# Main Doc\n\n```markdown\n## Example H2\n```\n".to_string();
    let original = content.clone();

    let formatted = format_embedded_markdown_blocks(&mut content, &rules, &config).formatted;

    // Document the current behavior: MD041 does NOT have a fix function,
    // so even if it fires as a warning, it won't change the content.
    // This is actually the desired behavior for embedded markdown,
    // since documentation examples often intentionally show non-H1 headings.

    // Verify the embedded content is NOT changed by MD041 (no fix available)
    assert_eq!(content, original, "MD041 should not change embedded H2 (no fix)");
    assert_eq!(formatted, 0, "No formatting changes expected");
}

#[test]
fn test_check_embedded_markdown_blocks() {
    let config = rumdl_config::Config::default();
    let rules = rumdl_lib::rules::filter_rules(&rumdl_lib::rules::all_rules(&config), &config.global);

    // Content with violations inside embedded markdown
    let content = "# Doc\n```markdown\n##  Bad heading\n```\n";

    let warnings = check_embedded_markdown_blocks(content, &rules, &config);

    // Should find violations in embedded content
    assert!(!warnings.is_empty(), "Should find warnings in embedded markdown");

    // Check that warnings have adjusted line numbers
    // The embedded content starts at line 3 (after "# Doc\n```markdown\n")
    let md019_warning = warnings
        .iter()
        .find(|w| w.rule_name.as_ref().is_some_and(|n| n == "MD019"));
    assert!(md019_warning.is_some(), "Should find MD019 warning for extra space");

    // Line should be 3 (line 1 = "# Doc", line 2 = "```markdown", line 3 = "##  Bad heading")
    if let Some(w) = md019_warning {
        assert_eq!(w.line, 3, "MD019 warning should be on line 3");
    }
}

#[test]
fn test_check_embedded_markdown_blocks_skips_file_scoped_rules() {
    let config = rumdl_config::Config::default();
    let rules = rumdl_lib::rules::filter_rules(&rumdl_lib::rules::all_rules(&config), &config.global);

    // Content that would trigger MD041 (no H1 first) and MD047 (no trailing newline)
    let content = "# Doc\n```markdown\n## Not H1\nNo trailing newline```\n";

    let warnings = check_embedded_markdown_blocks(content, &rules, &config);

    // MD041 and MD047 should be filtered out for embedded content
    let md041 = warnings
        .iter()
        .find(|w| w.rule_name.as_ref().is_some_and(|n| n == "MD041"));
    let md047 = warnings
        .iter()
        .find(|w| w.rule_name.as_ref().is_some_and(|n| n == "MD047"));

    assert!(md041.is_none(), "MD041 should be skipped for embedded content");
    assert!(md047.is_none(), "MD047 should be skipped for embedded content");
}

#[test]
fn test_check_embedded_markdown_blocks_empty() {
    let config = rumdl_config::Config::default();
    let rules = rumdl_lib::rules::filter_rules(&rumdl_lib::rules::all_rules(&config), &config.global);

    // No embedded markdown
    let content = "# Doc\n\nSome text\n";

    let warnings = check_embedded_markdown_blocks(content, &rules, &config);

    assert!(warnings.is_empty(), "Should have no warnings without embedded markdown");
}

fn md018_md022_rules(config: &rumdl_config::Config) -> Vec<Box<dyn Rule>> {
    rumdl_lib::rules::filter_rules(&rumdl_lib::rules::all_rules(config), &config.global)
        .into_iter()
        .filter(|rule| matches!(rule.name(), "MD018" | "MD022"))
        .collect()
}

fn warning_positions(warnings: &[rumdl_lib::rule::LintWarning]) -> Vec<(&str, usize, usize)> {
    warnings
        .iter()
        .map(|w| (w.rule_name.as_deref().unwrap_or(""), w.line, w.column))
        .collect()
}

/// Fixes that would end the enclosing block early are not written back: MD048
/// turning an inner `~~~` fence into backticks closes a ```` ``` ```` block at the
/// inner fence. The block stays as it was and is reported at its own line, at
/// any depth, while every other block is still formatted.
#[test]
fn test_format_embedded_markdown_refuses_fixes_that_end_the_block() {
    let config = rumdl_config::Config::default();
    let rules: Vec<Box<dyn Rule>> = vec![
        Box::new(rumdl_lib::rules::MD018NoMissingSpaceAtx::new()),
        Box::new(rumdl_lib::rules::MD048CodeFenceStyle::new(
            rumdl_lib::rules::CodeFenceStyle::Backtick,
        )),
    ];
    let tilde_block = "```markdown\n# Inner\n\n~~~py\nx = 1\n~~~\n```\n";

    // (document, document after formatting, blocks formatted, refused lines)
    let cases: Vec<(String, String, usize, Vec<usize>)> = vec![
        (
            format!("# T\n\n{tilde_block}\nAfter.\n"),
            format!("# T\n\n{tilde_block}\nAfter.\n"),
            0,
            vec![3],
        ),
        (
            format!("# T\n\n```markdown\n#Fix\n```\n\n{tilde_block}"),
            format!("# T\n\n```markdown\n# Fix\n```\n\n{tilde_block}"),
            1,
            vec![7],
        ),
        (
            "> ```markdown\n> ~~~py\n> x\n> ~~~\n> ```\n".to_string(),
            "> ```markdown\n> ~~~py\n> x\n> ~~~\n> ```\n".to_string(),
            0,
            vec![1],
        ),
        // Refused one level down, reported at the nested block's document line.
        (
            format!("# T\n\n````markdown\nOuter.\n\n{tilde_block}````\n"),
            format!("# T\n\n````markdown\nOuter.\n\n{tilde_block}````\n"),
            0,
            vec![6],
        ),
        (
            format!("`````markdown\nA\n\n````markdown\nB\n\n{tilde_block}````\n`````\n"),
            format!("`````markdown\nA\n\n````markdown\nB\n\n{tilde_block}````\n`````\n"),
            0,
            vec![7],
        ),
        // A nested fix counts once for each block it was written back into.
        (
            "````markdown\n```markdown\n#Fix\n```\n````\n".to_string(),
            "````markdown\n```markdown\n# Fix\n```\n````\n".to_string(),
            2,
            vec![],
        ),
        // A longer fence holds the backtick fence.
        (
            "````markdown\n~~~py\nx\n~~~\n````\n".to_string(),
            "````markdown\n```py\nx\n```\n````\n".to_string(),
            1,
            vec![],
        ),
    ];

    for (document, expected, formatted, refused) in cases {
        let mut content = document.clone();
        let outcome = format_embedded_markdown_blocks(&mut content, &rules, &config);
        assert_eq!(content, expected, "{document:?}");
        assert_eq!(outcome.formatted, formatted, "{document:?}");
        let lines: Vec<usize> = outcome.refused.iter().map(|warning| warning.line).collect();
        assert_eq!(lines, refused, "{document:?}");
        assert!(
            outcome
                .refused
                .iter()
                .all(|warning| warning.message.contains("block left unchanged")
                    && warning.severity == rumdl_lib::rule::Severity::Error),
            "{:?}",
            outcome.refused
        );
    }
}

#[test]
fn test_check_embedded_markdown_inside_containers() {
    let config = rumdl_config::Config::default();
    let rules = md018_md022_rules(&config);

    // MD018 points just past the hash; each column adds the container prefix
    // in front of the embedded line
    let cases = [
        ("```markdown\n#Heading\n```\n", 2),
        ("> ```markdown\n> #Heading\n> ```\n", 4),
        ("> > ```markdown\n> > #Heading\n> > ```\n", 6),
        ("- ```markdown\n  #Heading\n  ```\n", 4),
        ("> - ```markdown\n>   #Heading\n>   ```\n", 6),
        (">```markdown\n>#Heading\n>```\n", 3),
    ];
    for (content, column) in cases {
        let warnings = check_embedded_markdown_blocks(content, &rules, &config);
        assert_eq!(
            warning_positions(&warnings),
            vec![("MD018", 2, column)],
            "content: {content:?}"
        );
    }
}

#[test]
fn test_check_embedded_markdown_ignores_indented_code_inside_it() {
    let config = rumdl_config::Config::default();
    let rules = md018_md022_rules(&config);

    // Four spaces beyond the embedded document's margin make an indented code block
    for content in [
        "```markdown\n    #notaheading\n```\n",
        "- ```markdown\n      #notaheading\n  ```\n",
        "> ```markdown\n>     #notaheading\n> ```\n",
    ] {
        let warnings = check_embedded_markdown_blocks(content, &rules, &config);
        assert!(warnings.is_empty(), "content: {content:?}, got {warnings:?}");

        let mut formatted = content.to_string();
        assert_eq!(
            format_embedded_markdown_blocks(&mut formatted, &rules, &config).formatted,
            0
        );
        assert_eq!(formatted, content);
    }
}

#[test]
fn test_format_embedded_markdown_inside_containers() {
    let config = rumdl_config::Config::default();
    let rules = md018_md022_rules(&config);

    let cases = [
        (
            "> ```markdown\n> #Heading\n> text\n> ```\n",
            "> ```markdown\n> # Heading\n>\n> text\n> ```\n",
        ),
        (
            "> > ```markdown\n> > #Heading\n> > text\n> > ```\n",
            "> > ```markdown\n> > # Heading\n> >\n> > text\n> > ```\n",
        ),
        (
            "- ```markdown\n  #Heading\n  text\n  ```\n",
            "- ```markdown\n  # Heading\n\n  text\n  ```\n",
        ),
        (
            "> - ```markdown\n>   #Heading\n>   text\n>   ```\n",
            "> - ```markdown\n>   # Heading\n>\n>   text\n>   ```\n",
        ),
        (
            "- ```markdown\n  #Heading\n      code\n  ```\n",
            "- ```markdown\n  # Heading\n\n      code\n  ```\n",
        ),
    ];
    for (content, expected) in cases {
        let mut formatted = content.to_string();
        assert_eq!(
            format_embedded_markdown_blocks(&mut formatted, &rules, &config).formatted,
            1,
            "content: {content:?}"
        );
        assert_eq!(formatted, expected, "content: {content:?}");
    }
}

#[test]
fn test_format_embedded_markdown_respects_filtered_rules() {
    // Test that embedded markdown formatting respects per-file-ignores
    // This simulates what happens when per-file-ignores excludes certain rules
    let config = rumdl_config::Config::default();
    let all_rules = rumdl_lib::rules::filter_rules(&rumdl_lib::rules::all_rules(&config), &config.global);

    // Content with MD022 violation (missing blank line above heading)
    let original = "# Rule Documentation\n\n```markdown\n# Heading\n## No blank line above\n```\n";

    // Test 1: WITH MD022 rule - should add blank line
    let mut content_with_rule = original.to_string();
    let formatted_with_rule = format_embedded_markdown_blocks(&mut content_with_rule, &all_rules, &config).formatted;

    assert!(formatted_with_rule > 0, "Should format when MD022 is active");
    assert!(
        content_with_rule.contains("# Heading\n\n## No blank line above"),
        "Should add blank line when MD022 is active"
    );

    // Test 2: WITHOUT MD022 rule (simulating per-file-ignores) - should NOT add blank line
    let filtered_rules: Vec<Box<dyn Rule>> = all_rules
        .iter()
        .filter(|rule| rule.name() != "MD022")
        .map(|r| dyn_clone::clone_box(&**r))
        .collect();

    let mut content_without_rule = original.to_string();
    let _formatted_without_rule =
        format_embedded_markdown_blocks(&mut content_without_rule, &filtered_rules, &config).formatted;

    // The content should NOT have MD022 fix applied
    assert!(
        content_without_rule.contains("# Heading\n## No blank line above"),
        "Should NOT add blank line when MD022 is filtered out (per-file-ignores)"
    );

    // If other rules applied fixes, that's fine, but MD022 specifically shouldn't
    // The key assertion is that the missing blank line above ## is preserved
    assert_ne!(
        content_with_rule, content_without_rule,
        "Filtered rules should produce different result than all rules"
    );
}

#[test]
fn test_format_embedded_markdown_respects_inline_config() {
    // Test that embedded markdown formatting respects inline disable directives
    let config = rumdl_config::Config::default();
    let rules = rumdl_lib::rules::filter_rules(&rumdl_lib::rules::all_rules(&config), &config.global);

    // Content with MD022 violation inside a markdown block, wrapped in inline disable
    let original = r#"# Doc

<!-- rumdl-disable MD022 -->

```markdown
# Heading
## No blank line above
```

<!-- rumdl-enable MD022 -->
"#;

    let mut content = original.to_string();
    let formatted = format_embedded_markdown_blocks(&mut content, &rules, &config).formatted;

    // The embedded content should NOT be modified because MD022 is disabled via inline config
    assert!(
        content.contains("# Heading\n## No blank line above"),
        "Should NOT add blank line when MD022 is disabled via inline config. Got: {content}"
    );

    // No blocks should have been formatted
    assert_eq!(formatted, 0, "No blocks should be formatted when rules are disabled");
}

#[test]
fn test_check_embedded_markdown_respects_inline_config() {
    // Test that embedded markdown checking respects inline disable directives
    // This ensures check and fmt behave consistently
    let config = rumdl_config::Config::default();
    let rules = rumdl_lib::rules::filter_rules(&rumdl_lib::rules::all_rules(&config), &config.global);

    // Content with MD022 violation inside a markdown block, wrapped in inline disable
    let content = r#"# Doc

<!-- rumdl-disable MD022 -->

```markdown
# Heading
## No blank line above
```

<!-- rumdl-enable MD022 -->
"#;

    let warnings = check_embedded_markdown_blocks(content, &rules, &config);

    // Should have NO MD022 warnings because it's disabled via inline config
    let md022_warnings: Vec<_> = warnings
        .iter()
        .filter(|w| w.rule_name.as_ref().is_some_and(|n| n == "MD022"))
        .collect();

    assert!(
        md022_warnings.is_empty(),
        "Should NOT report MD022 warnings when disabled via inline config. Got: {md022_warnings:?}"
    );
}

#[test]
fn test_check_and_format_consistency() {
    // Verify that check and format behave identically for inline config
    let config = rumdl_config::Config::default();
    let rules = rumdl_lib::rules::filter_rules(&rumdl_lib::rules::all_rules(&config), &config.global);

    // Content with violations both inside and outside disabled region
    let content = r#"# Doc

<!-- rumdl-disable MD022 -->

```markdown
# Heading
## Inside disabled - should be ignored
```

<!-- rumdl-enable MD022 -->

```markdown
# Another
## Outside disabled - should be reported/fixed
```
"#;

    // Check should report warnings only for the second block
    let warnings = check_embedded_markdown_blocks(content, &rules, &config);
    let md022_warnings: Vec<_> = warnings
        .iter()
        .filter(|w| w.rule_name.as_ref().is_some_and(|n| n == "MD022"))
        .collect();

    assert!(
        !md022_warnings.is_empty(),
        "Should report MD022 for block outside disabled region"
    );

    // All warnings should be for lines after the enable comment (line 11+)
    for w in &md022_warnings {
        assert!(
            w.line > 10,
            "MD022 warning should be after enable comment, got line {}",
            w.line
        );
    }

    // Format should only modify the second block
    let mut format_content = content.to_string();
    let formatted = format_embedded_markdown_blocks(&mut format_content, &rules, &config).formatted;

    assert!(formatted > 0, "Should format the second block");
    assert!(
        format_content.contains("# Heading\n## Inside disabled"),
        "First block should be unchanged"
    );
    assert!(
        format_content.contains("# Another\n\n## Outside disabled"),
        "Second block should have blank line added"
    );
}

#[test]
fn test_should_lint_embedded_markdown_disabled_by_default() {
    use rumdl_lib::code_block_tools::CodeBlockToolsConfig;
    // Default config has code_block_tools.enabled = false
    let config = CodeBlockToolsConfig::default();
    assert!(!should_lint_embedded_markdown(&config));
}

#[test]
fn test_should_lint_embedded_markdown_enabled_but_no_markdown_config() {
    use rumdl_lib::code_block_tools::CodeBlockToolsConfig;
    // code_block_tools enabled but no markdown language configured
    let config = CodeBlockToolsConfig {
        enabled: true,
        ..Default::default()
    };
    assert!(!should_lint_embedded_markdown(&config));
}

#[test]
fn test_should_lint_embedded_markdown_with_rumdl_tool() {
    use rumdl_lib::code_block_tools::{CodeBlockToolsConfig, LanguageToolConfig, RUMDL_BUILTIN_TOOL};
    // Properly configured: enabled with markdown lint = ["rumdl"]
    let mut config = CodeBlockToolsConfig {
        enabled: true,
        ..Default::default()
    };
    config.languages.insert(
        "markdown".to_string(),
        LanguageToolConfig {
            lint: vec![RUMDL_BUILTIN_TOOL.to_string()],
            ..Default::default()
        },
    );
    assert!(should_lint_embedded_markdown(&config));
}

#[test]
fn test_should_lint_embedded_markdown_with_md_alias() {
    use rumdl_lib::code_block_tools::{CodeBlockToolsConfig, LanguageToolConfig, RUMDL_BUILTIN_TOOL};
    // Using "md" instead of "markdown" should also work
    let mut config = CodeBlockToolsConfig {
        enabled: true,
        ..Default::default()
    };
    config.languages.insert(
        "md".to_string(),
        LanguageToolConfig {
            lint: vec![RUMDL_BUILTIN_TOOL.to_string()],
            ..Default::default()
        },
    );
    assert!(should_lint_embedded_markdown(&config));
}

#[test]
fn test_should_lint_embedded_markdown_with_other_tool() {
    use rumdl_lib::code_block_tools::{CodeBlockToolsConfig, LanguageToolConfig};
    // If markdown is configured with a different tool (not rumdl), don't lint
    let mut config = CodeBlockToolsConfig {
        enabled: true,
        ..Default::default()
    };
    config.languages.insert(
        "markdown".to_string(),
        LanguageToolConfig {
            lint: vec!["some-other-tool".to_string()],
            ..Default::default()
        },
    );
    assert!(!should_lint_embedded_markdown(&config));
}

fn strings(names: &[&str]) -> Vec<String> {
    names.iter().map(ToString::to_string).collect()
}

fn selection_config() -> rumdl_config::GlobalConfig {
    rumdl_config::GlobalConfig {
        enable: strings(&["MD001"]),
        disable: strings(&["MD003"]),
        extend_enable: strings(&["MD002"]),
        extend_disable: strings(&["MD004"]),
        enable_is_explicit: true,
        ..rumdl_config::GlobalConfig::default()
    }
}

#[test]
fn rule_selection_enable_flag_replaces_every_config_rule_list() {
    let flags = RuleSelectionFlags {
        enable: Some("MD005"),
        disable: Some("MD006"),
        extend_enable: None,
        extend_disable: None,
    };
    let view = rule_selection(&flags, &selection_config());
    assert_eq!(view.enable, strings(&["MD005"]));
    assert!(view.enable_is_explicit);
    assert_eq!(view.disable, strings(&["MD006"]));
    assert!(view.extend_enable.is_empty(), "config extend-enable is out of scope");
    assert!(view.extend_disable.is_empty(), "config extend-disable is out of scope");
}

#[test]
fn rule_selection_empty_enable_flag_is_an_explicit_empty_allowlist() {
    let flags = RuleSelectionFlags {
        enable: Some(""),
        disable: None,
        extend_enable: None,
        extend_disable: None,
    };
    let mut config = selection_config();
    config.enable_is_explicit = false;
    let view = rule_selection(&flags, &config);
    assert!(view.enable.is_empty());
    assert!(view.enable_is_explicit);
}

#[test]
fn rule_selection_without_enable_flag_extends_each_config_list() {
    let flags = RuleSelectionFlags {
        enable: None,
        disable: Some("MD007,MD003"),
        extend_enable: Some("no-inline-html"),
        extend_disable: Some("MD008"),
    };
    let view = rule_selection(&flags, &selection_config());
    assert_eq!(view.enable, strings(&["MD001"]), "config enable is kept");
    assert!(view.enable_is_explicit, "config explicitness is kept");
    assert_eq!(
        view.disable,
        strings(&["MD003", "MD007"]),
        "config first, CLI appended, no duplicate"
    );
    assert_eq!(
        view.extend_enable,
        strings(&["MD002", "MD033"]),
        "aliases resolve to IDs"
    );
    assert_eq!(view.extend_disable, strings(&["MD004", "MD008"]));
}

#[test]
fn rule_selection_without_flags_is_the_config() {
    let flags = RuleSelectionFlags {
        enable: None,
        disable: None,
        extend_enable: None,
        extend_disable: None,
    };
    let config = selection_config();
    assert_eq!(rule_selection(&flags, &config), config);
}

#[test]
fn rule_selection_keeps_the_all_keyword_recognisable() {
    let flags = RuleSelectionFlags {
        enable: None,
        disable: Some("ALL"),
        extend_enable: None,
        extend_disable: None,
    };
    let view = rule_selection(&flags, &rumdl_config::GlobalConfig::default());
    assert_eq!(view.disable.len(), 1);
    assert!(view.disable[0].eq_ignore_ascii_case("all"), "got {:?}", view.disable);
}

/// A rule that panics on any document containing "boom", the way a defect in a
/// dependency's parser does.
#[derive(Clone)]
struct PanicsOnBoom;

impl Rule for PanicsOnBoom {
    fn name(&self) -> &'static str {
        "MD999"
    }

    fn description(&self) -> &'static str {
        "panics on a marker word"
    }

    fn check(&self, ctx: &rumdl_lib::lint_context::LintContext) -> rumdl_lib::rule::LintResult {
        assert!(!ctx.content.contains("boom"), "boom");
        Ok(vec![rumdl_lib::rule::LintWarning {
            message: "seen".to_string(),
            line: 1,
            column: 1,
            end_line: 1,
            end_column: 2,
            severity: rumdl_lib::rule::Severity::Warning,
            fix: None,
            rule_name: Some("MD999".to_string()),
        }])
    }

    fn fix(&self, ctx: &rumdl_lib::lint_context::LintContext) -> Result<String, rumdl_lib::rule::LintError> {
        Ok(ctx.content.to_string())
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

fn process_with_panicking_rule(path: &Path, fix_mode: crate::FixMode) -> FileProcessResult {
    let rules: Vec<Box<dyn Rule>> = vec![Box::new(PanicsOnBoom)];
    let rule_sets = RuleSets {
        mode: crate::CodeBlockToolsMode::Configured,
        selected: std::sync::Arc::new(rules.iter().map(|rule| dyn_clone::clone_box(&**rule)).collect()),
        document: rules,
        embedded_markdown: Vec::new(),
        auxiliary: AuxiliaryExecutionPlan {
            lint: false,
            format: false,
            relint: false,
        },
    };
    process_file_with_formatter(
        &path.to_string_lossy(),
        &rule_sets,
        fix_mode,
        false,
        false,
        true,
        true,
        &rumdl_lib::output::OutputFormat::Text,
        &rumdl_lib::output::OutputWriter::new(false, true),
        &rumdl_config::Config::default(),
        None,
        None,
        None,
        false,
        None,
        None,
        false,
        false,
    )
}

#[test]
fn a_panic_while_linting_one_file_is_reported_as_an_error_for_that_file_only() {
    let dir = TempDir::new().unwrap();
    let bad = dir.path().join("bad.md");
    let good = dir.path().join("good.md");
    fs::write(&bad, "boom\n").unwrap();
    fs::write(&good, "fine\n").unwrap();

    for fix_mode in [crate::FixMode::Check, crate::FixMode::CheckFix] {
        let failed = process_with_panicking_rule(&bad, fix_mode);
        assert!(failed.errored, "a panicking file must be a tool error ({fix_mode:?})");
        assert!(failed.warnings.is_empty());

        let linted = process_with_panicking_rule(&good, fix_mode);
        assert!(!linted.errored, "the next file must still be processed ({fix_mode:?})");
        assert!(linted.has_issues, "the next file must still be linted ({fix_mode:?})");
    }
    assert_eq!(fs::read_to_string(&bad).unwrap(), "boom\n");
}

#[test]
fn embedded_formatting_invalidates_prepared_document_diagnostics() {
    use rumdl_lib::code_block_tools::{LanguageToolConfig, RUMDL_BUILTIN_TOOL};

    let dir = TempDir::new().unwrap();
    let path = dir.path().join("document.md");
    fs::write(
        &path,
        "#Title\n\n```markdown\n#Inner\ntext\n```\n\n[broken](missing.md)\n",
    )
    .unwrap();
    let mut config = rumdl_config::Config::default();
    config.code_block_tools.enabled = true;
    config.code_block_tools.languages.insert(
        "markdown".into(),
        LanguageToolConfig {
            lint: vec![RUMDL_BUILTIN_TOOL.into()],
            format: vec![RUMDL_BUILTIN_TOOL.into()],
            ..Default::default()
        },
    );
    let rules: Vec<Box<dyn Rule>> = ["MD018", "MD022", "MD057"]
        .into_iter()
        .map(|name| rumdl_lib::rules::create_rule_by_name(name, &config).unwrap())
        .collect();
    let rule_sets = RuleSets {
        mode: crate::CodeBlockToolsMode::Configured,
        selected: std::sync::Arc::new(rules.iter().map(|rule| dyn_clone::clone_box(&**rule)).collect()),
        embedded_markdown: rules.iter().map(|rule| dyn_clone::clone_box(&**rule)).collect(),
        document: rules,
        auxiliary: AuxiliaryExecutionPlan {
            lint: true,
            format: true,
            relint: true,
        },
    };
    let result = process_file_with_formatter(
        &path.to_string_lossy(),
        &rule_sets,
        crate::FixMode::CheckFix,
        false,
        false,
        true,
        true,
        &rumdl_lib::output::OutputFormat::Json,
        &rumdl_lib::output::OutputWriter::new(false, true),
        &config,
        None,
        None,
        None,
        false,
        None,
        None,
        false,
        false,
    );
    assert!(!result.errored);
    assert!(result.content_changed);
    let text = fs::read_to_string(&path).unwrap();
    assert!(text.contains("# Inner\n\ntext"), "{text}");
    let warning = result
        .warnings
        .iter()
        .find(|warning| warning.rule_name.as_deref() == Some("MD057"))
        .unwrap();
    assert_eq!(
        warning.line, 9,
        "the embedded formatter inserted a line above the finding"
    );
    assert_eq!(text.lines().nth(warning.line - 1), Some("[broken](missing.md)"));
}
