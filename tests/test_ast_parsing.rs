#[cfg(test)]
mod test_ast_parsing {
    use markdown::mdast::Node;
    use markdown::{ParseOptions, to_mdast};

    #[test]
    fn test_code_span_parsing() {
        let test_cases = vec![
            ("Simple code span", "`<test>`"),
            ("Code span before block", "`<env>`\n\n```diff\n- old\n+ new\n```"),
            ("Multiple code spans", "`<one>` and `<two>`"),
            ("Code span at end of line", "Testing `<test>`\n```\ncode\n```"),
        ];

        for (name, content) in test_cases {
            println!("\n=== {name} ===");
            println!("Content: {content:?}");

            let ast = to_mdast(content, &ParseOptions::default()).unwrap();
            let inline_codes = count_inline_codes(&ast);

            println!("Found {inline_codes} InlineCode nodes");
            print_ast(&ast, 0);

            // Count backticks manually
            let backtick_pairs = content.matches('`').count() / 2;
            assert_eq!(
                inline_codes, backtick_pairs,
                "Test '{name}' failed: AST found {inline_codes} code spans but content has {backtick_pairs} backtick pairs"
            );
        }
    }

    fn count_inline_codes(node: &Node) -> usize {
        let mut count = 0;
        match node {
            Node::InlineCode(_) => count += 1,
            Node::Root(root) => {
                for child in &root.children {
                    count += count_inline_codes(child);
                }
            }
            Node::Paragraph(para) => {
                for child in &para.children {
                    count += count_inline_codes(child);
                }
            }
            Node::Heading(heading) => {
                for child in &heading.children {
                    count += count_inline_codes(child);
                }
            }
            Node::List(list) => {
                for child in &list.children {
                    count += count_inline_codes(child);
                }
            }
            Node::ListItem(item) => {
                for child in &item.children {
                    count += count_inline_codes(child);
                }
            }
            _ => {}
        }
        count
    }

    fn print_ast(node: &Node, level: usize) {
        let indent = "  ".repeat(level);
        match node {
            Node::Root(_) => {
                println!("{indent}Root");
                if let Node::Root(root) = node {
                    for child in &root.children {
                        print_ast(child, level + 1);
                    }
                }
            }
            Node::Paragraph(_) => {
                println!("{indent}Paragraph");
                if let Node::Paragraph(para) = node {
                    for child in &para.children {
                        print_ast(child, level + 1);
                    }
                }
            }
            Node::InlineCode(code) => {
                println!("{indent}InlineCode: '{}'", code.value);
            }
            Node::Code(code) => {
                println!("{indent}Code block: lang={:?}", code.lang);
            }
            Node::Text(text) => {
                let escaped = text.value.replace('\n', "\\n");
                println!("{indent}Text: '{escaped}'");
            }
            _ => {
                println!("{indent}Other: {node:?}");
            }
        }
    }
}
