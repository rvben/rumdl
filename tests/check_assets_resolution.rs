//! `check-assets` mention boundaries and flavor-aware reference extraction,
//! exercised through the real binary with exact JSON comparisons.
use pulldown_cmark::{Event, Parser, Tag};
use serde_json::{Value, json};
use std::fs;
use std::path::Path;
use std::process::{Command, Output};

fn put(root: &Path, path: &str, content: impl AsRef<[u8]>) {
    let path = root.join(path);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, content).unwrap();
}

/// `docs` is both the Markdown source and the asset directory ("assets stored
/// next to Markdown"), reported as JSON.
fn scan(root: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .current_dir(root)
        .args(["check-assets", "--root", ".", "--source", "docs", "--assets", "docs"])
        .args(["--output-format", "json"])
        .args(args)
        .output()
        .unwrap()
}

fn json_report(output: &Output, code: i32) -> Value {
    assert_eq!(
        output.status.code(),
        Some(code),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

fn lists(value: &Value) -> (Value, Value) {
    (value["unreferenced"].clone(), value["mentioned_but_unresolved"].clone())
}

fn assets(root: &Path, names: &[&str]) {
    for name in names {
        put(root, &format!("docs/{name}"), "image");
    }
}

// ---------------------------------------------------------------------------
// Mention boundaries
// ---------------------------------------------------------------------------

#[test]
fn a_longer_filename_does_not_mention_a_shorter_asset() {
    let dir = tempfile::tempdir().unwrap();
    assets(dir.path(), &["logo.png", "readme_logo.png"]);
    put(dir.path(), "docs/index.md", "<img src=\"readme_logo.png\">\n");
    let value = json_report(&scan(dir.path(), &[]), 1);
    assert_eq!(lists(&value), (json!(["docs/logo.png"]), json!([])));
}

#[test]
fn mention_boundaries_table() {
    // (source text, does it mention an asset named logo.png)
    let cases: &[(&str, bool)] = &[
        ("logo.png", true),
        ("\"logo.png\"", true),
        ("'logo.png'", true),
        ("(logo.png)", true),
        ("`logo.png`", true),
        ("{logo.png}", true),
        ("{{< figure src=\"logo.png\" >}}", true),
        ("src=logo.png", true),
        ("see logo.png.", true),
        ("see logo.png...", true),
        ("logo.png, then", true),
        ("logo.png?v=1", true),
        ("logo.png#top", true),
        ("a/logo.png", true),
        ("a\\logo.png", true),
        ("a%2Flogo.png", true),
        ("a%2flogo.png", true),
        ("a%5Clogo.png", true),
        ("logo.png%3Fv=1", true),
        ("the logo.png file", true),
        ("「logo.png」", true),
        ("_logo.png_", true),
        ("__logo.png__", true),
        ("*logo.png*", true),
        ("my__logo.png", false),
        ("logo.png__x", false),
        ("readme_logo.png", false),
        ("my-logo.png", false),
        ("Xlogo.png", false),
        ("2logo.png", false),
        ("élogo.png", false),
        ("foo.logo.png", false),
        ("logo.png.bak", false),
        ("logo.png.gz", false),
        ("logo.pngx", false),
        ("logo.png-2", false),
        ("logo.png_old", false),
        ("logo.pngé", false),
        ("logo.png%20copy", false),
        ("my%20logo.png", false),
        ("logo@2x.png", false),
    ];
    for (text, mentioned) in cases {
        let dir = tempfile::tempdir().unwrap();
        assets(dir.path(), &["logo.png"]);
        put(dir.path(), "docs/index.md", format!("text {text} text\n"));
        let code = if *mentioned { 0 } else { 1 };
        let value = json_report(&scan(dir.path(), &[]), code);
        let expected = if *mentioned {
            (json!([]), json!(["docs/logo.png"]))
        } else {
            (json!(["docs/logo.png"]), json!([]))
        };
        assert_eq!(lists(&value), expected, "source text {text:?}");
    }
}

#[test]
fn a_rejected_hit_does_not_hide_a_later_standalone_mention() {
    let dir = tempfile::tempdir().unwrap();
    assets(dir.path(), &["logo.png", "other.png"]);
    put(
        dir.path(),
        "docs/a.md",
        "readme_logo.png and later {{ \"logo.png\" }}\n",
    );
    put(dir.path(), "docs/b.md", "my-logo.png\n");
    let value = json_report(&scan(dir.path(), &[]), 1);
    assert_eq!(lists(&value), (json!(["docs/other.png"]), json!(["docs/logo.png"])));
}

#[test]
fn encoded_spellings_follow_the_same_boundaries() {
    let dir = tempfile::tempdir().unwrap();
    assets(dir.path(), &["my logo.png", "unused.png"]);
    put(
        dir.path(),
        "docs/index.md",
        "{{ img \"my%20logo.png\" }} and notmy%20logo.png\n",
    );
    let value = json_report(&scan(dir.path(), &[]), 1);
    assert_eq!(lists(&value), (json!(["docs/unused.png"]), json!(["docs/my logo.png"])));

    let dir = tempfile::tempdir().unwrap();
    assets(dir.path(), &["my logo.png"]);
    put(dir.path(), "docs/index.md", "{{ img \"notmy%20logo.png\" }}\n");
    let value = json_report(&scan(dir.path(), &[]), 1);
    assert_eq!(lists(&value), (json!(["docs/my logo.png"]), json!([])));
}

// ---------------------------------------------------------------------------
// Flavor-aware extraction
// ---------------------------------------------------------------------------

const ADMONITION: &str = "<img src=\"readme_logo.png\">\n\n![](plain.png)\n\n!!! tip \"x\"\n\n    ![](./adm.png)\n";

fn minimal_fixture(root: &Path) {
    assets(root, &["logo.png", "readme_logo.png", "adm.png", "plain.png"]);
    put(root, "docs/index.md", ADMONITION);
}

#[test]
fn the_oracle_agrees_the_admonition_image_is_not_commonmark() {
    let images = |text: &str| {
        Parser::new(text)
            .filter(|event| matches!(event, Event::Start(Tag::Image { .. })))
            .count()
    };
    // Indented code to CommonMark: this is why the standard flavor cannot resolve it.
    assert_eq!(images(ADMONITION), 1, "only the unindented image is an image");
    // The container body once dedented is an image: this is what MkDocs renders.
    assert_eq!(images("![](./adm.png)\n"), 1);
}

#[test]
fn mkdocs_flavor_resolves_images_in_admonitions_and_exposes_the_unused_asset() {
    let dir = tempfile::tempdir().unwrap();
    minimal_fixture(dir.path());
    let value = json_report(&scan(dir.path(), &["--flavor", "mkdocs"]), 1);
    assert_eq!(lists(&value), (json!(["docs/logo.png"]), json!([])));
    assert_eq!(value["assets_considered"], 4);
}

#[test]
fn standard_flavor_keeps_an_indented_code_image_unresolved() {
    let dir = tempfile::tempdir().unwrap();
    minimal_fixture(dir.path());
    let value = json_report(&scan(dir.path(), &["--flavor", "standard"]), 1);
    assert_eq!(lists(&value), (json!(["docs/logo.png"]), json!(["docs/adm.png"])));

    // Without any flavor the same applies: a real indented code block is code.
    let dir = tempfile::tempdir().unwrap();
    assets(dir.path(), &["code.png", "unused.png"]);
    put(dir.path(), "docs/index.md", "text\n\n    ![](./code.png)\n");
    let value = json_report(&scan(dir.path(), &["--flavor", "mkdocs"]), 1);
    assert_eq!(
        lists(&value),
        (json!(["docs/unused.png"]), json!(["docs/code.png"])),
        "indented code outside a container is code in every flavor"
    );
}

#[test]
fn scans_are_idempotent_and_never_write() {
    let dir = tempfile::tempdir().unwrap();
    minimal_fixture(dir.path());
    let snapshot = |root: &Path| {
        let mut entries: Vec<_> = walk(root);
        entries.sort();
        entries
    };
    fn walk(path: &Path) -> Vec<(String, Vec<u8>)> {
        let mut out = Vec::new();
        for entry in fs::read_dir(path).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                out.extend(walk(&path));
            } else {
                out.push((path.display().to_string(), fs::read(&path).unwrap()));
            }
        }
        out
    }
    let before = snapshot(dir.path());
    let first = scan(dir.path(), &["--flavor", "mkdocs"]);
    let second = scan(dir.path(), &["--flavor", "mkdocs"]);
    assert_eq!(first.stdout, second.stdout);
    assert_eq!(before, snapshot(dir.path()));
}

#[test]
fn mkdocs_containers_resolve_every_reference_form() {
    let dir = tempfile::tempdir().unwrap();
    assets(
        dir.path(),
        &[
            "tab.png",
            "collapsible.png",
            "nested.png",
            "inline-html.png",
            "refimg.png",
            "multiline.png",
            "under-opener.png",
            "a&b.png",
            "paren(1).png",
            "wrapped.png",
            "code8.png",
            "unused.png",
        ],
    );
    put(
        dir.path(),
        "docs/page.md",
        r#"=== "Tab"

    ![](tab.png)

??? note "Fold"

    ![](collapsible.png)

!!! warning

    !!! tip

        ![](nested.png)

!!! note

    <img src="inline-html.png">

!!! note

    ![Example][ref]

    [ref]: refimg.png

!!! note

    ![Example][multi]

    [multi]:
        multiline.png

!!! note
    [opener]: under-opener.png

!!! note

    ![](a&amp;b.png)
    [esc]: paren\(1\).png

<div markdown="1">
    ![](wrapped.png)
</div>

!!! note

        ![](code8.png)
"#,
    );
    let value = json_report(&scan(dir.path(), &["--flavor", "mkdocs"]), 1);
    assert_eq!(lists(&value), (json!(["docs/unused.png"]), json!(["docs/code8.png"])));
}

#[test]
fn front_matter_text_is_never_a_container_or_definition_source() {
    let dir = tempfile::tempdir().unwrap();
    assets(dir.path(), &["fm.png", "unused.png"]);
    put(
        dir.path(),
        "docs/page.md",
        "---\nexample: |\n  [asset]: ./fm.png\n---\n# Page\n",
    );
    let value = json_report(&scan(dir.path(), &["--flavor", "mkdocs"]), 1);
    assert_eq!(lists(&value), (json!(["docs/unused.png"]), json!(["docs/fm.png"])));
}

#[test]
fn myst_and_pandoc_containers_resolve_with_their_flavor() {
    let dir = tempfile::tempdir().unwrap();
    assets(
        dir.path(),
        &["myst.png", "pandoc.png", "backtick-directive.png", "unused.png"],
    );
    put(
        dir.path(),
        "docs/myst.md",
        ":::{note}\n![](myst.png)\n:::\n\n```{note}\n![](backtick-directive.png)\n```\n",
    );
    put(dir.path(), "docs/pandoc.md", "::: {.callout}\n![](pandoc.png)\n:::\n");
    let value = json_report(&scan(dir.path(), &["--flavor", "myst"]), 1);
    // The backtick directive body is a fenced code block to the parser (and to MD057), so it is a
    // literal mention only. Everything else resolves.
    assert_eq!(
        lists(&value),
        (json!(["docs/unused.png"]), json!(["docs/backtick-directive.png"]))
    );
}

#[test]
fn mdx_components_resolve_children_references_and_definitions() {
    let dir = tempfile::tempdir().unwrap();
    assets(
        dir.path(),
        &["mdx-child.png", "mdx-ref.png", "mdx-unused-def.png", "unused.png"],
    );
    put(
        dir.path(),
        "docs/page.mdx",
        "<Tabs>\n![](mdx-child.png)\n</Tabs>\n\n<Tabs>\n\n    ![logo][asset]\n\n    [asset]: ./mdx-ref.png\n    [spare]: ./mdx-unused-def.png\n\n</Tabs>\n",
    );
    let value = json_report(&scan(dir.path(), &["--isolated"]), 1);
    assert_eq!(lists(&value), (json!(["docs/unused.png"]), json!([])));
}

#[test]
fn entities_are_decoded_exactly_once() {
    let dir = tempfile::tempdir().unwrap();
    assets(dir.path(), &["a&b.png", "a&amp;b.png"]);
    put(
        dir.path(),
        "docs/page.md",
        "<img src=\"a&amp;b.png\">\n\n![](a&amp;amp;b.png)\n",
    );
    let value = json_report(&scan(dir.path(), &["--flavor", "mkdocs"]), 0);
    assert_eq!(lists(&value), (json!([]), json!([])));

    let dir = tempfile::tempdir().unwrap();
    assets(dir.path(), &["a&b.png", "a&amp;b.png"]);
    put(dir.path(), "docs/page.md", "!!! note\n\n    ![](a&amp;amp;b.png)\n");
    let value = json_report(&scan(dir.path(), &["--flavor", "mkdocs"]), 1);
    assert_eq!(lists(&value), (json!(["docs/a&b.png"]), json!([])));
}

// ---------------------------------------------------------------------------
// Configuration: the same resolution as `rumdl check`
// ---------------------------------------------------------------------------

fn project(root: &Path) {
    fs::create_dir_all(root.join(".git")).unwrap();
    minimal_fixture(root);
}

const RESOLVED: (&str, &str) = ("docs/logo.png", "docs/adm.png");

fn unresolved_admonition(value: &Value) -> bool {
    value["mentioned_but_unresolved"] == json!([RESOLVED.1])
}

#[test]
fn a_discovered_config_flavor_is_used() {
    let dir = tempfile::tempdir().unwrap();
    project(dir.path());
    put(dir.path(), ".rumdl.toml", "[global]\nflavor = \"mkdocs\"\n");
    let value = json_report(&scan(dir.path(), &[]), 1);
    assert_eq!(lists(&value), (json!([RESOLVED.0]), json!([])));
}

#[test]
fn isolated_and_no_config_ignore_a_discovered_config() {
    for flag in ["--isolated", "--no-config"] {
        let dir = tempfile::tempdir().unwrap();
        project(dir.path());
        put(dir.path(), ".rumdl.toml", "[global]\nflavor = \"mkdocs\"\n");
        let value = json_report(&scan(dir.path(), &[flag]), 1);
        assert!(unresolved_admonition(&value), "{flag}: {value}");
    }
}

#[test]
fn an_explicit_config_is_used_instead_of_discovery() {
    let dir = tempfile::tempdir().unwrap();
    project(dir.path());
    put(dir.path(), ".rumdl.toml", "[global]\nflavor = \"standard\"\n");
    put(dir.path(), "other.toml", "[global]\nflavor = \"mkdocs\"\n");
    let value = json_report(&scan(dir.path(), &["--config", "other.toml"]), 1);
    assert_eq!(lists(&value), (json!([RESOLVED.0]), json!([])));
    let output = scan(dir.path(), &["--config", "other.toml", "--no-config"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
}

#[test]
fn inline_config_overrides_apply() {
    let dir = tempfile::tempdir().unwrap();
    project(dir.path());
    let value = json_report(&scan(dir.path(), &["--config", "flavor = 'mkdocs'"]), 1);
    assert_eq!(lists(&value), (json!([RESOLVED.0]), json!([])));
}

#[test]
fn per_file_flavor_selects_by_path_and_beats_the_flavor_flag() {
    let dir = tempfile::tempdir().unwrap();
    project(dir.path());
    put(dir.path(), "docs/other/adm2.png", "image");
    put(dir.path(), "docs/other/page.md", "!!! tip\n\n    ![](adm2.png)\n");
    put(
        dir.path(),
        ".rumdl.toml",
        "[per-file-flavor]\n\"docs/other/**\" = \"mkdocs\"\n",
    );
    let value = json_report(&scan(dir.path(), &[]), 1);
    // docs/other is MkDocs; docs/index.md stays standard.
    assert_eq!(lists(&value), (json!([RESOLVED.0]), json!([RESOLVED.1])));

    // --flavor applies to files per-file-flavor does not match, and does not override a match.
    let value = json_report(&scan(dir.path(), &["--flavor", "standard"]), 1);
    assert_eq!(lists(&value), (json!([RESOLVED.0]), json!([RESOLVED.1])));
    let value = json_report(&scan(dir.path(), &["--flavor", "mkdocs"]), 1);
    assert_eq!(lists(&value), (json!([RESOLVED.0]), json!([])));
}

#[test]
fn flavor_flag_overrides_the_global_config_flavor() {
    let dir = tempfile::tempdir().unwrap();
    project(dir.path());
    put(dir.path(), ".rumdl.toml", "[global]\nflavor = \"mkdocs\"\n");
    let value = json_report(&scan(dir.path(), &["--flavor", "standard"]), 1);
    assert!(unresolved_admonition(&value));
}

#[test]
fn a_nested_directory_config_governs_its_files() {
    let dir = tempfile::tempdir().unwrap();
    project(dir.path());
    put(dir.path(), "docs/sub/adm2.png", "image");
    put(dir.path(), "docs/sub/.rumdl.toml", "[global]\nflavor = \"mkdocs\"\n");
    put(dir.path(), "docs/sub/page.md", "!!! tip\n\n    ![](adm2.png)\n");
    let value = json_report(&scan(dir.path(), &["--include", "**/*.png"]), 1);
    assert_eq!(lists(&value), (json!([RESOLVED.0]), json!([RESOLVED.1])));
}

#[test]
fn isolated_also_ignores_nested_directory_configs() {
    for flag in ["--isolated", "--no-config"] {
        let dir = tempfile::tempdir().unwrap();
        project(dir.path());
        put(dir.path(), "docs/sub/adm2.png", "image");
        put(dir.path(), "docs/sub/.rumdl.toml", "[global]\nflavor = \"mkdocs\"\n");
        put(dir.path(), "docs/sub/page.md", "!!! tip\n\n    ![](adm2.png)\n");
        let value = json_report(&scan(dir.path(), &["--include", "**/*.png", flag]), 1);
        assert_eq!(
            lists(&value),
            (json!([RESOLVED.0]), json!([RESOLVED.1, "docs/sub/adm2.png"])),
            "{flag}"
        );
    }
}

#[test]
fn extension_defaults_apply_without_any_config() {
    let dir = tempfile::tempdir().unwrap();
    assets(dir.path(), &["adm.png", "unused.png"]);
    put(dir.path(), "docs/page.mdx", "<Tabs>\n![](adm.png)\n</Tabs>\n");
    let value = json_report(&scan(dir.path(), &["--isolated"]), 1);
    assert_eq!(lists(&value), (json!(["docs/unused.png"]), json!([])));
}

#[test]
fn unreadable_configs_are_tool_errors_with_no_findings() {
    let dir = tempfile::tempdir().unwrap();
    project(dir.path());
    put(dir.path(), ".rumdl.toml", "[global\nflavor = ");
    let output = scan(dir.path(), &[]);
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());

    // A nested config that cannot be loaded would silently change the flavor of its files.
    let dir = tempfile::tempdir().unwrap();
    project(dir.path());
    put(dir.path(), "docs/sub/.rumdl.toml", "[global\nflavor = ");
    put(dir.path(), "docs/sub/page.md", "text\n");
    let output = scan(dir.path(), &[]);
    assert_eq!(
        output.status.code(),
        Some(2),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).contains("sub"));
}
