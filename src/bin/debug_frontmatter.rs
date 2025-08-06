use rumdl::rules::front_matter_utils::FrontMatterUtils;
use std::env;
use std::fs;

fn main() {
    let args: Vec<String> = env::args().collect();
    let content = if args.len() > 1 {
        fs::read_to_string(&args[1]).expect("Failed to read file")
    } else {
        r#"+++
title = "My Post"
tags = ["example", "test"]
+++

# Content

[missing] reference should be flagged."#
            .to_string()
    };

    println!("Content:");
    for (i, line) in content.lines().enumerate() {
        println!("Line {}: {}", i + 1, line);
    }
    println!();

    let line_count = content.lines().count();

    // Test with 0-based indexing (what the function actually uses)
    println!("Testing is_in_front_matter with 0-based indexing:");
    for i in 0..line_count {
        let in_frontmatter = FrontMatterUtils::is_in_front_matter(&content, i);
        println!("Line {} (0-based={}): in_frontmatter={}", i + 1, i, in_frontmatter);
    }

    // Test also with 1-based indexing (what we pass from MD052)
    println!("\nTesting is_in_front_matter with 1-based indexing:");
    for i in 1..=line_count {
        let in_frontmatter = FrontMatterUtils::is_in_front_matter(&content, i);
        println!("Line {i}: in_frontmatter={in_frontmatter}");
    }
}
