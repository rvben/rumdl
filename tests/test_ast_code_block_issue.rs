#[cfg(test)]
mod test_ast_code_block_issue {
    use markdown::mdast::Node;
    use markdown::{ParseOptions, to_mdast};

    #[test]
    fn test_ast_parsing_with_code_block_after() {
        let test_cases = vec![
            ("Just code span", "`<env>`"),
            ("Code span with text after", "`<env>` text"),
            ("Code span with newline", "`<env>`\n"),
            ("Code span with double newline", "`<env>`\n\n"),
            ("Code span before code block", "`<env>`\n\n```\ncode\n```"),
            ("Code span before diff block", "`<env>`\n\n```diff\n- old\n+ new\n```"),
        ];

        for (name, content) in test_cases {
            println!("\n=== {name} ===");
            println!("Content: {content:?}");

            let ast = to_mdast(content, &ParseOptions::default()).unwrap();
            let inline_codes = count_and_print(&ast, 0);

            println!("Total InlineCode nodes: {inline_codes}");

            assert!(
                inline_codes > 0,
                "Test '{name}' failed: Expected at least one InlineCode node"
            );
        }
    }

    fn count_and_print(node: &Node, level: usize) -> usize {
        let indent = "  ".repeat(level);
        let mut count = 0;

        match node {
            Node::Root(root) => {
                println!("{indent}Root");
                for child in &root.children {
                    count += count_and_print(child, level + 1);
                }
            }
            Node::Paragraph(para) => {
                println!("{indent}Paragraph");
                for child in &para.children {
                    count += count_and_print(child, level + 1);
                }
            }
            Node::InlineCode(code) => {
                let value = &code.value;
                println!("{indent}InlineCode: '{value}'");
                count += 1;
            }
            Node::Code(code) => {
                println!("{indent}Code block: lang={:?}", code.lang);
            }
            Node::Text(text) => {
                let value = text.value.replace('\n', "\\n");
                println!("{indent}Text: '{value}'");
            }
            _ => {
                println!("{indent}Other node");
            }
        }

        count
    }
}
