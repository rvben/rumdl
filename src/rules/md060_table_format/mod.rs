use crate::rule::{LintError, LintResult, LintWarning, Rule, Severity};
use crate::utils::range_utils::calculate_line_range;
use crate::utils::regex_cache::BLOCKQUOTE_PREFIX_RE;
use crate::utils::table_utils::TableUtils;
use unicode_width::UnicodeWidthStr;

mod md060_config;
use crate::md013_line_length::MD013Config;
pub use md060_config::ColumnAlign;
pub use md060_config::MD060Config;

/// Identifies the type of row in a table for formatting purposes.
#[derive(Debug, Clone, Copy, PartialEq)]
enum RowType {
    /// The first row containing column headers
    Header,
    /// The second row containing delimiter dashes (e.g., `|---|---|`)
    Delimiter,
    /// Data rows following the delimiter
    Body,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum ColumnAlignment {
    Left,
    Center,
    Right,
}

#[derive(Debug, Clone)]
struct TableFormatResult {
    lines: Vec<String>,
    auto_compacted: bool,
    aligned_width: Option<usize>,
}

/// Formatting options for a single table row.
#[derive(Debug, Clone, Copy)]
struct RowFormatOptions {
    /// The type of row being formatted
    row_type: RowType,
    /// Whether to use compact delimiter style (no spaces around dashes)
    compact_delimiter: bool,
    /// Global column alignment override
    column_align: ColumnAlign,
    /// Header-specific column alignment (overrides column_align for header)
    column_align_header: Option<ColumnAlign>,
    /// Body-specific column alignment (overrides column_align for body)
    column_align_body: Option<ColumnAlign>,
    /// Whether to skip padding on the last column for body rows
    loose_last_column: bool,
}

/// Rule MD060: Table Column Alignment
///
/// See [docs/md060.md](../../docs/md060.md) for full documentation, configuration, and examples.
///
/// This rule enforces consistent column alignment in Markdown tables for improved readability
/// in source form. When enabled, it ensures table columns are properly aligned with appropriate
/// padding.
///
/// ## Purpose
///
/// - **Readability**: Aligned tables are significantly easier to read in source form
/// - **Maintainability**: Properly formatted tables are easier to edit and review
/// - **Consistency**: Ensures uniform table formatting throughout documents
/// - **Developer Experience**: Makes working with tables in plain text more pleasant
///
/// ## Configuration Options
///
/// The rule supports the following configuration options:
///
/// ```toml
/// [MD013]
/// line-length = 100  # MD060 inherits this by default
///
/// [MD060]
/// enabled = false      # Default: opt-in for conservative adoption
/// style = "aligned"    # Can be "aligned", "compact", "tight", or "any"
/// max-width = 0        # Default: inherit from MD013's line-length
/// ```
///
/// ### Style Options
///
/// - **aligned**: Columns are padded with spaces for visual alignment (default)
/// - **compact**: Minimal spacing with single spaces
/// - **tight**: No spacing, pipes directly adjacent to content
/// - **any**: Preserve existing formatting style
///
/// ### Max Width (auto-compact threshold)
///
/// Controls when tables automatically switch from aligned to compact formatting:
///
/// - **`max-width = 0`** (default): Smart inheritance from MD013
/// - **`max-width = N`**: Explicit threshold, independent of MD013
///
/// When `max-width = 0`:
/// - If MD013 is disabled → unlimited (no auto-compact)
/// - If MD013.tables = false → unlimited (no auto-compact)
/// - If MD013.line_length = 0 → unlimited (no auto-compact)
/// - Otherwise → inherits MD013's line-length
///
/// This matches the behavior of Prettier's table formatting.
///
/// #### Examples
///
/// ```toml
/// # Inherit from MD013 (recommended)
/// [MD013]
/// line-length = 100
///
/// [MD060]
/// style = "aligned"
/// max-width = 0  # Tables exceeding 100 chars will be compacted
/// ```
///
/// ```toml
/// # Explicit threshold
/// [MD060]
/// style = "aligned"
/// max-width = 120  # Independent of MD013
/// ```
///
/// ## Examples
///
/// ### Aligned Style (Good)
///
/// ```markdown
/// | Name  | Age | City      |
/// |-------|-----|-----------|
/// | Alice | 30  | Seattle   |
/// | Bob   | 25  | Portland  |
/// ```
///
/// ### Unaligned (Bad)
///
/// ```markdown
/// | Name | Age | City |
/// |---|---|---|
/// | Alice | 30 | Seattle |
/// | Bob | 25 | Portland |
/// ```
///
/// ## Unicode Support
///
/// This rule properly handles:
/// - **CJK Characters**: Chinese, Japanese, Korean characters are correctly measured as double-width
/// - **Basic Emoji**: Most emoji are handled correctly
/// - **Inline Code**: Pipes in inline code blocks are properly masked
///
/// ## Known Limitations
///
/// **Complex Unicode Sequences**: Tables containing certain Unicode characters are automatically
/// skipped to prevent alignment corruption. These include:
/// - Zero-Width Joiner (ZWJ) emoji: 👨‍👩‍👧‍👦, 👩‍💻
/// - Zero-Width Space (ZWS): Invisible word break opportunities
/// - Zero-Width Non-Joiner (ZWNJ): Ligature prevention marks
/// - Word Joiner (WJ): Non-breaking invisible characters
///
/// These characters have inconsistent or zero display widths across terminals and fonts,
/// making accurate alignment impossible. The rule preserves these tables as-is rather than
/// risk corrupting them.
///
/// This is an honest limitation of terminal display technology, similar to what other tools
/// like markdownlint experience.
///
/// ## Fix Behavior
///
/// When applying automatic fixes, this rule:
/// - Calculates proper display width for each column using Unicode width measurements
/// - Pads cells with trailing spaces to align columns
/// - Preserves cell content exactly (only spacing is modified)
/// - Respects alignment indicators in delimiter rows (`:---`, `:---:`, `---:`)
/// - Automatically switches to compact mode for tables exceeding max_width
/// - Skips tables with ZWJ emoji to prevent corruption
#[derive(Debug, Clone, Default)]
pub struct MD060TableFormat {
    config: MD060Config,
    md013_config: MD013Config,
    md013_disabled: bool,
}

impl MD060TableFormat {
    pub fn new(enabled: bool, style: String) -> Self {
        use crate::types::LineLength;
        Self {
            config: MD060Config {
                enabled,
                style,
                max_width: LineLength::from_const(0),
                column_align: ColumnAlign::Auto,
                column_align_header: None,
                column_align_body: None,
                loose_last_column: false,
            },
            md013_config: MD013Config::default(),
            md013_disabled: false,
        }
    }

    pub fn from_config_struct(config: MD060Config, md013_config: MD013Config, md013_disabled: bool) -> Self {
        Self {
            config,
            md013_config,
            md013_disabled,
        }
    }

    /// Get the effective max width for table formatting.
    ///
    /// Priority order:
    /// 1. Explicit `max_width > 0` always takes precedence
    /// 2. When `max_width = 0` (inherit mode), check MD013 configuration:
    ///    - If MD013 is globally disabled → unlimited
    ///    - If `MD013.tables = false` → unlimited
    ///    - If `MD013.line_length = 0` → unlimited
    ///    - Otherwise → inherit MD013's line_length
    fn effective_max_width(&self) -> usize {
        // Explicit max_width always takes precedence
        if !self.config.max_width.is_unlimited() {
            return self.config.max_width.get();
        }

        // max_width = 0 means "inherit" - but inherit UNLIMITED if:
        // 1. MD013 is globally disabled
        // 2. MD013.tables = false (user doesn't care about table line length)
        // 3. MD013.line_length = 0 (no line length limit at all)
        if self.md013_disabled || !self.md013_config.tables || self.md013_config.line_length.is_unlimited() {
            return usize::MAX; // Unlimited
        }

        // Otherwise inherit MD013's line-length
        self.md013_config.line_length.get()
    }

    /// Check if text contains characters that break Unicode width calculations
    ///
    /// Tables with these characters are skipped to avoid alignment corruption:
    /// - Zero-Width Joiner (ZWJ, U+200D): Complex emoji like 👨‍👩‍👧‍👦
    /// - Zero-Width Space (ZWS, U+200B): Invisible word break opportunity
    /// - Zero-Width Non-Joiner (ZWNJ, U+200C): Prevents ligature formation
    /// - Word Joiner (WJ, U+2060): Prevents line breaks without taking space
    ///
    /// These characters have inconsistent display widths across terminals,
    /// making accurate alignment impossible.
    fn contains_problematic_chars(text: &str) -> bool {
        text.contains('\u{200D}')  // ZWJ
            || text.contains('\u{200B}')  // ZWS
            || text.contains('\u{200C}')  // ZWNJ
            || text.contains('\u{2060}') // Word Joiner
    }

    fn calculate_cell_display_width(cell_content: &str) -> usize {
        let masked = TableUtils::mask_pipes_in_inline_code(cell_content);
        masked.trim().width()
    }

    /// Parse a table row into cells using Standard flavor (default behavior).
    /// Used for tests and backward compatibility.
    #[cfg(test)]
    fn parse_table_row(line: &str) -> Vec<String> {
        TableUtils::split_table_row(line)
    }

    /// Parse a table row into cells, respecting flavor-specific behavior.
    ///
    /// For MkDocs flavor, pipes inside inline code are NOT cell delimiters.
    /// For Standard/GFM flavor, all pipes (except escaped) are cell delimiters.
    fn parse_table_row_with_flavor(line: &str, flavor: crate::config::MarkdownFlavor) -> Vec<String> {
        TableUtils::split_table_row_with_flavor(line, flavor)
    }

    fn is_delimiter_row(row: &[String]) -> bool {
        if row.is_empty() {
            return false;
        }
        row.iter().all(|cell| {
            let trimmed = cell.trim();
            // A delimiter cell must contain at least one dash
            // Empty cells are not delimiter cells
            !trimmed.is_empty()
                && trimmed.contains('-')
                && trimmed.chars().all(|c| c == '-' || c == ':' || c.is_whitespace())
        })
    }

    /// Extract blockquote prefix from a line (e.g., "> " or ">> ").
    /// Returns (prefix, content_without_prefix).
    fn extract_blockquote_prefix(line: &str) -> (&str, &str) {
        if let Some(m) = BLOCKQUOTE_PREFIX_RE.find(line) {
            (&line[..m.end()], &line[m.end()..])
        } else {
            ("", line)
        }
    }

    fn parse_column_alignments(delimiter_row: &[String]) -> Vec<ColumnAlignment> {
        delimiter_row
            .iter()
            .map(|cell| {
                let trimmed = cell.trim();
                let has_left_colon = trimmed.starts_with(':');
                let has_right_colon = trimmed.ends_with(':');

                match (has_left_colon, has_right_colon) {
                    (true, true) => ColumnAlignment::Center,
                    (false, true) => ColumnAlignment::Right,
                    _ => ColumnAlignment::Left,
                }
            })
            .collect()
    }

    fn calculate_column_widths(table_lines: &[&str], flavor: crate::config::MarkdownFlavor) -> Vec<usize> {
        let mut column_widths = Vec::new();
        let mut delimiter_cells: Option<Vec<String>> = None;

        for line in table_lines {
            let cells = Self::parse_table_row_with_flavor(line, flavor);

            // Save delimiter row for later processing, but don't use it for width calculation
            if Self::is_delimiter_row(&cells) {
                delimiter_cells = Some(cells);
                continue;
            }

            for (i, cell) in cells.iter().enumerate() {
                let width = Self::calculate_cell_display_width(cell);
                if i >= column_widths.len() {
                    column_widths.push(width);
                } else {
                    column_widths[i] = column_widths[i].max(width);
                }
            }
        }

        // GFM requires delimiter rows to have at least 3 dashes per column.
        // To ensure visual alignment, all columns must be at least width 3.
        let mut final_widths: Vec<usize> = column_widths.iter().map(|&w| w.max(3)).collect();

        // Adjust column widths to accommodate alignment indicators (colons) in delimiter row
        // This ensures the delimiter row has the same length as content rows
        if let Some(delimiter_cells) = delimiter_cells {
            for (i, cell) in delimiter_cells.iter().enumerate() {
                if i < final_widths.len() {
                    let trimmed = cell.trim();
                    let has_left_colon = trimmed.starts_with(':');
                    let has_right_colon = trimmed.ends_with(':');
                    let colon_count = (has_left_colon as usize) + (has_right_colon as usize);

                    // Minimum width needed: 3 dashes + colons
                    let min_width_for_delimiter = 3 + colon_count;
                    final_widths[i] = final_widths[i].max(min_width_for_delimiter);
                }
            }
        }

        final_widths
    }

    fn format_table_row(
        cells: &[String],
        column_widths: &[usize],
        column_alignments: &[ColumnAlignment],
        options: &RowFormatOptions,
    ) -> String {
        let num_cells = cells.len();
        let formatted_cells: Vec<String> = cells
            .iter()
            .enumerate()
            .map(|(i, cell)| {
                let is_last_column = i == num_cells - 1;
                let target_width = column_widths.get(i).copied().unwrap_or(0);

                match options.row_type {
                    RowType::Delimiter => {
                        let trimmed = cell.trim();
                        let has_left_colon = trimmed.starts_with(':');
                        let has_right_colon = trimmed.ends_with(':');

                        // Delimiter rows use the same cell format as content rows: | content |
                        // The "content" is dashes, possibly with colons for alignment
                        // For compact_delimiter mode, we don't add spaces, so we need 2 extra dashes
                        let extra_width = if options.compact_delimiter { 2 } else { 0 };
                        let dash_count = if has_left_colon && has_right_colon {
                            (target_width + extra_width).saturating_sub(2)
                        } else if has_left_colon || has_right_colon {
                            (target_width + extra_width).saturating_sub(1)
                        } else {
                            target_width + extra_width
                        };

                        let dashes = "-".repeat(dash_count.max(3)); // Minimum 3 dashes
                        let delimiter_content = if has_left_colon && has_right_colon {
                            format!(":{dashes}:")
                        } else if has_left_colon {
                            format!(":{dashes}")
                        } else if has_right_colon {
                            format!("{dashes}:")
                        } else {
                            dashes
                        };

                        // Add spaces around delimiter content unless compact_delimiter mode
                        if options.compact_delimiter {
                            delimiter_content
                        } else {
                            format!(" {delimiter_content} ")
                        }
                    }
                    RowType::Header | RowType::Body => {
                        let trimmed = cell.trim();
                        let current_width = Self::calculate_cell_display_width(cell);

                        // For loose last column in body rows, don't add padding
                        let skip_padding =
                            options.loose_last_column && is_last_column && options.row_type == RowType::Body;

                        let padding = if skip_padding {
                            0
                        } else {
                            target_width.saturating_sub(current_width)
                        };

                        // Determine which alignment to use based on row type
                        let effective_align = match options.row_type {
                            RowType::Header => options.column_align_header.unwrap_or(options.column_align),
                            RowType::Body => options.column_align_body.unwrap_or(options.column_align),
                            RowType::Delimiter => unreachable!(),
                        };

                        // Apply alignment: use override if specified, otherwise use delimiter indicators
                        let alignment = match effective_align {
                            ColumnAlign::Auto => column_alignments.get(i).copied().unwrap_or(ColumnAlignment::Left),
                            ColumnAlign::Left => ColumnAlignment::Left,
                            ColumnAlign::Center => ColumnAlignment::Center,
                            ColumnAlign::Right => ColumnAlignment::Right,
                        };

                        match alignment {
                            ColumnAlignment::Left => {
                                // Left: content on left, padding on right
                                format!(" {trimmed}{} ", " ".repeat(padding))
                            }
                            ColumnAlignment::Center => {
                                // Center: split padding on both sides
                                let left_padding = padding / 2;
                                let right_padding = padding - left_padding;
                                format!(" {}{trimmed}{} ", " ".repeat(left_padding), " ".repeat(right_padding))
                            }
                            ColumnAlignment::Right => {
                                // Right: padding on left, content on right
                                format!(" {}{trimmed} ", " ".repeat(padding))
                            }
                        }
                    }
                }
            })
            .collect();

        format!("|{}|", formatted_cells.join("|"))
    }

    fn format_table_compact(cells: &[String]) -> String {
        let formatted_cells: Vec<String> = cells.iter().map(|cell| format!(" {} ", cell.trim())).collect();
        format!("|{}|", formatted_cells.join("|"))
    }

    fn format_table_tight(cells: &[String]) -> String {
        let formatted_cells: Vec<String> = cells.iter().map(|cell| cell.trim().to_string()).collect();
        format!("|{}|", formatted_cells.join("|"))
    }

    /// Checks if a table is already aligned with consistent column widths
    /// and the delimiter row style matches the target style.
    ///
    /// A table is considered "already aligned" if:
    /// 1. All rows have the same display length
    /// 2. Each column has consistent cell width across all rows
    /// 3. The delimiter row has valid minimum widths (at least 3 chars per cell)
    /// 4. The delimiter row style matches the target style (compact_delimiter parameter)
    ///
    /// The `compact_delimiter` parameter indicates whether the target style is "aligned-no-space"
    /// (true = no spaces around dashes, false = spaces around dashes).
    fn is_table_already_aligned(
        table_lines: &[&str],
        flavor: crate::config::MarkdownFlavor,
        compact_delimiter: bool,
    ) -> bool {
        if table_lines.len() < 2 {
            return false;
        }

        // Check 1: All rows must have the same length
        let first_len = table_lines[0].len();
        if !table_lines.iter().all(|line| line.len() == first_len) {
            return false;
        }

        // Parse all rows and check column count consistency
        let parsed: Vec<Vec<String>> = table_lines
            .iter()
            .map(|line| Self::parse_table_row_with_flavor(line, flavor))
            .collect();

        if parsed.is_empty() {
            return false;
        }

        let num_columns = parsed[0].len();
        if !parsed.iter().all(|row| row.len() == num_columns) {
            return false;
        }

        // Check delimiter row has valid minimum widths (3 chars: at least one dash + optional colons)
        // Delimiter row is always at index 1
        if let Some(delimiter_row) = parsed.get(1) {
            if !Self::is_delimiter_row(delimiter_row) {
                return false;
            }
            // Check each delimiter cell has at least one dash (minimum valid is "---" or ":--" etc)
            for cell in delimiter_row {
                let trimmed = cell.trim();
                let dash_count = trimmed.chars().filter(|&c| c == '-').count();
                if dash_count < 1 {
                    return false;
                }
            }

            // Check if delimiter row style matches the target style
            // compact_delimiter=true means "aligned-no-space" (no spaces around dashes)
            // compact_delimiter=false means "aligned" (spaces around dashes)
            let delimiter_has_spaces = delimiter_row
                .iter()
                .all(|cell| cell.starts_with(' ') && cell.ends_with(' '));

            // If target is compact (no spaces) but current has spaces, not aligned
            // If target is spaced but current has no spaces, not aligned
            if compact_delimiter && delimiter_has_spaces {
                return false;
            }
            if !compact_delimiter && !delimiter_has_spaces {
                return false;
            }
        }

        // Check each column has consistent width across all content rows
        // Use cell.width() to get display width INCLUDING padding, not trimmed content
        // This correctly handles CJK characters (display width 2, byte length 3)
        for col_idx in 0..num_columns {
            let mut widths = Vec::new();
            for (row_idx, row) in parsed.iter().enumerate() {
                // Skip delimiter row for content width check
                if row_idx == 1 {
                    continue;
                }
                if let Some(cell) = row.get(col_idx) {
                    widths.push(cell.width());
                }
            }
            // All content cells in this column should have the same display width
            if !widths.is_empty() && !widths.iter().all(|&w| w == widths[0]) {
                return false;
            }
        }

        true
    }

    fn detect_table_style(table_lines: &[&str], flavor: crate::config::MarkdownFlavor) -> Option<String> {
        if table_lines.is_empty() {
            return None;
        }

        // Check all rows (except delimiter) to determine consistent style
        // A table is only "tight" or "compact" if ALL rows follow that pattern
        let mut is_tight = true;
        let mut is_compact = true;

        for line in table_lines {
            let cells = Self::parse_table_row_with_flavor(line, flavor);

            if cells.is_empty() {
                continue;
            }

            // Skip delimiter rows when detecting style
            if Self::is_delimiter_row(&cells) {
                continue;
            }

            // Check if this row has no padding
            let row_has_no_padding = cells.iter().all(|cell| !cell.starts_with(' ') && !cell.ends_with(' '));

            // Check if this row has exactly single-space padding
            let row_has_single_space = cells.iter().all(|cell| {
                let trimmed = cell.trim();
                cell == &format!(" {trimmed} ")
            });

            // If any row doesn't match tight, the table isn't tight
            if !row_has_no_padding {
                is_tight = false;
            }

            // If any row doesn't match compact, the table isn't compact
            if !row_has_single_space {
                is_compact = false;
            }

            // Early exit: if neither tight nor compact, it must be aligned
            if !is_tight && !is_compact {
                return Some("aligned".to_string());
            }
        }

        // Return the most restrictive style that matches
        if is_tight {
            Some("tight".to_string())
        } else if is_compact {
            Some("compact".to_string())
        } else {
            Some("aligned".to_string())
        }
    }

    fn fix_table_block(
        &self,
        lines: &[&str],
        table_block: &crate::utils::table_utils::TableBlock,
        flavor: crate::config::MarkdownFlavor,
    ) -> TableFormatResult {
        let mut result = Vec::new();
        let mut auto_compacted = false;
        let mut aligned_width = None;

        let table_lines: Vec<&str> = std::iter::once(lines[table_block.header_line])
            .chain(std::iter::once(lines[table_block.delimiter_line]))
            .chain(table_block.content_lines.iter().map(|&idx| lines[idx]))
            .collect();

        if table_lines.iter().any(|line| Self::contains_problematic_chars(line)) {
            return TableFormatResult {
                lines: table_lines.iter().map(|s| s.to_string()).collect(),
                auto_compacted: false,
                aligned_width: None,
            };
        }

        // Extract blockquote prefix from the header line (first line of table)
        // All lines in the same table should have the same blockquote level
        let (blockquote_prefix, _) = Self::extract_blockquote_prefix(table_lines[0]);

        // Extract list prefix if present (for tables inside list items)
        let list_context = &table_block.list_context;
        let (list_prefix, continuation_indent) = if let Some(ctx) = list_context {
            (ctx.list_prefix.as_str(), " ".repeat(ctx.content_indent))
        } else {
            ("", String::new())
        };

        // Strip blockquote prefix and list prefix from all lines for processing
        let stripped_lines: Vec<&str> = table_lines
            .iter()
            .enumerate()
            .map(|(i, line)| {
                let after_blockquote = Self::extract_blockquote_prefix(line).1;
                if list_context.is_some() {
                    if i == 0 {
                        // Header line: strip list prefix
                        crate::utils::table_utils::TableUtils::extract_list_prefix(after_blockquote).1
                    } else {
                        // Continuation lines: strip expected indentation
                        after_blockquote
                            .strip_prefix(&continuation_indent)
                            .unwrap_or(after_blockquote.trim_start())
                    }
                } else {
                    after_blockquote
                }
            })
            .collect();

        let style = self.config.style.as_str();

        match style {
            "any" => {
                let detected_style = Self::detect_table_style(&stripped_lines, flavor);
                if detected_style.is_none() {
                    return TableFormatResult {
                        lines: table_lines.iter().map(|s| s.to_string()).collect(),
                        auto_compacted: false,
                        aligned_width: None,
                    };
                }

                let target_style = detected_style.unwrap();

                // Parse column alignments from delimiter row (always at index 1)
                let delimiter_cells = Self::parse_table_row_with_flavor(stripped_lines[1], flavor);
                let column_alignments = Self::parse_column_alignments(&delimiter_cells);

                for (row_idx, line) in stripped_lines.iter().enumerate() {
                    let cells = Self::parse_table_row_with_flavor(line, flavor);
                    match target_style.as_str() {
                        "tight" => result.push(Self::format_table_tight(&cells)),
                        "compact" => result.push(Self::format_table_compact(&cells)),
                        _ => {
                            let column_widths = Self::calculate_column_widths(&stripped_lines, flavor);
                            let row_type = match row_idx {
                                0 => RowType::Header,
                                1 => RowType::Delimiter,
                                _ => RowType::Body,
                            };
                            let options = RowFormatOptions {
                                row_type,
                                compact_delimiter: false,
                                column_align: self.config.column_align,
                                column_align_header: self.config.column_align_header,
                                column_align_body: self.config.column_align_body,
                                loose_last_column: self.config.loose_last_column,
                            };
                            result.push(Self::format_table_row(
                                &cells,
                                &column_widths,
                                &column_alignments,
                                &options,
                            ));
                        }
                    }
                }
            }
            "compact" => {
                for line in &stripped_lines {
                    let cells = Self::parse_table_row_with_flavor(line, flavor);
                    result.push(Self::format_table_compact(&cells));
                }
            }
            "tight" => {
                for line in &stripped_lines {
                    let cells = Self::parse_table_row_with_flavor(line, flavor);
                    result.push(Self::format_table_tight(&cells));
                }
            }
            "aligned" | "aligned-no-space" => {
                let compact_delimiter = style == "aligned-no-space";

                // Determine if we need to reformat: skip if table is already aligned
                // UNLESS any alignment or formatting options require reformatting
                let needs_reformat = self.config.column_align != ColumnAlign::Auto
                    || self.config.column_align_header.is_some()
                    || self.config.column_align_body.is_some()
                    || self.config.loose_last_column;

                if !needs_reformat && Self::is_table_already_aligned(&stripped_lines, flavor, compact_delimiter) {
                    return TableFormatResult {
                        lines: table_lines.iter().map(|s| s.to_string()).collect(),
                        auto_compacted: false,
                        aligned_width: None,
                    };
                }

                let column_widths = Self::calculate_column_widths(&stripped_lines, flavor);

                // Calculate aligned table width: 1 (leading pipe) + num_columns * 3 (| cell |) + sum(column_widths)
                let num_columns = column_widths.len();
                let calc_aligned_width = 1 + (num_columns * 3) + column_widths.iter().sum::<usize>();
                aligned_width = Some(calc_aligned_width);

                // Auto-compact: if aligned table exceeds max width, use compact formatting instead
                if calc_aligned_width > self.effective_max_width() {
                    auto_compacted = true;
                    for line in &stripped_lines {
                        let cells = Self::parse_table_row_with_flavor(line, flavor);
                        result.push(Self::format_table_compact(&cells));
                    }
                } else {
                    // Parse column alignments from delimiter row (always at index 1)
                    let delimiter_cells = Self::parse_table_row_with_flavor(stripped_lines[1], flavor);
                    let column_alignments = Self::parse_column_alignments(&delimiter_cells);

                    for (row_idx, line) in stripped_lines.iter().enumerate() {
                        let cells = Self::parse_table_row_with_flavor(line, flavor);
                        let row_type = match row_idx {
                            0 => RowType::Header,
                            1 => RowType::Delimiter,
                            _ => RowType::Body,
                        };
                        let options = RowFormatOptions {
                            row_type,
                            compact_delimiter,
                            column_align: self.config.column_align,
                            column_align_header: self.config.column_align_header,
                            column_align_body: self.config.column_align_body,
                            loose_last_column: self.config.loose_last_column,
                        };
                        result.push(Self::format_table_row(
                            &cells,
                            &column_widths,
                            &column_alignments,
                            &options,
                        ));
                    }
                }
            }
            _ => {
                return TableFormatResult {
                    lines: table_lines.iter().map(|s| s.to_string()).collect(),
                    auto_compacted: false,
                    aligned_width: None,
                };
            }
        }

        // Re-add blockquote prefix and list prefix to all formatted lines
        let prefixed_result: Vec<String> = result
            .into_iter()
            .enumerate()
            .map(|(i, line)| {
                if list_context.is_some() {
                    if i == 0 {
                        // Header line: add list prefix
                        format!("{blockquote_prefix}{list_prefix}{line}")
                    } else {
                        // Continuation lines: add indentation
                        format!("{blockquote_prefix}{continuation_indent}{line}")
                    }
                } else {
                    format!("{blockquote_prefix}{line}")
                }
            })
            .collect();

        TableFormatResult {
            lines: prefixed_result,
            auto_compacted,
            aligned_width,
        }
    }
}

impl Rule for MD060TableFormat {
    fn name(&self) -> &'static str {
        "MD060"
    }

    fn description(&self) -> &'static str {
        "Table columns should be consistently aligned"
    }

    fn should_skip(&self, ctx: &crate::lint_context::LintContext) -> bool {
        !self.config.enabled || !ctx.likely_has_tables()
    }

    fn check(&self, ctx: &crate::lint_context::LintContext) -> LintResult {
        if !self.config.enabled {
            return Ok(Vec::new());
        }

        let content = ctx.content;
        let line_index = &ctx.line_index;
        let mut warnings = Vec::new();

        let lines: Vec<&str> = content.lines().collect();
        let table_blocks = &ctx.table_blocks;

        for table_block in table_blocks {
            let format_result = self.fix_table_block(&lines, table_block, ctx.flavor);

            let table_line_indices: Vec<usize> = std::iter::once(table_block.header_line)
                .chain(std::iter::once(table_block.delimiter_line))
                .chain(table_block.content_lines.iter().copied())
                .collect();

            // Build the whole-table fix once for all warnings in this table
            // This ensures that applying Quick Fix on any row fixes the entire table
            let table_start_line = table_block.start_line + 1; // Convert to 1-indexed
            let table_end_line = table_block.end_line + 1; // Convert to 1-indexed

            // Build the complete fixed table content
            let mut fixed_table_lines: Vec<String> = Vec::with_capacity(table_line_indices.len());
            for (i, &line_idx) in table_line_indices.iter().enumerate() {
                let fixed_line = &format_result.lines[i];
                // Add newline for all lines except the last if the original didn't have one
                if line_idx < lines.len() - 1 {
                    fixed_table_lines.push(format!("{fixed_line}\n"));
                } else {
                    fixed_table_lines.push(fixed_line.clone());
                }
            }
            let table_replacement = fixed_table_lines.concat();
            let table_range = line_index.multi_line_range(table_start_line, table_end_line);

            for (i, &line_idx) in table_line_indices.iter().enumerate() {
                let original = lines[line_idx];
                let fixed = &format_result.lines[i];

                if original != fixed {
                    let (start_line, start_col, end_line, end_col) = calculate_line_range(line_idx + 1, original);

                    let message = if format_result.auto_compacted {
                        if let Some(width) = format_result.aligned_width {
                            format!(
                                "Table too wide for aligned formatting ({} chars > max-width: {})",
                                width,
                                self.effective_max_width()
                            )
                        } else {
                            "Table too wide for aligned formatting".to_string()
                        }
                    } else {
                        "Table columns should be aligned".to_string()
                    };

                    // Each warning uses the same whole-table fix
                    // This ensures Quick Fix on any row aligns the entire table
                    warnings.push(LintWarning {
                        rule_name: Some(self.name().to_string()),
                        severity: Severity::Warning,
                        message,
                        line: start_line,
                        column: start_col,
                        end_line,
                        end_column: end_col,
                        fix: Some(crate::rule::Fix {
                            range: table_range.clone(),
                            replacement: table_replacement.clone(),
                        }),
                    });
                }
            }
        }

        Ok(warnings)
    }

    fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, LintError> {
        if !self.config.enabled {
            return Ok(ctx.content.to_string());
        }

        let content = ctx.content;
        let lines: Vec<&str> = content.lines().collect();
        let table_blocks = &ctx.table_blocks;

        let mut result_lines: Vec<String> = lines.iter().map(|&s| s.to_string()).collect();

        for table_block in table_blocks {
            let format_result = self.fix_table_block(&lines, table_block, ctx.flavor);

            let table_line_indices: Vec<usize> = std::iter::once(table_block.header_line)
                .chain(std::iter::once(table_block.delimiter_line))
                .chain(table_block.content_lines.iter().copied())
                .collect();

            for (i, &line_idx) in table_line_indices.iter().enumerate() {
                result_lines[line_idx] = format_result.lines[i].clone();
            }
        }

        let mut fixed = result_lines.join("\n");
        if content.ends_with('\n') && !fixed.ends_with('\n') {
            fixed.push('\n');
        }
        Ok(fixed)
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn default_config_section(&self) -> Option<(String, toml::Value)> {
        // Build TOML table explicitly to include all keys, even optional ones
        // (serde skips Option::None values, which causes config validation warnings)
        let mut table = toml::map::Map::new();
        table.insert("enabled".to_string(), toml::Value::Boolean(self.config.enabled));
        table.insert("style".to_string(), toml::Value::String(self.config.style.clone()));
        table.insert(
            "max-width".to_string(),
            toml::Value::Integer(self.config.max_width.get() as i64),
        );
        table.insert(
            "column-align".to_string(),
            toml::Value::String(
                match self.config.column_align {
                    ColumnAlign::Auto => "auto",
                    ColumnAlign::Left => "left",
                    ColumnAlign::Center => "center",
                    ColumnAlign::Right => "right",
                }
                .to_string(),
            ),
        );
        // Include optional keys with placeholder values so they're recognized as valid
        table.insert(
            "column-align-header".to_string(),
            toml::Value::String("auto".to_string()),
        );
        table.insert("column-align-body".to_string(), toml::Value::String("auto".to_string()));
        table.insert(
            "loose-last-column".to_string(),
            toml::Value::Boolean(self.config.loose_last_column),
        );

        Some((self.name().to_string(), toml::Value::Table(table)))
    }

    fn from_config(config: &crate::config::Config) -> Box<dyn Rule>
    where
        Self: Sized,
    {
        let rule_config = crate::rule_config_serde::load_rule_config::<MD060Config>(config);
        let md013_config = crate::rule_config_serde::load_rule_config::<MD013Config>(config);

        // Check if MD013 is globally disabled
        let md013_disabled = config.global.disable.iter().any(|r| r == "MD013");

        Box::new(Self::from_config_struct(rule_config, md013_config, md013_disabled))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lint_context::LintContext;
    use crate::types::LineLength;

    /// Helper to create an MD013Config with a specific line length for testing
    fn md013_with_line_length(line_length: usize) -> MD013Config {
        MD013Config {
            line_length: LineLength::from_const(line_length),
            tables: true, // Default: tables are checked
            ..Default::default()
        }
    }

    #[test]
    fn test_md060_disabled_by_default() {
        let rule = MD060TableFormat::default();
        let content = "| Name | Age |\n|---|---|\n| Alice | 30 |";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);

        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 0);

        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, content);
    }

    #[test]
    fn test_md060_align_simple_ascii_table() {
        let rule = MD060TableFormat::new(true, "aligned".to_string());

        let content = "| Name | Age |\n|---|---|\n| Alice | 30 |";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);

        let fixed = rule.fix(&ctx).unwrap();
        let expected = "| Name  | Age |\n| ----- | --- |\n| Alice | 30  |";
        assert_eq!(fixed, expected);

        // Verify all rows have equal length in aligned mode
        let lines: Vec<&str> = fixed.lines().collect();
        assert_eq!(lines[0].len(), lines[1].len());
        assert_eq!(lines[1].len(), lines[2].len());
    }

    #[test]
    fn test_md060_cjk_characters_aligned_correctly() {
        let rule = MD060TableFormat::new(true, "aligned".to_string());

        let content = "| Name | Age |\n|---|---|\n| 中文 | 30 |";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);

        let fixed = rule.fix(&ctx).unwrap();

        let lines: Vec<&str> = fixed.lines().collect();
        let cells_line1 = MD060TableFormat::parse_table_row(lines[0]);
        let cells_line3 = MD060TableFormat::parse_table_row(lines[2]);

        let width1 = MD060TableFormat::calculate_cell_display_width(&cells_line1[0]);
        let width3 = MD060TableFormat::calculate_cell_display_width(&cells_line3[0]);

        assert_eq!(width1, width3);
    }

    #[test]
    fn test_md060_basic_emoji() {
        let rule = MD060TableFormat::new(true, "aligned".to_string());

        let content = "| Status | Name |\n|---|---|\n| ✅ | Test |";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);

        let fixed = rule.fix(&ctx).unwrap();
        assert!(fixed.contains("Status"));
    }

    #[test]
    fn test_md060_zwj_emoji_skipped() {
        let rule = MD060TableFormat::new(true, "aligned".to_string());

        let content = "| Emoji | Name |\n|---|---|\n| 👨‍👩‍👧‍👦 | Family |";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);

        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, content);
    }

    #[test]
    fn test_md060_inline_code_with_escaped_pipes() {
        // In GFM tables, bare pipes in inline code STILL act as cell delimiters.
        // To include a literal pipe in table content (even in code), escape it with \|
        let rule = MD060TableFormat::new(true, "aligned".to_string());

        // CORRECT: `[0-9]\|[0-9]` - the \| is escaped, stays as content (2 columns)
        let content = "| Pattern | Regex |\n|---|---|\n| Time | `[0-9]\\|[0-9]` |";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);

        let fixed = rule.fix(&ctx).unwrap();
        assert!(fixed.contains(r"`[0-9]\|[0-9]`"), "Escaped pipes should be preserved");
    }

    #[test]
    fn test_md060_compact_style() {
        let rule = MD060TableFormat::new(true, "compact".to_string());

        let content = "| Name | Age |\n|---|---|\n| Alice | 30 |";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);

        let fixed = rule.fix(&ctx).unwrap();
        let expected = "| Name | Age |\n| --- | --- |\n| Alice | 30 |";
        assert_eq!(fixed, expected);
    }

    #[test]
    fn test_md060_tight_style() {
        let rule = MD060TableFormat::new(true, "tight".to_string());

        let content = "| Name | Age |\n|---|---|\n| Alice | 30 |";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);

        let fixed = rule.fix(&ctx).unwrap();
        let expected = "|Name|Age|\n|---|---|\n|Alice|30|";
        assert_eq!(fixed, expected);
    }

    #[test]
    fn test_md060_aligned_no_space_style() {
        // Issue #277: aligned-no-space style has no spaces in delimiter row
        let rule = MD060TableFormat::new(true, "aligned-no-space".to_string());

        let content = "| Name | Age |\n|---|---|\n| Alice | 30 |";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);

        let fixed = rule.fix(&ctx).unwrap();

        // Content rows have spaces, delimiter row does not
        let lines: Vec<&str> = fixed.lines().collect();
        assert_eq!(lines[0], "| Name  | Age |", "Header should have spaces around content");
        assert_eq!(
            lines[1], "|-------|-----|",
            "Delimiter should have NO spaces around dashes"
        );
        assert_eq!(lines[2], "| Alice | 30  |", "Content should have spaces around content");

        // All rows should have equal length
        assert_eq!(lines[0].len(), lines[1].len());
        assert_eq!(lines[1].len(), lines[2].len());
    }

    #[test]
    fn test_md060_aligned_no_space_preserves_alignment_indicators() {
        // Alignment indicators (:) should be preserved
        let rule = MD060TableFormat::new(true, "aligned-no-space".to_string());

        let content = "| Left | Center | Right |\n|:---|:---:|---:|\n| A | B | C |";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);

        let fixed = rule.fix(&ctx).unwrap();
        let lines: Vec<&str> = fixed.lines().collect();

        // Verify alignment indicators are preserved without spaces around them
        assert!(
            fixed.contains("|:"),
            "Should have left alignment indicator adjacent to pipe"
        );
        assert!(
            fixed.contains(":|"),
            "Should have right alignment indicator adjacent to pipe"
        );
        // Check for center alignment - the exact dash count depends on column width
        assert!(
            lines[1].contains(":---") && lines[1].contains("---:"),
            "Should have center alignment colons"
        );
    }

    #[test]
    fn test_md060_aligned_no_space_three_column_table() {
        // Test the exact format from issue #277
        let rule = MD060TableFormat::new(true, "aligned-no-space".to_string());

        let content = "| Header 1 | Header 2 | Header 3 |\n|---|---|---|\n| Row 1, Col 1 | Row 1, Col 2 | Row 1, Col 3 |\n| Row 2, Col 1 | Row 2, Col 2 | Row 2, Col 3 |";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);

        let fixed = rule.fix(&ctx).unwrap();
        let lines: Vec<&str> = fixed.lines().collect();

        // Verify delimiter row format: |--------------|--------------|--------------|
        assert!(lines[1].starts_with("|---"), "Delimiter should start with |---");
        assert!(lines[1].ends_with("---|"), "Delimiter should end with ---|");
        assert!(!lines[1].contains("| -"), "Delimiter should NOT have space after pipe");
        assert!(!lines[1].contains("- |"), "Delimiter should NOT have space before pipe");
    }

    #[test]
    fn test_md060_aligned_no_space_auto_compacts_wide_tables() {
        // Auto-compact should work with aligned-no-space when table exceeds max-width
        let config = MD060Config {
            enabled: true,
            style: "aligned-no-space".to_string(),
            max_width: LineLength::from_const(50),
            column_align: ColumnAlign::Auto,
            column_align_header: None,
            column_align_body: None,
            loose_last_column: false,
        };
        let rule = MD060TableFormat::from_config_struct(config, md013_with_line_length(80), false);

        // Wide table that exceeds 50 chars when aligned
        let content = "| Very Long Column Header A | Very Long Column Header B | Very Long Column Header C |\n|---|---|---|\n| x | y | z |";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);

        let fixed = rule.fix(&ctx).unwrap();

        // Should auto-compact to compact style (not aligned-no-space)
        assert!(
            fixed.contains("| --- |"),
            "Should be compact format when exceeding max-width"
        );
    }

    #[test]
    fn test_md060_aligned_no_space_cjk_characters() {
        // CJK characters should be handled correctly
        let rule = MD060TableFormat::new(true, "aligned-no-space".to_string());

        let content = "| Name | City |\n|---|---|\n| 中文 | 東京 |";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);

        let fixed = rule.fix(&ctx).unwrap();
        let lines: Vec<&str> = fixed.lines().collect();

        // All rows should have equal DISPLAY width (not byte length)
        // CJK characters are double-width, so byte length differs from display width
        use unicode_width::UnicodeWidthStr;
        assert_eq!(
            lines[0].width(),
            lines[1].width(),
            "Header and delimiter should have same display width"
        );
        assert_eq!(
            lines[1].width(),
            lines[2].width(),
            "Delimiter and content should have same display width"
        );

        // Delimiter should have no spaces
        assert!(!lines[1].contains("| -"), "Delimiter should NOT have space after pipe");
    }

    #[test]
    fn test_md060_aligned_no_space_minimum_width() {
        // Minimum column width (3 dashes) should be respected
        let rule = MD060TableFormat::new(true, "aligned-no-space".to_string());

        let content = "| A | B |\n|-|-|\n| 1 | 2 |";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);

        let fixed = rule.fix(&ctx).unwrap();
        let lines: Vec<&str> = fixed.lines().collect();

        // Should have at least 3 dashes per column (GFM requirement)
        assert!(lines[1].contains("---"), "Should have minimum 3 dashes");
        // All rows should have equal length
        assert_eq!(lines[0].len(), lines[1].len());
        assert_eq!(lines[1].len(), lines[2].len());
    }

    #[test]
    fn test_md060_any_style_consistency() {
        let rule = MD060TableFormat::new(true, "any".to_string());

        // Table is already compact, should stay compact
        let content = "| Name | Age |\n| --- | --- |\n| Alice | 30 |";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);

        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, content);

        // Table is aligned, should stay aligned
        let content_aligned = "| Name  | Age |\n| ----- | --- |\n| Alice | 30  |";
        let ctx_aligned = LintContext::new(content_aligned, crate::config::MarkdownFlavor::Standard, None);

        let fixed_aligned = rule.fix(&ctx_aligned).unwrap();
        assert_eq!(fixed_aligned, content_aligned);
    }

    #[test]
    fn test_md060_empty_cells() {
        let rule = MD060TableFormat::new(true, "aligned".to_string());

        let content = "| A | B |\n|---|---|\n|  | X |";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);

        let fixed = rule.fix(&ctx).unwrap();
        assert!(fixed.contains("|"));
    }

    #[test]
    fn test_md060_mixed_content() {
        let rule = MD060TableFormat::new(true, "aligned".to_string());

        let content = "| Name | Age | City |\n|---|---|---|\n| 中文 | 30 | NYC |";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);

        let fixed = rule.fix(&ctx).unwrap();
        assert!(fixed.contains("中文"));
        assert!(fixed.contains("NYC"));
    }

    #[test]
    fn test_md060_preserve_alignment_indicators() {
        let rule = MD060TableFormat::new(true, "aligned".to_string());

        let content = "| Left | Center | Right |\n|:---|:---:|---:|\n| A | B | C |";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);

        let fixed = rule.fix(&ctx).unwrap();

        assert!(fixed.contains(":---"), "Should contain left alignment");
        assert!(fixed.contains(":----:"), "Should contain center alignment");
        assert!(fixed.contains("----:"), "Should contain right alignment");
    }

    #[test]
    fn test_md060_minimum_column_width() {
        let rule = MD060TableFormat::new(true, "aligned".to_string());

        // Test with very short column content to ensure minimum width of 3
        // GFM requires at least 3 dashes in delimiter rows
        let content = "| ID | Name |\n|-|-|\n| 1 | A |";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);

        let fixed = rule.fix(&ctx).unwrap();

        let lines: Vec<&str> = fixed.lines().collect();
        assert_eq!(lines[0].len(), lines[1].len());
        assert_eq!(lines[1].len(), lines[2].len());

        // Verify minimum width is enforced
        assert!(fixed.contains("ID "), "Short content should be padded");
        assert!(fixed.contains("---"), "Delimiter should have at least 3 dashes");
    }

    #[test]
    fn test_md060_auto_compact_exceeds_default_threshold() {
        // Default max_width = 0, which inherits from default MD013 line_length = 80
        let config = MD060Config {
            enabled: true,
            style: "aligned".to_string(),
            max_width: LineLength::from_const(0),
            column_align: ColumnAlign::Auto,
            column_align_header: None,
            column_align_body: None,
            loose_last_column: false,
        };
        let rule = MD060TableFormat::from_config_struct(config, md013_with_line_length(80), false);

        // Table that would be 85 chars when aligned (exceeds 80)
        // Formula: 1 + (3 * 3) + (20 + 20 + 30) = 1 + 9 + 70 = 80 chars
        // But with actual content padding it will exceed
        let content = "| Very Long Column Header | Another Long Header | Third Very Long Header Column |\n|---|---|---|\n| Short | Data | Here |";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);

        let fixed = rule.fix(&ctx).unwrap();

        // Should use compact formatting (single spaces)
        assert!(fixed.contains("| Very Long Column Header | Another Long Header | Third Very Long Header Column |"));
        assert!(fixed.contains("| --- | --- | --- |"));
        assert!(fixed.contains("| Short | Data | Here |"));

        // Verify it's compact (no extra padding)
        let lines: Vec<&str> = fixed.lines().collect();
        // In compact mode, lines can have different lengths
        assert!(lines[0].len() != lines[1].len() || lines[1].len() != lines[2].len());
    }

    #[test]
    fn test_md060_auto_compact_exceeds_explicit_threshold() {
        // Explicit max_width = 50
        let config = MD060Config {
            enabled: true,
            style: "aligned".to_string(),
            max_width: LineLength::from_const(50),
            column_align: ColumnAlign::Auto,
            column_align_header: None,
            column_align_body: None,
            loose_last_column: false,
        };
        let rule = MD060TableFormat::from_config_struct(config, md013_with_line_length(80), false); // MD013 setting doesn't matter

        // Table that would exceed 50 chars when aligned
        // Column widths: 25 + 25 + 25 = 75 chars
        // Formula: 1 + (3 * 3) + 75 = 85 chars (exceeds 50)
        let content = "| Very Long Column Header A | Very Long Column Header B | Very Long Column Header C |\n|---|---|---|\n| Data | Data | Data |";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);

        let fixed = rule.fix(&ctx).unwrap();

        // Should use compact formatting (single spaces, no extra padding)
        assert!(
            fixed.contains("| Very Long Column Header A | Very Long Column Header B | Very Long Column Header C |")
        );
        assert!(fixed.contains("| --- | --- | --- |"));
        assert!(fixed.contains("| Data | Data | Data |"));

        // Verify it's compact (lines have different lengths)
        let lines: Vec<&str> = fixed.lines().collect();
        assert!(lines[0].len() != lines[2].len());
    }

    #[test]
    fn test_md060_stays_aligned_under_threshold() {
        // max_width = 100, table will be under this
        let config = MD060Config {
            enabled: true,
            style: "aligned".to_string(),
            max_width: LineLength::from_const(100),
            column_align: ColumnAlign::Auto,
            column_align_header: None,
            column_align_body: None,
            loose_last_column: false,
        };
        let rule = MD060TableFormat::from_config_struct(config, md013_with_line_length(80), false);

        // Small table that fits well under 100 chars
        let content = "| Name | Age |\n|---|---|\n| Alice | 30 |";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);

        let fixed = rule.fix(&ctx).unwrap();

        // Should use aligned formatting (all lines same length)
        let expected = "| Name  | Age |\n| ----- | --- |\n| Alice | 30  |";
        assert_eq!(fixed, expected);

        let lines: Vec<&str> = fixed.lines().collect();
        assert_eq!(lines[0].len(), lines[1].len());
        assert_eq!(lines[1].len(), lines[2].len());
    }

    #[test]
    fn test_md060_width_calculation_formula() {
        // Verify the width calculation formula: 1 + (num_columns * 3) + sum(column_widths)
        let config = MD060Config {
            enabled: true,
            style: "aligned".to_string(),
            max_width: LineLength::from_const(0),
            column_align: ColumnAlign::Auto,
            column_align_header: None,
            column_align_body: None,
            loose_last_column: false,
        };
        let rule = MD060TableFormat::from_config_struct(config, md013_with_line_length(30), false);

        // Create a table where we know exact column widths: 5 + 5 + 5 = 15
        // Expected aligned width: 1 + (3 * 3) + 15 = 1 + 9 + 15 = 25 chars
        // This is under 30, so should stay aligned
        let content = "| AAAAA | BBBBB | CCCCC |\n|---|---|---|\n| AAAAA | BBBBB | CCCCC |";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);

        let fixed = rule.fix(&ctx).unwrap();

        // Should be aligned
        let lines: Vec<&str> = fixed.lines().collect();
        assert_eq!(lines[0].len(), lines[1].len());
        assert_eq!(lines[1].len(), lines[2].len());
        assert_eq!(lines[0].len(), 25); // Verify formula

        // Now test with threshold = 24 (just under aligned width)
        let config_tight = MD060Config {
            enabled: true,
            style: "aligned".to_string(),
            max_width: LineLength::from_const(24),
            column_align: ColumnAlign::Auto,
            column_align_header: None,
            column_align_body: None,
            loose_last_column: false,
        };
        let rule_tight = MD060TableFormat::from_config_struct(config_tight, md013_with_line_length(80), false);

        let fixed_compact = rule_tight.fix(&ctx).unwrap();

        // Should be compact now (25 > 24)
        assert!(fixed_compact.contains("| AAAAA | BBBBB | CCCCC |"));
        assert!(fixed_compact.contains("| --- | --- | --- |"));
    }

    #[test]
    fn test_md060_very_wide_table_auto_compacts() {
        let config = MD060Config {
            enabled: true,
            style: "aligned".to_string(),
            max_width: LineLength::from_const(0),
            column_align: ColumnAlign::Auto,
            column_align_header: None,
            column_align_body: None,
            loose_last_column: false,
        };
        let rule = MD060TableFormat::from_config_struct(config, md013_with_line_length(80), false);

        // Very wide table with many columns
        // 8 columns with widths of 12 chars each = 96 chars
        // Formula: 1 + (8 * 3) + 96 = 121 chars (exceeds 80)
        let content = "| Column One A | Column Two B | Column Three | Column Four D | Column Five E | Column Six FG | Column Seven | Column Eight |\n|---|---|---|---|---|---|---|---|\n| A | B | C | D | E | F | G | H |";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);

        let fixed = rule.fix(&ctx).unwrap();

        // Should be compact (table would be way over 80 chars aligned)
        assert!(fixed.contains("| Column One A | Column Two B | Column Three | Column Four D | Column Five E | Column Six FG | Column Seven | Column Eight |"));
        assert!(fixed.contains("| --- | --- | --- | --- | --- | --- | --- | --- |"));
    }

    #[test]
    fn test_md060_inherit_from_md013_line_length() {
        // max_width = 0 should inherit from MD013's line_length
        let config = MD060Config {
            enabled: true,
            style: "aligned".to_string(),
            max_width: LineLength::from_const(0), // Inherit
            column_align: ColumnAlign::Auto,
            column_align_header: None,
            column_align_body: None,
            loose_last_column: false,
        };

        // Test with different MD013 line_length values
        let rule_80 = MD060TableFormat::from_config_struct(config.clone(), md013_with_line_length(80), false);
        let rule_120 = MD060TableFormat::from_config_struct(config.clone(), md013_with_line_length(120), false);

        // Medium-sized table
        let content = "| Column Header A | Column Header B | Column Header C |\n|---|---|---|\n| Some Data | More Data | Even More |";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);

        // With 80 char limit, likely compacts
        let _fixed_80 = rule_80.fix(&ctx).unwrap();

        // With 120 char limit, likely stays aligned
        let fixed_120 = rule_120.fix(&ctx).unwrap();

        // Verify 120 is aligned (all lines same length)
        let lines_120: Vec<&str> = fixed_120.lines().collect();
        assert_eq!(lines_120[0].len(), lines_120[1].len());
        assert_eq!(lines_120[1].len(), lines_120[2].len());
    }

    #[test]
    fn test_md060_edge_case_exactly_at_threshold() {
        // Create table that's exactly at the threshold
        // Formula: 1 + (num_columns * 3) + sum(column_widths) = max_width
        // For 2 columns with widths 5 and 5: 1 + 6 + 10 = 17
        let config = MD060Config {
            enabled: true,
            style: "aligned".to_string(),
            max_width: LineLength::from_const(17),
            column_align: ColumnAlign::Auto,
            column_align_header: None,
            column_align_body: None,
            loose_last_column: false,
        };
        let rule = MD060TableFormat::from_config_struct(config, md013_with_line_length(80), false);

        let content = "| AAAAA | BBBBB |\n|---|---|\n| AAAAA | BBBBB |";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);

        let fixed = rule.fix(&ctx).unwrap();

        // At threshold (17 <= 17), should stay aligned
        let lines: Vec<&str> = fixed.lines().collect();
        assert_eq!(lines[0].len(), 17);
        assert_eq!(lines[0].len(), lines[1].len());
        assert_eq!(lines[1].len(), lines[2].len());

        // Now test with threshold = 16 (just under)
        let config_under = MD060Config {
            enabled: true,
            style: "aligned".to_string(),
            max_width: LineLength::from_const(16),
            column_align: ColumnAlign::Auto,
            column_align_header: None,
            column_align_body: None,
            loose_last_column: false,
        };
        let rule_under = MD060TableFormat::from_config_struct(config_under, md013_with_line_length(80), false);

        let fixed_compact = rule_under.fix(&ctx).unwrap();

        // Should compact (17 > 16)
        assert!(fixed_compact.contains("| AAAAA | BBBBB |"));
        assert!(fixed_compact.contains("| --- | --- |"));
    }

    #[test]
    fn test_md060_auto_compact_warning_message() {
        // Verify that auto-compact generates an informative warning
        let config = MD060Config {
            enabled: true,
            style: "aligned".to_string(),
            max_width: LineLength::from_const(50),
            column_align: ColumnAlign::Auto,
            column_align_header: None,
            column_align_body: None,
            loose_last_column: false,
        };
        let rule = MD060TableFormat::from_config_struct(config, md013_with_line_length(80), false);

        // Table that will be auto-compacted (exceeds 50 chars when aligned)
        let content = "| Very Long Column Header A | Very Long Column Header B | Very Long Column Header C |\n|---|---|---|\n| Data | Data | Data |";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);

        let warnings = rule.check(&ctx).unwrap();

        // Should generate warnings with auto-compact message
        assert!(!warnings.is_empty(), "Should generate warnings");

        let auto_compact_warnings: Vec<_> = warnings
            .iter()
            .filter(|w| w.message.contains("too wide for aligned formatting"))
            .collect();

        assert!(!auto_compact_warnings.is_empty(), "Should have auto-compact warning");

        // Verify the warning message includes the width and threshold
        let first_warning = auto_compact_warnings[0];
        assert!(first_warning.message.contains("85 chars > max-width: 50"));
        assert!(first_warning.message.contains("Table too wide for aligned formatting"));
    }

    #[test]
    fn test_md060_issue_129_detect_style_from_all_rows() {
        // Issue #129: detect_table_style should check all rows, not just the first row
        // If header row has single-space padding but content rows have extra padding,
        // the table should be detected as "aligned" and preserved
        let rule = MD060TableFormat::new(true, "any".to_string());

        // Table where header looks compact but content is aligned
        let content = "| a long heading | another long heading |\n\
                       | -------------- | -------------------- |\n\
                       | a              | 1                    |\n\
                       | b b            | 2                    |\n\
                       | c c c          | 3                    |";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);

        let fixed = rule.fix(&ctx).unwrap();

        // Should preserve the aligned formatting of content rows
        assert!(
            fixed.contains("| a              | 1                    |"),
            "Should preserve aligned padding in first content row"
        );
        assert!(
            fixed.contains("| b b            | 2                    |"),
            "Should preserve aligned padding in second content row"
        );
        assert!(
            fixed.contains("| c c c          | 3                    |"),
            "Should preserve aligned padding in third content row"
        );

        // Entire table should remain unchanged because it's already properly aligned
        assert_eq!(fixed, content, "Table should be detected as aligned and preserved");
    }

    #[test]
    fn test_md060_regular_alignment_warning_message() {
        // Verify that regular alignment (not auto-compact) generates normal warning
        let config = MD060Config {
            enabled: true,
            style: "aligned".to_string(),
            max_width: LineLength::from_const(100), // Large enough to not trigger auto-compact
            column_align: ColumnAlign::Auto,
            column_align_header: None,
            column_align_body: None,
            loose_last_column: false,
        };
        let rule = MD060TableFormat::from_config_struct(config, md013_with_line_length(80), false);

        // Small misaligned table
        let content = "| Name | Age |\n|---|---|\n| Alice | 30 |";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);

        let warnings = rule.check(&ctx).unwrap();

        // Should generate warnings
        assert!(!warnings.is_empty(), "Should generate warnings");

        // Verify it's the standard alignment message, not auto-compact
        assert!(warnings[0].message.contains("Table columns should be aligned"));
        assert!(!warnings[0].message.contains("too wide"));
        assert!(!warnings[0].message.contains("max-width"));
    }

    // === Issue #219: Unlimited table width tests ===

    #[test]
    fn test_md060_unlimited_when_md013_disabled() {
        // When MD013 is globally disabled, max_width should be unlimited
        let config = MD060Config {
            enabled: true,
            style: "aligned".to_string(),
            max_width: LineLength::from_const(0), // Inherit
            column_align: ColumnAlign::Auto,
            column_align_header: None,
            column_align_body: None,
            loose_last_column: false,
        };
        let md013_config = MD013Config::default();
        let rule = MD060TableFormat::from_config_struct(config, md013_config, true /* disabled */);

        // Very wide table that would normally exceed 80 chars
        let content = "| Very Long Column Header A | Very Long Column Header B | Very Long Column Header C |\n|---|---|---|\n| data | data | data |";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        // Should be aligned (not compacted) since MD013 is disabled
        let lines: Vec<&str> = fixed.lines().collect();
        // In aligned mode, all lines have the same length
        assert_eq!(
            lines[0].len(),
            lines[1].len(),
            "Table should be aligned when MD013 is disabled"
        );
    }

    #[test]
    fn test_md060_unlimited_when_md013_tables_false() {
        // When MD013.tables = false, max_width should be unlimited
        let config = MD060Config {
            enabled: true,
            style: "aligned".to_string(),
            max_width: LineLength::from_const(0),
            column_align: ColumnAlign::Auto,
            column_align_header: None,
            column_align_body: None,
            loose_last_column: false,
        };
        let md013_config = MD013Config {
            tables: false, // User doesn't care about table line length
            line_length: LineLength::from_const(80),
            ..Default::default()
        };
        let rule = MD060TableFormat::from_config_struct(config, md013_config, false);

        // Wide table that would exceed 80 chars
        let content = "| Very Long Header A | Very Long Header B | Very Long Header C |\n|---|---|---|\n| x | y | z |";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        // Should be aligned (no auto-compact since tables=false)
        let lines: Vec<&str> = fixed.lines().collect();
        assert_eq!(
            lines[0].len(),
            lines[1].len(),
            "Table should be aligned when MD013.tables=false"
        );
    }

    #[test]
    fn test_md060_unlimited_when_md013_line_length_zero() {
        // When MD013.line_length = 0, max_width should be unlimited
        let config = MD060Config {
            enabled: true,
            style: "aligned".to_string(),
            max_width: LineLength::from_const(0),
            column_align: ColumnAlign::Auto,
            column_align_header: None,
            column_align_body: None,
            loose_last_column: false,
        };
        let md013_config = MD013Config {
            tables: true,
            line_length: LineLength::from_const(0), // No limit
            ..Default::default()
        };
        let rule = MD060TableFormat::from_config_struct(config, md013_config, false);

        // Wide table
        let content = "| Very Long Header | Another Long Header | Third Long Header |\n|---|---|---|\n| x | y | z |";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        // Should be aligned
        let lines: Vec<&str> = fixed.lines().collect();
        assert_eq!(
            lines[0].len(),
            lines[1].len(),
            "Table should be aligned when MD013.line_length=0"
        );
    }

    #[test]
    fn test_md060_explicit_max_width_overrides_md013_settings() {
        // Explicit max_width should always take precedence
        let config = MD060Config {
            enabled: true,
            style: "aligned".to_string(),
            max_width: LineLength::from_const(50), // Explicit limit
            column_align: ColumnAlign::Auto,
            column_align_header: None,
            column_align_body: None,
            loose_last_column: false,
        };
        let md013_config = MD013Config {
            tables: false,                          // This would make it unlimited...
            line_length: LineLength::from_const(0), // ...and this too
            ..Default::default()
        };
        let rule = MD060TableFormat::from_config_struct(config, md013_config, false);

        // Wide table that exceeds explicit 50-char limit
        let content = "| Very Long Column Header A | Very Long Column Header B | Very Long Column Header C |\n|---|---|---|\n| x | y | z |";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        // Should be compact (explicit max_width = 50 overrides MD013 settings)
        assert!(
            fixed.contains("| --- |"),
            "Should be compact format due to explicit max_width"
        );
    }

    #[test]
    fn test_md060_inherits_md013_line_length_when_tables_enabled() {
        // When MD013.tables = true and MD013.line_length is set, inherit that limit
        let config = MD060Config {
            enabled: true,
            style: "aligned".to_string(),
            max_width: LineLength::from_const(0), // Inherit
            column_align: ColumnAlign::Auto,
            column_align_header: None,
            column_align_body: None,
            loose_last_column: false,
        };
        let md013_config = MD013Config {
            tables: true,
            line_length: LineLength::from_const(50), // 50 char limit
            ..Default::default()
        };
        let rule = MD060TableFormat::from_config_struct(config, md013_config, false);

        // Wide table that exceeds 50 chars
        let content = "| Very Long Column Header A | Very Long Column Header B | Very Long Column Header C |\n|---|---|---|\n| x | y | z |";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        // Should be compact (inherited 50-char limit from MD013)
        assert!(
            fixed.contains("| --- |"),
            "Should be compact format when inheriting MD013 limit"
        );
    }

    // === Issue #311: aligned-no-space style tests ===

    #[test]
    fn test_aligned_no_space_reformats_spaced_delimiter() {
        // Table with "aligned" style (spaces around dashes) should be reformatted
        // when target style is "aligned-no-space"
        let config = MD060Config {
            enabled: true,
            style: "aligned-no-space".to_string(),
            max_width: LineLength::from_const(0),
            column_align: ColumnAlign::Auto,
            column_align_header: None,
            column_align_body: None,
            loose_last_column: false,
        };
        let rule = MD060TableFormat::from_config_struct(config, MD013Config::default(), false);

        // Input: aligned table with spaces around dashes
        let content = "| Header 1 | Header 2 |\n| -------- | -------- |\n| Cell 1   | Cell 2   |";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        // Should have no spaces around dashes in delimiter row
        // The dashes may be longer to match column width, but should have no spaces
        assert!(
            !fixed.contains("| ----"),
            "Delimiter should NOT have spaces after pipe. Got:\n{fixed}"
        );
        assert!(
            !fixed.contains("---- |"),
            "Delimiter should NOT have spaces before pipe. Got:\n{fixed}"
        );
        // Verify it has the compact delimiter format (dashes touching pipes)
        assert!(
            fixed.contains("|----"),
            "Delimiter should have dashes touching the leading pipe. Got:\n{fixed}"
        );
    }

    #[test]
    fn test_aligned_reformats_compact_delimiter() {
        // Table with "aligned-no-space" style (no spaces around dashes) should be reformatted
        // when target style is "aligned"
        let config = MD060Config {
            enabled: true,
            style: "aligned".to_string(),
            max_width: LineLength::from_const(0),
            column_align: ColumnAlign::Auto,
            column_align_header: None,
            column_align_body: None,
            loose_last_column: false,
        };
        let rule = MD060TableFormat::from_config_struct(config, MD013Config::default(), false);

        // Input: aligned-no-space table (no spaces around dashes)
        let content = "| Header 1 | Header 2 |\n|----------|----------|\n| Cell 1   | Cell 2   |";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        // Should have spaces around dashes in delimiter row
        assert!(
            fixed.contains("| -------- | -------- |") || fixed.contains("| ---------- | ---------- |"),
            "Delimiter should have spaces around dashes. Got:\n{fixed}"
        );
    }

    #[test]
    fn test_aligned_no_space_preserves_matching_table() {
        // Table already in "aligned-no-space" style should be preserved
        let config = MD060Config {
            enabled: true,
            style: "aligned-no-space".to_string(),
            max_width: LineLength::from_const(0),
            column_align: ColumnAlign::Auto,
            column_align_header: None,
            column_align_body: None,
            loose_last_column: false,
        };
        let rule = MD060TableFormat::from_config_struct(config, MD013Config::default(), false);

        // Input: already in aligned-no-space style
        let content = "| Header 1 | Header 2 |\n|----------|----------|\n| Cell 1   | Cell 2   |";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        // Should be preserved as-is
        assert_eq!(
            fixed, content,
            "Table already in aligned-no-space style should be preserved"
        );
    }

    #[test]
    fn test_aligned_preserves_matching_table() {
        // Table already in "aligned" style should be preserved
        let config = MD060Config {
            enabled: true,
            style: "aligned".to_string(),
            max_width: LineLength::from_const(0),
            column_align: ColumnAlign::Auto,
            column_align_header: None,
            column_align_body: None,
            loose_last_column: false,
        };
        let rule = MD060TableFormat::from_config_struct(config, MD013Config::default(), false);

        // Input: already in aligned style
        let content = "| Header 1 | Header 2 |\n| -------- | -------- |\n| Cell 1   | Cell 2   |";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        // Should be preserved as-is
        assert_eq!(fixed, content, "Table already in aligned style should be preserved");
    }

    #[test]
    fn test_cjk_table_display_width_consistency() {
        // Test that is_table_already_aligned correctly uses display width, not byte length
        // CJK characters have display width of 2, but byte length of 3 in UTF-8
        //
        // This table is NOT aligned because line lengths differ
        // (CJK chars take 3 bytes in UTF-8 but only 2 columns in display)
        let table_lines = vec!["| 名前 | Age |", "|------|-----|", "| 田中 | 25  |"];

        // First check is raw line length equality (byte-based), which fails
        let is_aligned =
            MD060TableFormat::is_table_already_aligned(&table_lines, crate::config::MarkdownFlavor::Standard, false);
        assert!(
            !is_aligned,
            "Table with uneven raw line lengths should NOT be considered aligned"
        );
    }

    #[test]
    fn test_cjk_width_calculation_in_aligned_check() {
        // calculate_cell_display_width trims content before calculating width
        // Verify CJK width is correctly calculated (2 per character)
        let cjk_width = MD060TableFormat::calculate_cell_display_width("名前");
        assert_eq!(cjk_width, 4, "Two CJK characters should have display width 4");

        let ascii_width = MD060TableFormat::calculate_cell_display_width("Age");
        assert_eq!(ascii_width, 3, "Three ASCII characters should have display width 3");

        // Test that spacing is trimmed before width calculation
        let padded_cjk = MD060TableFormat::calculate_cell_display_width(" 名前 ");
        assert_eq!(padded_cjk, 4, "Padded CJK should have same width after trim");

        // Test mixed content
        let mixed = MD060TableFormat::calculate_cell_display_width(" 日本語ABC ");
        // 3 CJK chars (width 6) + 3 ASCII (width 3) = 9
        assert_eq!(mixed, 9, "Mixed CJK/ASCII content");
    }

    // === Issue #317: column-align option tests ===

    #[test]
    fn test_md060_column_align_left() {
        // Default/explicit left alignment
        let config = MD060Config {
            enabled: true,
            style: "aligned".to_string(),
            max_width: LineLength::from_const(0),
            column_align: ColumnAlign::Left,
            column_align_header: None,
            column_align_body: None,
            loose_last_column: false,
        };
        let rule = MD060TableFormat::from_config_struct(config, MD013Config::default(), false);

        let content = "| Name | Age | City |\n|---|---|---|\n| Alice | 30 | Seattle |\n| Bob | 25 | Portland |";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);

        let fixed = rule.fix(&ctx).unwrap();
        let lines: Vec<&str> = fixed.lines().collect();

        // Left aligned: content on left, padding on right
        assert!(
            lines[2].contains("| Alice "),
            "Content should be left-aligned (Alice should have trailing padding)"
        );
        assert!(
            lines[3].contains("| Bob   "),
            "Content should be left-aligned (Bob should have trailing padding)"
        );
    }

    #[test]
    fn test_md060_column_align_center() {
        // Center alignment forces all columns to center
        let config = MD060Config {
            enabled: true,
            style: "aligned".to_string(),
            max_width: LineLength::from_const(0),
            column_align: ColumnAlign::Center,
            column_align_header: None,
            column_align_body: None,
            loose_last_column: false,
        };
        let rule = MD060TableFormat::from_config_struct(config, MD013Config::default(), false);

        let content = "| Name | Age | City |\n|---|---|---|\n| Alice | 30 | Seattle |\n| Bob | 25 | Portland |";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);

        let fixed = rule.fix(&ctx).unwrap();
        let lines: Vec<&str> = fixed.lines().collect();

        // Center aligned: padding split on both sides
        // "Bob" (3 chars) in "Name" column (5 chars) = 2 padding total, 1 left, 1 right
        assert!(
            lines[3].contains("|  Bob  |"),
            "Bob should be centered with padding on both sides. Got: {}",
            lines[3]
        );
    }

    #[test]
    fn test_md060_column_align_right() {
        // Right alignment forces all columns to right-align
        let config = MD060Config {
            enabled: true,
            style: "aligned".to_string(),
            max_width: LineLength::from_const(0),
            column_align: ColumnAlign::Right,
            column_align_header: None,
            column_align_body: None,
            loose_last_column: false,
        };
        let rule = MD060TableFormat::from_config_struct(config, MD013Config::default(), false);

        let content = "| Name | Age | City |\n|---|---|---|\n| Alice | 30 | Seattle |\n| Bob | 25 | Portland |";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);

        let fixed = rule.fix(&ctx).unwrap();
        let lines: Vec<&str> = fixed.lines().collect();

        // Right aligned: padding on left, content on right
        assert!(
            lines[3].contains("|   Bob |"),
            "Bob should be right-aligned with padding on left. Got: {}",
            lines[3]
        );
    }

    #[test]
    fn test_md060_column_align_auto_respects_delimiter() {
        // Auto mode (default) should respect delimiter row alignment indicators
        let config = MD060Config {
            enabled: true,
            style: "aligned".to_string(),
            max_width: LineLength::from_const(0),
            column_align: ColumnAlign::Auto,
            column_align_header: None,
            column_align_body: None,
            loose_last_column: false,
        };
        let rule = MD060TableFormat::from_config_struct(config, MD013Config::default(), false);

        // Left, center, right columns via delimiter indicators
        let content = "| Left | Center | Right |\n|:---|:---:|---:|\n| A | B | C |";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);

        let fixed = rule.fix(&ctx).unwrap();

        // Verify alignment is applied per-column based on delimiter
        assert!(fixed.contains("| A "), "Left column should be left-aligned");
        // Center and right columns with longer content in header
        let lines: Vec<&str> = fixed.lines().collect();
        // The content row should have B centered and C right-aligned
        // B (1 char) in "Center" (6 chars) = 5 padding, ~2 left, ~3 right
        // C (1 char) in "Right" (5 chars) = 4 padding, all on left
        assert!(
            lines[2].contains(" C |"),
            "Right column should be right-aligned. Got: {}",
            lines[2]
        );
    }

    #[test]
    fn test_md060_column_align_overrides_delimiter_indicators() {
        // column-align should override delimiter row indicators
        let config = MD060Config {
            enabled: true,
            style: "aligned".to_string(),
            max_width: LineLength::from_const(0),
            column_align: ColumnAlign::Right, // Override all to right
            column_align_header: None,
            column_align_body: None,
            loose_last_column: false,
        };
        let rule = MD060TableFormat::from_config_struct(config, MD013Config::default(), false);

        // Delimiter says left, center, right - but we override all to right
        let content = "| Left | Center | Right |\n|:---|:---:|---:|\n| A | B | C |";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);

        let fixed = rule.fix(&ctx).unwrap();
        let lines: Vec<&str> = fixed.lines().collect();

        // ALL columns should be right-aligned despite delimiter indicators
        // "A" in "Left" column (4 chars minimum due to header length) should be right-aligned
        assert!(
            lines[2].contains("    A |") || lines[2].contains("   A |"),
            "Even left-indicated column should be right-aligned. Got: {}",
            lines[2]
        );
    }

    #[test]
    fn test_md060_column_align_with_aligned_no_space() {
        // column-align should work with aligned-no-space style
        let config = MD060Config {
            enabled: true,
            style: "aligned-no-space".to_string(),
            max_width: LineLength::from_const(0),
            column_align: ColumnAlign::Center,
            column_align_header: None,
            column_align_body: None,
            loose_last_column: false,
        };
        let rule = MD060TableFormat::from_config_struct(config, MD013Config::default(), false);

        let content = "| Name | Age |\n|---|---|\n| Alice | 30 |\n| Bob | 25 |";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);

        let fixed = rule.fix(&ctx).unwrap();
        let lines: Vec<&str> = fixed.lines().collect();

        // Delimiter row should have no spaces (aligned-no-space)
        assert!(
            lines[1].contains("|---"),
            "Delimiter should have no spaces in aligned-no-space style. Got: {}",
            lines[1]
        );
        // Content should still be centered
        assert!(
            lines[3].contains("|  Bob  |"),
            "Content should be centered. Got: {}",
            lines[3]
        );
    }

    #[test]
    fn test_md060_column_align_config_parsing() {
        // Test that column-align config is correctly parsed
        let toml_str = r#"
enabled = true
style = "aligned"
column-align = "center"
"#;
        let config: MD060Config = toml::from_str(toml_str).expect("Should parse config");
        assert_eq!(config.column_align, ColumnAlign::Center);

        let toml_str = r#"
enabled = true
style = "aligned"
column-align = "right"
"#;
        let config: MD060Config = toml::from_str(toml_str).expect("Should parse config");
        assert_eq!(config.column_align, ColumnAlign::Right);

        let toml_str = r#"
enabled = true
style = "aligned"
column-align = "left"
"#;
        let config: MD060Config = toml::from_str(toml_str).expect("Should parse config");
        assert_eq!(config.column_align, ColumnAlign::Left);

        let toml_str = r#"
enabled = true
style = "aligned"
column-align = "auto"
"#;
        let config: MD060Config = toml::from_str(toml_str).expect("Should parse config");
        assert_eq!(config.column_align, ColumnAlign::Auto);
    }

    #[test]
    fn test_md060_column_align_default_is_auto() {
        // Without column-align specified, default should be Auto
        let toml_str = r#"
enabled = true
style = "aligned"
"#;
        let config: MD060Config = toml::from_str(toml_str).expect("Should parse config");
        assert_eq!(config.column_align, ColumnAlign::Auto);
    }

    #[test]
    fn test_md060_column_align_reformats_already_aligned_table() {
        // A table that is already aligned (left) should be reformatted when column-align=right
        let config = MD060Config {
            enabled: true,
            style: "aligned".to_string(),
            max_width: LineLength::from_const(0),
            column_align: ColumnAlign::Right,
            column_align_header: None,
            column_align_body: None,
            loose_last_column: false,
        };
        let rule = MD060TableFormat::from_config_struct(config, MD013Config::default(), false);

        // This table is already properly aligned with left alignment
        let content = "| Name  | Age |\n| ----- | --- |\n| Alice | 30  |\n| Bob   | 25  |";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);

        let fixed = rule.fix(&ctx).unwrap();
        let lines: Vec<&str> = fixed.lines().collect();

        // Should be reformatted with right alignment
        assert!(
            lines[2].contains("| Alice |") && lines[2].contains("|  30 |"),
            "Already aligned table should be reformatted with right alignment. Got: {}",
            lines[2]
        );
        assert!(
            lines[3].contains("|   Bob |") || lines[3].contains("|  Bob |"),
            "Bob should be right-aligned. Got: {}",
            lines[3]
        );
    }

    #[test]
    fn test_md060_column_align_with_cjk_characters() {
        // CJK characters have double display width - centering should account for this
        let config = MD060Config {
            enabled: true,
            style: "aligned".to_string(),
            max_width: LineLength::from_const(0),
            column_align: ColumnAlign::Center,
            column_align_header: None,
            column_align_body: None,
            loose_last_column: false,
        };
        let rule = MD060TableFormat::from_config_struct(config, MD013Config::default(), false);

        let content = "| Name | City |\n|---|---|\n| Alice | 東京 |\n| Bob | LA |";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);

        let fixed = rule.fix(&ctx).unwrap();

        // Both Alice and Bob should be centered, and 東京 should be properly aligned
        // considering its double-width display
        assert!(fixed.contains("Bob"), "Table should contain Bob");
        assert!(fixed.contains("東京"), "Table should contain 東京");
    }

    #[test]
    fn test_md060_column_align_ignored_for_compact_style() {
        // column-align should have no effect on compact style (minimal padding)
        let config = MD060Config {
            enabled: true,
            style: "compact".to_string(),
            max_width: LineLength::from_const(0),
            column_align: ColumnAlign::Right, // This should be ignored
            column_align_header: None,
            column_align_body: None,
            loose_last_column: false,
        };
        let rule = MD060TableFormat::from_config_struct(config, MD013Config::default(), false);

        let content = "| Name | Age |\n|---|---|\n| Alice | 30 |\n| Bob | 25 |";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);

        let fixed = rule.fix(&ctx).unwrap();

        // Compact style: single space padding, no alignment
        assert!(
            fixed.contains("| Alice |"),
            "Compact style should have single space padding, not alignment. Got: {fixed}"
        );
    }

    #[test]
    fn test_md060_column_align_ignored_for_tight_style() {
        // column-align should have no effect on tight style (no padding)
        let config = MD060Config {
            enabled: true,
            style: "tight".to_string(),
            max_width: LineLength::from_const(0),
            column_align: ColumnAlign::Center, // This should be ignored
            column_align_header: None,
            column_align_body: None,
            loose_last_column: false,
        };
        let rule = MD060TableFormat::from_config_struct(config, MD013Config::default(), false);

        let content = "| Name | Age |\n|---|---|\n| Alice | 30 |\n| Bob | 25 |";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);

        let fixed = rule.fix(&ctx).unwrap();

        // Tight style: no spaces at all
        assert!(
            fixed.contains("|Alice|"),
            "Tight style should have no spaces. Got: {fixed}"
        );
    }

    #[test]
    fn test_md060_column_align_with_empty_cells() {
        // Empty cells should be handled correctly with centering
        let config = MD060Config {
            enabled: true,
            style: "aligned".to_string(),
            max_width: LineLength::from_const(0),
            column_align: ColumnAlign::Center,
            column_align_header: None,
            column_align_body: None,
            loose_last_column: false,
        };
        let rule = MD060TableFormat::from_config_struct(config, MD013Config::default(), false);

        let content = "| Name | Age |\n|---|---|\n| Alice | 30 |\n|  | 25 |";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);

        let fixed = rule.fix(&ctx).unwrap();
        let lines: Vec<&str> = fixed.lines().collect();

        // Empty cell should have all padding (centered empty string)
        assert!(
            lines[3].contains("|       |") || lines[3].contains("|      |"),
            "Empty cell should be padded correctly. Got: {}",
            lines[3]
        );
    }

    #[test]
    fn test_md060_column_align_auto_preserves_already_aligned() {
        // With column-align=auto (default), already aligned tables should be preserved
        let config = MD060Config {
            enabled: true,
            style: "aligned".to_string(),
            max_width: LineLength::from_const(0),
            column_align: ColumnAlign::Auto,
            column_align_header: None,
            column_align_body: None,
            loose_last_column: false,
        };
        let rule = MD060TableFormat::from_config_struct(config, MD013Config::default(), false);

        // This table is already properly aligned
        let content = "| Name  | Age |\n| ----- | --- |\n| Alice | 30  |\n| Bob   | 25  |";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);

        let fixed = rule.fix(&ctx).unwrap();

        // Should be preserved as-is
        assert_eq!(
            fixed, content,
            "Already aligned table should be preserved with column-align=auto"
        );
    }
}
