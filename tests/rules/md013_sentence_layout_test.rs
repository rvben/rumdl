use super::{
    reflow_semantics::{Mode, ReflowSettings, check, check_with, reflow},
    sentence_layout,
};

#[test]
fn sentence_modes_format_short_and_unlimited_footnotes() {
    let joined = "A reference[^note].\n\n[^note]: First sentence. Second sentence.\n";
    let split = "A reference[^note].\n\n[^note]: First sentence.\n    Second sentence.\n";
    for width in [0, 80] {
        for mode in [Mode::SentencePerLine, Mode::SemanticLineBreaks, Mode::SentencePack] {
            let settings = ReflowSettings::with_mode(mode, width);
            let (input, expected) = if mode == Mode::SentencePack {
                (split, joined)
            } else {
                (joined, split)
            };
            assert_eq!(reflow(input, &settings).unwrap(), expected, "{settings:?}");
            check(input, &settings).unwrap();
        }
    }
}

#[test]
fn wrapping_invalid_backtick_fence_keeps_it_inline() {
    for input in [
        "```example `literal` trailing words.\n",
        "> ```example `literal` trailing words.\n",
        "- ```example `literal` trailing words.\n",
        "A reference[^note].\n\n[^note]: ```example `literal` trailing words.\n",
    ] {
        for mode in Mode::ALL {
            for width in [10, 20, 80] {
                let settings = ReflowSettings::with_mode(mode, width);
                let output = reflow(input, &settings).unwrap();
                assert!(output.contains("```example `literal`"), "{settings:?}: {output:?}");
                check(input, &settings).unwrap();
            }
        }
    }
}

#[test]
fn ordered_numbers_inside_prose_do_not_indent_wrapped_code() {
    for input in [
        "Prefix words 819) `literal code text`.\n",
        "Prefix words 7. `literal code text`.\n",
        "Prefix words\n819) `literal code text`.\n",
    ] {
        for mode in Mode::ALL {
            let mut settings = ReflowSettings::with_mode(mode, 10);
            settings.atomic_spans = false;
            check(input, &settings).unwrap();
        }
    }
}

#[test]
fn raw_html_blocks_end_the_paragraph_before_them() {
    for input in [
        "Prefix **word\n<!-- note -->tail\nContinuation **word.\n",
        "Prefix\n<!lowercase declaration>tail\nContinuation text.\n",
        "Prefix\n<?instruction?>tail\nContinuation text.\n",
        "Prefix\n<![CDATA[value]]>tail\nContinuation text.\n",
    ] {
        for mode in Mode::ALL {
            let settings = ReflowSettings::with_mode(mode, 40);
            check(input, &settings).unwrap();
        }
    }
}

#[test]
fn continuation_prefixes_do_not_add_whitespace_inside_wrapped_code() {
    for input in [
        "- Properties cannot use `literal code text` here.\n",
        "> Properties cannot use `literal code text` here.\n",
        "A reference[^note].\n\n[^note]: Properties cannot use `literal code text` here.\n",
    ] {
        for mode in Mode::ALL {
            let mut settings = ReflowSettings::with_mode(mode, 10);
            settings.atomic_spans = false;
            check(input, &settings).unwrap();
        }
    }
}

#[test]
fn lazy_continuation_of_an_empty_list_item_keeps_its_content_indent() {
    for input in [
        "-\n  value\n `\n",
        "3)\n   value\n `\n",
        "-\n  v\n `",
        "-\n  |\n||\n\n  ;\n`",
    ] {
        for mode in Mode::ALL {
            check(input, &ReflowSettings::with_mode(mode, 80)).unwrap();
        }
        check(input, &ReflowSettings::from_bytes(52, 5)).unwrap();
    }
}

#[test]
fn joining_an_invalid_fence_line_does_not_activate_a_bare_autolink() {
    let settings = ReflowSettings::from_bytes(236, 1);
    for input in [
        "[Read with me at xmpp:user@example.com\n```literal`\n",
        "[Read with me at xmpp:user@example.com\n```literal`",
        "[tlavorewith me at xmpp:user@example.com\n```lock_line_length`",
        "- [tlavorewith me at xmpp:user@example.com\n```lock_line_length`",
    ] {
        check(input, &settings).unwrap();
    }
}

#[test]
fn generated_sentence_layout_matrix() {
    for kind in 0..7 {
        for container in 0..6 {
            for span in 0..7 {
                for breaks in 0..2 {
                    sentence_layout::from_bytes([kind, container, span, 12, span, 0, span, breaks])
                        .check()
                        .unwrap_or_else(|error| panic!("{error}"));
                }
            }
        }
    }
}

proptest::proptest! {
    #[test]
    fn generated_sentence_layout_matches_its_known_boundaries(data in proptest::prelude::any::<[u8; 8]>()) {
        let result = sentence_layout::from_bytes(data).check();
        proptest::prop_assert!(result.is_ok(), "case: {:?}, failure: {:?}", data, result);
    }
}

#[test]
fn layout_oracle_rejects_render_equivalent_sentence_mistakes() {
    let step = sentence_layout::from_bytes([0; 8]);
    let split_label = step.expected.replace("1. Verify", "1.\nVerify");
    assert!(check_with(&step.input, &step.settings, |_, _| Ok(split_label.clone())).is_ok());
    assert!(step.verify_output(&split_label).is_err());
    let lowercase = sentence_layout::from_bytes([1, 0, 1, 0, 0, 0, 0, 0]);
    let joined_sentence = lowercase.expected.replace("\nrelease-please", " release-please");
    assert!(
        check_with(
            &lowercase.input,
            &lowercase.settings,
            |_, _| Ok(joined_sentence.clone())
        )
        .is_ok()
    );
    assert!(lowercase.verify_output(&joined_sentence).is_err());
}
