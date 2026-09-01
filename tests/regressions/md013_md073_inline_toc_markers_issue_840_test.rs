//! Regression tests for issue #840: MD073 treated marker-shaped text inside
//! inline code as real TOC delimiters. During `rumdl fmt`, its replacement then
//! deleted a heading and joined unrelated lists across that deleted section.

use std::fs;
use std::process::Command;
use tempfile::TempDir;

const REPRODUCTION: &str = r#"# Title

## Table of contents
## Headings, anchors and tables of contents<a id="headings"></a>

**Reasoning:**

- A hand-maintained table of contents drifts out of sync as headings are added, renamed or removed. `rumdl` can generate and validate one from `<!-- toc -->`/`<!-- tocstop -->` markers, including a configurable maximum heading depth, as part of the [linting](#linting) invocation; it is one option, not the only one.[...]

## Linting<a id="linting"></a>

**Reasoning:**

- `MD013` reflow implements the wrapping policy from [Paragraphs and line breaks](#paragraphs-and-line-breaks) instead of duplicating it. Disabling line-length validation would let unwrapped prose pass silently.
- `MD073` is not enabled by default. Combined with `<!-- toc -->`/`<!-- tocstop -->` markers, it implements the automatic table of contents from [Headings, anchors and tables of contents](#headings). It does nothing on a document that does not use those markers, so enabling it is safe even for a document a different generator already owns.

## Generated Markdown<a id="generated"></a>

Content.

## Author information<a id="author-information"></a>

Content.
"#;

fn format_reproduction(content: &str) -> String {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("document.md");
    fs::write(&path, content).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .args([
            "fmt",
            "--no-config",
            "--no-cache",
            "--enable",
            "MD013,MD073",
            "--config",
            "MD013.line-length=80",
            "--config",
            "MD013.reflow=true",
            "--config",
            "MD013.reflow-mode=\"default\"",
        ])
        .arg(&path)
        .output()
        .expect("rumdl fmt should run");

    assert!(
        output.status.success(),
        "rumdl fmt failed\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    fs::read_to_string(path).unwrap()
}

fn rendered_html(markdown: &str) -> String {
    let parser = pulldown_cmark::Parser::new_ext(markdown, pulldown_cmark::Options::empty());
    let mut html = String::new();
    pulldown_cmark::html::push_html(&mut html, parser);
    html.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[test]
fn inline_code_that_names_toc_markers_cannot_open_or_close_a_toc_region() {
    let formatted = format_reproduction(REPRODUCTION);

    assert_eq!(
        rendered_html(&formatted),
        rendered_html(REPRODUCTION),
        "formatting marker-shaped inline code must not change document structure\nformatted:\n{formatted}"
    );
}

#[test]
fn backslash_escaped_marker_text_cannot_open_or_close_a_toc_region() {
    // Documentation that shows the marker syntax as `\<!-- toc -->` renders it as
    // text, so the prose between the two escaped markers is not a TOC region.
    let input = "# Title\n\nWrite these two lines:\n\n\\<!-- toc -->\n\nProse that must survive.\n\n\\<!-- tocstop -->\n\n## Alpha\n";
    let formatted = format_reproduction(input);

    assert!(
        formatted.contains("Prose that must survive."),
        "escaped markers must not delimit a TOC region\nformatted:\n{formatted}"
    );
    assert_eq!(
        rendered_html(&formatted),
        rendered_html(input),
        "formatting escaped marker text must not change document structure\nformatted:\n{formatted}"
    );
}

#[test]
fn genuine_toc_markers_still_generate_a_toc() {
    let input = "# Title\n\n<!-- toc -->\n<!-- tocstop -->\n\n## Real section\n";
    let formatted = format_reproduction(input);

    assert!(
        formatted.contains("- [Real section](#real-section)"),
        "real TOC markers must remain active\nformatted:\n{formatted}"
    );
}

#[test]
fn marker_text_in_multibacktick_code_cannot_end_a_real_toc() {
    let input = "# Title\n\n<!-- toc -->\n- [First](#first)\n\n``use `<!-- tocstop -->` literally``\n<!-- tocstop -->\n\n## First\n\n## Second\n";
    let formatted = format_reproduction(input);

    assert!(
        formatted.contains("- [Second](#second)"),
        "the inline-code marker must not terminate the TOC before its real stop marker\nformatted:\n{formatted}"
    );
    assert!(
        !formatted.contains("``use `<!-- tocstop -->` literally``"),
        "the code-span line is TOC-region content and must be replaced before the real stop marker\nformatted:\n{formatted}"
    );
}

#[test]
fn generated_toc_uses_a_trailing_explicit_html_anchor() {
    let input = "# Title\n\n<!-- toc -->\n<!-- tocstop -->\n\n## Generated Markdown<a id=\"generated\"></a>\n";
    let formatted = format_reproduction(input);

    assert!(
        formatted.contains("- [Generated Markdown](#generated)"),
        "an explicit HTML anchor must override the generated heading slug\nformatted:\n{formatted}"
    );
    assert!(
        !formatted.contains("#generated-markdown"),
        "the generated slug must not be used when an explicit anchor is present\nformatted:\n{formatted}"
    );
}
