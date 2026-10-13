use std::fs;
use std::path::Path;
use std::process::{Command, Output};

const CONFIG: &str = "[global]\nenable = [\"MD003\", \"MD022\"]\n\n[MD003]\nstyle = \"setext\"\n";
const CONFIG_MD003_ONLY: &str = "[global]\nenable = [\"MD003\"]\n\n[MD003]\nstyle = \"setext\"\n";

fn run(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .current_dir(dir)
        .args(["check", "--no-cache", "--config", "rumdl.toml"])
        .args(args)
        .arg("test.md")
        .output()
        .unwrap()
}

fn setup(config: &str, content: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("rumdl.toml"), config).unwrap();
    fs::write(dir.path().join("test.md"), content).unwrap();
    dir
}

fn heading_texts(content: &str) -> Vec<String> {
    use pulldown_cmark::{Event, Parser, Tag, TagEnd};
    let mut out = Vec::new();
    let mut current: Option<String> = None;
    for event in Parser::new(content) {
        match event {
            Event::Start(Tag::Heading { .. }) => current = Some(String::new()),
            Event::End(TagEnd::Heading(_)) => out.extend(current.take()),
            Event::Text(t) => {
                if let Some(text) = current.as_mut() {
                    text.push_str(&t);
                }
            }
            _ => {}
        }
    }
    out
}

#[test]
fn md022_makes_paragraph_above_safe_and_the_fix_converges() {
    for newline in ["\n", "\r\n"] {
        let dir = setup(CONFIG, &format!("para{newline}# H{newline}"));
        let path = dir.path().join("test.md");

        let first = run(dir.path(), &["--fix"]);
        assert!(first.status.success(), "first --fix failed: {first:?}");
        let fixed = fs::read_to_string(&path).unwrap();
        assert_eq!(fixed, format!("para{newline}{newline}H{newline}={newline}"));
        assert_eq!(heading_texts(&fixed), vec!["H".to_string()]);

        let second = run(dir.path(), &["--fix"]);
        assert!(second.status.success(), "second --fix failed: {second:?}");
        assert_eq!(fs::read_to_string(&path).unwrap(), fixed, "second run must be a no-op");
    }
}

#[test]
fn without_md022_the_paragraph_case_is_left_unchanged_with_the_warning() {
    for newline in ["\n", "\r\n"] {
        let source = format!("para{newline}# H{newline}");
        let dir = setup(CONFIG_MD003_ONLY, &source);
        let path = dir.path().join("test.md");

        let output = run(dir.path(), &["--fix"]);
        assert!(
            !output.status.success(),
            "the unfixable warning must remain: {output:?}"
        );
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(stdout.contains("MD003"), "expected the MD003 warning, got: {stdout}");
        assert_eq!(fs::read_to_string(&path).unwrap(), source);
        assert_eq!(heading_texts(&source), vec!["H".to_string()]);
    }
}
