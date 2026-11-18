//! `html-comments` option of MD057 and MD051: inline links written inside HTML
//! comments are checked only when the option is on, at their exact position,
//! and nothing inside a comment is ever rewritten.

use std::fs;
use std::path::Path;
use std::process::Command;

fn run_with_status(dir: &Path, args: &[&str]) -> (bool, String) {
    let exe = env!("CARGO_BIN_EXE_rumdl");
    let output = Command::new(exe)
        .current_dir(dir)
        .env("RUMDL_CACHE_DIR", dir.join(".rumdl_cache"))
        .args(args)
        .output()
        .expect("failed to execute rumdl");
    let mut combined = String::from_utf8_lossy(&output.stdout).to_string();
    combined.push_str(&String::from_utf8_lossy(&output.stderr));
    (output.status.success(), combined)
}

fn workspace(config: &str, files: &[(&str, &str)]) -> tempfile::TempDir {
    let temp = tempfile::tempdir().unwrap();
    fs::write(temp.path().join(".rumdl.toml"), config).unwrap();
    for (name, body) in files {
        fs::write(temp.path().join(name), body).unwrap();
    }
    temp
}

const MD057_ON: &str = "[MD057]\nhtml-comments = true\n";

fn md057(dir: &Path) -> (bool, String) {
    run_with_status(dir, &["check", "--no-cache", "a.md", "--enable", "MD057"])
}

#[test]
fn md057_option_off_ignores_comment_links() {
    let temp = workspace("", &[("a.md", "# T\n\n<!-- [x](missing.md) -->\n")]);
    let (ok, out) = md057(temp.path());
    assert!(ok, "default must not read comments:\n{out}");
    assert!(!out.contains("MD057"), "{out}");
}

#[test]
fn md057_option_on_reports_comment_link_at_its_destination() {
    let temp = workspace(MD057_ON, &[("a.md", "# T\n\n<!-- [x](missing.md) -->\n")]);
    let (ok, out) = md057(temp.path());
    assert!(!ok, "{out}");
    assert!(
        out.contains("a.md:3:10: [MD057] Relative link 'missing.md' does not exist"),
        "{out}"
    );
}

#[test]
fn md057_option_on_accepts_the_snake_case_spelling() {
    let temp = workspace(
        "[MD057]\nhtml_comments = true\n",
        &[("a.md", "# T\n\n<!-- [x](missing.md) -->\n")],
    );
    let (ok, out) = md057(temp.path());
    assert!(!ok && out.contains("[MD057]"), "{out}");
}

#[test]
fn md057_existing_target_in_a_comment_passes() {
    let temp = workspace(MD057_ON, &[("a.md", "# T\n\n<!-- [x](b.md) -->\n"), ("b.md", "# B\n")]);
    let (ok, out) = md057(temp.path());
    assert!(ok, "{out}");
}

#[test]
fn md057_positions_in_mixed_multiline_unicode_and_wrapped_comments() {
    let src =
        "# T\n\nüñí <!-- [x](m1.md#f) --> after\n\n<!--\nnote\n[x](\nm2.md)\n-->\n\n> <!--\n> [x](m3.md)\n> -->\n";
    let temp = workspace(MD057_ON, &[("a.md", src)]);
    let (ok, out) = md057(temp.path());
    assert!(!ok, "{out}");
    assert!(
        out.contains("a.md:3:14: [MD057] Relative link 'm1.md' does not exist"),
        "{out}"
    );
    assert!(
        out.contains("a.md:8:1: [MD057] Relative link 'm2.md' does not exist"),
        "{out}"
    );
    assert!(
        out.contains("a.md:12:7: [MD057] Relative link 'm3.md' does not exist"),
        "{out}"
    );
}

#[test]
fn md057_crlf_comment_position() {
    let temp = workspace(MD057_ON, &[("a.md", "# T\r\n\r\n<!--\r\n[x](m.md)\r\n-->\r\n")]);
    let (ok, out) = md057(temp.path());
    assert!(!ok, "{out}");
    assert!(out.contains("a.md:4:5: [MD057]"), "{out}");
}

#[test]
fn md057_directive_comments_and_code_are_not_read() {
    let src = "# T\n\n<!-- rumdl-disable MD013 [x](m1.md) -->\n\n<!-- markdownlint-disable-next-line [x](m2.md) -->\n\n`<!-- [x](m3.md) -->`\n\n```\n<!-- [x](m4.md) -->\n```\n\n<!-- [x](m5.md\n";
    let temp = workspace(MD057_ON, &[("a.md", src)]);
    let (ok, out) = md057(temp.path());
    assert!(ok, "{out}");
}

#[test]
fn md057_images_autolinks_and_references_in_comments_are_not_read() {
    let src = "# T\n\n<!-- ![i](m1.png) <m2.md> [r][ref] [[wiki]] -->\n";
    let temp = workspace(MD057_ON, &[("a.md", src)]);
    let (ok, out) = md057(temp.path());
    assert!(ok, "{out}");
}

#[test]
fn md057_comment_links_are_never_rewritten_by_fmt() {
    let src = "# T\n\n<!-- [x](./a.md#t) [y](sub/../a.md) -->\n";
    let temp = workspace(
        "[MD057]\nhtml-comments = true\nself-referential-links = true\ncompact-paths = true\n",
        &[("a.md", src)],
    );
    let (_, out) = run_with_status(
        temp.path(),
        &["check", "--no-cache", "--fix", "a.md", "--enable", "MD057"],
    );
    assert_eq!(fs::read_to_string(temp.path().join("a.md")).unwrap(), src, "{out}");
    let (_, out) = run_with_status(temp.path(), &["fmt", "--no-cache", "a.md", "--enable", "MD057"]);
    assert_eq!(fs::read_to_string(temp.path().join("a.md")).unwrap(), src, "{out}");
}

#[test]
fn md057_absolute_links_in_comments_follow_the_policy() {
    let temp = workspace(
        "[MD057]\nhtml-comments = true\nabsolute-links = \"warn\"\n",
        &[("a.md", "# T\n\n<!-- [x](/abs.md) -->\n")],
    );
    let (_, out) = md057(temp.path());
    assert!(
        out.contains("a.md:3:10: [MD057] Absolute link '/abs.md' cannot be validated locally"),
        "{out}"
    );
}

#[test]
fn md057_the_same_link_outside_a_comment_is_unchanged_by_the_option() {
    let temp = workspace(MD057_ON, &[("a.md", "# T\n\n[x](missing.md)\n")]);
    let (ok, out) = md057(temp.path());
    assert!(!ok);
    assert_eq!(out.matches("[MD057]").count(), 1, "{out}");
}

const MD051_ON: &str = "[MD051]\nhtml-comments = true\n";

fn md051(dir: &Path) -> (bool, String) {
    run_with_status(dir, &["check", "--no-cache", "a.md", "b.md", "--enable", "MD051"])
}

#[test]
fn md051_option_off_ignores_comment_fragments() {
    let temp = workspace(
        "",
        &[
            ("a.md", "# T\n\n<!-- [x](#nope) [y](b.md#nope) -->\n"),
            ("b.md", "# B\n"),
        ],
    );
    let (ok, out) = md051(temp.path());
    assert!(ok, "{out}");
}

#[test]
fn md051_same_document_fragment_in_a_comment_resolves_against_headings() {
    let temp = workspace(
        MD051_ON,
        &[
            ("a.md", "# Title\n\n## Real\n\n<!-- [ok](#real) [bad](#nope) -->\n"),
            ("b.md", "# B\n"),
        ],
    );
    let (ok, out) = md051(temp.path());
    assert!(!ok, "{out}");
    assert!(
        out.contains("a.md:5:24: [MD051] Link anchor '#nope' does not exist in document headings"),
        "{out}"
    );
    assert_eq!(out.matches("[MD051]").count(), 1, "{out}");
}

#[test]
fn md051_a_document_with_only_comment_links_is_still_checked() {
    let temp = workspace(MD051_ON, &[("a.md", "<!-- [bad](#nope) -->\n"), ("b.md", "# B\n")]);
    let (ok, out) = md051(temp.path());
    assert!(!ok, "{out}");
    assert!(out.contains("a.md:1:12: [MD051]"), "{out}");
}

#[test]
fn md051_headings_inside_comments_are_not_anchors() {
    let temp = workspace(
        MD051_ON,
        &[
            ("a.md", "# Title\n\n<!--\n## Hidden\n-->\n\n<!-- [bad](#hidden) -->\n"),
            ("b.md", "# B\n<!--\n## Gone\n-->\n"),
            ("c.md", "<!-- [bad](b.md#gone) -->\n"),
        ],
    );
    let (ok, out) = run_with_status(
        temp.path(),
        &["check", "--no-cache", "a.md", "b.md", "c.md", "--enable", "MD051"],
    );
    assert!(!ok, "{out}");
    assert!(out.contains("a.md:7:12: [MD051] Link anchor '#hidden'"), "{out}");
    assert!(
        out.contains("c.md:1:12: [MD051] Link fragment 'gone' not found in 'b.md'"),
        "{out}"
    );
}

#[test]
fn md051_cross_file_fragment_in_a_comment_reports_once_at_the_destination() {
    let temp = workspace(
        MD051_ON,
        &[
            (
                "a.md",
                "# T\n\ntext <!-- [x](b.md#nope) --> more\n\n<!--\n[y](\nb.md#gone)\n-->\n",
            ),
            ("b.md", "# B\n\n## Real\n"),
        ],
    );
    let (ok, out) = md051(temp.path());
    assert!(!ok, "{out}");
    assert!(
        out.contains("a.md:3:15: [MD051] Link fragment 'nope' not found in 'b.md'"),
        "{out}"
    );
    assert!(
        out.contains("a.md:7:1: [MD051] Link fragment 'gone' not found in 'b.md'"),
        "{out}"
    );
    assert_eq!(out.matches("[MD051]").count(), 2, "{out}");
}

#[test]
fn md051_a_rendered_link_and_a_comment_link_to_the_same_fragment_are_two_reports() {
    let body = "# T\n\n[r](b.md#nope) <!-- [c](b.md#nope) -->\n";
    let files = [("a.md", body), ("b.md", "# B\n")];

    let temp = workspace(MD051_ON, &files);
    let (_, out) = md051(temp.path());
    assert!(
        out.contains("a.md:3:1: [MD051] Link fragment 'nope' not found in 'b.md'"),
        "{out}"
    );
    assert!(
        out.contains("a.md:3:25: [MD051] Link fragment 'nope' not found in 'b.md'"),
        "{out}"
    );
    assert_eq!(out.matches("[MD051]").count(), 2, "{out}");

    let temp = workspace("", &files);
    let (_, out) = md051(temp.path());
    assert!(out.contains("a.md:3:1: [MD051]"), "{out}");
    assert_eq!(out.matches("[MD051]").count(), 1, "{out}");
}

#[test]
fn md051_comment_fragments_that_resolve_pass_and_templates_are_exempt() {
    let temp = workspace(
        MD051_ON,
        &[
            (
                "a.md",
                "# T\n\n<!-- [x](b.md#real) [y](b.md#{{ id }}) [z](#{{ id }}) -->\n",
            ),
            ("b.md", "# B\n\n## Real\n"),
        ],
    );
    let (ok, out) = md051(temp.path());
    assert!(ok, "{out}");
}

#[test]
fn md051_double_hash_fragment_in_a_comment_is_one_report_with_one_hash_removed() {
    let temp = workspace(
        "[MD051]\nhtml-comments = true\n[MD057]\nhtml-comments = true\n",
        &[("a.md", "# T\n\n<!-- [x](b.md##x) -->\n"), ("b.md", "# B\n")],
    );
    let (_, out) = run_with_status(
        temp.path(),
        &["check", "--no-cache", "a.md", "b.md", "--enable", "MD051,MD057"],
    );
    assert_eq!(out.matches("[MD051]").count(), 1, "{out}");
    assert!(out.contains("Link fragment '#x' not found in 'b.md'"), "{out}");
}

#[test]
fn md051_snake_case_option_spelling_is_accepted() {
    let temp = workspace(
        "[MD051]\nhtml_comments = true\n",
        &[("a.md", "# T\n\n<!-- [x](#nope) -->\n"), ("b.md", "# B\n")],
    );
    let (ok, out) = md051(temp.path());
    assert!(!ok && out.contains("[MD051]"), "{out}");
}

#[test]
fn md051_comment_links_are_never_rewritten_by_fmt() {
    let src = "# T\n\n<!-- [x](#nope) [y](b.md#nope) -->\n";
    let temp = workspace(MD051_ON, &[("a.md", src), ("b.md", "# B\n")]);
    let (_, out) = run_with_status(temp.path(), &["fmt", "--no-cache", "a.md", "b.md"]);
    assert_eq!(fs::read_to_string(temp.path().join("a.md")).unwrap(), src, "{out}");
}

/// Output of checking `a.md` from stdin when its only link to `sub/b.md` is
/// inside a comment and `sub/` carries an unreadable config.
fn stdin_output_for_comment_link_to_broken_config(config: &str) -> String {
    let temp = workspace(config, &[("a.md", "# T\n\n<!-- [x](sub/b.md#h) -->\n")]);
    fs::create_dir(temp.path().join("sub")).unwrap();
    fs::write(temp.path().join("sub/b.md"), "# B\n\n## H\n").unwrap();
    fs::write(temp.path().join("sub/.rumdl.toml"), "[[[ not toml\n").unwrap();
    let exe = env!("CARGO_BIN_EXE_rumdl");
    let output = Command::new(exe)
        .current_dir(temp.path())
        .env("RUMDL_CACHE_DIR", temp.path().join(".rumdl_cache"))
        .args(["check", "--stdin", "--stdin-filename", "a.md", "--deny-config-warnings"])
        .stdin(fs::File::open(temp.path().join("a.md")).unwrap())
        .output()
        .unwrap();
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

#[test]
fn md051_stdin_loads_a_comment_link_target_config_only_when_the_option_is_on() {
    let on = stdin_output_for_comment_link_to_broken_config(MD051_ON);
    assert!(on.contains("sub/.rumdl.toml") || on.contains("not toml"), "{on}");

    let off = stdin_output_for_comment_link_to_broken_config("");
    assert!(!off.contains("not toml") && !off.contains("sub/.rumdl.toml"), "{off}");
}

#[test]
fn md051_comment_fragment_with_template_syntax_is_not_an_anchor() {
    let temp = workspace(
        "[MD051]\nhtml-comments = true\n",
        &[(
            "a.md",
            "# T\n\n<!-- [x](<#{{ id }}-suffix>) [y](<#{% id %}-s>) -->\n\n<!-- [z](#nope) -->\n",
        )],
    );
    let (ok, out) = run_with_status(temp.path(), &["check", "--no-cache", "a.md", "--enable", "MD051"]);
    assert!(!ok, "the plain missing anchor must still be reported:\n{out}");
    assert!(out.contains("'#nope'"), "{out}");
    assert!(
        !out.contains("{{") && !out.contains("{%"),
        "template fragments are not checked:\n{out}"
    );
}
