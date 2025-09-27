#[cfg(test)]
mod test_gfm_vs_default {
    use markdown::mdast::Node;
    use markdown::{ParseOptions, to_mdast};

    #[test]
    fn test_parsing_options_difference() {
        let content = "`<env>`\n\n```diff\n- old\n+ new\n```";

        println!("\n=== Testing with DEFAULT options ===");
        let ast_default = to_mdast(content, &ParseOptions::default()).unwrap();
        let count_default = count_inline_codes(&ast_default);
        println!("InlineCode nodes with default: {count_default}");

        println!("\n=== Testing with GFM options ===");
        let mut gfm_options = ParseOptions::gfm();
        gfm_options.constructs.frontmatter = true;
        let ast_gfm = to_mdast(content, &gfm_options).unwrap();
        let count_gfm = count_inline_codes(&ast_gfm);
        println!("InlineCode nodes with GFM: {count_gfm}");

        assert_eq!(
            count_default, count_gfm,
            "GFM and default parsing should find the same number of code spans"
        );
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
            _ => {
                // Check other node types that might have children
                if let Node::Heading(h) = node {
                    for child in &h.children {
                        count += count_inline_codes(child);
                    }
                } else if let Node::List(l) = node {
                    for child in &l.children {
                        count += count_inline_codes(child);
                    }
                } else if let Node::ListItem(li) = node {
                    for child in &li.children {
                        count += count_inline_codes(child);
                    }
                }
            }
        }
        count
    }
}
