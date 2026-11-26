use std::{fs, process::Command};
use tempfile::tempdir;

fn format(extension: &str, rule: &str, options: &[&str], input: &str) -> String {
    let dir = tempdir().unwrap();
    let file = dir.path().join(format!("test.{extension}"));
    fs::write(&file, input).unwrap();
    let mut command = Command::new(env!("CARGO_BIN_EXE_rumdl"));
    command.args(["fmt", "--no-config", "--no-cache", "--enable", rule]);
    for option in options {
        command.args(["-c", option]);
    }
    let result = command.arg(&file).output().unwrap();
    assert!(
        result.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
    fs::read_to_string(file).unwrap()
}

#[test]
fn md013_does_not_reflow_list_continuations_across_jsx() {
    let input = "# Title\n\n- **First**: Placeholder text.\n- **Second**: Placeholder text.\n\n    <Tip>\n    Some tip text here.\n\n    - Item one.\n    - Item two.\n    </Tip>\n";
    let options = [
        "MD013.line-length = 0",
        "MD013.reflow = true",
        "MD013.reflow-mode = 'normalize'",
    ];
    assert_eq!(format("mdx", "MD013", &options, input), input);
    let prose = "- A paragraph\n  on two lines.\n";
    assert_eq!(format("mdx", "MD013", &options, prose), "- A paragraph on two lines.\n");
}

#[test]
fn md030_moves_the_jsx_closer_back_with_its_owner() {
    let input = "*   First item\n\n    <Tip>\n    Some tip text:\n\n    - Nested item\n    </Tip>\n";
    let expected = "* First item\n\n  <Tip>\n  Some tip text:\n\n  - Nested item\n  </Tip>\n";
    assert_eq!(format("mdx", "MD030", &[], input), expected);
}

#[test]
fn md031_exempts_both_fence_boundaries_inside_items() {
    let options = ["MD031.list-items = false"];
    for extension in ["md", "mdx"] {
        let inside = "- Install the SDK:\n  ```bash\n  pip install example\n  ```\n- Next item\n";
        let outside = "- Install the SDK:\n```bash\npip install example\n```\n- Next item\n";
        let spaced = "- Install the SDK:\n\n```bash\npip install example\n```\n\n- Next item\n";
        assert_eq!(format(extension, "MD031", &options, inside), inside);
        assert_eq!(format(extension, "MD031", &options, outside), spaced);

        let nested = "> - Install the SDK:\n>\n>   ```bash\n>   pip install example\n>   ```\n> - Next item\n";
        assert_eq!(format(extension, "MD031", &options, nested), nested);
    }
}

#[test]
fn md046_never_converts_indented_text_or_fences_to_indented_code_in_mdx() {
    let indent =
        "Provide details:\n\n    - The **endpoint ID**:\n\n<Frame>\n  ![Endpoint ID](/images/endpoint.png)\n</Frame>\n";
    let fenced = "Provide details:\n\n```\n- The **endpoint ID**:\n```\n\n<Frame>\n  ![Endpoint ID](/images/endpoint.png)\n</Frame>\n";
    let options = ["MD046.style = 'fenced'"];
    assert_eq!(format("mdx", "MD046", &options, indent), indent);
    assert_eq!(format("md", "MD046", &options, indent), fenced);
    assert_eq!(format("mdx", "MD046", &["MD046.style = 'indented'"], fenced), fenced);
}

#[test]
fn md055_skips_jsx_comment_rows_and_fixes_visible_tables() {
    let input = "# Title\n\n{/*\n| Column | Description |\n| ------ | ----------- |\n| foo | Placeholder |\n| bar | Placeholder | */}\n\n| Column | Description |\n| ------ | ----------- |\n| baz | Visible {/* note */}\n";
    let expected = format!("{} |\n", input.trim_end());
    assert_eq!(
        format("mdx", "MD055", &["MD055.style = 'leading_and_trailing'"], input),
        expected
    );
}

#[test]
fn md058_list_items_option_preserves_tight_lists_and_checks_top_level_tables() {
    let inside = "1. Step:\n   | Team | Visibility |\n   | ---- | ---------- |\n   | Docs | Public     |\n2. Done.\n";
    let outside = "Available teams:\n| Team | Visibility |\n| ---- | ---------- |\n| Docs | Public     |\n";
    let spaced = "Available teams:\n\n| Team | Visibility |\n| ---- | ---------- |\n| Docs | Public     |\n";
    let options = ["MD058.list-items = false"];
    assert_eq!(format("md", "MD058", &options, inside), inside);
    let quoted = "- intro\n  > | a | b |\n  > |---|---|\n- next\n";
    assert_eq!(format("md", "MD058", &options, quoted), quoted);
    assert_ne!(format("md", "MD058", &[], quoted), quoted);
    assert_eq!(format("md", "MD058", &options, outside), spaced);
    assert_eq!(
        format("md", "MD058", &[], inside),
        inside
            .replace("Step:\n", "Step:\n\n")
            .replace("|\n2. Done.", "|\n\n2. Done.")
    );
}

#[test]
fn md088_reports_opted_in_literal_regions_without_fixing_them() {
    let input = concat!(
        "---\r\n",
        "title: \"It’s a “test”\"\r\n",
        "end: trailing\u{a0}\r\n",
        "---\r\n\r\n",
        "It’s prose with `code ’text’`.\r\n\r\n",
        "```text\r\n“fenced”\r\n```\r\n\r\n",
        "    indented ’code’\r\n",
    );
    let expected = input.replace("It’s prose", "It's prose");
    let dir = tempdir().unwrap();
    let file = dir.path().join("scope.md");
    fs::write(&file, input).unwrap();
    let check = |options: &[&str]| {
        let mut command = Command::new(env!("CARGO_BIN_EXE_rumdl"));
        command.args([
            "check",
            "--no-config",
            "--no-cache",
            "--enable",
            "MD088",
            "--output-format",
            "json",
        ]);
        for option in options {
            command.args(["-c", option]);
        }
        let result = command.arg(&file).output().unwrap();
        assert_eq!(
            result.status.code(),
            Some(1),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        serde_json::from_slice::<Vec<serde_json::Value>>(&result.stdout).unwrap()
    };
    let lines = |warnings: &[serde_json::Value]| {
        warnings
            .iter()
            .map(|warning| warning["line"].as_u64().unwrap())
            .collect::<Vec<_>>()
    };
    assert_eq!(lines(&check(&[])), [6]);
    let frontmatter = "MD088.check-frontmatter = true";
    let code = "MD088.code-blocks = true";
    assert_eq!(lines(&check(&[frontmatter])), [2, 2, 2, 6]);
    assert_eq!(lines(&check(&[code])), [6, 6, 6, 9, 9, 12, 12]);
    let options = [frontmatter, code, "MD088.normalize-spaces = true"];
    let opted_in = check(&options);
    assert_eq!(lines(&opted_in), [2, 2, 2, 3, 6, 6, 6, 9, 9, 12, 12]);
    let fixable: Vec<_> = opted_in
        .iter()
        .filter(|warning| warning["fixable"] == true)
        .map(|warning| warning["line"].as_u64().unwrap())
        .collect();
    assert_eq!(fixable, [6]);
    assert_eq!(format("md", "MD088", &options, input), expected);
}
