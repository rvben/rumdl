use std::fs;
use tempfile::TempDir;

fn fmt(input: &str, opt_in: bool) -> String {
    let temp = TempDir::new().unwrap();
    let path = temp.path().join("math.md");
    fs::write(&path, input).unwrap();
    let mut command = std::process::Command::new(env!("CARGO_BIN_EXE_rumdl"));
    command
        .arg("fmt")
        .arg("--no-config")
        .arg("--no-cache")
        .arg("--enable")
        .arg("MD013")
        .arg("-c")
        .arg("MD013.line-length=0")
        .arg("-c")
        .arg("MD013.reflow=true")
        .arg("-c")
        .arg("MD013.reflow-mode=\"sentence-per-line\"");
    if opt_in {
        command.arg("-c").arg("MD013.bracket-display-math=true");
    }
    let output = command.arg(&path).output().unwrap();
    assert!(
        output.status.success(),
        "fmt failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    fs::read_to_string(path).unwrap()
}

#[test]
fn preserves_tex_comment_and_adjacent_prose() {
    let input = "Before.\n\\[\nx = 1 % keep this comment\ny = 2\n\\]\nAfter.\n";
    let once = fmt(input, true);
    assert_eq!(once, input);
    assert_eq!(fmt(&once, true), once);
    assert_ne!(fmt(input, false), input, "the option must control this behavior");
}

#[test]
fn preserves_single_line_block_between_paragraphs() {
    let input = "Before.\n\\[ E = mc^2 \\]\nAfter.\n";
    assert_eq!(fmt(input, true), input);

    let list_input = "- \\[ E = mc^2 \\]\n  After.\n";
    assert_eq!(fmt(list_input, true), list_input);
}

#[test]
fn preserves_math_inside_list_and_blockquote() {
    for input in [
        "- Explain.\n  \\[\n  x = 1 % keep this comment\n  y = 2\n  \\]\n  Continue.\n",
        "- \\[\n  x = 1 % keep this comment\n  y = 2\n  \\]\n",
        "> Before.\n> \\[\n> x = 1 % keep this comment\n> y = 2\n> \\]\n> After.\n",
        ">> \\[\n>> x = 1 % keep this comment\n>> y = 2\n>> \\]\n",
        "- Item.\n\n  > \\[\n  > x = 1 % keep this comment\n  > y = 2\n  > \\]\n",
    ] {
        let once = fmt(input, true);
        assert_eq!(once, input, "math and its container prefixes must be preserved");
        assert_eq!(fmt(&once, true), once);
    }
}

#[test]
fn unmatched_opener_does_not_swallow_later_prose() {
    let input = "\\[\nFirst sentence. Second sentence.\n";
    let once = fmt(input, true);
    assert!(once.contains("First sentence.\nSecond sentence."), "{once}");
}

#[test]
fn math_blocks_false_exempts_bracket_display_math_from_width_check() {
    let temp = TempDir::new().unwrap();
    let path = temp.path().join("math.md");
    fs::write(&path, "\\[\nx = a + b + c + d + e + f + g\n\\]\n").unwrap();

    let check = |math_blocks: bool| {
        std::process::Command::new(env!("CARGO_BIN_EXE_rumdl"))
            .args(["check", "--no-config", "--no-cache", "--enable", "MD013"])
            .arg("-c")
            .arg("MD013.line-length=20")
            .arg("-c")
            .arg("MD013.bracket-display-math=true")
            .arg("-c")
            .arg(format!("MD013.math-blocks={math_blocks}"))
            .arg(&path)
            .output()
            .unwrap()
    };

    let exempt = check(false);
    assert_eq!(
        exempt.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&exempt.stdout)
    );
    let checked = check(true);
    assert_eq!(
        checked.status.code(),
        Some(1),
        "{}",
        String::from_utf8_lossy(&checked.stdout)
    );
    assert!(String::from_utf8_lossy(&checked.stdout).contains("[MD013]"));
}
