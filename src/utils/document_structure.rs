use lazy_static::lazy_static;
use regex::Regex;
use std::ops::Range;
use crate::rules::code_block_utils::CodeBlockUtils;

/// A struct that contains pre-computed information about a markdown document structure
/// to avoid redundant parsing of the same elements by multiple rules.
#[derive(Debug, Clone)]
pub struct DocumentStructure {
    /// Information about code block regions
    pub code_blocks: Vec<CodeBlock>,
    /// Whether the document contains code blocks
    pub has_code_blocks: bool,
    /// Line numbers of headings (1-indexed)
    pub heading_lines: Vec<usize>,
    /// Heading levels (1-6) for each heading
    pub heading_levels: Vec<usize>,
    /// Line numbers of list items (1-indexed)
    pub list_lines: Vec<usize>,
    /// Whether the document contains front matter
    pub has_front_matter: bool,
    /// Line range of front matter (1-indexed, inclusive)
    pub front_matter_range: Option<(usize, usize)>,
    /// Whether the document contains URLs
    pub has_urls: bool,
    /// Whether the document contains inline HTML
    pub has_html: bool,
    /// Bitmap of code block regions for fast lookups
    pub in_code_block: Vec<bool>,
    /// Line numbers of fenced code block starts (1-indexed)
    pub fenced_code_block_starts: Vec<usize>,
    /// Line numbers of fenced code block ends (1-indexed)
    pub fenced_code_block_ends: Vec<usize>,
}

/// Front matter block
#[derive(Debug, Clone)]
pub struct FrontMatter {
    pub start_line: usize,
    pub end_line: usize,
    pub content: String,
}

/// Heading information
#[derive(Debug, Clone, PartialEq)]
pub struct Heading {
    pub text: String,
    pub level: u32,
    pub line_number: usize,
    pub original_text: String,
    pub indentation: String,
}

/// Simple code block representation for document structure
#[derive(Debug, Clone)]
pub struct CodeBlock {
    /// The line where the code block starts (1-indexed)
    pub start_line: usize,
    /// The line where the code block ends (1-indexed, inclusive)
    pub end_line: usize,
    /// Optional language specifier
    pub language: Option<String>,
    /// Type of code block (fenced or indented)
    pub block_type: CodeBlockType,
}

/// Type of code block
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CodeBlockType {
    /// Fenced code block with ``` or ~~~
    Fenced,
    /// Indented code block
    Indented,
}

/// List item information
#[derive(Debug, Clone)]
pub struct ListItem {
    pub line_number: usize,
    pub indentation: usize,
    pub marker: String,
    pub marker_type: ListMarkerType,
    pub content: String,
}

/// Type of list marker
#[derive(Debug, Clone, PartialEq)]
pub enum ListMarkerType {
    Unordered,
    Ordered,
    Task,
}

/// Blockquote range in the document
#[derive(Debug, Clone)]
pub struct BlockquoteRange {
    pub start_line: usize,
    pub end_line: usize,
}

/// Code block processing state
#[allow(dead_code)]
enum InternalCodeBlockState {
    None,
    InFenced,
    InIndented,
}

impl DocumentStructure {
    /// Create a new DocumentStructure by analyzing the document content
    pub fn new(content: &str) -> Self {
        // Initialize with default values
        let mut structure = DocumentStructure {
            code_blocks: Vec::new(),
            has_code_blocks: false,
            heading_lines: Vec::new(),
            heading_levels: Vec::new(),
            list_lines: Vec::new(),
            has_front_matter: false,
            front_matter_range: None,
            has_urls: false,
            has_html: false,
            in_code_block: Vec::new(),
            fenced_code_block_starts: Vec::new(),
            fenced_code_block_ends: Vec::new(),
        };

        // Analyze the document and populate the structure
        structure.analyze(content);
        structure
    }

    /// Analyze the document content and populate the structure
    fn analyze(&mut self, content: &str) {
        // Early return for empty content
        if content.is_empty() {
            return;
        }

        // Detect front matter FIRST (needed before heading detection)
        self.detect_front_matter(content);

        // Compute code blocks (needed for other analyses)
        self.code_blocks = self.compute_code_blocks(content);
        self.has_code_blocks = !self.code_blocks.is_empty();

        // Compute bitmap of code block regions
        self.compute_code_block_bitmap(content);
        
        // Populate fenced code block starts and ends
        self.populate_fenced_code_blocks();

        // Detect headings after front matter is processed
        self.detect_headings(content);

        // Detect lists
        self.detect_lists(content);

        // Check for URLs
        self.has_urls = content.contains("http://") || content.contains("https://") || 
                       content.contains("ftp://");

        // Check for HTML tags
        self.has_html = content.contains('<') && (content.contains("</") || content.contains("/>"));
    }

    /// Compute a bitmap of code block regions for fast lookups
    fn compute_code_block_bitmap(&mut self, content: &str) {
        let line_count = content.lines().count();
        self.in_code_block = vec![false; line_count];

        for block in &self.code_blocks {
            let start = block.start_line.saturating_sub(1); // Convert 1-indexed to 0-indexed
            let end = block.end_line.min(line_count); // Ensure we don't go out of bounds
            
            // For fenced code blocks, skip the start and end lines (the "```" lines)
            if let CodeBlockType::Fenced = block.block_type {
                // Mark only the lines between fences as in code block
                for i in (start + 1)..(end - 1) {
                    if i < self.in_code_block.len() {
                        self.in_code_block[i] = true;
                    }
                }
            } else {
                // For indented code blocks, mark all lines
                for i in start..end {
                    if i < self.in_code_block.len() {
                        self.in_code_block[i] = true;
                    }
                }
            }
        }
    }

    /// Check if a particular line is inside a code block
    pub fn is_in_code_block(&self, line_num: usize) -> bool {
        if line_num == 0 || line_num > self.in_code_block.len() {
            return false;
        }
        self.in_code_block[line_num - 1] // Convert 1-indexed to 0-indexed
    }

    /// Detect headings in the document
    fn detect_headings(&mut self, content: &str) {
        lazy_static! {
            static ref ATX_HEADING: Regex = Regex::new(r"^(\s*)(#{1,6})(\s+|[^\s#])").unwrap();
            static ref SETEXT_HEADING_UNDERLINE: Regex = Regex::new(r"^(\s*)(=+|-+)\s*$").unwrap();
        }

        // Clear existing data
        self.heading_lines.clear();
        self.heading_levels.clear();
        
        let lines: Vec<&str> = content.lines().collect();
        
        for (i, line) in lines.iter().enumerate() {
            // Skip lines in code blocks or front matter
            if self.is_in_code_block(i + 1) || self.is_in_front_matter(i + 1) {
                continue;
            }

            // Skip empty lines
            if line.trim().is_empty() {
                continue;
            }

            // Check for ATX headings (both with and without spaces)
            if let Some(captures) = ATX_HEADING.captures(line) {
                let level = captures[2].len();
                self.heading_lines.push(i + 1);
                self.heading_levels.push(level);
                continue;
            }

            // Check for setext headings (line with ===== or ----- below)
            if i > 0 && !lines[i - 1].trim().is_empty() &&
               !self.is_in_front_matter(i) && // Check that previous line is not in front matter
               SETEXT_HEADING_UNDERLINE.is_match(line) {
                let level = if line.trim().starts_with('=') { 1 } else { 2 };
                self.heading_lines.push(i); // The heading is the previous line
                self.heading_levels.push(level);
            }
        }
    }

    /// Detect lists in the document
    fn detect_lists(&mut self, content: &str) {
        lazy_static! {
            // Modified regex to better capture list markers with or without content after them
            static ref LIST_MARKER: Regex = Regex::new(r"^(\s*)([\*\+\-]|\d+\.)(\s+\S|\s*$)").unwrap();
        }

        // Clear existing data
        self.list_lines.clear();
        
        for (i, line) in content.lines().enumerate() {
            // Skip lines in code blocks or front matter
            if self.is_in_code_block(i + 1) || self.is_in_front_matter(i + 1) {
                continue;
            }

            // Check for list markers using the improved regex
            if LIST_MARKER.is_match(line) {
                self.list_lines.push(i + 1);
            }
        }
    }

    /// Detect front matter in the document
    fn detect_front_matter(&mut self, content: &str) {
        let lines: Vec<&str> = content.lines().collect();
        
        // Clear existing data
        self.has_front_matter = false;
        self.front_matter_range = None;
        
        // If document starts with ---, it might have front matter
        if !lines.is_empty() && lines[0] == "---" {
            // Look for the closing delimiter
            for (i, line) in lines.iter().enumerate().skip(1) {
                if *line == "---" {
                    self.has_front_matter = true;
                    self.front_matter_range = Some((1, i + 1));
                    break;
                }
            }
        }
    }

    /// Compute code blocks in the document
    fn compute_code_blocks(&self, content: &str) -> Vec<CodeBlock> {
        lazy_static! {
            static ref FENCED_START: Regex = Regex::new(r"^(\s*)(`{3,}|~{3,})\s*([^`\s]*)").unwrap();
            static ref FENCED_END: Regex = Regex::new(r"^(\s*)(`{3,}|~{3,})\s*$").unwrap();
        }
        
        let mut code_blocks = Vec::new();
        let mut in_code_block = false;
        let mut current_block_start = 0;
        let mut current_language = None;
        let mut current_fence_char = ' ';
        let lines: Vec<&str> = content.lines().collect();
        
        let mut i = 0;
        while i < lines.len() {
            let line = lines[i];
            
            if !in_code_block {
                // Check for fenced code block start
                if let Some(captures) = FENCED_START.captures(line) {
                    in_code_block = true;
                    current_block_start = i + 1;
                    current_fence_char = captures.get(2).map_or('`', |m| m.as_str().chars().next().unwrap());
                    
                    // Only set language if it's not empty
                    let lang = captures.get(3).map(|m| m.as_str().to_string());
                    current_language = if let Some(l) = lang {
                        if !l.is_empty() { Some(l) } else { None }
                    } else {
                        None
                    };
                }
                // Check for indented code block (simplified)
                else if line.starts_with("    ") && !line.trim().is_empty() {
                    // Find the end of the indented block
                    let mut end_line = i;
                    while end_line + 1 < lines.len() && 
                          (lines[end_line + 1].starts_with("    ") || 
                           lines[end_line + 1].trim().is_empty()) {
                        end_line += 1;
                    }
                    
                    code_blocks.push(CodeBlock {
                        start_line: i + 1,
                        end_line: end_line + 1,
                        language: None,
                        block_type: CodeBlockType::Indented,
                    });
                    
                    // Skip to end of block
                    i = end_line;
                }
            } else {
                // Check for fenced code block end - must start with the same fence character
                if FENCED_END.is_match(line) && line.trim().starts_with(current_fence_char) {
                    code_blocks.push(CodeBlock {
                        start_line: current_block_start,
                        end_line: i + 1,
                        language: current_language.clone(),
                        block_type: CodeBlockType::Fenced,
                    });
                    
                    in_code_block = false;
                    current_language = None;
                    current_fence_char = ' ';
                }
            }
            
            i += 1;
        }
        
        // Handle case where file ends without closing code fence
        if in_code_block {
            code_blocks.push(CodeBlock {
                start_line: current_block_start,
                end_line: lines.len(),
                language: current_language,
                block_type: CodeBlockType::Fenced,
            });
        }
        
        code_blocks
    }

    /// Populate fenced code block starts and ends
    fn populate_fenced_code_blocks(&mut self) {
        self.fenced_code_block_starts.clear();
        self.fenced_code_block_ends.clear();
        
        for block in &self.code_blocks {
            if let CodeBlockType::Fenced = block.block_type {
                self.fenced_code_block_starts.push(block.start_line);
                self.fenced_code_block_ends.push(block.end_line);
            }
        }
    }

    /// Check if a line is in front matter
    pub fn is_in_front_matter(&self, line_num: usize) -> bool {
        if let Some((start, end)) = self.front_matter_range {
            line_num >= start && line_num <= end
        } else {
            false
        }
    }

    /// Count the number of trailing spaces in a line
    /// 
    /// This function returns the number of trailing spaces in a line,
    /// ignoring newlines but counting spaces before newlines.
    #[inline]
    pub fn count_trailing_spaces(line: &str) -> usize {
        // Prepare the string without newline if it ends with one
        let content = if line.ends_with('\n') {
            &line[..line.len() - 1]
        } else {
            line
        };
        
        // Count trailing spaces at the end, not including tabs
        let mut space_count = 0;
        for c in content.chars().rev() {
            if c == ' ' {
                space_count += 1;
            } else {
                break;
            }
        }
        
        space_count
    }
    
    /// Check if a line has trailing whitespace
    /// 
    /// This function returns true if the line has trailing spaces,
    /// false otherwise.
    #[inline]
    pub fn has_trailing_spaces(line: &str) -> bool {
        Self::count_trailing_spaces(line) > 0
    }

    /// Get a list of list start indices
    /// This method analyzes the list_lines to find where lists begin
    pub fn get_list_start_indices(&self) -> Vec<usize> {
        if self.list_lines.is_empty() {
            return Vec::new();
        }

        let mut list_starts = Vec::new();
        let mut prev_line = 0;

        for (i, &line_num) in self.list_lines.iter().enumerate() {
            // If this is the first item or there's a gap in line numbers, 
            // it's the start of a new list
            if i == 0 || line_num > prev_line + 1 {
                list_starts.push(line_num - 1); // Convert from 1-indexed to 0-indexed
            }
            prev_line = line_num;
        }

        list_starts
    }

    /// Get a list of list end indices
    /// This method analyzes the list_lines to find where lists end
    pub fn get_list_end_indices(&self) -> Vec<usize> {
        if self.list_lines.is_empty() {
            return Vec::new();
        }

        let mut list_ends = Vec::new();
        let list_lines = &self.list_lines;

        for (i, &line_num) in list_lines.iter().enumerate() {
            // If this is the last item or there's a gap after this item,
            // it's the end of a list
            if i == list_lines.len() - 1 || list_lines[i + 1] > line_num + 1 {
                list_ends.push(line_num - 1); // Convert from 1-indexed to 0-indexed
            }
        }

        list_ends
    }
}

/// Extended rule trait methods for using the document structure
pub trait DocumentStructureExtensions {
    /// Check if a rule should operate on a given line
    fn should_process_line(&self, line_num: usize, doc_structure: &DocumentStructure) -> bool {
        // Skip lines in code blocks by default
        !doc_structure.is_in_code_block(line_num)
    }
    
    /// Check if content contains elements relevant to this rule
    fn has_relevant_elements(&self, _content: &str, _doc_structure: &DocumentStructure) -> bool {
        // Default implementation returns true - rules should override this
        true
    }
}

/// Create a DocumentStructure from a string
pub fn document_structure_from_str(content: &str) -> DocumentStructure {
    DocumentStructure::new(content)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_document_structure_creation() {
        let content = "# Heading 1\n\nSome text.\n\n## Heading 2\n\nMore text.\n\n```\nCode block\n```\n";
        let structure = DocumentStructure::new(content);
        
        assert_eq!(structure.heading_lines.len(), 2);
        assert_eq!(structure.heading_levels.len(), 2);
        assert!(structure.has_code_blocks);
        assert_eq!(structure.code_blocks.len(), 1);
    }

    #[test]
    fn test_document_with_front_matter() {
        let content = "---\ntitle: Test Document\ndate: 2021-01-01\n---\n\n# Heading 1\n\nSome text.\n";
        let structure = DocumentStructure::new(content);
        
        assert!(structure.has_front_matter);
        assert!(structure.front_matter_range.is_some());
        assert_eq!(structure.heading_lines.len(), 1);
        assert!(!structure.has_code_blocks);
    }

    #[test]
    fn test_is_in_code_block() {
        let content = "# Heading\n\nText.\n\n```\ncode line 1\ncode line 2\n```\n\nMore text.\n";
        let structure = DocumentStructure::new(content);
        
        assert!(!structure.is_in_code_block(1)); // # Heading
        assert!(!structure.is_in_code_block(3)); // Text.
        assert!(!structure.is_in_code_block(5)); // ```
        assert!(structure.is_in_code_block(6));  // code line 1
        assert!(structure.is_in_code_block(7));  // code line 2
        assert!(!structure.is_in_code_block(8)); // ```
        assert!(!structure.is_in_code_block(10)); // More text.
    }
} 
