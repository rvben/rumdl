use std::{fs, process::Command};

fn assert_reflow(input: &str, expected: &str, mode: &str, extra: &[&str]) {
    for args in [vec!["fmt"], vec!["check", "--fix"]] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("doc.md");
        fs::write(&path, input).unwrap();
        for _ in 0..2 {
            let output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
                .args(&args)
                .args([
                    "--isolated",
                    "--no-cache",
                    "--enable",
                    "MD013",
                    "-c",
                    "MD013.reflow=true",
                    "-c",
                    "MD013.line-length=0",
                    "-c",
                    &format!("MD013.reflow-mode=\"{mode}\""),
                ])
                .args(extra)
                .arg(&path)
                .output()
                .unwrap();
            assert!(output.status.success(), "{args:?}: {output:?}");
            assert_eq!(fs::read_to_string(&path).unwrap(), expected, "{mode}");
        }
    }
}

#[test]
fn common_abbreviations_survive_all_sentence_modes() {
    for mode in ["sentence-per-line", "sentence-pack", "semantic-line-breaks"] {
        let input = "Capt. Smith used approx. 6.5% of the material in eq. 4.\n";
        assert_reflow(input, input, mode, &[]);
    }
    assert_reflow(
        "Col. Smith reviewed it. Lt. Jones agreed.\n",
        "Col. Smith reviewed it.\nLt. Jones agreed.\n",
        "sentence-per-line",
        &[],
    );
    assert_reflow(
        "See approx.\n6.5% in eq. 4.\n",
        "See approx. 6.5% in eq. 4.\n",
        "sentence-per-line",
        &[],
    );
}

#[test]
fn sentence_final_units_and_custom_abbreviations_keep_their_meaning() {
    assert_reflow(
        "The font is 12 pt. We kept it.\n",
        "The font is 12 pt.\nWe kept it.\n",
        "sentence-per-line",
        &[],
    );
    let input = "Read pt. Four next.\n";
    assert_reflow(
        input,
        input,
        "sentence-per-line",
        &["-c", "MD013.abbreviations=[\"pt\"]"],
    );
}

#[test]
fn reference_abbreviations_share_sentence_spacing_semantics() {
    for (input, expected) in [
        (
            "See p.  12, approx.  6.5, and Col.  Smith.\n",
            "See p. 12, approx. 6.5, and Col. Smith.\n",
        ),
        (
            "The symbol is p.  Next sentence.\n",
            "The symbol is p.  Next sentence.\n",
        ),
        ("We are in CA.  Next sentence.\n", "We are in CA.  Next sentence.\n"),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("doc.md");
        fs::write(&path, input).unwrap();
        let output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
            .args([
                "check",
                "--fix",
                "--isolated",
                "--no-cache",
                "--enable",
                "MD064",
                "-c",
                "MD064.allow-sentence-double-space=true",
            ])
            .arg(&path)
            .output()
            .unwrap();
        assert!(output.status.success(), "{output:?}");
        assert_eq!(fs::read_to_string(&path).unwrap(), expected);
    }
}

#[test]
fn numbered_abbreviations_keep_formatted_numbers_on_the_same_sentence() {
    for number in ["12", "*12*", "**12**", "`12`", "[12](https://example.com)"] {
        for separator in [" ", "\n"] {
            let input = format!("First sentence. See p.{separator}{number} for details.\n");
            let expected = format!("First sentence.\nSee p. {number} for details.\n");
            assert_reflow(&input, &expected, "sentence-per-line", &[]);
        }
    }
}
