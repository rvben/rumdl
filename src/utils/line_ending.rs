#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineEnding {
    Lf,
    Crlf,
    Mixed,
}

pub fn detect_line_ending_enum(content: &str) -> LineEnding {
    let has_crlf = content.contains("\r\n");
    // Check if there are LF characters that are NOT part of CRLF
    let content_without_crlf = content.replace("\r\n", "");
    let has_standalone_lf = content_without_crlf.contains('\n');

    match (has_crlf, has_standalone_lf) {
        (true, true) => LineEnding::Mixed, // Has both CRLF and standalone LF
        (true, false) => LineEnding::Crlf, // Only CRLF
        (false, true) => LineEnding::Lf,   // Only LF
        (false, false) => LineEnding::Lf,  // No line endings, default to LF
    }
}

pub fn detect_line_ending(content: &str) -> &'static str {
    // Compatibility function matching the old signature
    let crlf_count = content.matches("\r\n").count();
    let lf_count = content.matches('\n').count() - crlf_count;

    if crlf_count > lf_count { "\r\n" } else { "\n" }
}

pub fn normalize_line_ending(content: &str, target: LineEnding) -> String {
    match target {
        LineEnding::Lf => content.replace("\r\n", "\n"),
        LineEnding::Crlf => {
            // First normalize everything to LF, then convert to CRLF
            let normalized = content.replace("\r\n", "\n");
            normalized.replace('\n', "\r\n")
        }
        LineEnding::Mixed => content.to_string(), // Don't change mixed endings
    }
}

pub fn ensure_consistent_line_endings(original: &str, modified: &str) -> String {
    let original_ending = detect_line_ending_enum(original);

    // For mixed line endings, normalize to the most common one (like detect_line_ending does)
    let target_ending = if original_ending == LineEnding::Mixed {
        // Use the same logic as detect_line_ending: prefer the more common one
        let crlf_count = original.matches("\r\n").count();
        let lf_count = original.matches('\n').count() - crlf_count;
        if crlf_count > lf_count {
            LineEnding::Crlf
        } else {
            LineEnding::Lf
        }
    } else {
        original_ending
    };

    let modified_ending = detect_line_ending_enum(modified);

    if target_ending != modified_ending {
        normalize_line_ending(modified, target_ending)
    } else {
        modified.to_string()
    }
}

pub fn get_line_ending_str(ending: LineEnding) -> &'static str {
    match ending {
        LineEnding::Lf => "\n",
        LineEnding::Crlf => "\r\n",
        LineEnding::Mixed => "\n", // Default to LF for mixed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_line_ending_enum() {
        assert_eq!(detect_line_ending_enum("hello\nworld"), LineEnding::Lf);
        assert_eq!(detect_line_ending_enum("hello\r\nworld"), LineEnding::Crlf);
        assert_eq!(detect_line_ending_enum("hello\r\nworld\nmixed"), LineEnding::Mixed);
        assert_eq!(detect_line_ending_enum("no line endings"), LineEnding::Lf);
    }

    #[test]
    fn test_detect_line_ending() {
        assert_eq!(detect_line_ending("hello\nworld"), "\n");
        assert_eq!(detect_line_ending("hello\r\nworld"), "\r\n");
        assert_eq!(detect_line_ending("hello\r\nworld\nmixed"), "\n"); // More LF than CRLF
        assert_eq!(detect_line_ending("no line endings"), "\n");
    }

    #[test]
    fn test_normalize_line_ending() {
        assert_eq!(normalize_line_ending("hello\r\nworld", LineEnding::Lf), "hello\nworld");
        assert_eq!(
            normalize_line_ending("hello\nworld", LineEnding::Crlf),
            "hello\r\nworld"
        );
        assert_eq!(
            normalize_line_ending("hello\r\nworld\nmixed", LineEnding::Lf),
            "hello\nworld\nmixed"
        );
    }

    #[test]
    fn test_ensure_consistent_line_endings() {
        let original = "hello\r\nworld";
        let modified = "hello\nworld\nextra";
        assert_eq!(
            ensure_consistent_line_endings(original, modified),
            "hello\r\nworld\r\nextra"
        );

        let original = "hello\nworld";
        let modified = "hello\r\nworld\r\nextra";
        assert_eq!(
            ensure_consistent_line_endings(original, modified),
            "hello\nworld\nextra"
        );
    }
}
