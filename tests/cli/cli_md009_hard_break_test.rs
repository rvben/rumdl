use pulldown_cmark::{Parser, html};
use std::fs;
use std::path::Path;
use std::process::{Command, Output};
use tempfile::tempdir;

const BACKSLASH: &str = "MD009.hard-break-style=\"backslash\"";

fn rumdl(command: &str, file: &Path, config: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .current_dir(file.parent().unwrap())
        .args([command, "--isolated", "--no-cache", "--enable", "MD009"])
        .args(config)
        .arg(file)
        .output()
        .unwrap()
}

fn render(source: &str) -> String {
    let mut output = String::new();
    html::push_html(&mut output, Parser::new(source));
    output
}

#[test]
fn md009_backslash_mode_checks_formats_and_stays_clean() {
    let dir = tempdir().unwrap();
    let file = dir.path().join("doc.md");
    let source = "First line  \nSecond line\n";
    fs::write(&file, source).unwrap();

    let default = rumdl("fmt", &file, &[]);
    assert!(default.status.success(), "{default:?}");
    assert_eq!(fs::read_to_string(&file).unwrap(), source);

    let config = ["--config", BACKSLASH];
    let checked = rumdl("check", &file, &config);
    assert_eq!(checked.status.code(), Some(1), "{checked:?}");
    assert!(String::from_utf8_lossy(&checked.stdout).contains("Use a backslash for this hard line break"));
    let formatted = rumdl("fmt", &file, &config);
    assert!(formatted.status.success(), "{formatted:?}");
    let expected = "First line\\\nSecond line\n";
    assert_eq!(fs::read_to_string(&file).unwrap(), expected);
    let rerun = rumdl("fmt", &file, &config);
    assert!(rerun.status.success(), "{rerun:?}");
    assert_eq!(fs::read_to_string(file).unwrap(), expected);
}

#[test]
fn md009_backslash_mode_preserves_crlf_unicode_and_literal_backslashes() {
    let dir = tempdir().unwrap();
    let file = dir.path().join("doc.md");
    let source = concat!(
        "éclair\u{a0}  \r\n",
        "one\\   \r\n",
        "three\\\\\\  \r\n",
        "four\\\\\\\\  \r\n",
        "tab\t  \r\n",
        "already\\\r\n",
        "last\r\n",
    );
    let expected = concat!(
        "éclair\u{a0}\\\r\n",
        "one\\\\\\\r\n",
        "three\\\\\\\\\\\r\n",
        "four\\\\\\\\\\\r\n",
        "tab\\\r\n",
        "already\\\r\n",
        "last\r\n",
    );
    fs::write(&file, source).unwrap();
    let config = ["--config", BACKSLASH, "--config", "MD009.br-spaces=0"];
    let formatted = rumdl("fmt", &file, &config);
    assert!(formatted.status.success(), "{formatted:?}");
    assert_eq!(fs::read_to_string(file).unwrap(), expected);
    let before = render(source);
    assert_eq!(before.matches("<br").count(), 6);
    assert_eq!(render(expected), before);
}

#[test]
fn md009_backslash_mode_discounts_non_breaks_and_keeps_literal_whitespace() {
    let dir = tempdir().unwrap();
    let file = dir.path().join("doc.md");
    let literal = concat!(
        "`code  \nspan`\n\n",
        "Text <a title=\"long  \ntitle\">link</a>.\n\n",
        "[link](/url \"long  \ntitle\")\n\n",
        "![image](/url \"long  \ntitle\")\n\n",
        "[ref]: /url \"long  \ntitle\"\n\n[ref]\n\n",
        "```text\nraw  \n```\n",
    );
    let source = format!("# Heading  \n\n{literal}\nFinal line  \n");
    let expected = format!("# Heading\n\n{literal}\nFinal line\n");
    fs::write(&file, &source).unwrap();
    let formatted = rumdl("fmt", &file, &["--config", BACKSLASH]);
    assert!(formatted.status.success(), "{formatted:?}");
    assert_eq!(fs::read_to_string(file).unwrap(), expected);
    assert_eq!(render(&expected), render(&source));
}

#[test]
fn md009_backslash_mode_reports_unsupported_mkdocs_without_removing_its_hard_break() {
    let dir = tempdir().unwrap();
    let file = dir.path().join("doc.md");
    let source = "First line  \nSecond line\n";
    fs::write(&file, source).unwrap();
    let config = ["--flavor", "mkdocs", "--config", BACKSLASH];
    let checked = rumdl("check", &file, &config);
    assert_eq!(checked.status.code(), Some(1), "{checked:?}");
    assert!(String::from_utf8_lossy(&checked.stdout).contains("hard-break-style = \"backslash\""));
    let formatted = rumdl("fmt", &file, &config);
    assert!(formatted.status.success(), "{formatted:?}");
    assert_eq!(fs::read_to_string(file).unwrap(), source);
}

#[test]
fn md009_backslash_mode_formats_breaks_in_labels_and_containers() {
    let dir = tempdir().unwrap();
    let file = dir.path().join("doc.md");
    let source = concat!(
        "[soft \nhard  \nlabel](/url \"long  \ntitle\")\n\n",
        "[ref]: /url  \n  \"long  \ntitle\"  \n\n[ref]\n\n",
        "> quote  \n> continued\n\n",
        "- list  \n  continued\n",
    );
    let expected = concat!(
        "[soft\nhard\\\nlabel](/url \"long  \ntitle\")\n\n",
        "[ref]: /url\n  \"long  \ntitle\"\n\n[ref]\n\n",
        "> quote\\\n> continued\n\n",
        "- list\\\n  continued\n",
    );
    fs::write(&file, source).unwrap();
    let formatted = rumdl("fmt", &file, &["--config", BACKSLASH]);
    assert!(formatted.status.success(), "{formatted:?}");
    assert_eq!(fs::read_to_string(file).unwrap(), expected);
    assert_eq!(render(expected), render(source));
}

#[test]
fn md009_backslash_mode_keeps_hard_breaks_that_determine_reference_labels() {
    let dir = tempdir().unwrap();
    let file = dir.path().join("doc.md");
    let references = concat!(
        "[shortcut  \nlink]\n\n",
        "![collapsed  \nimage][]\n\n",
        "[unresolved  \nlabel]\n\n",
        "[other][unresolved  \nlabel]\n\n",
        "[shortcut link]: /shortcut\n",
        "[collapsed image]: /image\n",
        "[unresolved\\ label]: /another\n\n",
    );
    let source = format!("{references}[safe  \nlabel][shortcut link]\n");
    let expected = format!("{references}[safe\\\nlabel][shortcut link]\n");
    fs::write(&file, &source).unwrap();
    let formatted = rumdl("fmt", &file, &["--config", BACKSLASH]);
    assert!(formatted.status.success(), "{formatted:?}");
    let output = fs::read_to_string(file).unwrap();
    assert_eq!(output, expected);
    let rendered = render(&source);
    assert!(rendered.contains("href=\"/shortcut\"") && rendered.contains("src=\"/image\""));
    assert!(!rendered.contains("/another"));
    assert_eq!(render(&output), rendered);
    let diagnostics = String::from_utf8_lossy(&formatted.stdout);
    assert!(
        diagnostics.contains("would change the reference label"),
        "{formatted:?}"
    );
}
