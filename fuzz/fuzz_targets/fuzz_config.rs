#![no_main]

//! Fuzz target: config deserialization must never panic.
//!
//! Arbitrary bytes are interpreted as TOML and deserialized into Config.
//! Failures (invalid TOML, unknown keys, wrong types) are expected and fine —
//! panics are not.

use libfuzzer_sys::fuzz_target;
use rumdl_lib::config::Config;

fuzz_target!(|data: &[u8]| {
    let Ok(content) = std::str::from_utf8(data) else {
        return;
    };

    if content.len() > 50_000 {
        return;
    }

    // Must not panic — errors are fine
    let _ = toml::from_str::<Config>(content);
});
