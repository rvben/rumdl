//! Utilities for applying fixes consistently between CLI and LSP
//!
//! This module provides shared logic for applying markdown fixes to ensure
//! that both CLI batch fixes and LSP individual fixes produce identical results.

use crate::rule::{Fix, LintWarning};

/// Apply a list of warning fixes to content, simulating how the LSP client would apply them
/// This is used for testing consistency between CLI and LSP fix methods
pub fn apply_warning_fixes(content: &str, warnings: &[LintWarning]) -> Result<String, String> {
    let original_line_ending = crate::utils::detect_line_ending(content);
    let mut fixes: Vec<(usize, &Fix)> = warnings
        .iter()
        .enumerate()
        .filter_map(|(i, w)| w.fix.as_ref().map(|fix| (i, fix)))
        .collect();

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
                fix.range.start,
                fix.range.end
            ));
        }

        // Apply the fix by replacing the range with the replacement text
        // Normalize fix replacement to match document line endings
        let normalized_replacement = if original_line_ending == "\r\n" && !fix.replacement.contains("\r\n") {
            fix.replacement.replace('\n', "\r\n")
        } else {
            fix.replacement.clone()
        };
        
        result.replace_range(fix.range.clone(), &normalized_replacement);
    }

    // For consistency with CLI behavior, normalize all line endings in the result
    // to match the detected predominant style
    let normalized_result = if original_line_ending == "\r\n" {
        result.replace('\n', "\r\n")
    } else {
        result.replace("\r\n", "\n")
    };
    
    Ok(normalized_result)
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
            fix.range.start,
            fix.range.end
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
                range: 2..4, // "  " (two spaces)
                replacement: " ".to_string(), // single space
            }),
            rule_name: Some("MD030"),
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
                rule_name: Some("MD030"),
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
                rule_name: Some("MD030"),
            },
        ];

        let result = apply_warning_fixes(content, &warnings).unwrap();
        assert_eq!(result, "1. First\n* Second");
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

        // Invalid range (start > end)
        let invalid_fix2 = Fix {
            range: 5..3,
            replacement: "Hi".to_string(),
        };
        assert!(validate_fix_range(content, &invalid_fix2).is_err());
    }
}