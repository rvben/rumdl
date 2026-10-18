#![cfg(feature = "native")]
//! Protocol contracts are checked against raw original source, not parser text.
use rumdl_lib::custom_rules::{export, source_hash, validate};
use serde_json::{Value, json};
use std::io::Write;
use std::process::{Command, Output, Stdio};

fn payload(source: &str, diagnostics: Value) -> Value {
    json!({"protocol":"rumdl.diagnostics", "protocol_version":1, "source_hash":source_hash(source), "diagnostics":diagnostics})
}

#[test]
fn export_preserves_raw_ranges_and_context_in_nested_code() {
    let source = "---\ntitle: `hidden`\n---\n# Setup café\n\n- Run this:\n\n  ```bash\n  python x.py\n  ```\n\n> ~~~sh\r\n> pip install x\r\n> ~~~\r\n\nUse `` a ` b `` here.\n\n    pytest\n";
    let doc = export("guide.md", source).unwrap();
    assert_eq!(doc.source, source);
    let mut codes = Vec::new();
    for node in &doc.nodes {
        assert!(source.is_char_boundary(node.range[0]));
        assert!(source.is_char_boundary(node.range[1]));
        if matches!(node.kind, "code_span" | "fenced_code" | "indented_code") {
            let text: String = node.content_ranges.iter().map(|r| &source[r[0]..r[1]]).collect();
            codes.push((node.kind, text));
            assert!(!node.heading_path.is_empty());
        }
    }
    assert_eq!(
        codes,
        [
            ("fenced_code", "python x.py\n".into()),
            ("fenced_code", "pip install x\n".into()),
            ("code_span", " a ` b ".into()),
            ("indented_code", "pytest\n".into())
        ]
    );
    let first = doc.nodes.iter().find(|n| n.kind == "fenced_code").unwrap();
    assert_eq!(doc.nodes[first.parent.unwrap()].kind, "list_item");
    assert_eq!(doc.nodes[first.prev_sibling.unwrap()].kind, "paragraph");
    assert_eq!(
        serde_json::to_vec(&doc).unwrap(),
        serde_json::to_vec(&export("guide.md", source).unwrap()).unwrap()
    );
}

#[test]
fn unclosed_fences_and_multibyte_inline_ranges_are_original_bytes() {
    let source = "café 😀 `python run.py`\r\n\r\n~~~bash\r\npython x.py";
    let doc = export("guide.md", source).unwrap();
    let span = doc.nodes.iter().find(|n| n.kind == "code_span").unwrap();
    assert_eq!(&source[span.range[0]..span.range[1]], "`python run.py`");
    assert_eq!(
        &source[span.content_ranges[0][0]..span.content_ranges[0][1]],
        "python run.py"
    );
    let fence = doc.nodes.iter().find(|n| n.kind == "fenced_code").unwrap();
    assert_eq!(fence.range[1], source.len());
}

#[test]
fn validated_diagnostics_use_character_columns_and_reject_bad_ranges_atomically() {
    let source = "first\r\ncafé 😀 python x\r\n";
    let start = source.find("python").unwrap();
    let response = payload(
        source,
        json!([{"rule":"project/mise-command","start_byte":start,"end_byte":start+6,"message":"Use mise"}]),
    );
    let warnings = validate(source, &serde_json::to_vec(&response).unwrap()).unwrap();
    assert_eq!(
        (warnings[0].line, warnings[0].column, warnings[0].end_column),
        (2, 8, 14)
    );
    assert!(warnings[0].fix.is_none());
    for range in [
        [source.find('é').unwrap() + 1, start],
        [start, source.len() + 1],
        [start + 1, start],
    ] {
        let mut response = response.clone();
        response["diagnostics"]
            .as_array_mut()
            .unwrap()
            .push(json!({"rule":"project/test","start_byte":range[0],"end_byte":range[1],"message":"Bad"}));
        assert!(
            validate(source, &serde_json::to_vec(&response).unwrap())
                .unwrap_err()
                .contains("range")
        );
    }
}

#[test]
fn validator_rejects_wrong_versions_hashes_ids_messages_and_budgets() {
    let source = "x";
    let good = payload(
        source,
        json!([{"rule":"project/test","start_byte":0,"end_byte":1,"message":"Use mise"}]),
    );
    for (key, value, expected) in [
        ("protocol_version", json!(2), "protocol"),
        ("protocol", json!("other"), "protocol"),
        ("source_hash", json!("stale"), "hash"),
    ] {
        let mut response = good.clone();
        response[key] = value;
        assert!(
            validate(source, &serde_json::to_vec(&response).unwrap())
                .unwrap_err()
                .contains(expected)
        );
    }
    for rule in ["MD013", "project/MD013", "missing", "project/a/b", "/test", "project/"] {
        let mut response = good.clone();
        response["diagnostics"][0]["rule"] = json!(rule);
        assert!(
            validate(source, &serde_json::to_vec(&response).unwrap())
                .unwrap_err()
                .contains("rule IDs")
        );
    }
    for message in [
        "".to_string(),
        "\x1b[31m".to_string(),
        "\u{202e}spoof".to_string(),
        "x".repeat(4097),
    ] {
        let mut response = good.clone();
        response["diagnostics"][0]["message"] = json!(message);
        assert!(
            validate(source, &serde_json::to_vec(&response).unwrap())
                .unwrap_err()
                .contains("message")
        );
    }
    assert!(validate(source, b"not json").unwrap_err().contains("JSON"));
    assert!(
        validate(source, &vec![b' '; 1024 * 1024 + 1])
            .unwrap_err()
            .contains("1 MiB")
    );
    let many = payload(source, json!(vec![good["diagnostics"][0].clone(); 10_001]));
    assert!(validate(source, &serde_json::to_vec(&many).unwrap()).is_err());
    assert!(
        export("x.md", &"x".repeat(4 * 1024 * 1024 + 1))
            .unwrap_err()
            .contains("4 MiB")
    );
}

fn cli(args: &[&str], input: &[u8]) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(input).unwrap();
    child.wait_with_output().unwrap()
}

#[test]
fn stdin_export_and_report_roundtrip_catch_stale_sources_and_protocol_failures() {
    let source = "café 😀 `python run.py`\r\n";
    let output = cli(
        &[
            "experimental",
            "export-elements",
            "--stdin",
            "--stdin-filename",
            "pr.md",
        ],
        source.as_bytes(),
    );
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["path"], "pr.md");
    assert_eq!(value["source"], source);
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("guide.md");
    std::fs::write(&file, source).unwrap();
    let response = payload(
        source,
        json!([{"rule":"project/test","start_byte":source.find("python").unwrap(),"end_byte":source.find("python").unwrap()+6,"message":"Use mise"}]),
    );
    let args = ["experimental", "report-diagnostics", file.to_str().unwrap()];
    let output = cli(&args, &serde_json::to_vec(&response).unwrap());
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stdout).contains(":1:9:"));
    std::fs::write(&file, "changed").unwrap();
    let output = cli(&args, &serde_json::to_vec(&response).unwrap());
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).contains("hash mismatch"));
    for args in [
        vec!["experimental", "export-elements", "one.md", "two.md"],
        vec![
            "--config",
            "MD013.line-length=80",
            "experimental",
            "export-elements",
            "--stdin",
            "--stdin-filename",
            "pr.md",
        ],
    ] {
        assert_eq!(cli(&args, b"").status.code(), Some(2));
    }
}

#[test]
fn boundary_conversion_agrees_with_context_across_line_endings_and_long_lines() {
    let source = "café 😀\r\nnext\rfinal\n";
    let diagnostics: Vec<_> = source
        .char_indices()
        .map(|(offset, _)| json!({"rule":"project/test","start_byte":offset,"end_byte":offset,"message":"Boundary"}))
        .collect();
    let warnings = validate(
        source,
        &serde_json::to_vec(&payload(source, json!(diagnostics))).unwrap(),
    )
    .unwrap();
    let ctx = rumdl_lib::lint_context::LintContext::new(source, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let expected: Vec<_> = source
        .char_indices()
        .map(|(offset, _)| ctx.offset_to_line_col(offset))
        .collect();
    let actual: Vec<_> = warnings.iter().map(|w| (w.line, w.column)).collect();
    assert_eq!(actual, expected);
    let long = "x".repeat(1_000_000);
    let diagnostics: Vec<_> = (995_000..1_000_000)
        .map(|offset| json!({"rule":"a/b","start_byte":offset,"end_byte":offset+1,"message":"x"}))
        .collect();
    let warnings = validate(&long, &serde_json::to_vec(&payload(&long, json!(diagnostics))).unwrap()).unwrap();
    assert_eq!(warnings.len(), 5_000);
    assert_eq!(warnings[0].column, 995_001);
    assert_eq!(warnings.last().unwrap().end_column, 1_000_001);
}

#[test]
fn headings_in_examples_do_not_leak_outside_their_containers() {
    let source =
        "# Setup\n\n> # Installing mise\n>\n> ```sh\n> python bootstrap.py\n> ```\n\n```bash\npython app.py\n```\n";
    let doc = export("guide.md", source).unwrap();
    let fences: Vec<_> = doc.nodes.iter().filter(|n| n.kind == "fenced_code").collect();
    assert_eq!(fences.len(), 2);
    assert_ne!(fences[0].heading_path, fences[1].heading_path);
    assert_eq!(
        &source[doc.nodes[fences[1].heading_path[0]].range[0]..doc.nodes[fences[1].heading_path[0]].range[1]],
        "# Setup\n"
    );
}

#[test]
fn element_budget_stops_dense_code_documents() {
    assert!(
        export("dense.md", &"`x` ".repeat(100_001))
            .unwrap_err()
            .contains("100000")
    );
}

#[test]
fn configuration_flags_are_rejected_by_the_experimental_adapter() {
    for flag in ["--no-config", "--isolated"] {
        let output = cli(
            &[
                flag,
                "experimental",
                "export-elements",
                "--stdin",
                "--stdin-filename",
                "pr.md",
            ],
            b"",
        );
        assert_eq!(output.status.code(), Some(2));
        assert!(String::from_utf8_lossy(&output.stderr).contains("do not accept configuration flags"));
    }
    let output = cli(
        &[
            "--config",
            "MD013.line-length=80",
            "experimental",
            "export-elements",
            "--stdin",
            "--stdin-filename",
            "pr.md",
        ],
        b"",
    );
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("do not accept configuration flags"));
}
