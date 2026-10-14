//! Inline directive comments are recognized whatever whitespace follows `<!--`
//! and precedes `-->`: `<!--markdownlint-disable MD013 -->` configures the
//! document exactly as `<!-- markdownlint-disable MD013 -->` does.
//!
//! Every compact form is compared with the spaced form AND with an absolute
//! expectation, so a pair that is wrong the same way cannot pass. Controls: the
//! same document without the directive (the output MUST change), and near-miss
//! comments that are not directives.

use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};
use std::fs;
use std::process::{Command, Output};
use tempfile::tempdir;

const CONFIG: &str =
    "[global]\nenable = [\"MD013\"]\n[MD013]\nline-length = 40\nreflow = true\nreflow-mode = \"normalize\"\n";

const LONG: &str = "This is a deliberately long paragraph that exceeds forty columns and should stay on this single line while the rule is disabled.";

/// Run `rumdl <command> --config <file> --no-cache [extra] doc.md` and return the
/// output with the document as it is afterwards.
fn run(command: &str, extra: &[&str], doc: &str) -> (Output, String) {
    let dir = tempdir().unwrap();
    let config = dir.path().join("rumdl.toml");
    fs::write(&config, CONFIG).unwrap();
    let path = dir.path().join("doc.md");
    fs::write(&path, doc).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .current_dir(dir.path())
        .arg(command)
        .args(["--config", config.to_str().unwrap(), "--no-cache"])
        .args(extra)
        .arg(&path)
        .output()
        .expect("run rumdl");
    // 0 = clean, 1 = findings; anything else is a tool error that would make an
    // "unchanged" assertion meaningless.
    let code = output.status.code();
    assert!(
        matches!(code, Some(0 | 1)),
        "rumdl {command} failed ({code:?}): {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let after = fs::read_to_string(&path).unwrap();
    (output, after)
}

fn fmt(doc: &str) -> String {
    run("fmt", &[], doc).1
}

/// MD013 warnings `check` reports for the document.
fn md013_warnings(doc: &str) -> usize {
    let (output, _) = run("check", &[], doc);
    String::from_utf8_lossy(&output.stdout).matches("[MD013]").count()
}

/// Event kinds of a parse with `Html` bodies blanked, so two documents that
/// differ only in how a comment is spelled have equal skeletons.
fn skeleton(content: &str) -> Vec<String> {
    Parser::new_ext(content, Options::empty())
        .map(|event| match event {
            Event::Html(_) => "html".to_string(),
            Event::InlineHtml(_) => "inline-html".to_string(),
            Event::Start(Tag::Paragraph) => "p".to_string(),
            Event::End(TagEnd::Paragraph) => "/p".to_string(),
            other => format!("{other:?}"),
        })
        .collect()
}

/// Both spellings of a comment, as `(spaced, compact variants)`.
fn spellings(body: &str) -> Vec<String> {
    vec![
        format!("<!-- {body} -->"),
        format!("<!--{body} -->"),
        format!("<!-- {body}-->"),
        format!("<!--{body}-->"),
        format!("<!--  {body}  -->"),
        format!("<!--\t{body}\t-->"),
    ]
}

/// The issue's shape: a disable/enable pair around a long paragraph, and a second
/// long paragraph after the pair that must still be reported.
fn region(open: &str, close: &str) -> String {
    format!("# Example\n\n{open}\n{LONG}\n{close}\n\n{LONG}\n")
}

#[test]
fn issue_repro_fmt_leaves_paragraph_inside_compact_region() {
    let doc = format!("# Example\n\n<!--markdownlint-disable MD013 -->\n{LONG}\n<!--markdownlint-enable MD013 -->\n");
    let (_, after) = run("fmt", &[], &doc);
    assert_eq!(
        after, doc,
        "the paragraph inside the disabled region must not be rewrapped"
    );
}

#[test]
fn fmt_control_without_directive_rewraps() {
    // Positive control: the same paragraph with no directive MUST change.
    let doc = format!("# Example\n\n{LONG}\n");
    assert_ne!(fmt(&doc), doc, "the harness must be able to observe a rewrap");
}

#[test]
fn every_tool_and_spelling_disables_a_block() {
    for tool in ["markdownlint", "rumdl"] {
        for open in spellings(&format!("{tool}-disable MD013")) {
            for close in spellings(&format!("{tool}-enable MD013")) {
                let doc = region(&open, &close);
                // One finding: the paragraph after the region.
                assert_eq!(md013_warnings(&doc), 1, "{doc:?}");
            }
        }
    }
}

#[test]
fn fmt_output_is_identical_for_every_spelling_and_a_fixed_point() {
    let reference = fmt(&region(
        "<!-- markdownlint-disable MD013 -->",
        "<!-- markdownlint-enable MD013 -->",
    ));
    assert!(reference.contains(&format!("\n{LONG}\n<!--")), "{reference:?}");
    for open in spellings("markdownlint-disable MD013") {
        let doc = region(&open, "<!--markdownlint-enable MD013-->");
        let once = fmt(&doc);
        assert_eq!(skeleton(&once), skeleton(&reference), "structure differs for {open:?}");
        // The protected paragraph is untouched, the unprotected one is rewrapped.
        assert!(once.contains(&format!("\n{LONG}\n")), "{once:?}");
        assert_eq!(fmt(&once), once, "fmt is not idempotent for {open:?}");
    }
}

#[test]
fn compact_disable_line_and_next_line_and_file() {
    for tool in ["markdownlint", "rumdl"] {
        for comment in spellings(&format!("{tool}-disable-line MD013")) {
            let doc = format!("# T\n\n{LONG} {comment}\n\n{LONG}\n");
            assert_eq!(md013_warnings(&doc), 1, "{doc:?}");
        }
        for comment in spellings(&format!("{tool}-disable-next-line MD013")) {
            let doc = format!("# T\n\n{comment}\n{LONG}\n\n{LONG}\n");
            assert_eq!(md013_warnings(&doc), 1, "{doc:?}");
        }
        for comment in spellings(&format!("{tool}-disable-file MD013")) {
            let doc = format!("# T\n\n{comment}\n\n{LONG}\n\n{LONG}\n");
            assert_eq!(md013_warnings(&doc), 0, "{doc:?}");
        }
        for comment in spellings(&format!("{tool}-disable-file")) {
            let doc = format!("# T\n\n{comment}\n\n{LONG}\n");
            assert_eq!(md013_warnings(&doc), 0, "{doc:?}");
        }
    }
}

#[test]
fn compact_enable_file_cancels_disable_file() {
    // Spaced disable-file with a compact enable-file: only a recognized compact
    // enable-file brings the rule back.
    let doc = format!("# T\n\n<!-- rumdl-disable-file MD013 -->\n<!--rumdl-enable-file MD013-->\n\n{LONG}\n");
    assert_eq!(md013_warnings(&doc), 1, "{doc:?}");
}

#[test]
fn compact_capture_and_restore() {
    // disable, capture, enable, restore: the restore brings the disable back.
    let doc = format!(
        "# T\n\n<!--markdownlint-disable MD013-->\n<!--markdownlint-capture-->\n<!--markdownlint-enable MD013-->\n{LONG}\n<!--markdownlint-restore-->\n\n{LONG}\n"
    );
    assert_eq!(
        md013_warnings(&doc),
        1,
        "only the paragraph between enable and restore: {doc:?}"
    );
}

#[test]
fn compact_prettier_ignore_disables_the_next_line() {
    for comment in [
        "<!-- prettier-ignore -->",
        "<!--prettier-ignore-->",
        "<!--prettier-ignore -->",
        "<!-- prettier-ignore-->",
    ] {
        let doc = format!("# T\n\n{comment}\n{LONG}\n\n{LONG}\n");
        assert_eq!(md013_warnings(&doc), 1, "{doc:?}");
    }
    // Other prettier directives are not prettier-ignore.
    for comment in ["<!--prettier-ignore-start-->", "<!-- prettier-ignore-end -->"] {
        let doc = format!("# T\n\n{comment}\n{LONG}\n");
        assert_eq!(md013_warnings(&doc), 1, "{doc:?}");
    }
}

#[test]
fn compact_configure_file_one_line_and_multiline() {
    for comment in [
        "<!--markdownlint-configure-file { \"MD013\": { \"line_length\": 200 } }-->",
        "<!--rumdl-configure-file { \"MD013\": { \"line-length\": 200 } } -->",
        "<!--markdownlint-configure-file\n{\n  \"MD013\": { \"line_length\": 200 }\n}\n-->",
        "<!--markdownlint-configure-file { \"MD013\": false }-->",
    ] {
        let doc = format!("# T\n\n{comment}\n\n{LONG}\n");
        assert_eq!(md013_warnings(&doc), 0, "{doc:?}");
    }
}

#[test]
fn compact_directive_inside_blockquote_and_list_item() {
    let quote = format!("> <!--markdownlint-disable-next-line MD013-->\n> {LONG}\n\n{LONG}\n");
    assert_eq!(md013_warnings(&quote), 1, "{quote:?}");
    let list = format!("- item\n\n  <!--markdownlint-disable-next-line MD013-->\n  {LONG}\n\n{LONG}\n");
    assert_eq!(md013_warnings(&list), 1, "{list:?}");
}

#[test]
fn two_directives_with_mixed_spacing_on_one_line() {
    // Either order: the enable that follows the disable on the line must act.
    for line in [
        "<!-- markdownlint-disable MD013 --> <!--markdownlint-enable MD013-->",
        "<!--markdownlint-disable MD013--> <!-- markdownlint-enable MD013 -->",
    ] {
        let doc = format!("# T\n\n{line}\n{LONG}\n");
        assert_eq!(md013_warnings(&doc), 1, "{doc:?}");
    }
}

#[test]
fn near_misses_are_not_directives() {
    for comment in [
        "<!--markdownlint-disablefoo MD013 -->",
        "<!--rumdl-disableMD013-->",
        "<!--RUMDL-disable MD013 -->",
        "<!--Markdownlint-Disable MD013-->",
        "<!--note markdownlint-disable MD013 -->",
        "<!---- markdownlint-disable MD013 -->",
    ] {
        let doc = format!("# T\n\n{comment}\n\n{LONG}\n");
        assert_eq!(md013_warnings(&doc), 1, "{comment:?} must not disable anything");
        assert_ne!(fmt(&doc), doc, "{comment:?} must not protect the paragraph from fmt");
    }
}

#[test]
fn directives_in_code_are_not_directives() {
    let span = format!("# T\n\n`<!--markdownlint-disable MD013-->`\n\n{LONG}\n");
    assert_eq!(md013_warnings(&span), 1, "{span:?}");
    let fenced = format!("# T\n\n```markdown\n<!--markdownlint-disable MD013-->\n```\n\n{LONG}\n");
    assert_eq!(md013_warnings(&fenced), 1, "{fenced:?}");
    let indented = format!("# T\n\n    <!--markdownlint-disable MD013-->\n\n{LONG}\n");
    assert_eq!(md013_warnings(&indented), 1, "{indented:?}");
}

fn md044_findings(doc: &str) -> String {
    let dir = tempdir().unwrap();
    let path = dir.path().join("doc.md");
    fs::write(&path, doc).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .args(["check", "--isolated", "--no-cache", "--enable", "MD044"])
        .args(["--config", "MD044.names=[\"Markdownlint\", \"Rumdl\"]"])
        .arg(&path)
        .output()
        .expect("run rumdl");
    assert!(
        matches!(output.status.code(), Some(0 | 1)),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

#[test]
fn compact_directive_text_is_not_checked_as_prose_by_md044() {
    // MD044 skips directive comment lines so a tool name written in one is not
    // "corrected"; a compact directive line gets the same treatment.
    for line in [
        "<!-- markdownlint-disable MD013 -->",
        "<!--markdownlint-disable MD013 -->",
        "<!--rumdl-disable MD013-->",
        "<!--\trumdl-enable MD013-->",
    ] {
        let stdout = md044_findings(&format!("# T\n\n{line}\n\ntext\n"));
        assert!(!stdout.contains("[MD044]"), "{line:?}: {stdout}");
    }
    // Positive control: the same name in a comment that is not a directive is
    // still checked, so the harness can see a finding.
    for line in ["<!-- note markdownlint here -->", "<!--note markdownlint here-->"] {
        let stdout = md044_findings(&format!("# T\n\n{line}\n\ntext\n"));
        assert!(stdout.contains("[MD044]"), "{line:?} must still be checked: {stdout}");
    }
}
