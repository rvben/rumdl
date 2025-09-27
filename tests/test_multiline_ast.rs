#[cfg(test)]
mod test_multiline_ast {
    use markdown::mdast::Node;
    use rumdl_lib::utils::ast_utils::{clear_ast_cache, get_cached_ast};

    #[test]
    fn test_multiline_ast_parsing() {
        clear_ast_cache();

        let content = "Line 1 `<code>`\n<div>html</div>\nLine 3 `<more>` test";
        println!("\nContent:\n{content}");

        let ast = get_cached_ast(content);

        match ast.as_ref() {
            Node::Root(root) => {
                let child_count = root.children.len();
                println!("\nAST has {child_count} children");

                for (i, child) in root.children.iter().enumerate() {
                    let discriminant = std::mem::discriminant(child);
                    println!("Child {i}: {discriminant:?}");

                    if let Node::Paragraph(para) = child {
                        let para_child_count = para.children.len();
                        println!("  Paragraph has {para_child_count} children:");
                        for (j, pchild) in para.children.iter().enumerate() {
                            match pchild {
                                Node::InlineCode(code) => {
                                    let value = &code.value;
                                    println!("    {j}: InlineCode('{value}')");
                                }
                                Node::Text(text) => {
                                    let value = text.value.replace('\n', "\\n");
                                    println!("    {j}: Text('{value}')");
                                }
                                _ => {
                                    let discriminant = std::mem::discriminant(pchild);
                                    println!("    {j}: {discriminant:?}");
                                }
                            }
                        }
                    }
                }
            }
            _ => {
                println!("Non-root AST");
            }
        }
    }
}
