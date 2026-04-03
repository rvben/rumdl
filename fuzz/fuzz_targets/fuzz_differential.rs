#![no_main]

//! Fuzz target: linting the same content twice must produce identical results.
//!
//! Verifies that the linter is deterministic — no rule may mutate shared state
//! in a way that changes its output on a second pass over the same content.

use libfuzzer_sys::fuzz_target;
use rumdl_lib::config::{Config, MarkdownFlavor};
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

    let Ok(first) =
        rumdl_lib::lint(content, &rules, false, MarkdownFlavor::Standard, None, Some(&config))
    else {
        return;
    };

    let rules2 = all_rules(&config);
    let Ok(second) =
        rumdl_lib::lint(content, &rules2, false, MarkdownFlavor::Standard, None, Some(&config))
    else {
        return;
    };

    assert_eq!(
        first.len(),
        second.len(),
        "Linting the same content twice produced different warning counts ({} vs {})",
        first.len(),
        second.len(),
    );

    for (a, b) in first.iter().zip(second.iter()) {
        assert_eq!(a.rule_name, b.rule_name, "Rule name mismatch between runs");
        assert_eq!(a.line, b.line, "Line number mismatch between runs");
        assert_eq!(a.column, b.column, "Column mismatch between runs");
        assert_eq!(a.message, b.message, "Message mismatch between runs");
    }
});
