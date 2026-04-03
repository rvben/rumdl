#![no_main]

//! Fuzz target: linting arbitrary content must never panic.
//!
//! Covers the full lint pipeline for all rules against arbitrary inputs.

use libfuzzer_sys::fuzz_target;
use rumdl_lib::config::{Config, MarkdownFlavor};
use rumdl_lib::rules::all_rules;

fuzz_target!(|data: &[u8]| {
    let Ok(content) = std::str::from_utf8(data) else {
        return;
    };

    if content.len() > 100_000 {
        return;
    }

    let config = Config::default();
    let rules = all_rules(&config);

    // Must not panic regardless of content
    let _ = rumdl_lib::lint(content, &rules, false, MarkdownFlavor::Standard, None, Some(&config));
});
