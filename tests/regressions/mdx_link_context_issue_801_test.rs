//! MDX rendering semantics must hold through flavor selection and auto-fixes.
use std::fs;
use std::process::Command;
use tempfile::TempDir;

#[test]
fn mdx_links_are_checked_with_automatic_and_explicit_flavors() {
    let temp = TempDir::new().unwrap();
    let content = "<table>\n<tr><td>[click here](/docs) [Empty]()</td></tr>\n</table>\n";
    for (filename, explicit) in [("table.mdx", false), ("table.md", true)] {
        let path = temp.path().join(filename);
        fs::write(&path, content).unwrap();
        let mut command = Command::new(env!("CARGO_BIN_EXE_rumdl"));
        command.args(["check", "--no-config", "--no-cache", "--enable", "MD042,MD059,MD091"]);
        if explicit {
            command.args(["--flavor", "mdx"]);
        }
        let output = command.arg(path).output().unwrap();
        assert_eq!(output.status.code(), Some(1));
        let stdout = String::from_utf8(output.stdout).unwrap();
        assert!(stdout.contains("MD042"), "{stdout}");
        assert!(stdout.contains("MD059"), "{stdout}");
        assert!(!stdout.contains("MD091"), "{stdout}");
    }
}

#[test]
fn link_style_fix_preserves_jsx_javascript_and_titles_and_converges() {
    let temp = TempDir::new().unwrap();
    let path = temp.path().join("table.mdx");
    let content = "<table title=\"[Attribute](leave.md)\">\n<tbody><tr><td>é [Documentation][docs] {'[Literal](leave.md)'}</td></tr></tbody>\n</table>\n\n[docs]: target.md \"\"\n";
    fs::write(&path, content).unwrap();
    let run = || {
        Command::new(env!("CARGO_BIN_EXE_rumdl"))
            .args([
                "check",
                "--fix",
                "--no-config",
                "--no-cache",
                "--enable",
                "MD054",
                "--config",
                "MD054.full = false",
                "--config",
                "MD054.preferred-style = 'inline'",
            ])
            .arg(&path)
            .output()
            .unwrap()
    };
    let first = run();
    assert!(first.status.success(), "{}", String::from_utf8_lossy(&first.stdout));
    let fixed = fs::read_to_string(&path).unwrap();
    assert!(fixed.contains("é [Documentation](target.md \"\")"), "{fixed}");
    assert!(fixed.contains("title=\"[Attribute](leave.md)\""), "{fixed}");
    assert!(fixed.contains("{'[Literal](leave.md)'}"), "{fixed}");
    let second = run();
    assert!(second.status.success());
    assert_eq!(fs::read_to_string(path).unwrap(), fixed);
}
