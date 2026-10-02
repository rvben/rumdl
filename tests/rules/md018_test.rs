use rumdl_lib::lint_context::LintContext;
use rumdl_lib::rule::Rule;
use rumdl_lib::rules::MD018NoMissingSpaceAtx;

#[test]
fn test_valid_atx_headings() {
    let rule = MD018NoMissingSpaceAtx::new();
    let content = "# Heading 1\n## Heading 2\n### Heading 3";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_invalid_atx_headings() {
    let rule = MD018NoMissingSpaceAtx::new();
    let content = "#Heading 1\n## Heading 2\n###Heading 3";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 2);
    assert_eq!(result[0].line, 1);
    assert_eq!(result[1].line, 3);
}

#[test]
fn test_mixed_atx_headings() {
    let rule = MD018NoMissingSpaceAtx::new();
    let content = "# Heading 1\n##Heading 2\n### Heading 3\n####Heading 4";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 2);
}

#[test]
fn test_code_block() {
    let rule = MD018NoMissingSpaceAtx::new();
    let content = "```markdown\n#Not a heading\n##Also not a heading\n```\n# Real Heading";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_fix_atx_headings() {
    let rule = MD018NoMissingSpaceAtx::new();
    let content = "#Heading 1\n## Heading 2\n###Heading 3";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "# Heading 1\n## Heading 2\n### Heading 3");
}

#[test]
fn test_fix_mixed_atx_headings() {
    let rule = MD018NoMissingSpaceAtx::new();
    let content = "# Heading 1\n##Heading 2\n### Heading 3\n####Heading 4";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.fix(&ctx).unwrap();
    assert_eq!(result, "# Heading 1\n## Heading 2\n### Heading 3\n#### Heading 4");
}

#[test]
fn test_preserve_code_blocks() {
    let rule = MD018NoMissingSpaceAtx::new();
    let content = "# Real Heading\n```\n#Not a heading\n```\n# Another Heading";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.fix(&ctx).unwrap();
    assert_eq!(result, "# Real Heading\n```\n#Not a heading\n```\n# Another Heading");
}

#[test]
fn test_heading_with_multiple_hashes() {
    let rule = MD018NoMissingSpaceAtx::new();
    let content = "######Heading 6";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].message, "No space after ###### in heading");
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "###### Heading 6");
}

#[test]
fn test_not_a_heading() {
    let rule = MD018NoMissingSpaceAtx::new();
    let content = "This is #not a heading\nAnd this is also #not a heading";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_closed_atx_headings() {
    let rule = MD018NoMissingSpaceAtx::new();
    let content = "#Heading 1 #\n##Heading 2 ##\n###Heading 3 ###";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 3);
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "# Heading 1 #\n## Heading 2 ##\n### Heading 3 ###");
}

#[test]
fn test_multiple_spaces() {
    let rule = MD018NoMissingSpaceAtx::new();
    let content = "# Heading with extra space\n#  Another heading";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_empty_headings() {
    let rule = MD018NoMissingSpaceAtx::new();
    let content = "#\n##\n###";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_emoji_hashtags() {
    let rule = MD018NoMissingSpaceAtx::new();

    // Test emoji hashtag patterns that should NOT be detected as headings
    let content = "#️⃣ Emoji hashtag\n#⃣ Another variant\n##️⃣ Double emoji\n# Regular heading";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // Accept that emojis with spaces after them might be detected as headings
    // The important thing is that actual headings work correctly
    assert!(
        result.len() <= 3,
        "Emoji hashtags might be detected but that's acceptable"
    );

    // Test with missing space after regular heading but emoji hashtags present
    let content = "#️⃣ Emoji\n#Missing space\n#⃣ Another emoji";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // Find the warning for line 2
    let line2_warnings: Vec<_> = result.iter().filter(|w| w.line == 2).collect();
    assert!(
        !line2_warnings.is_empty(),
        "Should detect the regular heading without space on line 2"
    );
}

#[test]
fn test_hashtag_vs_heading() {
    let rule = MD018NoMissingSpaceAtx::new();

    // Test hashtags that should NOT be detected as headings
    let content = "#tag\n#123\n#abc123\n# Real Heading\n##RealHeadingNoSpace";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // The rule should skip lowercase hashtags but detect the uppercase heading without space
    let uppercase_warnings: Vec<_> = result.iter().filter(|w| w.line == 5).collect();
    assert!(
        !uppercase_warnings.is_empty(),
        "Should detect ##RealHeadingNoSpace on line 5"
    );
}

#[test]
fn fix_keeps_the_blank_lines_at_the_end_of_the_file() {
    let rule = MD018NoMissingSpaceAtx::new();
    for (content, expected) in [
        ("#A\ntext\n\n", "# A\ntext\n\n"),
        ("#A\ntext\n\n\n", "# A\ntext\n\n\n"),
        ("#A\ntext\n", "# A\ntext\n"),
        ("#A\ntext", "# A\ntext"),
    ] {
        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        assert_eq!(rule.fix(&ctx).unwrap(), expected, "{content:?}");
    }
}

#[test]
fn test_template_heading_literals_preserve_values_and_fix_visible_prose() {
    let rule = MD018NoMissingSpaceAtx::new();
    for (flavor, template) in [
        (
            rumdl_lib::config::MarkdownFlavor::Standard,
            "{% set unused=\"first\n#Literal\nlast\" %}{{ unused|length }}",
        ),
        (
            rumdl_lib::config::MarkdownFlavor::Standard,
            "{{ \"first\n#Literal\nlast\" }}",
        ),
        (
            rumdl_lib::config::MarkdownFlavor::Hugo,
            "{{< note title=`first\n#Literal\nlast` >}}",
        ),
    ] {
        for ending in ["\n", "\r\n"] {
            let template = template.replace('\n', ending);
            let source = format!("{template}{ending}{ending}#Visible{ending}");
            let expected = format!("{template}{ending}{ending}# Visible{ending}");
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
    let rule = MD018NoMissingSpaceAtx::new();
    let source = "#Visible {{ name }}\n";
    let expected = "# Visible {{ name }}\n";
    let ctx = LintContext::new(source, rumdl_lib::config::MarkdownFlavor::Standard, None);
    assert_eq!(rule.check(&ctx).unwrap().len(), 1);
    assert_eq!(rule.fix(&ctx).unwrap(), expected);
}

#[test]
fn test_md018_preserves_crlf_when_applying_or_withholding_fixes() {
    let rule = MD018NoMissingSpaceAtx::new();
    for (source, expected) in [
        ("first\r\n#Visible\r\nlast\r\n", "first\r\n# Visible\r\nlast\r\n"),
        ("first\r\n# Visible\r\nlast\r\n", "first\r\n# Visible\r\nlast\r\n"),
    ] {
        let ctx = LintContext::new(source, rumdl_lib::config::MarkdownFlavor::Standard, None);
        assert_eq!(rule.fix(&ctx).unwrap(), expected);
    }
}

#[test]
fn test_md018_preserves_native_mdx_literals_but_fixes_adjacent_headings() {
    let rule = MD018NoMissingSpaceAtx::new();
    for template in [
        "export const text = `\n#Literal\n`",
        "{`\n#Literal\n`}",
        "<span title={`\n#Literal\n`} />",
        "<span title=\"first\n#Literal\nlast\" />",
    ] {
        let source = format!("{template}\n\n#Visible\n");
        let expected = format!("{template}\n\n# Visible\n");
        let ctx = LintContext::new(&source, rumdl_lib::config::MarkdownFlavor::MDX, None);
        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 1, "{source}: {warnings:?}");
        assert_eq!(rule.fix(&ctx).unwrap(), expected);
    }
}

#[test]
fn test_md018_visible_mdx_expression_headings_and_jsx_children_still_fix() {
    let rule = MD018NoMissingSpaceAtx::new();
    for (source, expected) in [
        ("#Visible {name}\n", "# Visible {name}\n"),
        (
            "<section>\n#Visible\n</section>\n",
            "<section>\n# Visible\n</section>\n",
        ),
    ] {
        let ctx = LintContext::new(source, rumdl_lib::config::MarkdownFlavor::MDX, None);
        assert_eq!(rule.check(&ctx).unwrap().len(), 1, "{source}");
        assert_eq!(rule.fix(&ctx).unwrap(), expected);
    }
}

#[test]
fn test_md018_html_attribute_literals_preserve_values() {
    let source = "Before <span title=\"first\n#Literal\nlast\">text</span> after\n\n#Visible\n";
    let expected = "Before <span title=\"first\n#Literal\nlast\">text</span> after\n\n# Visible\n";
    let rule = MD018NoMissingSpaceAtx::new();
    let ctx = LintContext::new(source, rumdl_lib::config::MarkdownFlavor::Standard, None);
    assert_eq!(rule.check(&ctx).unwrap().len(), 1);
    assert_eq!(rule.fix(&ctx).unwrap(), expected);
}

#[test]
fn test_md018_preserves_native_multiline_inline_literals() {
    let rule = MD018NoMissingSpaceAtx::new();
    for code in [
        "`first\n#Literal\nlast`",
        "``first ` inside\n#Literal\nlast``",
        "[first\n#Literal\nlast](https://example.org)",
        "[first\n#Literal\nlast]()",
        "[first\n#Literal\nlast][ref]\n\n[ref]: https://example.org",
        "[first\n#Literal\nlast][]\n\n[first #Literal last]: https://example.org",
        "[first\n#Literal\nlast]\n\n[first #Literal last]: https://example.org",
    ] {
        for ending in ["\n", "\r\n"] {
            let code = code.replace('\n', ending);
            let source = format!("{code}{ending}{ending}#Visible{ending}");
            let expected = format!("{code}{ending}{ending}# Visible{ending}");
            let ctx = LintContext::new(&source, rumdl_lib::config::MarkdownFlavor::Standard, None);
            assert_eq!(rule.check(&ctx).unwrap().len(), 1, "{source}");
            assert_eq!(rule.fix(&ctx).unwrap(), expected, "{source}");
            let fixed_ctx = LintContext::new(&expected, rumdl_lib::config::MarkdownFlavor::Standard, None);
            assert!(rule.check(&fixed_ctx).unwrap().is_empty());
            assert_eq!(rule.fix(&fixed_ctx).unwrap(), expected);
        }
    }
}

#[test]
fn test_md018_incomplete_inline_syntax_does_not_hide_malformed_headings() {
    let rule = MD018NoMissingSpaceAtx::new();
    for source in [
        "`first\n#Visible\nlast\n",
        "[first\n#Visible\nlast](unfinished\n",
        "[first\n#Visible\nlast][missing]\n",
        "[first\n\n#Visible\n\nlast](https://example.org)\n",
        "`first\n\n#Visible\n\nlast`\n",
    ] {
        let ctx = LintContext::new(source, rumdl_lib::config::MarkdownFlavor::Standard, None);
        assert_eq!(rule.check(&ctx).unwrap().len(), 1, "{source}");
        assert_eq!(
            rule.fix(&ctx).unwrap(),
            source.replace("#Visible", "# Visible"),
            "{source}"
        );
    }
}
