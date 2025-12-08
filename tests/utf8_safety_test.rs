// Test file to verify UTF-8 safety in various code paths

#[cfg(test)]
mod tests {
    use rumdl_lib::config::MarkdownFlavor;
    use rumdl_lib::lint_context::LintContext;
    use rumdl_lib::rule::Rule;
    use rumdl_lib::rules::MD033NoInlineHtml;

    /// Test HTML tag parsing with CJK characters in attributes
    #[test]
    fn test_html_tag_with_cjk_attributes() {
        let content = r#"<div class="测试">안녕하세요</div>"#;
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        let rule = MD033NoInlineHtml::new();

        // This should not panic
        let result = rule.check(&ctx);
        assert!(
            result.is_ok(),
            "Should not panic with CJK characters in HTML attributes"
        );
    }

    /// Test HTML tag with CJK content
    #[test]
    fn test_html_tag_with_cjk_content() {
        let content = r#"<div>안녕하세요</div>"#;
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        let rule = MD033NoInlineHtml::new();

        let result = rule.check(&ctx);
        assert!(result.is_ok(), "Should not panic with CJK characters in HTML content");
    }

    /// Test code spans with CJK characters
    #[test]
    fn test_code_spans_with_cjk() {
        let content = r#"Text with `안녕하세요` code span."#;
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

        // Access code spans - this should not panic
        let code_spans = ctx.code_spans();
        assert!(!code_spans.is_empty(), "Should find code span");

        // Verify byte offsets are at character boundaries
        for span in code_spans.iter() {
            assert!(
                content.is_char_boundary(span.byte_offset),
                "Code span byte_offset {} should be at character boundary",
                span.byte_offset
            );
            assert!(
                content.is_char_boundary(span.byte_end),
                "Code span byte_end {} should be at character boundary",
                span.byte_end
            );
        }
    }

    /// Test HTML tag byte boundaries
    #[test]
    fn test_html_tag_byte_boundaries() {
        let content = r#"<div class="测试">안녕</div>"#;
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

        // Verify all HTML tag byte offsets are at character boundaries
        for tag in ctx.html_tags().iter() {
            assert!(
                content.is_char_boundary(tag.byte_offset),
                "HTML tag byte_offset {} should be at character boundary for tag: {:?}",
                tag.byte_offset,
                tag
            );
            assert!(
                content.is_char_boundary(tag.byte_end),
                "HTML tag byte_end {} should be at character boundary for tag: {:?}",
                tag.byte_end,
                tag
            );

            // Test that we can safely slice the tag
            let tag_str = &content[tag.byte_offset..tag.byte_end];
            assert!(!tag_str.is_empty(), "Tag string should not be empty");
        }
    }

    /// Test the exact issue #154 scenario
    #[test]
    fn test_issue_154_exact_scenario() {
        let content = r#"- 2023 년 초 이후 주가 상승        +1,000% (10 배 상승)  "#;
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

        // This should not panic
        let _ = &ctx.lines;
        let _ = ctx.html_tags();
        let _ = ctx.code_spans();
    }
}
