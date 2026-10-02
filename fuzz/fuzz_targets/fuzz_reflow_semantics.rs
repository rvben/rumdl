#![no_main]

//! Fuzz target: MD013 reflow must preserve what a document renders to, and a
//! second `fmt` pass must change nothing.
//!
//! The first two bytes select the reflow configuration (mode, line length and
//! the options that change where reflow may break); the rest is the document.
//! The properties themselves live in `oracle/reflow_semantics.rs`.

use libfuzzer_sys::fuzz_target;

#[path = "../oracle/reflow_semantics.rs"]
mod reflow_semantics;

use reflow_semantics::{ReflowSettings, ViolationKind, check};

/// Documents this target does not judge: a bare CR (a line ending to
/// CommonMark, which rumdl does not read as one anywhere; CRLF is judged), a
/// vertical tab or a form feed (whose handling at a line end the renderers
/// disagree on). None occurs in Markdown people write, and each would drown
/// every other finding.
fn has_out_of_scope_control(content: &str) -> bool {
    let bytes = content.as_bytes();
    bytes.iter().enumerate().any(|(i, &b)| {
        matches!(b, 0x0B | 0x0C) || (b == b'\r' && bytes.get(i + 1) != Some(&b'\n'))
    })
}

fuzz_target!(|data: &[u8]| {
    let [a, b, rest @ ..] = data else {
        return;
    };
    let Ok(content) = std::str::from_utf8(rest) else {
        return;
    };
    if content.len() > 10_000 || has_out_of_scope_control(content) {
        return;
    }

    let settings = ReflowSettings::from_bytes(*a, *b);
    if let Err(violation) = check(content, &settings) {
        let detail = match &violation.kind {
            ViolationKind::RenderChanged { before, after, .. } => {
                format!("rendered before:\n{before}\nrendered after:\n{after}")
            }
            ViolationKind::NotIdempotent { second } => format!("second pass:\n{second}"),
            ViolationKind::FixFailed(error) => format!("error: {error}"),
        };
        panic!(
            "{}\nreproduce: rumdl fmt {}\ninput:\n{:?}\noutput:\n{:?}\n{detail}",
            violation.label(),
            settings.shell_args(),
            violation.input,
            violation.output,
        );
    }
});
