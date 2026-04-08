use rumdl_lib::lint_context::LintContext;
use rumdl_lib::rule::Rule;
use rumdl_lib::rules::MD025SingleTitle;

#[test]
fn test_md025_valid() {
    let rule = MD025SingleTitle::default();
    let content = "# Title\n## Heading 2\n### Heading 3\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_md025_invalid() {
    let rule = MD025SingleTitle::default();
    let content = "# Title 1\n# Title 2\n## Heading\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].line, 2);
}

#[test]
fn test_md025_no_title() {
    let rule = MD025SingleTitle::default();
    let content = "## Heading 2\n### Heading 3\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_md025_with_front_matter() {
    // Frontmatter `title:` counts as the first H1, so a body H1 is a duplicate
    let rule = MD025SingleTitle::default();
    let content = "---\ntitle: Document Title\n---\n# Title\n## Heading 2\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1, "Body H1 should be flagged when frontmatter has title");
    assert_eq!(result[0].line, 4);
}

#[test]
fn test_md025_multiple_with_front_matter() {
    // Both body H1s are duplicates of the frontmatter title
    let rule = MD025SingleTitle::default();
    let content = "---\ntitle: Document Title\n---\n# Title 1\n## Heading 2\n# Title 2\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 2);
    assert_eq!(result[0].line, 4);
    assert_eq!(result[1].line, 6);
}

#[test]
fn test_md025_with_code_blocks() {
    let rule = MD025SingleTitle::default();
    let content = "# Title\n\n```markdown\n# This is not a real title\n```\n\n## Heading\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty(), "Should ignore titles in code blocks");
}

#[test]
fn test_md025_with_custom_level() {
    let rule = MD025SingleTitle::new(2, "");
    let content = "# Heading 1\n## Heading 2.1\n## Heading 2.2\n### Heading 3\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].line, 3);
}

#[test]
fn test_md025_indented_headings() {
    let rule = MD025SingleTitle::default();
    let content = "# Title 1\n\n  # Title 2\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].line, 3);
}

#[test]
fn test_md025_with_multiple_violations() {
    let rule = MD025SingleTitle::default();
    let content = "# Title 1\n\n# Title 2\n\n# Title 3\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 2);
    assert_eq!(result[0].line, 3);
    assert_eq!(result[1].line, 5);
}

#[test]
fn test_md025_empty_document() {
    let rule = MD025SingleTitle::default();
    let content = "";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_md025_closing_hashes() {
    let rule = MD025SingleTitle::default();
    let content = "# Title 1 #\n\n# Title 2 #\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].line, 3);
}

#[test]
fn test_md025_setext_headings() {
    let rule = MD025SingleTitle::default();
    // Setext headings (using === or ---) are now detected by this rule
    // Multiple level-1 setext headings should be flagged
    let content = "Title 1\n=======\n\nTitle 2\n=======\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1); // Second title should be flagged
    assert_eq!(result[0].line, 4); // "Title 2" line
}

#[test]
fn test_md025_performance() {
    let rule = MD025SingleTitle::default();

    // Generate a large document with many headings
    let mut content = String::new();
    content.push_str("# Main Title\n\n");

    for i in 1..=100 {
        content.push_str(&format!("## Heading {i}\n\nSome text here.\n\n"));
    }

    let ctx = LintContext::new(&content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let start = std::time::Instant::now();
    let result = rule.check(&ctx).unwrap();
    let duration = start.elapsed();

    assert!(result.is_empty());
    assert!(
        duration.as_millis() < 500,
        "Processing large document should take less than 500ms"
    );
}

#[test]
fn test_md025_fix() {
    let rule = MD025SingleTitle::default();
    let content = "# Title 1\n# Title 2\n## Heading\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.fix(&ctx).unwrap();
    // Only the duplicate H1 is demoted; child headings are not cascaded
    assert_eq!(result, "# Title 1\n## Title 2\n## Heading\n");
}

#[test]
fn test_md025_fix_multiple() {
    let rule = MD025SingleTitle::default();
    let content = "# Title 1\n# Title 2\n# Title 3\n## Heading\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.fix(&ctx).unwrap();
    // Only duplicate H1s are demoted; child headings are not cascaded
    assert_eq!(result, "# Title 1\n## Title 2\n## Title 3\n## Heading\n");
}

#[test]
fn test_md025_fix_with_indentation() {
    let rule = MD025SingleTitle::default();
    // In Markdown, content indented with 4+ spaces is considered a code block
    // so the heavily indented heading is not processed as a heading
    let content = "# Title 1\n  # Title 2\n    # Title 3\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let fixed = rule.fix(&ctx).unwrap();
    let fixed_ctx = LintContext::new(&fixed, rumdl_lib::config::MarkdownFlavor::Standard, None);

    // Expected behavior: verify the title is fixed properly
    assert!(fixed_ctx.content.contains("# Title 1"));
    assert!(fixed_ctx.content.contains("Title 2"));
    assert!(fixed_ctx.content.contains("Title 3"));

    // Ensure there are no duplicate H1 headings (the issue this rule checks for)
    let result = rule.check(&fixed_ctx).unwrap();
    assert!(result.is_empty(), "Fixed content should have no warnings");
}

#[test]
fn test_md025_fix_demotes_duplicates_only() {
    // Fix demotes only the duplicate top-level headings, not their children
    let rule = MD025SingleTitle::default();

    let content = "# 1_1\n# 1_2\n## 1_2-2_1\n# 1_3\n## 1_3-2_1\n### 1_3-2_1-3_1\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.fix(&ctx).unwrap();
    assert_eq!(
        result, "# 1_1\n## 1_2\n## 1_2-2_1\n## 1_3\n## 1_3-2_1\n### 1_3-2_1-3_1\n",
        "Fix should only demote duplicate top-level headings"
    );
}

#[test]
fn test_md025_fix_preserves_non_duplicate_headings() {
    // Non-duplicate headings at other levels are not modified
    let rule = MD025SingleTitle::default();

    let content = "# Title\n# Second\n###### Deep\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.fix(&ctx).unwrap();
    // # Second → ## Second, ###### Deep is not a duplicate H1 so stays as-is
    assert_eq!(result, "# Title\n## Second\n###### Deep\n");
}

#[test]
fn test_md025_fix_multiple_duplicates_with_children() {
    // Only the duplicate H1s are demoted; children stay at their original level
    let rule = MD025SingleTitle::default();

    let content = "# Keep\n# Demote1\n## Child1\n## Child2\n# Demote2\n## Child3\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.fix(&ctx).unwrap();
    assert_eq!(
        result,
        "# Keep\n## Demote1\n## Child1\n## Child2\n## Demote2\n## Child3\n"
    );
}

#[test]
fn test_md025_fix_deep_hierarchy_unchanged() {
    // Only the duplicate H1 is demoted; deeper headings are untouched
    let rule = MD025SingleTitle::default();

    let content = "# Main\n# Other\n## H2\n### H3\n#### H4\n##### H5\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.fix(&ctx).unwrap();
    assert_eq!(result, "# Main\n## Other\n## H2\n### H3\n#### H4\n##### H5\n");
}

#[test]
fn test_md025_fix_with_frontmatter_title_demotes_h1() {
    let rule = MD025SingleTitle::default();

    // Frontmatter title counts as first heading, so body H1 gets demoted
    let content = "---\ntitle: FM Title\n---\n\n# Body H1\n## Sub\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.fix(&ctx).unwrap();
    assert!(
        result.contains("## Body H1"),
        "Body H1 should be demoted when frontmatter has title, got: {result}"
    );
    // ## Sub is not a duplicate H1, so it stays as-is
    assert!(result.contains("## Sub"), "## Sub should stay unchanged, got: {result}");
}

#[test]
fn test_md025_fix_allowed_section_not_demoted() {
    // Allowed sections (like "References") should not be demoted.
    // MD025SingleTitle::new() sets allow_document_sections: true.
    let rule = MD025SingleTitle::new(1, "");

    let content = "# Main\n# Other\n## Child\n# References\n## Ref1\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.fix(&ctx).unwrap();
    // "Other" demoted to ##, "Child" stays at ##, "References" is allowed (kept as #)
    assert_eq!(result, "# Main\n## Other\n## Child\n# References\n## Ref1\n");
}

#[test]
fn test_md025_fix_idempotent() {
    // Running fix twice should produce the same result
    let rule = MD025SingleTitle::default();

    let content = "# Title\n# Duplicate\n## Sub\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let first_fix = rule.fix(&ctx).unwrap();

    let ctx2 = LintContext::new(&first_fix, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let second_fix = rule.fix(&ctx2).unwrap();
    assert_eq!(first_fix, second_fix, "Fix should be idempotent");
}

#[test]
fn test_md025_check_has_per_warning_fix() {
    // Per-warning fix demotes the target heading; cascade is handled by fix() method
    let rule = MD025SingleTitle::default();
    let content = "# Title 1\n# Title 2\n## Heading\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let warnings = rule.check(&ctx).unwrap();
    assert_eq!(warnings.len(), 1);
    assert!(warnings[0].fix.is_some(), "Per-warning fix should exist");
}

#[test]
fn test_md025_fix_setext_child_preserved() {
    // Setext child headings at level 2 are not duplicate H1s, so they stay as-is
    let rule = MD025SingleTitle::default();
    let content = "# Title\n# Other\nSetext Child\n-------------\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.fix(&ctx).unwrap();
    // Only # Other gets demoted; the setext H2 is not a duplicate H1
    assert!(
        result.contains("## Other"),
        "Duplicate H1 should be demoted, got: {result}"
    );
    assert!(
        result.contains("Setext Child\n-------------"),
        "Setext H2 should be preserved as-is, got: {result}"
    );
}

#[test]
fn test_md025_fix_level2_config() {
    let rule = MD025SingleTitle::new(2, "");
    let content = "# H1\n## First\n### Child1\n## Dup\n### Child2\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.fix(&ctx).unwrap();
    // ## First kept, ## Dup demoted to ###, ### Child2 stays as-is (not a duplicate H2)
    assert_eq!(result, "# H1\n## First\n### Child1\n### Dup\n### Child2\n");
}

#[test]
fn test_md025_fix_atx_closed_child_preserved() {
    // ATX-closed child headings are not duplicate H1s, so they stay as-is
    let rule = MD025SingleTitle::default();
    let content = "# Keep\n# Demote\n## Child ##\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.fix(&ctx).unwrap();
    assert_eq!(result, "# Keep\n## Demote\n## Child ##\n");
}

#[test]
fn test_md025_fix_code_block_preserved() {
    // Headings in code blocks are not touched; non-duplicate headings are preserved
    let rule = MD025SingleTitle::default();
    let content = "# Keep\n# Demote\n```\n## Not a heading\n```\n## Real Child\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.fix(&ctx).unwrap();
    assert!(
        result.contains("## Not a heading"),
        "Heading in code block must not be demoted, got: {result}"
    );
    assert!(
        result.contains("## Real Child"),
        "Non-duplicate child heading should stay as-is, got: {result}"
    );
}

#[test]
fn test_md025_fix_inline_disable_respected() {
    let rule = MD025SingleTitle::default();
    let content = "# Title\n# Dup\n## Child\n<!-- markdownlint-disable MD025 -->\n# Preserved\n## Under\n<!-- markdownlint-enable MD025 -->\n## After\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.fix(&ctx).unwrap();
    // # Dup demoted to ##, ## Child stays as-is (not a duplicate H1)
    assert!(result.contains("## Dup"), "Dup should be demoted, got: {result}");
    assert!(result.contains("## Child"), "Child should stay as ##, got: {result}");
    // # Preserved is in a disabled region, kept as-is
    assert!(
        result.contains("# Preserved"),
        "Preserved heading must stay as-is, got: {result}"
    );
    // ## Under stays as-is (not a duplicate H1)
    assert!(result.contains("## Under"), "Under should stay as ##, got: {result}");
}

#[test]
fn test_md025_per_warning_fix_setext_includes_underline() {
    // Per-warning fix for Setext duplicate must cover both text + underline
    let rule = MD025SingleTitle::default();
    let content = "# First ATX\n\nDuplicate Setext\n================\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let warnings = rule.check(&ctx).unwrap();
    assert_eq!(warnings.len(), 1);
    let fix = warnings[0].fix.as_ref().expect("Should have fix");
    // Apply the fix and verify no stray underline
    let mut fixed = ctx.content.to_string();
    fixed.replace_range(fix.range.clone(), &fix.replacement);
    assert!(
        !fixed.contains("================"),
        "Per-warning fix must not leave stray Setext underline, got: {fixed}"
    );
    assert!(
        fixed.contains("## Duplicate Setext"),
        "Per-warning fix should demote to ##, got: {fixed}"
    );
}

// === Roundtrip safety tests: fix() then check() should produce 0 violations ===

#[test]
fn test_md025_fix_roundtrip_simple() {
    let rule = MD025SingleTitle::default();
    let content = "# Title 1\n# Title 2\n## Heading\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let fixed = rule.fix(&ctx).unwrap();
    let fixed_ctx = LintContext::new(&fixed, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let warnings = rule.check(&fixed_ctx).unwrap();
    assert!(
        warnings.is_empty(),
        "Fixed content should have no MD025 warnings, got {} warnings for: {fixed}",
        warnings.len()
    );
}

#[test]
fn test_md025_fix_roundtrip_multiple() {
    let rule = MD025SingleTitle::default();
    let content = "# Title 1\n# Title 2\n# Title 3\n## Heading\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let fixed = rule.fix(&ctx).unwrap();
    let fixed_ctx = LintContext::new(&fixed, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let warnings = rule.check(&fixed_ctx).unwrap();
    assert!(
        warnings.is_empty(),
        "Fixed content should have no MD025 warnings, got {} warnings for: {fixed}",
        warnings.len()
    );
}

#[test]
fn test_md025_fix_roundtrip_setext() {
    let rule = MD025SingleTitle::default();
    let content = "Title 1\n=======\n\nTitle 2\n=======\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let warnings = rule.check(&ctx).unwrap();
    assert_eq!(warnings.len(), 1, "Should have 1 warning");
    let fix = warnings[0].fix.as_ref().expect("Should have fix");
    // Debug: print the fix range and replacement
    let range_text = &content[fix.range.clone()];
    assert!(
        !fix.replacement.is_empty(),
        "Fix replacement should not be empty, range covers: {:?}, replacement: {:?}",
        range_text,
        fix.replacement
    );
    let fixed = rule.fix(&ctx).unwrap();
    assert!(
        fixed.contains("## Title 2"),
        "Setext duplicate should be demoted to ATX ##, got: {fixed:?}"
    );
    let fixed_ctx = LintContext::new(&fixed, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let warnings = rule.check(&fixed_ctx).unwrap();
    assert!(
        warnings.is_empty(),
        "Fixed setext content should have no MD025 warnings, got {} warnings for: {:?}",
        warnings.len(),
        fixed
    );
}

#[test]
fn test_md025_fix_roundtrip_frontmatter() {
    let rule = MD025SingleTitle::default();
    let content = "---\ntitle: FM Title\n---\n\n# Body H1\n## Sub\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let fixed = rule.fix(&ctx).unwrap();
    let fixed_ctx = LintContext::new(&fixed, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let warnings = rule.check(&fixed_ctx).unwrap();
    assert!(
        warnings.is_empty(),
        "Fixed frontmatter content should have no MD025 warnings, got {} warnings for: {fixed}",
        warnings.len()
    );
}

#[test]
fn test_md025_fix_roundtrip_frontmatter_multiple_h1s() {
    let rule = MD025SingleTitle::default();
    let content = "---\ntitle: FM Title\n---\n\n# First Body H1\n\nContent\n\n# Second Body H1\n\nMore content\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let fixed = rule.fix(&ctx).unwrap();
    let fixed_ctx = LintContext::new(&fixed, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let warnings = rule.check(&fixed_ctx).unwrap();
    assert!(
        warnings.is_empty(),
        "Fixed content should have no MD025 warnings, got {} warnings for: {fixed}",
        warnings.len()
    );
}

#[test]
fn test_md025_fix_roundtrip_with_inline_formatting() {
    let rule = MD025SingleTitle::default();
    let content = "# **Bold Title**\n\n# *Italic Title*\n\n# `Code Title`\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let fixed = rule.fix(&ctx).unwrap();
    let fixed_ctx = LintContext::new(&fixed, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let warnings = rule.check(&fixed_ctx).unwrap();
    assert!(
        warnings.is_empty(),
        "Fixed content should have no MD025 warnings, got {} warnings for: {fixed}",
        warnings.len()
    );
}

#[test]
fn test_md025_fix_roundtrip_code_blocks_ignored() {
    let rule = MD025SingleTitle::default();
    let content = "# Title\n\n```\n# Not a heading\n```\n\n# Duplicate\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let fixed = rule.fix(&ctx).unwrap();
    // Code block content should be preserved
    assert!(
        fixed.contains("# Not a heading"),
        "Code block heading should be preserved, got: {fixed}"
    );
    let fixed_ctx = LintContext::new(&fixed, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let warnings = rule.check(&fixed_ctx).unwrap();
    assert!(
        warnings.is_empty(),
        "Fixed content should have no MD025 warnings, got {} warnings for: {fixed}",
        warnings.len()
    );
}

#[test]
fn test_md025_fix_roundtrip_mixed_setext_atx() {
    let rule = MD025SingleTitle::default();
    let content = "Title One\n=========\n\n# Title Two\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let fixed = rule.fix(&ctx).unwrap();
    let fixed_ctx = LintContext::new(&fixed, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let warnings = rule.check(&fixed_ctx).unwrap();
    assert!(
        warnings.is_empty(),
        "Fixed mixed setext/atx content should have no MD025 warnings, got {} warnings for: {fixed}",
        warnings.len()
    );
}

#[test]
fn test_md025_level6_not_demoted_beyond_h6() {
    // Markdown only supports heading levels 1-6. When level=6, duplicate H6
    // headings cannot be demoted to H7 (which doesn't exist). The fix must
    // leave them untouched rather than creating invalid "####### ..." lines.
    let rule = MD025SingleTitle::new(6, "");
    let content = "###### First\n###### Second\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);

    // check() should still detect the violation
    let warnings = rule.check(&ctx).unwrap();
    assert_eq!(warnings.len(), 1, "Should detect duplicate H6");
    assert!(
        warnings[0].fix.is_none(),
        "Fix should be None when demotion would exceed H6"
    );

    // fix() should leave the content unchanged (unfixable)
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, content, "Content should be unchanged when H6 cannot be demoted");
}
