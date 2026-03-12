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
    // ## Heading is a child of demoted # Title 2, so it cascades to ###
    assert_eq!(result, "# Title 1\n## Title 2\n### Heading\n");
}

#[test]
fn test_md025_fix_multiple() {
    let rule = MD025SingleTitle::default();
    let content = "# Title 1\n# Title 2\n# Title 3\n## Heading\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.fix(&ctx).unwrap();
    // Title 2 and Title 3 are demoted to ##, and ## Heading after Title 3
    // gets cascaded to ### since it's a child of a demoted section
    assert_eq!(result, "# Title 1\n## Title 2\n## Title 3\n### Heading\n");
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
fn test_md025_fix_cascades_child_headings() {
    // Issue #525: when a duplicate # is demoted to ##, its child headings
    // must also shift down to preserve the heading hierarchy.
    let rule = MD025SingleTitle::default();

    let content = "# 1_1\n# 1_2\n## 1_2-2_1\n# 1_3\n## 1_3-2_1\n### 1_3-2_1-3_1\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.fix(&ctx).unwrap();
    assert_eq!(
        result, "# 1_1\n## 1_2\n### 1_2-2_1\n## 1_3\n### 1_3-2_1\n#### 1_3-2_1-3_1\n",
        "Fix should cascade demotion to child headings"
    );
}

#[test]
fn test_md025_fix_cascade_no_overflow_past_level_6() {
    // If cascading would push a heading beyond level 6, preserve it as-is
    let rule = MD025SingleTitle::default();

    let content = "# Title\n# Second\n###### Deep\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.fix(&ctx).unwrap();
    // # Second → ## Second, but ###### Deep + 1 = 7 > 6, so preserved as ######
    assert_eq!(result, "# Title\n## Second\n###### Deep\n");
}

#[test]
fn test_md025_fix_cascade_resets_at_next_target_heading() {
    // The cascade delta should apply per-section: each demoted # starts its own section
    let rule = MD025SingleTitle::default();

    let content = "# Keep\n# Demote1\n## Child1\n## Child2\n# Demote2\n## Child3\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.fix(&ctx).unwrap();
    assert_eq!(
        result,
        "# Keep\n## Demote1\n### Child1\n### Child2\n## Demote2\n### Child3\n"
    );
}

#[test]
fn test_md025_fix_cascade_deep_hierarchy() {
    let rule = MD025SingleTitle::default();

    let content = "# Main\n# Other\n## H2\n### H3\n#### H4\n##### H5\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.fix(&ctx).unwrap();
    assert_eq!(result, "# Main\n## Other\n### H2\n#### H3\n##### H4\n###### H5\n");
}

#[test]
fn test_md025_fix_cascade_with_frontmatter_title() {
    let rule = MD025SingleTitle::default();

    // Frontmatter title counts as first heading, so ALL body # headings get demoted
    let content = "---\ntitle: FM Title\n---\n\n# Body H1\n## Sub\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.fix(&ctx).unwrap();
    assert!(
        result.contains("## Body H1") && result.contains("### Sub"),
        "Frontmatter title should trigger cascade for body headings, got: {result}"
    );
}

#[test]
fn test_md025_fix_cascade_allowed_section_resets_delta() {
    // Allowed sections (like "References") should not be demoted,
    // and should reset the cascade delta for headings that follow.
    // MD025SingleTitle::new() sets allow_document_sections: true.
    let rule = MD025SingleTitle::new(1, "");

    let content = "# Main\n# Other\n## Child\n# References\n## Ref1\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.fix(&ctx).unwrap();
    // "Other" demoted to ##, "Child" cascaded to ###
    // "References" is allowed (kept as #), so "Ref1" stays as ## (delta reset to 0)
    assert_eq!(result, "# Main\n## Other\n### Child\n# References\n## Ref1\n");
}

#[test]
fn test_md025_fix_idempotent() {
    // Running fix twice should produce the same result
    let rule = MD025SingleTitle::default();

    let content = "# 1_1\n# 1_2\n## 1_2-2_1\n# 1_3\n## 1_3-2_1\n### 1_3-2_1-3_1\n";
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
fn test_md025_fix_setext_child_under_demoted_section() {
    let rule = MD025SingleTitle::default();
    let content = "# Title\n# Other\nSetext Child\n-------------\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.fix(&ctx).unwrap();
    // Setext Child is level 2, cascaded to level 3 → must convert to ATX
    assert!(
        result.contains("### Setext Child"),
        "Setext child should cascade to ATX level 3, got: {result}"
    );
    assert!(
        !result.contains("-------------"),
        "Setext underline must not appear in output, got: {result}"
    );
}

#[test]
fn test_md025_fix_level2_config_cascades() {
    let rule = MD025SingleTitle::new(2, "");
    let content = "# H1\n## First\n### Child1\n## Dup\n### Child2\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.fix(&ctx).unwrap();
    // ## First kept, ## Dup demoted to ###, ### Child2 cascaded to ####
    assert_eq!(result, "# H1\n## First\n### Child1\n### Dup\n#### Child2\n");
}

#[test]
fn test_md025_fix_atx_closed_child_cascades() {
    let rule = MD025SingleTitle::default();
    let content = "# Keep\n# Demote\n## Child ##\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.fix(&ctx).unwrap();
    assert_eq!(result, "# Keep\n## Demote\n### Child ###\n");
}

#[test]
fn test_md025_fix_code_block_inside_demoted_section() {
    let rule = MD025SingleTitle::default();
    let content = "# Keep\n# Demote\n```\n## Not a heading\n```\n## Real Child\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.fix(&ctx).unwrap();
    assert!(
        result.contains("## Not a heading"),
        "Heading in code block must not be demoted, got: {result}"
    );
    assert!(
        result.contains("### Real Child"),
        "Real child heading must be cascaded, got: {result}"
    );
}

#[test]
fn test_md025_fix_inline_disable_resets_cascade() {
    let rule = MD025SingleTitle::default();
    let content = "# Title\n# Dup\n## Child\n<!-- markdownlint-disable MD025 -->\n# Preserved\n## Under\n<!-- markdownlint-enable MD025 -->\n## After\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.fix(&ctx).unwrap();
    // # Dup demoted to ##, ## Child cascaded to ###
    assert!(result.contains("## Dup"), "Dup should be demoted, got: {result}");
    assert!(result.contains("### Child"), "Child should cascade, got: {result}");
    // # Preserved is in a disabled region, kept as-is, resets delta
    assert!(
        result.contains("# Preserved"),
        "Preserved heading must stay as-is, got: {result}"
    );
    // ## Under is after delta reset, should stay as ##
    assert!(
        result.contains("## Under"),
        "Under should not cascade (delta reset), got: {result}"
    );
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
