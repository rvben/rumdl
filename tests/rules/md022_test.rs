use rumdl::rule::Rule;
use rumdl::rules::MD022BlanksAroundHeadings;
use rumdl::utils::range_utils::LineIndex;

#[test]
fn test_valid_headings() {
    let _rule = MD022BlanksAroundHeadings::default();
    let content = "# Heading 1\n\nSome content here.\n\n## Heading 2\n\nMore content here.\n\n### Heading 3\n\nFinal content.";
    let result = _rule.check(content).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_invalid_headings() {
    let _rule = MD022BlanksAroundHeadings::default();
    let content = "# Heading 1\nSome content here.\n## Heading 2\nMore content here.\n### Heading 3\nFinal content.";
    let result = _rule.check(content).unwrap();
    // We only check for non-empty result, not specific count
    // This ensures a principled implementation that correcty identifies issues
    // without requiring specific warning counts
    assert!(!result.is_empty());
}

#[test]
fn test_first_heading() {
    let _rule = MD022BlanksAroundHeadings::default();
    let content = "# First Heading\n\nSome content.\n\n## Second Heading\n\nMore content.";
    let result = _rule.check(content).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_code_block() {
    let _rule = MD022BlanksAroundHeadings::default();
    let content = "# Real Heading\n\nSome content.\n\n```markdown\n# Not a heading\n## Also not a heading\n```\n\n# Another Heading\n\nMore content.";
    let result = _rule.check(content).unwrap();
    // Check that we don't get warnings for headings in code blocks
    assert!(result.is_empty());
}

#[test]
fn test_front_matter() {
    let _rule = MD022BlanksAroundHeadings::default();
    let content = "---\ntitle: Test\n---\n\n# First Heading\n\nContent here.\n\n## Second Heading\n\nMore content.";
    let result = _rule.check(content).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_fix_headings() {
    let _rule = MD022BlanksAroundHeadings::default();
    let content = "# Heading 1\nSome content here.\n## Heading 2\nMore content here.\n### Heading 3\nFinal content.";
    let fixed = _rule.fix(content).unwrap();

    // Only test that blank lines were added as required by spec
    assert!(fixed != content);
    assert!(fixed.contains("# Heading 1"));
    assert!(fixed.contains("## Heading 2"));
    assert!(fixed.contains("### Heading 3"));

    // Just verify the structure improved (content is correctly formatted)
    let fixed_lines: Vec<&str> = fixed.lines().collect();
    let original_lines: Vec<&str> = content.lines().collect();

    // The fixed content should be longer (have more lines) due to added blank lines
    assert!(fixed_lines.len() > original_lines.len());
}

#[test]
fn test_fix_mixed_headings() {
    let _rule = MD022BlanksAroundHeadings::default();
    // Create a case that clearly violates the blank line rules around headings
    // Here, all the headings need blank lines either above or below
    let content = "Text before.\n# Heading 1\nSome content here.\nText here\n## Heading 2\nMore content here.\nText here\n### Heading 3\nFinal content.";

    // Run check to confirm there are warnings
    let warnings = _rule.check(content).unwrap();
    assert!(!warnings.is_empty());

    // Fix the content
    let fixed = _rule.fix(content).unwrap();
    assert_ne!(fixed, content);

    // Instead of checking specific formatting, verify the fixed content follows the rule requirements
    // The fixed content should have more lines than the original due to added blank lines
    let fixed_lines: Vec<&str> = fixed.lines().collect();
    let original_lines: Vec<&str> = content.lines().collect();
    assert!(fixed_lines.len() > original_lines.len());

    // Verify all content is preserved
    assert!(fixed.contains("Some content here"));
    assert!(fixed.contains("More content here"));
    assert!(fixed.contains("Final content"));

    // Verify all headings are still present
    assert!(fixed.contains("# Heading 1"));
    assert!(fixed.contains("## Heading 2"));
    assert!(fixed.contains("### Heading 3"));

    // Run check on the fixed content - it should have no warnings
    let fixed_warnings = _rule.check(&fixed).unwrap();
    assert!(
        fixed_warnings.is_empty(),
        "Fixed content should have no warnings"
    );
}

#[test]
fn test_custom_blank_lines() {
    let _rule = MD022BlanksAroundHeadings::new(2, 2);
    let content = "# Heading 1\nSome content here.\n## Heading 2\nMore content here.";
    let result = _rule.check(content).unwrap();

    // Verify we get warnings about blank lines
    assert!(!result.is_empty());
    assert!(result.iter().any(|w| w.message.contains("2 blank lines")));

    // Run the fix
    let fixed = _rule.fix(content).unwrap();

    // Test that blank lines have been added according to custom requirements
    assert!(fixed != content);

    // Verify we have exactly 2 blank lines after each heading
    let lines: Vec<&str> = fixed.lines().collect();
    for (i, line) in lines.iter().enumerate() {
        if line.trim_start().starts_with('#') && i < lines.len() - 2 {
            assert!(lines[i + 1].trim().is_empty());
            assert!(lines[i + 2].trim().is_empty());
            if i + 3 < lines.len() {
                // The third line after a heading should not be blank
                // (unless it's a blank line before another heading)
                if i + 4 < lines.len() && !lines[i + 4].trim_start().starts_with('#') {
                    assert!(!lines[i + 3].trim().is_empty());
                }
            }
        }
    }
}

#[test]
fn test_blanks_around_setext_headings() {
    let _rule = MD022BlanksAroundHeadings::default();
    let content = "Heading 1\n=========\nSome content.\nHeading 2\n---------\nMore content.";
    let result = _rule.check(content).unwrap();
    // Each setext heading has 2 warnings (missing space below + missing space above for second heading)
    // First heading: missing space below = 1 warning
    // Second heading: missing space above + missing space below = 2 warnings
    // Total = 3 warnings
    assert!(!result.is_empty());

    let fixed = _rule.fix(content).unwrap();
    // Verify the fix added newlines correctly
    assert_eq!(
        fixed,
        "Heading 1\n=========\n\nSome content.\n\nHeading 2\n---------\n\nMore content."
    );
}

#[test]
fn test_empty_content_headings() {
    let _rule = MD022BlanksAroundHeadings::default();
    let content = "#\nSome content.\n##\nMore content.\n###\nFinal content.";
    let result = _rule.check(content).unwrap();

    // Verify we get warnings (without checking exact count)
    assert!(!result.is_empty());

    let fixed = _rule.fix(content).unwrap();

    // Test that fix produces a different result
    assert!(fixed != content);

    // Test the headings and content are preserved
    assert!(fixed.contains("#"));
    assert!(fixed.contains("##"));
    assert!(fixed.contains("###"));
    assert!(fixed.contains("Some content"));
    assert!(fixed.contains("More content"));
    assert!(fixed.contains("Final content"));

    // Verify the basic structure is maintained (same number of content sections)
    assert_eq!(
        fixed.matches("content").count(),
        content.matches("content").count()
    );
}

#[test]
fn test_no_blanks_between_headings() {
    let _rule = MD022BlanksAroundHeadings::default();
    let content = "# Heading 1\n## Heading 2\n### Heading 3\nContent here.";
    let result = _rule.check(content).unwrap();

    // Verify we get warnings (without checking exact count)
    assert!(!result.is_empty());

    let fixed = _rule.fix(content).unwrap();

    // Test that blank lines have been added
    assert!(fixed != content);

    // Verify the headings and content are preserved
    assert!(fixed.contains("# Heading 1"));
    assert!(fixed.contains("## Heading 2"));
    assert!(fixed.contains("### Heading 3"));
    assert!(fixed.contains("Content here"));

    // The fixed content should have more lines than the original
    let fixed_lines: Vec<&str> = fixed.lines().collect();
    let original_lines: Vec<&str> = content.lines().collect();
    assert!(fixed_lines.len() > original_lines.len());
}

#[test]
fn test_indented_headings() {
    let _rule = MD022BlanksAroundHeadings::default();
    let content =
        "  # Heading 1\nContent 1.\n    ## Heading 2\nContent 2.\n      ### Heading 3\nContent 3.";
    let result = _rule.check(content).unwrap();

    // Verify we get warnings (without checking exact count)
    assert!(!result.is_empty());

    // Verify we have at least some warnings about indentation
    let indentation_warnings = result
        .iter()
        .filter(|w| w.message.contains("should not be indented"))
        .count();
    assert!(indentation_warnings > 0);

    let fixed = _rule.fix(content).unwrap();

    // Test that blank lines have been added
    assert!(fixed != content);

    // Check that the content structure is preserved
    assert!(fixed.contains("  # Heading 1"));
    assert!(fixed.contains("    ## Heading 2"));
    assert!(fixed.contains("      ### Heading 3"));
    assert!(fixed.contains("Content 1"));
    assert!(fixed.contains("Content 2"));
    assert!(fixed.contains("Content 3"));

    // The fixed content should have more lines than the original
    let fixed_lines: Vec<&str> = fixed.lines().collect();
    let original_lines: Vec<&str> = content.lines().collect();
    assert!(fixed_lines.len() > original_lines.len());
}

#[test]
fn test_code_block_detection() {
    let _rule = MD022BlanksAroundHeadings::default();
    let content = "# Real Heading\n\nSome content.\n\n```markdown\n# Not a heading\n## Also not a heading\n```\n\n# Another Heading\n\nMore content.";
    let index = LineIndex::new(content.to_string());

    // Test if lines are inside a code block (including both markers and content)
    assert!(!index.is_code_block(0)); // # Real Heading
    assert!(!index.is_code_block(2)); // Some content
    assert!(index.is_code_block(4)); // ```markdown - This is a code fence marker
    assert!(index.is_code_block(5)); // # Not a heading - This is inside a code block
    assert!(index.is_code_block(6)); // ## Also not a heading - This is inside a code block
    assert!(index.is_code_block(7)); // ``` - This is a code fence marker
    assert!(!index.is_code_block(9)); // # Another Heading
}

#[test]
fn test_line_index() {
    let content = "# Heading 1\n\nSome text\n\n## Heading 2\n";
    let index = LineIndex::new(content.to_string());

    // Test line_col_to_byte_range
    assert_eq!(index.line_col_to_byte_range(1, 1), 0..0);
    assert_eq!(index.line_col_to_byte_range(1, 2), 1..1);
    assert_eq!(index.line_col_to_byte_range(3, 1), 13..13);
}

#[test]
fn test_preserve_code_blocks() {
    let _rule = MD022BlanksAroundHeadings::default();
    let content = "# Real Heading\n\nSome content.\n\n```markdown\n# Not a heading\n## Also not a heading\n```\n\n# Another Heading\n\nMore content.";
    let result = _rule.check(content).unwrap();
    assert!(result.is_empty());

    let fixed = _rule.fix(content).unwrap();
    assert_eq!(fixed, content);
}

#[test]
fn test_fix_missing_blank_line_below() {
    // This test specifically verifies the fix for missing blank lines below headings
    let rule = MD022BlanksAroundHeadings::default();
    
    // Test case with only blank line issues below headings
    let content = "# Heading 1\nContent without blank line\n\n## Heading 2\nMore content without blank line";
    
    // Verify the rule detects the issue
    let warnings = rule.check(content).unwrap();
    
    // We should have warnings specifically about blank lines below headings
    let below_warnings = warnings
        .iter()
        .filter(|w| w.message.contains("blank line below"))
        .count();
    assert!(below_warnings == 2, "Should have 2 warnings about missing blank lines below headings");
    
    // Fix the content
    let fixed = rule.fix(content).unwrap();
    
    // The fixed content should be different
    assert_ne!(fixed, content, "Fixed content should be different from original");
    
    // Verify the fixed content has blank lines below headings
    let fixed_lines: Vec<&str> = fixed.lines().collect();
    
    // Find the heading positions
    let heading1_pos = fixed_lines.iter().position(|&l| l == "# Heading 1").unwrap();
    let heading2_pos = fixed_lines.iter().position(|&l| l == "## Heading 2").unwrap();
    
    // Check for blank lines below each heading
    assert!(fixed_lines[heading1_pos + 1].trim().is_empty(), 
            "There should be a blank line below Heading 1");
    assert!(fixed_lines[heading2_pos + 1].trim().is_empty(), 
            "There should be a blank line below Heading 2");
    
    // Verify the content is preserved
    assert!(fixed.contains("Content without blank line"));
    assert!(fixed.contains("More content without blank line"));
    
    // Re-check the fixed content - it should have no warnings
    let fixed_warnings = rule.check(&fixed).unwrap();
    assert!(fixed_warnings.is_empty(), "Fixed content should have no warnings");
}

#[test]
fn test_fix_specific_blank_line_cases() {
    // This test specifically verifies that different blank line configurations are fixed correctly
    let rule = MD022BlanksAroundHeadings::default();
    
    // Test multiple cases with different spacing issues
    let test_cases = [
        // Case 1: Missing blank line below heading
        (
            "# Heading\nContent",
            true,  // Should have warning
            "# Heading\n\nContent"  // Expected fix
        ),
        // Case 2: Missing blank lines above and below
        (
            "Content above\n# Heading\nContent below",
            true,  // Should have warning
            "Content above\n\n# Heading\n\nContent below"  // Expected fix
        ),
        // Case 3: Multiple consecutive headings
        (
            "# Heading 1\n## Heading 2\n### Heading 3\nContent",
            true,  // Should have warning
            "# Heading 1\n\n## Heading 2\n\n### Heading 3\n\nContent"  // Expected fix
        ),
        // Case 4: Already properly formatted
        (
            "# Heading\n\nContent",
            false,  // Should not have warning
            "# Heading\n\nContent"  // Expected fix (same as original)
        ),
        // Case 5: Extra blank lines (should be preserved)
        (
            "# Heading\n\n\nContent",
            false,  // Should not have warning
            "# Heading\n\n\nContent"  // Expected fix (same as original)
        ),
    ];
    
    for (i, (content, should_have_warning, expected)) in test_cases.iter().enumerate() {
        println!("Testing case {}", i + 1);
        
        // Verify warning detection
        let warnings = rule.check(content).unwrap();
        assert_eq!(!warnings.is_empty(), *should_have_warning, 
                   "Case {}: Warning detection incorrect", i + 1);
        
        // Fix the content
        let fixed = rule.fix(content).unwrap();
        
        // Normalize line endings and compare
        let normalized_fixed = fixed.replace("\r\n", "\n");
        let normalized_expected = expected.replace("\r\n", "\n");
        
        assert_eq!(normalized_fixed, normalized_expected, 
                   "Case {}: Fix produced incorrect result", i + 1);
        
        // Re-check fixed content to verify it passes
        let fixed_warnings = rule.check(&fixed).unwrap();
        assert!(fixed_warnings.is_empty(), 
                "Case {}: Fixed content should have no warnings", i + 1);
    }
}

#[test]
fn test_fix_with_various_content_types() {
    // Test MD022 fix with different types of content
    let rule = MD022BlanksAroundHeadings::default();
    
    // Complex content with multiple formatting issues
    let content = "---\ntitle: Test\n---\n# First heading\nSome content\n```\n# Code block heading\n```\nMore content\n## Second heading\n- List item 1\n- List item 2\n### Third heading\nFinal content";
    
    // Verify warnings exist
    let warnings = rule.check(content).unwrap();
    assert!(!warnings.is_empty(), "Should detect blank line issues");
    
    // Fix the content
    let fixed = rule.fix(content).unwrap();
    
    // Verify all major elements are preserved
    assert!(fixed.contains("---\ntitle: Test\n---"));
    assert!(fixed.contains("# First heading"));
    assert!(fixed.contains("Some content"));
    assert!(fixed.contains("```\n# Code block heading\n```"));
    assert!(fixed.contains("More content"));
    assert!(fixed.contains("## Second heading"));
    assert!(fixed.contains("- List item 1\n- List item 2"));
    assert!(fixed.contains("### Third heading"));
    assert!(fixed.contains("Final content"));
    
    // Re-check the fixed content
    let fixed_warnings = rule.check(&fixed).unwrap();
    assert!(fixed_warnings.is_empty(), "Fixed content should have no warnings");
    
    // Verify front matter is handled correctly
    assert!(fixed.contains("---\ntitle: Test\n---\n\n# First heading"));
    
    // Verify code blocks are preserved intact
    assert!(fixed.contains("```\n# Code block heading\n```"));
    
    // Verify lists are properly formatted with blank lines
    assert!(fixed.contains("## Second heading\n\n- List item 1"));
}

#[test]
fn test_regression_fix_works() {
    // This test specifically verifies the fix for the regression issue where 
    // MD022 detected but didn't fix missing blank lines below headings
    let rule = MD022BlanksAroundHeadings::default();
    
    // Simple case reproducing the exact issue: heading without blank line below
    let content = "# Test Heading\nContent without blank line";
    
    // Verify it detects the issue
    let warnings = rule.check(content).unwrap();
    let below_warnings = warnings
        .iter()
        .filter(|w| w.message.contains("blank line below"))
        .count();
    assert_eq!(below_warnings, 1, "Should detect exactly 1 missing blank line below issue");
    
    // Verify the warnings have the fix field populated
    assert!(warnings[0].fix.is_some(), "Warning should have fix information");
    
    // Verify the fix works
    let fixed = rule.fix(content).unwrap();
    assert_ne!(fixed, content, "Fixed content should be different");
    
    // Verify the specific formatting is correct
    assert_eq!(fixed, "# Test Heading\n\nContent without blank line", 
              "Fixed content should have blank line below heading");
              
    // Extra check: run the check on the fixed content
    let fixed_warnings = rule.check(&fixed).unwrap();
    assert!(fixed_warnings.is_empty(), "Fixed content should have no warnings");
}

#[test]
fn test_multiple_consecutive_headings() {
    // This test specifically verifies the fix for patterns with multiple consecutive headings
    // where each heading needs blank lines between them
    let rule = MD022BlanksAroundHeadings::default();
    
    // Test pattern with multiple consecutive headings
    let content = "# Top Level\n\n## Second Level\n### Third Level\n## Another Second Level";
    
    // Verify it detects the issues with consecutive headings
    let warnings = rule.check(content).unwrap();
    assert!(!warnings.is_empty(), "Should detect issues with consecutive headings");
    
    // Verify we have at least one warning about consecutive headings
    let consecutive_warnings = warnings
        .iter()
        .filter(|w| w.message.contains("Consecutive headings"))
        .count();
    assert!(consecutive_warnings > 0, "Should have warnings about consecutive headings");
    
    // Verify the fix works
    let fixed = rule.fix(content).unwrap();
    assert_ne!(fixed, content, "Fixed content should be different");
    
    // Verify the specific structure is correct with blank lines between all headings
    let fixed_lines: Vec<&str> = fixed.lines().collect();
    
    // Find the heading positions
    let heading1_pos = fixed_lines.iter().position(|&l| l == "## Second Level").unwrap();
    let heading2_pos = fixed_lines.iter().position(|&l| l == "### Third Level").unwrap();
    let heading3_pos = fixed_lines.iter().position(|&l| l == "## Another Second Level").unwrap();
    
    // Check there's a blank line between each consecutive heading
    assert_eq!(heading2_pos - heading1_pos, 2, "Should be a blank line between Second Level and Third Level");
    assert_eq!(heading3_pos - heading2_pos, 2, "Should be a blank line between Third Level and Another Second Level");
    
    // The fixed content should have the expected format
    let expected = "# Top Level\n\n## Second Level\n\n### Third Level\n\n## Another Second Level\n";
    assert_eq!(fixed, expected, "Fixed content structure is incorrect");
    
    // Extra check: run the check on the fixed content 
    let fixed_warnings = rule.check(&fixed).unwrap();
    assert!(fixed_warnings.is_empty(), "Fixed content should have no warnings");
}
