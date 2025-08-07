/// Shared table detection and processing utilities for markdown linting rules
///
/// This module provides optimized table detection and processing functionality
/// that can be shared across multiple table-related rules (MD055, MD056, MD058).
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
        let mut total_non_empty_parts = 0;

        for part in &parts {
            let part_trimmed = part.trim();
            // Skip empty parts (from leading/trailing pipes)
            if part_trimmed.is_empty() {
                continue;
            }
            total_non_empty_parts += 1;

            // Count parts that look like table cells (reasonable content, no newlines)
            if !part_trimmed.contains('\n') {
                valid_parts += 1;
            }
        }

        // All non-empty parts must be valid (no newlines) for the row to be valid
        if total_non_empty_parts == 0 {
            return false;
        }

        if valid_parts != total_non_empty_parts {
            // Some cells contain newlines, not a valid table row
            return false;
        }

        // GFM allows single-column tables, so >= 1 valid part is enough
        // when the line has proper table formatting (pipes)
        if trimmed.starts_with('|') && trimmed.ends_with('|') {
            // Properly formatted table row with pipes on both ends
            valid_parts >= 1
        } else {
            // For rows without proper pipe formatting, require at least 2 cells
            valid_parts >= 2
        }
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
            if part_trimmed.chars().all(|c| c == '-' || c == ':' || c.is_whitespace()) && part_trimmed.contains('-') {
                valid_delimiter_parts += 1;
            }
        }

        valid_delimiter_parts >= 2
    }

    /// Find all table blocks in the content with optimized detection
    pub fn find_table_blocks(content: &str, ctx: &crate::lint_context::LintContext) -> Vec<TableBlock> {
        let lines: Vec<&str> = content.lines().collect();
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
            // Skip lines in code blocks using cached code blocks from context
            let line_start = line_positions[i];
            if ctx.is_in_code_block_or_span(line_start) {
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

        // Users shouldn't have to escape pipes in regex patterns, etc.
        let masked_row = Self::mask_pipes_in_inline_code(trimmed);

        // Handle case with leading/trailing pipes
        let mut cell_count = 0;
        let parts: Vec<&str> = masked_row.split('|').collect();

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

    /// Mask pipes inside inline code blocks with a placeholder character
    fn mask_pipes_in_inline_code(text: &str) -> String {
        let mut result = String::new();
        let chars: Vec<char> = text.chars().collect();
        let mut i = 0;

        while i < chars.len() {
            if chars[i] == '`' {
                // Count consecutive backticks at start
                let start = i;
                let mut backtick_count = 0;
                while i < chars.len() && chars[i] == '`' {
                    backtick_count += 1;
                    i += 1;
                }

                // Look for matching closing backticks
                let mut found_closing = false;
                let mut j = i;

                while j < chars.len() {
                    if chars[j] == '`' {
                        // Count potential closing backticks
                        let close_start = j;
                        let mut close_count = 0;
                        while j < chars.len() && chars[j] == '`' {
                            close_count += 1;
                            j += 1;
                        }

                        if close_count == backtick_count {
                            // Found matching closing backticks
                            found_closing = true;

                            // Valid inline code - add with pipes masked
                            result.extend(chars[start..i].iter());

                            for &ch in chars.iter().take(close_start).skip(i) {
                                if ch == '|' {
                                    result.push('_'); // Mask pipe with underscore
                                } else {
                                    result.push(ch);
                                }
                            }

                            result.extend(chars[close_start..j].iter());
                            i = j;
                            break;
                        }
                        // If not matching, continue searching (j is already past these backticks)
                    } else {
                        j += 1;
                    }
                }

                if !found_closing {
                    // No matching closing found, treat as regular text
                    result.extend(chars[start..i].iter());
                }
            } else {
                result.push(chars[i]);
                i += 1;
            }
        }

        result
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
    use crate::lint_context::LintContext;

    #[test]
    fn test_is_potential_table_row() {
        // Basic valid table rows
        assert!(TableUtils::is_potential_table_row("| Header 1 | Header 2 |"));
        assert!(TableUtils::is_potential_table_row("| Cell 1 | Cell 2 |"));
        assert!(TableUtils::is_potential_table_row("Cell 1 | Cell 2"));
        assert!(TableUtils::is_potential_table_row("| Cell |")); // Single-column tables are valid in GFM

        // Multiple cells
        assert!(TableUtils::is_potential_table_row("| A | B | C | D | E |"));

        // With whitespace
        assert!(TableUtils::is_potential_table_row("  | Indented | Table |  "));
        assert!(TableUtils::is_potential_table_row("| Spaces | Around |"));

        // Not table rows
        assert!(!TableUtils::is_potential_table_row("- List item"));
        assert!(!TableUtils::is_potential_table_row("* Another list"));
        assert!(!TableUtils::is_potential_table_row("+ Plus list"));
        assert!(!TableUtils::is_potential_table_row("Regular text"));
        assert!(!TableUtils::is_potential_table_row(""));
        assert!(!TableUtils::is_potential_table_row("   "));

        // Code blocks
        assert!(!TableUtils::is_potential_table_row("`code with | pipe`"));
        assert!(!TableUtils::is_potential_table_row("``multiple | backticks``"));

        // Single pipe not enough
        assert!(!TableUtils::is_potential_table_row("Just one |"));
        assert!(!TableUtils::is_potential_table_row("| Just one"));

        // Very long cells are valid in tables (no length limit for cell content)
        let long_cell = "a".repeat(150);
        assert!(TableUtils::is_potential_table_row(&format!("| {long_cell} | b |")));

        // Cells with newlines
        assert!(!TableUtils::is_potential_table_row("| Cell with\nnewline | Other |"));
    }

    #[test]
    fn test_is_delimiter_row() {
        // Basic delimiter rows
        assert!(TableUtils::is_delimiter_row("|---|---|"));
        assert!(TableUtils::is_delimiter_row("| --- | --- |"));
        assert!(TableUtils::is_delimiter_row("|:---|---:|"));
        assert!(TableUtils::is_delimiter_row("|:---:|:---:|"));

        // With varying dash counts
        assert!(TableUtils::is_delimiter_row("|-|--|"));
        assert!(TableUtils::is_delimiter_row("|-------|----------|"));

        // With whitespace
        assert!(TableUtils::is_delimiter_row("|  ---  |  ---  |"));
        assert!(TableUtils::is_delimiter_row("| :--- | ---: |"));

        // Multiple columns
        assert!(TableUtils::is_delimiter_row("|---|---|---|---|"));

        // Without leading/trailing pipes
        assert!(TableUtils::is_delimiter_row("--- | ---"));
        assert!(TableUtils::is_delimiter_row(":--- | ---:"));

        // Not delimiter rows
        assert!(!TableUtils::is_delimiter_row("| Header | Header |"));
        assert!(!TableUtils::is_delimiter_row("Regular text"));
        assert!(!TableUtils::is_delimiter_row(""));
        assert!(!TableUtils::is_delimiter_row("|||"));
        assert!(!TableUtils::is_delimiter_row("| | |"));

        // Must have dashes
        assert!(!TableUtils::is_delimiter_row("| : | : |"));
        assert!(!TableUtils::is_delimiter_row("|    |    |"));

        // Mixed content
        assert!(!TableUtils::is_delimiter_row("| --- | text |"));
        assert!(!TableUtils::is_delimiter_row("| abc | --- |"));
    }

    #[test]
    fn test_count_cells() {
        // Basic counts
        assert_eq!(TableUtils::count_cells("| Cell 1 | Cell 2 | Cell 3 |"), 3);
        assert_eq!(TableUtils::count_cells("Cell 1 | Cell 2 | Cell 3"), 3);
        assert_eq!(TableUtils::count_cells("| Cell 1 | Cell 2"), 2);
        assert_eq!(TableUtils::count_cells("Cell 1 | Cell 2 |"), 2);

        // Single cell
        assert_eq!(TableUtils::count_cells("| Cell |"), 1);
        assert_eq!(TableUtils::count_cells("Cell"), 0); // No pipe

        // Empty cells
        assert_eq!(TableUtils::count_cells("|  |  |  |"), 3);
        assert_eq!(TableUtils::count_cells("| | | |"), 3);

        // Many cells
        assert_eq!(TableUtils::count_cells("| A | B | C | D | E | F |"), 6);

        // Edge cases
        assert_eq!(TableUtils::count_cells("||"), 1); // One empty cell
        assert_eq!(TableUtils::count_cells("|||"), 2); // Two empty cells

        // No table
        assert_eq!(TableUtils::count_cells("Regular text"), 0);
        assert_eq!(TableUtils::count_cells(""), 0);
        assert_eq!(TableUtils::count_cells("   "), 0);

        // Whitespace handling
        assert_eq!(TableUtils::count_cells("  | A | B |  "), 2);
        assert_eq!(TableUtils::count_cells("|   A   |   B   |"), 2);
    }

    #[test]
    fn test_count_cells_with_inline_code() {
        // Test the user's actual example from Issue #34
        assert_eq!(TableUtils::count_cells("| Challenge | Solution |"), 2);
        assert_eq!(
            TableUtils::count_cells("| Hour:minute:second formats | `^([0-1]?\\d|2[0-3]):[0-5]\\d:[0-5]\\d$` |"),
            2
        );

        // Test basic inline code with pipes
        assert_eq!(TableUtils::count_cells("| Command | `echo | grep` |"), 2);
        assert_eq!(TableUtils::count_cells("| A | `code | with | pipes` | B |"), 3);

        // Test escaped pipes (correct GFM)
        assert_eq!(TableUtils::count_cells("| Command | `echo \\| grep` |"), 2);

        // Test multiple inline code blocks
        assert_eq!(TableUtils::count_cells("| `code | one` | `code | two` |"), 2);

        // Test edge cases
        assert_eq!(TableUtils::count_cells("| Empty inline | `` | cell |"), 3);
        assert_eq!(TableUtils::count_cells("| `single|pipe` |"), 1);

        // Test that basic table structure still works
        assert_eq!(TableUtils::count_cells("| A | B | C |"), 3);
        assert_eq!(TableUtils::count_cells("| One | Two |"), 2);
    }

    #[test]
    fn test_determine_pipe_style() {
        // All pipe styles
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

        // With whitespace
        assert_eq!(
            TableUtils::determine_pipe_style("  | Cell 1 | Cell 2 |  "),
            Some("leading_and_trailing")
        );
        assert_eq!(
            TableUtils::determine_pipe_style("  | Cell 1 | Cell 2  "),
            Some("leading_only")
        );

        // No pipes
        assert_eq!(TableUtils::determine_pipe_style("Regular text"), None);
        assert_eq!(TableUtils::determine_pipe_style(""), None);
        assert_eq!(TableUtils::determine_pipe_style("   "), None);

        // Single pipe cases
        assert_eq!(TableUtils::determine_pipe_style("|"), Some("leading_and_trailing"));
        assert_eq!(TableUtils::determine_pipe_style("| Cell"), Some("leading_only"));
        assert_eq!(TableUtils::determine_pipe_style("Cell |"), Some("trailing_only"));
    }

    #[test]
    fn test_find_table_blocks_simple() {
        let content = "| Header 1 | Header 2 |
|-----------|-----------|
| Cell 1    | Cell 2    |
| Cell 3    | Cell 4    |";

        let ctx = LintContext::new(content);

        let tables = TableUtils::find_table_blocks(content, &ctx);
        assert_eq!(tables.len(), 1);

        let table = &tables[0];
        assert_eq!(table.start_line, 0);
        assert_eq!(table.end_line, 3);
        assert_eq!(table.header_line, 0);
        assert_eq!(table.delimiter_line, 1);
        assert_eq!(table.content_lines, vec![2, 3]);
    }

    #[test]
    fn test_find_table_blocks_multiple() {
        let content = "Some text

| Table 1 | Col A |
|----------|-------|
| Data 1   | Val 1 |

More text

| Table 2 | Col 2 |
|----------|-------|
| Data 2   | Data  |";

        let ctx = LintContext::new(content);

        let tables = TableUtils::find_table_blocks(content, &ctx);
        assert_eq!(tables.len(), 2);

        // First table
        assert_eq!(tables[0].start_line, 2);
        assert_eq!(tables[0].end_line, 4);
        assert_eq!(tables[0].header_line, 2);
        assert_eq!(tables[0].delimiter_line, 3);
        assert_eq!(tables[0].content_lines, vec![4]);

        // Second table
        assert_eq!(tables[1].start_line, 8);
        assert_eq!(tables[1].end_line, 10);
        assert_eq!(tables[1].header_line, 8);
        assert_eq!(tables[1].delimiter_line, 9);
        assert_eq!(tables[1].content_lines, vec![10]);
    }

    #[test]
    fn test_find_table_blocks_no_content_rows() {
        let content = "| Header 1 | Header 2 |
|-----------|-----------|

Next paragraph";

        let ctx = LintContext::new(content);

        let tables = TableUtils::find_table_blocks(content, &ctx);
        assert_eq!(tables.len(), 1);

        let table = &tables[0];
        assert_eq!(table.start_line, 0);
        assert_eq!(table.end_line, 1); // Just header and delimiter
        assert_eq!(table.content_lines.len(), 0);
    }

    #[test]
    fn test_find_table_blocks_in_code_block() {
        let content = "```
| Not | A | Table |
|-----|---|-------|
| In  | Code | Block |
```

| Real | Table |
|------|-------|
| Data | Here  |";

        let ctx = LintContext::new(content);

        let tables = TableUtils::find_table_blocks(content, &ctx);
        assert_eq!(tables.len(), 1); // Only the table outside code block

        let table = &tables[0];
        assert_eq!(table.header_line, 6);
        assert_eq!(table.delimiter_line, 7);
    }

    #[test]
    fn test_find_table_blocks_no_tables() {
        let content = "Just regular text
No tables here
- List item with | pipe
* Another list item";

        let ctx = LintContext::new(content);

        let tables = TableUtils::find_table_blocks(content, &ctx);
        assert_eq!(tables.len(), 0);
    }

    #[test]
    fn test_find_table_blocks_malformed() {
        let content = "| Header without delimiter |
| This looks like table |
But no delimiter row

| Proper | Table |
|---------|-------|
| Data    | Here  |";

        let ctx = LintContext::new(content);

        let tables = TableUtils::find_table_blocks(content, &ctx);
        assert_eq!(tables.len(), 1); // Only the proper table
        assert_eq!(tables[0].header_line, 4);
    }

    #[test]
    fn test_edge_cases() {
        // Test empty content
        assert!(!TableUtils::is_potential_table_row(""));
        assert!(!TableUtils::is_delimiter_row(""));
        assert_eq!(TableUtils::count_cells(""), 0);
        assert_eq!(TableUtils::determine_pipe_style(""), None);

        // Test whitespace only
        assert!(!TableUtils::is_potential_table_row("   "));
        assert!(!TableUtils::is_delimiter_row("   "));
        assert_eq!(TableUtils::count_cells("   "), 0);
        assert_eq!(TableUtils::determine_pipe_style("   "), None);

        // Test single character
        assert!(!TableUtils::is_potential_table_row("|"));
        assert!(!TableUtils::is_delimiter_row("|"));
        assert_eq!(TableUtils::count_cells("|"), 0); // Need at least 2 parts

        // Test very long lines are valid table rows (no length limit)
        // Test both single-column and multi-column long lines
        let long_single = format!("| {} |", "a".repeat(200));
        assert!(TableUtils::is_potential_table_row(&long_single)); // Single-column table with long content

        let long_multi = format!("| {} | {} |", "a".repeat(200), "b".repeat(200));
        assert!(TableUtils::is_potential_table_row(&long_multi)); // Multi-column table with long content

        // Test unicode
        assert!(TableUtils::is_potential_table_row("| 你好 | 世界 |"));
        assert!(TableUtils::is_potential_table_row("| émoji | 🎉 |"));
        assert_eq!(TableUtils::count_cells("| 你好 | 世界 |"), 2);
    }

    #[test]
    fn test_table_block_struct() {
        let block = TableBlock {
            start_line: 0,
            end_line: 5,
            header_line: 0,
            delimiter_line: 1,
            content_lines: vec![2, 3, 4, 5],
        };

        // Test Debug trait
        let debug_str = format!("{block:?}");
        assert!(debug_str.contains("TableBlock"));
        assert!(debug_str.contains("start_line: 0"));

        // Test Clone trait
        let cloned = block.clone();
        assert_eq!(cloned.start_line, block.start_line);
        assert_eq!(cloned.end_line, block.end_line);
        assert_eq!(cloned.header_line, block.header_line);
        assert_eq!(cloned.delimiter_line, block.delimiter_line);
        assert_eq!(cloned.content_lines, block.content_lines);
    }
}
