use std::fs;
use std::process::Command;

#[test]
fn test_md060_continuation_survives_cli_with_md056_and_line_endings() {
    for command in ["check", "fmt"] {
        for newline in ["\n", "\r\n"] {
            for final_newline in [false, true] {
                let dir = tempfile::tempdir().unwrap();
                let path = dir.path().join("test.md");
                let content = "| Name | Status |\n|------|--------|\n| Alpha | Done |\nNote: this line must survive";
                let expected =
                    "| Name | Status |\n| ------ | -------- |\n| Alpha | Done |\nNote: this line must survive";
                let ending = if final_newline { newline } else { "" };
                fs::write(&path, format!("{}{ending}", content.replace('\n', newline))).unwrap();
                let expected = format!("{}{ending}", expected.replace('\n', newline));
                for _ in 0..2 {
                    let mut cmd = Command::new(env!("CARGO_BIN_EXE_rumdl"));
                    cmd.current_dir(dir.path())
                        .args([command, "--no-cache", "--enable", "MD056,MD060"]);
                    if command == "check" {
                        cmd.arg("--fix");
                    }
                    let output = cmd.arg("test.md").output().unwrap();
                    assert!(output.status.success(), "CLI failed: {output:?}");
                    assert_eq!(fs::read_to_string(&path).unwrap(), expected);
                }
            }
        }
    }
}
