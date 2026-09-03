//! Regression coverage for issue #846: a tagged fenced block between ordered
//! list items must not lose its fence, language, or internal indentation.

use assert_cmd::cargo::cargo_bin_cmd;

const TAGGED_BLOCK_BETWEEN_STEPS: &str = r#"1. Configure the server:

```json
{
  "mcpServers": {
    "phrase": {
      "command": "npx"
    }
  }
}
```

2. Continue setup.
"#;

const UNTAGGED_BLOCK_BETWEEN_STEPS: &str = r#"1. Configure the server:

```
{
  "mcpServers": {}
}
```

2. Continue setup.
"#;

const MIXED_BLOCK_STYLES: &str = r#"    first indented block

Text between blocks.

    second indented block

1. Configure the server:

```json
{
  "mcpServers": {}
}
```

2. Continue setup.
"#;

const MIXED_UNTAGGED_BLOCK_STYLES: &str = r#"    first indented block

Text between blocks.

    second indented block

1. Configure the server:

```
{
  "mcpServers": {}
}
```

2. Continue setup.
"#;

const LOSSLESS_CONSISTENT_OUTPUT: &str = r#"```
first indented block
```

Text between blocks.

```
second indented block
```

1. Configure the server:

```json
{
  "mcpServers": {}
}
```

2. Continue setup.
"#;

const LOSSLESS_UNTAGGED_CONSISTENT_OUTPUT: &str = r#"```
first indented block
```

Text between blocks.

```
second indented block
```

1. Configure the server:

```
{
  "mcpServers": {}
}
```

2. Continue setup.
"#;

fn assert_fmt(input: &str, expected: &str, config: Option<&str>) {
    let mut command = cargo_bin_cmd!("rumdl");
    command.args(["fmt", "--stdin", "--silent", "--no-config", "--enable", "MD046,MD077"]);
    if let Some(config) = config {
        command.args(["--config", config]);
    }

    let output = command.write_stdin(input).output().expect("run rumdl fmt");
    assert!(
        output.status.success(),
        "rumdl fmt failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8(output.stdout).unwrap(), expected);
}

#[test]
fn fmt_preserves_tagged_fence_when_indented_style_is_explicit() {
    assert_fmt(
        TAGGED_BLOCK_BETWEEN_STEPS,
        TAGGED_BLOCK_BETWEEN_STEPS,
        Some(r#"MD046.style = "indented""#),
    );
}

#[test]
fn fmt_preserves_fence_when_indented_conversion_would_stop_being_code() {
    assert_fmt(
        UNTAGGED_BLOCK_BETWEEN_STEPS,
        UNTAGGED_BLOCK_BETWEEN_STEPS,
        Some(r#"MD046.style = "indented""#),
    );
}

#[test]
fn fmt_keeps_only_the_structurally_unsafe_fence() {
    let input = "```\nsafe\n```\n\n1. First step.\n\n```\nunsafe\n```\n\n2. Next step.\n";
    let expected = "    safe\n\n1. First step.\n\n```\nunsafe\n```\n\n2. Next step.\n";

    assert_fmt(input, expected, Some(r#"MD046.style = "indented""#));
}

#[test]
fn fmt_keeps_only_a_fence_that_interrupts_paragraph_text() {
    let input = "```\nsafe\n```\n\nParagraph\n```\nunsafe\n```\n";
    let expected = "    safe\n\nParagraph\n```\nunsafe\n```\n";

    assert_fmt(input, expected, Some(r#"MD046.style = "indented""#));
}

#[test]
fn fmt_keeps_only_the_fence_needed_to_separate_code_blocks() {
    let input = "```\nsafe\n```\n\nText between.\n\n```\nboundary\n```\n\n    neighbor\n";
    let expected = "    safe\n\nText between.\n\n```\nboundary\n```\n\n    neighbor\n";

    assert_fmt(input, expected, Some(r#"MD046.style = "indented""#));
}

#[test]
fn fmt_preserves_code_indentation_when_converting_an_indented_fence() {
    let expected = "    root\n      child\n";

    for indent in ["", " ", "  ", "   "] {
        let input = format!("{indent}```\n{indent}root\n{indent}  child\n{indent}```\n");
        assert_fmt(&input, expected, Some(r#"MD046.style = "indented""#));
    }
}

#[test]
fn fmt_treats_an_over_indented_fence_as_code_content() {
    let input = "```\na\n    ```\nb\n```\n";
    let expected = "    a\n        ```\n    b\n";

    assert_fmt(input, expected, Some(r#"MD046.style = "indented""#));
}

#[test]
fn fmt_preserves_fences_whose_payload_starts_or_ends_with_a_blank_line() {
    let input = "```\n\nleading\n```\n\nText.\n\n```\ntrailing\n\n```\n";

    assert_fmt(input, input, Some(r#"MD046.style = "indented""#));
}

#[test]
fn fmt_keeps_only_an_empty_fenced_block() {
    let input = "```\n```\n\nText.\n\n```\nfoo\n```\n";
    let expected = "```\n```\n\nText.\n\n    foo\n";

    assert_fmt(input, expected, Some(r#"MD046.style = "indented""#));
}

#[test]
fn fmt_converts_a_fence_relative_to_its_nested_list_container() {
    let input = "  - nested\n\n    ```\n    foo\n    ```\n";
    let expected = "  - nested\n\n        foo\n";

    assert_fmt(input, expected, Some(r#"MD046.style = "indented""#));
}

#[test]
fn fmt_converts_tab_indented_list_fences_without_changing_code_content() {
    let expected = "1. item\n\n       foo\n";

    for input in ["1. item\n\n\t```\n    foo\n\t```\n", "1. item\n\n\t```\n\tfoo\n\t```\n"] {
        assert_fmt(input, expected, Some(r#"MD046.style = "indented""#));
    }
    assert_fmt(expected, expected, Some(r#"MD046.style = "indented""#));
}

#[test]
fn fmt_preserves_a_literal_leading_tab_in_fenced_code() {
    let input = "```\n\tfoo\n```\n";
    let expected = "    \tfoo\n";

    assert_fmt(input, expected, Some(r#"MD046.style = "indented""#));
}

#[test]
fn fmt_preserves_fences_that_open_on_a_list_marker_line() {
    let inputs = [
        "- ```\n  foo\n  ```\n",
        "- ```json\n  {}\n  ```\n",
        "1. ```\n   foo\n   ```\n",
        "1. ```json\n   {}\n   ```\n",
    ];

    for input in inputs {
        assert_fmt(input, input, Some(r#"MD046.style = "indented""#));
    }
}

#[test]
fn fmt_consistent_preserves_list_marker_line_fences() {
    let untagged = "\tfirst\n\nText.\n\n\tsecond\n\n- ```\n  foo\n  ```\n";
    let untagged_expected = "```\nfirst\n```\n\nText.\n\n```\nsecond\n```\n\n- ```\n  foo\n  ```\n";
    let tagged = "\tfirst\n\nText.\n\n\tsecond\n\n1. ```json\n   {}\n   ```\n";
    let tagged_expected = "```\nfirst\n```\n\nText.\n\n```\nsecond\n```\n\n1. ```json\n   {}\n   ```\n";

    assert_fmt(untagged, untagged_expected, None);
    assert_fmt(untagged_expected, untagged_expected, None);
    assert_fmt(tagged, tagged_expected, None);
    assert_fmt(tagged_expected, tagged_expected, None);
}

#[test]
fn fmt_consistent_strips_tab_code_prefixes_when_falling_back_to_fences() {
    let input = "\tfirst\n\nText.\n\n\tsecond\n\n1. step\n\n```\nunsafe\n```\n\n2. next\n";
    let expected = "```\nfirst\n```\n\nText.\n\n```\nsecond\n```\n\n1. step\n\n```\nunsafe\n```\n\n2. next\n";

    assert_fmt(input, expected, None);
    assert_fmt(expected, expected, None);
}

#[test]
fn fmt_converts_an_unclosed_untagged_fence_without_adding_a_fence() {
    let input = "```\nfoo\n";
    let expected = "    foo\n";

    assert_fmt(input, expected, Some(r#"MD046.style = "indented""#));
}

#[test]
fn fmt_consistent_uses_fences_when_indented_conversion_would_lose_metadata() {
    assert_fmt(MIXED_BLOCK_STYLES, LOSSLESS_CONSISTENT_OUTPUT, None);
}

#[test]
fn fmt_consistent_ignores_tagged_fences_in_excluded_containers() {
    let input = "    first\n\nText between.\n\n    second\n\n[^note]:\n    ```json\n    {}\n    ```\n";

    assert_fmt(input, input, None);
}

#[test]
fn fmt_consistent_ignores_unsupported_fences_in_excluded_containers() {
    let input = "    one\n\nText A.\n\n    two\n\nText B.\n\n    three\n\nText C.\n\n```\nfour\n```\n\n[^note]:\n    ```\n    ignored\n    ```\n";
    let expected = "    one\n\nText A.\n\n    two\n\nText B.\n\n    three\n\nText C.\n\n    four\n\n[^note]:\n    ```\n    ignored\n    ```\n";

    assert_fmt(input, expected, None);
}

#[test]
fn fmt_consistent_uses_fences_when_indented_conversion_would_change_structure() {
    assert_fmt(MIXED_UNTAGGED_BLOCK_STYLES, LOSSLESS_UNTAGGED_CONSISTENT_OUTPUT, None);
}
