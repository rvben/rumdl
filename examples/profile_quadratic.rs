fn main() {
    unsafe {
        std::env::set_var("RUMDL_PROFILE_QUADRATIC", "1");
    }

    let sep = "=".repeat(80);
    println!("{}", sep);
    println!("PROFILING EACH OPERATION AT DIFFERENT SCALES");
    println!("{}", sep);

    let sizes = vec![
        (1_000, "1K lines"),
        (5_000, "5K lines"),
        (10_000, "10K lines"),
        (25_000, "25K lines"),
        (50_000, "50K lines"),
    ];

    for (target_lines, label) in sizes {
        let content = create_content(target_lines);
        let actual_lines = content.lines().count();

        let dash_line = "-".repeat(80);
        println!("\n{}", dash_line);
        println!("{} ({} KB, {} lines)", label, content.len() / 1024, actual_lines);
        println!("{}", dash_line);

        // Run once to see timings
        let _ = rumdl_lib::lint_context::LintContext::new(&content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        println!();
    }
}

fn create_content(target_lines: usize) -> String {
    let patterns = vec![
        "# Main Heading\n\n",
        "## Section Heading\n\n",
        "### Subsection\n\n",
        "This is a paragraph with `inline code`, **bold text**, and [a link](https://example.com). It contains multiple sentences to make it realistic. Some more text here with *emphasis* and `more code`.\n\n",
        "- List item 1 with `code` and [link](https://example.com)\n",
        "- List item 2 with **bold** and *emphasis*\n",
        "  - Nested item with `more code`\n",
        "  - Another nested item\n",
        "- List item 3\n\n",
        "1. First numbered item with `code`\n",
        "2. Second item with [reference link][ref1]\n",
        "3. Third item with **bold text**\n\n",
        "> Blockquote with `code` and [a link](https://example.com)\n",
        "> Second line of blockquote with **bold**\n",
        "> > Nested blockquote with *emphasis*\n\n",
        "```rust\nfn example() {\n    println!(\"Hello, world!\");\n    let x = 42;\n}\n```\n\n",
        "```python\ndef example():\n    print('Hello, world!')\n    x = 42\n```\n\n",
        "| Header 1 | Header 2 with `code` | Header 3 |\n",
        "|----------|---------------------|----------|\n",
        "| Cell 1 with `code` | Cell 2 | Cell 3 with [link](https://example.com) |\n",
        "| Cell 4 | Cell 5 with **bold** | Cell 6 |\n\n",
        "<!-- This is a comment -->\n\n",
        "[ref1]: https://example1.com\n",
        "[ref2]: https://example2.com \"Title\"\n\n",
        "\n",
    ];

    let mut content = String::new();
    let mut line_count = 0;
    let mut pattern_idx = 0;

    while line_count < target_lines {
        let pattern = patterns[pattern_idx % patterns.len()];
        content.push_str(pattern);
        line_count += pattern.lines().count();
        pattern_idx += 1;
    }

    content
}
