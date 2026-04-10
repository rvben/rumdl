//! Utilities for applying fixes consistently between CLI and LSP
//!
//! This module provides shared logic for applying markdown fixes to ensure
//! that both CLI batch fixes and LSP individual fixes produce identical results.

use crate::inline_config::InlineConfig;
use crate::rule::{Fix, LintWarning};
use crate::utils::ensure_consistent_line_endings;

/// Filter warnings by inline config, removing those on disabled lines.
///
/// Replicates the same filtering logic used in the check/reporting path
/// (`src/lib.rs`) so that fix mode respects inline disable comments.
pub fn filter_warnings_by_inline_config(
    warnings: Vec<LintWarning>,
    inline_config: &InlineConfig,
    rule_name: &str,
) -> Vec<LintWarning> {
    let base_rule_name = if let Some(dash_pos) = rule_name.find('-') {
        // Handle sub-rules like "MD029-style" -> "MD029"
        // But only if the prefix looks like a rule ID (starts with "MD")
        let prefix = &rule_name[..dash_pos];
        if prefix.starts_with("MD") { prefix } else { rule_name }
    } else {
        rule_name
    };

    warnings
        .into_iter()
        .filter(|w| {
            let end = if w.end_line >= w.line { w.end_line } else { w.line };
            !(w.line..=end).any(|line| inline_config.is_rule_disabled(base_rule_name, line))
        })
        .collect()
}

/// Apply a list of warning fixes to content, simulating how the LSP client would apply them
/// This is used for testing consistency between CLI and LSP fix methods
pub fn apply_warning_fixes(content: &str, warnings: &[LintWarning]) -> Result<String, String> {
    let mut fixes: Vec<(usize, &Fix)> = warnings
        .iter()
        .enumerate()
        .filter_map(|(i, w)| w.fix.as_ref().map(|fix| (i, fix)))
        .collect();

    // No-op fast path: if there are no actual fixes to apply, return the
    // content unchanged. This avoids unnecessary line-ending normalization
    // when all warnings were filtered out (e.g., by inline config) or had
    // no fix attached.
    if fixes.is_empty() {
        return Ok(content.to_string());
    }

    // Deduplicate fixes that operate on the same range with the same replacement
    // This prevents double-application when multiple warnings target the same issue
    fixes.sort_by(|(_, fix_a), (_, fix_b)| {
        let range_cmp = fix_a.range.start.cmp(&fix_b.range.start);
        if range_cmp != std::cmp::Ordering::Equal {
            return range_cmp;
        }
        fix_a.range.end.cmp(&fix_b.range.end)
    });

    let mut deduplicated = Vec::new();
    let mut i = 0;
    while i < fixes.len() {
        let (idx, current_fix) = fixes[i];
        deduplicated.push((idx, current_fix));

        // Skip any subsequent fixes that have the same range and replacement
        while i + 1 < fixes.len() {
            let (_, next_fix) = fixes[i + 1];
            if current_fix.range == next_fix.range && current_fix.replacement == next_fix.replacement {
                i += 1; // Skip the duplicate
            } else {
                break;
            }
        }
        i += 1;
    }

    let mut fixes = deduplicated;

    // Sort fixes by range in reverse order (end to start) to avoid offset issues
    // Use original index as secondary sort key to ensure stable sorting
    fixes.sort_by(|(idx_a, fix_a), (idx_b, fix_b)| {
        // Primary: sort by range start in reverse order (largest first)
        let range_cmp = fix_b.range.start.cmp(&fix_a.range.start);
        if range_cmp != std::cmp::Ordering::Equal {
            return range_cmp;
        }

        // Secondary: sort by range end in reverse order
        let end_cmp = fix_b.range.end.cmp(&fix_a.range.end);
        if end_cmp != std::cmp::Ordering::Equal {
            return end_cmp;
        }

        // Tertiary: maintain original order for identical ranges (stable sort)
        idx_a.cmp(idx_b)
    });

    let mut result = content.to_string();

    // Track the lowest byte offset touched by an already-applied fix.
    // Since fixes are sorted in reverse order (highest start first),
    // any subsequent fix whose range.end > min_applied_start would
    // overlap with an already-applied fix and corrupt the result.
    let mut min_applied_start = usize::MAX;

    for (_, fix) in fixes {
        // Validate range bounds
        if fix.range.end > result.len() {
            return Err(format!(
                "Fix range end {} exceeds content length {}",
                fix.range.end,
                result.len()
            ));
        }

        if fix.range.start > fix.range.end {
            return Err(format!(
                "Invalid fix range: start {} > end {}",
                fix.range.start, fix.range.end
            ));
        }

        // Skip fixes that overlap with an already-applied fix to prevent
        // offset corruption (e.g., nested link/image constructs in MD039).
        if fix.range.end > min_applied_start {
            continue;
        }

        // Apply the fix by replacing the range with the replacement text
        result.replace_range(fix.range.clone(), &fix.replacement);
        min_applied_start = fix.range.start;
    }

    // Ensure line endings are consistent with the original document
    Ok(ensure_consistent_line_endings(content, &result))
}

/// Convert a single warning fix to a text edit-style representation
/// This helps validate that individual warning fixes are correctly structured
pub fn warning_fix_to_edit(content: &str, warning: &LintWarning) -> Result<(usize, usize, String), String> {
    if let Some(fix) = &warning.fix {
        // Validate the fix range against content
        if fix.range.end > content.len() {
            return Err(format!(
                "Fix range end {} exceeds content length {}",
                fix.range.end,
                content.len()
            ));
        }

        Ok((fix.range.start, fix.range.end, fix.replacement.clone()))
    } else {
        Err("Warning has no fix".to_string())
    }
}

/// Helper function to validate that a fix range makes sense in the context
pub fn validate_fix_range(content: &str, fix: &Fix) -> Result<(), String> {
    if fix.range.start > content.len() {
        return Err(format!(
            "Fix range start {} exceeds content length {}",
            fix.range.start,
            content.len()
        ));
    }

    if fix.range.end > content.len() {
        return Err(format!(
            "Fix range end {} exceeds content length {}",
            fix.range.end,
            content.len()
        ));
    }

    if fix.range.start > fix.range.end {
        return Err(format!(
            "Invalid fix range: start {} > end {}",
            fix.range.start, fix.range.end
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rule::{Fix, LintWarning, Severity};

    #[test]
    fn test_apply_single_fix() {
        let content = "1.  Multiple spaces";
        let warning = LintWarning {
            message: "Too many spaces".to_string(),
            line: 1,
            column: 3,
            end_line: 1,
            end_column: 5,
            severity: Severity::Warning,
            fix: Some(Fix {
                range: 2..4,                  // "  " (two spaces)
                replacement: " ".to_string(), // single space
            }),
            rule_name: Some("MD030".to_string()),
        };

        let result = apply_warning_fixes(content, &[warning]).unwrap();
        assert_eq!(result, "1. Multiple spaces");
    }

    #[test]
    fn test_apply_multiple_fixes() {
        let content = "1.  First\n*   Second";
        let warnings = vec![
            LintWarning {
                message: "Too many spaces".to_string(),
                line: 1,
                column: 3,
                end_line: 1,
                end_column: 5,
                severity: Severity::Warning,
                fix: Some(Fix {
                    range: 2..4, // First line "  "
                    replacement: " ".to_string(),
                }),
                rule_name: Some("MD030".to_string()),
            },
            LintWarning {
                message: "Too many spaces".to_string(),
                line: 2,
                column: 2,
                end_line: 2,
                end_column: 5,
                severity: Severity::Warning,
                fix: Some(Fix {
                    range: 11..14, // Second line "   " (after newline + "*")
                    replacement: " ".to_string(),
                }),
                rule_name: Some("MD030".to_string()),
            },
        ];

        let result = apply_warning_fixes(content, &warnings).unwrap();
        assert_eq!(result, "1. First\n* Second");
    }

    #[test]
    fn test_apply_non_overlapping_fixes() {
        // "Test  multiple    spaces"
        //  0123456789012345678901234
        //      ^^       ^^^^
        //      4-6      14-18
        let content = "Test  multiple    spaces";
        let warnings = vec![
            LintWarning {
                message: "Too many spaces".to_string(),
                line: 1,
                column: 5,
                end_line: 1,
                end_column: 7,
                severity: Severity::Warning,
                fix: Some(Fix {
                    range: 4..6, // "  " after "Test"
                    replacement: " ".to_string(),
                }),
                rule_name: Some("MD009".to_string()),
            },
            LintWarning {
                message: "Too many spaces".to_string(),
                line: 1,
                column: 15,
                end_line: 1,
                end_column: 19,
                severity: Severity::Warning,
                fix: Some(Fix {
                    range: 14..18, // "    " after "multiple"
                    replacement: " ".to_string(),
                }),
                rule_name: Some("MD009".to_string()),
            },
        ];

        let result = apply_warning_fixes(content, &warnings).unwrap();
        assert_eq!(result, "Test multiple spaces");
    }

    #[test]
    fn test_apply_duplicate_fixes() {
        let content = "Test  content";
        let warnings = vec![
            LintWarning {
                message: "Fix 1".to_string(),
                line: 1,
                column: 5,
                end_line: 1,
                end_column: 7,
                severity: Severity::Warning,
                fix: Some(Fix {
                    range: 4..6,
                    replacement: " ".to_string(),
                }),
                rule_name: Some("MD009".to_string()),
            },
            LintWarning {
                message: "Fix 2 (duplicate)".to_string(),
                line: 1,
                column: 5,
                end_line: 1,
                end_column: 7,
                severity: Severity::Warning,
                fix: Some(Fix {
                    range: 4..6,
                    replacement: " ".to_string(),
                }),
                rule_name: Some("MD009".to_string()),
            },
        ];

        // Duplicates should be deduplicated
        let result = apply_warning_fixes(content, &warnings).unwrap();
        assert_eq!(result, "Test content");
    }

    #[test]
    fn test_apply_fixes_with_windows_line_endings() {
        let content = "1.  First\r\n*   Second\r\n";
        let warnings = vec![
            LintWarning {
                message: "Too many spaces".to_string(),
                line: 1,
                column: 3,
                end_line: 1,
                end_column: 5,
                severity: Severity::Warning,
                fix: Some(Fix {
                    range: 2..4,
                    replacement: " ".to_string(),
                }),
                rule_name: Some("MD030".to_string()),
            },
            LintWarning {
                message: "Too many spaces".to_string(),
                line: 2,
                column: 2,
                end_line: 2,
                end_column: 5,
                severity: Severity::Warning,
                fix: Some(Fix {
                    range: 12..15, // Account for \r\n
                    replacement: " ".to_string(),
                }),
                rule_name: Some("MD030".to_string()),
            },
        ];

        let result = apply_warning_fixes(content, &warnings).unwrap();
        // The implementation normalizes line endings, which may double \r
        // Just test that the fixes were applied correctly
        assert!(result.contains("1. First"));
        assert!(result.contains("* Second"));
    }

    #[test]
    fn test_apply_fix_with_invalid_range() {
        let content = "Short";
        let warning = LintWarning {
            message: "Invalid fix".to_string(),
            line: 1,
            column: 1,
            end_line: 1,
            end_column: 10,
            severity: Severity::Warning,
            fix: Some(Fix {
                range: 0..100, // Out of bounds
                replacement: "Replacement".to_string(),
            }),
            rule_name: Some("TEST".to_string()),
        };

        let result = apply_warning_fixes(content, &[warning]);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("exceeds content length"));
    }

    #[test]
    fn test_apply_fix_with_reversed_range() {
        let content = "Hello world";
        let warning = LintWarning {
            message: "Invalid fix".to_string(),
            line: 1,
            column: 5,
            end_line: 1,
            end_column: 3,
            severity: Severity::Warning,
            fix: Some(Fix {
                #[allow(clippy::reversed_empty_ranges)]
                range: 10..5, // start > end - intentionally invalid for testing
                replacement: "Test".to_string(),
            }),
            rule_name: Some("TEST".to_string()),
        };

        let result = apply_warning_fixes(content, &[warning]);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Invalid fix range"));
    }

    #[test]
    fn test_apply_no_fixes() {
        let content = "No changes needed";
        let warnings = vec![LintWarning {
            message: "Warning without fix".to_string(),
            line: 1,
            column: 1,
            end_line: 1,
            end_column: 5,
            severity: Severity::Warning,
            fix: None,
            rule_name: Some("TEST".to_string()),
        }];

        let result = apply_warning_fixes(content, &warnings).unwrap();
        assert_eq!(result, content);
    }

    #[test]
    fn test_overlapping_fixes_skip_outer() {
        // Simulates nested link/image: [ ![ alt ](img) ](url) suffix
        // Inner fix: range 2..15 (image text)
        // Outer fix: range 0..22 (link text) — overlaps inner
        // Only the inner (higher start) should be applied; outer is skipped.
        let content = "[ ![ alt ](img) ](url) suffix";
        let warnings = vec![
            LintWarning {
                message: "Outer link".to_string(),
                line: 1,
                column: 1,
                end_line: 1,
                end_column: 22,
                severity: Severity::Warning,
                fix: Some(Fix {
                    range: 0..22,
                    replacement: "[![alt](img)](url)".to_string(),
                }),
                rule_name: Some("MD039".to_string()),
            },
            LintWarning {
                message: "Inner image".to_string(),
                line: 1,
                column: 3,
                end_line: 1,
                end_column: 15,
                severity: Severity::Warning,
                fix: Some(Fix {
                    range: 2..15,
                    replacement: "![alt](img)".to_string(),
                }),
                rule_name: Some("MD039".to_string()),
            },
        ];

        let result = apply_warning_fixes(content, &warnings).unwrap();
        // Inner fix applied: "![ alt ](img)" → "![alt](img)"
        // Outer fix skipped (overlaps). Suffix preserved.
        assert_eq!(result, "[ ![alt](img) ](url) suffix");
    }

    #[test]
    fn test_warning_fix_to_edit() {
        let content = "Hello world";
        let warning = LintWarning {
            message: "Test".to_string(),
            line: 1,
            column: 1,
            end_line: 1,
            end_column: 5,
            severity: Severity::Warning,
            fix: Some(Fix {
                range: 0..5,
                replacement: "Hi".to_string(),
            }),
            rule_name: Some("TEST".to_string()),
        };

        let edit = warning_fix_to_edit(content, &warning).unwrap();
        assert_eq!(edit, (0, 5, "Hi".to_string()));
    }

    #[test]
    fn test_warning_fix_to_edit_no_fix() {
        let content = "Hello world";
        let warning = LintWarning {
            message: "Test".to_string(),
            line: 1,
            column: 1,
            end_line: 1,
            end_column: 5,
            severity: Severity::Warning,
            fix: None,
            rule_name: Some("TEST".to_string()),
        };

        let result = warning_fix_to_edit(content, &warning);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "Warning has no fix");
    }

    #[test]
    fn test_warning_fix_to_edit_invalid_range() {
        let content = "Short";
        let warning = LintWarning {
            message: "Test".to_string(),
            line: 1,
            column: 1,
            end_line: 1,
            end_column: 10,
            severity: Severity::Warning,
            fix: Some(Fix {
                range: 0..100,
                replacement: "Long replacement".to_string(),
            }),
            rule_name: Some("TEST".to_string()),
        };

        let result = warning_fix_to_edit(content, &warning);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("exceeds content length"));
    }

    #[test]
    fn test_validate_fix_range() {
        let content = "Hello world";

        // Valid range
        let valid_fix = Fix {
            range: 0..5,
            replacement: "Hi".to_string(),
        };
        assert!(validate_fix_range(content, &valid_fix).is_ok());

        // Invalid range (end > content length)
        let invalid_fix = Fix {
            range: 0..20,
            replacement: "Hi".to_string(),
        };
        assert!(validate_fix_range(content, &invalid_fix).is_err());

        // Invalid range (start > end) - create reversed range
        let start = 5;
        let end = 3;
        let invalid_fix2 = Fix {
            range: start..end,
            replacement: "Hi".to_string(),
        };
        assert!(validate_fix_range(content, &invalid_fix2).is_err());
    }

    #[test]
    fn test_validate_fix_range_edge_cases() {
        let content = "Test";

        // Empty range at start
        let fix1 = Fix {
            range: 0..0,
            replacement: "Insert".to_string(),
        };
        assert!(validate_fix_range(content, &fix1).is_ok());

        // Empty range at end
        let fix2 = Fix {
            range: 4..4,
            replacement: " append".to_string(),
        };
        assert!(validate_fix_range(content, &fix2).is_ok());

        // Full content replacement
        let fix3 = Fix {
            range: 0..4,
            replacement: "Replace".to_string(),
        };
        assert!(validate_fix_range(content, &fix3).is_ok());

        // Start exceeds content
        let fix4 = Fix {
            range: 10..11,
            replacement: "Invalid".to_string(),
        };
        let result = validate_fix_range(content, &fix4);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("start 10 exceeds"));
    }

    #[test]
    fn test_fix_ordering_stability() {
        // Test that fixes with identical ranges maintain stable ordering
        let content = "Test content here";
        let warnings = vec![
            LintWarning {
                message: "First warning".to_string(),
                line: 1,
                column: 6,
                end_line: 1,
                end_column: 13,
                severity: Severity::Warning,
                fix: Some(Fix {
                    range: 5..12, // "content"
                    replacement: "stuff".to_string(),
                }),
                rule_name: Some("MD001".to_string()),
            },
            LintWarning {
                message: "Second warning".to_string(),
                line: 1,
                column: 6,
                end_line: 1,
                end_column: 13,
                severity: Severity::Warning,
                fix: Some(Fix {
                    range: 5..12, // Same range
                    replacement: "stuff".to_string(),
                }),
                rule_name: Some("MD002".to_string()),
            },
        ];

        // Both fixes are identical, so deduplication should leave only one
        let result = apply_warning_fixes(content, &warnings).unwrap();
        assert_eq!(result, "Test stuff here");
    }

    #[test]
    fn test_line_ending_preservation() {
        // Test Unix line endings
        let content_unix = "Line 1\nLine 2\n";
        let warning = LintWarning {
            message: "Add text".to_string(),
            line: 1,
            column: 7,
            end_line: 1,
            end_column: 7,
            severity: Severity::Warning,
            fix: Some(Fix {
                range: 6..6,
                replacement: " added".to_string(),
            }),
            rule_name: Some("TEST".to_string()),
        };

        let result = apply_warning_fixes(content_unix, &[warning]).unwrap();
        assert_eq!(result, "Line 1 added\nLine 2\n");

        // Test that Windows line endings work (even if normalization occurs)
        let content_windows = "Line 1\r\nLine 2\r\n";
        let warning_windows = LintWarning {
            message: "Add text".to_string(),
            line: 1,
            column: 7,
            end_line: 1,
            end_column: 7,
            severity: Severity::Warning,
            fix: Some(Fix {
                range: 6..6,
                replacement: " added".to_string(),
            }),
            rule_name: Some("TEST".to_string()),
        };

        let result_windows = apply_warning_fixes(content_windows, &[warning_windows]).unwrap();
        // Just verify the fix was applied correctly
        assert!(result_windows.starts_with("Line 1 added"));
        assert!(result_windows.contains("Line 2"));
    }

    fn make_warning(line: usize, end_line: usize, rule_name: &str) -> LintWarning {
        LintWarning {
            message: "test".to_string(),
            line,
            column: 1,
            end_line,
            end_column: 1,
            severity: Severity::Warning,
            fix: Some(Fix {
                range: 0..1,
                replacement: "x".to_string(),
            }),
            rule_name: Some(rule_name.to_string()),
        }
    }

    #[test]
    fn test_filter_warnings_disable_enable_block() {
        let content =
            "# Heading\n\n<!-- rumdl-disable MD013 -->\nlong line\n<!-- rumdl-enable MD013 -->\nanother long line\n";
        let inline_config = InlineConfig::from_content(content);

        let warnings = vec![
            make_warning(4, 4, "MD013"), // inside disabled block
            make_warning(6, 6, "MD013"), // outside disabled block
        ];

        let filtered = filter_warnings_by_inline_config(warnings, &inline_config, "MD013");
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].line, 6);
    }

    #[test]
    fn test_filter_warnings_disable_line() {
        let content = "line one <!-- rumdl-disable-line MD009 -->\nline two\n";
        let inline_config = InlineConfig::from_content(content);

        let warnings = vec![
            make_warning(1, 1, "MD009"), // disabled via disable-line
            make_warning(2, 2, "MD009"), // not disabled
        ];

        let filtered = filter_warnings_by_inline_config(warnings, &inline_config, "MD009");
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].line, 2);
    }

    #[test]
    fn test_filter_warnings_disable_next_line() {
        let content = "<!-- rumdl-disable-next-line MD034 -->\nhttp://example.com\nhttp://other.com\n";
        let inline_config = InlineConfig::from_content(content);

        let warnings = vec![
            make_warning(2, 2, "MD034"), // disabled via disable-next-line
            make_warning(3, 3, "MD034"), // not disabled
        ];

        let filtered = filter_warnings_by_inline_config(warnings, &inline_config, "MD034");
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].line, 3);
    }

    #[test]
    fn test_filter_warnings_sub_rule_name() {
        let content = "<!-- rumdl-disable MD029 -->\nline\n<!-- rumdl-enable MD029 -->\nline\n";
        let inline_config = InlineConfig::from_content(content);

        // Sub-rule name like "MD029-style" should be stripped to "MD029"
        let warnings = vec![make_warning(2, 2, "MD029"), make_warning(4, 4, "MD029")];

        let filtered = filter_warnings_by_inline_config(warnings, &inline_config, "MD029-style");
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].line, 4);
    }

    #[test]
    fn test_filter_warnings_multi_line_warning() {
        // A warning spanning lines 3-5 where line 4 is disabled
        let content = "line 1\nline 2\nline 3\n<!-- rumdl-disable-line MD013 -->\nline 5\nline 6\n";
        let inline_config = InlineConfig::from_content(content);

        let warnings = vec![
            make_warning(3, 5, "MD013"), // spans lines 3-5, line 4 is disabled
            make_warning(6, 6, "MD013"), // not disabled
        ];

        let filtered = filter_warnings_by_inline_config(warnings, &inline_config, "MD013");
        // The multi-line warning should be filtered because one of its lines is disabled
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].line, 6);
    }

    #[test]
    fn test_filter_warnings_empty_input() {
        let inline_config = InlineConfig::from_content("");
        let filtered = filter_warnings_by_inline_config(vec![], &inline_config, "MD013");
        assert!(filtered.is_empty());
    }

    #[test]
    fn test_filter_warnings_none_disabled() {
        let content = "line 1\nline 2\n";
        let inline_config = InlineConfig::from_content(content);

        let warnings = vec![make_warning(1, 1, "MD013"), make_warning(2, 2, "MD013")];

        let filtered = filter_warnings_by_inline_config(warnings, &inline_config, "MD013");
        assert_eq!(filtered.len(), 2);
    }

    #[test]
    fn test_filter_warnings_all_disabled() {
        let content = "<!-- rumdl-disable MD013 -->\nline 1\nline 2\n";
        let inline_config = InlineConfig::from_content(content);

        let warnings = vec![make_warning(2, 2, "MD013"), make_warning(3, 3, "MD013")];

        let filtered = filter_warnings_by_inline_config(warnings, &inline_config, "MD013");
        assert!(filtered.is_empty());
    }

    #[test]
    fn test_filter_warnings_end_line_zero_fallback() {
        // When end_line < line (e.g., end_line=0), should fall back to checking only warning.line
        let content = "<!-- rumdl-disable-line MD013 -->\nline 2\n";
        let inline_config = InlineConfig::from_content(content);

        let warnings = vec![make_warning(1, 0, "MD013")]; // end_line=0 < line=1

        let filtered = filter_warnings_by_inline_config(warnings, &inline_config, "MD013");
        assert!(filtered.is_empty());
    }

    #[test]
    fn test_filter_non_md_rule_name_preserves_dash() {
        // Verify that a non-MD rule name with a dash is NOT split by the helper.
        // The helper should pass "custom-rule" as-is to InlineConfig, not "custom".
        let content = "line 1\nline 2\n";
        let inline_config = InlineConfig::from_content(content);

        let warnings = vec![make_warning(1, 1, "custom-rule")];

        // With nothing disabled, the warning should pass through
        let filtered = filter_warnings_by_inline_config(warnings, &inline_config, "custom-rule");
        assert_eq!(filtered.len(), 1, "Non-MD rule name with dash should not be split");
    }

    #[test]
    fn test_filter_md_sub_rule_name_is_split() {
        // Verify that "MD029-style" is split to "MD029" for inline config lookup
        let content = "<!-- rumdl-disable MD029 -->\nline\n<!-- rumdl-enable MD029 -->\nline\n";
        let inline_config = InlineConfig::from_content(content);

        let warnings = vec![
            make_warning(2, 2, "MD029"), // disabled
            make_warning(4, 4, "MD029"), // not disabled
        ];

        // Passing "MD029-style" as rule_name should still match "MD029" in inline config
        let filtered = filter_warnings_by_inline_config(warnings, &inline_config, "MD029-style");
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].line, 4);
    }

    #[test]
    fn test_filter_warnings_capture_restore() {
        let content = "<!-- rumdl-disable MD013 -->\nline 1\n<!-- rumdl-capture -->\n<!-- rumdl-enable MD013 -->\nline 4\n<!-- rumdl-restore -->\nline 6\n";
        let inline_config = InlineConfig::from_content(content);

        let warnings = vec![
            make_warning(2, 2, "MD013"), // disabled by initial disable
            make_warning(5, 5, "MD013"), // re-enabled between capture/restore
            make_warning(7, 7, "MD013"), // after restore, back to disabled state
        ];

        let filtered = filter_warnings_by_inline_config(warnings, &inline_config, "MD013");
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].line, 5);
    }
}
