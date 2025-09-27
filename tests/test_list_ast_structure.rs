#[cfg(test)]
mod test_list_ast_structure {
    use markdown::mdast::Node;
    use markdown::{ParseOptions, to_mdast};

    #[test]
    fn test_list_with_backslash_continuations() {
        let content = r#"# Header

1. First\
   Second\
   Third

2. Next item

Text"#;

        println!("Content:\n{content}\n");

        let ast = to_mdast(content, &ParseOptions::default()).unwrap();
        print_ast(&ast, 0);

        // Check list structure
        check_list_boundaries(&ast, content);
    }

    fn print_ast(node: &Node, level: usize) {
        let indent = "  ".repeat(level);
        match node {
            Node::Root(root) => {
                println!("{indent}Root");
                for child in &root.children {
                    print_ast(child, level + 1);
                }
            }
            Node::List(list) => {
                println!("{indent}List (ordered={}, pos={:?})", list.ordered, list.position);
                for child in &list.children {
                    print_ast(child, level + 1);
                }
            }
            Node::ListItem(item) => {
                println!("{indent}ListItem (pos={:?})", item.position);
                for child in &item.children {
                    print_ast(child, level + 1);
                }
            }
            Node::Paragraph(para) => {
                println!("{indent}Paragraph (pos={:?})", para.position);
                for child in &para.children {
                    print_ast(child, level + 1);
                }
            }
            Node::Text(text) => {
                let display_text = text.value.replace('\n', "\\n");
                println!("{indent}Text: '{display_text}' (pos={:?})", text.position);
            }
            Node::Break(br) => {
                println!("{indent}Break (hard line break) (pos={:?})", br.position);
            }
            Node::Heading(heading) => {
                println!("{indent}Heading level {} (pos={:?})", heading.depth, heading.position);
                for child in &heading.children {
                    print_ast(child, level + 1);
                }
            }
            _ => {
                println!("{indent}Other node type");
            }
        }
    }

    fn check_list_boundaries(node: &Node, content: &str) {
        let lines: Vec<&str> = content.lines().collect();

        fn find_lists(node: &Node, lines: &[&str]) {
            match node {
                Node::List(list) => {
                    if let Some(pos) = &list.position {
                        let start_line = pos.start.line;
                        let end_line = pos.end.line;

                        println!("\nList boundaries:");
                        println!(
                            "  Start: line {} ('{}')",
                            start_line,
                            lines.get(start_line - 1).unwrap_or(&"")
                        );
                        println!(
                            "  End: line {} ('{}')",
                            end_line,
                            lines.get(end_line - 1).unwrap_or(&"")
                        );

                        // Check what's before and after
                        if start_line > 1 {
                            println!(
                                "  Before: line {} ('{}')",
                                start_line - 1,
                                lines.get(start_line - 2).unwrap_or(&"")
                            );
                        }
                        if end_line <= lines.len() {
                            println!(
                                "  After: line {} ('{}')",
                                end_line + 1,
                                lines.get(end_line).unwrap_or(&"")
                            );
                        }
                    }
                }
                Node::Root(root) => {
                    for child in &root.children {
                        find_lists(child, lines);
                    }
                }
                _ => {}
            }
        }

        find_lists(node, &lines);
    }
}
