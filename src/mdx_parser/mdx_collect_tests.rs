use crate::mdx_parser::{MdxSignal, ParseOptions, to_mdast};
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

#[test]
fn mdx_collect() {
    let options = ParseOptions {
        mdx_expression_parse: Some(Box::new(|value, _kind| parse(value))),
        mdx_esm_parse: Some(Box::new(parse)),
        ..ParseOptions::mdx()
    };

    assert_eq!(
        to_mdast("<>{}x\n{{}", &options).map_err(|message| message.to_string()),
        Err("2:4: Unexpected end of file (test:eof)".into()),
        "should collect each expression from its own start"
    );
}
