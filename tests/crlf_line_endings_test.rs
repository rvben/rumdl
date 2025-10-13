/// Test that rules handle CRLF line endings correctly
///
/// This is a regression test for a bug where MD034, MD046, and MD057 used
/// `.map(|l| l.len() + 1)` to calculate byte positions, which assumes Unix
/// line endings (\n = 1 byte). This caused text corruption on Windows files
/// with CRLF line endings (\r\n = 2 bytes).
use assert_cmd::Command;
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
    Command::cargo_bin("rumdl").unwrap().arg("fmt").arg(&test_file).assert();

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
    Command::cargo_bin("rumdl").unwrap().arg("fmt").arg(&test_file).assert();

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
    Command::cargo_bin("rumdl").unwrap().arg("fmt").arg(&test_file).assert();

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
    Command::cargo_bin("rumdl").unwrap().arg("fmt").arg(&test_file).assert();

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
