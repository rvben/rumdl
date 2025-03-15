//! Utilities for position/range conversions

/// Convert line/column positions to byte ranges
pub fn line_col_to_byte_range(content: &str, line: usize, column: usize) -> std::ops::Range<usize> {
    // Make sure we don't go out of bounds
    let lines: Vec<&str> = content.lines().collect();
    
    // If the line number is beyond the end of the content, return the last position
    if line == 0 || line > lines.len() {
        return content.len()..content.len();
    }
    
    // Calculate the start byte position by summing the lengths of previous lines plus newlines
    let line_start = lines[..line-1]
        .iter()
        .map(|l| l.len() + 1) // +1 for newline
        .sum::<usize>();
    
    // Get the current line to check its length
    let current_line = lines[line-1];
    
    // Make sure the column isn't beyond the end of the line
    let col = if column == 0 {
        1
    } else if column > current_line.len() + 1 {
        current_line.len() + 1
    } else {
        column
    };
    
    let start = line_start + col - 1; // Convert to 0-based
    
    // Ensure start position doesn't exceed content length
    let safe_start = std::cmp::min(start, content.len());
    
    safe_start..safe_start
}
