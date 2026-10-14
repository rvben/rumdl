//! MD013 `reflow-break-code-spans`: wrapping emphasis without splitting code.
//!
//! `atomic-spans = false` lets emphasis, strong, strikethrough and inline code
//! wrap word by word. `reflow-break-code-spans = false` keeps inline code whole
//! while the rest still wraps. The key never makes code wrap when
//! `atomic-spans = true`, and the whitespace, container and indentation
//! safeguards apply whatever its value.
//!
//! Every case runs the real binary and asserts exact output, idempotence, and
//! that the document structure is unchanged. The structure oracle parses the
//! input and the output with pulldown-cmark, treats a soft break as a space,
//! and compares the event streams, so reflow may only move soft breaks.

use pulldown_cmark::{Event, Options, Parser};
use std::fs;
use std::process::Command;
use tempfile::tempdir;

/// The parse of `content` with soft breaks read as spaces and adjacent text
/// merged, so two documents that differ only in where they wrap compare equal.
fn structure(content: &str) -> Vec<String> {
    let mut events: Vec<String> = Vec::new();
    let mut text = String::new();
    let flush = |text: &mut String, events: &mut Vec<String>| {
        if !text.is_empty() {
            events.push(format!("Text({})", std::mem::take(text)));
        }
    };
    for event in Parser::new_ext(content, Options::ENABLE_STRIKETHROUGH) {
        match event {
            Event::Text(t) => text.push_str(&t),
            Event::SoftBreak => text.push(' '),
            Event::Code(code) => {
                flush(&mut text, &mut events);
                events.push(format!("Code({code})"));
            }
            other => {
                flush(&mut text, &mut events);
                events.push(format!("{other:?}"));
            }
        }
    }
    flush(&mut text, &mut events);
    events
}

struct Settings {
    atomic: bool,
    /// `None` leaves the key unset (its default).
    break_code: Option<bool>,
    mode: &'static str,
    width: usize,
}

impl Settings {
    fn new(atomic: bool, break_code: Option<bool>) -> Self {
        Self {
            atomic,
            break_code,
            mode: "normalize",
            width: 40,
        }
    }

    fn mode(mut self, mode: &'static str) -> Self {
        self.mode = mode;
        self
    }

    fn args(&self) -> Vec<String> {
        let mut args = vec![
            "MD013.reflow=true".to_string(),
            format!("MD013.line-length={}", self.width),
            format!("MD013.reflow-mode=\"{}\"", self.mode),
            format!("MD013.atomic-spans={}", self.atomic),
        ];
        if let Some(value) = self.break_code {
            args.push(format!("MD013.reflow-break-code-spans={value}"));
        }
        args
    }
}

fn run(subcommand: &str, path: &std::path::Path, settings: &Settings) -> std::process::Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_rumdl"));
    command
        .args([subcommand, "--isolated", "--no-cache", "--enable", "MD013"])
        .arg("--output-format")
        .arg("concise");
    for setting in settings.args() {
        command.arg("-c").arg(setting);
    }
    command.arg(path).output().expect("run rumdl")
}

/// Format `content` and assert the structure is preserved and a second pass
/// changes nothing. Returns the formatted text.
fn fmt(content: &str, settings: &Settings) -> String {
    let dir = tempdir().unwrap();
    let path = dir.path().join("doc.md");
    fs::write(&path, content).unwrap();
    // The exit status reports what is left unfixed, which is not under test.
    run("fmt", &path, settings);
    let formatted = fs::read_to_string(&path).unwrap();
    assert_eq!(
        structure(content),
        structure(&formatted),
        "reflow changed the document structure\ninput:\n{content}\noutput:\n{formatted}"
    );
    run("fmt", &path, settings);
    assert_eq!(
        fs::read_to_string(&path).unwrap(),
        formatted,
        "a second fmt pass changed the output"
    );
    formatted
}

/// The three-way table for one input: `atomic = true` with either value of the
/// key, `atomic = false` with the key at its default and set to `true`, and
/// `atomic = false` with the key set to `false`.
fn table(content: &str, mode: &'static str) -> (String, String, String) {
    let atomic_default = fmt(content, &Settings::new(true, None).mode(mode));
    let atomic_true = fmt(content, &Settings::new(true, Some(true)).mode(mode));
    let atomic_false = fmt(content, &Settings::new(true, Some(false)).mode(mode));
    assert_eq!(
        atomic_default, atomic_true,
        "atomic-spans = true ignores the key (default vs true)"
    );
    assert_eq!(
        atomic_default, atomic_false,
        "atomic-spans = true ignores the key (default vs false)"
    );

    let break_default = fmt(content, &Settings::new(false, None).mode(mode));
    let break_true = fmt(content, &Settings::new(false, Some(true)).mode(mode));
    assert_eq!(break_default, break_true, "the key defaults to true");

    let whole = fmt(content, &Settings::new(false, Some(false)).mode(mode));
    (atomic_default, break_true, whole)
}

const ISSUE: &str = "# Example

This paragraph contains **several emphasized words together** and then `several code words together` at the end.

Alpha beta gamma delta epsilon zeta `one two three four five six seven` omega.
";

#[test]
fn issue_input_keeps_code_whole_and_wraps_emphasis() {
    let (atomic, split, whole) = table(ISSUE, "normalize");

    // Code split across lines: today's behaviour, byte for byte.
    assert_eq!(
        split,
        "# Example

This paragraph contains **several
emphasized words together** and then
`several code words together` at the
end.

Alpha beta gamma delta epsilon zeta `one
two three four five six seven` omega.
"
    );
    // The requested output: bold wraps, code stays on one line.
    assert_eq!(
        whole,
        "# Example

This paragraph contains **several
emphasized words together** and then
`several code words together` at the
end.

Alpha beta gamma delta epsilon zeta
`one two three four five six seven`
omega.
"
    );
    // atomic-spans = true holds the emphasis whole as well, whatever the key.
    assert_eq!(
        atomic,
        "# Example

This paragraph contains
**several emphasized words together**
and then `several code words together`
at the end.

Alpha beta gamma delta epsilon zeta
`one two three four five six seven`
omega.
"
    );
}

#[test]
fn modes_agree_with_the_table() {
    let content = "Alpha beta gamma delta epsilon zeta `one two three four five six seven` omega. Next sentence has **bold words that go on and on for a while** to wrap.\n";

    let split = "Alpha beta gamma delta epsilon zeta `one
two three four five six seven` omega.
Next sentence has **bold words that go
on and on for a while** to wrap.
";
    let whole = "Alpha beta gamma delta epsilon zeta
`one two three four five six seven`
omega. Next sentence has **bold words
that go on and on for a while** to wrap.
";
    for mode in ["default", "normalize"] {
        let (_, got_split, got_whole) = table(content, mode);
        assert_eq!(got_split, split, "{mode}: break = true");
        assert_eq!(got_whole, whole, "{mode}: break = false");
    }

    let (_, got_split, got_whole) = table(content, "semantic-line-breaks");
    assert_eq!(got_split, split, "semantic-line-breaks: break = true");
    assert_eq!(
        got_whole,
        "Alpha beta gamma delta epsilon zeta
`one two three four five six seven`
omega.
Next sentence has **bold words that go
on and on for a while** to wrap.
",
        "semantic-line-breaks: break = false"
    );

    // These modes put a sentence on a line whatever its length, so the key has
    // nothing to decide.
    let per_line = "Alpha beta gamma delta epsilon zeta `one two three four five six seven` omega.
Next sentence has **bold words that go on and on for a while** to wrap.
";
    for mode in ["sentence-per-line", "sentence-pack"] {
        let (_, got_split, got_whole) = table(content, mode);
        assert_eq!(got_split, per_line, "{mode}: break = true");
        assert_eq!(got_whole, per_line, "{mode}: break = false");
    }
}

#[test]
fn containers_keep_code_whole_for_every_value() {
    // Wrapping inside a container could insert literal indentation into a code
    // span, so the container paths hold it whole whatever the key says.
    let cases = [
        (
            "- Alpha beta gamma delta epsilon zeta `one two three four five six seven` omega.\n",
            "- Alpha beta gamma delta epsilon zeta
  `one two three four five six seven`
  omega.
",
        ),
        (
            "> Alpha beta gamma delta epsilon zeta `one two three four five six seven` omega.\n",
            "> Alpha beta gamma delta epsilon zeta
> `one two three four five six seven`
> omega.
",
        ),
        (
            "> - Alpha beta gamma delta epsilon zeta `one two three four five six seven` omega.\n",
            "> - Alpha beta gamma delta epsilon zeta
>   `one two three four five six seven`
>   omega.
",
        ),
        (
            "- Item\n\n  Alpha beta gamma delta epsilon zeta `one two three four five six seven` omega.\n",
            "- Item

  Alpha beta gamma delta epsilon zeta
  `one two three four five six seven`
  omega.
",
        ),
    ];
    for (content, expected) in cases {
        let (atomic, split, whole) = table(content, "normalize");
        assert_eq!(atomic, expected, "atomic\n{content}");
        assert_eq!(split, expected, "break = true\n{content}");
        assert_eq!(whole, expected, "break = false\n{content}");
    }
}

#[test]
fn container_emphasis_still_wraps_with_code_whole() {
    let content =
        "- Alpha **bold words that go on and on for a while** then `one two three four five six seven` omega.\n";
    let whole = fmt(content, &Settings::new(false, Some(false)));
    assert_eq!(
        whole,
        "- Alpha **bold words that go on and on
  for a while** then
  `one two three four five six seven`
  omega.
"
    );
}

#[test]
fn code_span_longer_than_the_limit_stays_whole() {
    let content = "Alpha `this is a code span with many words that exceeds the limit` omega.\n";
    let (atomic, split, whole) = table(content, "normalize");
    let expected = "Alpha
`this is a code span with many words that exceeds the limit`
omega.
";
    assert_eq!(whole, expected);
    assert_eq!(atomic, expected);
    // With the key at its default the span is broken to fit, as before.
    assert_ne!(split, expected);
    assert!(
        split.lines().all(|line| line.len() <= 40),
        "split output fits:\n{split}"
    );

    // The span is left over-long on its own line. Whether the check reports it
    // is the existing length-exemption logic, so assert the real result: with
    // `code-spans` at its default the line is reported.
    let dir = tempdir().unwrap();
    let path = dir.path().join("doc.md");
    fs::write(&path, &whole).unwrap();
    let output = run("check", &path, &Settings::new(false, Some(false)));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("doc.md:2:") && stdout.contains("MD013"),
        "the over-long code line is reported: {stdout}"
    );
}

#[test]
fn whitespace_unsafe_code_stays_whole_for_every_value() {
    // Wrapping would collapse two spaces or a tab, so these never wrap.
    for content in [
        "Alpha beta gamma delta `one  two three four five six` omega zeta eta theta.\n",
        "Alpha beta gamma delta `one\ttwo three four five six` omega zeta eta theta.\n",
    ] {
        let (atomic, split, whole) = table(content, "normalize");
        assert_eq!(atomic, split);
        assert_eq!(split, whole);
        assert!(
            whole.contains("`one  two three four five six`") || whole.contains("`one\ttwo three four five six`"),
            "code kept byte for byte:\n{whole}"
        );
    }
}

#[test]
fn code_nested_in_emphasis_stays_whole() {
    let content = "**bold with `code words inside here` and more bold words to wrap around**\n";
    let expected = "**bold with `code words inside here` and
more bold words to wrap around**
";
    let (_, split, whole) = table(content, "normalize");
    assert_eq!(split, expected);
    assert_eq!(whole, expected);
}

#[test]
fn over_long_emphasis_wraps_under_atomic_spans() {
    // The emphasis exception: a span longer than the limit wraps even when
    // spans are atomic, whatever the key says.
    let content = "**one two three four five six seven eight nine ten eleven twelve** and `code words here`.\n";
    let (atomic, _, _) = table(content, "normalize");
    assert_eq!(
        atomic,
        "**one two three four five six seven
eight nine ten eleven twelve** and
`code words here`.
"
    );
}

#[test]
fn key_is_accepted_under_both_spellings_without_warning() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("doc.md");
    fs::write(&path, "# Title\n\nText.\n").unwrap();
    for spelling in ["reflow-break-code-spans", "reflow_break_code_spans"] {
        let output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
            .args(["check", "--isolated", "--no-cache", "-c"])
            .arg(format!("MD013.{spelling}=false"))
            .arg(&path)
            .output()
            .unwrap();
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(!stderr.contains("Unknown option"), "{spelling}: {stderr}");
    }
    // Negative control: a misspelling does warn, so the check above can fail.
    let output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .args([
            "check",
            "--isolated",
            "--no-cache",
            "-c",
            "MD013.reflow-break-code-span=false",
        ])
        .arg(&path)
        .output()
        .unwrap();
    assert!(String::from_utf8_lossy(&output.stderr).contains("Unknown option"));

    // Both spellings take effect.
    for spelling in ["reflow-break-code-spans", "reflow_break_code_spans"] {
        let settings = Settings::new(false, None);
        let extra = format!("MD013.{spelling}=false");
        let path = dir.path().join("wrap.md");
        fs::write(
            &path,
            "Alpha beta gamma delta epsilon zeta `one two three four five six seven` omega.\n",
        )
        .unwrap();
        let mut command = Command::new(env!("CARGO_BIN_EXE_rumdl"));
        command.args(["fmt", "--isolated", "--no-cache", "--enable", "MD013"]);
        for arg in settings.args() {
            command.arg("-c").arg(arg);
        }
        command.arg("-c").arg(extra).arg(&path).output().unwrap();
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            "Alpha beta gamma delta epsilon zeta\n`one two three four five six seven`\nomega.\n",
            "{spelling}"
        );
    }
}

#[test]
fn default_config_lists_the_key() {
    let output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .args(["config", "--defaults", "--isolated"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("reflow-break-code-spans = true"),
        "default config output lists the key as true:\n{stdout}"
    );
}

#[test]
fn structure_oracle_detects_code_changes() {
    // Negative controls: the oracle must see a changed code span and a changed
    // emphasis range, and ignore only soft breaks.
    assert_ne!(structure("`a b`\n"), structure("`a  b`\n"));
    assert_ne!(structure("`a b`\n"), structure("`a` b\n"));
    assert_ne!(structure("**a b**\n"), structure("**a** b\n"));
    assert_eq!(structure("a b c\n"), structure("a\nb c\n"));
    assert_eq!(structure("`a b`\n"), structure("`a\nb`\n"));
}
