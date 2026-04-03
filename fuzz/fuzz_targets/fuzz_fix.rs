#![no_main]

//! Fuzz target: applying fixes must never panic and must produce valid UTF-8.
//!
//! Unlike fuzz_fix_idempotency, this target focuses on the fix application itself:
//! the coordinator must not panic, and its output must be a valid string.

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

    let Ok(warnings) = rumdl_lib::lint(content, &rules, false, MarkdownFlavor::Standard, None, Some(&config)) else {
        return;
    };

    if warnings.is_empty() {
        return;
    }

    let mut buf = content.to_string();
    let coordinator = FixCoordinator::new();
    if let Ok(_) = coordinator.apply_fixes_iterative(&rules, &warnings, &mut buf, &config, 10, None) {
        // Output must be valid UTF-8 (it is, since buf is a String)
        // Verify the fixed content can itself be linted without panicking
        let _ = rumdl_lib::lint(&buf, &rules, false, MarkdownFlavor::Standard, None, Some(&config));
    }
});
