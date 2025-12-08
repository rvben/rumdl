#[cfg(test)]
mod test_lint_context_flow {
    use rumdl_lib::config::MarkdownFlavor;
    use rumdl_lib::lint_context::LintContext;

    #[test]
    fn test_code_span_caching_issue() {
        let content = "`<env>`\n\n```diff\n- old\n+ new\n```";
        println!("\n=== Testing content with code block ===");
        println!("Content: {content:?}");

        // Create context (this is when code spans are initially parsed)
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

        // Access code_spans() - this might trigger re-parsing
        let spans1 = ctx.code_spans();
        println!("First access - found {} code spans", spans1.len());
        for span in spans1.iter() {
            println!(
                "  Span: line={}, cols={}-{}, content='{}'",
                span.line, span.start_col, span.end_col, span.content
            );
        }

        // Access again
        let spans2 = ctx.code_spans();
        println!("Second access - found {} code spans", spans2.len());

        // Test is_in_code_span
        println!("\nTesting is_in_code_span(1, 2): {}", ctx.is_in_code_span(1, 2));

        assert!(!spans1.is_empty(), "Should find code spans");
        assert_eq!(spans1.len(), spans2.len(), "Should be consistent");
    }

    #[test]
    fn test_without_code_block() {
        let content = "`<env>`";
        println!("\n=== Testing content without code block ===");
        println!("Content: {content:?}");

        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        let spans = ctx.code_spans();

        println!("Found {} code spans", spans.len());
        assert!(!spans.is_empty(), "Should find code spans");
    }
}
