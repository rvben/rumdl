use std::{fs, process::Command};

#[test]
fn footnote_ordering_works_through_both_cli_fix_paths() {
    for args in [vec!["fmt"], vec!["check", "--fix"]] {
        for (input, expected) in [
            (
                "# Title\n\nHello [^1].\nWorld [^2].\n\n[^2]: Second footnote.\n[^1]: First footnote.\n",
                "# Title\n\nHello [^1].\nWorld [^2].\n\n[^1]: First footnote.\n[^2]: Second footnote.\n",
            ),
            (
                "Refs[^a][^b].\r\n\r\n[^b]: β\r\n\r\n    Second paragraph.\r\n\r\n[^a]: café",
                "Refs[^a][^b].\r\n\r\n[^a]: café\r\n\r\n[^b]: β\r\n\r\n    Second paragraph.",
            ),
        ] {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("doc.md");
            fs::write(&path, input).unwrap();
            for _ in 0..2 {
                let output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
                    .args(&args)
                    .args(["--isolated", "--no-cache", "--enable", "MD067"])
                    .arg(&path)
                    .output()
                    .unwrap();
                assert!(output.status.success(), "{args:?}: {output:?}");
                assert_eq!(fs::read_to_string(&path).unwrap(), expected);
            }
        }
    }
}

#[test]
fn footnote_disable_on_later_definition_is_used_and_preserved() {
    let input = "Refs[^a][^b].\n\n[^b]: B\n<!-- rumdl-disable-next-line MD067 -->\n[^a]: A\n";
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("doc.md");
    fs::write(&path, input).unwrap();
    for args in [vec!["check"], vec!["fmt"], vec!["check", "--fix"]] {
        let output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
            .args(&args)
            .args(["--isolated", "--no-cache", "--enable", "MD067,MD087"])
            .arg(&path)
            .output()
            .unwrap();
        assert!(output.status.success(), "{args:?}: {output:?}");
        assert_eq!(fs::read_to_string(&path).unwrap(), input);
    }
}

#[test]
fn comments_and_frontmatter_do_not_affect_footnote_reference_order() {
    for (name, prefix) in [
        ("doc.mdx", "{/* see [^b] */}\n"),
        ("doc.mdx", "{/*\nsee [^b]\n*/}\n"),
        ("doc.mdx", "---\ntitle: \"[^b]\"\n---\n"),
        ("doc.md", "---\ntitle: \"[^b]\"\n---\n"),
    ] {
        let input = format!("{prefix}Text [^a] [^b].\n\n[^a]: A\n[^b]: B\n");
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(name);
        fs::write(&path, &input).unwrap();
        for args in [vec!["check"], vec!["fmt"], vec!["check", "--fix"]] {
            let output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
                .args(&args)
                .args(["--isolated", "--no-cache", "--enable", "MD067"])
                .arg(&path)
                .output()
                .unwrap();
            assert!(output.status.success(), "{name}, {args:?}: {output:?}");
            assert_eq!(fs::read_to_string(&path).unwrap(), input);
        }
    }
}
