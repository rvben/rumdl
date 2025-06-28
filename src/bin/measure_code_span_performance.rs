use rumdl::lint_context::LintContext;
use std::time::Instant;

fn main() {
    let test_cases = vec![
        // Small file with many code spans
        ("small_many_spans", generate_many_code_spans(100)),
        // Medium file with nested structures
        ("medium_nested", generate_nested_content(500)),
        // Large file with mixed content
        ("large_mixed", generate_mixed_content(1000)),
        // Extra large file
        ("xlarge_mixed", generate_mixed_content(5000)),
    ];

    println!("Code Span Detection Performance Test");
    println!("====================================\n");

    for (name, content) in &test_cases {
        println!("Test case: {} ({} lines)", name, content.lines().count());

        // Measure LintContext creation time (includes code span parsing)
        let mut times = Vec::new();
        for _ in 0..10 {
            let start = Instant::now();
            let ctx = LintContext::new(content);
            let elapsed = start.elapsed();
            times.push(elapsed.as_micros());

            if times.len() == 1 {
                println!("  Code spans detected: {}", ctx.code_spans.len());
            }
        }

        // Calculate average time
        let avg_time = times.iter().sum::<u128>() / times.len() as u128;
        println!("  Average LintContext creation: {} μs", avg_time);

        println!();
    }
}

fn generate_many_code_spans(lines: usize) -> String {
    let mut content = String::new();
    for i in 0..lines {
        if i % 3 == 0 {
            content.push_str(&format!("Line {} with `code span {}` and more `inline code`\n", i, i));
        } else if i % 3 == 1 {
            content.push_str(&format!("Regular line {} with no code\n", i));
        } else {
            content.push_str(&format!("Line {} with ``nested `backticks` here``\n", i));
        }
    }
    content
}

fn generate_nested_content(lines: usize) -> String {
    let mut content = String::new();
    content.push_str("# Header with `code`\n\n");

    for i in 0..lines / 4 {
        content.push_str(&format!("Paragraph {} with `inline code` and [link](url)\n", i));
        content.push_str("```rust\n");
        content.push_str("fn example() {\n");
        content.push_str("    // Code block content\n");
        content.push_str("}\n");
        content.push_str("```\n\n");
    }

    for i in 0..lines / 4 {
        content.push_str(&format!("- List item {} with `code`\n", i));
        content.push_str("  - Nested with `more code`\n");
    }

    content
}

fn generate_mixed_content(lines: usize) -> String {
    let mut content = String::new();

    for i in 0..lines {
        match i % 10 {
            0 => content.push_str(&format!("# Heading {} with `code`\n", i)),
            1 => content.push_str("Regular text with `inline code` and **emphasis**\n"),
            2 => content.push_str(&format!("URL: https://example.com/path/{}\n", i)),
            3 => content.push_str(&format!("Email: user{}@example.com\n", i)),
            4 => content.push_str("```\ncode block\n```\n"),
            5 => content.push_str(&format!("HTML: <div>content {}</div>\n", i)),
            6 => content.push_str(&format!("- List with `code {}` item\n", i)),
            7 => content.push_str("Emphasis *with `code`* inside\n"),
            8 => content.push_str(&format!("> Blockquote with `code {}`\n", i)),
            _ => content.push_str(&format!("Plain text line {}\n", i)),
        }
    }

    content
}
