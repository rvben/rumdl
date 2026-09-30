//! MD013 reflow defects found by the reflow semantics oracle
//! (`fuzz/oracle/reflow_semantics.rs`).
//!
//! Each test pins the exact output of the production fix path and also runs
//! the oracle over the input, so a regression fails with the rendered
//! difference as well as the text one.

use super::reflow_semantics::{Mode, Outcome, ReflowSettings, check, reflow};

const REFLOW_MODES: [Mode; 3] = [Mode::Normalize, Mode::SentencePerLine, Mode::SemanticLineBreaks];

fn assert_reflows_to(input: &str, line_length: u64, modes: &[Mode], expected: &str) {
    for &mode in modes {
        let settings = ReflowSettings::with_mode(mode, line_length);
        let output = reflow(input, &settings).expect("reflow runs");
        assert_eq!(
            output, expected,
            "{mode:?} at line length {line_length}, input {input:?}"
        );
        if let Err(violation) = check(input, &settings) {
            panic!("{mode:?}: {}", violation.label());
        }
    }
}

/// A list item whose marker line holds nothing is empty unless the next line
/// is indented to its content column. An unindented line after it starts a
/// paragraph outside the list, and joining it onto the marker moved it into
/// the list.
#[test]
fn an_empty_list_item_keeps_the_paragraph_after_it_out_of_the_list() {
    for input in [
        "Tasks:\n\n-\nFollow-up notes go here.\n",
        "Tasks:\n\n*\nFollow-up notes go here.\n",
        "Tasks:\n\n1.\nFollow-up notes go here.\n",
        "Tasks:\n\n3)\nFollow-up notes go here.\n",
        "Tasks:\n\n-\t\nFollow-up notes go here.\n",
        "Tasks:\n\n- \nFollow-up notes go here.\n",
        "- first\n-\nFollow-up notes go here.\n",
        "- first\n\n  -\n  Follow-up notes go here.\n",
    ] {
        assert_reflows_to(input, 80, &REFLOW_MODES, input);
    }
}

/// The paragraph after an empty item is still reflowed, as a paragraph of
/// its own.
#[test]
fn the_paragraph_after_an_empty_list_item_is_still_reflowed() {
    assert_reflows_to(
        "-\nOne two three four five six seven eight.\n",
        20,
        &[Mode::Normalize],
        "-\nOne two three four\nfive six seven\neight.\n",
    );
}

/// Content indented to the content column belongs to the item and stays in
/// it.
#[test]
fn content_under_an_empty_marker_line_stays_in_the_item() {
    for input in ["-\n  one\n  two\n", "1.\n   one\n   two\n"] {
        for &mode in &REFLOW_MODES {
            let settings = ReflowSettings::with_mode(mode, 80);
            match check(input, &settings) {
                Ok(Outcome::Unchanged | Outcome::Rewritten) => {}
                Err(violation) => panic!("{mode:?} on {input:?}: {}", violation.label()),
            }
        }
    }
}
