//! A line indented four columns past a blockquote's edge under a running
//! paragraph is text that paragraph continues, so no rule that reads headings
//! sees a heading there. Against the real binary.

use std::fs;
use std::process::{Command, Output};
use tempfile::tempdir;

/// Runs `rumdl check` on `content` with only `rule` enabled and `config` appended
/// to the config file.
fn run(content: &str, rule: &str, config: &str) -> Output {
    run_with(content, rule, config, &[])
}

fn run_with(content: &str, rule: &str, config: &str, args: &[&str]) -> Output {
    let dir = tempdir().unwrap();
    let path = dir.path().join("doc.md");
    let config_path = dir.path().join("rumdl.toml");
    fs::write(&path, content).unwrap();
    fs::write(&config_path, format!("[global]\nenable = [\"{rule}\"]\n\n{config}\n")).unwrap();
    Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .args(["check", "--no-cache"])
        .args(args)
        .arg("--config")
        .arg(&config_path)
        .arg(&path)
        .output()
        .expect("run rumdl check")
}

fn lines(output: &Output, rule: &str) -> Vec<usize> {
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|line| line.contains(&format!("[{rule}]")))
        .filter_map(|line| line.split(':').nth(1)?.trim().parse().ok())
        .collect()
}

/// Each case is the same document written with the continuation line indented
/// four columns (text) and three columns (a heading).
#[test]
fn duplicate_heading_rule_ignores_continuation_text() {
    for (text, heading) in [
        ("# Same\n\n> p\n>     # Same\n", "# Same\n\n> p\n>    # Same\n"),
        ("# Same\n\n> p\n>\t\t# Same\n", "# Same\n\n> p\n>\t# Same\n"),
        ("# Same\n\n> > p\n> >     # Same\n", "# Same\n\n> > p\n> >    # Same\n"),
        (
            "# Same\n\n- x\n\n  > p\n  >     # Same\n",
            "# Same\n\n- x\n\n  > p\n  >    # Same\n",
        ),
    ] {
        let output = run(text, "MD024", "");
        assert!(lines(&output, "MD024").is_empty(), "{text:?}: {output:?}");
        let output = run(heading, "MD024", "");
        assert_eq!(lines(&output, "MD024").len(), 1, "{heading:?}: {output:?}");
    }
}

#[test]
fn required_headings_rule_ignores_continuation_text() {
    let config = "[MD043]\nheadings = [\"# A\"]\n";
    let output = run("# A\n\n> p\n>     ## Extra\n", "MD043", config);
    assert!(lines(&output, "MD043").is_empty(), "{output:?}");
    let output = run("# A\n\n> p\n>    ## Extra\n", "MD043", config);
    assert_eq!(lines(&output, "MD043"), vec![4], "{output:?}");
}

#[test]
fn fragment_rule_has_no_anchor_for_continuation_text() {
    let output = run("# T\n\n> p\n>     ## B\n\n[x](#b)\n", "MD051", "");
    assert_eq!(lines(&output, "MD051"), vec![6], "{output:?}");
    let output = run("# T\n\n> p\n>    ## B\n\n[x](#b)\n", "MD051", "");
    assert!(lines(&output, "MD051").is_empty(), "{output:?}");
}

/// MDX has no indented code, so the indented line stays a heading there, at the
/// top level and in a quote alike.
#[test]
fn mdx_keeps_the_indented_heading() {
    for src in ["# T\n\np\n    ## B\n\n[x](#b)\n", "# T\n\n> p\n>     ## B\n\n[x](#b)\n"] {
        let output = run_with(src, "MD051", "", &["--flavor", "mdx"]);
        assert!(lines(&output, "MD051").is_empty(), "{src:?}: {output:?}");
        let output = run_with(src, "MD051", "", &["--flavor", "standard"]);
        assert_eq!(lines(&output, "MD051"), vec![6], "{src:?}: {output:?}");
    }
}
