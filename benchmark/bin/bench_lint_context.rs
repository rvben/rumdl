use rumdl_lib::lint_context::LintContext;
use std::time::Instant;

fn main() {
    // Create test content with various markdown features
    let test_content = r#"
# Heading 1

This is a **bold** text with *italic* and `code` spans.

## Heading 2

Here's a list:
- Item 1 with [link](https://example.com)
- Item 2 with ![image](image.png)
  - Nested item
- Item 3

### Tables

| Column 1 | Column 2 | Column 3 |
|----------|:--------:|---------:|
| Cell 1   | Cell 2   | Cell 3   |
| Cell 4   | Cell 5   | Cell 6   |

> This is a blockquote
> with multiple lines

```rust
fn main() {
    println!("Hello, world!");
}
```

Some bare URLs: https://github.com/rvben/rumdl and email@example.com

<div class="custom">
  <p>HTML content</p>
</div>

More **emphasis** and _underscores_ everywhere.
"#;

    println!("Testing LintContext performance enhancements...");

    let iterations = 1000;

    // Test LintContext creation time
    let start = Instant::now();
    for _ in 0..iterations {
        let _ctx = LintContext::new(test_content);
    }
    let creation_time = start.elapsed();
    println!(
        "LintContext creation: {:?} per iteration ({} iterations)",
        creation_time / iterations,
        iterations
    );

    // Create a context to test accessor methods
    let ctx = LintContext::new(test_content);

    // Test character frequency queries
    let start = Instant::now();
    for _ in 0..iterations * 10 {
        let _has_headings = ctx.likely_has_headings();
        let _has_lists = ctx.likely_has_lists();
        let _has_emphasis = ctx.likely_has_emphasis();
        let _has_tables = ctx.likely_has_tables();
        let _has_code = ctx.likely_has_code();
        let _has_html = ctx.likely_has_html();
    }
    let query_time = start.elapsed();
    println!(
        "Character frequency queries: {:?} per 6 queries ({} iterations)",
        query_time / (iterations * 10),
        iterations * 10
    );

    // Test lazy-loaded parsing
    let start = Instant::now();
    let _html_tags = ctx.html_tags();
    let html_parse_time = start.elapsed();
    println!("HTML tag parsing (first access): {html_parse_time:?}");

    let start = Instant::now();
    let _html_tags = ctx.html_tags();
    let html_cache_time = start.elapsed();
    println!("HTML tag parsing (cached): {html_cache_time:?}");

    let start = Instant::now();
    let _emphasis_spans = ctx.emphasis_spans();
    let emphasis_parse_time = start.elapsed();
    println!("Emphasis span parsing (first access): {emphasis_parse_time:?}");

    let start = Instant::now();
    let _table_rows = ctx.table_rows();
    let table_parse_time = start.elapsed();
    println!("Table parsing (first access): {table_parse_time:?}");

    let start = Instant::now();
    let _bare_urls = ctx.bare_urls();
    let url_parse_time = start.elapsed();
    println!("Bare URL parsing (first access): {url_parse_time:?}");

    // Print statistics about what was found
    println!("\nContent analysis:");
    println!("- Lines: {}", ctx.lines.len());
    println!("- Character frequencies:");
    println!("  - Hash (#): {}", ctx.char_frequency.hash_count);
    println!("  - Asterisk (*): {}", ctx.char_frequency.asterisk_count);
    println!("  - Underscore (_): {}", ctx.char_frequency.underscore_count);
    println!("  - Hyphen (-): {}", ctx.char_frequency.hyphen_count);
    println!("  - Pipe (|): {}", ctx.char_frequency.pipe_count);
    println!("  - Bracket ([): {}", ctx.char_frequency.bracket_count);
    println!("  - Backtick (`): {}", ctx.char_frequency.backtick_count);
    println!("  - Less than (<): {}", ctx.char_frequency.lt_count);
    println!("- Parsed elements:");
    println!("  - Links: {}", ctx.links.len());
    println!("  - Images: {}", ctx.images.len());
    println!("  - HTML tags: {}", ctx.html_tags().len());
    println!("  - Emphasis spans: {}", ctx.emphasis_spans().len());
    println!("  - Table rows: {}", ctx.table_rows().len());
    println!("  - Bare URLs: {}", ctx.bare_urls().len());
    println!("  - List blocks: {}", ctx.list_blocks.len());
}
