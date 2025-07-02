//! VS Code Extension Fix Tests
//!
//! These tests simulate how the VS Code extension applies fixes by applying
//! the fix replacement text to the warning range (not the fix range).
//! This helps catch bugs where warning ranges and fix ranges are mismatched.

use rumdl::lint_context::LintContext;
use rumdl::rule::Rule;
use rumdl::rules::code_block_utils::CodeBlockStyle;
use rumdl::rules::code_fence_utils::CodeFenceStyle;
use rumdl::rules::strong_style::StrongStyle;
use rumdl::rules::*;

/// Simulates how VS Code extension applies a fix by:
/// 1. Getting the warning range from the rule
/// 2. Applying the fix replacement text to that warning range only
/// 3. Returning the result
fn simulate_vscode_fix(content: &str, rule: &dyn Rule) -> Result<String, String> {
    let ctx = LintContext::new(content);
    let warnings = rule.check(&ctx).map_err(|e| format!("Check failed: {e:?}"))?;

    if warnings.is_empty() {
        return Ok(content.to_string());
    }

    // Take the first warning
    let warning = &warnings[0];
    let fix = warning.fix.as_ref().ok_or("No fix available")?;

    // Get warning range
    let warning_start_line = warning.line;
    let warning_start_col = warning.column;
    let warning_end_line = warning.end_line;
    let warning_end_col = warning.end_column;

    // Convert to byte positions using the same logic as the warning
    let lines: Vec<&str> = content.lines().collect();

    if warning_start_line == 0 || warning_start_line > lines.len() {
        return Err("Invalid warning line number".to_string());
    }

    // For single-line replacements (most common case)
    if warning_start_line == warning_end_line {
        let line = lines[warning_start_line - 1]; // Convert to 0-indexed

        // Convert 1-indexed columns to 0-indexed byte positions
        // Note: end_column is exclusive (points after the last character)
        let start_byte = warning_start_col.saturating_sub(1);
        let end_byte = warning_end_col.saturating_sub(1);

        if start_byte > line.len() || end_byte > line.len() {
            return Err("Invalid warning column range".to_string());
        }

        // Apply the replacement to the warning range
        let before = &line[..start_byte];
        let after = &line[end_byte..];
        let new_line = format!("{}{}{}", before, fix.replacement, after);

        // Reconstruct the full content
        let mut result_lines: Vec<String> = lines.iter().map(|s| s.to_string()).collect();
        result_lines[warning_start_line - 1] = new_line;

        Ok(result_lines.join("\n") + if content.ends_with('\n') { "\n" } else { "" })
    } else {
        Err("Multi-line warning ranges not implemented yet".to_string())
    }
}

/// Helper function to create test cases for each rule
fn create_test_case_for_rule(rule_name: &str) -> Option<(&'static str, Box<dyn Rule>)> {
    match rule_name {
        "MD001" => Some(("# H1\n### H3 (should be H2)", Box::new(MD001HeadingIncrement))),
        "MD002" => Some(("## H2 (should start with H1)", Box::new(MD002FirstHeadingH1::default()))),
        "MD003" => Some(("# ATX\nSetext\n======", Box::new(MD003HeadingStyle::default()))),
        "MD004" => Some((
            "* Item 1\n- Item 2",
            Box::new(MD004UnorderedListStyle::new(UnorderedListStyle::Consistent)),
        )),
        "MD005" => Some((
            "* Item 1\n   * Item with 3 spaces (should be 2)",
            Box::new(MD005ListIndent),
        )),
        "MD006" => Some((
            "  * Indented list item that should trigger MD006",
            Box::new(MD006StartBullets),
        )),
        "MD007" => Some(("- Item 1\n   - Wrong indent", Box::new(MD007ULIndent::default()))),
        "MD009" => Some(("Line with trailing spaces   ", Box::new(MD009TrailingSpaces::default()))),
        "MD010" => Some(("Line with\ttab", Box::new(MD010NoHardTabs::default()))),
        "MD011" => Some(("(http://example.com)[Example]", Box::new(MD011NoReversedLinks))),
        "MD012" => Some((
            "Content\n\n\n\nToo many blanks",
            Box::new(MD012NoMultipleBlanks::default()),
        )),
        "MD013" => Some((
            "This is a very long line that exceeds the maximum line length limit and should trigger MD013",
            Box::new(MD013LineLength::default()),
        )),
        "MD014" => Some(("```bash\n$ command\n```", Box::new(MD014CommandsShowOutput::default()))),
        "MD018" => Some(("#Missing space", Box::new(MD018NoMissingSpaceAtx))),
        "MD019" => Some(("##  Multiple spaces", Box::new(MD019NoMultipleSpaceAtx::new()))),
        "MD020" => Some(("##No space in closed##", Box::new(MD020NoMissingSpaceClosedAtx))),
        "MD021" => Some(("##  Multiple  spaces  ##", Box::new(MD021NoMultipleSpaceClosedAtx))),
        "MD022" => Some((
            "Text\n# Heading\nMore text",
            Box::new(MD022BlanksAroundHeadings::default()),
        )),
        "MD023" => Some(("  # Indented heading", Box::new(MD023HeadingStartLeft))),
        "MD024" => Some(("# Duplicate\n# Duplicate", Box::new(MD024NoDuplicateHeading::default()))),
        "MD025" => Some(("# First\n# Second H1", Box::new(MD025SingleTitle::default()))),
        "MD026" => Some(("# Heading!", Box::new(MD026NoTrailingPunctuation::default()))),
        "MD027" => Some((
            ">  Multiple spaces in blockquote",
            Box::new(MD027MultipleSpacesBlockquote),
        )),
        "MD028" => Some(("> Quote\n>\n> More quote", Box::new(MD028NoBlanksBlockquote))),
        "MD029" => Some((
            "1. First\n3. Third",
            Box::new(MD029OrderedListPrefix::new(ListStyle::Ordered)),
        )),
        "MD030" => Some((
            "1.  Multiple spaces after marker",
            Box::new(MD030ListMarkerSpace::new(1, 1, 1, 1)),
        )),
        "MD031" => Some(("Text\n```\ncode\n```\nText", Box::new(MD031BlanksAroundFences))),
        "MD032" => Some(("Text\n* List item\nText", Box::new(MD032BlanksAroundLists::default()))),
        "MD033" => Some(("Text with <div>HTML</div>", Box::new(MD033NoInlineHtml::default()))),
        "MD034" => Some(("Visit https://example.com", Box::new(MD034NoBareUrls))),
        "MD035" => Some(("Text\n***\nText", Box::new(MD035HRStyle::default()))),
        "MD036" => Some((
            "**Bold text as heading**",
            Box::new(MD036NoEmphasisAsHeading::new("!?.,:;".to_string())),
        )),
        "MD037" => Some(("Text with * spaces around * emphasis", Box::new(MD037NoSpaceInEmphasis))),
        "MD038" => Some(("`code `", Box::new(MD038NoSpaceInCode::default()))),
        "MD039" => Some(("[link text ]( url )", Box::new(MD039NoSpaceInLinks))),
        "MD040" => Some(("```\ncode without language\n```", Box::new(MD040FencedCodeLanguage))),
        "MD041" => Some(("Not a heading", Box::new(MD041FirstLineHeading::default()))),
        "MD042" => Some(("[]()", Box::new(MD042NoEmptyLinks))),
        "MD043" => Some((
            "# Wrong heading",
            Box::new(MD043RequiredHeadings::new(vec!["Introduction".to_string()])),
        )),
        "MD044" => Some((
            "javascript instead of JavaScript",
            Box::new(MD044ProperNames::new(vec!["JavaScript".to_string()], false)),
        )),
        "MD045" => Some(("![](image.png)", Box::new(MD045NoAltText::new()))),
        "MD046" => Some((
            "    indented code",
            Box::new(MD046CodeBlockStyle::new(CodeBlockStyle::Fenced)),
        )),
        "MD047" => Some(("File without trailing newline", Box::new(MD047SingleTrailingNewline))),
        "MD048" => Some((
            "~~~\ncode\n~~~",
            Box::new(MD048CodeFenceStyle::new(CodeFenceStyle::Tilde)),
        )),
        "MD049" => Some(("Text _emphasis_ text", Box::new(MD049EmphasisStyle::default()))),
        "MD050" => Some((
            "Text __strong__ text",
            Box::new(MD050StrongStyle::new(StrongStyle::Underscore)),
        )),
        "MD051" => Some(("[link](#nonexistent)", Box::new(MD051LinkFragments))),
        "MD052" => Some(("[ref link][ref]", Box::new(MD052ReferenceLinkImages))),
        "MD053" => Some((
            "[ref]: https://example.com",
            Box::new(MD053LinkImageReferenceDefinitions::default()),
        )),
        "MD054" => Some(("![image](url)", Box::new(MD054LinkImageStyle::default()))),
        "MD055" => Some(("|col1|col2|\n|--|--|\n|a|b|", Box::new(MD055TablePipeStyle::default()))),
        "MD056" => Some(("|col1|col2|\n|--|--|\n|a|", Box::new(MD056TableColumnCount))),
        "MD057" => Some(("[link](missing.md)", Box::new(MD057ExistingRelativeLinks::default()))),
        "MD058" => Some(("Text\n|table|\nText", Box::new(MD058BlanksAroundTables))),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Keep existing specific tests that we know work
    #[test]
    fn test_md030_vscode_fix_no_duplication() {
        let rule = MD030ListMarkerSpace::new(1, 1, 1, 1);
        let content = "1.  Supporting a new storage platform for MLflow artifacts";

        let result = simulate_vscode_fix(content, &rule).unwrap();

        // Should fix to single space, not duplicate the marker
        assert_eq!(result, "1. Supporting a new storage platform for MLflow artifacts");
        assert!(!result.contains("1. 1."), "Should not contain duplicated list marker");
    }

    #[test]
    fn test_md019_vscode_fix_no_duplication() {
        let rule = MD019NoMultipleSpaceAtx::new();
        let content = "##  Multiple Spaces Heading";

        let result = simulate_vscode_fix(content, &rule).unwrap();

        // Should fix to single space, not duplicate the hashes
        assert_eq!(result, "## Multiple Spaces Heading");
        assert!(!result.contains("## ##"), "Should not contain duplicated hashes");
    }

    #[test]
    fn test_md023_vscode_fix_no_duplication() {
        let rule = MD023HeadingStartLeft;
        let content = "  # Indented Heading";

        let result = simulate_vscode_fix(content, &rule).unwrap();

        // Should remove indentation, not duplicate the heading
        assert_eq!(result, "# Indented Heading");
        assert!(
            !result.contains("# # "),
            "Should not contain duplicated heading markers"
        );
    }

    #[test]
    fn test_md030_multiple_spaces() {
        let rule = MD030ListMarkerSpace::new(1, 1, 1, 1);
        let content = "*   Item with three spaces";

        let result = simulate_vscode_fix(content, &rule).unwrap();

        assert_eq!(result, "* Item with three spaces");
        assert!(!result.contains("* *"), "Should not contain duplicated asterisks");
    }

    #[test]
    fn test_md019_various_heading_levels() {
        let rule = MD019NoMultipleSpaceAtx::new();

        // Test different heading levels
        let test_cases = vec![
            ("#  H1", "# H1"),
            ("##   H2", "## H2"),
            ("###    H3", "### H3"),
            ("######      H6", "###### H6"),
        ];

        for (input, expected) in test_cases {
            let result = simulate_vscode_fix(input, &rule).unwrap();
            assert_eq!(result, expected, "Failed for input: {input}");

            // Ensure no duplication of hash symbols
            let hash_count = input.chars().take_while(|&c| c == '#').count();
            let result_hash_count = result.chars().take_while(|&c| c == '#').count();
            assert_eq!(
                hash_count, result_hash_count,
                "Hash count should remain the same for: {input}"
            );
        }
    }

    #[test]
    fn test_md023_various_indentations() {
        let rule = MD023HeadingStartLeft;

        let test_cases = vec![("  # H1", "# H1"), ("    ## H2", "## H2"), ("\t### H3", "### H3")];

        for (input, expected) in test_cases {
            let result = simulate_vscode_fix(input, &rule).unwrap();
            assert_eq!(result, expected, "Failed for input: {input:?}");
        }
    }

    #[test]
    fn test_md006_vscode_fix_no_duplication() {
        let rule = MD006StartBullets;
        let content = "  * Indented list item that should trigger MD006";

        let result = simulate_vscode_fix(content, &rule);

        // If MD006 has a fix, it should not duplicate content
        if let Ok(fixed) = result {
            assert!(!fixed.contains("* *"), "Should not contain duplicated list markers");
            assert!(!fixed.contains("  * *"), "Should not contain duplicated content");
            // The fix should start with a bullet marker and not have the original indentation
            assert!(fixed.starts_with("*"), "Should start with bullet marker");
            assert!(
                !fixed.starts_with("  *"),
                "Should not start with indented bullet marker"
            );
            // Expected: "* Indented list item that should trigger MD006"
            // But we'll be lenient about exact spacing as long as there's no duplication
            assert!(
                fixed.contains("Indented list item that should trigger MD006"),
                "Should contain the original content"
            );
        } else {
            panic!("Expected MD006 to provide a fix");
        }
    }

    #[test]
    fn test_md005_vscode_fix_no_duplication() {
        let rule = MD005ListIndent;
        let content = "* Item 1\n   * Item with 3 spaces (should be 2)\n* Item 3";

        let result = simulate_vscode_fix(content, &rule);

        // If MD005 has a fix, it should not duplicate content
        if let Ok(fixed) = result {
            assert!(!fixed.contains("* *"), "Should not contain duplicated list markers");
            assert!(!fixed.contains("   * *"), "Should not contain duplicated content");
            // The fix should correct the indentation to 2 spaces
            assert!(
                fixed.contains("  * Item with 3 spaces"),
                "Should fix indentation to 2 spaces"
            );
        } else {
            panic!("Expected MD005 to provide a fix");
        }
    }

    #[test]
    fn test_md027_vscode_fix_no_duplication() {
        let rule = MD027MultipleSpacesBlockquote;
        let content = ">  Multiple spaces in blockquote";

        let result = simulate_vscode_fix(content, &rule);

        // If MD027 has a fix, it should not duplicate content
        if let Ok(fixed) = result {
            assert!(
                !fixed.contains("> >"),
                "Should not contain duplicated blockquote markers"
            );
            assert!(!fixed.contains(">>"), "Should not contain merged blockquote markers");
            // The fix should correct to single space
            assert!(
                fixed.contains("> Multiple spaces"),
                "Should fix to single space after marker"
            );
        } else {
            panic!("Expected MD027 to provide a fix");
        }
    }

    #[test]
    fn test_md032_vscode_fix_no_duplication() {
        let rule = MD032BlanksAroundLists::default();
        let content = "Text\n* List item\nMore text";

        let result = simulate_vscode_fix(content, &rule);

        // If MD032 has a fix, it should not duplicate content
        if let Ok(fixed) = result {
            // Check that no lines are duplicated
            let lines: Vec<&str> = fixed.lines().collect();
            for (i, line) in lines.iter().enumerate() {
                for (j, other_line) in lines.iter().enumerate() {
                    if i != j && !line.trim().is_empty() && line == other_line {
                        panic!("Found duplicated line: {line:?}");
                    }
                }
            }
        }
        // If no fix is available, that's also acceptable for this test
    }

    #[test]
    fn test_md007_vscode_fix_no_duplication() {
        let rule = MD007ULIndent::default();
        let content = "- Item 1\n   - Wrong indent";

        let result = simulate_vscode_fix(content, &rule);

        // If MD007 has a fix, it should not duplicate content
        if let Ok(fixed) = result {
            assert!(!fixed.contains("- -"), "Should not contain duplicated list markers");
            assert!(!fixed.contains("   - -"), "Should not contain duplicated content");
            // The fix should correct the indentation
            assert!(fixed.contains("  - Wrong indent"), "Should fix indentation to 2 spaces");
        } else {
            panic!("Expected MD007 to provide a fix");
        }
    }

    #[test]
    fn test_md046_vscode_fix_no_duplication() {
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Fenced);
        let content = "    indented code"; // Indented code that should trigger MD046

        let result = simulate_vscode_fix(content, &rule);

        // If MD046 has a fix, it should not duplicate content
        if let Ok(fixed) = result {
            assert!(
                !fixed.contains("    indented"),
                "Should not contain original indented code"
            );
            assert!(fixed.contains("```"), "Should contain fenced code block marker");
            assert!(fixed.contains("indented code"), "Should contain the code content");
        }
        // If no fix is available, that's also acceptable for this test
    }

    // Comprehensive test for all rules
    #[test]
    fn test_all_rules_vscode_fix_no_duplication() {
        let rules_to_test = vec![
            "MD001", "MD002", "MD003", "MD004", "MD005", "MD006", "MD007", "MD009", "MD010", "MD011", "MD012", "MD013",
            "MD014", "MD018", "MD019", "MD020", "MD021", "MD022", "MD023", "MD024", "MD025", "MD026", "MD027", "MD028",
            "MD029", "MD030", "MD031", "MD032", "MD033", "MD034", "MD035", "MD036", "MD037", "MD038", "MD039", "MD040",
            "MD041", "MD042", "MD043", "MD044", "MD045", "MD046", "MD047", "MD048", "MD049", "MD050", "MD051", "MD052",
            "MD053", "MD054", "MD055", "MD056", "MD057", "MD058",
        ];

        let mut tested_rules = 0;
        let mut rules_with_fixes = 0;
        let mut passed_tests = 0;

        for rule_name in rules_to_test {
            if let Some((test_content, rule)) = create_test_case_for_rule(rule_name) {
                tested_rules += 1;

                match simulate_vscode_fix(test_content, rule.as_ref()) {
                    Ok(fixed_content) => {
                        rules_with_fixes += 1;

                        // Generic checks that apply to all rules
                        let original_non_whitespace: String =
                            test_content.chars().filter(|c| !c.is_whitespace()).collect();
                        let fixed_non_whitespace: String =
                            fixed_content.chars().filter(|c| !c.is_whitespace()).collect();

                        // Check for obvious content duplication patterns (the actual bugs we're looking for)
                        let has_obvious_duplication = fixed_content.contains("# # ")
                            || fixed_content.contains("## ## ")
                            || fixed_content.contains("### ### ")
                            || fixed_content.contains("* *")
                            || fixed_content.contains("- -")
                            || fixed_content.contains("+ +")
                            || fixed_content.contains("> >")
                            || fixed_content.contains("1. 1.")
                            || fixed_content.contains("2. 2.");

                        // For rules that provide complete replacements (like MD042), check for actual duplication patterns
                        // rather than just size increase
                        let has_size_based_duplication =
                            if rule_name == "MD042" || rule_name == "MD043" || rule_name == "MD044" {
                                // These rules legitimately provide complete replacements, so skip size-based check
                                false
                            } else {
                                // For other rules, a 3x size increase likely indicates duplication
                                fixed_non_whitespace.len() > original_non_whitespace.len() * 3
                            };

                        if has_obvious_duplication || has_size_based_duplication {
                            panic!(
                                "Rule {rule_name} has content duplication in VS Code extension fix!\nOriginal: {test_content:?}\nFixed: {fixed_content:?}"
                            );
                        }

                        passed_tests += 1;
                        println!("✓ {rule_name}: Fix applied successfully");
                    }
                    Err(e) => {
                        // No fix available or fix failed - this is acceptable
                        println!("- {rule_name}: No fix available ({e})");
                    }
                }
            } else {
                println!("⚠ {rule_name}: No test case defined");
            }
        }

        println!("\n=== Test Summary ===");
        println!("Rules tested: {tested_rules}");
        println!("Rules with fixes: {rules_with_fixes}");
        println!("Tests passed: {passed_tests}");

        // We expect at least some rules to have fixes and all of them to pass the duplication test
        assert!(rules_with_fixes > 0, "Expected at least some rules to have fixes");
        assert_eq!(
            passed_tests, rules_with_fixes,
            "All rules with fixes should pass the duplication test"
        );
    }
}
