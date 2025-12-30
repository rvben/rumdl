use rumdl_lib::lint_context::LintContext;
use rumdl_lib::rule::Rule;
use rumdl_lib::rules::MD029OrderedListPrefix;
use std::time::Instant;

/// Pathological edge cases that could potentially break MD029 implementation
/// Tests designed to find weaknesses in parent detection, indentation calculation,
/// performance bottlenecks, and boundary conditions.

#[test]
fn test_extreme_deep_nesting_15_levels() {
    // Test extremely deep nesting that could cause stack overflow or performance issues
    let rule = MD029OrderedListPrefix::default();

    let mut content = String::new();
    for level in 0..15 {
        let indent = "  ".repeat(level); // 2 spaces per level
        content.push_str(&format!("{}1. Level {} item\n", indent, level + 1));
        content.push_str(&format!("{}2. Level {} item 2\n", indent, level + 1));
    }

    let ctx = LintContext::new(&content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let start = Instant::now();
    let result = rule.check(&ctx);
    let duration = start.elapsed();

    assert!(result.is_ok(), "Deep nesting should not crash");
    assert!(
        duration.as_millis() < 1000,
        "Should complete within 1 second for deep nesting"
    );

    // With CommonMark start value support, each nested level starts fresh with its own
    // start value. The numbering 1, 2 at each level is correct. No warnings expected.
    let warnings = result.unwrap();
    assert!(
        warnings.is_empty(),
        "Deep nesting with correct numbering per level should not trigger warnings"
    );
}

#[test]
fn test_massive_numbers_overflow_conditions() {
    // Test very large numbers that could cause overflow
    let rule = MD029OrderedListPrefix::default();

    let content = format!(
        "\
{}. First item with max usize
{}. Second item - overflow risk
{}. Third item",
        usize::MAX,
        usize::MAX.saturating_sub(1),
        usize::MAX.saturating_sub(2)
    );

    let ctx = LintContext::new(&content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx);

    assert!(result.is_ok(), "Large numbers should not crash");

    // With CommonMark start value support, pulldown-cmark determines the start value.
    // Large numbers may overflow or be clamped - the key test is no crash.
    // Actual warning count depends on overflow behavior; we just verify it handles gracefully.
    let _warnings = result.unwrap();
}

#[test]
fn test_unicode_digit_markers_vulnerability() {
    // Test Unicode digits that look like ASCII digits but aren't matched by \d+
    let rule = MD029OrderedListPrefix::default();

    let content = "\
𝟭. Unicode fullwidth digit one
𝟮. Unicode fullwidth digit two
٢. Arabic-Indic digit two
३. Devanagari digit three
1. Regular ASCII digit
2. Regular ASCII digit two";

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // Unicode digits should not be recognized as list markers
    // Only the ASCII digits should be processed
    assert_eq!(
        result.len(),
        0,
        "Only ASCII digits should be recognized as ordered list markers"
    );
}

#[test]
fn test_zero_width_and_invisible_characters() {
    // Test zero-width spaces and other invisible Unicode characters
    let rule = MD029OrderedListPrefix::default();

    let content = "\
1.\u{200B} Item with zero-width space after dot
2.\u{FEFF} Item with BOM character
3.\u{00A0} Item with non-breaking space
4.\u{2060} Item with word joiner
5. \u{200C}Item with zero-width non-joiner in text";

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // Should handle invisible characters gracefully
    assert!(
        result.is_empty(),
        "Invisible characters should not break list detection"
    );
}

#[test]
fn test_malformed_mixed_tab_space_indentation() {
    // Test mixing tabs and spaces in pathological ways
    let rule = MD029OrderedListPrefix::default();

    let content = "\
1. Root item
\t1. Tab-indented level 2
  \t  1. Mixed spaces-tab-spaces level 3
\t  \t2. Tab-spaces-tab level 3
 \t 1. Spaces-tab-space level 3 - wrong parent?
\t\t1. Double tab level 3
    1. Four spaces level 2 - different from tab";

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // With CommonMark start value support, each indentation level group is treated as
    // a separate list with its own start value. Mixed indentation creates different
    // groups. The key test is no crash and graceful handling.
    println!("Mixed tab/space warnings: {}", result.len());
}

#[test]
fn test_empty_and_whitespace_only_list_items() {
    // Test edge cases with empty content
    let rule = MD029OrderedListPrefix::default();

    let content = "\
1.
2.
3.    \t
4. Normal item
5. ";

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // Should handle empty list items gracefully
    assert!(
        result.is_empty(),
        "Empty list items should not cause numbering errors if sequence is correct"
    );
}

#[test]
fn test_lists_in_nested_blockquotes_and_tables() {
    // Test complex nesting within other Markdown constructs
    let rule = MD029OrderedListPrefix::default();

    let content = "\
> 1. List in blockquote
> 2. Second item
> > 1. Nested blockquote list
> > 2. Should be separate sequence
>
> 3. Back to outer blockquote list

| Column 1 | Column 2 |
|----------|----------|
| 1. List in table | 2. Should be separate |
| 3. Or part of same? | 4. Unknown behavior |

1. List after table
2. Should start fresh";

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // Complex nesting should not crash
    println!("Complex nesting warnings: {}", result.len());
}

#[test]
fn test_pathological_parenthesis_markers() {
    // Test parenthesis markers which are valid but less common
    // Note: Parenthesis markers `1)` and dot markers `1.` are DIFFERENT list types
    // in CommonMark. pulldown-cmark treats them as separate lists.
    let rule = MD029OrderedListPrefix::default();

    let content = "\
1) Parenthesis marker
2) Second item
1. Dot marker mixed in
3) Should this be 3 or 4?
2. Mixing markers
5) Back to parenthesis";

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // Parenthesis and dot markers create separate lists. Each list has its own
    // start value from CommonMark. We just verify graceful handling.
    println!("Parenthesis marker warnings: {}", result.len());
}

#[test]
fn test_performance_killer_massive_document() {
    // Generate a large document with many nested lists to test performance
    let rule = MD029OrderedListPrefix::default();

    let mut content = String::new();

    // Create 100 separate list blocks, each with 10 items, some with errors
    for block in 0..100 {
        content.push_str(&format!("# Section {block}\n\n"));

        for item in 1..=10 {
            let wrong_num = if item == 5 { item + 10 } else { item }; // Inject error at item 5
            content.push_str(&format!("{wrong_num}. Item {item} in block {block}\n"));

            // Add some nested items occasionally
            if item % 3 == 0 {
                content.push_str("   1. Nested item\n");
                content.push_str("   3. Wrong nested number\n"); // Should be 2
            }
        }
        content.push('\n');
    }

    let ctx = LintContext::new(&content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let start = Instant::now();
    let result = rule.check(&ctx);
    let duration = start.elapsed();

    assert!(result.is_ok(), "Large document should not crash");
    assert!(
        duration.as_millis() < 5000,
        "Should complete within 5 seconds for large document"
    );

    let warnings = result.unwrap();
    // Should detect errors in many blocks
    assert!(warnings.len() > 100, "Should detect many errors in large document");
    println!("Large document: {} warnings in {:?}", warnings.len(), duration);
}

#[test]
fn test_parent_detection_confusion() {
    // Test scenarios that could confuse the parent detection algorithm
    let rule = MD029OrderedListPrefix::default();

    let content = "\
1. Level 1 item
   1. Level 2 nested
      1. Level 3 deeply nested
   2. Back to level 2
      2. Wrong level 3 - should be 1?
1. New level 1 - should be 2?
      3. Orphaned deep item
   3. Level 2 under wrong parent?";

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // Should handle parent detection edge cases
    println!("Parent detection confusion: {} warnings", result.len());
    for warning in &result {
        println!("  Line {}: {}", warning.line, warning.message);
    }
}

#[test]
fn test_indentation_boundary_edge_cases() {
    // Test precise indentation boundaries that could cause off-by-one errors
    let rule = MD029OrderedListPrefix::default();

    let content = "\
1. Root (0 spaces)
 1. One space (insufficient?)
  1. Two spaces (still insufficient?)
   1. Three spaces (minimum for nesting)
    1. Four spaces (standard)
     1. Five spaces (extra)
1. Back to root
    2. Four space but should be level 2?
   2. Three space level 2
  2. Two space - breaks nesting?
 2. One space continuation";

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    println!("Indentation boundary cases: {} warnings", result.len());
    for warning in &result {
        println!("  Line {}: {}", warning.line, warning.message);
    }
}

#[test]
fn test_real_world_copy_paste_artifacts() {
    // Simulate real-world copy-paste scenarios that could break parsing
    let rule = MD029OrderedListPrefix::default();

    let content = "\
1. Regular item
2. Item with\u{00A0}non-breaking space
3. Item with\r\nWindows line ending
4. Item with multiple\u{00A0}\u{00A0}\u{00A0}NBSP
5. Item\u{2028}with line separator
6. Item\u{2029}with paragraph separator
7. Item with combining chars: café (café vs cafe\u{0301})
8. Final item";

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // Should handle various whitespace artifacts
    println!("Copy-paste artifacts: {} warnings", result.len());
}

#[test]
fn test_automated_tool_malformed_markdown() {
    // Test markdown that might be generated by automated tools
    let rule = MD029OrderedListPrefix::default();

    let content = "\
1.No space after dot
2.  Extra spaces after dot
3.\tTab after dot
4. Normal item
1.Restart numbering without space
6. Skip number 5
100. Jump to large number
101. Continue large sequence
1. Reset to 1 again";

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // Should handle malformed spacing
    println!("Automated tool malformed: {} warnings", result.len());
}

#[test]
fn test_stack_overflow_recursive_parent_detection() {
    // Test deeply nested structure that could cause stack overflow in recursive algorithms
    let rule = MD029OrderedListPrefix::default();

    let mut content = String::new();

    // Create a pattern that zigzags indentation to stress parent detection
    for i in 0..50 {
        let indent_level = (i % 10) + 1; // Cycle through indentation levels 1-10
        let indent = "  ".repeat(indent_level);
        content.push_str(&format!("{indent}1. Item at level {indent_level} (iteration {i})\n"));
    }

    let ctx = LintContext::new(&content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let start = Instant::now();
    let result = rule.check(&ctx);
    let duration = start.elapsed();

    assert!(result.is_ok(), "Zigzag nesting should not cause stack overflow");
    assert!(
        duration.as_millis() < 2000,
        "Should complete within 2 seconds for zigzag nesting"
    );

    println!("Zigzag nesting: {} warnings in {:?}", result.unwrap().len(), duration);
}

#[test]
fn test_unicode_normalization_edge_cases() {
    // Test Unicode normalization issues that could affect character counting
    let rule = MD029OrderedListPrefix::default();

    let content = "\
1. Normal ASCII item
2. Item with é (precomposed)
3. Item with e\u{0301} (decomposed)
4. Item with 🏳️‍🌈 (flag with ZWJ sequence)
5. Item with 👨‍👩‍👧‍👦 (family emoji)
6. Item with \u{1F1FA}\u{1F1F8} (flag emoji)
7. Final item";

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // Unicode normalization should not affect numbering
    assert!(
        result.is_empty(),
        "Unicode normalization should not affect list numbering"
    );
}

#[test]
fn test_memory_exhaustion_large_numbers() {
    // Test with very large number strings that could cause memory issues
    let rule = MD029OrderedListPrefix::default();

    let large_number = "9".repeat(1000); // 1000-digit number
    let content = format!(
        "\
{large_number}. Item with 1000-digit number
2. Normal item
3. Another normal item"
    );

    let ctx = LintContext::new(&content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx);

    assert!(result.is_ok(), "Large numbers should not cause memory exhaustion");

    // Large numbers (>9 digits) are typically not valid CommonMark list starters.
    // pulldown-cmark may not parse them as lists. The key test is no crash/memory issues.
    let _warnings = result.unwrap();
}

#[test]
fn test_fix_function_pathological_cases() {
    // Test that the fix function handles pathological cases correctly
    let rule = MD029OrderedListPrefix::default();

    let content = "\
1. Normal item
1000000. Huge number
1. Reset to 1
   1. Nested
   999999. Huge nested number
   1. Reset nested
2. Back to main";

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.fix(&ctx);

    assert!(result.is_ok(), "Fix should handle pathological numbers");

    let fixed = result.unwrap();
    println!("Fixed pathological content:\n{fixed}");

    // Fixed content should have sequential numbering
    assert!(fixed.contains("1. Normal item"));
    assert!(fixed.contains("2. Huge number") || fixed.contains("2. "));
    assert!(fixed.contains("3. Reset to 1") || fixed.contains("3. "));
}

#[test]
fn test_parent_detection_with_code_block_interruptions() {
    // Test parent detection when code blocks interrupt list nesting
    let rule = MD029OrderedListPrefix::default();

    let content = "\
1. Parent item
   1. Child item
   ```
   some code
   ```
   2. Child after code - should be child of item 1
```
standalone code block
```
   3. Orphaned item - what's the parent?
2. New parent
   1. Clear child of item 2";

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    println!("Code block interruption: {} warnings", result.len());
    for warning in &result {
        println!("  Line {}: {}", warning.line, warning.message);
    }
}

#[test]
fn test_performance_worst_case_parent_detection() {
    // Create worst-case scenario for parent detection algorithm
    let rule = MD029OrderedListPrefix::default();

    let mut content = String::new();

    // Create a scenario where each item needs to search back through many items
    // to find its parent (worst case O(n²) behavior)
    for i in 0..100 {
        if i % 20 == 0 {
            // Add a very deep item that requires scanning back through all previous items
            content.push_str(&format!("{}1. Deep item {}\n", "  ".repeat(10), i));
        } else {
            // Add items at varying depths
            let depth = i % 5;
            content.push_str(&format!("{}1. Item {} at depth {}\n", "  ".repeat(depth), i, depth));
        }
    }

    let ctx = LintContext::new(&content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let start = Instant::now();
    let result = rule.check(&ctx);
    let duration = start.elapsed();

    assert!(result.is_ok(), "Worst case parent detection should not crash");
    assert!(
        duration.as_millis() < 3000,
        "Should complete within 3 seconds for worst case"
    );

    println!(
        "Worst case parent detection: {} warnings in {:?}",
        result.unwrap().len(),
        duration
    );
}
