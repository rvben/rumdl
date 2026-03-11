//! Performance stress tests for deeply nested list structures

use rumdl_lib::lint_context::LintContext;
use rumdl_lib::rule::Rule;
use rumdl_lib::rules::MD032BlanksAroundLists;
use std::time::Instant;

/// Performance stress tests for deeply nested list structures
/// Tests parsing, validation, and fixing performance with extreme nesting scenarios

#[test]
fn test_deeply_nested_unordered_lists_performance() {
    // Test with 15 levels of nested unordered lists, 5 items per level
    let content = generate_deeply_nested_unordered_lists(15, 5);

    let start = Instant::now();
    let ctx = LintContext::new(&content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let parsing_duration = start.elapsed();

    // Should parse within reasonable time (< 100ms for 15 levels)
    assert!(
        parsing_duration.as_millis() < 100,
        "Parsing took {}ms, expected < 100ms",
        parsing_duration.as_millis()
    );

    // Verify list blocks are detected correctly
    assert!(!ctx.list_blocks.is_empty(), "Should detect list blocks");

    // Test rule performance
    let rule = MD032BlanksAroundLists::default();
    let rule_start = Instant::now();
    let warnings = rule.check(&ctx).unwrap();
    let rule_duration = rule_start.elapsed();

    assert!(
        rule_duration.as_millis() < 50,
        "Rule checking took {}ms, expected < 50ms",
        rule_duration.as_millis()
    );

    println!(
        "Deeply nested unordered lists (15 levels): parse={}ms, rule={}ms, warnings={}",
        parsing_duration.as_millis(),
        rule_duration.as_millis(),
        warnings.len()
    );
}

#[test]
fn test_deeply_nested_ordered_lists_performance() {
    // Test with 12 levels of nested ordered lists, 8 items per level
    let content = generate_deeply_nested_ordered_lists(12, 8);

    let start = Instant::now();
    let ctx = LintContext::new(&content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let parsing_duration = start.elapsed();

    assert!(
        parsing_duration.as_millis() < 150,
        "Parsing took {}ms, expected < 150ms",
        parsing_duration.as_millis()
    );

    // Test rule performance with ordered lists
    let rule = MD032BlanksAroundLists::default();
    let rule_start = Instant::now();
    let warnings = rule.check(&ctx).unwrap();
    let rule_duration = rule_start.elapsed();

    assert!(
        rule_duration.as_millis() < 75,
        "Rule checking took {}ms, expected < 75ms",
        rule_duration.as_millis()
    );

    println!(
        "Deeply nested ordered lists (12 levels): parse={}ms, rule={}ms, warnings={}",
        parsing_duration.as_millis(),
        rule_duration.as_millis(),
        warnings.len()
    );
}

#[test]
fn test_mixed_deeply_nested_lists_performance() {
    // Test alternating ordered/unordered lists with deep nesting
    let content = generate_mixed_deeply_nested_lists(10, 6);

    let start = Instant::now();
    let ctx = LintContext::new(&content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let parsing_duration = start.elapsed();

    assert!(
        parsing_duration.as_millis() < 120,
        "Parsing took {}ms, expected < 120ms",
        parsing_duration.as_millis()
    );

    // Verify complex nested structure is parsed correctly
    assert!(ctx.list_blocks.len() >= 2, "Should detect multiple list blocks");

    let rule = MD032BlanksAroundLists::default();
    let rule_start = Instant::now();
    let warnings = rule.check(&ctx).unwrap();
    let rule_duration = rule_start.elapsed();

    assert!(
        rule_duration.as_millis() < 60,
        "Rule checking took {}ms, expected < 60ms",
        rule_duration.as_millis()
    );

    println!(
        "Mixed deeply nested lists (10 levels): parse={}ms, rule={}ms, warnings={}",
        parsing_duration.as_millis(),
        rule_duration.as_millis(),
        warnings.len()
    );
}

#[test]
fn test_extremely_wide_nested_lists_performance() {
    // Test with moderate nesting but very wide (many items per level)
    let content = generate_wide_nested_lists(6, 50);

    let start = Instant::now();
    let ctx = LintContext::new(&content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let parsing_duration = start.elapsed();

    assert!(
        parsing_duration.as_millis() < 200,
        "Parsing took {}ms, expected < 200ms",
        parsing_duration.as_millis()
    );

    // Test performance with large number of list items
    let rule = MD032BlanksAroundLists::default();
    let rule_start = Instant::now();
    let warnings = rule.check(&ctx).unwrap();
    let rule_duration = rule_start.elapsed();

    assert!(
        rule_duration.as_millis() < 100,
        "Rule checking took {}ms, expected < 100ms",
        rule_duration.as_millis()
    );

    println!(
        "Wide nested lists (6 levels, 50 items): parse={}ms, rule={}ms, warnings={}",
        parsing_duration.as_millis(),
        rule_duration.as_millis(),
        warnings.len()
    );
}

#[test]
fn test_pathological_nesting_with_content_performance() {
    // Test deeply nested lists with code blocks, blockquotes, and other content
    let content = generate_pathological_nested_content(8);

    let start = Instant::now();
    let ctx = LintContext::new(&content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let parsing_duration = start.elapsed();

    // More lenient for complex content parsing
    assert!(
        parsing_duration.as_millis() < 300,
        "Parsing took {}ms, expected < 300ms",
        parsing_duration.as_millis()
    );

    let rule = MD032BlanksAroundLists::default();
    let rule_start = Instant::now();
    let warnings = rule.check(&ctx).unwrap();
    let rule_duration = rule_start.elapsed();

    assert!(
        rule_duration.as_millis() < 150,
        "Rule checking took {}ms, expected < 150ms",
        rule_duration.as_millis()
    );

    println!(
        "Pathological nested content (8 levels): parse={}ms, rule={}ms, warnings={}",
        parsing_duration.as_millis(),
        rule_duration.as_millis(),
        warnings.len()
    );
}

#[test]
fn test_fix_performance_on_deeply_nested_lists() {
    // Test fix performance on deeply nested lists that need blank line fixes
    let content = generate_nested_lists_needing_fixes(10, 4);

    let ctx = LintContext::new(&content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let rule = MD032BlanksAroundLists::default();

    // Ensure there are warnings to fix
    let warnings = rule.check(&ctx).unwrap();
    assert!(!warnings.is_empty(), "Should have warnings to fix");

    let fix_start = Instant::now();
    let fixed_content = rule.fix(&ctx).unwrap();
    let fix_duration = fix_start.elapsed();

    assert!(
        fix_duration.as_millis() < 100,
        "Fix took {}ms, expected < 100ms",
        fix_duration.as_millis()
    );

    // Verify fix actually worked
    assert_ne!(fixed_content, content, "Content should be modified by fix");

    // Verify fix resolves issues
    let fixed_ctx = LintContext::new(&fixed_content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let warnings_after_fix = rule.check(&fixed_ctx).unwrap();
    assert!(
        warnings_after_fix.len() < warnings.len(),
        "Fix should reduce number of warnings"
    );

    println!(
        "Fix on nested lists needing fixes (10 levels): fix={}ms, warnings_before={}, warnings_after={}",
        fix_duration.as_millis(),
        warnings.len(),
        warnings_after_fix.len()
    );
}

#[test]
fn test_memory_usage_with_extreme_nesting() {
    // Test that deeply nested lists don't cause excessive memory usage
    let content = generate_memory_stress_lists(20, 3);

    let start = Instant::now();
    let ctx = LintContext::new(&content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let parsing_duration = start.elapsed();

    // Verify the structure is parsed without panic or excessive time
    assert!(
        parsing_duration.as_millis() < 500,
        "Memory stress test took {}ms, expected < 500ms",
        parsing_duration.as_millis()
    );

    // Verify basic functionality still works
    assert!(
        !ctx.list_blocks.is_empty(),
        "Should detect list blocks even with extreme nesting"
    );

    let rule = MD032BlanksAroundLists::default();
    let rule_start = Instant::now();
    let _warnings = rule.check(&ctx).unwrap(); // Don't panic
    let rule_duration = rule_start.elapsed();

    assert!(
        rule_duration.as_millis() < 200,
        "Rule on memory stress test took {}ms, expected < 200ms",
        rule_duration.as_millis()
    );

    println!(
        "Memory stress test (20 levels): parse={}ms, rule={}ms",
        parsing_duration.as_millis(),
        rule_duration.as_millis()
    );
}

// Helper functions to generate test content

fn generate_deeply_nested_unordered_lists(max_depth: usize, items_per_level: usize) -> String {
    let mut content = String::new();
    content.push_str("# Test Document\n\n");

    for depth in 0..max_depth {
        let indent = "  ".repeat(depth);
        for item in 1..=items_per_level {
            content.push_str(&format!(
                "{}* List item at depth {} number {}\n",
                indent,
                depth + 1,
                item
            ));
            if item == 1 {
                content.push_str(&format!("{indent}  Additional content for item {item}\n"));
            }
        }
        if depth < max_depth - 1 {
            content.push('\n');
        }
    }

    content.push_str("\n\nEnd of document.\n");
    content
}

fn generate_deeply_nested_ordered_lists(max_depth: usize, items_per_level: usize) -> String {
    let mut content = String::new();
    content.push_str("# Ordered Lists Test\n\n");

    for depth in 0..max_depth {
        let indent = "   ".repeat(depth); // 3 spaces for ordered list indentation
        for item in 1..=items_per_level {
            content.push_str(&format!(
                "{}{}. Ordered item at depth {} number {}\n",
                indent,
                item,
                depth + 1,
                item
            ));
            if item == items_per_level / 2 {
                content.push_str(&format!("{indent}   Continuation paragraph for item {item}\n"));
            }
        }
        if depth < max_depth - 1 {
            content.push('\n');
        }
    }

    content.push_str("\n\nEnd of ordered test.\n");
    content
}

fn generate_mixed_deeply_nested_lists(max_depth: usize, items_per_level: usize) -> String {
    let mut content = String::new();
    content.push_str("# Mixed Lists Test\n\n");

    for depth in 0..max_depth {
        let indent = "  ".repeat(depth);
        let is_ordered = depth % 2 == 0;

        for item in 1..=items_per_level {
            if is_ordered {
                content.push_str(&format!("{indent}{item}. Mixed ordered item {item}\n"));
            } else {
                content.push_str(&format!("{indent}- Mixed unordered item {item}\n"));
            }
        }

        if depth < max_depth - 1 {
            content.push('\n');
        }
    }

    content.push_str("\n\nEnd of mixed test.\n");
    content
}

fn generate_wide_nested_lists(max_depth: usize, items_per_level: usize) -> String {
    let mut content = String::new();
    content.push_str("# Wide Lists Test\n\n");

    for depth in 0..max_depth {
        let indent = "  ".repeat(depth);

        for item in 1..=items_per_level {
            content.push_str(&format!("{}* Wide item {} at depth {}\n", indent, item, depth + 1));

            // Add some variety to stress test parsing
            if item % 10 == 0 {
                content.push_str(&format!("{indent}  Special content for item {item}\n"));
            }
        }

        if depth < max_depth - 1 {
            content.push('\n');
        }
    }

    content.push_str("\n\nEnd of wide test.\n");
    content
}

fn generate_pathological_nested_content(max_depth: usize) -> String {
    let mut content = String::new();
    content.push_str("# Pathological Content Test\n\n");

    for depth in 0..max_depth {
        let indent = "  ".repeat(depth);

        // Mix different types of content within nested lists
        content.push_str(&format!("{}* Complex item at depth {}\n", indent, depth + 1));
        content.push_str(&format!("{indent}  \n")); // Blank line
        content.push_str(&format!("{indent}  > Blockquote within list\n"));
        content.push_str(&format!("{indent}  > Second line of quote\n"));
        content.push_str(&format!("{indent}  \n"));
        content.push_str(&format!("{indent}  ```code\n"));
        content.push_str(&format!("{indent}  Code block within list\n"));
        content.push_str(&format!("{indent}  ```\n"));
        content.push_str(&format!("{indent}  \n"));
        content.push_str(&format!("{indent}  Another paragraph in the list item.\n"));

        if depth < max_depth - 1 {
            content.push('\n');
        }
    }

    content.push_str("\n\nEnd of pathological test.\n");
    content
}

fn generate_nested_lists_needing_fixes(max_depth: usize, items_per_level: usize) -> String {
    let mut content = String::new();
    content.push_str("# Lists Needing Fixes\n");

    for depth in 0..max_depth {
        let indent = "  ".repeat(depth);

        // Deliberately create content that needs MD032 fixes (missing blank lines)
        for item in 1..=items_per_level {
            content.push_str(&format!("{}* Item {} at depth {}\n", indent, item, depth + 1));
        }

        // Add content immediately after list without blank line (should trigger MD032)
        content.push_str(&format!(
            "{}Paragraph immediately after list at depth {}\n",
            " ".repeat(depth * 2),
            depth + 1
        ));

        if depth < max_depth - 1 {
            content.push_str(&format!("{indent}* Next list starts immediately\n"));
        }
    }

    content.push_str("Final paragraph.\n");
    content
}

fn generate_memory_stress_lists(max_depth: usize, items_per_level: usize) -> String {
    let mut content = String::new();
    content.push_str("# Memory Stress Test\n\n");

    // Generate extremely deep nesting to stress test memory usage
    for depth in 0..max_depth {
        let indent = "  ".repeat(depth);

        for item in 1..=items_per_level {
            content.push_str(&format!("{}* Deep item {} (depth {})\n", indent, item, depth + 1));

            // Add content that exercises different parsing paths
            if depth % 3 == 0 {
                content.push_str(&format!("{indent}  **Bold text** in item {item}\n"));
            }
            if depth % 5 == 0 {
                content.push_str(&format!("{indent}  [Link text](https://example.com/{depth}/{item})\n"));
            }
        }
    }

    content.push_str("\n\nEnd of memory stress test.\n");
    content
}
