use regex::Regex;
use std::sync::LazyLock;

// Jinja2 template delimiters
static JINJA_EXPRESSION_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\{\{.*?\}\}").expect("Failed to compile Jinja expression regex"));

static JINJA_STATEMENT_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\{%.*?%\}").expect("Failed to compile Jinja statement regex"));

/// Pre-compute all Jinja template ranges in the content
pub fn find_jinja_ranges(content: &str) -> Vec<(usize, usize)> {
    let mut ranges = Vec::new();

    // Collect Jinja expressions {{ ... }}
    for mat in JINJA_EXPRESSION_REGEX.find_iter(content) {
        ranges.push((mat.start(), mat.end()));
    }

    // Collect Jinja statements {% ... %}
    for mat in JINJA_STATEMENT_REGEX.find_iter(content) {
        ranges.push((mat.start(), mat.end()));
    }

    // Sort by start position for efficient binary search later
    ranges.sort_by_key(|r| r.0);
    ranges
}

/// Check if a position is within a Jinja2 template expression or statement
pub fn is_in_jinja_template(content: &str, pos: usize) -> bool {
    // Check Jinja expressions {{ ... }}
    for mat in JINJA_EXPRESSION_REGEX.find_iter(content) {
        if pos >= mat.start() && pos < mat.end() {
            return true;
        }
    }

    // Check Jinja statements {% ... %}
    for mat in JINJA_STATEMENT_REGEX.find_iter(content) {
        if pos >= mat.start() && pos < mat.end() {
            return true;
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_jinja_expression_detection() {
        let content = "Some text {{ variable }} more text";

        // Position before Jinja
        assert!(!is_in_jinja_template(content, 5));

        // Position inside Jinja expression
        assert!(is_in_jinja_template(content, 15));

        // Position after Jinja
        assert!(!is_in_jinja_template(content, 30));
    }

    #[test]
    fn test_jinja_statement_detection() {
        let content = "{% if condition %} text {% endif %}";

        // Inside first statement
        assert!(is_in_jinja_template(content, 5));

        // Between statements
        assert!(!is_in_jinja_template(content, 20));

        // Inside second statement
        assert!(is_in_jinja_template(content, 28));
    }

    #[test]
    fn test_complex_jinja_expression() {
        let content = "{{ pd_read_csv()[index] | filter }}";

        // The entire expression should be detected
        assert!(is_in_jinja_template(content, 10));
        assert!(is_in_jinja_template(content, 20));
    }
}
