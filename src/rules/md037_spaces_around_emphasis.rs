use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, Severity};
use crate::utils::range_utils::LineIndex;
use lazy_static::lazy_static;
use regex::Regex;

lazy_static! {
    // Improved code block detection patterns
    static ref FENCED_CODE_BLOCK_START: Regex = Regex::new(r"^(\s*)```(?:[^`\r\n]*)$").unwrap();
    static ref FENCED_CODE_BLOCK_END: Regex = Regex::new(r"^(\s*)```\s*$").unwrap();
    static ref ALTERNATE_FENCED_CODE_BLOCK_START: Regex = Regex::new(r"^(\s*)~~~(?:[^~\r\n]*)$").unwrap();
    static ref ALTERNATE_FENCED_CODE_BLOCK_END: Regex = Regex::new(r"^(\s*)~~~\s*$").unwrap();
    static ref INDENTED_CODE_BLOCK: Regex = Regex::new(r"^(\s{4,})").unwrap();

    // Front matter detection
    static ref FRONT_MATTER_DELIM: Regex = Regex::new(r"^---\s*$").unwrap();

    // Enhanced emphasis patterns with better handling of edge cases
    static ref ASTERISK_EMPHASIS: Regex = Regex::new(r"\*\s+([^*\s][^*]*?)\s+\*|\*\s+([^*\s][^*]*?)\*|\*([^*\s][^*]*?)\s+\*").unwrap();
    static ref DOUBLE_ASTERISK_EMPHASIS: Regex = Regex::new(r"\*\*\s+([^*\s][^*]*?)\s+\*\*|\*\*\s+([^*\s][^*]*?)\*\*|\*\*([^*\s][^*]*?)\s+\*\*").unwrap();
    static ref UNDERSCORE_EMPHASIS: Regex = Regex::new(r"_\s+([^_\s][^_]*?)\s+_|_\s+([^_\s][^_]*?)_|_([^_\s][^_]*?)\s+_").unwrap();
    static ref DOUBLE_UNDERSCORE_EMPHASIS: Regex = Regex::new(r"__\s+([^_\s][^_]*?)\s+__|__\s+([^_\s][^_]*?)__|__([^_\s][^_]*?)\s+__").unwrap();

    // Detect potential unbalanced emphasis without using look-behind/ahead
    static ref UNBALANCED_ASTERISK: Regex = Regex::new(r"\*([^*]+)$|^([^*]*)\*").unwrap();
    static ref UNBALANCED_DOUBLE_ASTERISK: Regex = Regex::new(r"\*\*([^*]+)$|^([^*]*)\*\*").unwrap();
    static ref UNBALANCED_UNDERSCORE: Regex = Regex::new(r"_([^_]+)$|^([^_]*)_").unwrap();
    static ref UNBALANCED_DOUBLE_UNDERSCORE: Regex = Regex::new(r"__([^_]+)$|^([^_]*)__").unwrap();

    // Better detection of inline code with support for multiple backticks
    static ref INLINE_CODE: Regex = Regex::new(r"(`+)([^`]|[^`].*?[^`])(`+)").unwrap();

    // List markers pattern - used to avoid confusion with emphasis
    static ref LIST_MARKER: Regex = Regex::new(r"^\s*[*+-]\s+").unwrap();

    // Valid emphasis at start of line that should not be treated as lists
    static ref VALID_START_EMPHASIS: Regex = Regex::new(r"^(\*\*[^*\s]|\*[^*\s]|__[^_\s]|_[^_\s])").unwrap();

    // Documentation style patterns
    static ref DOC_METADATA_PATTERN: Regex = Regex::new(r"^\s*\*?\s*\*\*[^*]+\*\*\s*:").unwrap();

    // Bold text pattern (for preserving bold text in documentation)
    static ref BOLD_TEXT_PATTERN: Regex = Regex::new(r"\*\*[^*]+\*\*").unwrap();

    // Multi-line emphasis detection (for potential future use)
    static ref MULTI_LINE_EMPHASIS_START: Regex = Regex::new(r"(\*\*|\*|__|_)([^*_\s].*?)$").unwrap();
    static ref MULTI_LINE_EMPHASIS_END: Regex = Regex::new(r"^(.*?)(\*\*|\*|__|_)").unwrap();
}

/// Structure to track code block state
#[derive(Default)]
struct CodeBlockState {
    in_fenced_code: bool,
    in_alternate_fenced: bool,
    in_front_matter: bool,
}

impl CodeBlockState {
    fn new() -> Self {
        CodeBlockState {
            in_fenced_code: false,
            in_alternate_fenced: false,
            in_front_matter: false,
        }
    }

    fn is_in_code_block(&self, line: &str) -> bool {
        if self.in_fenced_code || self.in_alternate_fenced || self.in_front_matter {
            return true;
        }

        // Check if the line is an indented code block
        INDENTED_CODE_BLOCK.is_match(line)
    }

    fn update(&mut self, line: &str) {
        // Front matter handling
        if FRONT_MATTER_DELIM.is_match(line) {
            self.in_front_matter = !self.in_front_matter;
            return;
        }

        // Skip updating code block state if in front matter
        if self.in_front_matter {
            return;
        }

        // Fenced code block handling
        if FENCED_CODE_BLOCK_START.is_match(line) {
            self.in_fenced_code = true;
        } else if FENCED_CODE_BLOCK_END.is_match(line) && self.in_fenced_code {
            self.in_fenced_code = false;
        }

        // Alternate fenced code block handling
        if ALTERNATE_FENCED_CODE_BLOCK_START.is_match(line) {
            self.in_alternate_fenced = true;
        } else if ALTERNATE_FENCED_CODE_BLOCK_END.is_match(line) && self.in_alternate_fenced {
            self.in_alternate_fenced = false;
        }
    }
}

// Enhanced inline code replacement to handle nested backticks
fn replace_inline_code(line: &str) -> String {
    let mut result = line.to_string();
    let mut offset = 0;

    for cap in INLINE_CODE.captures_iter(line) {
        if let (Some(full_match), Some(_opening), Some(_content), Some(_closing)) =
            (cap.get(0), cap.get(1), cap.get(2), cap.get(3))
        {
            let match_start = full_match.start();
            let match_end = full_match.end();
            let placeholder = " ".repeat(match_end - match_start);

            result.replace_range(match_start + offset..match_end + offset, &placeholder);
            offset += placeholder.len() - (match_end - match_start);
        }
    }

    result
}

#[derive(Default)]
pub struct MD037SpacesAroundEmphasis;

impl Rule for MD037SpacesAroundEmphasis {
    fn name(&self) -> &'static str {
        "MD037"
    }

    fn description(&self) -> &'static str {
        "Spaces inside emphasis markers"
    }

    fn check(&self, content: &str) -> LintResult {
        let _timer = crate::profiling::ScopedTimer::new("MD037_check");

        // Optimize for empty content
        if content.is_empty() {
            return Ok(vec![]);
        }

        // Early check if any emphasis markers exist at all
        if !content.contains('*') && !content.contains('_') {
            return Ok(vec![]);
        }

        let mut warnings = Vec::new();
        let lines: Vec<&str> = content.lines().collect();
        let mut code_block_state = CodeBlockState::default();
        let line_index = LineIndex::new(content.to_string());

        for (i, line) in lines.iter().enumerate() {
            // Update code block state
            {
                let _timer = crate::profiling::ScopedTimer::new("MD037_update_code_block");
                code_block_state.update(line);
            }

            // Skip processing if we're in a code block
            if code_block_state.is_in_code_block(line) {
                continue;
            }

            // Skip if the line doesn't contain any emphasis markers
            if !line.contains('*') && !line.contains('_') {
                continue;
            }

            // Process the line for emphasis patterns
            let line_no_code = {
                let _timer = crate::profiling::ScopedTimer::new("MD037_replace_inline_code");
                replace_inline_code(line)
            };

            {
                let _timer = crate::profiling::ScopedTimer::new("MD037_check_patterns");
                check_emphasis_patterns(&line_no_code, i + 1, line, &mut warnings, &line_index);
            }
        }

        Ok(warnings)
    }

    fn fix(&self, content: &str) -> Result<String, LintError> {
        let _timer = crate::profiling::ScopedTimer::new("MD037_fix");

        let lines: Vec<&str> = content.lines().collect();
        let mut fixed_lines = Vec::new();
        let mut code_block_state = CodeBlockState::new();

        for line in lines.iter() {
            // Track code blocks
            if FENCED_CODE_BLOCK_START.is_match(line) {
                code_block_state.in_fenced_code = true;
                fixed_lines.push(line.to_string());
                fixed_lines.push('\n'.to_string());
                continue;
            }

            // Update code block state
            code_block_state.update(line);

            // Don't modify lines in code blocks
            if code_block_state.is_in_code_block(line) {
                fixed_lines.push(line.to_string());
                continue;
            }

            // Fix emphasis patterns
            fixed_lines.push(fix_emphasis_patterns(line));
        }

        // Join lines and preserve trailing newline
        let result = if fixed_lines.is_empty() {
            String::new()
        } else {
            fixed_lines.join("\n")
        };

        // Preserve trailing newline if original had it
        let result = if content.ends_with('\n') {
            format!("{}\n", result.trim_end())
        } else {
            result
        };

        Ok(result)
    }
}

// Check for spaces inside emphasis markers with enhanced handling
fn check_emphasis_patterns(
    line: &str,
    line_num: usize,
    original_line: &str,
    warnings: &mut Vec<LintWarning>,
    line_index: &LineIndex,
) {
    // Skip if this is a list marker rather than emphasis
    if LIST_MARKER.is_match(line) {
        return;
    }

    // Skip documentation patterns
    let trimmed = line.trim_start();
    if (trimmed.starts_with("* *") && line.contains("*:"))
        || (trimmed.starts_with("* **") && line.contains("**:"))
        || DOC_METADATA_PATTERN.is_match(line)
        || BOLD_TEXT_PATTERN.is_match(line)
    {
        return;
    }

    // Skip valid emphasis at the start of a line
    if VALID_START_EMPHASIS.is_match(line) {
        // Still check the rest of the line for emphasis issues
        if let Some(emphasis_start) = line.find(' ') {
            let rest_of_line = &line[emphasis_start..];
            if rest_of_line.contains('*') {
                check_emphasis_with_pattern(
                    rest_of_line,
                    &ASTERISK_EMPHASIS,
                    "*",
                    line_num,
                    original_line,
                    warnings,
                    line_index,
                );
                check_emphasis_with_pattern(
                    rest_of_line,
                    &DOUBLE_ASTERISK_EMPHASIS,
                    "**",
                    line_num,
                    original_line,
                    warnings,
                    line_index,
                );
            }
            if rest_of_line.contains('_') {
                check_emphasis_with_pattern(
                    rest_of_line,
                    &UNDERSCORE_EMPHASIS,
                    "_",
                    line_num,
                    original_line,
                    warnings,
                    line_index,
                );
                check_emphasis_with_pattern(
                    rest_of_line,
                    &DOUBLE_UNDERSCORE_EMPHASIS,
                    "__",
                    line_num,
                    original_line,
                    warnings,
                    line_index,
                );
            }
        }
        return;
    }

    // Check emphasis patterns based on marker type
    if line.contains('*') {
        check_emphasis_with_pattern(
            line,
            &ASTERISK_EMPHASIS,
            "*",
            line_num,
            original_line,
            warnings,
            line_index,
        );
        check_emphasis_with_pattern(
            line,
            &DOUBLE_ASTERISK_EMPHASIS,
            "**",
            line_num,
            original_line,
            warnings,
            line_index,
        );
    }

    if line.contains('_') {
        check_emphasis_with_pattern(
            line,
            &UNDERSCORE_EMPHASIS,
            "_",
            line_num,
            original_line,
            warnings,
            line_index,
        );
        check_emphasis_with_pattern(
            line,
            &DOUBLE_UNDERSCORE_EMPHASIS,
            "__",
            line_num,
            original_line,
            warnings,
            line_index,
        );
    }
}

// Check a specific emphasis pattern and add warnings
fn check_emphasis_with_pattern(
    line: &str,
    pattern: &Regex,
    _marker_type: &str,
    line_num: usize,
    original_line: &str,
    warnings: &mut Vec<LintWarning>,
    line_index: &LineIndex,
) {
    for m in pattern.find_iter(line) {
        // Don't flag at the beginning of a line if it could be confused with a list marker
        if m.start() == 0 && (line.starts_with('*') || line.starts_with("**")) {
            continue;
        }

        // Compute the actual position in the original line
        let actual_start = find_actual_position(original_line, m.start());

        let fixed = fix_specific_emphasis_section(original_line, m.start(), m.end());
        let md_text = &original_line[m.start()..m.end()];
        warnings.push(LintWarning {
            line: line_num,
            column: actual_start + 1,
            message: format!("Spaces inside emphasis markers: '{}'", md_text),
            severity: Severity::Warning,
            fix: Some(Fix {
                range: line_index.line_col_to_byte_range(line_num, actual_start + 1),
                replacement: fixed,
            }),
        });
    }
}

// Find the actual position in the original line accounting for code spans
fn find_actual_position(original_line: &str, position_in_processed: usize) -> usize {
    // This is a simplification - for a complete solution, we would need to
    // track character positions during the inline code replacement
    let mut in_code = false;
    let mut backtick_count = 0;
    let mut processed_pos = 0;

    for (i, c) in original_line.chars().enumerate() {
        if c == '`' {
            backtick_count += 1;
            if backtick_count == 1 {
                in_code = !in_code;
            } else if backtick_count > 1 && !in_code {
                // Multiple backticks starting code span
                in_code = true;
                backtick_count = 0;
            } else if backtick_count > 1 && in_code {
                // Multiple backticks ending code span
                in_code = false;
                backtick_count = 0;
            }
        } else {
            backtick_count = 0;

            if !in_code {
                processed_pos += 1;
            }

            if processed_pos > position_in_processed {
                return i;
            }
        }
    }

    // Fallback
    position_in_processed.min(original_line.len())
}

// Fix a specific section of emphasis
fn fix_specific_emphasis_section(line: &str, start_approx: usize, end_approx: usize) -> String {
    // Try to identify the specific emphasis section
    let section = &line[start_approx.min(line.len())..end_approx.min(line.len())];

    // Detect the type of emphasis
    if section.starts_with("**") && section.ends_with("**") {
        let content = section
            .trim_start_matches("**")
            .trim_end_matches("**")
            .trim();
        return format!("**{}**", content);
    } else if section.starts_with('*') && section.ends_with('*') {
        let content = section.trim_start_matches('*').trim_end_matches('*').trim();
        return format!("*{}*", content);
    } else if section.starts_with("__") && section.ends_with("__") {
        let content = section
            .trim_start_matches("__")
            .trim_end_matches("__")
            .trim();
        return format!("__{}__", content);
    } else if section.starts_with('_') && section.ends_with('_') {
        let content = section.trim_start_matches('_').trim_end_matches('_').trim();
        return format!("_{}_", content);
    }

    // Fallback - fix the entire line
    fix_emphasis_patterns(line)
}

// Fix spaces inside emphasis markers
fn fix_emphasis_patterns(line: &str) -> String {
    // Save code spans first
    let (line_no_code, code_spans) = extract_code_spans(line);

    let mut result = line_no_code;

    // Fix emphasis patterns
    result = ASTERISK_EMPHASIS
        .replace_all(&result, |caps: &regex::Captures| {
            for i in 1..4 {
                if let Some(m) = caps.get(i) {
                    return format!("*{}*", m.as_str());
                }
            }
            caps.get(0).map_or("", |m| m.as_str()).to_string()
        })
        .to_string();

    result = DOUBLE_ASTERISK_EMPHASIS
        .replace_all(&result, |caps: &regex::Captures| {
            for i in 1..4 {
                if let Some(m) = caps.get(i) {
                    return format!("**{}**", m.as_str());
                }
            }
            caps.get(0).map_or("", |m| m.as_str()).to_string()
        })
        .to_string();

    result = UNDERSCORE_EMPHASIS
        .replace_all(&result, |caps: &regex::Captures| {
            for i in 1..4 {
                if let Some(m) = caps.get(i) {
                    return format!("_{}_", m.as_str());
                }
            }
            caps.get(0).map_or("", |m| m.as_str()).to_string()
        })
        .to_string();

    result = DOUBLE_UNDERSCORE_EMPHASIS
        .replace_all(&result, |caps: &regex::Captures| {
            for i in 1..4 {
                if let Some(m) = caps.get(i) {
                    return format!("__{}__", m.as_str());
                }
            }
            caps.get(0).map_or("", |m| m.as_str()).to_string()
        })
        .to_string();

    // Restore code spans
    restore_code_spans(result, code_spans)
}

// Extract code spans from a line, replacing them with placeholders
fn extract_code_spans(line: &str) -> (String, Vec<(String, String)>) {
    let mut result = line.to_string();
    let mut code_spans = Vec::new();
    let mut positions = Vec::new();

    for (i, cap) in INLINE_CODE.captures_iter(line).enumerate() {
        if let Some(m) = cap.get(0) {
            let code_span = line[m.start()..m.end()].to_string();
            let placeholder = format!("CODE_SPAN_{}", i);
            code_spans.push((placeholder.clone(), code_span));
            positions.push((m.start(), m.end(), placeholder));
        }
    }

    // Replace code spans in reverse order to maintain indices
    positions.sort_by(|a, b| b.0.cmp(&a.0));
    for (start, end, placeholder) in positions {
        if start < result.len() && end <= result.len() {
            result.replace_range(start..end, &placeholder);
        }
    }

    (result, code_spans)
}

// Restore code spans from placeholders
fn restore_code_spans(mut content: String, code_spans: Vec<(String, String)>) -> String {
    for (placeholder, code_span) in code_spans {
        content = content.replace(&placeholder, &code_span);
    }
    content
}
