use std::fmt;

/// A literal closer makes the rendered roles of this fence and later markers
/// unknowable without evaluating the template. Keep earlier independent fences
/// available, but do not infer or fix fence structure from this opener onward.
pub(crate) fn fence_ambiguity_limit(ctx: &crate::lint_context::LintContext) -> Option<usize> {
    let lines = ctx.raw_lines();
    ctx.code_block_details.iter().find_map(|detail| {
        if !detail.is_fenced || ctx.is_inside_template_code(detail.start) || ctx.is_inside_mdx_code(detail.start) {
            return None;
        }
        let start_line = ctx
            .line_offsets
            .partition_point(|&off| off <= detail.start)
            .saturating_sub(1);
        let end_line = ctx
            .line_offsets
            .partition_point(|&off| off <= detail.end.saturating_sub(1))
            .saturating_sub(1);
        if start_line == end_line {
            return None;
        }
        let opener = lines.get(start_line)?.as_bytes();
        let open_start = opener.iter().position(|&b| b == b'`' || b == b'~')?;
        let fence_char = opener[open_start];
        let open_len = opener[open_start..].iter().take_while(|&&b| b == fence_char).count();
        let closer = lines.get(end_line)?;
        let close_start = closer.bytes().position(|b| b == b'`' || b == b'~')?;
        let close_len = closer.as_bytes()[close_start..]
            .iter()
            .take_while(|&&b| b == fence_char)
            .count();
        // A cached unclosed block also ends at EOF. Only a matching bare fence
        // can establish this ambiguity, rather than arbitrary final body text.
        if open_len < 3
            || close_len < open_len
            || !closer[..close_start].chars().all(|c| c.is_whitespace() || c == '>')
            || !closer[close_start + close_len..].chars().all(char::is_whitespace)
        {
            return None;
        }
        let close_offset = ctx.line_offsets[end_line] + close_start;
        (ctx.is_inside_template_code(close_offset) || ctx.is_inside_mdx_code(close_offset))
            .then_some(ctx.line_offsets[start_line])
    })
}

/// The style for code fence markers (MD048)
#[derive(Debug, PartialEq, Eq, Clone, Copy, Default, Hash)]
pub enum CodeFenceStyle {
    /// Consistent with the first code fence style found
    #[default]
    Consistent,
    /// Backtick style (```)
    Backtick,
    /// Tilde style (~~~)
    Tilde,
}

impl fmt::Display for CodeFenceStyle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CodeFenceStyle::Backtick => write!(f, "backtick"),
            CodeFenceStyle::Tilde => write!(f, "tilde"),
            CodeFenceStyle::Consistent => write!(f, "consistent"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fence_ambiguity_requires_a_literal_matching_closer() {
        use crate::config::MarkdownFlavor;
        use crate::lint_context::LintContext;
        let prefix = "~~~rust\nSafe\n~~~\n\n";
        for ending in ["\n", "\r\n"] {
            for marker in ["~~~", "~~~~", "  ~~~"] {
                let code = format!("Visible {{{{ \"first\n{marker}\nlast\" }}}}");
                let source = format!("{prefix}~~~rust\n{code}\n~~~\nAfter.\n").replace('\n', ending);
                let ctx = LintContext::new(&source, MarkdownFlavor::Standard, None);
                assert_eq!(
                    fence_ambiguity_limit(&ctx),
                    Some(prefix.replace('\n', ending).len()),
                    "{source}"
                );
            }
        }
        for (opener, marker, tail) in [
            ("~~~rust", "~~~ trailing", "last\" }}\n~~~\n"),
            ("~~~rust", "```", "last\" }}\n~~~\n"),
            ("~~~~rust", "~~~", "last\" }}\n~~~~\n"),
            ("~~~rust", "~~~ trailing", ""),
        ] {
            let source = format!("{opener}\nVisible {{{{ \"first\n{marker}\n{tail}");
            let ctx = LintContext::new(&source, MarkdownFlavor::Standard, None);
            assert_eq!(fence_ambiguity_limit(&ctx), None, "{source}");
        }
    }

    #[test]
    fn test_code_fence_style_default() {
        let style = CodeFenceStyle::default();
        assert_eq!(style, CodeFenceStyle::Consistent);
    }

    #[test]
    fn test_code_fence_style_display() {
        assert_eq!(format!("{}", CodeFenceStyle::Backtick), "backtick");
        assert_eq!(format!("{}", CodeFenceStyle::Tilde), "tilde");
        assert_eq!(format!("{}", CodeFenceStyle::Consistent), "consistent");
    }
}
