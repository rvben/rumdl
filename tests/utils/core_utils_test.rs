use rumdl_lib::utils::StrExt;
use rumdl_lib::utils::fast_hash;

#[test]
fn test_trailing_spaces() {
    // No trailing spaces
    assert_eq!("Hello".trailing_spaces(), 0);
    assert_eq!("Hello\n".trailing_spaces(), 0); // \n is not counted as a space
    assert_eq!("".trailing_spaces(), 0);

    // With trailing spaces
    assert_eq!("Hello ".trailing_spaces(), 1);
    assert_eq!("Hello  ".trailing_spaces(), 2);
    assert_eq!("Hello   ".trailing_spaces(), 3);

    // Only spaces
    assert_eq!(" ".trailing_spaces(), 1);
    assert_eq!("  ".trailing_spaces(), 2);
    assert_eq!("   ".trailing_spaces(), 3);

    // Mixed content
    assert_eq!("Hello world ".trailing_spaces(), 1);
    assert_eq!("  Hello world  ".trailing_spaces(), 2);
    assert_eq!("Hello  world   ".trailing_spaces(), 3);

    // With tabs and spaces
    assert_eq!("Hello\t ".trailing_spaces(), 1); // Space after tab is counted
    assert_eq!("Hello \t".trailing_spaces(), 0); // Tab at the end breaks trailing spaces

    // With newlines
    assert_eq!("Hello  \n".trailing_spaces(), 2); // Spaces before \n are counted
    assert_eq!("Hello\n".trailing_spaces(), 0); // Just \n has no trailing spaces
    assert_eq!("Hello \n".trailing_spaces(), 1); // One space before \n
}

#[test]
fn test_replace_trailing_spaces() {
    // No trailing spaces
    assert_eq!("Hello".replace_trailing_spaces(""), "Hello");
    assert_eq!("Hello\n".replace_trailing_spaces(""), "Hello\n"); // \n is preserved
    assert_eq!("".replace_trailing_spaces(""), "");

    // With trailing spaces, replacing with empty string
    assert_eq!("Hello ".replace_trailing_spaces(""), "Hello");
    assert_eq!("Hello  ".replace_trailing_spaces(""), "Hello");
    assert_eq!("Hello   ".replace_trailing_spaces(""), "Hello");

    // With trailing spaces, replacing with custom string
    assert_eq!("Hello ".replace_trailing_spaces("-"), "Hello-");
    assert_eq!("Hello  ".replace_trailing_spaces("--"), "Hello--");
    assert_eq!("Hello   ".replace_trailing_spaces("···"), "Hello···");

    // Only spaces
    assert_eq!(" ".replace_trailing_spaces(""), "");
    assert_eq!("  ".replace_trailing_spaces(""), "");
    assert_eq!("   ".replace_trailing_spaces(""), "");

    // Mixed content
    assert_eq!("Hello world ".replace_trailing_spaces(""), "Hello world");
    assert_eq!("  Hello world  ".replace_trailing_spaces(""), "  Hello world");
    assert_eq!("Hello  world   ".replace_trailing_spaces(""), "Hello  world");

    // With tabs and spaces
    assert_eq!("Hello\t ".replace_trailing_spaces(""), "Hello\t"); // Space after tab is replaced
    assert_eq!("Hello \t".replace_trailing_spaces(""), "Hello \t"); // Tab at the end breaks trailing spaces

    // With newlines
    assert_eq!("Hello  \n".replace_trailing_spaces(""), "Hello\n"); // Spaces before \n are replaced
    assert_eq!("Hello  \n".replace_trailing_spaces("-"), "Hello-\n"); // Spaces before \n are replaced with custom string
    assert_eq!("Hello\n".replace_trailing_spaces(""), "Hello\n"); // Just \n is preserved
}

#[test]
fn test_fast_hash() {
    // Empty string
    let empty_hash = fast_hash("");
    assert_ne!(empty_hash, 0); // Hash should be non-zero

    // Same string produces same hash
    let hash1 = fast_hash("test string");
    let hash2 = fast_hash("test string");
    assert_eq!(hash1, hash2);

    // Different strings produce different hashes
    let hash_a = fast_hash("string a");
    let hash_b = fast_hash("string b");
    assert_ne!(hash_a, hash_b);

    // Case sensitivity
    let hash_lower = fast_hash("test");
    let hash_upper = fast_hash("TEST");
    assert_ne!(hash_lower, hash_upper);

    // Length matters
    let hash_short = fast_hash("test");
    let hash_long = fast_hash("test "); // Extra space
    assert_ne!(hash_short, hash_long);

    // Long strings
    let long_string = "a".repeat(1000);
    let hash_long = fast_hash(&long_string);
    assert_ne!(hash_long, 0);
}

#[test]
fn test_complex_str_ext_usage() {
    // Mixed newlines and spaces
    let text = "Line with trailing spaces   \nAnother line  \nNo trailing spaces\n   Indented line   ";

    // Check each line
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines[0].trailing_spaces(), 3);
    assert_eq!(lines[1].trailing_spaces(), 2);
    assert_eq!(lines[2].trailing_spaces(), 0);
    assert_eq!(lines[3].trailing_spaces(), 3);

    // Replace trailing spaces in each line
    let fixed_lines: Vec<String> = text.lines().map(|line| line.replace_trailing_spaces("")).collect();

    assert_eq!(fixed_lines[0], "Line with trailing spaces");
    assert_eq!(fixed_lines[1], "Another line");
    assert_eq!(fixed_lines[2], "No trailing spaces");
    assert_eq!(fixed_lines[3], "   Indented line");
}

#[test]
fn test_unicode_handling() {
    // Unicode characters with trailing spaces
    let text = "Unicode: 你好, Привет, こんにちは  ";
    assert_eq!(text.trailing_spaces(), 2);
    assert_eq!(text.replace_trailing_spaces(""), "Unicode: 你好, Привет, こんにちは");

    // Hash of unicode strings
    let hash1 = fast_hash("Unicode: 你好");
    let hash2 = fast_hash("Unicode: 您好"); // Slightly different character
    assert_ne!(hash1, hash2);

    // Emoji with trailing spaces
    let emoji_text = "Emoji: 😊 😎 👍  ";
    assert_eq!(emoji_text.trailing_spaces(), 2);
    assert_eq!(emoji_text.replace_trailing_spaces(""), "Emoji: 😊 😎 👍");
}

#[test]
fn test_has_trailing_spaces() {
    // Test cases without trailing spaces
    assert!(!("Hello".has_trailing_spaces()));
    assert!(!("Hello\n".has_trailing_spaces()));
    assert!(!("".has_trailing_spaces()));
    assert!(!("Hello\t".has_trailing_spaces())); // Tab is not a space
    assert!(!("Hello world".has_trailing_spaces()));

    // Test cases with trailing spaces
    assert!("Hello ".has_trailing_spaces());
    assert!("Hello  ".has_trailing_spaces());
    assert!("Hello   ".has_trailing_spaces());
    assert!(" ".has_trailing_spaces());
    assert!("  ".has_trailing_spaces());
    assert!("Hello world ".has_trailing_spaces());
    assert!("Hello\t ".has_trailing_spaces()); // Space after tab
    assert!("Hello  \n".has_trailing_spaces()); // Spaces before newline

    // Edge cases
    assert!(!("\n".has_trailing_spaces())); // Just newline
    assert!(!("\t".has_trailing_spaces())); // Just tab
    assert!(!("\r\n".has_trailing_spaces())); // Windows line ending
    assert!(!("  \r\n".has_trailing_spaces())); // The implementation only strips \n, not \r\n, so \r is seen as non-space
}

#[test]
fn test_has_trailing_spaces_with_mixed_whitespace() {
    // Mixed whitespace patterns
    assert!(!("Hello\t\n".has_trailing_spaces())); // Tab then newline
    assert!("Hello \t ".has_trailing_spaces()); // Space, tab, space
    assert!(!("Hello \t".has_trailing_spaces())); // Space then tab (tab breaks trailing spaces)
    assert!("Hello\t  ".has_trailing_spaces()); // Tab then spaces

    // Multiple lines
    let multiline = "Line 1  \nLine 2\nLine 3 ";
    // Note: has_trailing_spaces works on the whole string, not individual lines
    assert!(multiline.has_trailing_spaces()); // Last line has trailing space

    let multiline_no_trailing = "Line 1  \nLine 2\nLine 3";
    assert!(!multiline_no_trailing.has_trailing_spaces());

    let multiline_with_newline = "Line 1  \nLine 2\nLine 3 \n";
    assert!(multiline_with_newline.has_trailing_spaces()); // Space before final newline
}
