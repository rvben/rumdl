//! `ignore-link-urls` forgives a line-length violation; it does not change how
//! reflow measures a line. Reflow therefore rewraps a paragraph whose only
//! over-length cause is a link URL regardless of the setting (issue #749).
//! `stern` and `code-spans` are equally absent from reflow, so this is the scope
//! of a reporting option rather than anything special about `ignore-link-urls`.
//! `strict` is the one exception, pinned below.

use std::fs;
use std::process::Command;
use tempfile::tempdir;

fn rumdl_bin() -> &'static str {
    env!("CARGO_BIN_EXE_rumdl")
}

const LONG_LINK_LINE: &str = "This is a piece of [text](https://example.com/lol/alskjdhflkajshdfljkahsdfljkhasdkjfhasdkjfhasdjklfhlkajsdhfkjlashdflkjashdflkjahdskfjlhasdjfha) that should be on a single line\n";

fn run_check(config: &str) -> (String, String) {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join(".rumdl.toml"), config).unwrap();
    fs::write(dir.path().join("test.md"), LONG_LINK_LINE).unwrap();

    let output = Command::new(rumdl_bin())
        .current_dir(dir.path())
        .args(["check", "--no-cache", "test.md"])
        .output()
        .unwrap();
    (
        String::from_utf8_lossy(&output.stdout).to_string(),
        String::from_utf8_lossy(&output.stderr).to_string(),
    )
}

fn md013_findings(config: &str) -> Vec<String> {
    run_check(config)
        .0
        .lines()
        .filter(|line| line.contains("[MD013]"))
        .map(str::to_string)
        .collect()
}

#[test]
fn ignore_link_urls_forgives_the_line_when_reflow_is_off() {
    // Control for the test below: the option does exactly what it documents when
    // reflow is not involved.
    assert!(
        md013_findings("[MD013]\nignore-link-urls = true\n").is_empty(),
        "URL-only overflow should be forgiven"
    );
    assert_eq!(
        md013_findings("[MD013]\nignore-link-urls = false\n").len(),
        1,
        "counting URLs should flag the line"
    );
}

#[test]
fn ignore_link_urls_does_not_change_what_reflow_does() {
    // Both values produce the same result once reflow is on. This is the behavior
    // #749 asked to change; it is pinned so the answer stays deliberate.
    assert_eq!(
        md013_findings("[MD013]\nreflow = true\nignore-link-urls = true\n"),
        md013_findings("[MD013]\nreflow = true\nignore-link-urls = false\n"),
        "ignore-link-urls must not alter reflow's result"
    );
}

/// docs/md013.md claims `stern` and `code-spans` leave reflow alone while
/// `strict` does not. Pin both halves: a doc claim that drifts from the code is
/// worse than no claim.
#[test]
fn strict_affects_reflow_while_the_other_reporting_options_do_not() {
    // A paragraph whose only over-length line is a standalone link. Non-strict
    // mode exempts such a line, so reflow reports nothing.
    let paragraph = format!(
        "Some intro words here that are short.\n{}More trailing words here.\n",
        {
            let l = LONG_LINK_LINE;
            let start = l.find("[text]").unwrap();
            let end = l.find(") that").unwrap() + 1;
            format!("{}\n", &l[start..end])
        }
    );

    let dir = tempdir().unwrap();
    fs::write(dir.path().join("test.md"), &paragraph).unwrap();
    let count = |config: &str| -> usize {
        fs::write(dir.path().join(".rumdl.toml"), config).unwrap();
        let output = Command::new(rumdl_bin())
            .current_dir(dir.path())
            .args(["check", "--no-cache", "test.md"])
            .output()
            .unwrap();
        String::from_utf8_lossy(&output.stdout)
            .lines()
            .filter(|line| line.contains("[MD013]"))
            .count()
    };

    assert_eq!(
        count("[MD013]\nreflow = true\n"),
        0,
        "baseline: the link line is exempt"
    );
    assert_eq!(
        count("[MD013]\nreflow = true\nstrict = true\n"),
        1,
        "strict removes the standalone-link exemption inside reflow"
    );
    for neutral in ["stern = true", "code-spans = false", "ignore-link-urls = false"] {
        assert_eq!(
            count(&format!("[MD013]\nreflow = true\n{neutral}\n")),
            0,
            "`{neutral}` must not change what reflow reports"
        );
    }
}

#[test]
fn setting_ignore_link_urls_with_reflow_is_not_a_config_error() {
    // The option has a scope, like `stern` and `code-spans`, which reflow also does
    // not consult. Scope is documented, not warned about: a warning here would fire
    // on a perfectly coherent config and single out one option among several.
    for config in [
        "[MD013]\nreflow = true\nignore-link-urls = true\n",
        "[MD013]\nreflow = true\nignore-link-urls = false\n",
    ] {
        let (_, stderr) = run_check(config);
        assert!(
            !stderr.contains("[config warning]"),
            "no config warning expected for {config:?}, got:\n{stderr}"
        );
    }
}
