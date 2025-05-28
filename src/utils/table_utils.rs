/// Shared table detection and processing utilities for markdown linting rules
///
/// This module provides optimized table detection and processing functionality
/// that can be shared across multiple table-related rules (MD055, MD056, MD058).
use crate::utils::code_block_utils::CodeBlockUtils;

/// Represents a table block in the document
#[derive(Debug, Clone)]
pub struct TableBlock {
    pub start_line: usize,
    pub end_line: usize,
    pub header_line: usize,
    pub delimiter_line: usize,
    pub content_lines: Vec<usize>,
}

/// Shared table detection utilities
pub struct TableUtils;

impl TableUtils {
    /// Check if a line looks like a potential table row
    pub fn is_potential_table_row(line: &str) -> bool {
        let trimmed = line.trim();
        if trimmed.is_empty() || !trimmed.contains('|') {
            return false;
        }

        // Skip lines that are clearly not table rows
        if trimmed.starts_with("- ") || trimmed.starts_with("* ") || trimmed.starts_with("+ ") {
            return false;
        }

        // Skip lines that are clearly code or inline code
        if trimmed.starts_with("`") || trimmed.contains("``") {
            return false;
        }

        // Must have at least 2 parts when split by |
        let parts: Vec<&str> = trimmed.split('|').collect();
        if parts.len() < 2 {
            return false;
        }

        // Check if it looks like a table row by having reasonable content between pipes
        let mut valid_parts = 0;
        for part in &parts {
            let part_trimmed = part.trim();
            // Skip empty parts (from leading/trailing pipes)
            if part_trimmed.is_empty() {
                continue;
            }
            // Count parts that look like table cells (not too long, reasonable content)
            if part_trimmed.len() <= 100 && !part_trimmed.contains('\n') {
                valid_parts += 1;
            }
        }

        // Must have at least 2 valid cell-like parts
        valid_parts >= 2
    }

    /// Check if a line is a table delimiter row (e.g., |---|---|)
    pub fn is_delimiter_row(line: &str) -> bool {
        let trimmed = line.trim();
        if !trimmed.contains('|') || !trimmed.contains('-') {
            return false;
        }

        // Split by pipes and check each part
        let parts: Vec<&str> = trimmed.split('|').collect();
        let mut valid_delimiter_parts = 0;

        for part in &parts {
            let part_trimmed = part.trim();
            if part_trimmed.is_empty() {
                continue; // Skip empty parts from leading/trailing pipes
            }

            // Check if this part looks like a delimiter (contains dashes and optionally colons)
            if part_trimmed
                .chars()
                .all(|c| c == '-' || c == ':' || c.is_whitespace())
                && part_trimmed.contains('-')
            {
                valid_delimiter_parts += 1;
            }
        }

        valid_delimiter_parts >= 2
    }

    /// Find all table blocks in the content with optimized detection
    pub fn find_table_blocks(content: &str) -> Vec<TableBlock> {
        let lines: Vec<&str> = content.lines().collect();
        let code_blocks = CodeBlockUtils::detect_code_blocks(content);
        let mut tables = Vec::new();
        let mut i = 0;

        // Pre-compute line positions for efficient code block checking
        let mut line_positions = Vec::with_capacity(lines.len());
        let mut pos = 0;
        for line in &lines {
            line_positions.push(pos);
            pos += line.len() + 1; // +1 for newline
        }

        while i < lines.len() {
            // Skip lines in code blocks using pre-computed positions
            let line_start = line_positions[i];
            if code_blocks
                .iter()
                .any(|(start, end)| line_start >= *start && line_start < *end)
            {
                i += 1;
                continue;
            }

            // Look for potential table start
            if Self::is_potential_table_row(lines[i]) {
                // Check if the next line is a delimiter row
                if i + 1 < lines.len() && Self::is_delimiter_row(lines[i + 1]) {
                    // Found a table! Find its end
                    let table_start = i;
                    let header_line = i;
                    let delimiter_line = i + 1;
                    let mut table_end = i + 1; // Include the delimiter row
                    let mut content_lines = Vec::new();

                    // Continue while we have table rows
                    let mut j = i + 2;
                    while j < lines.len() {
                        let line = lines[j];
                        if line.trim().is_empty() {
                            // Empty line ends the table
                            break;
                        }
                        if Self::is_potential_table_row(line) {
                            content_lines.push(j);
                            table_end = j;
                            j += 1;
                        } else {
                            // Non-table line ends the table
                            break;
                        }
                    }

                    tables.push(TableBlock {
                        start_line: table_start,
                        end_line: table_end,
                        header_line,
                        delimiter_line,
                        content_lines,
                    });
                    i = table_end + 1;
                } else {
                    i += 1;
                }
            } else {
                i += 1;
            }
        }

        tables
    }

    /// Count the number of cells in a table row
    pub fn count_cells(row: &str) -> usize {
        let trimmed = row.trim();

        // Skip non-table rows
        if !trimmed.contains('|') {
            return 0;
        }

        // Handle case with leading/trailing pipes
        let mut cell_count = 0;
        let parts: Vec<&str> = trimmed.split('|').collect();

        for (i, part) in parts.iter().enumerate() {
            // Skip first part if it's empty and there's a leading pipe
            if i == 0 && part.trim().is_empty() && parts.len() > 1 {
                continue;
            }

            // Skip last part if it's empty and there's a trailing pipe
            if i == parts.len() - 1 && part.trim().is_empty() && parts.len() > 1 {
                continue;
            }

            cell_count += 1;
        }

        cell_count
    }

    /// Determine the pipe style of a table row
    pub fn determine_pipe_style(line: &str) -> Option<&'static str> {
        let trimmed = line.trim();
        if !trimmed.contains('|') {
            return None;
        }

        let has_leading = trimmed.starts_with('|');
        let has_trailing = trimmed.ends_with('|');

        match (has_leading, has_trailing) {
            (true, true) => Some("leading_and_trailing"),
            (true, false) => Some("leading_only"),
            (false, true) => Some("trailing_only"),
            (false, false) => Some("no_leading_or_trailing"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_potential_table_row() {
        assert!(TableUtils::is_potential_table_row(
            "| Header 1 | Header 2 |"
        ));
        assert!(TableUtils::is_potential_table_row("| Cell 1 | Cell 2 |"));
        assert!(!TableUtils::is_potential_table_row("- List item"));
        assert!(!TableUtils::is_potential_table_row("Regular text"));
        assert!(!TableUtils::is_potential_table_row(""));
    }

    #[test]
    fn test_is_delimiter_row() {
        assert!(TableUtils::is_delimiter_row("|---|---|"));
        assert!(TableUtils::is_delimiter_row("| --- | --- |"));
        assert!(TableUtils::is_delimiter_row("|:---|---:|"));
        assert!(!TableUtils::is_delimiter_row("| Header | Header |"));
        assert!(!TableUtils::is_delimiter_row("Regular text"));
    }

    #[test]
    fn test_count_cells() {
        assert_eq!(TableUtils::count_cells("| Cell 1 | Cell 2 | Cell 3 |"), 3);
        assert_eq!(TableUtils::count_cells("Cell 1 | Cell 2 | Cell 3"), 3);
        assert_eq!(TableUtils::count_cells("| Cell 1 | Cell 2"), 2);
        assert_eq!(TableUtils::count_cells("Regular text"), 0);
    }

    #[test]
    fn test_determine_pipe_style() {
        assert_eq!(
            TableUtils::determine_pipe_style("| Cell 1 | Cell 2 |"),
            Some("leading_and_trailing")
        );
        assert_eq!(
            TableUtils::determine_pipe_style("| Cell 1 | Cell 2"),
            Some("leading_only")
        );
        assert_eq!(
            TableUtils::determine_pipe_style("Cell 1 | Cell 2 |"),
            Some("trailing_only")
        );
        assert_eq!(
            TableUtils::determine_pipe_style("Cell 1 | Cell 2"),
            Some("no_leading_or_trailing")
        );
        assert_eq!(TableUtils::determine_pipe_style("Regular text"), None);
    }
}
