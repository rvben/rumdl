#![no_main]

//! Fuzz target that verifies fix idempotency:
//! After applying fixes, re-linting and re-fixing should produce the same content.

use libfuzzer_sys::fuzz_target;
use rumdl_lib::config::{Config, MarkdownFlavor};
use rumdl_lib::fix_coordinator::FixCoordinator;
use rumdl_lib::rules::all_rules;

fuzz_target!(|data: &[u8]| {
    let Ok(content) = std::str::from_utf8(data) else {
        return;
    };

    // Skip extreme inputs
    if content.is_empty() || content.len() > 50_000 {
        return;
    }

    let config = Config::default();
    let rules = all_rules(&config);

    // Lint the content
    let Ok(warnings) = rumdl_lib::lint(content, &rules, false, MarkdownFlavor::Standard, Some(&config)) else {
        return;
    };

    if warnings.is_empty() {
        return;
    }

    // Apply fixes
    let mut fixed1 = content.to_string();
    let coordinator = FixCoordinator::new();
    let Ok(result1) = coordinator.apply_fixes_iterative(&rules, &warnings, &mut fixed1, &config, 10) else {
        return;
    };

    if result1.rules_fixed == 0 {
        return;
    }

    // Re-lint the fixed content
    let Ok(warnings2) = rumdl_lib::lint(&fixed1, &rules, false, MarkdownFlavor::Standard, Some(&config)) else {
        return;
    };

    // Apply fixes again
    let mut fixed2 = fixed1.clone();
    let _ = coordinator.apply_fixes_iterative(&rules, &warnings2, &mut fixed2, &config, 10);

    // Idempotency check: second fix pass should not change content
    assert_eq!(fixed1, fixed2, "Fix is not idempotent");
});
