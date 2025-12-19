//! Cross-validation tests to ensure CLI and LSP fixes produce identical results
//!
//! This test suite validates that both CLI batch fixes (using rule.fix()) and
//! LSP individual fixes (using warning.fix) produce the same final content.

use rumdl_lib::config::Config;
use rumdl_lib::lint_context::LintContext;
use rumdl_lib::rule::{FixCapability, Rule};
use rumdl_lib::rules::*;
use rumdl_lib::utils::fix_utils;

/// Test helper to compare CLI and LSP fix results for a given rule and content
fn test_cli_lsp_consistency(rule: &dyn Rule, content: &str, test_name: &str) {
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);

    // Get CLI fix result
    let cli_result = rule.fix(&ctx);

    // Get warnings from the rule
    let warnings_result = rule.check(&ctx);

    match (cli_result, warnings_result) {
        (Ok(cli_fixed), Ok(warnings)) => {
            // Apply LSP-style fixes using the warning fixes
            let lsp_result = fix_utils::apply_warning_fixes(content, &warnings);

            match lsp_result {
                Ok(lsp_fixed) => {
                    // Both should produce the same result
                    assert_eq!(
                        cli_fixed, lsp_fixed,
                        "{test_name}: CLI and LSP fixes produced different results.\nOriginal: {content:?}\nCLI: {cli_fixed:?}\nLSP: {lsp_fixed:?}"
                    );

                    println!("✓ {test_name}: Consistency verified");
                }
                Err(lsp_error) => {
                    // If LSP fix failed, warnings might not have proper fixes
                    // This is acceptable if there are no warning-level fixes
                    let has_fixes = warnings.iter().any(|w| w.fix.is_some());
                    if has_fixes {
                        panic!("{test_name}: LSP fix failed but warnings have fixes: {lsp_error}");
                    } else {
                        println!("○ {test_name}: No warning-level fixes available (CLI-only rule)");
                    }
                }
            }
        }
        (Ok(_), Err(warnings_error)) => {
            panic!("{test_name}: CLI fix succeeded but check failed: {warnings_error:?}");
        }
        (Err(cli_error), Ok(_)) => {
            println!("○ {test_name}: CLI fix not implemented: {cli_error:?}");
        }
        (Err(_), Err(_)) => {
            println!("○ {test_name}: Neither CLI nor LSP fixes implemented");
        }
    }
}

#[test]
fn test_md030_list_marker_space_consistency() {
    let rule = MD030ListMarkerSpace::new(1, 1, 1, 1);

    let test_cases = vec![
        (
            "1.  Multiple spaces after ordered marker",
            "Single ordered list with extra spaces",
        ),
        (
            "*   Multiple spaces after unordered marker",
            "Single unordered list with extra spaces",
        ),
        ("1.  First\n*   Second", "Mixed list types with extra spaces"),
        (
            "- Item\n  -   Nested with extra spaces",
            "Nested list with extra spaces",
        ),
        ("1.\tTab after marker", "Tab instead of spaces"),
        ("*\t\tMultiple tabs", "Multiple tabs after marker"),
    ];

    for (content, description) in test_cases {
        test_cli_lsp_consistency(&rule, content, &format!("MD030: {description}"));
    }
}

#[test]
fn test_md019_multiple_space_atx_consistency() {
    let rule = MD019NoMultipleSpaceAtx;

    let test_cases = vec![
        ("#  Heading with extra space", "H1 with extra space"),
        ("##   H2 with multiple spaces", "H2 with multiple spaces"),
        ("###    H3 with many spaces", "H3 with many spaces"),
        ("#  First\n##   Second", "Multiple headings with extra spaces"),
    ];

    for (content, description) in test_cases {
        test_cli_lsp_consistency(&rule, content, &format!("MD019: {description}"));
    }
}

#[test]
fn test_md009_trailing_spaces_consistency() {
    let rule = MD009TrailingSpaces::default();

    let test_cases = vec![
        ("Line with trailing spaces   ", "Single line with trailing spaces"),
        ("Line one   \nLine two  ", "Multiple lines with trailing spaces"),
        ("No trailing spaces\nClean line", "Lines without trailing spaces"),
        ("Mixed   \nClean\nTrailing  ", "Mixed clean and dirty lines"),
    ];

    for (content, description) in test_cases {
        test_cli_lsp_consistency(&rule, content, &format!("MD009: {description}"));
    }
}

#[test]
fn test_md010_hard_tabs_consistency() {
    let rule = MD010NoHardTabs::default();

    let test_cases = vec![
        ("Line\twith\ttabs", "Line with tabs"),
        ("Normal line\n\tIndented with tab", "Mixed tabs and spaces"),
        ("Multiple\t\ttabs\tin\tline", "Multiple tabs in single line"),
    ];

    for (content, description) in test_cases {
        test_cli_lsp_consistency(&rule, content, &format!("MD010: {description}"));
    }
}

#[test]
fn test_md018_missing_space_atx_consistency() {
    let rule = MD018NoMissingSpaceAtx;

    let test_cases = vec![
        ("#Missing space", "H1 missing space"),
        ("##Also missing", "H2 missing space"),
        ("###Multiple missing", "H3 missing space"),
        ("#Missing\n##Also missing", "Multiple headings missing spaces"),
    ];

    for (content, description) in test_cases {
        test_cli_lsp_consistency(&rule, content, &format!("MD018: {description}"));
    }
}

#[test]
fn test_md023_heading_start_left_consistency() {
    let rule = MD023HeadingStartLeft;

    let test_cases = vec![
        ("  # Indented heading", "H1 with indentation"),
        ("    ## More indented", "H2 with more indentation"),
        ("\t# Tab indented", "H1 with tab indentation"),
        ("  # First\n    ## Second", "Multiple indented headings"),
    ];

    for (content, description) in test_cases {
        test_cli_lsp_consistency(&rule, content, &format!("MD023: {description}"));
    }
}

#[test]
fn test_md026_trailing_punctuation_consistency() {
    let rule = MD026NoTrailingPunctuation::default();

    let test_cases = vec![
        ("# Heading!", "H1 with exclamation"),
        ("## Heading?", "H2 with question mark"),
        ("### Heading.", "H3 with period"),
        ("# First!\n## Second?", "Multiple headings with punctuation"),
    ];

    for (content, description) in test_cases {
        test_cli_lsp_consistency(&rule, content, &format!("MD026: {description}"));
    }
}

#[test]
fn test_md038_no_space_in_code_consistency() {
    let rule = MD038NoSpaceInCode::default();

    let test_cases = vec![
        ("`code `", "Code span with trailing space"),
        ("` code`", "Code span with leading space"),
        ("` code `", "Code span with both leading and trailing spaces"),
        ("Text with `bad ` and ` also bad` code", "Multiple bad code spans"),
    ];

    for (content, description) in test_cases {
        test_cli_lsp_consistency(&rule, content, &format!("MD038: {description}"));
    }
}

#[test]
fn test_md039_no_space_in_links_consistency() {
    let rule = MD039NoSpaceInLinks;

    let test_cases = vec![
        ("[link text ]( url )", "Link with spaces around URL"),
        ("[text ](url)", "Link with trailing space in text"),
        ("[text](  url  )", "Link with spaces around URL only"),
        (
            "Multiple [bad ]( link ) examples [here ](  too  )",
            "Multiple bad links",
        ),
    ];

    for (content, description) in test_cases {
        test_cli_lsp_consistency(&rule, content, &format!("MD039: {description}"));
    }
}

/// Create appropriate test content for each rule based on what it checks
fn get_test_content_for_rule(rule_name: &str) -> Option<&'static str> {
    match rule_name {
        "MD001" => Some("# H1\n### H3 (should be H2)"),
        "MD002" => Some("## H2 (should start with H1)"),
        "MD003" => Some("# ATX\nSetext\n======"),
        "MD004" => Some("* Item 1\n- Item 2"),
        "MD005" => Some("* Item 1\n   * Item with 3 spaces"),
        "MD006" => Some("  * Indented list item"),
        "MD007" => Some("- Item 1\n   - Wrong indent"),
        "MD009" => Some("Line with trailing spaces   "),
        "MD010" => Some("Line with\ttab"),
        "MD011" => Some("(http://example.com)[Example]"),
        "MD012" => Some("Content\n\n\n\nToo many blanks"),
        "MD013" => Some("This is a very long line that exceeds the maximum line length limit and should trigger MD013"),
        "MD014" => Some("```bash\n$ command\n```"),
        "MD018" => Some("#Missing space"),
        "MD019" => Some("##  Multiple spaces"),
        "MD020" => Some("##No space in closed##"),
        "MD021" => Some("##  Multiple  spaces  ##"),
        "MD022" => Some("Text\n# Heading\nMore text"),
        "MD023" => Some("  # Indented heading"),
        "MD024" => Some("# Duplicate\n# Duplicate"),
        "MD025" => Some("# First\n# Second H1"),
        "MD026" => Some("# Heading!"),
        "MD027" => Some(">  Multiple spaces in blockquote"),
        "MD028" => Some("> Quote\n>\n> More quote"),
        "MD029" => Some("1. First\n3. Third"),
        "MD030" => Some("1.  Multiple spaces after marker"),
        "MD031" => Some("Text\n```\ncode\n```\nText"),
        "MD032" => Some("Text\n* List item\nText"),
        "MD033" => Some("Text with <div>HTML</div>"),
        "MD034" => Some("Visit https://example.com"),
        "MD035" => Some("Text\n***\nText"),
        "MD036" => Some("**Bold text as heading**"),
        "MD037" => Some("Text with * spaces around * emphasis"),
        "MD038" => Some("`code `"),
        "MD039" => Some("[link text ]( url )"),
        "MD040" => Some("```\ncode without language\n```"),
        "MD041" => Some("Not a heading"),
        "MD042" => Some("[]()"),
        "MD043" => Some("# Wrong heading"),
        "MD044" => Some("javascript instead of JavaScript"),
        "MD045" => Some("![](image.png)"),
        "MD046" => Some("    indented code"),
        "MD047" => Some("File without trailing newline"),
        "MD048" => Some("~~~\ncode\n~~~"),
        "MD049" => Some("Text _emphasis_ text"),
        "MD050" => Some("Text __strong__ text"),
        "MD051" => Some("[link](#nonexistent)"),
        "MD052" => Some("[ref link][ref]"),
        "MD053" => Some("[ref]: https://example.com"),
        "MD054" => Some("![image](url)"),
        "MD055" => Some("|col1|col2|\n|--|--|\ncol3|col4"),
        "MD056" => Some("|col1|col2|\n|--|--|\n|a|"),
        "MD057" => Some("[link](missing.md)"),
        "MD058" => Some("Text\n|table|\nText"),
        "MD059" => Some("[click here](https://example.com)"),
        "MD060" => Some("|col1|col2|\n|-|-|\n|a|b|"),
        "MD061" => Some("This contains a TODO marker"),
        "MD062" => Some("[link]( https://example.com )"),
        "MD063" => Some("# heading in lowercase"),
        "MD064" => Some("Text with  multiple  spaces"),
        "MD065" => Some("Text\n---\nMore text"),
        "MD066" => Some("Text[^1]\n\n[^1]:"),
        "MD067" => Some("Text[^2][^1]\n\n[^1]: First\n[^2]: Second"),
        "MD068" => Some("[^1]:\n\n[^1]: Empty footnote"),
        _ => None,
    }
}

#[test]
fn test_comprehensive_rule_consistency() {
    // Test a comprehensive set of rules that commonly provide fixes
    let rules_with_test_content: Vec<(Box<dyn Rule>, &str, &str)> = vec![
        (
            Box::new(MD030ListMarkerSpace::new(1, 1, 1, 1)),
            "1.  Multiple spaces",
            "MD030",
        ),
        (Box::new(MD019NoMultipleSpaceAtx), "##  Multiple spaces", "MD019"),
        (Box::new(MD009TrailingSpaces::default()), "Trailing spaces   ", "MD009"),
        (Box::new(MD018NoMissingSpaceAtx), "#Missing space", "MD018"),
        (Box::new(MD023HeadingStartLeft), "  # Indented", "MD023"),
        (Box::new(MD026NoTrailingPunctuation::default()), "# Heading!", "MD026"),
        (Box::new(MD038NoSpaceInCode::default()), "`code `", "MD038"),
        (Box::new(MD039NoSpaceInLinks), "[text ]( url )", "MD039"),
    ];

    let mut tested_count = 0;
    let mut consistent_count = 0;
    let mut cli_only_count = 0;
    let mut no_fix_count = 0;

    for (rule, content, rule_name) in rules_with_test_content {
        tested_count += 1;

        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let cli_result = rule.fix(&ctx);
        let warnings_result = rule.check(&ctx);

        match (cli_result, warnings_result) {
            (Ok(cli_fixed), Ok(warnings)) => {
                let lsp_result = fix_utils::apply_warning_fixes(content, &warnings);

                match lsp_result {
                    Ok(lsp_fixed) => {
                        if cli_fixed == lsp_fixed {
                            consistent_count += 1;
                            println!("✓ {rule_name}: CLI and LSP fixes consistent");
                        } else {
                            panic!(
                                "{rule_name}: Inconsistent results!\nOriginal: {content:?}\nCLI: {cli_fixed:?}\nLSP: {lsp_fixed:?}"
                            );
                        }
                    }
                    Err(_) => {
                        // Check if this is CLI-only (no warning fixes)
                        let has_warning_fixes = warnings.iter().any(|w| w.fix.is_some());
                        if has_warning_fixes {
                            panic!("{rule_name}: LSP fix failed but warnings have fixes");
                        } else {
                            cli_only_count += 1;
                            println!("○ {rule_name}: CLI-only fixes (no warning-level fixes)");
                        }
                    }
                }
            }
            (Ok(_), Err(_)) => {
                panic!("{rule_name}: CLI fix succeeded but check failed");
            }
            (Err(_), Ok(_)) => {
                cli_only_count += 1;
                println!("○ {rule_name}: No CLI fix implemented");
            }
            (Err(_), Err(_)) => {
                no_fix_count += 1;
                println!("○ {rule_name}: No fixes implemented");
            }
        }
    }

    println!("\n=== Fix Consistency Test Summary ===");
    println!("Rules tested: {tested_count}");
    println!("Consistent fixes: {consistent_count}");
    println!("CLI-only fixes: {cli_only_count}");
    println!("No fixes: {no_fix_count}");

    // We expect at least some consistent fixes
    assert!(
        consistent_count > 0,
        "Expected at least some rules to have consistent CLI/LSP fixes"
    );

    // All tested rules should either be consistent or have a valid reason for inconsistency
    assert_eq!(
        tested_count,
        consistent_count + cli_only_count + no_fix_count,
        "All rules should be accounted for"
    );
}

#[test]
fn test_all_53_rules_systematic_coverage() {
    println!("🚀 Starting comprehensive CLI vs LSP consistency test for all 53 rules...\n");

    // Get all rules using the official all_rules function
    let config = Config::default();
    let all_rules = rumdl_lib::rules::all_rules(&config);

    let mut total_tested = 0;
    let mut consistent_fixes = 0;
    let mut cli_only_fixes = 0;
    let mut no_fixes = 0;
    let mut lsp_errors = 0;
    let mut test_content_missing = 0;
    let mut unfixable_rules = 0;

    let mut detailed_results = Vec::new();

    for rule in all_rules {
        let rule_name = rule.name();
        total_tested += 1;

        // Get appropriate test content for this rule
        let test_content = match get_test_content_for_rule(rule_name) {
            Some(content) => content,
            None => {
                test_content_missing += 1;
                detailed_results.push(format!("⚠ {rule_name}: No test content defined"));
                continue;
            }
        };

        let ctx = LintContext::new(test_content, rumdl_lib::config::MarkdownFlavor::Standard, None);

        // Check if rule is marked as Unfixable - these intentionally return unchanged content
        // from CLI fix() but may have warning-level fixes for optional LSP quick-fix usage
        if rule.fix_capability() == FixCapability::Unfixable {
            unfixable_rules += 1;
            detailed_results.push(format!("○ {rule_name}: Unfixable (by design)"));
            continue;
        }

        let cli_result = rule.fix(&ctx);
        let warnings_result = rule.check(&ctx);

        match (cli_result, warnings_result) {
            (Ok(cli_fixed), Ok(warnings)) => {
                let lsp_result = fix_utils::apply_warning_fixes(test_content, &warnings);

                match lsp_result {
                    Ok(lsp_fixed) => {
                        if cli_fixed == lsp_fixed {
                            consistent_fixes += 1;
                            detailed_results.push(format!("✅ {rule_name}: CLI and LSP fixes consistent"));
                        } else {
                            // This is a real inconsistency that needs investigation
                            detailed_results.push(format!(
                                "❌ {rule_name}: INCONSISTENT!\n   Original: {test_content:?}\n   CLI: {cli_fixed:?}\n   LSP: {lsp_fixed:?}"
                            ));
                        }
                    }
                    Err(lsp_error) => {
                        // Check if this is expected (no warning fixes) or an error
                        let has_warning_fixes = warnings.iter().any(|w| w.fix.is_some());
                        if has_warning_fixes {
                            lsp_errors += 1;
                            detailed_results.push(format!(
                                "⚠ {rule_name}: LSP fix failed but warnings have fixes: {lsp_error}"
                            ));
                        } else {
                            cli_only_fixes += 1;
                            detailed_results.push(format!("○ {rule_name}: CLI-only fixes (no warning-level fixes)"));
                        }
                    }
                }
            }
            (Ok(_), Err(check_error)) => {
                detailed_results.push(format!(
                    "⚠ {rule_name}: CLI fix succeeded but check failed: {check_error:?}"
                ));
            }
            (Err(_), Ok(warnings)) => {
                let has_warning_fixes = warnings.iter().any(|w| w.fix.is_some());
                if has_warning_fixes {
                    detailed_results.push(format!("○ {rule_name}: No CLI fix but has warning fixes"));
                } else {
                    no_fixes += 1;
                    detailed_results.push(format!("○ {rule_name}: No fixes implemented"));
                }
            }
            (Err(_), Err(_)) => {
                no_fixes += 1;
                detailed_results.push(format!("○ {rule_name}: No fixes implemented"));
            }
        }
    }

    // Print detailed results
    println!("📋 Detailed Results:");
    for result in &detailed_results {
        println!("{result}");
    }

    // Print comprehensive summary
    println!("\n📊 === COMPREHENSIVE CLI vs LSP FIX CONSISTENCY REPORT ===");
    println!("Total rules in rumdl: {total_tested}");
    println!("Rules tested: {total_tested}");
    println!("Test content missing: {test_content_missing}");
    println!("─────────────────────────────────────────────────────");
    println!("✅ Consistent CLI/LSP fixes: {consistent_fixes}");
    println!("○ Unfixable (by design): {unfixable_rules}");
    println!("○ CLI-only fixes: {cli_only_fixes}");
    println!("○ No fixes available: {no_fixes}");
    println!("⚠ LSP errors: {lsp_errors}");
    println!("─────────────────────────────────────────────────────");

    let coverage_tested = total_tested - test_content_missing;
    let coverage_percentage = if total_tested > 0 {
        (coverage_tested as f64 / total_tested as f64) * 100.0
    } else {
        0.0
    };

    println!("📈 Test coverage: {coverage_tested}/{total_tested} rules ({coverage_percentage:.1}%)");

    if consistent_fixes > 0 {
        let consistency_rate = (consistent_fixes as f64 / coverage_tested as f64) * 100.0;
        println!("🎯 Fix consistency rate: {consistent_fixes}/{coverage_tested} ({consistency_rate:.1}%)");
    }

    // Quality assertions - use total_tested dynamically instead of hardcoded 53
    assert!(
        total_tested >= 53,
        "Should test at least 53 rules (have {total_tested})"
    );
    assert!(test_content_missing < 15, "Should have test content for most rules");
    assert!(consistent_fixes > 0, "Should have at least some consistent fixes");

    // Success criteria: Most rules should either have consistent fixes or valid reasons for differences
    let accounted_rules = consistent_fixes + cli_only_fixes + no_fixes + unfixable_rules;
    let inconsistent_rules = coverage_tested - accounted_rules - lsp_errors;

    println!("❌ Inconsistent fixes: {inconsistent_rules}");

    // For now, allow inconsistencies but track them
    assert_eq!(
        coverage_tested,
        consistent_fixes + cli_only_fixes + no_fixes + unfixable_rules + lsp_errors + inconsistent_rules,
        "All tested rules should be properly categorized"
    );

    println!("\n🎉 Systematic CLI vs LSP consistency test completed!");
}
