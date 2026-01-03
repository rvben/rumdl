use crate::rules::heading_utils::HeadingStyle;
use fancy_regex::Regex as FancyRegex;
use std::sync::LazyLock;
use regex::Regex;

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
    /// Heading regions (start_line, end_line) for each heading (ATX: start==end, Setext: start=content, end=marker)
    pub heading_regions: Vec<(usize, usize)>,
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
    /// Style of the first heading found in the document (for consistent style rules)
    pub first_heading_style: Option<HeadingStyle>,
    /// OPTIMIZATION 1: Detailed information about inline code spans
    pub code_spans: Vec<CodeSpan>,
    /// OPTIMIZATION 1: Bitmap indicating which line-column positions are within code spans
    pub in_code_span: Vec<Vec<bool>>,
    /// OPTIMIZATION 2: Collection of links in the document
    pub links: Vec<Link>,
    /// OPTIMIZATION 2: Collection of images in the document
    pub images: Vec<Image>,
    /// OPTIMIZATION 3: Detailed information about list items
    pub list_items: Vec<ListItem>,
    /// OPTIMIZATION 4: Blockquotes in the document
    pub blockquotes: Vec<BlockquoteRange>,
    /// OPTIMIZATION 4: Bitmap indicating which lines are inside blockquotes
    pub in_blockquote: Vec<bool>,
    /// Bitmap indicating which lines are inside HTML blocks
    pub in_html_block: Vec<bool>,
    /// Line numbers of horizontal rules (1-indexed)
    pub horizontal_rule_lines: Vec<usize>,
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

/// OPTIMIZATION 1: Inline code span representation
#[derive(Debug, Clone)]
pub struct CodeSpan {
    /// The line number where the code span is (1-indexed)
    pub line: usize,
    /// Starting column of the code span (1-indexed)
    pub start_col: usize,
    /// Ending column of the code span (1-indexed)
    pub end_col: usize,
    /// The content of the code span (without the backticks)
    pub content: String,
}

/// OPTIMIZATION 2: Link representation
#[derive(Debug, Clone)]
pub struct Link {
    /// The line number where the link is (1-indexed)
    pub line: usize,
    /// Starting column of the link (1-indexed)
    pub start_col: usize,
    /// Ending column of the link (1-indexed)
    pub end_col: usize,
    /// The text displayed for the link
    pub text: String,
    /// The destination URL
    pub url: String,
    /// Whether this is a reference link [text][reference]
    pub is_reference: bool,
    /// The reference ID (for reference links)
    pub reference_id: Option<String>,
}

/// OPTIMIZATION 2: Image representation
#[derive(Debug, Clone)]
pub struct Image {
    /// The line number where the image is (1-indexed)
    pub line: usize,
    /// Starting column of the image (1-indexed)
    pub start_col: usize,
    /// Ending column of the image (1-indexed)
    pub end_col: usize,
    /// The alt text of the image
    pub alt_text: String,
    /// The source URL
    pub src: String,
    /// Whether this is a reference image ![text][reference]
    pub is_reference: bool,
    /// The reference ID (for reference images)
    pub reference_id: Option<String>,
}

// Cached regex patterns for performance
    // Quick check patterns
    static CONTAINS_ATX_HEADING: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?m)^(\s*)#{1,6}").unwrap());
    static CONTAINS_SETEXT_UNDERLINE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?m)^(\s*)(=+|-+)\s*$").unwrap());
    static CONTAINS_LIST_MARKERS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?m)^(\s*)([*+-]|\d+\.)").unwrap());
    static CONTAINS_BLOCKQUOTE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?m)^(\s*)>").unwrap());
    static CONTAINS_HTML_BLOCK: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?m)^(\s*)<[a-zA-Z]").unwrap());

impl DocumentStructure {
    /// Create a new DocumentStructure by analyzing the document content
    pub fn new(content: &str) -> Self {
        // Initialize with default values
        let mut structure = DocumentStructure {
            code_blocks: Vec::new(),
            has_code_blocks: false,
            heading_lines: Vec::new(),
            heading_levels: Vec::new(),
            heading_regions: Vec::new(),
            list_lines: Vec::new(),
            has_front_matter: false,
            front_matter_range: None,
            has_urls: false,
            has_html: false,
            in_code_block: Vec::new(),
            fenced_code_block_starts: Vec::new(),
            fenced_code_block_ends: Vec::new(),
            first_heading_style: None,
            // Initialize new optimization fields
            code_spans: Vec::new(),
            in_code_span: Vec::new(),
            links: Vec::new(),
            images: Vec::new(),
            list_items: Vec::new(),
            blockquotes: Vec::new(),
            in_blockquote: Vec::new(),
            in_html_block: Vec::new(),
            horizontal_rule_lines: Vec::new(),
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

        // Initialize line-based bitmaps early to avoid index errors
        let lines: Vec<&str> = content.lines().collect();
        self.in_code_span = vec![Vec::new(); lines.len()];
        for (i, line) in lines.iter().enumerate() {
            self.in_code_span[i] = vec![false; line.len() + 1]; // +1 for 1-indexed columns
        }
        self.in_blockquote = vec![false; lines.len()];
        self.in_html_block = vec![false; lines.len()];

        // Detect front matter FIRST (needed before heading detection)
        self.detect_front_matter(content);

        // Quick checks to skip expensive operations if not needed
        let has_blockquote_markers = CONTAINS_BLOCKQUOTE.is_match(content);
        let has_html_blocks = CONTAINS_HTML_BLOCK.is_match(content);

        // Detect HTML blocks BEFORE computing code blocks (HTML blocks should not be treated as indented code)
        if has_html_blocks {
            self.detect_html_blocks(content);
        }

        // Compute code blocks
        self.code_blocks = self.compute_code_blocks(content);
        self.has_code_blocks = !self.code_blocks.is_empty();

        // Compute bitmap of code block regions
        self.compute_code_block_bitmap(content);

        // Populate fenced code block starts and ends
        self.populate_fenced_code_blocks();
        let has_backticks = content.contains('`');
        let has_brackets = content.contains('[');
        let has_headings = CONTAINS_ATX_HEADING.is_match(content) || CONTAINS_SETEXT_UNDERLINE.is_match(content);
        // More comprehensive list detection to handle edge cases
        let has_list_markers = CONTAINS_LIST_MARKERS.is_match(content)
            || content.contains("- ")
            || content.contains("* ")
            || content.contains("+ ")
            || content.contains("1. ")
            || content.contains("2. ")
            || content.contains("3. ")
            || content.contains("4. ")
            || content.contains("5. ")
            || content.contains("6. ")
            || content.contains("7. ")
            || content.contains("8. ")
            || content.contains("9. ")
            || content.contains("10. ")
            || content.contains("11. ")
            || content.contains("12. ");

        // OPTIMIZATION 4: Detect blockquotes only if needed
        if has_blockquote_markers {
            self.detect_blockquotes(content);
        }

        // OPTIMIZATION 1: Detect inline code spans only if needed
        if has_backticks {
            self.detect_code_spans(content);
        }

        // OPTIMIZATION 2: Detect links and images only if needed
        if has_brackets {
            self.detect_links_and_images(content);
        }

        // Detect headings only if needed
        if has_headings {
            self.detect_headings(content);
        }

        // OPTIMIZATION 3: Detect lists only if needed
        if has_list_markers {
            self.detect_list_items(content);
        }

        // Detect horizontal rules only if needed
        let has_potential_hrs = content.contains("---")
            || content.contains("***")
            || content.contains("___")
            || content.contains("- -")
            || content.contains("* *")
            || content.contains("_ _");
        if has_potential_hrs {
            self.detect_horizontal_rules(content);
        }

        // Check for URLs only if needed
        if crate::utils::early_returns::has_urls(content) {
            self.has_urls = true;
        }

        // Check for HTML tags only if needed
        if has_html_blocks && (content.contains("</") || content.contains("/>")) {
            self.has_html = true;
        }
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
                if end > start + 1 {
                    for i in (start + 1)..(end - 1) {
                        if i < self.in_code_block.len() {
                            self.in_code_block[i] = true;
                        }
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
                    static ATX_HEADING: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^(\s*)(#{1,6})(\s+|[^\s#])").unwrap());
            static SETEXT_HEADING_UNDERLINE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^(\s*)(=+|-+)\s*$").unwrap());

        // Clear existing data
        self.heading_lines.clear();
        self.heading_levels.clear();
        self.heading_regions.clear();
        self.first_heading_style = None;

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
                // Extract heading text after hashes and whitespace
                let mut chars = line.trim().chars();
                while chars.next() == Some('#') {}
                let heading_text = chars.as_str().trim();
                if heading_text.is_empty() {
                    continue; // Skip empty ATX headings
                }
                self.heading_lines.push(i + 1);
                self.heading_levels.push(level);
                self.heading_regions.push((i + 1, i + 1)); // ATX: start==end

                // If this is the first heading detected, set the style
                if self.first_heading_style.is_none() {
                    // Determine if it's a closed ATX heading
                    if line.trim().ends_with('#') {
                        self.first_heading_style = Some(HeadingStyle::AtxClosed);
                    } else {
                        self.first_heading_style = Some(HeadingStyle::Atx);
                    }
                }
                continue;
            }

            // Check for setext headings (line with ===== or ----- below)
            if i > 0 && !lines[i - 1].trim().is_empty() &&
               !self.is_in_front_matter(i) && // Check that previous line is not in front matter
               SETEXT_HEADING_UNDERLINE.is_match(line)
            {
                let content_line = lines[i - 1].trim();
                if content_line.is_empty() {
                    continue; // Skip empty Setext headings
                }

                // Per CommonMark spec 4.3, setext heading content cannot be interpretable as:
                // list item, ATX heading, block quote, thematic break, code fence, or HTML block

                // Skip list items (-, *, +) and thematic breaks (---, ***, etc.)
                if content_line.starts_with('-')
                    || content_line.starts_with('*')
                    || content_line.starts_with('+')
                {
                    continue;
                }

                // Skip underscore thematic breaks (___)
                if content_line.starts_with('_') {
                    let non_ws: String = content_line.chars().filter(|c| !c.is_whitespace()).collect();
                    if non_ws.len() >= 3 && non_ws.chars().all(|c| c == '_') {
                        continue;
                    }
                }

                // Skip numbered lists (1. Item, 2. Item, etc.)
                if let Some(first_char) = content_line.chars().next() {
                    if first_char.is_ascii_digit() {
                        let num_end = content_line
                            .chars()
                            .take_while(|c| c.is_ascii_digit())
                            .count();
                        if num_end < content_line.len() {
                            let next = content_line.chars().nth(num_end);
                            if next == Some('.') || next == Some(')') {
                                continue;
                            }
                        }
                    }
                }

                // Skip ATX headings
                if ATX_HEADING.is_match(lines[i - 1]) {
                    continue;
                }

                // Skip blockquotes
                if content_line.starts_with('>') {
                    continue;
                }

                // Skip code fences
                let trimmed_start = lines[i - 1].trim_start();
                if trimmed_start.len() >= 3 {
                    let first_three: String = trimmed_start.chars().take(3).collect();
                    if first_three == "```" || first_three == "~~~" {
                        continue;
                    }
                }

                // Skip HTML blocks
                if content_line.starts_with('<') {
                    continue;
                }

                let level = if line.trim().starts_with('=') { 1 } else { 2 };
                self.heading_lines.push(i); // The heading is the previous line (content line)
                self.heading_levels.push(level);
                self.heading_regions.push((i, i + 1)); // Setext: (content, marker)

                // If this is the first heading detected, set the style
                if self.first_heading_style.is_none() {
                    if level == 1 {
                        self.first_heading_style = Some(HeadingStyle::Setext1);
                    } else {
                        self.first_heading_style = Some(HeadingStyle::Setext2);
                    }
                }
            }
        }

        // Default to ATX if no headings are found
        if self.heading_lines.is_empty() {
            self.first_heading_style = Some(HeadingStyle::Atx);
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
                    // Fenced code blocks can be indented 0-3 spaces according to CommonMark
            static FENCED_START: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^(\s{0,3})(`{3,}|~{3,})\s*([^`\s]*)").unwrap());
            static FENCED_END: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^(\s{0,3})(`{3,}|~{3,})\s*$").unwrap());

        let mut code_blocks = Vec::new();
        let mut in_code_block = false;
        let mut current_block_start = 0;
        let mut current_language = None;
        let mut current_fence_char = ' ';
        let mut current_fence_length = 0; // Track fence length for proper nesting
        let mut current_fence_indent = 0; // Track fence indentation
        let lines: Vec<&str> = content.lines().collect();

        let mut i = 0;
        while i < lines.len() {
            let line = lines[i];

            if !in_code_block {
                // Check for fenced code block start
                if let Some(captures) = FENCED_START.captures(line) {
                    in_code_block = true;
                    current_block_start = i + 1;
                    let indent = captures.get(1).map_or("", |m| m.as_str());
                    current_fence_indent = indent.len();
                    let fence = captures.get(2).map_or("```", |m| m.as_str());
                    current_fence_char = fence.chars().next().unwrap();
                    current_fence_length = fence.len();

                    // Only set language if it's not empty
                    let lang = captures.get(3).map(|m| m.as_str().to_string());
                    current_language = lang.filter(|l| !l.is_empty());
                }
                // Check for indented code block (CommonMark compliant)
                // But skip if we're inside an HTML block OR if it's a list item
                // According to CommonMark, list items take precedence over indented code blocks
                else if Self::is_indented_code_line(line)
                    && !line.trim().is_empty()
                    && !self.is_in_html_block(i + 1)
                    && !Self::is_potential_list_item(line)
                {
                    // According to CommonMark, any content indented by 4+ spaces OR a tab is a code block
                    // unless it's inside an HTML block or it's a list item
                    let mut end_line = i;

                    // Find the end of this indented code block
                    // Continue while we have indented lines OR blank lines that are followed by more indented lines
                    while end_line + 1 < lines.len() {
                        let next_line = lines[end_line + 1];

                        if Self::is_indented_code_line(next_line)
                            && !next_line.trim().is_empty()
                            && !self.is_in_html_block(end_line + 2)
                            && !Self::is_potential_list_item(next_line)
                        {
                            // Found another indented line that's not in HTML or a list item, continue the block
                            end_line += 1;
                        } else if next_line.trim().is_empty() {
                            // Found a blank line, check if there are more indented lines after it
                            let mut lookahead = end_line + 2;
                            let mut found_indented = false;

                            while lookahead < lines.len() {
                                let lookahead_line = lines[lookahead];
                                if Self::is_indented_code_line(lookahead_line)
                                    && !lookahead_line.trim().is_empty()
                                    && !self.is_in_html_block(lookahead + 1)
                                    && !Self::is_potential_list_item(lookahead_line)
                                {
                                    found_indented = true;
                                    break;
                                } else if !lookahead_line.trim().is_empty() {
                                    // Found non-empty, non-indented line, stop looking
                                    break;
                                }
                                lookahead += 1;
                            }

                            if found_indented {
                                // Include this blank line as part of the code block
                                end_line += 1;
                            } else {
                                // No more indented lines, end the block here
                                break;
                            }
                        } else {
                            // Found non-empty, non-indented line, end the block
                            break;
                        }
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
                // Check for fenced code block end - must start with the same fence character,
                // be at least as long as the opening fence, and have same or less indentation
                if let Some(captures) = FENCED_END.captures(line) {
                    let indent = captures.get(1).map_or("", |m| m.as_str());
                    let fence = captures.get(2).map_or("", |m| m.as_str());

                    // CommonMark: closing fence must have same or less indentation than opening
                    if fence.starts_with(current_fence_char)
                        && fence.len() >= current_fence_length
                        && indent.len() <= current_fence_indent
                    {
                        code_blocks.push(CodeBlock {
                            start_line: current_block_start,
                            end_line: i + 1,
                            language: current_language.clone(),
                            block_type: CodeBlockType::Fenced,
                        });

                        in_code_block = false;
                        current_language = None;
                        current_fence_char = ' ';
                        current_fence_length = 0;
                        current_fence_indent = 0;
                    }
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
        let content = line.strip_suffix('\n').unwrap_or(line);

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

    /// Check if a line is indented code according to CommonMark specification
    ///
    /// According to CommonMark, a line is considered indented code if it starts with:
    /// - 4 or more spaces, OR
    /// - A tab character
    #[inline]
    fn is_indented_code_line(line: &str) -> bool {
        if line.starts_with('\t') {
            return true;
        }

        // Count leading spaces
        let mut space_count = 0;
        for c in line.chars() {
            if c == ' ' {
                space_count += 1;
            } else {
                break;
            }
        }

        space_count >= 4
    }

    /// Check if a line is potentially a list item
    /// This is used to prevent list items from being detected as indented code blocks
    #[inline]
    fn is_potential_list_item(line: &str) -> bool {
                    // Simple regex to detect potential list items
            // Matches lines that start with optional whitespace followed by a list marker
            static LIST_ITEM_PATTERN: LazyLock<Regex> = LazyLock::new(|| Regex::new(
                r"^[ \t]*([*+-]|\d+[.)]])[ \t]"
            ).unwrap());
        }
        LIST_ITEM_PATTERN.is_match(line)
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

    /// OPTIMIZATION 1: Detect inline code spans in the document
    fn detect_code_spans(&mut self, content: &str) {
        // Clear existing data
        self.code_spans.clear();

        let lines: Vec<&str> = content.lines().collect();

        // Note: in_code_span bitmap is already initialized in analyze() method

        for (line_num, line) in lines.iter().enumerate() {
            // Skip lines in code blocks
            if self.is_in_code_block(line_num + 1) {
                continue;
            }

            // Skip empty lines
            if line.is_empty() {
                continue;
            }

            let mut i = 0;
            while i < line.len() {
                // Look for backtick
                if let Some(start_pos) = line[i..].find('`') {
                    let start_idx = i + start_pos;

                    // Look for closing backtick
                    if let Some(end_pos) = line[start_idx + 1..].find('`') {
                        let end_idx = start_idx + 1 + end_pos;

                        // We found a code span
                        let content = line[start_idx + 1..end_idx].to_string();

                        // Add to code_spans collection
                        self.code_spans.push(CodeSpan {
                            line: line_num + 1,       // 1-indexed
                            start_col: start_idx + 1, // 1-indexed
                            end_col: end_idx + 1,     // 1-indexed
                            content,
                        });

                        // Mark in the bitmap
                        for col in start_idx..=end_idx {
                            if col < self.in_code_span[line_num].len() {
                                self.in_code_span[line_num][col] = true;
                            }
                        }

                        // Continue from after the closing backtick
                        i = end_idx + 1;
                    } else {
                        // No closing backtick found
                        i = start_idx + 1;
                    }
                } else {
                    // No more backticks in this line
                    break;
                }
            }
        }
    }

    /// OPTIMIZATION 2: Detect links and images in the document
    fn detect_links_and_images(&mut self, content: &str) {
                    // Regex for inline links: [text](url) - handles escaped brackets
            static INLINE_LINK: LazyLock<FancyRegex> = LazyLock::new(|| FancyRegex::new(r"(?x)
                (?<!\\)                               # Not preceded by backslash
                \[((?:[^\[\]\\]|\\.|\[[^\]]*\])*)\]  # Link text (handles nested brackets and escapes)
                \(([^)]*)\)                           # URL in parentheses
            ").unwrap());
            // Regex for reference links: [text][id] or [text][] (implicit) - handles escaped brackets
            static REFERENCE_LINK: LazyLock<FancyRegex> = LazyLock::new(|| FancyRegex::new(r"(?x)
                (?<!\\)                               # Not preceded by backslash
                \[((?:[^\[\]\\]|\\.|\[[^\]]*\])*)\]  # Link text (handles nested brackets and escapes)
                \[([^\]]*)\]                          # Reference ID
            ").unwrap());
            // Regex for shortcut reference links: [text]
            static SHORTCUT_LINK: LazyLock<FancyRegex> = LazyLock::new(|| FancyRegex::new(r"(?x)
                (?<!\\)                               # Not preceded by backslash
                \[([^\]]+)\]                          # Link text
                (?!\(|\[)                             # Not followed by ( or [
            ").unwrap());
            // Regex for link definitions: [id]: url
            static LINK_DEFINITION: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\s*\[([^\]]+)\]:\s+(.+)$").unwrap());
            // Regex for inline images: ![alt](src) - handles escaped brackets
            static INLINE_IMAGE: LazyLock<FancyRegex> = LazyLock::new(|| FancyRegex::new(r"(?x)
                (?<!\\)                               # Not preceded by backslash
                !\[((?:[^\[\]\\]|\\.|\[[^\]]*\])*)\] # Alt text (handles nested brackets and escapes)
                \(([^)]*)\)                           # Source URL
            ").unwrap());
            // Regex for reference images: ![alt][id] - handles escaped brackets
            static REFERENCE_IMAGE: LazyLock<FancyRegex> = LazyLock::new(|| FancyRegex::new(r"(?x)
                (?<!\\)                               # Not preceded by backslash
                !\[((?:[^\[\]\\]|\\.|\[[^\]]*\])*)\] # Alt text (handles nested brackets and escapes)
                \[([^\]]*)\]                          # Reference ID
            ").unwrap());
        }

        // Clear existing data
        self.links.clear();
        self.images.clear();

        let lines: Vec<&str> = content.lines().collect();

        // First, find all link definitions
        let mut link_defs = std::collections::HashMap::new();
        for (line_num, line) in lines.iter().enumerate() {
            // Skip lines in code blocks
            if self.is_in_code_block(line_num + 1) {
                continue;
            }

            // Check for link definitions
            if let Some(cap) = LINK_DEFINITION.captures(line) {
                let id = cap.get(1).map_or("", |m| m.as_str()).to_string();
                let url = cap.get(2).map_or("", |m| m.as_str()).to_string();
                link_defs.insert(id.to_lowercase(), url);
            }
        }

        // Now find all links and images
        for (line_num, line) in lines.iter().enumerate() {
            // Skip lines in code blocks
            if self.is_in_code_block(line_num + 1) {
                continue;
            }

            // Skip empty lines
            if line.is_empty() {
                continue;
            }

            // Check if this line contains a character that would indicate a link or image
            if !line.contains('[') && !line.contains('!') {
                continue;
            }

            // Process each character position to ensure we don't detect links inside code spans
            let mut i = 0;
            while i < line.len() {
                // Skip if this position is in a code span
                if i < self.in_code_span[line_num].len() && self.in_code_span[line_num][i] {
                    i += 1;
                    continue;
                }

                // Check for inline links starting at this position
                if let Some(rest) = line.get(i..) {
                    if rest.starts_with('[') {
                        // Check if this bracket is escaped or part of an escaped image
                        let is_escaped = i > 0 && line.chars().nth(i - 1) == Some('\\');
                        let is_escaped_image =
                            i > 1 && line.chars().nth(i - 2) == Some('\\') && line.chars().nth(i - 1) == Some('!');
                        if !is_escaped && !is_escaped_image {
                            if let Ok(Some(cap)) = INLINE_LINK.captures(rest) {
                                let whole_match = cap.get(0).unwrap();
                                let text = cap.get(1).map_or("", |m| m.as_str()).to_string();
                                let url = cap.get(2).map_or("", |m| m.as_str()).to_string();

                                // Ensure we're not inside a code span
                                let is_in_span = (i..i + whole_match.end()).any(|pos| {
                                    pos < self.in_code_span[line_num].len() && self.in_code_span[line_num][pos]
                                });

                                if !is_in_span {
                                    self.links.push(Link {
                                        line: line_num + 1,             // 1-indexed
                                        start_col: i + 1,               // 1-indexed
                                        end_col: i + whole_match.end(), // 1-indexed
                                        text,
                                        url,
                                        is_reference: false,
                                        reference_id: None,
                                    });
                                }

                                // Skip past this link
                                i += whole_match.end();
                            } else if let Ok(Some(cap)) = REFERENCE_LINK.captures(rest) {
                                let whole_match = cap.get(0).unwrap();
                                let text = cap.get(1).map_or("", |m| m.as_str()).to_string();
                                let id = cap.get(2).map_or("", |m| m.as_str()).to_string();

                                // Use the ID or text as the reference
                                let ref_id = if id.is_empty() { text.clone() } else { id };

                                // Look up the URL from link definitions
                                let url = link_defs.get(&ref_id.to_lowercase()).cloned().unwrap_or_default();

                                // Ensure we're not inside a code span
                                let is_in_span = (i..i + whole_match.end()).any(|pos| {
                                    pos < self.in_code_span[line_num].len() && self.in_code_span[line_num][pos]
                                });

                                if !is_in_span {
                                    self.links.push(Link {
                                        line: line_num + 1,             // 1-indexed
                                        start_col: i + 1,               // 1-indexed
                                        end_col: i + whole_match.end(), // 1-indexed
                                        text,
                                        url,
                                        is_reference: true,
                                        reference_id: Some(ref_id),
                                    });
                                }

                                // Skip past this link
                                i += whole_match.end();
                            } else {
                                // No match found, move to next character
                                i += 1;
                            }
                        } else {
                            // Bracket is escaped or part of escaped image, skip it
                            i += 1;
                        }
                    } else if rest.starts_with("![") {
                        // Check if this image is escaped
                        let is_escaped = i > 0 && line.chars().nth(i - 1) == Some('\\');
                        if !is_escaped {
                            if let Ok(Some(cap)) = INLINE_IMAGE.captures(rest) {
                                let whole_match = cap.get(0).unwrap();
                                let alt_text = cap.get(1).map_or("", |m| m.as_str()).to_string();
                                let src = cap.get(2).map_or("", |m| m.as_str()).to_string();

                                // Ensure we're not inside a code span
                                let is_in_span = (i..i + whole_match.end()).any(|pos| {
                                    pos < self.in_code_span[line_num].len() && self.in_code_span[line_num][pos]
                                });

                                if !is_in_span {
                                    self.images.push(Image {
                                        line: line_num + 1,             // 1-indexed
                                        start_col: i + 1,               // 1-indexed
                                        end_col: i + whole_match.end(), // 1-indexed
                                        alt_text,
                                        src,
                                        is_reference: false,
                                        reference_id: None,
                                    });
                                }

                                // Skip past this image
                                i += whole_match.end();
                            } else if let Ok(Some(cap)) = REFERENCE_IMAGE.captures(rest) {
                                let whole_match = cap.get(0).unwrap();
                                let alt_text = cap.get(1).map_or("", |m| m.as_str()).to_string();
                                let id = cap.get(2).map_or("", |m| m.as_str()).to_string();

                                // Use the ID or alt_text as the reference
                                let ref_id = if id.is_empty() { alt_text.clone() } else { id };

                                // Look up the URL from link definitions
                                let src = link_defs.get(&ref_id.to_lowercase()).cloned().unwrap_or_default();

                                // Ensure we're not inside a code span
                                let is_in_span = (i..i + whole_match.end()).any(|pos| {
                                    pos < self.in_code_span[line_num].len() && self.in_code_span[line_num][pos]
                                });

                                if !is_in_span {
                                    self.images.push(Image {
                                        line: line_num + 1,             // 1-indexed
                                        start_col: i + 1,               // 1-indexed
                                        end_col: i + whole_match.end(), // 1-indexed
                                        alt_text,
                                        src,
                                        is_reference: true,
                                        reference_id: Some(ref_id),
                                    });
                                }

                                // Skip past this image
                                i += whole_match.end();
                            } else {
                                // No match found, move to next character
                                i += 1;
                            }
                        } else {
                            // Image is escaped, skip it
                            i += 1;
                        }
                    } else {
                        // Neither a link nor an image, move to next character
                        i += 1;
                    }
                } else {
                    // We've reached the end of the line
                    break;
                }
            }
        }
    }

    /// OPTIMIZATION 3: Detect list items with detailed information
    fn detect_list_items(&mut self, content: &str) {
        // Use fancy-regex for advanced Markdown list item detection
        // - Allow any number of spaces/tabs before the marker
        // - Marker must be *, +, or -
        // - At least one space/tab after the marker
        // - Use lookbehind to ensure marker is at the start or after whitespace
        // - Use Unicode support for whitespace
                    static ref UL_MARKER: FancyRegex =
                FancyRegex::new(r"^(?P<indent>[ \t]*)(?P<marker>[*+-])(?P<after>[ \t]+)(?P<content>.*)$").unwrap();
            static ref OL_MARKER: FancyRegex =
                FancyRegex::new(r"^(?P<indent>[ \t]*)(?P<marker>\d+\.)(?P<after>[ \t]+)(?P<content>.*)$").unwrap();
            static TASK_MARKER: LazyLock<FancyRegex> = LazyLock::new(|| FancyRegex::new(
                r"^(?P<indent>[ \t]*)(?P<marker>[*+-])(?P<after>[ \t]+)\[(?P<checked>[ xX])\](?P<content>.*)$"
            )
            .unwrap());
        }
        self.list_items.clear();
        self.list_lines.clear();
        let lines: Vec<&str> = content.lines().collect();
        for (line_num, line) in lines.iter().enumerate() {
            if self.is_in_code_block(line_num + 1) || self.is_in_front_matter(line_num + 1) {
                continue;
            }
            if line.trim().is_empty() {
                continue;
            }
            // Use fancy-regex for advanced matching
            if let Ok(Some(cap)) = TASK_MARKER.captures(line) {
                let indentation = cap.name("indent").map_or(0, |m| m.as_str().len());
                let marker = cap.name("marker").map_or("", |m| m.as_str()).to_string();
                let content = cap.name("content").map_or("", |m| m.as_str()).to_string();
                self.list_lines.push(line_num + 1);
                self.list_items.push(ListItem {
                    line_number: line_num + 1,
                    indentation,
                    marker: marker.clone(),
                    marker_type: ListMarkerType::Task,
                    content,
                });
                continue;
            }
            if let Ok(Some(cap)) = UL_MARKER.captures(line) {
                let indentation = cap.name("indent").map_or(0, |m| m.as_str().len());
                let marker = cap.name("marker").map_or("", |m| m.as_str()).to_string();
                let content = cap.name("content").map_or("", |m| m.as_str()).to_string();
                self.list_lines.push(line_num + 1);
                self.list_items.push(ListItem {
                    line_number: line_num + 1,
                    indentation,
                    marker: marker.clone(),
                    marker_type: ListMarkerType::Unordered,
                    content,
                });
                continue;
            }
            if let Ok(Some(cap)) = OL_MARKER.captures(line) {
                let indentation = cap.name("indent").map_or(0, |m| m.as_str().len());
                let marker = cap.name("marker").map_or("", |m| m.as_str()).to_string();
                let content = cap.name("content").map_or("", |m| m.as_str()).to_string();
                self.list_lines.push(line_num + 1);
                self.list_items.push(ListItem {
                    line_number: line_num + 1,
                    indentation,
                    marker: marker.clone(),
                    marker_type: ListMarkerType::Ordered,
                    content,
                });
                continue;
            }
        }
    }

    /// OPTIMIZATION 4: Detect blockquotes in the document
    fn detect_blockquotes(&mut self, content: &str) {
                    static BLOCKQUOTE_MARKER: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\s*>(.*)$").unwrap());

        // Clear existing data
        self.blockquotes.clear();

        let lines: Vec<&str> = content.lines().collect();

        // Note: in_blockquote bitmap is already initialized in analyze() method

        let mut in_blockquote = false;
        let mut start_line = 0;

        for (i, line) in lines.iter().enumerate() {
            // Skip lines in code blocks or front matter
            if self.is_in_code_block(i + 1) || self.is_in_front_matter(i + 1) {
                continue;
            }

            let is_blockquote_line = BLOCKQUOTE_MARKER.is_match(line);

            if is_blockquote_line {
                // Mark this line as inside a blockquote
                self.in_blockquote[i] = true;

                if !in_blockquote {
                    // Start of a new blockquote
                    in_blockquote = true;
                    start_line = i + 1; // 1-indexed
                }
            } else if in_blockquote {
                // End of a blockquote
                self.blockquotes.push(BlockquoteRange {
                    start_line,
                    end_line: i, // Previous line was the end
                });

                in_blockquote = false;
            }
        }

        // Handle case where file ends with a blockquote
        if in_blockquote {
            self.blockquotes.push(BlockquoteRange {
                start_line,
                end_line: lines.len(), // Last line
            });
        }
    }

    /// Detect horizontal rules in the document
    fn detect_horizontal_rules(&mut self, content: &str) {
                    // Horizontal rule patterns - simplified to match Markdown spec
            static HR_HYPHEN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[ \t]*-[ \t]*-[ \t]*-[ \t-]*$").unwrap());
            static HR_ASTERISK: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[ \t]*\*[ \t]*\*[ \t]*\*[ \t\*]*$").unwrap());
            static HR_UNDERSCORE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[ \t]*_[ \t]*_[ \t]*_[ \t_]*$").unwrap());

        // Clear existing data
        self.horizontal_rule_lines.clear();

        let lines: Vec<&str> = content.lines().collect();

        for (i, line) in lines.iter().enumerate() {
            // Skip lines in code blocks or front matter
            if self.is_in_code_block(i + 1) || self.is_in_front_matter(i + 1) {
                continue;
            }

            // Check for horizontal rule patterns
            if HR_HYPHEN.is_match(line) || HR_ASTERISK.is_match(line) || HR_UNDERSCORE.is_match(line) {
                // Additional validation: ensure it's not part of a setext heading
                // (setext headings have content on the previous line)
                let is_setext_marker = if i > 0 {
                    let prev_line = lines[i - 1].trim();
                    !prev_line.is_empty()
                        && !self.is_in_code_block(i)
                        && !self.is_in_front_matter(i)
                        && line.trim().chars().all(|c| c == '-' || c == ' ')
                } else {
                    false
                };

                if !is_setext_marker {
                    self.horizontal_rule_lines.push(i + 1); // 1-indexed
                }
            }
        }
    }

    /// Detect HTML blocks (block-level HTML regions) according to CommonMark spec
    fn detect_html_blocks(&mut self, content: &str) {
        let lines: Vec<&str> = content.lines().collect();
        // Note: in_html_block bitmap is already initialized in analyze() method

        let mut i = 0;
        while i < lines.len() {
            let line = lines[i];
            let trimmed = line.trim_start();

            // Skip lines already in code blocks
            if self.is_in_code_block(i + 1) {
                i += 1;
                continue;
            }

            // Check for HTML block start conditions (simplified version of CommonMark)
            if self.is_html_block_start(trimmed) {
                let start_line = i;

                // Find the end of the HTML block
                let end_line = self.find_html_block_end(&lines, start_line);

                // Mark all lines in the block as HTML
                for line_idx in start_line..=end_line {
                    if line_idx < self.in_html_block.len() {
                        self.in_html_block[line_idx] = true;
                    }
                }

                // Skip to after the block
                i = end_line + 1;
            } else {
                i += 1;
            }
        }
    }

    /// Check if a line starts an HTML block
    fn is_html_block_start(&self, trimmed: &str) -> bool {
        if trimmed.is_empty() || !trimmed.starts_with('<') {
            return false;
        }

        // Extract tag name
        let mut chars = trimmed[1..].chars();
        let mut tag_name = String::new();

        // Handle closing tags
        let is_closing = chars.as_str().starts_with('/');
        if is_closing {
            chars.next(); // Skip the '/'
        }

        // Extract tag name
        for ch in chars {
            if ch.is_ascii_alphabetic() || ch == '-' {
                tag_name.push(ch);
            } else {
                break;
            }
        }

        if tag_name.is_empty() {
            return false;
        }

        // List of HTML block elements (based on CommonMark and markdownlint)
        const BLOCK_ELEMENTS: &[&str] = &[
            "address",
            "article",
            "aside",
            "base",
            "basefont",
            "blockquote",
            "body",
            "caption",
            "center",
            "col",
            "colgroup",
            "dd",
            "details",
            "dialog",
            "dir",
            "div",
            "dl",
            "dt",
            "fieldset",
            "figcaption",
            "figure",
            "footer",
            "form",
            "frame",
            "frameset",
            "h1",
            "h2",
            "h3",
            "h4",
            "h5",
            "h6",
            "head",
            "header",
            "hr",
            "html",
            "iframe",
            "legend",
            "li",
            "link",
            "main",
            "menu",
            "menuitem",
            "nav",
            "noframes",
            "ol",
            "optgroup",
            "option",
            "p",
            "param",
            "section",
            "source",
            "summary",
            "table",
            "tbody",
            "td",
            "tfoot",
            "th",
            "thead",
            "title",
            "tr",
            "track",
            "ul",
            "img",
            "picture",
        ];

        BLOCK_ELEMENTS.contains(&tag_name.to_ascii_lowercase().as_str())
    }

    /// Find the end line of an HTML block starting at start_line
    fn find_html_block_end(&self, lines: &[&str], start_line: usize) -> usize {
        let start_trimmed = lines[start_line].trim_start();

        // Extract the tag name from the start line
        let tag_name = self.extract_tag_name(start_trimmed);

        // Look for the closing tag or blank line
        for (i, line) in lines.iter().enumerate().skip(start_line + 1) {
            let trimmed = line.trim();

            // HTML block ends on blank line
            if trimmed.is_empty() {
                return i - 1; // Don't include the blank line
            }

            // HTML block ends when we find the matching closing tag
            if let Some(ref tag) = tag_name {
                let closing_tag = format!("</{tag}");
                if trimmed.contains(&closing_tag) {
                    return i;
                }
            }
        }

        // If no end found, block continues to end of document
        lines.len() - 1
    }

    /// Extract tag name from an HTML line
    fn extract_tag_name(&self, trimmed: &str) -> Option<String> {
        if !trimmed.starts_with('<') {
            return None;
        }

        let mut chars = trimmed[1..].chars();

        // Skip closing tag indicator
        if chars.as_str().starts_with('/') {
            chars.next();
        }

        let mut tag_name = String::new();
        for ch in chars {
            if ch.is_ascii_alphabetic() || ch == '-' {
                tag_name.push(ch);
            } else {
                break;
            }
        }

        if tag_name.is_empty() {
            None
        } else {
            Some(tag_name.to_ascii_lowercase())
        }
    }

    /// Check if a position is inside a code span
    pub fn is_in_code_span(&self, line_num: usize, col: usize) -> bool {
        if line_num == 0 || line_num > self.in_code_span.len() {
            return false;
        }

        let line_idx = line_num - 1; // Convert 1-indexed to 0-indexed

        if col == 0 || col > self.in_code_span[line_idx].len() {
            return false;
        }

        self.in_code_span[line_idx][col - 1] // Convert 1-indexed to 0-indexed
    }

    /// Check if a line is inside a blockquote
    pub fn is_in_blockquote(&self, line_num: usize) -> bool {
        if line_num == 0 || line_num > self.in_blockquote.len() {
            return false;
        }

        self.in_blockquote[line_num - 1] // Convert 1-indexed to 0-indexed
    }

    /// Get detailed information about a list item at a specific line
    pub fn get_list_item_at_line(&self, line_num: usize) -> Option<&ListItem> {
        self.list_items.iter().find(|item| item.line_number == line_num)
    }

    /// Get all list items with a specific marker type
    pub fn get_list_items_by_type(&self, marker_type: ListMarkerType) -> Vec<&ListItem> {
        self.list_items
            .iter()
            .filter(|item| item.marker_type == marker_type)
            .collect()
    }

    /// Get all links with empty text or URLs
    pub fn get_empty_links(&self) -> Vec<&Link> {
        self.links
            .iter()
            .filter(|link| link.text.trim().is_empty() || link.url.trim().is_empty())
            .collect()
    }

    /// Get all images with empty alt text
    pub fn get_images_without_alt_text(&self) -> Vec<&Image> {
        self.images
            .iter()
            .filter(|img| img.alt_text.trim().is_empty())
            .collect()
    }

    /// Check if a line is inside an HTML block
    pub fn is_in_html_block(&self, line_num: usize) -> bool {
        if line_num == 0 || line_num > self.in_html_block.len() {
            return false;
        }
        self.in_html_block[line_num - 1]
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
    fn has_relevant_elements(
        &self,
        _ctx: &crate::lint_context::LintContext,
        _doc_structure: &DocumentStructure,
    ) -> bool {
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
    fn test_nested_code_blocks() {
        let content = r#"```markdown
1. First item

   ```python
   code_in_list()
   ```

2. Second item
```"#;

        let structure = DocumentStructure::new(content);

        // Should have exactly one code block (the outer markdown block)
        assert_eq!(structure.code_blocks.len(), 1);
        assert_eq!(structure.code_blocks[0].start_line, 1);
        assert_eq!(structure.code_blocks[0].end_line, 9);

        // Lines 2-8 should be inside the code block
        for line in 2..=8 {
            assert!(structure.is_in_code_block(line), "Line {line} should be in code block");
        }
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
        assert!(structure.is_in_code_block(6)); // code line 1
        assert!(structure.is_in_code_block(7)); // code line 2
        assert!(!structure.is_in_code_block(8)); // ```
        assert!(!structure.is_in_code_block(10)); // More text.
    }

    #[test]
    fn test_headings_edge_cases() {
        // ATX, closed ATX, Setext, mixed styles
        let content =
            "  # ATX Heading\n# Closed ATX Heading #\nSetext H1\n=======\nSetext H2\n-------\n\n# ATX Again\n";
        let structure = DocumentStructure::new(content);
        assert_eq!(structure.heading_lines, vec![1, 2, 3, 5, 8]);
        assert_eq!(structure.heading_levels, vec![1, 1, 1, 2, 1]);

        // Headings in code blocks and front matter (should be ignored)
        let content = "---\ntitle: Test\n---\n# Heading 1\n\n```\n# Not a heading\n```\n# Heading 2\n";
        let structure = DocumentStructure::new(content);
        assert_eq!(structure.heading_lines, vec![4, 9]);
        assert_eq!(structure.heading_levels, vec![1, 1]);

        // Empty headings
        let content = "#\n## \n###  \n# Not Empty\n";
        let structure = DocumentStructure::new(content);
        assert_eq!(structure.heading_lines, vec![4]);
        assert_eq!(structure.heading_levels, vec![1]);

        // Headings with trailing whitespace
        let content = "# Heading \n# Heading\n";
        let structure = DocumentStructure::new(content);
        assert_eq!(structure.heading_lines, vec![1, 2]);
        assert_eq!(structure.heading_levels, vec![1, 1]);

        // Headings with indentation
        let content = "   # Indented\n    # Not a heading (too much indent)\n# Valid\n";
        let structure = DocumentStructure::new(content);
        assert_eq!(structure.heading_lines, vec![1, 3]);
        assert_eq!(structure.heading_levels, vec![1, 1]);

        // Multiple duplicates and edge line numbers
        let content = "# Dup\n# Dup\n# Unique\n# Dup\n";
        let structure = DocumentStructure::new(content);
        assert_eq!(structure.heading_lines, vec![1, 2, 3, 4]);
        assert_eq!(structure.heading_levels, vec![1, 1, 1, 1]);

        // Headings after code blocks/front matter
        let content = "```\n# Not a heading\n```\n# Real Heading\n";
        let structure = DocumentStructure::new(content);
        assert_eq!(structure.heading_lines, vec![4]);
        assert_eq!(structure.heading_levels, vec![1]);

        let content = "---\ntitle: Test\n---\n# Heading\n";
        let structure = DocumentStructure::new(content);
        assert_eq!(structure.heading_lines, vec![4]);
        assert_eq!(structure.heading_levels, vec![1]);

        // Setext headings with blank lines before/after
        let content = "\nSetext\n=======\n\nSetext2\n-------\n";
        let structure = DocumentStructure::new(content);
        assert_eq!(structure.heading_lines, vec![2, 5]);
        assert_eq!(structure.heading_levels, vec![1, 2]);

        // Headings with special characters
        let content = "# Heading!@#$%^&*()\nSetext Special\n=======\n";
        let structure = DocumentStructure::new(content);
        assert_eq!(structure.heading_lines, vec![1, 2]);
        assert_eq!(structure.heading_levels, vec![1, 1]);
    }

    #[test]
    fn test_horizontal_rule_detection() {
        // Test basic horizontal rules
        let content = "Text\n\n---\n\nMore text\n\n***\n\nFinal\n\n___\n\nEnd";
        let structure = DocumentStructure::new(content);
        assert_eq!(structure.horizontal_rule_lines, vec![3, 7, 11]);

        // Test horizontal rules with spaces
        let content = "Text\n\n- - -\n\n* * *\n\n_ _ _\n\nEnd";
        let structure = DocumentStructure::new(content);
        assert_eq!(structure.horizontal_rule_lines, vec![3, 5, 7]);

        // Test setext headings are not detected as horizontal rules
        let content = "# ATX\n\nSetext\n------\n\n---\n\nAnother\n======\n";
        let structure = DocumentStructure::new(content);
        assert_eq!(structure.horizontal_rule_lines, vec![6]); // Only the actual HR
        assert_eq!(structure.heading_lines, vec![1, 3, 8]); // Three headings

        // Test horizontal rules in code blocks are ignored
        let content = "Text\n\n```\n---\n***\n```\n\n---\n\nEnd";
        let structure = DocumentStructure::new(content);
        assert_eq!(structure.horizontal_rule_lines, vec![8]); // Only the one outside code block

        // Test horizontal rules in front matter are ignored
        let content = "---\ntitle: Test\n---\n\n---\n\nContent";
        let structure = DocumentStructure::new(content);
        assert_eq!(structure.horizontal_rule_lines, vec![5]); // Only the one after front matter
    }
}
