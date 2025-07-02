//!
//! AST parsing utilities and caching for rumdl
//!
//! This module provides shared AST parsing and caching functionality to avoid
//! reparsing the same Markdown content multiple times across different rules.

use crate::rule::MarkdownAst;
use lazy_static::lazy_static;
use std::collections::HashMap;
use std::panic;
use std::sync::{Arc, Mutex};

/// Cache for parsed AST nodes
#[derive(Debug)]
pub struct AstCache {
    cache: HashMap<u64, Arc<MarkdownAst>>,
    usage_stats: HashMap<u64, u64>,
}

impl Default for AstCache {
    fn default() -> Self {
        Self::new()
    }
}

impl AstCache {
    pub fn new() -> Self {
        Self {
            cache: HashMap::new(),
            usage_stats: HashMap::new(),
        }
    }

    /// Get or parse AST for the given content
    pub fn get_or_parse(&mut self, content: &str) -> Arc<MarkdownAst> {
        let content_hash = crate::utils::fast_hash(content);

        if let Some(ast) = self.cache.get(&content_hash) {
            *self.usage_stats.entry(content_hash).or_insert(0) += 1;
            return ast.clone();
        }

        // Parse the AST
        let ast = Arc::new(parse_markdown_ast(content));
        self.cache.insert(content_hash, ast.clone());
        *self.usage_stats.entry(content_hash).or_insert(0) += 1;

        ast
    }

    /// Get cache statistics
    pub fn get_stats(&self) -> HashMap<u64, u64> {
        self.usage_stats.clone()
    }

    /// Clear the cache
    pub fn clear(&mut self) {
        self.cache.clear();
        self.usage_stats.clear();
    }

    /// Get cache size
    pub fn len(&self) -> usize {
        self.cache.len()
    }

    /// Check if cache is empty
    pub fn is_empty(&self) -> bool {
        self.cache.is_empty()
    }
}

lazy_static! {
    /// Global AST cache instance
    static ref GLOBAL_AST_CACHE: Arc<Mutex<AstCache>> = Arc::new(Mutex::new(AstCache::new()));
}

/// Get or parse AST from the global cache
pub fn get_cached_ast(content: &str) -> Arc<MarkdownAst> {
    let mut cache = GLOBAL_AST_CACHE.lock().unwrap();
    cache.get_or_parse(content)
}

/// Get AST cache statistics
pub fn get_ast_cache_stats() -> HashMap<u64, u64> {
    let cache = GLOBAL_AST_CACHE.lock().unwrap();
    cache.get_stats()
}

/// Clear the global AST cache
pub fn clear_ast_cache() {
    let mut cache = GLOBAL_AST_CACHE.lock().unwrap();
    cache.clear();
}

/// Parse Markdown content into an AST
pub fn parse_markdown_ast(content: &str) -> MarkdownAst {
    // Check for problematic patterns that cause the markdown crate to panic
    if content_has_problematic_lists(content) {
        log::debug!("Detected problematic list patterns, skipping AST parsing");
        return MarkdownAst::Root(markdown::mdast::Root {
            children: vec![],
            position: None,
        });
    }

    // Try to parse AST with GFM extensions enabled, but handle panics from the markdown crate
    match panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut parse_options = markdown::ParseOptions::gfm();
        parse_options.constructs.frontmatter = true; // Also enable frontmatter parsing
        markdown::to_mdast(content, &parse_options)
    })) {
        Ok(Ok(ast)) => {
            // Successfully parsed AST
            ast
        }
        Ok(Err(err)) => {
            // Parsing failed with an error
            log::debug!("Failed to parse markdown AST in ast_utils: {err:?}");
            MarkdownAst::Root(markdown::mdast::Root {
                children: vec![],
                position: None,
            })
        }
        Err(_) => {
            // Parsing panicked
            log::debug!("Markdown AST parsing panicked in ast_utils, falling back to empty AST");
            MarkdownAst::Root(markdown::mdast::Root {
                children: vec![],
                position: None,
            })
        }
    }
}

/// Check if content contains patterns that cause the markdown crate to panic
fn content_has_problematic_lists(content: &str) -> bool {
    let lines: Vec<&str> = content.lines().collect();

    // Look for mixed list markers in consecutive lines (which causes the panic)
    for window in lines.windows(3) {
        if window.len() >= 2 {
            let line1 = window[0].trim_start();
            let line2 = window[1].trim_start();

            // Check if both lines are list items with different markers
            let is_list1 = line1.starts_with("* ") || line1.starts_with("+ ") || line1.starts_with("- ");
            let is_list2 = line2.starts_with("* ") || line2.starts_with("+ ") || line2.starts_with("- ");

            if is_list1 && is_list2 {
                let marker1 = line1.chars().next().unwrap_or(' ');
                let marker2 = line2.chars().next().unwrap_or(' ');

                // If different markers, this could cause a panic
                if marker1 != marker2 {
                    return true;
                }
            }
        }
    }

    // Check for mixed markers with different indentation levels
    for i in 0..lines.len().saturating_sub(1) {
        let line1 = lines[i];
        let line2 = lines[i + 1];

        // Get the full line for marker check
        let trimmed1 = line1.trim_start();
        let trimmed2 = line2.trim_start();

        let is_list1 = trimmed1.starts_with("* ") || trimmed1.starts_with("+ ") || trimmed1.starts_with("- ");
        let is_list2 = trimmed2.starts_with("* ") || trimmed2.starts_with("+ ") || trimmed2.starts_with("- ");

        if is_list1 && is_list2 {
            let marker1 = trimmed1.chars().next().unwrap_or(' ');
            let marker2 = trimmed2.chars().next().unwrap_or(' ');

            // If different markers (even with different indentation), this could cause issues
            if marker1 != marker2 {
                return true;
            }
        }
    }

    false
}

/// Check if AST contains specific node types
pub fn ast_contains_node_type(ast: &MarkdownAst, node_type: &str) -> bool {
    match ast {
        MarkdownAst::Root(root) => root
            .children
            .iter()
            .any(|child| ast_contains_node_type(child, node_type)),
        MarkdownAst::Heading(_) if node_type == "heading" => true,
        MarkdownAst::List(_) if node_type == "list" => true,
        MarkdownAst::Link(_) if node_type == "link" => true,
        MarkdownAst::Image(_) if node_type == "image" => true,
        MarkdownAst::Code(_) if node_type == "code" => true,
        MarkdownAst::InlineCode(_) if node_type == "inline_code" => true,
        MarkdownAst::Emphasis(_) if node_type == "emphasis" => true,
        MarkdownAst::Strong(_) if node_type == "strong" => true,
        MarkdownAst::Html(_) if node_type == "html" => true,
        MarkdownAst::Blockquote(_) if node_type == "blockquote" => true,
        MarkdownAst::Table(_) if node_type == "table" => true,
        _ => {
            // Check children recursively
            if let Some(children) = ast.children() {
                children.iter().any(|child| ast_contains_node_type(child, node_type))
            } else {
                false
            }
        }
    }
}

/// Extract all nodes of a specific type from the AST
pub fn extract_nodes_by_type<'a>(ast: &'a MarkdownAst, node_type: &str) -> Vec<&'a MarkdownAst> {
    let mut nodes = Vec::new();
    extract_nodes_by_type_recursive(ast, node_type, &mut nodes);
    nodes
}

fn extract_nodes_by_type_recursive<'a>(ast: &'a MarkdownAst, node_type: &str, nodes: &mut Vec<&'a MarkdownAst>) {
    match ast {
        MarkdownAst::Heading(_) if node_type == "heading" => nodes.push(ast),
        MarkdownAst::List(_) if node_type == "list" => nodes.push(ast),
        MarkdownAst::Link(_) if node_type == "link" => nodes.push(ast),
        MarkdownAst::Image(_) if node_type == "image" => nodes.push(ast),
        MarkdownAst::Code(_) if node_type == "code" => nodes.push(ast),
        MarkdownAst::InlineCode(_) if node_type == "inline_code" => nodes.push(ast),
        MarkdownAst::Emphasis(_) if node_type == "emphasis" => nodes.push(ast),
        MarkdownAst::Strong(_) if node_type == "strong" => nodes.push(ast),
        MarkdownAst::Html(_) if node_type == "html" => nodes.push(ast),
        MarkdownAst::Blockquote(_) if node_type == "blockquote" => nodes.push(ast),
        MarkdownAst::Table(_) if node_type == "table" => nodes.push(ast),
        _ => {}
    }

    // Check children recursively
    if let Some(children) = ast.children() {
        for child in children {
            extract_nodes_by_type_recursive(child, node_type, nodes);
        }
    }
}

/// Utility function to get text content from AST nodes
pub fn get_text_content(ast: &MarkdownAst) -> String {
    match ast {
        MarkdownAst::Text(text) => text.value.clone(),
        MarkdownAst::InlineCode(code) => code.value.clone(),
        MarkdownAst::Code(code) => code.value.clone(),
        _ => {
            if let Some(children) = ast.children() {
                children.iter().map(get_text_content).collect::<Vec<_>>().join("")
            } else {
                String::new()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ast_cache() {
        let mut cache = AstCache::new();
        let content = "# Hello World\n\nThis is a test.";

        let ast1 = cache.get_or_parse(content);
        let ast2 = cache.get_or_parse(content);

        // Should return the same Arc (cached)
        assert!(Arc::ptr_eq(&ast1, &ast2));
        assert_eq!(cache.len(), 1);

        // Test usage stats
        let stats = cache.get_stats();
        let content_hash = crate::utils::fast_hash(content);
        assert_eq!(stats.get(&content_hash), Some(&2));
    }

    #[test]
    fn test_ast_cache_multiple_documents() {
        let mut cache = AstCache::new();
        let content1 = "# Document 1";
        let content2 = "# Document 2";
        let content3 = "# Document 3";

        let _ast1 = cache.get_or_parse(content1);
        let _ast2 = cache.get_or_parse(content2);
        let _ast3 = cache.get_or_parse(content3);
        assert_eq!(cache.len(), 3);

        // Access first document again
        let _ast1_again = cache.get_or_parse(content1);
        assert_eq!(cache.len(), 3); // Still 3 documents

        let stats = cache.get_stats();
        let hash1 = crate::utils::fast_hash(content1);
        assert_eq!(stats.get(&hash1), Some(&2)); // Accessed twice
    }

    #[test]
    fn test_ast_cache_clear() {
        let mut cache = AstCache::new();
        cache.get_or_parse("# Test");
        cache.get_or_parse("## Another");

        assert_eq!(cache.len(), 2);
        assert!(!cache.is_empty());

        cache.clear();
        assert_eq!(cache.len(), 0);
        assert!(cache.is_empty());
        assert!(cache.get_stats().is_empty());
    }

    #[test]
    fn test_parse_markdown_ast() {
        let content = "# Hello World\n\nThis is a test.";
        let ast = parse_markdown_ast(content);

        assert!(matches!(ast, MarkdownAst::Root(_)));
    }

    #[test]
    fn test_problematic_list_detection() {
        // Mixed list markers that would cause panic
        let problematic = "* Item 1\n- Item 2\n+ Item 3";
        assert!(content_has_problematic_lists(problematic));

        // Consistent markers should be fine
        let ok_content = "* Item 1\n* Item 2\n* Item 3";
        assert!(!content_has_problematic_lists(ok_content));

        // Different marker types separated by content
        let separated = "* Item 1\n\nSome text\n\n- Item 2";
        assert!(!content_has_problematic_lists(separated));

        // Edge case: markers with different indentation
        let indented = "* Item 1\n  - Subitem";
        assert!(content_has_problematic_lists(indented));
    }

    #[test]
    fn test_parse_malformed_markdown() {
        // Test various malformed markdown that might cause issues
        let test_cases = vec![
            "",                           // Empty
            "\n\n\n",                     // Only newlines
            "```",                        // Unclosed code block
            "```\ncode\n```extra```",     // Multiple code blocks
            "[link]()",                   // Empty link URL
            "![]()",                      // Empty image
            "|table|without|header|",     // Malformed table
            "> > > deeply nested quotes", // Deep nesting
            "# \n## \n### ",              // Empty headings
            "*unclosed emphasis",         // Unclosed emphasis
            "**unclosed strong",          // Unclosed strong
            "[unclosed link",             // Unclosed link
            "![unclosed image",           // Unclosed image
            "---\ntitle: test",           // Unclosed front matter
        ];

        for content in test_cases {
            let ast = parse_markdown_ast(content);
            // Should always return a valid AST, even if empty
            assert!(matches!(ast, MarkdownAst::Root(_)));
        }
    }

    #[test]
    fn test_ast_with_mixed_list_markers() {
        // This should trigger the problematic list detection
        let content = "* First\n- Second\n+ Third";
        let ast = parse_markdown_ast(content);

        // Should return empty AST due to problematic pattern
        if let MarkdownAst::Root(root) = ast {
            assert!(root.children.is_empty());
        } else {
            panic!("Expected Root AST node");
        }
    }

    #[test]
    fn test_ast_contains_node_type() {
        let content = "# Hello World\n\nThis is a [link](http://example.com).";
        let ast = parse_markdown_ast(content);

        assert!(ast_contains_node_type(&ast, "heading"));
        assert!(ast_contains_node_type(&ast, "link"));
        assert!(!ast_contains_node_type(&ast, "table"));

        // Test with empty AST
        let empty_ast = MarkdownAst::Root(markdown::mdast::Root {
            children: vec![],
            position: None,
        });
        assert!(!ast_contains_node_type(&empty_ast, "heading"));
    }

    #[test]
    fn test_ast_contains_nested_nodes() {
        let content = "> # Heading in blockquote\n> \n> With a [link](url)";
        let ast = parse_markdown_ast(content);

        assert!(ast_contains_node_type(&ast, "blockquote"));
        assert!(ast_contains_node_type(&ast, "heading"));
        assert!(ast_contains_node_type(&ast, "link"));
    }

    #[test]
    fn test_extract_nodes_by_type() {
        let content = "# Heading 1\n\n## Heading 2\n\nSome text.";
        let ast = parse_markdown_ast(content);

        let headings = extract_nodes_by_type(&ast, "heading");
        assert_eq!(headings.len(), 2);

        // Test extracting non-existent type
        let tables = extract_nodes_by_type(&ast, "table");
        assert_eq!(tables.len(), 0);
    }

    #[test]
    fn test_extract_multiple_node_types() {
        let content = "# Heading\n\n*emphasis* and **strong** and `code`\n\n[link](url) and ![image](img.png)";
        let ast = parse_markdown_ast(content);

        assert_eq!(extract_nodes_by_type(&ast, "heading").len(), 1);
        assert_eq!(extract_nodes_by_type(&ast, "emphasis").len(), 1);
        assert_eq!(extract_nodes_by_type(&ast, "strong").len(), 1);
        assert_eq!(extract_nodes_by_type(&ast, "inline_code").len(), 1);
        assert_eq!(extract_nodes_by_type(&ast, "link").len(), 1);
        assert_eq!(extract_nodes_by_type(&ast, "image").len(), 1);
    }

    #[test]
    fn test_get_text_content() {
        let content = "Hello world";
        let ast = MarkdownAst::Text(markdown::mdast::Text {
            value: content.to_string(),
            position: None,
        });

        assert_eq!(get_text_content(&ast), content);

        // Test inline code
        let code_ast = MarkdownAst::InlineCode(markdown::mdast::InlineCode {
            value: "code".to_string(),
            position: None,
        });
        assert_eq!(get_text_content(&code_ast), "code");

        // Test code block
        let block_ast = MarkdownAst::Code(markdown::mdast::Code {
            value: "fn main() {}".to_string(),
            lang: None,
            meta: None,
            position: None,
        });
        assert_eq!(get_text_content(&block_ast), "fn main() {}");
    }

    #[test]
    fn test_get_text_content_nested() {
        // Create a paragraph with mixed content
        let paragraph = MarkdownAst::Paragraph(markdown::mdast::Paragraph {
            children: vec![
                MarkdownAst::Text(markdown::mdast::Text {
                    value: "Hello ".to_string(),
                    position: None,
                }),
                MarkdownAst::Strong(markdown::mdast::Strong {
                    children: vec![MarkdownAst::Text(markdown::mdast::Text {
                        value: "world".to_string(),
                        position: None,
                    })],
                    position: None,
                }),
                MarkdownAst::Text(markdown::mdast::Text {
                    value: "!".to_string(),
                    position: None,
                }),
            ],
            position: None,
        });

        assert_eq!(get_text_content(&paragraph), "Hello world!");
    }

    #[test]
    fn test_global_cache_functions() {
        // Clear cache first to ensure clean state
        clear_ast_cache();

        let content = "# Global cache test";
        let ast1 = get_cached_ast(content);
        let ast2 = get_cached_ast(content);

        // Should be the same instance
        assert!(Arc::ptr_eq(&ast1, &ast2));

        // Check stats
        let stats = get_ast_cache_stats();
        assert!(!stats.is_empty());

        // Clear and verify
        clear_ast_cache();
        let stats_after = get_ast_cache_stats();
        assert!(stats_after.is_empty());
    }

    #[test]
    fn test_unicode_content() {
        let unicode_content = "# 你好世界\n\n这是一个测试。\n\n## Ñoño\n\n🚀 Emoji content!";
        let ast = parse_markdown_ast(unicode_content);

        assert!(matches!(ast, MarkdownAst::Root(_)));
        assert!(ast_contains_node_type(&ast, "heading"));

        // Extract headings and verify count
        let headings = extract_nodes_by_type(&ast, "heading");
        assert_eq!(headings.len(), 2);
    }

    #[test]
    fn test_very_large_document() {
        // Generate a large document
        let mut content = String::new();
        for i in 0..1000 {
            content.push_str(&format!("# Heading {i}\n\nParagraph {i}\n\n"));
        }

        let ast = parse_markdown_ast(&content);
        assert!(matches!(ast, MarkdownAst::Root(_)));

        // Should have 1000 headings
        let headings = extract_nodes_by_type(&ast, "heading");
        assert_eq!(headings.len(), 1000);
    }

    #[test]
    fn test_deeply_nested_structure() {
        let content = "> > > > > Deeply nested blockquote\n> > > > > > Even deeper";
        let ast = parse_markdown_ast(content);

        assert!(matches!(ast, MarkdownAst::Root(_)));
        assert!(ast_contains_node_type(&ast, "blockquote"));
    }

    #[test]
    fn test_all_node_types() {
        let comprehensive_content = r#"# Heading

> Blockquote

- List item

| Table | Header |
|-------|--------|
| Cell  | Cell   |

```rust
code block
```

*emphasis* **strong** `inline code`

[link](url) ![image](img.png)

<div>HTML</div>

---
"#;

        let ast = parse_markdown_ast(comprehensive_content);

        // Test all node type detections
        assert!(ast_contains_node_type(&ast, "heading"));
        assert!(ast_contains_node_type(&ast, "blockquote"));
        assert!(ast_contains_node_type(&ast, "list"));
        // Tables are now supported with GFM extension enabled
        assert!(ast_contains_node_type(&ast, "table"));
        assert!(ast_contains_node_type(&ast, "code"));
        assert!(ast_contains_node_type(&ast, "emphasis"));
        assert!(ast_contains_node_type(&ast, "strong"));
        assert!(ast_contains_node_type(&ast, "inline_code"));
        assert!(ast_contains_node_type(&ast, "link"));
        assert!(ast_contains_node_type(&ast, "image"));
        assert!(ast_contains_node_type(&ast, "html"));
    }

    #[test]
    fn test_gfm_table_parsing() {
        // Test that GFM tables are properly parsed
        let table_content = r#"| Column 1 | Column 2 |
|----------|----------|
| Cell 1   | Cell 2   |
| Cell 3   | Cell 4   |"#;

        let ast = parse_markdown_ast(table_content);
        assert!(ast_contains_node_type(&ast, "table"));

        let tables = extract_nodes_by_type(&ast, "table");
        assert_eq!(tables.len(), 1);

        // Test more complex table with alignment
        let complex_table = r#"| Left | Center | Right |
|:-----|:------:|------:|
| L    |   C    |     R |
| Left |  Mid   | Right |"#;

        let ast2 = parse_markdown_ast(complex_table);
        assert!(ast_contains_node_type(&ast2, "table"));
    }

    #[test]
    fn test_edge_case_empty_nodes() {
        // Test with nodes that have empty content
        let empty_text = MarkdownAst::Text(markdown::mdast::Text {
            value: String::new(),
            position: None,
        });
        assert_eq!(get_text_content(&empty_text), "");

        // Test with node that has no children method
        let thematic_break = MarkdownAst::ThematicBreak(markdown::mdast::ThematicBreak { position: None });
        assert_eq!(get_text_content(&thematic_break), "");
    }
}
