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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_jinja_ranges_expressions() {
        let content = "Some text {{ variable }} more text";
        let ranges = find_jinja_ranges(content);
        assert_eq!(ranges.len(), 1);
        assert_eq!(&content[ranges[0].0..ranges[0].1], "{{ variable }}");
    }

    #[test]
    fn test_find_jinja_ranges_statements() {
        let content = "{% if condition %} text {% endif %}";
        let ranges = find_jinja_ranges(content);
        assert_eq!(ranges.len(), 2);
        assert_eq!(&content[ranges[0].0..ranges[0].1], "{% if condition %}");
        assert_eq!(&content[ranges[1].0..ranges[1].1], "{% endif %}");
    }

    #[test]
    fn test_find_jinja_ranges_complex_expression() {
        let content = "{{ pd_read_csv()[index] | filter }}";
        let ranges = find_jinja_ranges(content);
        assert_eq!(ranges.len(), 1);
        assert_eq!(ranges[0], (0, content.len()));
    }

    #[test]
    fn test_find_jinja_ranges_sorted() {
        let content = "{% if x %} foo {{ bar }} baz {% endif %}";
        let ranges = find_jinja_ranges(content);
        assert_eq!(ranges.len(), 3);
        assert!(ranges[0].0 < ranges[1].0);
        assert!(ranges[1].0 < ranges[2].0);
    }
}
