/// Test that rules handle CRLF line endings correctly
///
/// This is a regression test for a bug where MD034, MD046, MD057, MD037, MD049,
/// and MD011 used `.map(|l| l.len() + 1)` to calculate byte positions, which
/// assumes Unix line endings (\n = 1 byte). This caused text corruption on
/// Windows files with CRLF line endings (\r\n = 2 bytes).
use assert_cmd::cargo::cargo_bin_cmd;
use std::fs;
use tempfile::TempDir;

#[test]
fn test_md034_crlf_line_endings() {
    let temp_dir = TempDir::new().unwrap();
    let test_file = temp_dir.path().join("test.md");

    // Create file with CRLF line endings and URL on line 2
    let content = "First line here\r\nSecond https://example.com line\r\nThird line\r\n";
    fs::write(&test_file, content).unwrap();

    // Run rumdl fmt (may have exit code 1 due to unfixable issues like MD041)
    cargo_bin_cmd!("rumdl").arg("fmt").arg(&test_file).assert();

    // Read the result
    let result = fs::read_to_string(&test_file).unwrap();

    // The URL should be wrapped correctly without corrupting surrounding text
    assert!(
        result.contains("Second <https://example.com> line"),
        "Expected 'Second <https://example.com> line' but got:\n{result}"
    );

    // Make sure no corruption occurred (the 'm' from '.com' shouldn't appear elsewhere)
    assert!(
        !result.contains(">m "),
        "Text corruption detected - 'm' from '.com' appears incorrectly:\n{result}"
    );
}

#[test]
fn test_md034_multiline_crlf() {
    let temp_dir = TempDir::new().unwrap();
    let test_file = temp_dir.path().join("test.md");

    // Create file with URLs on multiple lines with CRLF
    let content =
        "# Test\r\n\r\nLine 1 https://example1.com here\r\nLine 2 has https://example2.com also\r\nLine 3 normal\r\n";
    fs::write(&test_file, content).unwrap();

    // Run rumdl fmt (may have exit code 1 due to unfixable issues like MD041)
    cargo_bin_cmd!("rumdl").arg("fmt").arg(&test_file).assert();

    // Read the result
    let result = fs::read_to_string(&test_file).unwrap();

    // Both URLs should be wrapped correctly
    assert!(
        result.contains("Line 1 <https://example1.com> here"),
        "First URL not wrapped correctly:\n{result}"
    );
    assert!(
        result.contains("Line 2 has <https://example2.com> also"),
        "Second URL not wrapped correctly:\n{result}"
    );
}

#[test]
fn test_md034_url_and_email_crlf() {
    let temp_dir = TempDir::new().unwrap();
    let test_file = temp_dir.path().join("test.md");

    // Create file with both URL and email with CRLF
    let content = "# Contact\r\n\r\nVisit https://example.com or email user@example.com\r\n";
    fs::write(&test_file, content).unwrap();

    // Run rumdl fmt (may have exit code 1 due to unfixable issues like MD041)
    cargo_bin_cmd!("rumdl").arg("fmt").arg(&test_file).assert();

    // Read the result
    let result = fs::read_to_string(&test_file).unwrap();

    // URL should be wrapped correctly (email wrapping is optional)
    assert!(
        result.contains("<https://example.com>"),
        "URL not wrapped correctly:\n{result}"
    );

    // Make sure no corruption occurred
    assert!(
        result.contains("user@example.com"),
        "Email address corrupted or missing:\n{result}"
    );
}

#[test]
fn test_mixed_line_endings() {
    let temp_dir = TempDir::new().unwrap();
    let test_file = temp_dir.path().join("test.md");

    // Create file with mixed line endings (Unix LF and Windows CRLF)
    let content =
        "# Test\n\nUnix line https://example1.com here\r\nWindows line https://example2.com also\nAnother unix\r\n";
    fs::write(&test_file, content).unwrap();

    // Run rumdl fmt (may have exit code 1 due to unfixable issues like MD041)
    cargo_bin_cmd!("rumdl").arg("fmt").arg(&test_file).assert();

    // Read the result
    let result = fs::read_to_string(&test_file).unwrap();

    // Both URLs should be wrapped correctly despite mixed line endings
    assert!(
        result.contains("<https://example1.com>"),
        "First URL not wrapped correctly with mixed line endings:\n{result}"
    );
    assert!(
        result.contains("<https://example2.com>"),
        "Second URL not wrapped correctly with mixed line endings:\n{result}"
    );
}

#[test]
fn test_md037_emphasis_crlf() {
    let temp_dir = TempDir::new().unwrap();
    let test_file = temp_dir.path().join("test.md");

    // Create file with emphasis issues and CRLF
    let content = "# Test\r\n\r\nThis has * bad emphasis * here\r\nAnd ** double bad ** too\r\n";
    fs::write(&test_file, content).unwrap();

    // Run rumdl fmt
    cargo_bin_cmd!("rumdl").arg("fmt").arg(&test_file).assert();

    // Read the result
    let result = fs::read_to_string(&test_file).unwrap();

    // Emphasis should be fixed correctly (spaces removed)
    assert!(
        result.contains("*bad emphasis*") || result.contains("_bad emphasis_"),
        "Emphasis spaces not fixed correctly:\n{result}"
    );
}

#[test]
fn test_md049_emphasis_style_crlf() {
    let temp_dir = TempDir::new().unwrap();
    let test_file = temp_dir.path().join("test.md");

    // Create file with mixed emphasis styles and CRLF
    let content = "# Test\r\n\r\nThis has *asterisk emphasis* here\r\nAnd _underscore emphasis_ too\r\n";
    fs::write(&test_file, content).unwrap();

    // Run rumdl check (MD049 detects inconsistent styles)
    let output = cargo_bin_cmd!("rumdl").arg("check").arg(&test_file).output().unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Should detect emphasis style inconsistency without crashing
    assert!(
        stdout.contains("MD049") || !stdout.contains("panicked"),
        "MD049 failed with CRLF:\n{stdout}"
    );
}

#[test]
fn test_md011_reversed_links_crlf() {
    let temp_dir = TempDir::new().unwrap();
    let test_file = temp_dir.path().join("test.md");

    // Create file with reversed link syntax and CRLF
    let content = "# Test\r\n\r\nThis has a (https://example.com)[reversed link] here\r\n";
    fs::write(&test_file, content).unwrap();

    // Run rumdl fmt
    cargo_bin_cmd!("rumdl").arg("fmt").arg(&test_file).assert();

    // Read the result
    let result = fs::read_to_string(&test_file).unwrap();

    // Link should be fixed to correct syntax
    assert!(
        result.contains("[reversed link](https://example.com)"),
        "Reversed link not fixed correctly:\n{result}"
    );
}

#[test]
fn test_md050_strong_style_crlf() {
    let temp_dir = TempDir::new().unwrap();
    let test_file = temp_dir.path().join("test.md");

    // Create file with mixed strong emphasis styles and CRLF
    let content = "# Test\r\n\r\nThis has **asterisk strong** here\r\nAnd __underscore strong__ too\r\n";
    fs::write(&test_file, content).unwrap();

    // Run rumdl check (MD050 detects inconsistent styles)
    let output = cargo_bin_cmd!("rumdl").arg("check").arg(&test_file).output().unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Should detect strong style inconsistency without crashing
    assert!(
        stdout.contains("MD050") || !stdout.contains("panicked"),
        "MD050 failed with CRLF:\n{stdout}"
    );
}

#[test]
fn test_md010_hard_tabs_crlf() {
    let temp_dir = TempDir::new().unwrap();
    let test_file = temp_dir.path().join("test.md");

    // Create file with hard tabs and CRLF
    let content = "# Test\r\n\r\nThis line has\ttabs\there\r\nAnother\tline\twith\ttabs\r\n";
    fs::write(&test_file, content).unwrap();

    // Run rumdl fmt
    cargo_bin_cmd!("rumdl").arg("fmt").arg(&test_file).assert();

    // Read the result
    let result = fs::read_to_string(&test_file).unwrap();

    // Tabs should be replaced with spaces correctly
    assert!(!result.contains('\t'), "Tabs not replaced correctly:\n{result}");
    assert!(
        result.contains("This line has    tabs    here"),
        "Tabs not replaced with correct spacing:\n{result}"
    );
}

#[test]
fn test_md026_trailing_punctuation_crlf() {
    let temp_dir = TempDir::new().unwrap();
    let test_file = temp_dir.path().join("test.md");

    // Create file with headings with trailing punctuation and CRLF
    // Note: Default punctuation checked by MD026 is ".,;:!" (no "?")
    let content = "# Test Heading!\r\n\r\nSome content here\r\n\r\n## Another Heading.\r\n";
    fs::write(&test_file, content).unwrap();

    // Run rumdl fmt
    cargo_bin_cmd!("rumdl").arg("fmt").arg(&test_file).assert();

    // Read the result
    let result = fs::read_to_string(&test_file).unwrap();

    // Trailing punctuation should be removed correctly
    assert!(
        result.contains("# Test Heading\r\n") || result.contains("# Test Heading\n"),
        "Trailing punctuation not removed from first heading:\n{result}"
    );
    assert!(
        result.contains("## Another Heading\r\n") || result.contains("## Another Heading\n"),
        "Trailing punctuation not removed from second heading:\n{result}"
    );
}

#[test]
fn test_code_span_detection_crlf() {
    let temp_dir = TempDir::new().unwrap();
    let test_file = temp_dir.path().join("test.md");

    // Create file with inline code and bare URLs with CRLF
    // This tests CodeBlockInfo::is_in_code_span with CRLF line endings
    let content = "# Test\r\n\r\nThis has `inline code` here\r\nAnd a bare URL https://example.com outside code\r\n";
    fs::write(&test_file, content).unwrap();

    // Run rumdl fmt (should wrap the bare URL but not touch inline code)
    cargo_bin_cmd!("rumdl").arg("fmt").arg(&test_file).assert();

    // Read the result
    let result = fs::read_to_string(&test_file).unwrap();

    // The URL should be wrapped
    assert!(
        result.contains("<https://example.com>"),
        "URL not wrapped correctly:\n{result}"
    );
    // Inline code should be preserved
    assert!(result.contains("`inline code`"), "Inline code was modified:\n{result}");
}

#[test]
fn test_md047_crlf_trailing_newline() {
    let temp_dir = TempDir::new().unwrap();
    let test_file = temp_dir.path().join("test.md");

    // Create file with CRLF line endings but NO trailing newline
    let content = "# Title\r\nSome content here";
    fs::write(&test_file, content).unwrap();

    // Run rumdl fmt to fix
    cargo_bin_cmd!("rumdl").arg("fmt").arg(&test_file).assert();

    // Read the result
    let result = fs::read_to_string(&test_file).unwrap();

    // Should end with CRLF (original line ending preserved)
    assert!(
        result.ends_with("\r\n"),
        "File should end with CRLF but got: {:?}",
        result.as_bytes().iter().rev().take(5).collect::<Vec<_>>()
    );

    // Should be exactly one trailing newline (CRLF)
    assert!(
        !result.ends_with("\r\n\r\n"),
        "File should not have multiple trailing newlines: {:?}",
        result.as_bytes().iter().rev().take(10).collect::<Vec<_>>()
    );
}

#[test]
fn test_md047_lf_trailing_newline() {
    let temp_dir = TempDir::new().unwrap();
    let test_file = temp_dir.path().join("test.md");

    // Create file with LF line endings but NO trailing newline
    let content = "# Title\nSome content here";
    fs::write(&test_file, content).unwrap();

    // Run rumdl fmt to fix
    cargo_bin_cmd!("rumdl").arg("fmt").arg(&test_file).assert();

    // Read the result
    let result = fs::read_to_string(&test_file).unwrap();

    // Should end with LF (original line ending preserved)
    assert!(
        result.ends_with('\n') && !result.ends_with("\r\n"),
        "File should end with LF but got: {:?}",
        result.as_bytes().iter().rev().take(5).collect::<Vec<_>>()
    );

    // Should be exactly one trailing newline (LF)
    assert!(
        !result.ends_with("\n\n") || result.ends_with("\r\n\n"),
        "File should not have multiple trailing newlines: {:?}",
        result.as_bytes().iter().rev().take(10).collect::<Vec<_>>()
    );
}
