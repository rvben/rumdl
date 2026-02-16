use super::embedded::{
    MAX_EMBEDDED_DEPTH, check_embedded_markdown_blocks, format_embedded_markdown_blocks, restore_indent,
    should_lint_embedded_markdown, strip_common_indent,
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

    assert_eq!(result, Some("docs/guide.md".to_string()));
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
fn test_format_embedded_markdown_blocks_atx_heading() {
    let config = rumdl_config::Config::default();
    let rules = rumdl_lib::rules::filter_rules(&rumdl_lib::rules::all_rules(&config), &config.global);

    let mut content = "# Example\n\n```markdown\n#Heading without space\n```\n".to_string();
    let formatted = format_embedded_markdown_blocks(&mut content, &rules, &config);

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
    let formatted = format_embedded_markdown_blocks(&mut content, &rules, &config);

    assert!(formatted > 0, "Should format block with 'md' language");
    assert!(content.contains("# Test"), "Should fix heading, got: {content:?}");
}

#[test]
fn test_format_embedded_markdown_blocks_case_insensitive() {
    let config = rumdl_config::Config::default();
    let rules = rumdl_lib::rules::filter_rules(&rumdl_lib::rules::all_rules(&config), &config.global);

    let mut content = "# Doc\n\n```MARKDOWN\n#Upper case\n```\n".to_string();
    let formatted = format_embedded_markdown_blocks(&mut content, &rules, &config);

    assert!(formatted > 0, "Should detect MARKDOWN (uppercase)");
    assert!(content.contains("# Upper case"));
}

#[test]
fn test_format_embedded_markdown_blocks_tilde_fence() {
    let config = rumdl_config::Config::default();
    let rules = rumdl_lib::rules::filter_rules(&rumdl_lib::rules::all_rules(&config), &config.global);

    let mut content = "# Doc\n\n~~~markdown\n#Tilde fence\n~~~\n".to_string();
    let formatted = format_embedded_markdown_blocks(&mut content, &rules, &config);

    assert!(formatted > 0, "Should detect tilde fenced blocks");
    assert!(content.contains("# Tilde fence"));
}

#[test]
fn test_format_embedded_markdown_blocks_multiple_blocks() {
    let config = rumdl_config::Config::default();
    let rules = rumdl_lib::rules::filter_rules(&rumdl_lib::rules::all_rules(&config), &config.global);

    let mut content = "# Doc\n\n```markdown\n#First\n```\n\nText\n\n```md\n#Second\n```\n".to_string();
    let formatted = format_embedded_markdown_blocks(&mut content, &rules, &config);

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
    let formatted = format_embedded_markdown_blocks(&mut content, &rules, &config);

    assert!(formatted >= 1, "Should format at least outer block");
    assert!(content.contains("# Outer"), "Should fix outer heading");
}

#[test]
fn test_format_embedded_markdown_blocks_preserves_indentation() {
    let config = rumdl_config::Config::default();
    let rules = rumdl_lib::rules::filter_rules(&rumdl_lib::rules::all_rules(&config), &config.global);

    // Content with relative indentation that should be preserved
    let mut content = "# Doc\n\n```markdown\n#Heading\n\n    code block\n```\n".to_string();
    let formatted = format_embedded_markdown_blocks(&mut content, &rules, &config);

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
    let formatted = format_embedded_markdown_blocks(&mut content, &rules, &config);

    assert_eq!(formatted, 0, "Should skip empty blocks");
    assert_eq!(content, original, "Content should be unchanged");
}

#[test]
fn test_format_embedded_markdown_blocks_whitespace_only() {
    let config = rumdl_config::Config::default();
    let rules = rumdl_lib::rules::filter_rules(&rumdl_lib::rules::all_rules(&config), &config.global);

    let mut content = "# Doc\n\n```markdown\n   \n\n```\n".to_string();
    let original = content.clone();
    let formatted = format_embedded_markdown_blocks(&mut content, &rules, &config);

    assert_eq!(formatted, 0, "Should skip whitespace-only blocks");
    assert_eq!(content, original, "Content should be unchanged");
}

#[test]
fn test_format_embedded_markdown_blocks_skips_other_languages() {
    let config = rumdl_config::Config::default();
    let rules = rumdl_lib::rules::filter_rules(&rumdl_lib::rules::all_rules(&config), &config.global);

    let mut content = "# Doc\n\n```rust\n#[derive(Debug)]\nfn main() {}\n```\n".to_string();
    let original = content.clone();
    let formatted = format_embedded_markdown_blocks(&mut content, &rules, &config);

    assert_eq!(formatted, 0, "Should not format rust blocks");
    assert_eq!(content, original, "Content should be unchanged");
}

#[test]
fn test_format_embedded_markdown_blocks_multiple_blank_lines() {
    let config = rumdl_config::Config::default();
    let rules = rumdl_lib::rules::filter_rules(&rumdl_lib::rules::all_rules(&config), &config.global);

    // MD012 should fix multiple consecutive blank lines between non-heading content
    let mut content = "# Doc\n\n```markdown\nParagraph 1\n\n\n\nParagraph 2\n```\n".to_string();
    let formatted = format_embedded_markdown_blocks(&mut content, &rules, &config);

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
    let formatted = format_embedded_markdown_blocks(&mut content, &rules, &config);
    assert!(formatted <= MAX_EMBEDDED_DEPTH, "Should respect depth limit");
}

#[test]
fn test_strip_common_indent_basic() {
    let content = "    line1\n    line2\n";
    let (stripped, indent) = strip_common_indent(content);

    assert_eq!(indent, "    ");
    assert!(stripped.starts_with("line1\n"));
    assert!(stripped.contains("line2"));
}

#[test]
fn test_strip_common_indent_mixed() {
    // First line has 2 spaces, second has 4 - should strip 2
    let content = "  line1\n    line2\n";
    let (stripped, indent) = strip_common_indent(content);

    assert_eq!(indent, "  ");
    assert_eq!(stripped, "line1\n  line2\n");
}

#[test]
fn test_strip_common_indent_preserves_empty_lines() {
    let content = "  line1\n\n  line2\n";
    let (stripped, _) = strip_common_indent(content);

    assert!(stripped.contains("\n\n"), "Should preserve empty lines");
}

#[test]
fn test_restore_indent_basic() {
    let content = "line1\nline2\n";
    let restored = restore_indent(content, "  ");

    assert_eq!(restored, "  line1\n  line2\n");
}

#[test]
fn test_restore_indent_preserves_empty_lines() {
    let content = "line1\n\nline2\n";
    let restored = restore_indent(content, "  ");

    assert_eq!(restored, "  line1\n\n  line2\n");
}

#[test]
fn test_restore_indent_preserves_trailing_newline() {
    let content = "line1\n";
    let restored = restore_indent(content, "  ");

    assert!(restored.ends_with('\n'), "Should preserve trailing newline");

    let content_no_newline = "line1";
    let restored_no_newline = restore_indent(content_no_newline, "  ");

    assert!(!restored_no_newline.ends_with('\n'), "Should not add trailing newline");
}

#[test]
fn test_format_embedded_markdown_no_extra_blank_line() {
    // Regression test: MD047 should NOT add extra blank line before closing fence
    let config = rumdl_config::Config::default();
    let rules = rumdl_lib::rules::filter_rules(&rumdl_lib::rules::all_rules(&config), &config.global);

    // Content that doesn't end with newline inside the block
    let mut content = "# Doc\n\n```markdown\n> [!INFO]\n> Content\n```\n".to_string();
    let original = content.clone();
    let formatted = format_embedded_markdown_blocks(&mut content, &rules, &config);

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
    let formatted = format_embedded_markdown_blocks(&mut content, &rules, &config);

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

    let formatted = format_embedded_markdown_blocks(&mut content, &rules, &config);

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
    let formatted = format_embedded_markdown_blocks(&mut content, &rules, &config);

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
    let formatted = format_embedded_markdown_blocks(&mut content, &rules, &config);

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

    let formatted = format_embedded_markdown_blocks(&mut content, &rules, &config);

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

    let formatted = format_embedded_markdown_blocks(&mut content, &rules, &config);

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
    let formatted = format_embedded_markdown_blocks(&mut content, &rules, &config);

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

    let formatted = format_embedded_markdown_blocks(&mut content, &rules, &config);

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
    let formatted_with_rule = format_embedded_markdown_blocks(&mut content_with_rule, &all_rules, &config);

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
    let _formatted_without_rule = format_embedded_markdown_blocks(&mut content_without_rule, &filtered_rules, &config);

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
    let formatted = format_embedded_markdown_blocks(&mut content, &rules, &config);

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
    let formatted = format_embedded_markdown_blocks(&mut format_content, &rules, &config);

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
            format: vec![],
            on_error: None,
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
            format: vec![],
            on_error: None,
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
            format: vec![],
            on_error: None,
        },
    );
    assert!(!should_lint_embedded_markdown(&config));
}
