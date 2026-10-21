use crate::mdx_parser::{MdxSignal, ParseOptions, message, to_mdast, unist::Point};
use pretty_assertions::assert_eq;

/// A stand-in for a JavaScript parser: brackets must match and `"` strings
/// are skipped; open brackets or strings at the end ask for more.
fn parse(value: &str) -> MdxSignal {
    let mut open = vec![];
    let mut in_string = false;
    for (index, byte) in value.bytes().enumerate() {
        if in_string {
            in_string = byte != b'"';
            continue;
        }
        match byte {
            b'"' => in_string = true,
            b'(' => open.push(b')'),
            b'[' => open.push(b']'),
            b'{' => open.push(b'}'),
            b')' | b']' | b'}' if open.pop() != Some(byte) => {
                return MdxSignal::Error(
                    "Unexpected closing bracket".into(),
                    index,
                    Box::new("test".into()),
                    Box::new("close".into()),
                );
            }
            _ => {}
        }
    }
    if in_string || !open.is_empty() {
        MdxSignal::Eof(
            "Unexpected end of file".into(),
            Box::new("test".into()),
            Box::new("eof".into()),
        )
    } else {
        MdxSignal::Ok
    }
}

fn options(factor: Option<usize>) -> ParseOptions {
    ParseOptions {
        mdx_expression_parse: Some(Box::new(|value, _kind| parse(value))),
        mdx_esm_parse: Some(Box::new(parse)),
        mdx_parse_budget_factor: factor,
        ..ParseOptions::mdx()
    }
}

fn budget_error(result: Result<crate::mdx_parser::mdast::Node, message::Message>) -> Option<Point> {
    match result {
        Err(message) if *message.rule_id == "mdx-parse-budget" => match message.place {
            Some(place) => match *place {
                message::Place::Point(point) => Some(point),
                message::Place::Position(position) => Some(position.start),
            },
            None => None,
        },
        _ => None,
    }
}

#[test]
fn mdx_parse_budget() -> Result<(), message::Message> {
    let document = "export const a = [\nb,\n\nc]\n\n{\"}}\"}\n\n# d {e}";

    assert_eq!(
        to_mdast(document, &options(None))?,
        to_mdast(document, &options(Some(100)))?,
        "should parse the same with a large enough budget"
    );

    let braces = format!("a {{\"{}\"}}", "}".repeat(100));
    assert_eq!(
        budget_error(to_mdast(&braces, &options(Some(4)))),
        Some(Point::new(1, 3, 2)),
        "should stop at the opening brace of an expression that needs more parsing than allowed"
    );
    assert!(
        to_mdast(&braces, &options(None)).is_ok(),
        "should parse that expression without a budget"
    );

    let esm = format!("export const a = [\n{}]", "b,\n\n".repeat(100));
    assert_eq!(
        budget_error(to_mdast(&esm, &options(Some(4)))),
        Some(Point::new(1, 1, 0)),
        "should stop at the keyword of ESM that needs more parsing than allowed"
    );
    assert!(
        to_mdast(&esm, &options(None)).is_ok(),
        "should parse that ESM without a budget"
    );

    let agnostic = ParseOptions {
        mdx_parse_budget_factor: Some(4),
        ..ParseOptions::mdx()
    };
    let tags = format!("{}{}x</a>", "<a b={\n".repeat(20), "}>".repeat(20));
    assert_eq!(
        budget_error(to_mdast(&tags, &agnostic)),
        Some(Point::new(5, 6, 33)),
        "should count braces against the budget without a parser, and stop at the brace that runs over it"
    );
    assert!(
        to_mdast(&tags, &ParseOptions::mdx()).is_ok(),
        "should count braces again without a budget"
    );

    let paragraphs = "{\"}\"}\n\n".repeat(50);
    assert!(
        to_mdast(&paragraphs, &options(Some(1))).is_ok(),
        "should allow reading each part once"
    );

    // Each expression reads 1 + 2 + … + 20 + 22 = 232 bytes: 5 times the 50
    // bytes of the document fits one, not two.
    let expression = format!("{{\"{}\"}}", "}".repeat(20));
    let twice = format!("{}\n\n{}", expression, expression);
    assert_eq!(
        budget_error(to_mdast(&twice, &options(Some(5)))),
        Some(Point::new(3, 1, 26)),
        "should share the budget across expressions"
    );

    Ok(())
}
