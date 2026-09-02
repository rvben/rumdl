//! Recognition of whole-line GitHub Agentic Workflows control directives.
//!
//! This intentionally recognizes only syntax that gh-aw treats as structural.
//! Template expressions such as `${{ github.ref }}` and directive-looking text
//! embedded in prose remain ordinary Markdown.

/// Return whether `line` is a complete, supported gh-aw control directive.
#[inline]
pub(crate) fn is_control_line(line: &str) -> bool {
    let trimmed = line.trim();
    let Some(inner) = trimmed.strip_prefix("{{").and_then(|value| value.strip_suffix("}}")) else {
        return false;
    };
    let inner = inner.trim_end();

    if matches!(inner, "/if" | "#endif" | "#else" | "else") {
        return true;
    }

    [
        "#if",
        "#elseif",
        "#else-if",
        "#else_if",
        "elseif",
        "else-if",
        "else_if",
        "#runtime-import",
        "#runtime-import?",
        "#import",
    ]
    .into_iter()
    .any(|name| {
        inner.strip_prefix(name).is_some_and(|argument| {
            matches!(argument.as_bytes().first(), Some(b' ' | b'\t'))
                && !argument.trim().is_empty()
                && has_balanced_braces(argument)
        })
    })
}

/// Reject surplus delimiters without excluding balanced GitHub Actions
/// expressions such as `${{ github.event.issue.number }}` inside a condition.
fn has_balanced_braces(value: &str) -> bool {
    let mut depth = 0_u32;
    for byte in value.bytes() {
        match byte {
            b'{' => depth += 1,
            b'}' => {
                let Some(next) = depth.checked_sub(1) else {
                    return false;
                };
                depth = next;
            }
            _ => {}
        }
    }
    depth == 0
}

/// Return whether a link destination is a gh-aw output-template placeholder.
#[inline]
pub(crate) fn is_output_placeholder(value: &str) -> bool {
    let Some(inner) = value.strip_prefix('{').and_then(|value| value.strip_suffix('}')) else {
        return false;
    };

    !inner.is_empty()
        && inner
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'))
}
