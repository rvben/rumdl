//!
//! AST parsing utilities and caching for rumdl
//!
//! This module provides shared AST parsing and caching functionality to avoid
//! reparsing the same Markdown content multiple times across different rules.

use crate::rule::MarkdownAst;
use lazy_static::lazy_static;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Cache for parsed AST nodes
#[derive(Debug)]
pub struct AstCache {
    cache: HashMap<u64, Arc<MarkdownAst>>,
    usage_stats: HashMap<u64, u64>,
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
    // Use the markdown crate to parse the content
    markdown::to_mdast(content, &markdown::ParseOptions::default())
        .unwrap_or_else(|_| {
            // Fallback to an empty root node if parsing fails
            MarkdownAst::Root(markdown::mdast::Root {
                children: vec![],
                position: None,
            })
        })
}

/// Check if AST contains specific node types
pub fn ast_contains_node_type(ast: &MarkdownAst, node_type: &str) -> bool {
    match ast {
        MarkdownAst::Root(root) => {
            root.children.iter().any(|child| ast_contains_node_type(child, node_type))
        }
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

fn extract_nodes_by_type_recursive<'a>(
    ast: &'a MarkdownAst,
    node_type: &str,
    nodes: &mut Vec<&'a MarkdownAst>,
) {
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
    }

    #[test]
    fn test_parse_markdown_ast() {
        let content = "# Hello World\n\nThis is a test.";
        let ast = parse_markdown_ast(content);

        assert!(matches!(ast, MarkdownAst::Root(_)));
    }

    #[test]
    fn test_ast_contains_node_type() {
        let content = "# Hello World\n\nThis is a [link](http://example.com).";
        let ast = parse_markdown_ast(content);

        assert!(ast_contains_node_type(&ast, "heading"));
        assert!(ast_contains_node_type(&ast, "link"));
        assert!(!ast_contains_node_type(&ast, "table"));
    }

    #[test]
    fn test_extract_nodes_by_type() {
        let content = "# Heading 1\n\n## Heading 2\n\nSome text.";
        let ast = parse_markdown_ast(content);

        let headings = extract_nodes_by_type(&ast, "heading");
        assert_eq!(headings.len(), 2);
    }

    #[test]
    fn test_get_text_content() {
        let content = "Hello world";
        let ast = MarkdownAst::Text(markdown::mdast::Text {
            value: content.to_string(),
            position: None,
        });

        assert_eq!(get_text_content(&ast), content);
    }
}