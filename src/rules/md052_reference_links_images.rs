use crate::rule::{LintError, LintResult, LintWarning, Rule, Severity};
use crate::utils::mkdocs_patterns::is_mkdocs_auto_reference;
use crate::utils::range_utils::calculate_match_range;
use crate::utils::regex_cache::{HTML_COMMENT_PATTERN, SHORTCUT_REF_REGEX};
use crate::utils::skip_context::{is_in_front_matter, is_in_math_context, is_in_table_cell};
use lazy_static::lazy_static;
use regex::Regex;
use std::collections::{HashMap, HashSet};

lazy_static! {
    // Pattern to match reference definitions [ref]: url
    // Note: \S* instead of \S+ to allow empty definitions like [ref]:
    // The capturing group handles nested brackets to support cases like [`union[t, none]`]:
    static ref REF_REGEX: Regex = Regex::new(r"^\s*\[((?:[^\[\]\\]|\\.|\[[^\]]*\])*)\]:\s*.*").unwrap();

    // Pattern for list items to exclude from reference checks (standard regex is fine)
    static ref LIST_ITEM_REGEX: Regex = Regex::new(r"^\s*[-*+]\s+(?:\[[xX\s]\]\s+)?").unwrap();

    // Pattern for code blocks (standard regex is fine)
    static ref FENCED_CODE_START: Regex = Regex::new(r"^(\s*)(`{3,}|~{3,})").unwrap();

    // Pattern for output example sections (standard regex is fine)
    static ref OUTPUT_EXAMPLE_START: Regex = Regex::new(r"^#+\s*(?:Output|Example|Output Style|Output Format)\s*$").unwrap();

    // Pattern for GitHub alerts/callouts in blockquotes (e.g., > [!NOTE], > [!TIP], etc.)
    // Extended to include additional common alert types
    static ref GITHUB_ALERT_REGEX: Regex = Regex::new(r"^\s*>\s*\[!(NOTE|TIP|IMPORTANT|WARNING|CAUTION|INFO|SUCCESS|FAILURE|DANGER|BUG|EXAMPLE|QUOTE)\]").unwrap();

    // Pattern to detect URLs that may contain brackets (IPv6, API endpoints, etc.)
    // This pattern specifically looks for:
    // - IPv6 addresses: https://[::1] or https://[2001:db8::1]
    // - IPv6 with zone IDs: https://[fe80::1%eth0]
    // - IPv6 mixed notation: https://[::ffff:192.0.2.1]
    // - API paths with array notation: https://api.example.com/users[0]
    // But NOT markdown reference links that happen to follow URLs
    static ref URL_WITH_BRACKETS: Regex = Regex::new(
        r"https?://(?:\[[0-9a-fA-F:.%]+\]|[^\s\[\]]+/[^\s]*\[\d+\])"
    ).unwrap();
}

/// Rule MD052: Reference links and images should use reference style
///
/// See [docs/md052.md](../../docs/md052.md) for full documentation, configuration, and examples.
///
/// This rule is triggered when a reference link or image uses a reference that isn't defined.
#[derive(Clone, Default)]
pub struct MD052ReferenceLinkImages {}

impl MD052ReferenceLinkImages {
    pub fn new() -> Self {
        Self {}
    }

    /// Strip surrounding backticks from a string
    /// Used for MkDocs auto-reference detection where `module.Class` should be treated as module.Class
    fn strip_backticks(s: &str) -> &str {
        s.trim_start_matches('`').trim_end_matches('`')
    }

    /// Check if a string is a valid Python identifier
    /// Used for MkDocs auto-reference detection where single-word backtick-wrapped identifiers
    /// like `str`, `int`, etc. should be accepted as valid auto-references
    fn is_valid_python_identifier(s: &str) -> bool {
        if s.is_empty() {
            return false;
        }
        let first_char = s.chars().next().unwrap();
        if !first_char.is_ascii_alphabetic() && first_char != '_' {
            return false;
        }
        s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
    }

    /// Check if a pattern is likely NOT a markdown reference
    /// Returns true if this pattern should be skipped
    fn is_likely_not_reference(text: &str) -> bool {
        // Skip numeric patterns (array indices, ranges)
        if text.chars().all(|c| c.is_ascii_digit()) {
            return true;
        }

        // Skip numeric ranges like [1:3], [0:10], etc.
        if text.contains(':') && text.chars().all(|c| c.is_ascii_digit() || c == ':') {
            return true;
        }

        // Skip patterns that look like config sections [tool.something], [section.subsection]
        // But not if they contain other non-alphanumeric chars like hyphens or underscores
        if text.contains('.') && !text.contains(' ') && !text.contains('-') && !text.contains('_') {
            // Config sections typically have dots, no spaces, and only alphanumeric + dots
            return true;
        }

        // Skip glob/wildcard patterns like [*], [...], [**]
        if text == "*" || text == "..." || text == "**" {
            return true;
        }

        // Skip patterns that look like file paths [dir/file], [src/utils]
        if text.contains('/') && !text.contains(' ') && !text.starts_with("http") {
            return true;
        }

        // Skip programming type annotations like [int, str], [Dict[str, Any]]
        // These typically have commas and/or nested brackets
        if text.contains(',') || text.contains('[') || text.contains(']') {
            // Check if it looks like a type annotation pattern
            return true;
        }

        // Note: We don't filter out patterns with backticks because backticks in reference names
        // are valid markdown syntax, e.g., [`dataclasses.InitVar`] is a valid reference name

        // Skip patterns that look like module/class paths ONLY if they don't have backticks
        // Backticks indicate intentional code formatting in a reference name
        // e.g., skip [dataclasses.initvar] but allow [`typing.ClassVar`]
        if !text.contains('`')
            && text.contains('.')
            && !text.contains(' ')
            && !text.contains('-')
            && !text.contains('_')
        {
            return true;
        }

        // Note: We don't filter based on word count anymore because legitimate references
        // can have many words, like "python language reference for import statements"
        // Word count filtering was causing false positives where valid references were
        // being incorrectly flagged as unused

        // Skip patterns that are just punctuation or operators
        if text.chars().all(|c| !c.is_alphanumeric() && c != ' ') {
            return true;
        }

        // Skip very short non-word patterns (likely operators or syntax)
        if text.len() <= 2 && !text.chars().all(|c| c.is_alphabetic()) {
            return true;
        }

        // Skip quoted patterns like ["E501"], ["ALL"], ["E", "F"]
        if (text.starts_with('"') && text.ends_with('"'))
            || (text.starts_with('\'') && text.ends_with('\''))
            || text.contains('"')
            || text.contains('\'')
        {
            return true;
        }

        // Skip descriptive patterns with colon like [default: the project root]
        // But allow simple numeric ranges which are handled above
        if text.contains(':') && text.contains(' ') {
            return true;
        }

        // Skip alert/admonition patterns like [!WARN], [!NOTE], etc.
        if text.starts_with('!') {
            return true;
        }

        // Skip single uppercase letters (likely type parameters) like [T], [U], [K], [V]
        if text.len() == 1 && text.chars().all(|c| c.is_ascii_uppercase()) {
            return true;
        }

        // Skip common programming type names and short identifiers
        // that are likely not markdown references
        let common_non_refs = [
            "object", "Object", "any", "Any", "inv", "void", "bool", "int", "float", "str", "char", "i8", "i16", "i32",
            "i64", "i128", "isize", "u8", "u16", "u32", "u64", "u128", "usize", "f32", "f64",
        ];

        if common_non_refs.contains(&text) {
            return true;
        }

        false
    }

    /// Check if a position is inside any code span
    fn is_in_code_span(line: usize, col: usize, code_spans: &[crate::lint_context::CodeSpan]) -> bool {
        code_spans
            .iter()
            .any(|span| span.line == line && col >= span.start_col && col < span.end_col)
    }

    /// Check if a byte position is within an HTML comment
    fn is_in_html_comment(content: &str, byte_pos: usize) -> bool {
        for m in HTML_COMMENT_PATTERN.find_iter(content) {
            if m.start() <= byte_pos && byte_pos < m.end() {
                return true;
            }
        }
        false
    }

    /// Check if a byte position is within an HTML tag
    fn is_in_html_tag(ctx: &crate::lint_context::LintContext, byte_pos: usize) -> bool {
        // Check HTML tags
        for html_tag in ctx.html_tags().iter() {
            if html_tag.byte_offset <= byte_pos && byte_pos < html_tag.byte_end {
                return true;
            }
        }
        false
    }

    fn extract_references(&self, content: &str, mkdocs_mode: bool) -> HashSet<String> {
        use crate::config::MarkdownFlavor;
        use crate::utils::skip_context::is_mkdocs_snippet_line;

        let mut references = HashSet::new();
        let mut in_code_block = false;
        let mut code_fence_marker = String::new();

        for line in content.lines() {
            // Skip lines that look like MkDocs snippet markers (only in MkDocs mode)
            if is_mkdocs_snippet_line(
                line,
                if mkdocs_mode {
                    MarkdownFlavor::MkDocs
                } else {
                    MarkdownFlavor::Standard
                },
            ) {
                continue;
            }
            // Handle code block boundaries
            if let Some(cap) = FENCED_CODE_START.captures(line) {
                if let Some(fence) = cap.get(2) {
                    // Get the fence marker (``` or ~~~) without the indentation
                    let fence_str = fence.as_str();
                    if !in_code_block {
                        in_code_block = true;
                        code_fence_marker = fence_str.to_string();
                    } else if line.trim_start().starts_with(&code_fence_marker) {
                        // Check if this could be a closing fence
                        let trimmed = line.trim_start();
                        // A closing fence should be just the fence characters, possibly with trailing whitespace
                        if trimmed.starts_with(&code_fence_marker) {
                            let after_fence = &trimmed[code_fence_marker.len()..];
                            if after_fence.trim().is_empty() {
                                in_code_block = false;
                                code_fence_marker.clear();
                            }
                        }
                    }
                }
                continue;
            }

            // Skip lines in code blocks
            if in_code_block {
                continue;
            }

            // Check for abbreviation syntax (*[ABBR]: Definition) and skip it
            // Abbreviations are not reference links and should not be tracked
            if line.trim_start().starts_with("*[") {
                continue;
            }

            if let Some(cap) = REF_REGEX.captures(line) {
                // Store references in lowercase for case-insensitive comparison
                if let Some(reference) = cap.get(1) {
                    references.insert(reference.as_str().to_lowercase());
                }
            }
        }

        references
    }

    fn find_undefined_references(
        &self,
        content: &str,
        references: &HashSet<String>,
        ctx: &crate::lint_context::LintContext,
        mkdocs_mode: bool,
    ) -> Vec<(usize, usize, usize, String)> {
        let mut undefined = Vec::new();
        let mut reported_refs = HashMap::new();
        let mut in_code_block = false;
        let mut code_fence_marker = String::new();
        let mut in_example_section = false;

        // Get code spans once for the entire function
        let code_spans = ctx.code_spans();

        // Use cached data for reference links and images
        for link in &ctx.links {
            if !link.is_reference {
                continue; // Skip inline links
            }

            // Skip links inside code spans
            if Self::is_in_code_span(link.line, link.start_col, &code_spans) {
                continue;
            }

            // Skip links inside HTML comments
            if Self::is_in_html_comment(content, link.byte_offset) {
                continue;
            }

            // Skip links inside HTML tags
            if Self::is_in_html_tag(ctx, link.byte_offset) {
                continue;
            }

            // Skip links inside math contexts
            if is_in_math_context(ctx, link.byte_offset) {
                continue;
            }

            // Skip links inside table cells
            if is_in_table_cell(ctx, link.line, link.start_col) {
                continue;
            }

            // Skip links inside frontmatter (convert from 1-based to 0-based line numbers)
            if is_in_front_matter(content, link.line.saturating_sub(1)) {
                continue;
            }

            if let Some(ref_id) = &link.reference_id {
                let reference_lower = ref_id.to_lowercase();

                // Skip MkDocs auto-references if in MkDocs mode
                // Check both the reference_id and the link text for shorthand references
                // Strip backticks since MkDocs resolves `module.Class` as module.Class
                let stripped_ref = Self::strip_backticks(ref_id);
                let stripped_text = Self::strip_backticks(&link.text);
                if mkdocs_mode
                    && (is_mkdocs_auto_reference(stripped_ref)
                        || is_mkdocs_auto_reference(stripped_text)
                        || (ref_id != stripped_ref && Self::is_valid_python_identifier(stripped_ref))
                        || (link.text.as_str() != stripped_text && Self::is_valid_python_identifier(stripped_text)))
                {
                    continue;
                }

                // Check if reference is defined
                if !references.contains(&reference_lower) && !reported_refs.contains_key(&reference_lower) {
                    // Check if the line is in an example section or list item
                    if let Some(line_info) = ctx.line_info(link.line) {
                        if OUTPUT_EXAMPLE_START.is_match(&line_info.content) {
                            in_example_section = true;
                            continue;
                        }

                        if in_example_section {
                            continue;
                        }

                        // Skip list items
                        if LIST_ITEM_REGEX.is_match(&line_info.content) {
                            continue;
                        }

                        // Skip lines that are HTML content
                        let trimmed = line_info.content.trim_start();
                        if trimmed.starts_with('<') {
                            continue;
                        }
                    }

                    let match_len = link.byte_end - link.byte_offset;
                    undefined.push((link.line - 1, link.start_col, match_len, ref_id.clone()));
                    reported_refs.insert(reference_lower, true);
                }
            }
        }

        // Use cached data for reference images
        for image in &ctx.images {
            if !image.is_reference {
                continue; // Skip inline images
            }

            // Skip images inside code spans
            if Self::is_in_code_span(image.line, image.start_col, &code_spans) {
                continue;
            }

            // Skip images inside HTML comments
            if Self::is_in_html_comment(content, image.byte_offset) {
                continue;
            }

            // Skip images inside HTML tags
            if Self::is_in_html_tag(ctx, image.byte_offset) {
                continue;
            }

            // Skip images inside math contexts
            if is_in_math_context(ctx, image.byte_offset) {
                continue;
            }

            // Skip images inside table cells
            if is_in_table_cell(ctx, image.line, image.start_col) {
                continue;
            }

            // Skip images inside frontmatter (convert from 1-based to 0-based line numbers)
            if is_in_front_matter(content, image.line.saturating_sub(1)) {
                continue;
            }

            if let Some(ref_id) = &image.reference_id {
                let reference_lower = ref_id.to_lowercase();

                // Skip MkDocs auto-references if in MkDocs mode
                // Check both the reference_id and the alt text for shorthand references
                // Strip backticks since MkDocs resolves `module.Class` as module.Class
                let stripped_ref = Self::strip_backticks(ref_id);
                let stripped_alt = Self::strip_backticks(&image.alt_text);
                if mkdocs_mode
                    && (is_mkdocs_auto_reference(stripped_ref)
                        || is_mkdocs_auto_reference(stripped_alt)
                        || (ref_id != stripped_ref && Self::is_valid_python_identifier(stripped_ref))
                        || (image.alt_text.as_str() != stripped_alt && Self::is_valid_python_identifier(stripped_alt)))
                {
                    continue;
                }

                // Check if reference is defined
                if !references.contains(&reference_lower) && !reported_refs.contains_key(&reference_lower) {
                    // Check if the line is in an example section or list item
                    if let Some(line_info) = ctx.line_info(image.line) {
                        if OUTPUT_EXAMPLE_START.is_match(&line_info.content) {
                            in_example_section = true;
                            continue;
                        }

                        if in_example_section {
                            continue;
                        }

                        // Skip list items
                        if LIST_ITEM_REGEX.is_match(&line_info.content) {
                            continue;
                        }

                        // Skip lines that are HTML content
                        let trimmed = line_info.content.trim_start();
                        if trimmed.starts_with('<') {
                            continue;
                        }
                    }

                    let match_len = image.byte_end - image.byte_offset;
                    undefined.push((image.line - 1, image.start_col, match_len, ref_id.clone()));
                    reported_refs.insert(reference_lower, true);
                }
            }
        }

        // Build a set of byte ranges that are already covered by parsed links/images
        let mut covered_ranges: Vec<(usize, usize)> = Vec::new();

        // Add ranges from parsed links
        for link in &ctx.links {
            covered_ranges.push((link.byte_offset, link.byte_end));
        }

        // Add ranges from parsed images
        for image in &ctx.images {
            covered_ranges.push((image.byte_offset, image.byte_end));
        }

        // Sort ranges by start position
        covered_ranges.sort_by_key(|&(start, _)| start);

        // Handle shortcut references [text] which aren't captured in ctx.links
        // Need to use regex for these
        let lines: Vec<&str> = content.lines().collect();
        in_example_section = false; // Reset for line-by-line processing

        for (line_num, line) in lines.iter().enumerate() {
            // Skip lines in frontmatter (line_num is already 0-based)
            if is_in_front_matter(content, line_num) {
                continue;
            }

            // Handle code blocks
            if let Some(cap) = FENCED_CODE_START.captures(line) {
                if let Some(fence) = cap.get(2) {
                    // Get the fence marker (``` or ~~~) without the indentation
                    let fence_str = fence.as_str();
                    if !in_code_block {
                        in_code_block = true;
                        code_fence_marker = fence_str.to_string();
                    } else if line.trim_start().starts_with(&code_fence_marker) {
                        // Check if this could be a closing fence
                        let trimmed = line.trim_start();
                        // A closing fence should be just the fence characters, possibly with trailing whitespace
                        if trimmed.starts_with(&code_fence_marker) {
                            let after_fence = &trimmed[code_fence_marker.len()..];
                            if after_fence.trim().is_empty() {
                                in_code_block = false;
                                code_fence_marker.clear();
                            }
                        }
                    }
                }
                continue;
            }

            if in_code_block {
                continue;
            }

            // Check for example sections
            if OUTPUT_EXAMPLE_START.is_match(line) {
                in_example_section = true;
                continue;
            }

            if in_example_section {
                // Check if we're exiting the example section (another heading)
                if line.starts_with('#') && !OUTPUT_EXAMPLE_START.is_match(line) {
                    in_example_section = false;
                } else {
                    continue;
                }
            }

            // Skip list items
            if LIST_ITEM_REGEX.is_match(line) {
                continue;
            }

            // Skip lines that are HTML content
            let trimmed_line = line.trim_start();
            if trimmed_line.starts_with('<') {
                continue;
            }

            // Skip GitHub alerts/callouts (e.g., > [!TIP])
            if GITHUB_ALERT_REGEX.is_match(line) {
                continue;
            }

            // Skip abbreviation definitions (*[ABBR]: Definition)
            // These are not reference links and should not be checked
            if trimmed_line.starts_with("*[") {
                continue;
            }

            // Collect positions of brackets that are part of URLs (IPv6, etc.)
            // so we can exclude them from reference checking
            let mut url_bracket_ranges: Vec<(usize, usize)> = Vec::new();
            for mat in URL_WITH_BRACKETS.find_iter(line) {
                // Find all bracket pairs within this URL match
                let url_str = mat.as_str();
                let url_start = mat.start();

                // Find brackets within the URL (e.g., in https://[::1]:8080)
                let mut idx = 0;
                while idx < url_str.len() {
                    if let Some(bracket_start) = url_str[idx..].find('[') {
                        let bracket_start_abs = url_start + idx + bracket_start;
                        if let Some(bracket_end) = url_str[idx + bracket_start + 1..].find(']') {
                            let bracket_end_abs = url_start + idx + bracket_start + 1 + bracket_end + 1;
                            url_bracket_ranges.push((bracket_start_abs, bracket_end_abs));
                            idx += bracket_start + bracket_end + 2;
                        } else {
                            break;
                        }
                    } else {
                        break;
                    }
                }
            }

            // Check shortcut references: [reference]
            if let Ok(captures) = SHORTCUT_REF_REGEX.captures_iter(line).collect::<Result<Vec<_>, _>>() {
                for cap in captures {
                    if let Some(ref_match) = cap.get(1) {
                        // Check if this bracket is part of a URL (IPv6, etc.)
                        let bracket_start = cap.get(0).unwrap().start();
                        let bracket_end = cap.get(0).unwrap().end();

                        // Skip if this bracket pair is within any URL bracket range
                        let is_in_url = url_bracket_ranges
                            .iter()
                            .any(|&(url_start, url_end)| bracket_start >= url_start && bracket_end <= url_end);

                        if is_in_url {
                            continue;
                        }

                        let reference = ref_match.as_str();
                        let reference_lower = reference.to_lowercase();

                        // Skip patterns that are likely not markdown references
                        if Self::is_likely_not_reference(reference) {
                            continue;
                        }

                        // Skip GitHub alerts (including extended types)
                        if let Some(alert_type) = reference.strip_prefix('!')
                            && matches!(
                                alert_type,
                                "NOTE"
                                    | "TIP"
                                    | "WARNING"
                                    | "IMPORTANT"
                                    | "CAUTION"
                                    | "INFO"
                                    | "SUCCESS"
                                    | "FAILURE"
                                    | "DANGER"
                                    | "BUG"
                                    | "EXAMPLE"
                                    | "QUOTE"
                            )
                        {
                            continue;
                        }

                        // Skip MkDocs snippet section markers like [start:section] or [end:section]
                        // when they appear as part of snippet syntax (e.g., # -8<- [start:section])
                        if mkdocs_mode
                            && (reference.starts_with("start:") || reference.starts_with("end:"))
                            && (crate::utils::mkdocs_snippets::is_snippet_section_start(line)
                                || crate::utils::mkdocs_snippets::is_snippet_section_end(line))
                        {
                            continue;
                        }

                        // Skip MkDocs auto-references if in MkDocs mode
                        // Strip backticks since MkDocs resolves `module.Class` as module.Class
                        let stripped_ref = Self::strip_backticks(reference);
                        if mkdocs_mode
                            && (is_mkdocs_auto_reference(stripped_ref)
                                || (reference != stripped_ref && Self::is_valid_python_identifier(stripped_ref)))
                        {
                            continue;
                        }

                        if !references.contains(&reference_lower) && !reported_refs.contains_key(&reference_lower) {
                            let full_match = cap.get(0).unwrap();
                            let col = full_match.start();

                            // Skip if inside code span
                            let code_spans = ctx.code_spans();
                            if Self::is_in_code_span(line_num + 1, col, &code_spans) {
                                continue;
                            }

                            // Check if this position is within a covered range
                            let line_start_byte = ctx.line_offsets[line_num];
                            let byte_pos = line_start_byte + col;

                            // Skip if inside HTML comment
                            if Self::is_in_html_comment(content, byte_pos) {
                                continue;
                            }

                            // Skip if inside HTML tag
                            if Self::is_in_html_tag(ctx, byte_pos) {
                                continue;
                            }

                            // Skip if inside math context
                            if is_in_math_context(ctx, byte_pos) {
                                continue;
                            }

                            // Skip if inside table cell
                            if is_in_table_cell(ctx, line_num + 1, col) {
                                continue;
                            }

                            let byte_end = byte_pos + (full_match.end() - full_match.start());

                            // Check if this shortcut ref overlaps with any parsed link/image
                            let mut is_covered = false;
                            for &(range_start, range_end) in &covered_ranges {
                                if range_start <= byte_pos && byte_end <= range_end {
                                    // This shortcut ref is completely within a parsed link/image
                                    is_covered = true;
                                    break;
                                }
                                if range_start > byte_end {
                                    // No need to check further (ranges are sorted)
                                    break;
                                }
                            }

                            if is_covered {
                                continue;
                            }

                            // More sophisticated checks to avoid false positives

                            // Check 1: If preceded by ], this might be part of [text][ref]
                            // Look for the pattern ...][ref] and check if there's a matching [ before
                            let line_chars: Vec<char> = line.chars().collect();
                            if col > 0 && col <= line_chars.len() && line_chars.get(col - 1) == Some(&']') {
                                // Look backwards for a [ that would make this [text][ref]
                                let mut bracket_count = 1; // We already saw one ]
                                let mut check_pos = col.saturating_sub(2);
                                let mut found_opening = false;

                                while check_pos > 0 && check_pos < line_chars.len() {
                                    match line_chars.get(check_pos) {
                                        Some(&']') => bracket_count += 1,
                                        Some(&'[') => {
                                            bracket_count -= 1;
                                            if bracket_count == 0 {
                                                // Check if this [ is escaped
                                                if check_pos == 0 || line_chars.get(check_pos - 1) != Some(&'\\') {
                                                    found_opening = true;
                                                }
                                                break;
                                            }
                                        }
                                        _ => {}
                                    }
                                    if check_pos == 0 {
                                        break;
                                    }
                                    check_pos = check_pos.saturating_sub(1);
                                }

                                if found_opening {
                                    // This is part of [text][ref], skip it
                                    continue;
                                }
                            }

                            // Check 2: If there's an escaped bracket pattern before this
                            // e.g., \[text\][ref], the [ref] shouldn't be treated as a shortcut
                            let before_text = &line[..col];
                            if before_text.contains("\\]") {
                                // Check if there's a \[ before the \]
                                if let Some(escaped_close_pos) = before_text.rfind("\\]") {
                                    let search_text = &before_text[..escaped_close_pos];
                                    if search_text.contains("\\[") {
                                        // This looks like \[...\][ref], skip it
                                        continue;
                                    }
                                }
                            }

                            let match_len = full_match.end() - full_match.start();
                            undefined.push((line_num, col, match_len, reference.to_string()));
                            reported_refs.insert(reference_lower, true);
                        }
                    }
                }
            }
        }

        undefined
    }
}

impl Rule for MD052ReferenceLinkImages {
    fn name(&self) -> &'static str {
        "MD052"
    }

    fn description(&self) -> &'static str {
        "Reference links and images should use a reference that exists"
    }

    fn check(&self, ctx: &crate::lint_context::LintContext) -> LintResult {
        let content = ctx.content;
        let mut warnings = Vec::new();

        // OPTIMIZATION: Early exit if no reference-style links/images exist
        // Check if there are any reference-style links or images in the document
        let has_reference_links = ctx.links.iter().any(|l| l.is_reference);
        let has_reference_images = ctx.images.iter().any(|i| i.is_reference);

        // Quick check: If document contains no brackets at all, nothing to check
        if !content.contains('[') {
            return Ok(warnings);
        }

        // Quick check for reference definitions
        let has_reference_definitions = content.contains("]:");

        // If we have no reference links/images AND no reference definitions,
        // then check if we might have shortcut references [text]
        if !has_reference_links && !has_reference_images && !has_reference_definitions {
            // Only do expensive shortcut checking if we have brackets but no links/images/refs
            // This handles the case where all brackets are inline links [text](url)
            let all_brackets_are_inline = ctx.links.iter().all(|l| !l.is_reference)
                && ctx.images.iter().all(|i| !i.is_reference)
                && ctx.links.len() + ctx.images.len() > 0;

            if all_brackets_are_inline {
                return Ok(warnings); // All brackets accounted for as inline links/images
            }
        }

        // Check if we're in MkDocs mode from the context
        let mkdocs_mode = ctx.flavor == crate::config::MarkdownFlavor::MkDocs;

        let references = self.extract_references(content, mkdocs_mode);

        // Use optimized detection method with cached link/image data
        for (line_num, col, match_len, reference) in
            self.find_undefined_references(content, &references, ctx, mkdocs_mode)
        {
            let lines: Vec<&str> = content.lines().collect();
            let line_content = lines.get(line_num).unwrap_or(&"");

            // Calculate precise character range for the entire undefined reference
            let (start_line, start_col, end_line, end_col) =
                calculate_match_range(line_num + 1, line_content, col, match_len);

            warnings.push(LintWarning {
                rule_name: Some(self.name()),
                line: start_line,
                column: start_col,
                end_line,
                end_column: end_col,
                message: format!("Reference '{reference}' not found"),
                severity: Severity::Warning,
                fix: None,
            });
        }

        Ok(warnings)
    }

    /// Check if this rule should be skipped for performance
    fn should_skip(&self, ctx: &crate::lint_context::LintContext) -> bool {
        // Skip if content is empty or has no links/images
        ctx.content.is_empty() || !ctx.likely_has_links_or_images()
    }

    fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, LintError> {
        let content = ctx.content;
        // No automatic fix available for undefined references
        Ok(content.to_string())
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn from_config(_config: &crate::config::Config) -> Box<dyn Rule>
    where
        Self: Sized,
    {
        // Flavor is now accessed from LintContext during check
        Box::new(MD052ReferenceLinkImages::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lint_context::LintContext;

    #[test]
    fn test_valid_reference_link() {
        let rule = MD052ReferenceLinkImages::new();
        let content = "[text][ref]\n\n[ref]: https://example.com";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_undefined_reference_link() {
        let rule = MD052ReferenceLinkImages::new();
        let content = "[text][undefined]";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("Reference 'undefined' not found"));
    }

    #[test]
    fn test_valid_reference_image() {
        let rule = MD052ReferenceLinkImages::new();
        let content = "![alt][img]\n\n[img]: image.jpg";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_undefined_reference_image() {
        let rule = MD052ReferenceLinkImages::new();
        let content = "![alt][missing]";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("Reference 'missing' not found"));
    }

    #[test]
    fn test_case_insensitive_references() {
        let rule = MD052ReferenceLinkImages::new();
        let content = "[Text][REF]\n\n[ref]: https://example.com";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_shortcut_reference_valid() {
        let rule = MD052ReferenceLinkImages::new();
        let content = "[ref]\n\n[ref]: https://example.com";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_shortcut_reference_undefined() {
        let rule = MD052ReferenceLinkImages::new();
        let content = "[undefined]";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("Reference 'undefined' not found"));
    }

    #[test]
    fn test_inline_links_ignored() {
        let rule = MD052ReferenceLinkImages::new();
        let content = "[text](https://example.com)";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_inline_images_ignored() {
        let rule = MD052ReferenceLinkImages::new();
        let content = "![alt](image.jpg)";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_references_in_code_blocks_ignored() {
        let rule = MD052ReferenceLinkImages::new();
        let content = "```\n[undefined]\n```\n\n[ref]: https://example.com";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_references_in_inline_code_ignored() {
        let rule = MD052ReferenceLinkImages::new();
        let content = "`[undefined]`";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();

        // References inside inline code spans should be ignored
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_comprehensive_inline_code_detection() {
        let rule = MD052ReferenceLinkImages::new();
        let content = r#"# Test

This `[inside]` should be ignored.
This [outside] should be flagged.
Reference links `[text][ref]` in code are ignored.
Regular reference [text][missing] should be flagged.
Images `![alt][img]` in code are ignored.
Regular image ![alt][badimg] should be flagged.

Multiple `[one]` and `[two]` in code ignored, but [three] is not.

```
[code block content] should be ignored
```

`Multiple [refs] in [same] code span` ignored."#;

        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();

        // Should only flag: outside, missing, badimg, three (4 total)
        assert_eq!(result.len(), 4);

        let messages: Vec<&str> = result.iter().map(|w| &*w.message).collect();
        assert!(messages.iter().any(|m| m.contains("outside")));
        assert!(messages.iter().any(|m| m.contains("missing")));
        assert!(messages.iter().any(|m| m.contains("badimg")));
        assert!(messages.iter().any(|m| m.contains("three")));

        // Should NOT flag any references inside code spans
        assert!(!messages.iter().any(|m| m.contains("inside")));
        assert!(!messages.iter().any(|m| m.contains("one")));
        assert!(!messages.iter().any(|m| m.contains("two")));
        assert!(!messages.iter().any(|m| m.contains("refs")));
        assert!(!messages.iter().any(|m| m.contains("same")));
    }

    #[test]
    fn test_multiple_undefined_references() {
        let rule = MD052ReferenceLinkImages::new();
        let content = "[link1][ref1] [link2][ref2] [link3][ref3]";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 3);
        assert!(result[0].message.contains("ref1"));
        assert!(result[1].message.contains("ref2"));
        assert!(result[2].message.contains("ref3"));
    }

    #[test]
    fn test_mixed_valid_and_undefined() {
        let rule = MD052ReferenceLinkImages::new();
        let content = "[valid][ref] [invalid][missing]\n\n[ref]: https://example.com";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("missing"));
    }

    #[test]
    fn test_empty_reference() {
        let rule = MD052ReferenceLinkImages::new();
        let content = "[text][]\n\n[ref]: https://example.com";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();

        // Empty reference should use the link text as reference
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn test_escaped_brackets_ignored() {
        let rule = MD052ReferenceLinkImages::new();
        let content = "\\[not a link\\]";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_list_items_ignored() {
        let rule = MD052ReferenceLinkImages::new();
        let content = "- [undefined]\n* [another]\n+ [third]";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();

        // List items that look like shortcut references should be ignored
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_output_example_section_ignored() {
        let rule = MD052ReferenceLinkImages::new();
        let content = "## Output\n\n[undefined]\n\n## Normal Section\n\n[missing]";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();

        // Only the reference outside the Output section should be flagged
        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("missing"));
    }

    #[test]
    fn test_reference_definitions_in_code_blocks_ignored() {
        let rule = MD052ReferenceLinkImages::new();
        let content = "[link][ref]\n\n```\n[ref]: https://example.com\n```";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();

        // Reference defined in code block should not count
        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("ref"));
    }

    #[test]
    fn test_multiple_references_to_same_undefined() {
        let rule = MD052ReferenceLinkImages::new();
        let content = "[first][missing] [second][missing] [third][missing]";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();

        // Should only report once per unique reference
        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("missing"));
    }

    #[test]
    fn test_reference_with_special_characters() {
        let rule = MD052ReferenceLinkImages::new();
        let content = "[text][ref-with-hyphens]\n\n[ref-with-hyphens]: https://example.com";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_issue_51_html_attribute_not_reference() {
        // Test for issue #51 - HTML attributes with square brackets shouldn't be treated as references
        let rule = MD052ReferenceLinkImages::new();
        let content = r#"# Example

## Test

Want to fill out this form?

<form method="post">
    <input type="email" name="fields[email]" id="drip-email" placeholder="email@domain.com">
</form>"#;
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(
            result.len(),
            0,
            "HTML attributes with square brackets should not be flagged as undefined references"
        );
    }

    #[test]
    fn test_extract_references() {
        let rule = MD052ReferenceLinkImages::new();
        let content = "[ref1]: url1\n[Ref2]: url2\n[REF3]: url3";
        let refs = rule.extract_references(content, false);

        assert_eq!(refs.len(), 3);
        assert!(refs.contains("ref1"));
        assert!(refs.contains("ref2"));
        assert!(refs.contains("ref3"));
    }

    #[test]
    fn test_inline_code_not_flagged() {
        let rule = MD052ReferenceLinkImages::new();

        // Test that arrays in inline code are not flagged as references
        let content = r#"# Test

Configure with `["JavaScript", "GitHub", "Node.js"]` in your settings.

Also, `[todo]` is not a reference link.

But this [reference] should be flagged.

And this `[inline code]` should not be flagged.
"#;

        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let warnings = rule.check(&ctx).unwrap();

        // Should only flag [reference], not the ones in backticks
        assert_eq!(warnings.len(), 1, "Should only flag one undefined reference");
        assert!(warnings[0].message.contains("'reference'"));
    }

    #[test]
    fn test_code_block_references_ignored() {
        let rule = MD052ReferenceLinkImages::new();

        let content = r#"# Test

```markdown
[undefined] reference in code block
![undefined] image in code block
```

[real-undefined] reference outside
"#;

        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let warnings = rule.check(&ctx).unwrap();

        // Should only flag [real-undefined], not the ones in code block
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].message.contains("'real-undefined'"));
    }

    #[test]
    fn test_html_comments_ignored() {
        // Test for issue #20 - MD052 should not flag content inside HTML comments
        let rule = MD052ReferenceLinkImages::new();

        // Test the exact case from issue #20
        let content = r#"<!--- write fake_editor.py 'import sys\nopen(*sys.argv[1:], mode="wt").write("2 3 4 4 2 3 2")' -->
<!--- set_env EDITOR 'python3 fake_editor.py' -->

```bash
$ python3 vote.py
3 votes for: 2
2 votes for: 3, 4
```"#;
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 0, "Should not flag [1:] inside HTML comments");

        // Test various reference patterns inside HTML comments
        let content = r#"<!-- This is [ref1] and [ref2][ref3] -->
Normal [text][undefined]
<!-- Another [comment][with] references -->"#;
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(
            result.len(),
            1,
            "Should only flag the undefined reference outside comments"
        );
        assert!(result[0].message.contains("undefined"));

        // Test multi-line HTML comments
        let content = r#"<!--
[ref1]
[ref2][ref3]
-->
[actual][undefined]"#;
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(
            result.len(),
            1,
            "Should not flag references in multi-line HTML comments"
        );
        assert!(result[0].message.contains("undefined"));

        // Test mixed scenarios
        let content = r#"<!-- Comment with [1:] pattern -->
Valid [link][ref]
<!-- More [refs][in][comments] -->
![image][missing]

[ref]: https://example.com"#;
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1, "Should only flag missing image reference");
        assert!(result[0].message.contains("missing"));
    }

    #[test]
    fn test_frontmatter_ignored() {
        // Test for issue #24 - MD052 should not flag content inside frontmatter
        let rule = MD052ReferenceLinkImages::new();

        // Test YAML frontmatter with arrays and references
        let content = r#"---
layout: post
title: "My Jekyll Post"
date: 2023-01-01
categories: blog
tags: ["test", "example"]
author: John Doe
---

# My Blog Post

This is the actual markdown content that should be linted.

[undefined] reference should be flagged.

## Section 1

Some content here."#;
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();

        // Should only flag [undefined] in the content, not the ["test", "example"] array in frontmatter
        assert_eq!(
            result.len(),
            1,
            "Should only flag the undefined reference outside frontmatter"
        );
        assert!(result[0].message.contains("undefined"));

        // Test TOML frontmatter
        let content = r#"+++
title = "My Post"
tags = ["example", "test"]
+++

# Content

[missing] reference should be flagged."#;
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(
            result.len(),
            1,
            "Should only flag the undefined reference outside TOML frontmatter"
        );
        assert!(result[0].message.contains("missing"));
    }

    #[test]
    fn test_mkdocs_snippet_markers_not_flagged() {
        // Test for issue #68 - MkDocs snippet selection markers should not be flagged as undefined references
        let rule = MD052ReferenceLinkImages::new();

        // Test snippet section markers
        let content = r#"# Document with MkDocs Snippets

Some content here.

# -8<- [start:remote-content]

This is the remote content section.

# -8<- [end:remote-content]

More content here.

<!-- --8<-- [start:another-section] -->
Content in another section
<!-- --8<-- [end:another-section] -->"#;
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::MkDocs);
        let result = rule.check(&ctx).unwrap();

        // Should not flag any snippet markers as undefined references
        assert_eq!(
            result.len(),
            0,
            "Should not flag MkDocs snippet markers as undefined references"
        );

        // Test that the snippet marker lines are properly skipped
        // but regular undefined references on other lines are still caught
        let content = r#"# Document

# -8<- [start:section]
Content with [reference] inside snippet section
# -8<- [end:section]

Regular [undefined] reference outside snippet markers."#;
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::MkDocs);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(
            result.len(),
            2,
            "Should flag undefined references but skip snippet marker lines"
        );
        // The references inside the content should be flagged, but not start: and end:
        assert!(result[0].message.contains("reference"));
        assert!(result[1].message.contains("undefined"));

        // Test in standard mode - should flag the markers as undefined
        let content = r#"# Document

# -8<- [start:section]
# -8<- [end:section]"#;
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(
            result.len(),
            2,
            "In standard mode, snippet markers should be flagged as undefined references"
        );
    }

    #[test]
    fn test_github_alerts_not_flagged() {
        // Test for issue #60 - GitHub alerts should not be flagged as undefined references
        let rule = MD052ReferenceLinkImages::new();

        // Test various GitHub alert types
        let content = r#"# Document with GitHub Alerts

> [!NOTE]
> This is a note alert.

> [!TIP]
> This is a tip alert.

> [!IMPORTANT]
> This is an important alert.

> [!WARNING]
> This is a warning alert.

> [!CAUTION]
> This is a caution alert.

Regular content with [undefined] reference."#;
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();

        // Should only flag the undefined reference, not the GitHub alerts
        assert_eq!(
            result.len(),
            1,
            "Should only flag the undefined reference, not GitHub alerts"
        );
        assert!(result[0].message.contains("undefined"));
        assert_eq!(result[0].line, 18); // Line with [undefined]

        // Test GitHub alerts with additional content
        let content = r#"> [!TIP]
> Here's a useful tip about [something].
> Multiple lines are allowed.

[something] is mentioned but not defined."#;
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();

        // Should flag only the [something] outside blockquotes
        // The test shows we're only catching one, which might be correct behavior
        // matching markdownlint's approach
        assert_eq!(result.len(), 1, "Should flag undefined reference");
        assert!(result[0].message.contains("something"));

        // Test GitHub alerts with proper references
        let content = r#"> [!NOTE]
> See [reference] for more details.

[reference]: https://example.com"#;
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();

        // Should not flag anything - [!NOTE] is GitHub alert and [reference] is defined
        assert_eq!(result.len(), 0, "Should not flag GitHub alerts or defined references");
    }
}
