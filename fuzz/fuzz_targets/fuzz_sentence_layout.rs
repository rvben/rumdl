#![no_main]

//! Grammar-guided fuzzing with independently specified sentence boundaries.

use libfuzzer_sys::fuzz_target;

#[path = "../oracle/reflow_semantics.rs"]
mod reflow_semantics;
#[path = "../oracle/sentence_layout.rs"]
mod sentence_layout;

fuzz_target!(|data: [u8; 8]| {
    if let Err(error) = sentence_layout::from_bytes(data).check() {
        panic!("{error}");
    }
});
