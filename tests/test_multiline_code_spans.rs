#[cfg(test)]
mod test_multiline_code_spans {
    use rumdl_lib::config::MarkdownFlavor;
    use rumdl_lib::lint_context::LintContext;

    #[test]
    fn test_multiline_code_span_detection() {
        let content = "Line 1 `<code>`\n<div>html</div>\nLine 3 `<more>` test";
        println!("\nContent:\n{content}");

        let ctx = LintContext::new(content, MarkdownFlavor::Standard);
        let code_spans = ctx.code_spans();

        let span_count = code_spans.len();
        println!("\nCode spans found: {span_count}");
        for span in code_spans.iter() {
            let line = span.line;
            let start = span.start_col;
            let end = span.end_col;
            let content = &span.content;
            println!(
                "  Line {line}: cols {start}-{end} (0-indexed), content='{content}'"
            );
        }

        // Test specific positions
        println!("\nTesting positions:");
        let result = ctx.is_in_code_span(1, 8);
        println!(
            "  Line 1, col 8 (inside first code span): {result}"
        );
        let result = ctx.is_in_code_span(2, 2);
        println!("  Line 2, col 2 (in <div>): {result}");
        let result = ctx.is_in_code_span(3, 9);
        println!(
            "  Line 3, col 9 (position of < in <more>): {result}"
        );
        let result = ctx.is_in_code_span(3, 10);
        println!(
            "  Line 3, col 10 (position of m in <more>): {result}"
        );

        assert!(code_spans.len() >= 2, "Should find at least 2 code spans");
    }
}
