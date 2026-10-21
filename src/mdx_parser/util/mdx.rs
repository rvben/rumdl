use alloc::{boxed::Box, string::String};

/// Signal used as feedback when parsing MDX ESM/expressions.
#[derive(Clone, Debug)]
pub enum Signal {
    /// A syntax error.
    ///
    /// `markdown-rs` will crash with error message `String`, and convert the
    /// `usize` (byte offset into `&str` passed to `MdxExpressionParse` or
    /// `MdxEsmParse`) to where it happened in the whole document.
    ///
    /// ## Examples
    ///
    /// ```rust ignore
    /// Signal::Error("Unexpected `\"`, expected identifier".into(), 1)
    /// ```
    Error(String, usize, Box<String>, Box<String>),
    /// An error at the end of the (partial?) expression.
    ///
    /// `markdown-rs` will either crash with error message `String` if it
    /// doesn’t have any more text, or it will try again later when more text
    /// is available.
    ///
    /// ## Examples
    ///
    /// ```rust ignore
    /// Signal::Eof("Unexpected end of file in string literal".into())
    /// ```
    Eof(String, Box<String>, Box<String>),
    /// Done, successfully.
    ///
    /// `markdown-rs` knows that this is the end of a valid expression/esm and
    /// continues with markdown.
    ///
    /// ## Examples
    ///
    /// ```rust ignore
    /// Signal::Ok
    /// ```
    Ok,
}

/// Signature of a function that parses MDX ESM.
///
/// Can be passed as `mdx_esm_parse` in
/// [`ParseOptions`][crate::mdx_parser::configuration::ParseOptions] to support
/// ESM according to a certain grammar (typically, a programming language).
pub type EsmParse = dyn Fn(&str) -> Signal;

/// Expression kind.
#[derive(Clone, Debug)]
pub enum ExpressionKind {
    /// Kind of expressions in prose.
    ///
    /// ```mdx
    /// > | # {Math.PI}
    ///       ^^^^^^^^^
    ///   |
    /// > | {Math.PI}
    ///     ^^^^^^^^^
    /// ```
    Expression,
    /// Kind of expressions as attributes.
    ///
    /// ```mdx
    /// > | <a {...b}>
    ///        ^^^^^^
    /// ```
    AttributeExpression,
    /// Kind of expressions as attribute values.
    ///
    /// ```mdx
    /// > | <a b={c}>
    ///          ^^^
    /// ```
    AttributeValueExpression,
}

/// Signature of a function that parses MDX expressions.
///
/// Can be passed as `mdx_expression_parse` in
/// [`ParseOptions`][crate::mdx_parser::configuration::ParseOptions] to support
/// expressions according to a certain grammar (typically, a programming
/// language).
///
pub type ExpressionParse = dyn Fn(&str, &ExpressionKind) -> Signal;

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::boxed::Box;

    #[test]
    fn test_mdx_expression_parse() {
        fn func(_value: &str, _kind: &ExpressionKind) -> Signal {
            Signal::Ok
        }

        let func_accepting = |_a: Box<ExpressionParse>| true;

        assert!(
            matches!(func("a", &ExpressionKind::Expression), Signal::Ok),
            "should expose an `ExpressionParse` type (1)"
        );

        assert!(
            func_accepting(Box::new(func)),
            "should expose an `ExpressionParse` type (2)"
        );
    }

    #[test]
    fn test_mdx_esm_parse() {
        fn func(_value: &str) -> Signal {
            Signal::Ok
        }

        let func_accepting = |_a: Box<EsmParse>| true;

        assert!(matches!(func("a"), Signal::Ok), "should expose an `EsmParse` type (1)");

        assert!(func_accepting(Box::new(func)), "should expose an `EsmParse` type (2)");
    }
}
