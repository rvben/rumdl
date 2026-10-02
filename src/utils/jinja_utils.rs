use regex::Regex;
use std::sync::LazyLock;

// Preserve recognition of later, complete tags after malformed literal markup.
static JINJA_EXPRESSION_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\{\{.*?\}\}").expect("Failed to compile Jinja expression regex"));
static JINJA_STATEMENT_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\{%.*?%\}").expect("Failed to compile Jinja statement regex"));

/// Pre-compute complete Jinja expression and statement ranges.
/// Delimiters inside quoted strings or dictionary literals do not close a tag.
pub fn find_jinja_ranges(content: &str) -> Vec<(usize, usize)> {
    let bytes = content.as_bytes();
    let mut ranges = Vec::new();
    let mut cursor = 0;

    while let Some(offset) = content[cursor..].find('{') {
        let start = cursor + offset;
        let closing = match bytes.get(start + 1) {
            Some(b'{') => b'}',
            Some(b'%') => b'%',
            _ => {
                cursor = start + 1;
                continue;
            }
        };
        cursor = start + 2;
        let mut quote = None;
        let mut escaped = false;
        let mut braces = 0usize;
        let mut closed = false;
        while cursor < bytes.len() {
            let byte = bytes[cursor];
            if let Some(delimiter) = quote {
                if escaped {
                    escaped = false;
                } else if byte == b'\\' {
                    escaped = true;
                } else if byte == delimiter {
                    quote = None;
                }
            } else if braces == 0 && byte == closing && bytes.get(cursor + 1) == Some(&b'}') {
                cursor += 2;
                ranges.push((start, cursor));
                closed = true;
                break;
            } else {
                match byte {
                    b'\'' | b'"' => quote = Some(byte),
                    b'{' => braces += 1,
                    b'}' => braces = braces.saturating_sub(1),
                    _ => {}
                }
            }
            cursor += 1;
        }
        if !closed {
            // An unclosed tag may be ordinary Markdown containing braces or an
            // apostrophe. Recover the previous single-line matches in this tail
            // once, rather than rescanning it for every unmatched opening brace.
            for regex in [&*JINJA_EXPRESSION_REGEX, &*JINJA_STATEMENT_REGEX] {
                ranges.extend(
                    regex
                        .find_iter(&content[start..])
                        .map(|mat| (start + mat.start(), start + mat.end())),
                );
            }
            ranges.sort_unstable();
            break;
        }
    }

    // Complete tags are sorted and nonoverlapping; malformed tails retain the
    // legacy matches used by callers before the quote-aware scan.
    ranges
}

/// Quoted values inside recognized Jinja expressions and statements.
/// Leave other template dialects sharing braces to their existing context checks.
pub(crate) fn find_jinja_string_ranges(content: &str, templates: &[(usize, usize)]) -> Vec<(usize, usize)> {
    let bytes = content.as_bytes();
    let mut strings = Vec::new();
    for &(start, end) in templates {
        let body = &content[start + 2..end - 2];
        if content[start..].starts_with("{{") {
            if body.trim_start().starts_with(['#', '/', '<', '>', '%']) {
                continue;
            }
        } else if start > 0 && bytes[start - 1] == b'{' {
            // The inner `{%` of a Hugo `{{% ... %}}` shortcode is not Jinja.
            continue;
        }

        let mut quote = None;
        let mut string_start = 0;
        let mut escaped = false;
        for (offset, &byte) in bytes[start + 2..end - 2].iter().enumerate() {
            let pos = start + 2 + offset;
            if let Some(delimiter) = quote {
                if escaped {
                    escaped = false;
                } else if byte == b'\\' {
                    escaped = true;
                } else if byte == delimiter {
                    strings.push((string_start, pos));
                    quote = None;
                }
            } else if matches!(byte, b'\'' | b'"') {
                quote = Some(byte);
                string_start = pos + 1;
            }
        }
    }

    // Shared template ranges can overlap (a quoted expression inside a
    // statement). Merge duplicates and overlaps for point queries by binary search.
    strings.sort_unstable();
    let mut merged: Vec<(usize, usize)> = Vec::with_capacity(strings.len());
    for (start, end) in strings {
        if let Some(last) = merged.last_mut()
            && start <= last.1
        {
            last.1 = last.1.max(end);
        } else {
            merged.push((start, end));
        }
    }
    merged
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

    #[test]
    fn test_jinja_ranges_respect_strings_newlines_and_dictionary_braces() {
        for template in [
            "{{\n\"hidden@example.com\"\n}}",
            "{% set target =\r\n\"https://example.com\"\r\n%}",
            "{{ \"https://example.com/{{value}}\" }}",
            "{% set target = 'hidden@example.com %} later' %}",
            "{{ \"escaped \\\" }} hidden@example.com\" }}",
            "{{{\"key\":\"hidden@example.com\"}}}",
        ] {
            let content = format!("日本語 Before {template} after");
            let start = content.find(template).unwrap();
            assert_eq!(
                find_jinja_ranges(&content),
                vec![(start, start + template.len())],
                "the complete template must be recognized: {content}"
            );
        }
    }

    #[test]
    fn test_jinja_ranges_recover_after_unclosed_literal_markup() {
        for prefix in [
            "{{ not closed\n",
            "{{ prose \" ordinary text\n",
            "{{ apostrophe's prose }}\n",
        ] {
            let content = format!("{prefix}\n{{{{ \"hidden@example.com\" }}}}");
            let start = content.rfind("{{").unwrap();
            assert!(find_jinja_ranges(&content).contains(&(start, content.len())));
            assert!(
                find_jinja_string_ranges(&content, &find_jinja_ranges(&content))
                    .iter()
                    .any(|&(start, end)| &content[start..end] == "hidden@example.com")
            );
        }
    }
}
