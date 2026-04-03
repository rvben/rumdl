#![no_main]

//! Fuzz target: verify key properties that must hold for all inputs.
//!
//! Properties verified:
//! 1. Warning line numbers are within bounds (1..=line_count).
//! 2. Warning column numbers are plausible (non-zero).
//! 3. Fix ranges do not exceed content length.
//! 4. After applying all fixes, linting the result does not panic.

use libfuzzer_sys::fuzz_target;
use rumdl_lib::config::{Config, MarkdownFlavor};
use rumdl_lib::fix_coordinator::FixCoordinator;
use rumdl_lib::rules::all_rules;

fuzz_target!(|data: &[u8]| {
    let Ok(content) = std::str::from_utf8(data) else {
        return;
    };

    if content.len() > 50_000 {
        return;
    }

    let config = Config::default();
    let rules = all_rules(&config);

    let Ok(warnings) =
        rumdl_lib::lint(content, &rules, false, MarkdownFlavor::Standard, None, Some(&config))
    else {
        return;
    };

    let line_count = content.lines().count().max(1);
    let content_len = content.len();

    for w in &warnings {
        // Line numbers must be within the document
        assert!(
            w.line >= 1 && w.line <= line_count + 1,
            "Warning line {} out of bounds (document has {} lines, rule: {:?})",
            w.line,
            line_count,
            w.rule_name,
        );

        // Column must be non-zero (1-indexed)
        assert!(
            w.column >= 1,
            "Warning column {} is zero (rule: {:?})",
            w.column,
            w.rule_name,
        );

        // Fix byte ranges must be within content bounds
        if let Some(fix) = &w.fix {
            assert!(
                fix.range.start <= content_len && fix.range.end <= content_len,
                "Fix range {:?} exceeds content length {} (rule: {:?})",
                fix.range,
                content_len,
                w.rule_name,
            );
            assert!(
                fix.range.start <= fix.range.end,
                "Fix range start {} > end {} (rule: {:?})",
                fix.range.start,
                fix.range.end,
                w.rule_name,
            );
        }
    }

    // Applying fixes must not panic, and the result must be lintable
    if !warnings.is_empty() {
        let mut buf = content.to_string();
        let coordinator = FixCoordinator::new();
        if coordinator
            .apply_fixes_iterative(&rules, &warnings, &mut buf, &config, 10, None)
            .is_ok()
        {
            let _ = rumdl_lib::lint(&buf, &rules, false, MarkdownFlavor::Standard, None, Some(&config));
        }
    }
});
