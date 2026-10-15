//! MD001, MD024, MD025 and MD043 treat a heading inside a blockquote as a heading
//! of the document, like a heading inside a list item, against the real binary.

use std::fs;
use std::process::{Command, Output};
use tempfile::tempdir;

/// Runs `rumdl check` on `content` with only `rule` enabled and `config` appended
/// to the config file. Returns the output and the document afterwards.
fn run(content: &str, rule: &str, config: &str, fix: bool) -> (Output, String) {
    let dir = tempdir().unwrap();
    let path = dir.path().join("doc.md");
    let config_path = dir.path().join("rumdl.toml");
    fs::write(&path, content).unwrap();
    fs::write(&config_path, format!("[global]\nenable = [\"{rule}\"]\n\n{config}\n")).unwrap();
    let mut command = Command::new(env!("CARGO_BIN_EXE_rumdl"));
    command.args(["check", "--no-cache", "--config"]).arg(&config_path);
    if fix {
        command.arg("--fix");
    }
    let output = command.arg(&path).output().expect("run rumdl check");
    (output, fs::read_to_string(&path).unwrap())
}

fn lines(output: &Output, rule: &str) -> Vec<usize> {
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|line| line.contains(&format!("[{rule}]")))
        .filter_map(|line| line.split(':').nth(1)?.trim().parse().ok())
        .collect()
}

mod md024 {
    use super::*;

    #[test]
    fn quoted_duplicate_is_reported() {
        for (src, line) in [
            ("# Same\n\n> # Same\n", 3),
            ("> # Same\n\n> # Same\n", 3),
            ("# Same\n\n> > # Same\n", 3),
            ("# Same\n\n- x\n\n  > # Same\n", 5),
            ("# Same\n\n> - # Same\n", 3),
        ] {
            let (output, _) = run(src, "MD024", "", false);
            assert_eq!(lines(&output, "MD024"), vec![line], "{src:?}: {output:?}");
        }
    }

    #[test]
    fn distinct_quoted_headings_are_clean() {
        let (output, _) = run("# A\n\n> # B\n", "MD024", "", false);
        assert!(lines(&output, "MD024").is_empty(), "{output:?}");
    }

    #[test]
    fn fix_changes_nothing() {
        let src = "# Same\n\n> # Same\n";
        let (_, fixed) = run(src, "MD024", "", true);
        assert_eq!(fixed, src);
    }
}

mod md043 {
    use super::*;

    const CONFIG: &str = "[MD043]\nheadings = [\"# A\", \"## B\"]\n";

    #[test]
    fn unexpected_quoted_heading_is_reported() {
        for (src, line) in [
            ("# A\n\n## B\n\n> ## Extra\n", 5),
            ("# A\n\n## B\n\n> > ## Extra\n", 5),
            ("# A\n\n## B\n\n- x\n\n  > ## Extra\n", 7),
            ("# A\n\n## B\n\n> - ## Extra\n", 5),
        ] {
            let (output, _) = run(src, "MD043", CONFIG, false);
            assert_eq!(lines(&output, "MD043"), vec![line], "{src:?}: {output:?}");
        }
    }

    #[test]
    fn matching_quoted_heading_is_clean() {
        let (output, _) = run("# A\n\n> ## B\n", "MD043", CONFIG, false);
        assert!(lines(&output, "MD043").is_empty(), "{output:?}");
    }

    #[test]
    fn quote_only_document_is_checked() {
        let (output, _) = run("> ## Wrong\n", "MD043", CONFIG, false);
        assert!(!lines(&output, "MD043").is_empty(), "{output:?}");
    }
}
