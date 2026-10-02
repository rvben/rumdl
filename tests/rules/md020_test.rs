use rumdl_lib::lint_context::LintContext;
use rumdl_lib::rule::Rule;
use rumdl_lib::rules::MD020NoMissingSpaceClosedAtx;

#[test]
fn test_valid_closed_atx_headings() {
    let rule = MD020NoMissingSpaceClosedAtx::new();
    let content = "# Heading 1 #\n## Heading 2 ##\n### Heading 3 ###";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_invalid_closed_atx_headings() {
    let rule = MD020NoMissingSpaceClosedAtx::new();
    let content = "# Heading 1#\n## Heading 2##\n### Heading 3###";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 3);
    assert_eq!(result[0].line, 1);
    assert_eq!(result[0].column, 11);
    assert_eq!(result[0].message, "Missing space before # at end of closed heading");
}

#[test]
fn test_mixed_closed_atx_headings() {
    let rule = MD020NoMissingSpaceClosedAtx::new();
    let content = "# Heading 1 #\n## Heading 2##\n### Heading 3 ###\n#### Heading 4####";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 2);
}

#[test]
fn test_code_block() {
    let rule = MD020NoMissingSpaceClosedAtx::new();
    let content = "```markdown\n# Not a heading#\n## Also not a heading##\n```\n# Real Heading #";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_fix_closed_atx_headings() {
    let rule = MD020NoMissingSpaceClosedAtx::new();
    let content = "# Heading 1#\n## Heading 2##\n### Heading 3###";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.fix(&ctx).unwrap();
    assert_eq!(result, "# Heading 1 #\n## Heading 2 ##\n### Heading 3 ###");
}

#[test]
fn test_fix_mixed_closed_atx_headings() {
    let rule = MD020NoMissingSpaceClosedAtx::new();
    let content = "# Heading 1 #\n## Heading 2##\n### Heading 3 ###\n#### Heading 4####";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.fix(&ctx).unwrap();
    assert_eq!(
        result,
        "# Heading 1 #\n## Heading 2 ##\n### Heading 3 ###\n#### Heading 4 ####"
    );
}

#[test]
fn test_preserve_code_blocks() {
    let rule = MD020NoMissingSpaceClosedAtx::new();
    let content = "# Real Heading #\n```\n# Not a heading#\n```\n# Another Heading #";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.fix(&ctx).unwrap();
    assert_eq!(
        result,
        "# Real Heading #\n```\n# Not a heading#\n```\n# Another Heading #"
    );
}

#[test]
fn test_heading_with_multiple_hashes() {
    let rule = MD020NoMissingSpaceClosedAtx::new();
    let content = "###### Heading 6######";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(
        result[0].message,
        "Missing space before ###### at end of closed heading"
    );
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "###### Heading 6 ######");
}

#[test]
fn test_not_a_heading() {
    let rule = MD020NoMissingSpaceClosedAtx::new();
    let content = "This is #not a heading#\nAnd this is also #not a heading#";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_indented_closed_atx_headings() {
    let rule = MD020NoMissingSpaceClosedAtx::new();
    let content = "  # Heading 1#\n    ## Heading 2##\n      ### Heading 3###";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "  # Heading 1 #\n    ## Heading 2##\n      ### Heading 3###");
}

#[test]
fn test_empty_closed_atx_headings() {
    let rule = MD020NoMissingSpaceClosedAtx::new();
    let content = "# #\n## ##\n### ###";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_missing_space_at_start() {
    let rule = MD020NoMissingSpaceClosedAtx::new();
    let content = "#Heading 1 #\n##Heading 2 ##\n###Heading 3 ###";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 3);
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "# Heading 1 #\n## Heading 2 ##\n### Heading 3 ###");
}

#[test]
fn test_custom_id_no_space_at_end() {
    let rule = MD020NoMissingSpaceClosedAtx::new();
    let content = "# Heading# {#custom-id}\n## Another# {#id-2}\n### Third# {#id3}";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 3);
    assert_eq!(result[0].line, 1);
    assert_eq!(result[0].message, "Missing space before # at end of closed heading");
    assert_eq!(result[1].line, 2);
    assert_eq!(result[2].line, 3);

    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(
        fixed,
        "# Heading # {#custom-id}\n## Another # {#id-2}\n### Third # {#id3}"
    );
}

#[test]
fn test_custom_id_no_space_at_start() {
    let rule = MD020NoMissingSpaceClosedAtx::new();
    let content = "#Heading # {#custom-id}\n##Another # {#id-2}\n###Third # {#id3}";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 3);
    assert_eq!(result[0].message, "Missing space after # at start of closed heading");

    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(
        fixed,
        "# Heading # {#custom-id}\n## Another # {#id-2}\n### Third # {#id3}"
    );
}

#[test]
fn test_custom_id_no_space_both() {
    let rule = MD020NoMissingSpaceClosedAtx::new();
    let content = "#Heading# {#custom-id}\n##Another## {#id-2}\n###Third### {#id3}";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 3);
    assert!(result[0].message.contains("Missing space inside hashes"));

    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(
        fixed,
        "# Heading # {#custom-id}\n## Another ## {#id-2}\n### Third ### {#id3}"
    );
}

#[test]
fn test_custom_id_with_valid_spacing() {
    let rule = MD020NoMissingSpaceClosedAtx::new();
    let content = "# Heading # {#custom-id}\n## Another ## {#id-2}\n### Third ### {#id3}";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(
        result.is_empty(),
        "Properly spaced headings with custom IDs should not be flagged"
    );
}

#[test]
fn test_custom_id_various_formats() {
    let rule = MD020NoMissingSpaceClosedAtx::new();
    // Test various custom ID formats
    let content = "# Test# {#simple}\n# Test# {#with-dashes}\n# Test# {#with_underscores}\n# Test# {#MixedCase123}";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 4, "All headings should be flagged for missing space");

    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(
        fixed,
        "# Test # {#simple}\n# Test # {#with-dashes}\n# Test # {#with_underscores}\n# Test # {#MixedCase123}"
    );
}

#[test]
fn test_custom_id_with_spaces_around() {
    let rule = MD020NoMissingSpaceClosedAtx::new();
    // Test custom IDs with various spacing
    let content = "# Heading#  {#id1}\n# Heading#\t{#id2}\n# Heading# {#id3}  \n# Heading#  {#id4}  ";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(
        result.len(),
        4,
        "All headings should be flagged regardless of spacing around custom ID"
    );

    let fixed = rule.fix(&ctx).unwrap();
    let expected = "# Heading #  {#id1}\n# Heading #\t{#id2}\n# Heading # {#id3}  \n# Heading #  {#id4}  ";
    assert_eq!(fixed, expected);
}

#[test]
fn test_custom_id_multiple_levels() {
    let rule = MD020NoMissingSpaceClosedAtx::new();
    let content = "# Level 1# {#l1}\n## Level 2## {#l2}\n### Level 3### {#l3}\n#### Level 4#### {#l4}\n##### Level 5##### {#l5}\n###### Level 6###### {#l6}";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 6, "All heading levels should be flagged");

    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(
        fixed,
        "# Level 1 # {#l1}\n## Level 2 ## {#l2}\n### Level 3 ### {#l3}\n#### Level 4 #### {#l4}\n##### Level 5 ##### {#l5}\n###### Level 6 ###### {#l6}"
    );
}

#[test]
fn test_not_custom_id_patterns() {
    let rule = MD020NoMissingSpaceClosedAtx::new();
    // Test things that look like custom IDs but aren't
    let content = "# Heading # not {#id}\n# Heading # text after\n# Heading # {not-id}\n# Heading # {#id} extra";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(
        result.is_empty(),
        "These are not closed ATX with custom IDs and should not be flagged"
    );
}

#[test]
fn test_custom_id_indented() {
    let rule = MD020NoMissingSpaceClosedAtx::new();
    let content = "  # Indented# {#custom-id}\n   ## More indent## {#id2}";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 2, "Indented headings with custom IDs should be flagged");

    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "  # Indented # {#custom-id}\n   ## More indent ## {#id2}");
}

#[test]
fn fix_keeps_the_blank_lines_at_the_end_of_the_file() {
    let rule = MD020NoMissingSpaceClosedAtx::new();
    for (content, expected) in [
        ("# A#\ntext\n\n", "# A #\ntext\n\n"),
        ("# A#\ntext\n\n\n", "# A #\ntext\n\n\n"),
        ("# A#\ntext\n", "# A #\ntext\n"),
        ("# A#\ntext", "# A #\ntext"),
    ] {
        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        assert_eq!(rule.fix(&ctx).unwrap(), expected, "{content:?}");
    }
}

#[test]
fn test_template_heading_literals_preserve_values_and_fix_visible_prose() {
    let rule = MD020NoMissingSpaceClosedAtx::new();
    for (flavor, template) in [
        (
            rumdl_lib::config::MarkdownFlavor::Standard,
            "{% set unused=\"first\n#Literal#\nlast\" %}{{ unused|length }}",
        ),
        (
            rumdl_lib::config::MarkdownFlavor::Standard,
            "{{ \"first\n#Literal#\nlast\" }}",
        ),
        (
            rumdl_lib::config::MarkdownFlavor::Hugo,
            "{{< note title=`first\n#Literal#\nlast` >}}",
        ),
    ] {
        for ending in ["\n", "\r\n"] {
            let template = template.replace('\n', ending);
            let source = format!("{template}{ending}{ending}#Visible#{ending}");
            let expected = format!("{template}{ending}{ending}# Visible #{ending}");
            let ctx = LintContext::new(&source, flavor, None);
            let warnings = rule.check(&ctx).unwrap();
            assert_eq!(warnings.len(), 1, "{source}: {warnings:?}");
            assert_eq!(rule.fix(&ctx).unwrap(), expected, "{source}");
            let ctx = LintContext::new(&expected, flavor, None);
            assert!(rule.check(&ctx).unwrap().is_empty());
            assert_eq!(rule.fix(&ctx).unwrap(), expected);
        }
    }
}

#[test]
fn test_visible_heading_with_template_expression_remains_lintable() {
    let rule = MD020NoMissingSpaceClosedAtx::new();
    let source = "#Visible {{ name }}#\n";
    let expected = "# Visible {{ name }} #\n";
    let ctx = LintContext::new(source, rumdl_lib::config::MarkdownFlavor::Standard, None);
    assert_eq!(rule.check(&ctx).unwrap().len(), 1);
    assert_eq!(rule.fix(&ctx).unwrap(), expected);
}

#[test]
fn test_md020_preserves_crlf_when_applying_or_withholding_fixes() {
    let rule = MD020NoMissingSpaceClosedAtx::new();
    for (source, expected) in [
        ("first\r\n#Visible#\r\nlast\r\n", "first\r\n# Visible #\r\nlast\r\n"),
        ("first\r\n# Visible #\r\nlast\r\n", "first\r\n# Visible #\r\nlast\r\n"),
    ] {
        let ctx = LintContext::new(source, rumdl_lib::config::MarkdownFlavor::Standard, None);
        assert_eq!(rule.fix(&ctx).unwrap(), expected);
    }
}

#[test]
fn test_md020_html_attribute_literals_preserve_values() {
    let source = "Before <span title=\"first\n#Literal#\nlast\">text</span> after\n\n#Visible#\n";
    let expected = "Before <span title=\"first\n#Literal#\nlast\">text</span> after\n\n# Visible #\n";
    let rule = MD020NoMissingSpaceClosedAtx::new();
    let ctx = LintContext::new(source, rumdl_lib::config::MarkdownFlavor::Standard, None);
    assert_eq!(rule.check(&ctx).unwrap().len(), 1);
    assert_eq!(rule.fix(&ctx).unwrap(), expected);
}

#[test]
fn test_md020_preserves_native_multiline_code_spans() {
    let rule = MD020NoMissingSpaceClosedAtx::new();
    for code in ["`first\n#Literal#\nlast`", "``first ` inside\n#Literal#\nlast``"] {
        for ending in ["\n", "\r\n"] {
            let code = code.replace('\n', ending);
            let source = format!("{code}{ending}{ending}#Visible#{ending}");
            let expected = format!("{code}{ending}{ending}# Visible #{ending}");
            let ctx = LintContext::new(&source, rumdl_lib::config::MarkdownFlavor::Standard, None);
            assert_eq!(rule.check(&ctx).unwrap().len(), 1, "{source}");
            assert_eq!(rule.fix(&ctx).unwrap(), expected, "{source}");
        }
    }
}
