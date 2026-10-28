//! Cross-validation tests to ensure CLI and LSP fixes produce identical results
//!
//! This test suite validates that both CLI batch fixes (using rule.fix()) and
//! LSP individual fixes (using warning.fix) produce the same final content.

use rumdl_lib::config::Config;
use rumdl_lib::lint_context::LintContext;
use rumdl_lib::rule::{FixCapability, Rule};
use rumdl_lib::rules::*;
use rumdl_lib::utils::fix_utils;

#[test]
fn overlapping_heading_spacing_diagnostics_insert_one_blank() {
    let rules: Vec<Box<dyn Rule>> = vec![
        Box::new(MD022BlanksAroundHeadings::default()),
        Box::new(MD031BlanksAroundFences::default()),
        Box::new(MD032BlanksAroundLists::default()),
    ];
    for block in ["- item", "```rust\ncode\n```"] {
        let content = format!("# Heading\n{block}\n");
        let ctx = LintContext::new(&content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let warnings: Vec<_> = rules.iter().flat_map(|rule| rule.check(&ctx).unwrap()).collect();
        assert!(warnings.len() >= 2, "the fixture must exercise overlapping findings");
        assert_eq!(
            fix_utils::apply_warning_fixes(&content, &warnings).unwrap(),
            format!("# Heading\n\n{block}\n")
        );
    }
}

#[test]
fn overlapping_heading_spacing_respects_counts_and_blockquotes() {
    for count in [1, 2, 3] {
        let rules: Vec<Box<dyn Rule>> = vec![
            Box::new(MD022BlanksAroundHeadings::with_values(1, count)),
            Box::new(MD031BlanksAroundFences::default()),
            Box::new(MD032BlanksAroundLists::default()),
        ];
        for prefix in ["", ">", "> >"] {
            for block in ["- item", "```rust\ncode\n```"] {
                let line_prefix = if prefix.is_empty() {
                    String::new()
                } else {
                    format!("{prefix} ")
                };
                let block = block
                    .lines()
                    .map(|line| format!("{line_prefix}{line}\n"))
                    .collect::<String>();
                let content = format!("{line_prefix}# Heading\n{block}");
                let ctx = LintContext::new(&content, rumdl_lib::config::MarkdownFlavor::Standard, None);
                let warnings: Vec<_> = rules.iter().flat_map(|rule| rule.check(&ctx).unwrap()).collect();
                assert!(warnings.len() >= 2, "the fixture must exercise overlapping findings");
                let expected = format!("{line_prefix}# Heading\n{}{block}", format!("{prefix}\n").repeat(count));
                let fixed = fix_utils::apply_warning_fixes(&content, &warnings).unwrap();
                assert_eq!(fixed, expected, "count={count}, prefix={prefix:?}");
                let ctx = LintContext::new(&fixed, rumdl_lib::config::MarkdownFlavor::Standard, None);
                assert!(rules.iter().all(|rule| rule.check(&ctx).unwrap().is_empty()));
            }
        }
    }
}

#[test]
fn overlapping_heading_spacing_preserves_line_endings_and_list_containers() {
    let rules: Vec<Box<dyn Rule>> = vec![
        Box::new(MD022BlanksAroundHeadings::with_values(1, 2)),
        Box::new(MD031BlanksAroundFences::default()),
        Box::new(MD032BlanksAroundLists::default()),
    ];
    for eol in ["\n", "\r\n"] {
        for (input, expected) in [
            ("# Heading\n- item\n", "# Heading\n\n\n- item\n"),
            ("- # Heading\n  - sub\n", "- # Heading\n\n\n  - sub\n"),
            (
                "- # Heading\n  ```\n  code\n  ```\n",
                "- # Heading\n\n\n  ```\n  code\n  ```\n",
            ),
            ("> # Heading\n> - item\n", "> # Heading\n>\n>\n> - item\n"),
        ] {
            let input = input.replace('\n', eol);
            let expected = expected.replace('\n', eol);
            let ctx = LintContext::new(&input, rumdl_lib::config::MarkdownFlavor::Standard, None);
            let warnings: Vec<_> = rules.iter().flat_map(|rule| rule.check(&ctx).unwrap()).collect();
            let fixed = fix_utils::apply_warning_fixes(&input, &warnings).unwrap();
            assert_eq!(fixed, expected);
            let ctx = LintContext::new(&fixed, rumdl_lib::config::MarkdownFlavor::Standard, None);
            assert!(rules.iter().all(|rule| rule.check(&ctx).unwrap().is_empty()));
        }
    }
}

#[test]
fn footnote_reordering_is_atomic_when_another_rule_edits_a_definition() {
    let rules: Vec<Box<dyn Rule>> = vec![
        Box::new(MD009TrailingSpaces::default()),
        Box::new(MD067FootnoteDefinitionOrder::new()),
    ];
    let content = "Refs[^a][^b].\n\n[^b]: B \n[^a]: A\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let warnings: Vec<_> = rules.iter().flat_map(|rule| rule.check(&ctx).unwrap()).collect();
    assert_eq!(warnings.len(), 2);
    let fixed = fix_utils::apply_warning_fixes(content, &warnings).unwrap();
    assert_eq!(fixed, "Refs[^a][^b].\n\n[^b]: B\n[^a]: A\n");
    let mut current = fixed;
    for _ in 0..2 {
        let ctx = LintContext::new(&current, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let warnings: Vec<_> = rules.iter().flat_map(|rule| rule.check(&ctx).unwrap()).collect();
        current = fix_utils::apply_warning_fixes(&current, &warnings).unwrap();
        assert_eq!(current, "Refs[^a][^b].\n\n[^a]: A\n[^b]: B\n");
    }
}

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
    let rule = MD018NoMissingSpaceAtx::new();

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

#[test]
fn test_md075_orphaned_table_rows_issue_420_consistency() {
    let rule = MD075OrphanedTableRows::default();
    let content = "\
| Value        | Description                                       |
| ------------ | ------------------------------------------------- |
| `consistent` | All code blocks must use the same style (default) |

| `fenced` | All code blocks must use fenced style (``` or ~~~) |
| `indented` | All code blocks must use indented style (4 spaces) |";

    test_cli_lsp_consistency(&rule, content, "MD075: issue #420 inline-fence orphan rows");
}

#[test]
fn test_md058_blanks_around_tables_consistency() {
    let rule = MD058BlanksAroundTables::default();

    let test_cases = vec![
        (
            "Intro\n| a | b |\n| --- | --- |\n| 1 | 2 |\nAfter\n",
            "Table needing blanks, content ends with a newline",
        ),
        (
            "Intro\n| a | b |\n| --- | --- |\n| 1 | 2 |\nAfter",
            "Table needing blanks, content ends without a newline",
        ),
    ];

    for (content, description) in test_cases {
        test_cli_lsp_consistency(&rule, content, &format!("MD058: {description}"));
    }
}

#[test]
fn test_md065_blanks_around_horizontal_rules_consistency() {
    let rule = MD065BlanksAroundHorizontalRules;

    let test_cases = vec![
        ("Text\n***\nMore text\n", "Thematic break, content ends with a newline"),
        ("Text\n***\nMore text", "Thematic break, content ends without a newline"),
    ];

    for (content, description) in test_cases {
        test_cli_lsp_consistency(&rule, content, &format!("MD065: {description}"));
    }
}

#[test]
fn test_md022_blanks_around_headings_consistency() {
    let rule = MD022BlanksAroundHeadings::default();

    let test_cases = vec![
        ("Text\n# Heading\nMore text", "Heading surrounded by paragraphs"),
        (
            "text\n# Heading\n    - deeper\n",
            "Heading followed by an indented list marker",
        ),
        ("text\n# Heading\n- item\n", "Heading followed by a list item"),
        ("text\n# Heading\n```\ncode\n```\n", "Heading followed by a code fence"),
    ];

    for (content, description) in test_cases {
        test_cli_lsp_consistency(&rule, content, &format!("MD022: {description}"));
    }
}

/// Create appropriate test content for each rule based on what it checks
fn get_test_content_for_rule(rule_name: &str) -> Option<&'static str> {
    match rule_name {
        "MD001" => Some("# H1\n### H3 (should be H2)"),
        "MD002" => Some("## H2 (should start with H1)"),
        "MD003" => Some("# ATX\nSetext\n======"),
        "MD004" => Some("* Item 1\n- Item 2"),
        "MD005" => Some("- A\n  - B\n   - C\n"),
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
        "MD022" => Some("text\n# Heading\n    - deeper\n"),
        "MD023" => Some("  # Indented heading"),
        "MD024" => Some("# Duplicate\n# Duplicate"),
        "MD025" => Some("# First\n# Second H1"),
        "MD026" => Some("# Heading!"),
        "MD027" => Some(">  Multiple spaces in blockquote"),
        "MD028" => Some("> First\n\n> Second\n"),
        "MD029" => Some("1. First\n3. Third"),
        "MD030" => Some("1.  Multiple spaces after marker"),
        "MD031" => Some("Text\n```\ncode\n```\nText"),
        "MD032" => Some("Text\n* List item\nText"),
        "MD033" => Some("Text with <div>HTML</div>"),
        "MD034" => Some("Visit https://example.com"),
        "MD035" => Some("***\n\n---\n"),
        "MD036" => Some("**Bold text as heading**"),
        "MD037" => Some("Text with * spaces around * emphasis"),
        "MD038" => Some("`code `"),
        "MD039" => Some("[link text ]( url )"),
        "MD040" => Some("```\ncode without language\n```"),
        "MD041" => Some("Not a heading"),
        "MD042" => Some("[https://example.com]()"),
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
        "MD054" => Some("[label](https://example.com)\n"),
        "MD055" => Some("|col1|col2|\n|--|--|\ncol3|col4"),
        "MD056" => Some("|col1|col2|\n|--|--|\n|a|"),
        "MD057" => Some("[link](missing.md)"),
        "MD058" => Some("Text\n| a | b |\n| - | - |\n| 1 | 2 |\nText\n"),
        "MD059" => Some("[click here](https://example.com)"),
        "MD060" => Some("| Name | Value |\n| --- | --- |\n| A | longer value |\n"),
        "MD061" => Some("This contains a TODO marker"),
        "MD062" => Some("[link]( https://example.com )"),
        "MD063" => Some("# heading in lowercase"),
        "MD064" => Some("Text with  multiple  spaces"),
        "MD065" => Some("Text\n***\nMore text\n"),
        "MD066" => Some("Text[^missing]\n"),
        "MD067" => Some("Text[^2][^1]\n\n[^1]: First\n[^2]: Second"),
        "MD068" => Some("[^1]:\n\n[^1]: Empty footnote"),
        "MD069" => Some("- - Item\n"),
        "MD070" => Some("```markdown\n```python\ncode\n```\n```\n"),
        "MD071" => Some("---\ntitle: Example\n---\n# Heading\n"),
        "MD072" => Some("---\nz: 1\na: 2\n---\n"),
        "MD073" => Some("<!-- toc -->\n- [Old](#old)\n<!-- tocstop -->\n\n## New\n"),
        "MD074" => Some("# Navigation\n"),
        "MD075" => Some("| A | B |\n| --- | --- |\n| a | b |\n\n| c | d |\n"),
        "MD076" => Some("- One\n\n- Two\n- Three\n"),
        "MD077" => Some("- Item\n\n continuation\n"),
        "MD078" => Some("```{r}\nplot(1:10)\n```\n"),
        "MD079" => Some("```{r several words}\nplot(1:10)\n```\n"),
        "MD080" => Some("# C++\n\n# C\n"),
        "MD081" => Some("**bold** and **more**\n"),
        "MD082" => Some("# Parent\n\n## Child\n"),
        "MD083" => Some("Broken dash â€“ here"),
        "MD084" => Some("left \u{2060} right"),
        "MD085" => Some("A paragraph\n  with an indented continuation"),
        "MD086" => Some("<!-- a comment that is never closed"),
        "MD087" => Some("Short line <!-- rumdl-disable-line MD013 -->"),
        "MD088" => Some("Text with \u{201C}quotes\u{201D} and dashes\u{2010}"),
        "MD089" => Some("日本語english"),
        "MD090" => Some("Prose\n\n---\n\n## Next"),
        "MD091" => Some("<div align=\"center\">\n[Docs](/docs)\n</div>"),
        "MD092" => Some("<<<<<<< HEAD\nours\n=======\ntheirs\n>>>>>>> branch\n"),
        "MD093" => Some("## Method `map()`"),
        // Invalid UTF-8 cannot be expressed in a &str; the rule is Unfixable either way.
        "MD094" => Some("Caf\u{FFFD}"),
        "MD095" => Some("[Trust me bro!](https://\u{0435}xample.com)"),
        "MD096" => Some("| Name |\n| --- |\n| Zulu |\n| Alpha |\n"),
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
        (Box::new(MD018NoMissingSpaceAtx::new()), "#Missing space", "MD018"),
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

/// Every registered rule must have a fixture that actually diagnoses something.
/// Configuration and context belong to the fixture, not to permissive skips.
#[test]
fn test_all_rules_systematic_coverage() {
    use rumdl_lib::config::{MarkdownFlavor, RuleConfig};
    let mut config = Config::default();
    for (name, options) in [
        ("MD013", "reflow = true"),
        ("MD028", "fix = true"),
        ("MD036", "fix = true"),
        ("MD043", "headings = ['# Expected']"),
        ("MD044", "names = ['JavaScript']"),
        ("MD046", "style = 'fenced'"),
        ("MD048", "style = 'backtick'"),
        ("MD049", "style = 'asterisk'"),
        ("MD050", "style = 'asterisk'"),
        ("MD054", "inline = false"),
        ("MD060", "enabled = true\nstyle = 'aligned'"),
        ("MD061", "terms = ['TODO']"),
        ("MD063", "enabled = true"),
        ("MD072", "enabled = true"),
        ("MD073", "enabled = true"),
        ("MD081", "max-per-paragraph = 1"),
        ("MD096", "column = 'Name'"),
    ] {
        config.rules.insert(
            name.into(),
            RuleConfig {
                severity: None,
                values: toml::from_str(options).unwrap(),
            },
        );
    }
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join("docs")).unwrap();
    std::fs::write(
        dir.path().join("mkdocs.yml"),
        "site_name: Fixture\nnav:\n  - Missing: missing.md\n",
    )
    .unwrap();
    let path = dir.path().join("docs/probe.md");
    let rules = all_rules(&config);
    let mut failures = Vec::new();
    for rule in &rules {
        let name = rule.name();
        let Some(content) = get_test_content_for_rule(name) else {
            failures.push(format!("{name}: missing fixture"));
            continue;
        };
        std::fs::write(&path, content).unwrap();
        let flavor = match name {
            "MD074" => MarkdownFlavor::MkDocs,
            "MD078" | "MD079" => MarkdownFlavor::Quarto,
            _ => MarkdownFlavor::Standard,
        };
        let mut ctx = LintContext::new(content, flavor, Some(path.clone()));
        // A literal replacement character is valid UTF-8, so MD094 needs real
        // decoder evidence, not a string that merely looks like a decode error.
        let invalid = match rumdl_lib::encoding::decode(b"Caf\xe9") {
            rumdl_lib::encoding::Decoded::Lossy { text, invalid } => {
                if name == "MD094" {
                    assert_eq!(text, content);
                }
                invalid
            }
            _ => unreachable!(),
        };
        if name == "MD094" {
            ctx = ctx.with_invalid_utf8(&invalid);
        }
        let warnings = if name == "MD087" {
            let participating: Vec<Box<dyn Rule>> =
                vec![Box::new(MD013LineLength::default()), dyn_clone::clone_box(&**rule)];
            rumdl_lib::lint(content, &participating, false, flavor, None, Some(&config)).unwrap()
        } else {
            rule.check(&ctx).unwrap_or_else(|error| panic!("{name}: {error}"))
        };
        if !warnings.iter().any(|warning| {
            warning
                .rule_name
                .as_deref()
                .is_some_and(|reported| reported.split('-').next() == Some(name))
        }) {
            failures.push(format!("{name}: fixture produced no diagnostic for its rule"));
            continue;
        }
        if rule.fix_capability() == FixCapability::Unfixable {
            match rule.fix(&ctx) {
                Ok(unchanged) => assert_eq!(unchanged, content, "{name}: unfixable rule rewrote content"),
                Err(rumdl_lib::rule::LintError::FixFailed(_)) => {}
                Err(error) => panic!("{name}: unexpected fix error: {error}"),
            }
            continue;
        }
        let cli = rule.fix(&ctx).unwrap_or_else(|error| panic!("{name}: {error}"));
        if !warnings.iter().any(|warning| warning.fix.is_some()) || cli == content {
            failures.push(format!("{name}: fixable fixture did not exercise a fix"));
            continue;
        }
        let lsp = fix_utils::apply_warning_fixes(content, &warnings)
            .unwrap_or_else(|error| panic!("{name}: invalid warning fix: {error}"));
        if cli != lsp {
            failures.push(format!("{name}: CLI/LSP mismatch: {cli:?} vs {lsp:?}"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
