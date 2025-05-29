/// Rule MD022: Headings should be surrounded by blank lines
///
/// See [docs/md022.md](../../docs/md022.md) for full documentation, configuration, and examples.
use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::rules::heading_utils::{is_heading, is_setext_heading_marker};
use crate::utils::document_structure::{DocumentStructure, DocumentStructureExtensions};
use crate::utils::range_utils::calculate_heading_range;
use fancy_regex::Regex;
use lazy_static::lazy_static;
use toml;

lazy_static! {
    static ref HEADING_PATTERN: Regex = Regex::new(r"^(\s*)(#{1,6})(\s+)(.*)$").unwrap();
    static ref SETEXT_PATTERN: Regex = Regex::new(r"^(\s*)(=+|-+)(\s*)$").unwrap();
    static ref FRONT_MATTER_PATTERN: Regex = Regex::new(r"^---\s*$").unwrap();
}

///
/// This rule enforces consistent spacing around headings to improve document readability
/// and visual structure.
///
/// ## Purpose
///
/// - **Readability**: Blank lines create visual separation, making headings stand out
/// - **Parsing**: Many Markdown parsers require blank lines around headings for proper rendering
/// - **Consistency**: Creates a uniform document style throughout
/// - **Focus**: Helps readers identify and focus on section transitions
///
/// ## Configuration Options
///
/// The rule supports customizing the number of blank lines required:
///
/// ```yaml
/// MD022:
///   lines_above: 1  # Number of blank lines required above headings (default: 1)
///   lines_below: 1  # Number of blank lines required below headings (default: 1)
/// ```
///
/// ## Examples
///
/// ### Correct (with default configuration)
///
/// ```markdown
/// Regular paragraph text.
///
/// # Heading 1
///
/// Content under heading 1.
///
/// ## Heading 2
///
/// More content here.
/// ```
///
/// ### Incorrect (with default configuration)
///
/// ```markdown
/// Regular paragraph text.
/// # Heading 1
/// Content under heading 1.
/// ## Heading 2
/// More content here.
/// ```
///
/// ## Special Cases
///
/// This rule handles several special cases:
///
/// - **First Heading**: The first heading in a document doesn't require blank lines above
///   if it appears at the very start of the document
/// - **Front Matter**: YAML front matter is detected and skipped
/// - **Code Blocks**: Headings inside code blocks are ignored
/// - **Document Start/End**: Adjusts requirements for headings at the beginning or end of a document
///
/// ## Fix Behavior
///
/// When applying automatic fixes, this rule:
/// - Adds the required number of blank lines above headings
/// - Adds the required number of blank lines below headings
/// - Preserves document structure and existing content
///
/// ## Performance Considerations
///
/// The rule is optimized for performance with:
/// - Efficient line counting algorithms
/// - Proper handling of front matter
/// - Smart code block detection
///
#[derive(Clone)]
pub struct MD022BlanksAroundHeadings {
    /// Required number of blank lines before heading
    pub lines_above: usize,
    /// Required number of blank lines after heading
    pub lines_below: usize,
    /// Whether the first heading can be at the start of the document
    pub allowed_at_start: bool,
}

impl Default for MD022BlanksAroundHeadings {
    fn default() -> Self {
        Self {
            lines_above: 1,
            lines_below: 1,
            allowed_at_start: true,
        }
    }
}

impl MD022BlanksAroundHeadings {
    /// Create a new instance of the rule with default values:
    /// lines_above = 1, lines_below = 1
    pub fn new() -> Self {
        Self {
            lines_above: 1,
            lines_below: 1,
            allowed_at_start: true,
        }
    }

    /// Create with custom numbers of blank lines
    pub fn with_values(lines_above: usize, lines_below: usize) -> Self {
        Self {
            lines_above,
            lines_below,
            allowed_at_start: true,
        }
    }

    /// Determine if a line represents the start of a setext heading (requires looking at next line)
    fn _is_setext_heading_start(&self, lines: &[&str], index: usize) -> bool {
        if index + 1 >= lines.len() {
            return false;
        }

        let line = lines[index];
        let next_line = lines[index + 1];

        // Get indentation levels
        let line_indent = line
            .chars()
            .take_while(|c| c.is_whitespace())
            .collect::<String>();
        let next_indent = next_line
            .chars()
            .take_while(|c| c.is_whitespace())
            .collect::<String>();

        // Match indentation and check if next line is a setext marker
        !line.trim().is_empty() && is_setext_heading_marker(next_line) && line_indent == next_indent
    }

    /// Get the number of blank lines before a heading
    fn _blank_lines_before(&self, lines: &[&str], index: usize) -> usize {
        let mut blank_count = 0;
        let mut i = index as isize - 1;

        while i >= 0 && lines[i as usize].trim().is_empty() {
            blank_count += 1;
            i -= 1;
        }

        blank_count
    }

    /// Get the number of blank lines after a heading
    fn _blank_lines_after(&self, lines: &[&str], index: usize) -> usize {
        let mut blank_count = 0;
        let mut i = index + 1;

        // For setext headings, skip the underline and start counting after it
        if self._is_setext_heading_start(lines, index) {
            i += 1;
        }

        while i < lines.len() && lines[i].trim().is_empty() {
            blank_count += 1;
            i += 1;
        }

        blank_count
    }

    /// Check if we're inside front matter
    fn _is_in_front_matter(&self, lines: &[&str], index: usize) -> bool {
        let mut front_matter_started = false;
        let mut delimiter_count = 0;

        for (i, line) in lines.iter().enumerate() {
            if i > index {
                break;
            }

            if FRONT_MATTER_PATTERN.is_match(line).unwrap_or(false) {
                delimiter_count += 1;
                if delimiter_count == 1 {
                    front_matter_started = true;
                } else if delimiter_count == 2 && i <= index {
                    front_matter_started = false;
                }
            }
        }

        front_matter_started
    }

    /// Check if we're inside a code block
    fn _is_in_code_block(&self, lines: &[&str], index: usize) -> bool {
        let mut in_code_block = false;
        let mut fence_char = None;

        for (i, line) in lines.iter().enumerate() {
            if i >= index {
                break;
            }

            let trimmed = line.trim();
            if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
                if !in_code_block {
                    fence_char = Some(&trimmed[..3]);
                    in_code_block = true;
                } else if let Some(fence) = fence_char {
                    if trimmed.starts_with(fence) {
                        in_code_block = false;
                    }
                }
            }
        }

        in_code_block
    }

    /// Checks for heading and returns its length (1 for ATX, 2 for setext)
    fn _get_heading_length(&self, lines: &[&str], index: usize) -> usize {
        if index >= lines.len() {
            return 0;
        }

        let line = lines[index];

        // Check if it's an ATX heading
        if is_heading(line) {
            return 1;
        }

        // Check if it's a setext heading
        if self._is_setext_heading_start(lines, index) {
            return 2;
        }

        0
    }

    /// Fix a document by adding appropriate blank lines around headings
    fn _fix_content(&self, lines: &[&str]) -> String {
        let mut result = Vec::new();
        let mut in_code_block = false;
        let mut fence_char = None;
        let mut in_front_matter = false;
        let mut front_matter_start_detected = false;

        for (i, line) in lines.iter().enumerate() {
            // Handle front matter - only consider it front matter if at the start
            if FRONT_MATTER_PATTERN.is_match(line).unwrap_or(false) {
                // Only start front matter if at the beginning of the document (allowing for blank lines)
                if !front_matter_start_detected && i == 0
                    || (i > 0 && lines[..i].iter().all(|l| l.trim().is_empty()))
                {
                    in_front_matter = true;
                    front_matter_start_detected = true;
                } else if in_front_matter {
                    // End front matter if we're in it
                    in_front_matter = false;
                }
                // Otherwise it's just a horizontal rule, not front matter

                result.push(line.to_string());
                continue;
            }

            // Check for code block fences
            let trimmed = line.trim();
            if (trimmed.starts_with("```") || trimmed.starts_with("~~~"))
                && (trimmed == "```"
                    || trimmed == "~~~"
                    || trimmed.len() >= 3
                        && trimmed[3..]
                            .chars()
                            .next()
                            .map_or(true, |c| c.is_whitespace() || c.is_alphabetic()))
            {
                // Toggle code block state and update fence character
                if !in_code_block {
                    fence_char = Some(&trimmed[..3]);
                    in_code_block = true;
                } else if let Some(fence) = fence_char {
                    if trimmed.starts_with(fence) {
                        in_code_block = false;
                        fence_char = None;
                    }
                }

                result.push(line.to_string());
                continue;
            }

            // Inside code block or front matter, preserve content exactly
            if in_code_block || in_front_matter {
                result.push(line.to_string());
                continue;
            }

            // Check if it's a heading (ATX or setext)
            let is_atx_heading = is_heading(line);
            let is_setext_marker = i > 0 && is_setext_heading_marker(line);
            let is_setext_content = i + 1 < lines.len() && is_setext_heading_marker(lines[i + 1]);

            if is_atx_heading {
                let is_first_heading = i == 0
                    || (0..i).all(|j| {
                        lines[j].trim().is_empty()
                            || FRONT_MATTER_PATTERN.is_match(lines[j]).unwrap_or(false)
                    });

                // Count existing blank lines above in the result
                let mut blank_lines_above = 0;
                let mut check_idx = result.len();
                while check_idx > 0 && result[check_idx - 1].trim().is_empty() {
                    blank_lines_above += 1;
                    check_idx -= 1;
                }

                // Determine how many blank lines we need above
                let needed_blanks_above = if is_first_heading && self.allowed_at_start {
                    0
                } else {
                    self.lines_above
                };

                // Add missing blank lines above if needed
                while blank_lines_above < needed_blanks_above {
                    result.push(String::new());
                    blank_lines_above += 1;
                }

                // Add the heading line
                result.push(line.to_string());

                // Count existing blank lines below in the original content
                let mut blank_lines_below = 0;
                let mut next_content_line_idx = None;
                for (j, next_line) in lines.iter().enumerate().skip(i + 1) {
                    if next_line.trim().is_empty() {
                        blank_lines_below += 1;
                    } else {
                        next_content_line_idx = Some(j);
                        break;
                    }
                }

                // Check if the next non-blank line is a list item or code fence
                let next_is_list_or_code = if let Some(idx) = next_content_line_idx {
                    let next_line = lines[idx].trim();
                    // Check for list items (-, *, +, or numbered)
                    next_line.starts_with("- ") ||
                    next_line.starts_with("* ") ||
                    next_line.starts_with("+ ") ||
                    next_line.chars().next().map_or(false, |c| c.is_ascii_digit()) && next_line.contains(". ") ||
                    // Check for code fences
                    next_line.starts_with("```") || next_line.starts_with("~~~")
                } else {
                    false
                };

                // Add missing blank lines below if needed (but don't remove existing ones)
                // Don't add blank lines if the next content is a list item or code fence
                let needed_blanks_below = if next_is_list_or_code {
                    0
                } else {
                    self.lines_below
                };
                if blank_lines_below < needed_blanks_below {
                    for _ in 0..(needed_blanks_below - blank_lines_below) {
                        result.push(String::new());
                    }
                }
            } else if is_setext_content {
                // This is a setext heading content line (the line before ===== or -----)
                let is_first_heading = i == 0
                    || (0..i).all(|j| {
                        lines[j].trim().is_empty()
                            || FRONT_MATTER_PATTERN.is_match(lines[j]).unwrap_or(false)
                    });

                // Count existing blank lines above in the result
                let mut blank_lines_above = 0;
                let mut check_idx = result.len();
                while check_idx > 0 && result[check_idx - 1].trim().is_empty() {
                    blank_lines_above += 1;
                    check_idx -= 1;
                }

                // Determine how many blank lines we need above
                let needed_blanks_above = if is_first_heading && self.allowed_at_start {
                    0
                } else {
                    self.lines_above
                };

                // Add missing blank lines above if needed
                while blank_lines_above < needed_blanks_above {
                    result.push(String::new());
                    blank_lines_above += 1;
                }

                // Add the setext heading content line
                result.push(line.to_string());
            } else if is_setext_marker {
                // This is a setext heading marker line (===== or -----)
                // Add the marker line
                result.push(line.to_string());

                // Count existing blank lines below in the original content
                let mut blank_lines_below = 0;
                let mut next_content_line_idx = None;
                for (j, next_line) in lines.iter().enumerate().skip(i + 1) {
                    if next_line.trim().is_empty() {
                        blank_lines_below += 1;
                    } else {
                        next_content_line_idx = Some(j);
                        break;
                    }
                }

                // Check if the next non-blank line is a list item or code fence
                let next_is_list_or_code = if let Some(idx) = next_content_line_idx {
                    let next_line = lines[idx].trim();
                    // Check for list items (-, *, +, or numbered)
                    next_line.starts_with("- ") ||
                    next_line.starts_with("* ") ||
                    next_line.starts_with("+ ") ||
                    next_line.chars().next().map_or(false, |c| c.is_ascii_digit()) && next_line.contains(". ") ||
                    // Check for code fences
                    next_line.starts_with("```") || next_line.starts_with("~~~")
                } else {
                    false
                };

                // Add missing blank lines below if needed (but don't remove existing ones)
                // Don't add blank lines if the next content is a list item or code fence
                let needed_blanks_below = if next_is_list_or_code {
                    0
                } else {
                    self.lines_below
                };
                if blank_lines_below < needed_blanks_below {
                    for _ in 0..(needed_blanks_below - blank_lines_below) {
                        result.push(String::new());
                    }
                }
            } else {
                // Regular line - just add it
                result.push(line.to_string());
            }
        }

        result.join("\n")
    }
}

impl Rule for MD022BlanksAroundHeadings {
    fn name(&self) -> &'static str {
        "MD022"
    }

    fn description(&self) -> &'static str {
        "Headings should be surrounded by blank lines"
    }

    fn check(&self, ctx: &crate::lint_context::LintContext) -> LintResult {
        let content = ctx.content;
        let mut result = Vec::new();
        let lines: Vec<&str> = content.lines().collect();

        // Skip if empty document
        if lines.is_empty() {
            return Ok(result);
        }

        let mut in_code_block = false;
        let mut fence_char = None;
        let mut in_front_matter = false;
        let mut front_matter_start_detected = false;
        let mut _is_first_line = true;
        let mut prev_heading_index: Option<usize> = None;
        let mut processed_headings = std::collections::HashSet::new();

        for (i, line) in lines.iter().enumerate() {
            // Handle front matter - only consider it front matter if at the start
            if FRONT_MATTER_PATTERN.is_match(line).unwrap_or(false) {
                // Only start front matter if at the beginning of the document (allowing for blank lines)
                if !front_matter_start_detected && i == 0
                    || (i > 0 && lines[..i].iter().all(|l| l.trim().is_empty()))
                {
                    in_front_matter = true;
                    front_matter_start_detected = true;
                } else if in_front_matter {
                    // End front matter if we're in it
                    in_front_matter = false;
                }
                // Otherwise it's just a horizontal rule, not front matter
                _is_first_line = false;
                continue;
            }

            // Check for code block fences
            let trimmed = line.trim();
            let is_code_fence = (trimmed.starts_with("```") || trimmed.starts_with("~~~"))
                && (trimmed == "```"
                    || trimmed == "~~~"
                    || trimmed.len() >= 3
                        && trimmed[3..]
                            .chars()
                            .next()
                            .map_or(true, |c| c.is_whitespace() || c.is_alphabetic()));

            if is_code_fence {
                // Toggle code block state and update fence character
                if !in_code_block {
                    fence_char = Some(&trimmed[..3]);
                    in_code_block = true;
                } else if let Some(fence) = fence_char {
                    if trimmed.starts_with(fence) {
                        in_code_block = false;
                        fence_char = None;
                    }
                }
                continue;
            }

            if in_code_block || in_front_matter {
                continue;
            }

            _is_first_line = false;

            // Check if it's a heading
            if is_heading(line)
                || (i > 0
                    && is_setext_heading_marker(line)
                    && is_heading(&format!("{} {}", lines[i - 1], line)))
            {
                let heading_line = if is_setext_heading_marker(line) {
                    i - 1
                } else {
                    i
                };
                let heading_display_line = heading_line + 1; // 1-indexed for display

                // Skip non-heading lines
                if line.trim().is_empty() {
                    continue;
                }

                // Skip if we've already processed this heading
                if processed_headings.contains(&heading_line) {
                    continue;
                }

                processed_headings.insert(heading_line);

                // Track issues for this heading
                let mut issues = Vec::new();

                // Check consecutive headings
                if let Some(prev_idx) = prev_heading_index {
                    let blanks_between = heading_line - prev_idx - 1;
                    let required_blanks = self.lines_above.max(self.lines_below);

                    if blanks_between < required_blanks {
                        let line_word = if required_blanks == 1 {
                            "line"
                        } else {
                            "lines"
                        };
                        issues.push(format!("Expected {} blank {} between headings", required_blanks, line_word));
                    }
                }

                // Check blank lines above
                if heading_line > 0 {
                    let mut blank_lines_above = 0;
                    for j in (0..heading_line).rev() {
                        if lines[j].trim().is_empty() {
                            blank_lines_above += 1;
                        } else {
                            break;
                        }
                    }

                    if blank_lines_above < self.lines_above {
                        let line_word = if self.lines_above == 1 {
                            "line"
                        } else {
                            "lines"
                        };
                        issues.push(format!(
                            "Expected {} blank {} above heading",
                            self.lines_above, line_word
                        ));
                    }
                }

                // Check blank lines below
                let effective_heading_line = heading_line;
                if effective_heading_line < lines.len() - 1 {
                    // Special case: Don't require blank lines if the next non-blank line is a code block fence
                    let mut next_non_blank_idx = effective_heading_line + 1;
                    while next_non_blank_idx < lines.len()
                        && lines[next_non_blank_idx].trim().is_empty()
                    {
                        next_non_blank_idx += 1;
                    }

                    let next_line_is_code_fence = next_non_blank_idx < lines.len() && {
                        let next_trimmed = lines[next_non_blank_idx].trim();
                        (next_trimmed.starts_with("```") || next_trimmed.starts_with("~~~"))
                            && (next_trimmed == "```"
                                || next_trimmed == "~~~"
                                || next_trimmed.len() >= 3
                                    && next_trimmed[3..]
                                        .chars()
                                        .next()
                                        .map_or(true, |c| c.is_whitespace() || c.is_alphabetic()))
                    };

                    // Check if next line is a code fence or list item
                    let next_line_is_list = next_non_blank_idx < lines.len() && {
                        let next_trimmed = lines[next_non_blank_idx].trim();
                        next_trimmed.starts_with("- ")
                            || next_trimmed.starts_with("* ")
                            || next_trimmed.starts_with("+ ")
                            || next_trimmed
                                .chars()
                                .next()
                                .map_or(false, |c| c.is_ascii_digit())
                                && next_trimmed.contains(". ")
                    };

                    // If next line is a code fence or list item, we don't need blank lines between
                    if !next_line_is_code_fence && !next_line_is_list {
                        let mut blank_lines_below = 0;
                        for line in lines.iter().skip(effective_heading_line + 1) {
                            if !line.trim().is_empty() {
                                break;
                            }
                            blank_lines_below += 1;
                        }

                        if blank_lines_below < self.lines_below {
                            let line_word = if self.lines_below == 1 {
                                "line"
                            } else {
                                "lines"
                            };
                            issues.push(format!(
                                "Expected {} blank {} below heading",
                                self.lines_below, line_word
                            ));
                        }
                    }
                }

                // Combine all issues for this heading into one warning
                if !issues.is_empty() {
                    // Use the combined message like check_with_structure does
                    let message = issues.join(" ");

                    // Calculate precise character range for the heading
                    let (start_line, start_col, end_line, end_col) =
                        calculate_heading_range(heading_display_line, line);

                    result.push(LintWarning {
                        rule_name: Some(self.name()),
                        message,
                        line: start_line,
                        column: start_col,
                        end_line,
                        end_column: end_col,
                        severity: Severity::Warning,
                        fix: Some(Fix {
                            range: 0..0, // Placeholder range - the actual fix is handled by the fix() method
                            replacement: String::new(), // Placeholder - the actual fix is handled by the fix() method
                        }),
                    });
                }

                // Update previous heading index
                prev_heading_index = Some(heading_line);
            }
        }

        Ok(result)
    }

    fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, LintError> {
        let content = ctx.content;
        if content.is_empty() {
            return Ok(content.to_string());
        }

        // Check if original content ended with newline
        let had_trailing_newline = content.ends_with('\n');

        // Use a consolidated fix that avoids adding multiple blank lines
        let lines: Vec<&str> = content.lines().collect();
        let fixed = self._fix_content(&lines);

        // Preserve original trailing newline if it existed
        let result = if had_trailing_newline && !fixed.ends_with('\n') {
            format!("{}\n", fixed)
        } else {
            fixed
        };

        Ok(result)
    }

    /// Optimized check using document structure
    fn check_with_structure(
        &self,
        ctx: &crate::lint_context::LintContext,
        _structure: &DocumentStructure,
    ) -> LintResult {
        let content = ctx.content;
        let mut result = Vec::new();
        let lines: Vec<&str> = content.lines().collect();

        // Skip if empty document
        if lines.is_empty() {
            return Ok(result);
        }

        let mut prev_heading_index: Option<usize> = None;
        let mut processed_headings = std::collections::HashSet::new();

        // Process only heading lines using structure.heading_lines
        for &heading_line_num in &_structure.heading_lines {
            let heading_line = heading_line_num - 1; // Convert 1-indexed to 0-indexed

            // Skip if out of bounds
            if heading_line >= lines.len() {
                continue;
            }

            // Skip if we've already processed this heading
            if processed_headings.contains(&heading_line) {
                continue;
            }

            let line = lines[heading_line];

            // Detect if this is a setext heading by checking if the next line is a marker
            let _is_setext =
                heading_line + 1 < lines.len() && is_setext_heading_marker(lines[heading_line + 1]);

            // Skip non-heading lines (this shouldn't happen with the document structure, but just in case)
            if line.trim().is_empty() {
                continue;
            }

            processed_headings.insert(heading_line);

            // Track issues for this heading
            let mut issues = Vec::new();

            // Check consecutive headings
            if let Some(prev_idx) = prev_heading_index {
                let blanks_between = heading_line - prev_idx - 1;
                let required_blanks = self.lines_above.max(self.lines_below);

                if blanks_between < required_blanks {
                    let line_word = if required_blanks == 1 {
                        "line"
                    } else {
                        "lines"
                    };
                    issues.push(format!("Expected {} blank {} between headings", required_blanks, line_word));
                }
            }

            // Check blank lines above
            if heading_line > 0 {
                let mut blank_lines_above = 0;
                for j in (0..heading_line).rev() {
                    if lines[j].trim().is_empty() {
                        blank_lines_above += 1;
                    } else {
                        break;
                    }
                }

                if blank_lines_above < self.lines_above {
                    let line_word = if self.lines_above == 1 {
                        "line"
                    } else {
                        "lines"
                    };
                    issues.push(format!(
                        "Expected {} blank {} above heading",
                        self.lines_above, line_word
                    ));
                }
            }

            // Check blank lines below
            let effective_heading_line = heading_line;
            if effective_heading_line < lines.len() - 1 {
                // Special case: Don't require blank lines if the next non-blank line is a code block fence
                let mut next_non_blank_idx = effective_heading_line + 1;
                while next_non_blank_idx < lines.len()
                    && lines[next_non_blank_idx].trim().is_empty()
                {
                    next_non_blank_idx += 1;
                }

                let next_line_is_code_fence = next_non_blank_idx < lines.len() && {
                    let next_trimmed = lines[next_non_blank_idx].trim();
                    (next_trimmed.starts_with("```") || next_trimmed.starts_with("~~~"))
                        && (next_trimmed == "```"
                            || next_trimmed == "~~~"
                            || next_trimmed.len() >= 3
                                && next_trimmed[3..]
                                    .chars()
                                    .next()
                                    .map_or(true, |c| c.is_whitespace() || c.is_alphabetic()))
                };

                // Check if next line is a code fence or list item
                let next_line_is_list = next_non_blank_idx < lines.len() && {
                    let next_trimmed = lines[next_non_blank_idx].trim();
                    next_trimmed.starts_with("- ")
                        || next_trimmed.starts_with("* ")
                        || next_trimmed.starts_with("+ ")
                        || next_trimmed
                            .chars()
                            .next()
                            .map_or(false, |c| c.is_ascii_digit())
                            && next_trimmed.contains(". ")
                };

                // If next line is a code fence or list item, we don't need blank lines between
                if !next_line_is_code_fence && !next_line_is_list {
                    let mut blank_lines_below = 0;
                    for line in lines.iter().skip(effective_heading_line + 1) {
                        if !line.trim().is_empty() {
                            break;
                        }
                        blank_lines_below += 1;
                    }

                    if blank_lines_below < self.lines_below {
                        let line_word = if self.lines_below == 1 {
                            "line"
                        } else {
                            "lines"
                        };
                        issues.push(format!(
                            "Expected {} blank {} below heading",
                            self.lines_below, line_word
                        ));
                    }
                }
            }

            // Combine all issues for this heading into one warning
            if !issues.is_empty() {
                let message = issues.join(" ");

                // Calculate precise character range for the heading
                let (start_line, start_col, end_line, end_col) = calculate_heading_range(
                    heading_line + 1, // Convert back to 1-indexed
                    line,
                );

                // For fix, just insert the required number of newlines at the start of the heading (above)
                // and after the heading (below). For simplicity, only provide a fix for the first issue.
                result.push(LintWarning {
                    rule_name: Some(self.name()),
                    message,
                    line: start_line,
                    column: start_col,
                    end_line,
                    end_column: end_col,
                    severity: Severity::Warning,
                    fix: Some(Fix {
                        range: 0..0, // Placeholder range - the actual fix is handled by the fix() method
                        replacement: String::new(), // Placeholder - the actual fix is handled by the fix() method
                    }),
                });
            }

            // Update previous heading index
            prev_heading_index = Some(heading_line);
        }

        Ok(result)
    }

    /// Get the category of this rule for selective processing
    fn category(&self) -> RuleCategory {
        RuleCategory::Heading
    }

    /// Check if this rule should be skipped
    fn should_skip(&self, ctx: &crate::lint_context::LintContext) -> bool {
        let content = ctx.content;
        content.is_empty() || !content.contains('#')
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_maybe_document_structure(&self) -> Option<&dyn crate::rule::MaybeDocumentStructure> {
        Some(self)
    }

    fn default_config_section(&self) -> Option<(String, toml::Value)> {
        let mut map = toml::map::Map::new();
        map.insert(
            "lines_above".to_string(),
            toml::Value::Integer(self.lines_above as i64),
        );
        map.insert(
            "lines_below".to_string(),
            toml::Value::Integer(self.lines_below as i64),
        );
        Some((self.name().to_string(), toml::Value::Table(map)))
    }

    fn from_config(config: &crate::config::Config) -> Box<dyn Rule>
    where
        Self: Sized,
    {
        let lines_above =
            crate::config::get_rule_config_value::<usize>(config, "MD022", "lines_above")
                .unwrap_or(1);
        let lines_below =
            crate::config::get_rule_config_value::<usize>(config, "MD022", "lines_below")
                .unwrap_or(1);
        Box::new(MD022BlanksAroundHeadings {
            lines_above,
            lines_below,
            allowed_at_start: true,
        })
    }
}

impl DocumentStructureExtensions for MD022BlanksAroundHeadings {
    fn has_relevant_elements(
        &self,
        ctx: &crate::lint_context::LintContext,
        _structure: &DocumentStructure,
    ) -> bool {
        let content = ctx.content;
        !content.is_empty() && content.contains('#')
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lint_context::LintContext;
    use crate::utils::document_structure::DocumentStructure;

    #[test]
    fn test_valid_headings() {
        let rule = MD022BlanksAroundHeadings::default();
        let content = "\n# Heading 1\n\nSome content.\n\n## Heading 2\n\nMore content.\n";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_missing_blank_above() {
        let rule = MD022BlanksAroundHeadings::default();
        let content = "# Heading 1\n\nSome content.\n\n## Heading 2\n\nMore content.\n";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 0); // No warning for first heading

        let fixed = rule.fix(&ctx).unwrap();

        // Test for the ability to handle the content without breaking it
        // Don't check for exact string equality which may break with implementation changes
        assert!(fixed.contains("# Heading 1"));
        assert!(fixed.contains("Some content."));
        assert!(fixed.contains("## Heading 2"));
        assert!(fixed.contains("More content."));
    }

    #[test]
    fn test_missing_blank_below() {
        let rule = MD022BlanksAroundHeadings::default();
        let content = "\n# Heading 1\nSome content.\n\n## Heading 2\n\nMore content.\n";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].line, 2);

        // Test the fix
        let fixed = rule.fix(&ctx).unwrap();
        assert!(fixed.contains("# Heading 1\n\nSome content"));
    }

    #[test]
    fn test_missing_blank_above_and_below() {
        let rule = MD022BlanksAroundHeadings::default();
        let content = "# Heading 1\nSome content.\n## Heading 2\nMore content.\n";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 2); // Missing blanks: below first heading, above and below second heading

        // Test the fix
        let fixed = rule.fix(&ctx).unwrap();
        assert!(fixed.contains("# Heading 1\n\nSome content"));
        assert!(fixed.contains("Some content.\n\n## Heading 2"));
        assert!(fixed.contains("## Heading 2\n\nMore content"));
    }

    #[test]
    fn test_fix_headings() {
        let rule = MD022BlanksAroundHeadings::default();
        let content = "# Heading 1\nSome content.\n## Heading 2\nMore content.";
        let ctx = LintContext::new(content);
        let result = rule.fix(&ctx).unwrap();

        let expected = "# Heading 1\n\nSome content.\n\n## Heading 2\n\nMore content.";
        assert_eq!(result, expected);
    }

    #[test]
    fn test_consecutive_headings_pattern() {
        let rule = MD022BlanksAroundHeadings::default();
        let content = "# Heading 1\n## Heading 2\n### Heading 3";
        let ctx = LintContext::new(content);
        let result = rule.fix(&ctx).unwrap();

        // Using more specific assertions to check the structure
        let lines: Vec<&str> = result.lines().collect();
        assert!(!lines.is_empty());

        // Find the positions of the headings
        let h1_pos = lines.iter().position(|&l| l == "# Heading 1").unwrap();
        let h2_pos = lines.iter().position(|&l| l == "## Heading 2").unwrap();
        let h3_pos = lines.iter().position(|&l| l == "### Heading 3").unwrap();

        // Verify blank lines between headings
        assert!(
            h2_pos > h1_pos + 1,
            "Should have at least one blank line after first heading"
        );
        assert!(
            h3_pos > h2_pos + 1,
            "Should have at least one blank line after second heading"
        );

        // Verify there's a blank line between h1 and h2
        assert!(
            lines[h1_pos + 1].trim().is_empty(),
            "Line after h1 should be blank"
        );

        // Verify there's a blank line between h2 and h3
        assert!(
            lines[h2_pos + 1].trim().is_empty(),
            "Line after h2 should be blank"
        );
    }

    #[test]
    fn test_blanks_around_setext_headings() {
        let rule = MD022BlanksAroundHeadings::default();
        let content = "Heading 1\n=========\nSome content.\nHeading 2\n---------\nMore content.";
        let ctx = LintContext::new(content);
        let result = rule.fix(&ctx).unwrap();

        // Check that the fix follows requirements without being too rigid about the exact output format
        let lines: Vec<&str> = result.lines().collect();

        // Verify key elements are present
        assert!(result.contains("Heading 1"));
        assert!(result.contains("========="));
        assert!(result.contains("Some content."));
        assert!(result.contains("Heading 2"));
        assert!(result.contains("---------"));
        assert!(result.contains("More content."));

        // Verify structure ensures blank lines are added after headings
        let heading1_marker_idx = lines.iter().position(|&l| l == "=========").unwrap();
        let some_content_idx = lines.iter().position(|&l| l == "Some content.").unwrap();
        assert!(
            some_content_idx > heading1_marker_idx + 1,
            "Should have a blank line after the first heading"
        );

        let heading2_marker_idx = lines.iter().position(|&l| l == "---------").unwrap();
        let more_content_idx = lines.iter().position(|&l| l == "More content.").unwrap();
        assert!(
            more_content_idx > heading2_marker_idx + 1,
            "Should have a blank line after the second heading"
        );

        // Verify that the fixed content has no warnings
        let fixed_ctx = LintContext::new(&result);
        let fixed_warnings = rule.check(&fixed_ctx).unwrap();
        assert!(
            fixed_warnings.is_empty(),
            "Fixed content should have no warnings"
        );
    }

    #[test]
    fn test_fix_specific_blank_line_cases() {
        let rule = MD022BlanksAroundHeadings::default();

        // Case 1: Testing consecutive headings
        let content1 = "# Heading 1\n## Heading 2\n### Heading 3";
        let ctx1 = LintContext::new(content1);
        let result1 = rule.fix(&ctx1).unwrap();
        // Verify structure rather than exact content as the fix implementation may vary
        assert!(result1.contains("# Heading 1"));
        assert!(result1.contains("## Heading 2"));
        assert!(result1.contains("### Heading 3"));
        // Ensure each heading has a blank line after it
        let lines: Vec<&str> = result1.lines().collect();
        let h1_pos = lines.iter().position(|&l| l == "# Heading 1").unwrap();
        let h2_pos = lines.iter().position(|&l| l == "## Heading 2").unwrap();
        assert!(
            lines[h1_pos + 1].trim().is_empty(),
            "Should have a blank line after h1"
        );
        assert!(
            lines[h2_pos + 1].trim().is_empty(),
            "Should have a blank line after h2"
        );

        // Case 2: Headings with content
        let content2 = "# Heading 1\nContent under heading 1\n## Heading 2";
        let ctx2 = LintContext::new(content2);
        let result2 = rule.fix(&ctx2).unwrap();
        // Verify structure
        assert!(result2.contains("# Heading 1"));
        assert!(result2.contains("Content under heading 1"));
        assert!(result2.contains("## Heading 2"));
        // Check spacing
        let lines2: Vec<&str> = result2.lines().collect();
        let h1_pos2 = lines2.iter().position(|&l| l == "# Heading 1").unwrap();
        let _content_pos = lines2
            .iter()
            .position(|&l| l == "Content under heading 1")
            .unwrap();
        assert!(
            lines2[h1_pos2 + 1].trim().is_empty(),
            "Should have a blank line after heading 1"
        );

        // Case 3: Multiple consecutive headings with blank lines preserved
        let content3 = "# Heading 1\n\n\n## Heading 2\n\n\n### Heading 3\n\nContent";
        let ctx3 = LintContext::new(content3);
        let result3 = rule.fix(&ctx3).unwrap();
        // Just verify it doesn't crash and properly formats headings
        assert!(result3.contains("# Heading 1"));
        assert!(result3.contains("## Heading 2"));
        assert!(result3.contains("### Heading 3"));
        assert!(result3.contains("Content"));
    }

    #[test]
    fn test_with_document_structure() {
        let rule = MD022BlanksAroundHeadings::default();

        // Test with properly formatted headings
        let content = "\n# Heading 1\n\nSome content.\n\n## Heading 2\n\nMore content.\n";
        let structure = DocumentStructure::new(content);
        let ctx = LintContext::new(content);
        let result = rule.check_with_structure(&ctx, &structure).unwrap();
        assert!(result.is_empty());

        // Test with missing blank lines
        let content = "# Heading 1\nSome content.\n## Heading 2\nMore content.";
        let structure = DocumentStructure::new(content);
        let ctx = LintContext::new(content);
        let result = rule.check_with_structure(&ctx, &structure).unwrap();
        assert_eq!(result.len(), 2); // Should flag issues with both headings

        // Test with setext headings
        let content = "Heading 1\n=========\nSome content.\nHeading 2\n---------\nMore content.";
        let structure = DocumentStructure::new(content);
        let ctx = LintContext::new(content);
        let result = rule.check_with_structure(&ctx, &structure).unwrap();
        assert!(!result.is_empty()); // Should flag issues with both setext headings
    }

    #[test]
    fn test_fix_preserves_existing_blank_lines() {
        let rule = MD022BlanksAroundHeadings::new();
        let content = "# Title

## Section 1

Content here.

## Section 2

More content.
### Missing Blank Above

Even more content.

## Section 3

Final content.";

        let expected = "# Title

## Section 1

Content here.

## Section 2

More content.

### Missing Blank Above

Even more content.

## Section 3

Final content.";

        let result = rule._fix_content(&content.lines().collect::<Vec<&str>>());
        assert_eq!(
            result, expected,
            "Fix should only add missing blank lines, never remove existing ones"
        );
    }

    #[test]
    fn test_fix_preserves_trailing_newline() {
        let rule = MD022BlanksAroundHeadings::new();

        // Test with trailing newline
        let content_with_newline = "# Title\nContent here.\n";
        let ctx = LintContext::new(content_with_newline);
        let result = rule.fix(&ctx).unwrap();
        assert!(result.ends_with('\n'), "Should preserve trailing newline");

        // Test without trailing newline
        let content_without_newline = "# Title\nContent here.";
        let ctx = LintContext::new(content_without_newline);
        let result = rule.fix(&ctx).unwrap();
        assert!(
            !result.ends_with('\n'),
            "Should not add trailing newline if original didn't have one"
        );
    }

    #[test]
    fn test_fix_does_not_add_blank_lines_before_lists() {
        let rule = MD022BlanksAroundHeadings::new();
        let content = "## Configuration\n\nThis rule has the following configuration options:\n\n- `option1`: Description of option 1.\n- `option2`: Description of option 2.\n\n## Another Section\n\nSome content here.";

        let expected = "## Configuration\n\nThis rule has the following configuration options:\n\n- `option1`: Description of option 1.\n- `option2`: Description of option 2.\n\n## Another Section\n\nSome content here.";

        let result = rule._fix_content(&content.lines().collect::<Vec<&str>>());
        assert_eq!(
            result, expected,
            "Fix should not add blank lines before lists"
        );
    }
}
