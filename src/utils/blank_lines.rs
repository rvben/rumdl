//! What counts as a blank line for the rules that require blank lines around a
//! construct (MD022, MD031, MD032, MD058).
//!
//! Those rules exist so a reader can see where one block ends and the next
//! begins, and a line holding nothing but an HTML comment separates two blocks
//! as plainly as an empty one does. CommonMark disagrees - a comment is an HTML
//! block of type 2 that ends on the line carrying `-->`, so the construct below
//! it really does start with no blank line between them - but reporting that
//! costs more than it is worth: the fix inserts a blank line, and for a
//! directive comment (`<!-- prettier-ignore -->`, `<!-- markdownlint-disable
//! -->`, a generator's own trigger) adjacency *is* the meaning, so rewriting the
//! document turns the directive off. rumdl treats such a line as blank instead.
//! (#866)
//!
//! The odd corners are deliberate and match markdownlint's `isBlankLine`, whose
//! convention this is: an unclosed `<!--` and a bare `-->` each count, which is
//! what makes a comment spanning several lines work, and `>` characters are
//! removed so the whole convention holds inside a blockquote.

/// Whether a line contributes nothing but HTML comments, and so separates the
/// blocks around it the way an empty line does.
///
/// A line with any other content is not blank, however little: `text <!-- c -->`
/// is a paragraph, and a construct written directly below it is missing its
/// blank line.
pub fn is_blank_or_comment_only(line: &str) -> bool {
    if line.trim().is_empty() {
        return true;
    }
    remove_comments(line).replace('>', "").trim().is_empty()
}

/// Remove every HTML comment from a line, tolerating a comment that opens
/// without closing and a close marker with no opener.
///
/// An unpaired marker takes everything on its side of the line with it: an
/// opener swallows the rest of the line (the comment continues below), a closer
/// swallows the start of it (the comment began above). That is what lets a
/// comment spanning several lines count as blank at both ends.
fn remove_comments(line: &str) -> String {
    const OPEN: &str = "<!--";
    const CLOSE: &str = "-->";

    let mut remaining = line;
    let mut kept = String::new();
    loop {
        let open = remaining.find(OPEN);
        let close = remaining.find(CLOSE);
        match (open, close) {
            (None, None) => {
                kept.push_str(remaining);
                return kept;
            }
            // An opener with no closer: the comment continues onto the next line,
            // so it takes the rest of this one.
            (Some(open), None) => {
                kept.push_str(&remaining[..open]);
                return kept;
            }
            // A closer with no opener before it: the comment started on an
            // earlier line, so everything up to and including it is inside it.
            (None, Some(close)) => {
                remaining = &remaining[close + CLOSE.len()..];
            }
            (Some(open), Some(close)) if close < open => {
                remaining = &remaining[close + CLOSE.len()..];
            }
            // A complete comment: drop it and carry on with what is left.
            (Some(open), Some(close)) => {
                kept.push_str(&remaining[..open]);
                remaining = &remaining[close + CLOSE.len()..];
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_line_is_blank() {
        assert!(is_blank_or_comment_only(""));
        assert!(is_blank_or_comment_only("   "));
        assert!(is_blank_or_comment_only("\t"));
    }

    #[test]
    fn a_comment_only_line_is_blank() {
        assert!(is_blank_or_comment_only("<!-- c -->"));
        assert!(is_blank_or_comment_only("  <!-- prettier-ignore -->  "));
        assert!(is_blank_or_comment_only("<!-- a --><!-- b -->"));
    }

    #[test]
    fn an_unpaired_marker_is_blank_so_a_multi_line_comment_works_at_both_ends() {
        assert!(is_blank_or_comment_only("<!--"));
        assert!(is_blank_or_comment_only("-->"));
        assert!(is_blank_or_comment_only("<!-- opens here"));
        assert!(is_blank_or_comment_only("closes here -->"));
    }

    #[test]
    fn the_convention_holds_inside_a_blockquote() {
        assert!(is_blank_or_comment_only("> <!-- c -->"));
        assert!(is_blank_or_comment_only(">> <!-- c -->"));
        assert!(is_blank_or_comment_only(">"));
    }

    #[test]
    fn a_line_carrying_anything_else_is_not_blank() {
        assert!(!is_blank_or_comment_only("text"));
        assert!(!is_blank_or_comment_only("text <!-- c -->"));
        assert!(!is_blank_or_comment_only("<!-- c --> text"));
        assert!(!is_blank_or_comment_only("<!-- a --> x <!-- b -->"));
        assert!(!is_blank_or_comment_only("> text"));
        assert!(!is_blank_or_comment_only("-->text<!--"));
    }
}
