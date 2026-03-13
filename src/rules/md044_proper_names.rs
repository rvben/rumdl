use crate::utils::fast_hash;
use crate::utils::regex_cache::{escape_regex, get_cached_regex};

use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, Severity};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

mod md044_config;
pub use md044_config::MD044Config;

type WarningPosition = (usize, usize, String); // (line, column, found_name)

/// Rule MD044: Proper names should be capitalized
///
/// See [docs/md044.md](../../docs/md044.md) for full documentation, configuration, and examples.
///
/// This rule is triggered when proper names are not capitalized correctly in the document.
/// For example, if you have defined "JavaScript" as a proper name, the rule will flag any
/// occurrences of "javascript" or "Javascript" as violations.
///
/// ## Purpose
///
/// Ensuring consistent capitalization of proper names improves document quality and
/// professionalism. This is especially important for technical documentation where
/// product names, programming languages, and technologies often have specific
/// capitalization conventions.
///
/// ## Configuration Options
///
/// The rule supports the following configuration options:
///
/// ```yaml
/// MD044:
///   names: []                # List of proper names to check for correct capitalization
///   code-blocks: false       # Whether to check code blocks (default: false)
/// ```
///
/// Example configuration:
///
/// ```yaml
/// MD044:
///   names: ["JavaScript", "Node.js", "TypeScript"]
///   code-blocks: true
/// ```
///
/// ## Performance Optimizations
///
/// This rule implements several performance optimizations:
///
/// 1. **Regex Caching**: Pre-compiles and caches regex patterns for each proper name
/// 2. **Content Caching**: Caches results based on content hashing for repeated checks
/// 3. **Efficient Text Processing**: Uses optimized algorithms to avoid redundant text processing
/// 4. **Smart Code Block Detection**: Efficiently identifies and optionally excludes code blocks
///
/// ## Edge Cases Handled
///
/// - **Word Boundaries**: Only matches complete words, not substrings within other words
/// - **Case Sensitivity**: Properly handles case-specific matching
/// - **Code Blocks**: Optionally checks code blocks (controlled by code-blocks setting)
/// - **Markdown Formatting**: Handles proper names within Markdown formatting elements
///
/// ## Fix Behavior
///
/// When fixing issues, this rule replaces incorrect capitalization with the correct form
/// as defined in the configuration.
///
/// Check if a trimmed line is an inline config comment from a linting tool.
/// Recognized tools: rumdl, markdownlint, Vale, and remark-lint.
fn is_inline_config_comment(trimmed: &str) -> bool {
    trimmed.starts_with("<!-- rumdl-")
        || trimmed.starts_with("<!-- markdownlint-")
        || trimmed.starts_with("<!-- vale off")
        || trimmed.starts_with("<!-- vale on")
        || (trimmed.starts_with("<!-- vale ") && trimmed.contains(" = "))
        || trimmed.starts_with("<!-- vale style")
        || trimmed.starts_with("<!-- lint disable ")
        || trimmed.starts_with("<!-- lint enable ")
        || trimmed.starts_with("<!-- lint ignore ")
}

#[derive(Clone)]
pub struct MD044ProperNames {
    config: MD044Config,
    // Cache the combined regex pattern string
    combined_pattern: Option<String>,
    // Precomputed lowercase name variants for fast pre-checks
    name_variants: Vec<String>,
    // Cache for name violations by content hash
    content_cache: Arc<Mutex<HashMap<u64, Vec<WarningPosition>>>>,
}

impl MD044ProperNames {
    pub fn new(names: Vec<String>, code_blocks: bool) -> Self {
        let config = MD044Config {
            names,
            code_blocks,
            html_elements: true, // Default to checking HTML elements
            html_comments: true, // Default to checking HTML comments
        };
        let combined_pattern = Self::create_combined_pattern(&config);
        let name_variants = Self::build_name_variants(&config);
        Self {
            config,
            combined_pattern,
            name_variants,
            content_cache: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    // Helper function for consistent ASCII normalization
    fn ascii_normalize(s: &str) -> String {
        s.replace(['é', 'è', 'ê', 'ë'], "e")
            .replace(['à', 'á', 'â', 'ä', 'ã', 'å'], "a")
            .replace(['ï', 'î', 'í', 'ì'], "i")
            .replace(['ü', 'ú', 'ù', 'û'], "u")
            .replace(['ö', 'ó', 'ò', 'ô', 'õ'], "o")
            .replace('ñ', "n")
            .replace('ç', "c")
    }

    pub fn from_config_struct(config: MD044Config) -> Self {
        let combined_pattern = Self::create_combined_pattern(&config);
        let name_variants = Self::build_name_variants(&config);
        Self {
            config,
            combined_pattern,
            name_variants,
            content_cache: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    // Create a combined regex pattern for all proper names
    fn create_combined_pattern(config: &MD044Config) -> Option<String> {
        if config.names.is_empty() {
            return None;
        }

        // Create patterns for all names and their variations
        let mut patterns: Vec<String> = config
            .names
            .iter()
            .flat_map(|name| {
                let mut variations = vec![];
                let lower_name = name.to_lowercase();

                // Add the lowercase version
                variations.push(escape_regex(&lower_name));

                // Add version without dots
                let lower_name_no_dots = lower_name.replace('.', "");
                if lower_name != lower_name_no_dots {
                    variations.push(escape_regex(&lower_name_no_dots));
                }

                // Add ASCII-normalized versions for common accented characters
                let ascii_normalized = Self::ascii_normalize(&lower_name);

                if ascii_normalized != lower_name {
                    variations.push(escape_regex(&ascii_normalized));

                    // Also add version without dots
                    let ascii_no_dots = ascii_normalized.replace('.', "");
                    if ascii_normalized != ascii_no_dots {
                        variations.push(escape_regex(&ascii_no_dots));
                    }
                }

                variations
            })
            .collect();

        // Sort patterns by length (longest first) to avoid shorter patterns matching within longer ones
        patterns.sort_by_key(|b| std::cmp::Reverse(b.len()));

        // Combine all patterns into a single regex with capture groups
        // Don't use \b as it doesn't work with Unicode - we'll check boundaries manually
        Some(format!(r"(?i)({})", patterns.join("|")))
    }

    fn build_name_variants(config: &MD044Config) -> Vec<String> {
        let mut variants = HashSet::new();
        for name in &config.names {
            let lower_name = name.to_lowercase();
            variants.insert(lower_name.clone());

            let lower_no_dots = lower_name.replace('.', "");
            if lower_name != lower_no_dots {
                variants.insert(lower_no_dots);
            }

            let ascii_normalized = Self::ascii_normalize(&lower_name);
            if ascii_normalized != lower_name {
                variants.insert(ascii_normalized.clone());

                let ascii_no_dots = ascii_normalized.replace('.', "");
                if ascii_normalized != ascii_no_dots {
                    variants.insert(ascii_no_dots);
                }
            }
        }

        variants.into_iter().collect()
    }

    // Find all name violations in the content and return positions.
    // `content_lower` is the pre-computed lowercase version of `content` to avoid redundant allocations.
    fn find_name_violations(
        &self,
        content: &str,
        ctx: &crate::lint_context::LintContext,
        content_lower: &str,
    ) -> Vec<WarningPosition> {
        // Early return: if no names configured or content is empty
        if self.config.names.is_empty() || content.is_empty() || self.combined_pattern.is_none() {
            return Vec::new();
        }

        // Early return: quick check if any of the configured names might be in content
        let has_potential_matches = self.name_variants.iter().any(|name| content_lower.contains(name));

        if !has_potential_matches {
            return Vec::new();
        }

        // Check if we have cached results
        let hash = fast_hash(content);
        {
            // Use a separate scope for borrowing to minimize lock time
            if let Ok(cache) = self.content_cache.lock()
                && let Some(cached) = cache.get(&hash)
            {
                return cached.clone();
            }
        }

        let mut violations = Vec::new();

        // Get the regex from global cache
        let combined_regex = match &self.combined_pattern {
            Some(pattern) => match get_cached_regex(pattern) {
                Ok(regex) => regex,
                Err(_) => return Vec::new(),
            },
            None => return Vec::new(),
        };

        // Use ctx.lines for better performance
        for (line_idx, line_info) in ctx.lines.iter().enumerate() {
            let line_num = line_idx + 1;
            let line = line_info.content(ctx.content);

            // Skip code fence lines (```language or ~~~language)
            let trimmed = line.trim_start();
            if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
                continue;
            }

            // Skip if in code block (when code_blocks = false)
            if !self.config.code_blocks && line_info.in_code_block {
                continue;
            }

            // Skip if in HTML block (when html_elements = false)
            if !self.config.html_elements && line_info.in_html_block {
                continue;
            }

            // Skip HTML comments using pre-computed line flag
            if !self.config.html_comments && line_info.in_html_comment {
                continue;
            }

            // Skip JSX expressions and MDX comments (MDX flavor)
            if line_info.in_jsx_expression || line_info.in_mdx_comment {
                continue;
            }

            // Skip Obsidian comments (Obsidian flavor)
            if line_info.in_obsidian_comment {
                continue;
            }

            // For frontmatter lines, determine offset where checkable value content starts.
            // YAML keys should not be checked against proper names - only values.
            let fm_value_offset = if line_info.in_front_matter {
                Self::frontmatter_value_offset(line)
            } else {
                0
            };
            if fm_value_offset == usize::MAX {
                continue;
            }

            // Skip inline config comments (rumdl, markdownlint, Vale, remark-lint directives)
            if is_inline_config_comment(trimmed) {
                continue;
            }

            // Early return: skip lines that don't contain any potential matches
            let line_lower = line.to_lowercase();
            let has_line_matches = self.name_variants.iter().any(|name| line_lower.contains(name));

            if !has_line_matches {
                continue;
            }

            // Use the combined regex to find all matches in one pass
            for cap in combined_regex.find_iter(line) {
                let found_name = &line[cap.start()..cap.end()];

                // Check word boundaries manually for Unicode support
                let start_pos = cap.start();
                let end_pos = cap.end();

                // Skip matches in the key portion of frontmatter lines
                if start_pos < fm_value_offset {
                    continue;
                }

                // Skip matches inside HTML tag attributes (handles multi-line tags)
                let byte_pos = line_info.byte_offset + start_pos;
                if ctx.is_in_html_tag(byte_pos) {
                    continue;
                }

                if !Self::is_at_word_boundary(line, start_pos, true) || !Self::is_at_word_boundary(line, end_pos, false)
                {
                    continue; // Not at word boundary
                }

                // Skip if in inline code when code_blocks is false
                if !self.config.code_blocks {
                    if ctx.is_in_code_block_or_span(byte_pos) {
                        continue;
                    }
                    // pulldown-cmark doesn't parse markdown syntax inside HTML
                    // comments, HTML blocks, or frontmatter, so backtick-wrapped
                    // text isn't detected by is_in_code_block_or_span. Check directly.
                    if (line_info.in_html_comment || line_info.in_html_block || line_info.in_front_matter)
                        && Self::is_in_backtick_code_in_line(line, start_pos)
                    {
                        continue;
                    }
                }

                // Skip if in link URL or reference definition
                if Self::is_in_link(ctx, byte_pos) {
                    continue;
                }

                // Skip if inside an angle-bracket URL (e.g., <https://...>)
                // The link parser skips autolinks inside HTML comments,
                // so we detect them directly in the line text.
                if Self::is_in_angle_bracket_url(line, start_pos) {
                    continue;
                }

                // Find which proper name this matches
                if let Some(proper_name) = self.get_proper_name_for(found_name) {
                    // Only flag if it's not already correct
                    if found_name != proper_name {
                        violations.push((line_num, cap.start() + 1, found_name.to_string()));
                    }
                }
            }
        }

        // Store in cache (ignore if mutex is poisoned)
        if let Ok(mut cache) = self.content_cache.lock() {
            cache.insert(hash, violations.clone());
        }
        violations
    }

    /// Check if a byte position is within a link URL (not link text)
    ///
    /// Link text should be checked for proper names, but URLs should be skipped.
    /// For `[text](url)` - check text, skip url
    /// For `[text][ref]` - check text, skip reference portion
    /// For `[[text]]` (WikiLinks) - check text, skip brackets
    fn is_in_link(ctx: &crate::lint_context::LintContext, byte_pos: usize) -> bool {
        use pulldown_cmark::LinkType;

        // Binary search links (sorted by byte_offset) to find candidate containing byte_pos
        let link_idx = ctx.links.partition_point(|link| link.byte_offset <= byte_pos);
        if link_idx > 0 {
            let link = &ctx.links[link_idx - 1];
            if byte_pos < link.byte_end {
                // WikiLinks [[text]] start with '[[', regular links [text] start with '['
                let text_start = if matches!(link.link_type, LinkType::WikiLink { .. }) {
                    link.byte_offset + 2
                } else {
                    link.byte_offset + 1
                };
                let text_end = text_start + link.text.len();

                // If position is within the text portion, skip only if text is a URL
                if byte_pos >= text_start && byte_pos < text_end {
                    return Self::link_text_is_url(&link.text);
                }
                // Position is in the URL/reference portion, skip it
                return true;
            }
        }

        // Binary search images (sorted by byte_offset) to find candidate containing byte_pos
        let image_idx = ctx.images.partition_point(|img| img.byte_offset <= byte_pos);
        if image_idx > 0 {
            let image = &ctx.images[image_idx - 1];
            if byte_pos < image.byte_end {
                // Image starts with '![' so alt text starts at byte_offset + 2
                let alt_start = image.byte_offset + 2;
                let alt_end = alt_start + image.alt_text.len();

                // If position is within the alt text portion, don't skip
                if byte_pos >= alt_start && byte_pos < alt_end {
                    return false;
                }
                // Position is in the URL/reference portion, skip it
                return true;
            }
        }

        // Check pre-computed reference definitions
        ctx.is_in_reference_def(byte_pos)
    }

    /// Check if link text is a URL that should not have proper name corrections.
    /// Matches markdownlint behavior: skip text starting with `http://`, `https://`, or `www.`.
    fn link_text_is_url(text: &str) -> bool {
        let lower = text.trim().to_ascii_lowercase();
        lower.starts_with("http://") || lower.starts_with("https://") || lower.starts_with("www.")
    }

    /// Check if a position within a line falls inside an angle-bracket URL (`<scheme://...>`).
    ///
    /// The link parser skips autolinks inside HTML comments, so `ctx.links` won't
    /// contain them. This function detects angle-bracket URLs directly in the line
    /// text, covering both HTML comments and regular text as a safety net.
    fn is_in_angle_bracket_url(line: &str, pos: usize) -> bool {
        let bytes = line.as_bytes();
        let len = bytes.len();
        let mut i = 0;
        while i < len {
            if bytes[i] == b'<' {
                let after_open = i + 1;
                // Check for a valid URI scheme per CommonMark autolink spec:
                // scheme = [a-zA-Z][a-zA-Z0-9+.-]{0,31}
                // followed by ':'
                if after_open < len && bytes[after_open].is_ascii_alphabetic() {
                    let mut s = after_open + 1;
                    let scheme_max = (after_open + 32).min(len);
                    while s < scheme_max
                        && (bytes[s].is_ascii_alphanumeric()
                            || bytes[s] == b'+'
                            || bytes[s] == b'-'
                            || bytes[s] == b'.')
                    {
                        s += 1;
                    }
                    if s < len && bytes[s] == b':' {
                        // Valid scheme found; scan for closing '>' with no spaces or '<'
                        let mut j = s + 1;
                        let mut found_close = false;
                        while j < len {
                            match bytes[j] {
                                b'>' => {
                                    found_close = true;
                                    break;
                                }
                                b' ' | b'<' => break,
                                _ => j += 1,
                            }
                        }
                        if found_close && pos >= i && pos <= j {
                            return true;
                        }
                        if found_close {
                            i = j + 1;
                            continue;
                        }
                    }
                }
            }
            i += 1;
        }
        false
    }

    /// Check if a position within a line falls inside backtick-delimited code.
    ///
    /// pulldown-cmark does not parse markdown syntax inside HTML comments, so
    /// `ctx.is_in_code_block_or_span` returns false for backtick-wrapped text
    /// within comments. This function detects backtick code spans directly in
    /// the line text following CommonMark rules: a code span starts with N
    /// backticks and ends with exactly N backticks.
    fn is_in_backtick_code_in_line(line: &str, pos: usize) -> bool {
        let bytes = line.as_bytes();
        let len = bytes.len();
        let mut i = 0;
        while i < len {
            if bytes[i] == b'`' {
                // Count the opening backtick sequence length
                let open_start = i;
                while i < len && bytes[i] == b'`' {
                    i += 1;
                }
                let tick_len = i - open_start;

                // Scan forward for a closing sequence of exactly tick_len backticks
                while i < len {
                    if bytes[i] == b'`' {
                        let close_start = i;
                        while i < len && bytes[i] == b'`' {
                            i += 1;
                        }
                        if i - close_start == tick_len {
                            // Matched pair found; the code span content is between
                            // the end of the opening backticks and the start of the
                            // closing backticks (exclusive of the backticks themselves).
                            let content_start = open_start + tick_len;
                            let content_end = close_start;
                            if pos >= content_start && pos < content_end {
                                return true;
                            }
                            // Continue scanning after this pair
                            break;
                        }
                        // Not the right length; keep scanning
                    } else {
                        i += 1;
                    }
                }
            } else {
                i += 1;
            }
        }
        false
    }

    // Check if a character is a word boundary (handles Unicode)
    fn is_word_boundary_char(c: char) -> bool {
        !c.is_alphanumeric()
    }

    // Check if position is at a word boundary using byte-level lookups.
    fn is_at_word_boundary(content: &str, pos: usize, is_start: bool) -> bool {
        if is_start {
            if pos == 0 {
                return true;
            }
            match content[..pos].chars().next_back() {
                None => true,
                Some(c) => Self::is_word_boundary_char(c),
            }
        } else {
            if pos >= content.len() {
                return true;
            }
            match content[pos..].chars().next() {
                None => true,
                Some(c) => Self::is_word_boundary_char(c),
            }
        }
    }

    /// For a frontmatter line, return the byte offset where the checkable
    /// value portion starts. Returns `usize::MAX` if the entire line should be
    /// skipped (frontmatter delimiters, key-only lines, YAML comments, flow constructs).
    fn frontmatter_value_offset(line: &str) -> usize {
        let trimmed = line.trim();

        // Skip frontmatter delimiters and empty lines
        if trimmed == "---" || trimmed == "+++" || trimmed.is_empty() {
            return usize::MAX;
        }

        // Skip YAML comments
        if trimmed.starts_with('#') {
            return usize::MAX;
        }

        // YAML list item: "  - item" or "  - key: value"
        let stripped = line.trim_start();
        if let Some(after_dash) = stripped.strip_prefix("- ") {
            let leading = line.len() - stripped.len();
            // Check if the list item contains a mapping (e.g., "- key: value")
            if let Some(result) = Self::kv_value_offset(line, after_dash, leading + 2) {
                return result;
            }
            // Bare list item value (no colon) - check content after "- "
            return leading + 2;
        }
        if stripped == "-" {
            return usize::MAX;
        }

        // Key-value pair with colon separator (YAML): "key: value"
        if let Some(result) = Self::kv_value_offset(line, stripped, line.len() - stripped.len()) {
            return result;
        }

        // Key-value pair with equals separator (TOML): "key = value"
        if let Some(eq_pos) = line.find('=') {
            let after_eq = eq_pos + 1;
            if after_eq < line.len() && line.as_bytes()[after_eq] == b' ' {
                let value_start = after_eq + 1;
                let value_slice = &line[value_start..];
                let value_trimmed = value_slice.trim();
                if value_trimmed.is_empty() {
                    return usize::MAX;
                }
                // For quoted values, skip the opening quote character
                if (value_trimmed.starts_with('"') && value_trimmed.ends_with('"'))
                    || (value_trimmed.starts_with('\'') && value_trimmed.ends_with('\''))
                {
                    let quote_offset = value_slice.find(['"', '\'']).unwrap_or(0);
                    return value_start + quote_offset + 1;
                }
                return value_start;
            }
            // Equals with no space after or at end of line -> no value to check
            return usize::MAX;
        }

        // No separator found - continuation line or bare value, check the whole line
        0
    }

    /// Parse a key-value pair using colon separator within `content` that starts
    /// at `base_offset` in the original line. Returns `Some(offset)` if a colon
    /// separator is found, `None` if no colon is present.
    fn kv_value_offset(line: &str, content: &str, base_offset: usize) -> Option<usize> {
        let colon_pos = content.find(':')?;
        let abs_colon = base_offset + colon_pos;
        let after_colon = abs_colon + 1;
        if after_colon < line.len() && line.as_bytes()[after_colon] == b' ' {
            let value_start = after_colon + 1;
            let value_slice = &line[value_start..];
            let value_trimmed = value_slice.trim();
            if value_trimmed.is_empty() {
                return Some(usize::MAX);
            }
            // Skip flow mappings and flow sequences - too complex for heuristic parsing
            if value_trimmed.starts_with('{') || value_trimmed.starts_with('[') {
                return Some(usize::MAX);
            }
            // For quoted values, skip the opening quote character
            if (value_trimmed.starts_with('"') && value_trimmed.ends_with('"'))
                || (value_trimmed.starts_with('\'') && value_trimmed.ends_with('\''))
            {
                let quote_offset = value_slice.find(['"', '\'']).unwrap_or(0);
                return Some(value_start + quote_offset + 1);
            }
            return Some(value_start);
        }
        // Colon with no space after or at end of line -> no value to check
        Some(usize::MAX)
    }

    // Get the proper name that should be used for a found name
    fn get_proper_name_for(&self, found_name: &str) -> Option<String> {
        let found_lower = found_name.to_lowercase();

        // Iterate through the configured proper names
        for name in &self.config.names {
            let lower_name = name.to_lowercase();
            let lower_name_no_dots = lower_name.replace('.', "");

            // Direct match
            if found_lower == lower_name || found_lower == lower_name_no_dots {
                return Some(name.clone());
            }

            // Check ASCII-normalized version
            let ascii_normalized = Self::ascii_normalize(&lower_name);

            let ascii_no_dots = ascii_normalized.replace('.', "");

            if found_lower == ascii_normalized || found_lower == ascii_no_dots {
                return Some(name.clone());
            }
        }
        None
    }
}

impl Rule for MD044ProperNames {
    fn name(&self) -> &'static str {
        "MD044"
    }

    fn description(&self) -> &'static str {
        "Proper names should have the correct capitalization"
    }

    fn should_skip(&self, ctx: &crate::lint_context::LintContext) -> bool {
        if self.config.names.is_empty() {
            return true;
        }
        // Quick check if any configured name variants exist (case-insensitive)
        let content_lower = if ctx.content.is_ascii() {
            ctx.content.to_ascii_lowercase()
        } else {
            ctx.content.to_lowercase()
        };
        !self.name_variants.iter().any(|name| content_lower.contains(name))
    }

    fn check(&self, ctx: &crate::lint_context::LintContext) -> LintResult {
        let content = ctx.content;
        if content.is_empty() || self.config.names.is_empty() || self.combined_pattern.is_none() {
            return Ok(Vec::new());
        }

        // Compute lowercase content once and reuse across all checks
        let content_lower = if content.is_ascii() {
            content.to_ascii_lowercase()
        } else {
            content.to_lowercase()
        };

        // Early return: use pre-computed name_variants for the quick check
        let has_potential_matches = self.name_variants.iter().any(|name| content_lower.contains(name));

        if !has_potential_matches {
            return Ok(Vec::new());
        }

        let line_index = &ctx.line_index;
        let violations = self.find_name_violations(content, ctx, &content_lower);

        let warnings = violations
            .into_iter()
            .filter_map(|(line, column, found_name)| {
                self.get_proper_name_for(&found_name).map(|proper_name| LintWarning {
                    rule_name: Some(self.name().to_string()),
                    line,
                    column,
                    end_line: line,
                    end_column: column + found_name.len(),
                    message: format!("Proper name '{found_name}' should be '{proper_name}'"),
                    severity: Severity::Warning,
                    fix: Some(Fix {
                        range: line_index.line_col_to_byte_range_with_length(line, column, found_name.len()),
                        replacement: proper_name,
                    }),
                })
            })
            .collect();

        Ok(warnings)
    }

    fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, LintError> {
        let content = ctx.content;
        if content.is_empty() || self.config.names.is_empty() {
            return Ok(content.to_string());
        }

        let content_lower = if content.is_ascii() {
            content.to_ascii_lowercase()
        } else {
            content.to_lowercase()
        };
        let violations = self.find_name_violations(content, ctx, &content_lower);
        if violations.is_empty() {
            return Ok(content.to_string());
        }

        // Process lines and build the fixed content
        let mut fixed_lines = Vec::new();

        // Group violations by line
        let mut violations_by_line: HashMap<usize, Vec<(usize, String)>> = HashMap::new();
        for (line_num, col_num, found_name) in violations {
            violations_by_line
                .entry(line_num)
                .or_default()
                .push((col_num, found_name));
        }

        // Sort violations within each line in reverse order
        for violations in violations_by_line.values_mut() {
            violations.sort_by_key(|b| std::cmp::Reverse(b.0));
        }

        // Process each line
        for (line_idx, line_info) in ctx.lines.iter().enumerate() {
            let line_num = line_idx + 1;

            // Skip lines where this rule is disabled by inline config
            if ctx.inline_config().is_rule_disabled(self.name(), line_num) {
                fixed_lines.push(line_info.content(ctx.content).to_string());
                continue;
            }

            if let Some(line_violations) = violations_by_line.get(&line_num) {
                // This line has violations, fix them
                let mut fixed_line = line_info.content(ctx.content).to_string();

                for (col_num, found_name) in line_violations {
                    if let Some(proper_name) = self.get_proper_name_for(found_name) {
                        let start_col = col_num - 1; // Convert to 0-based
                        let end_col = start_col + found_name.len();

                        if end_col <= fixed_line.len()
                            && fixed_line.is_char_boundary(start_col)
                            && fixed_line.is_char_boundary(end_col)
                        {
                            fixed_line.replace_range(start_col..end_col, &proper_name);
                        }
                    }
                }

                fixed_lines.push(fixed_line);
            } else {
                // No violations on this line, keep it as is
                fixed_lines.push(line_info.content(ctx.content).to_string());
            }
        }

        // Join lines with newlines, preserving the original ending
        let mut result = fixed_lines.join("\n");
        if content.ends_with('\n') && !result.ends_with('\n') {
            result.push('\n');
        }
        Ok(result)
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn default_config_section(&self) -> Option<(String, toml::Value)> {
        let json_value = serde_json::to_value(&self.config).ok()?;
        Some((
            self.name().to_string(),
            crate::rule_config_serde::json_to_toml_value(&json_value)?,
        ))
    }

    fn from_config(config: &crate::config::Config) -> Box<dyn Rule>
    where
        Self: Sized,
    {
        let rule_config = crate::rule_config_serde::load_rule_config::<MD044Config>(config);
        Box::new(Self::from_config_struct(rule_config))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lint_context::LintContext;

    fn create_context(content: &str) -> LintContext<'_> {
        LintContext::new(content, crate::config::MarkdownFlavor::Standard, None)
    }

    #[test]
    fn test_correctly_capitalized_names() {
        let rule = MD044ProperNames::new(
            vec![
                "JavaScript".to_string(),
                "TypeScript".to_string(),
                "Node.js".to_string(),
            ],
            true,
        );

        let content = "This document uses JavaScript, TypeScript, and Node.js correctly.";
        let ctx = create_context(content);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Should not flag correctly capitalized names");
    }

    #[test]
    fn test_incorrectly_capitalized_names() {
        let rule = MD044ProperNames::new(vec!["JavaScript".to_string(), "TypeScript".to_string()], true);

        let content = "This document uses javascript and typescript incorrectly.";
        let ctx = create_context(content);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 2, "Should flag two incorrect capitalizations");
        assert_eq!(result[0].message, "Proper name 'javascript' should be 'JavaScript'");
        assert_eq!(result[0].line, 1);
        assert_eq!(result[0].column, 20);
        assert_eq!(result[1].message, "Proper name 'typescript' should be 'TypeScript'");
        assert_eq!(result[1].line, 1);
        assert_eq!(result[1].column, 35);
    }

    #[test]
    fn test_names_at_beginning_of_sentences() {
        let rule = MD044ProperNames::new(vec!["JavaScript".to_string(), "Python".to_string()], true);

        let content = "javascript is a great language. python is also popular.";
        let ctx = create_context(content);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 2, "Should flag names at beginning of sentences");
        assert_eq!(result[0].line, 1);
        assert_eq!(result[0].column, 1);
        assert_eq!(result[1].line, 1);
        assert_eq!(result[1].column, 33);
    }

    #[test]
    fn test_names_in_code_blocks_checked_by_default() {
        let rule = MD044ProperNames::new(vec!["JavaScript".to_string()], true);

        let content = r#"Here is some text with JavaScript.

```javascript
// This javascript should be checked
const lang = "javascript";
```

But this javascript should be flagged."#;

        let ctx = create_context(content);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 3, "Should flag javascript inside and outside code blocks");
        assert_eq!(result[0].line, 4);
        assert_eq!(result[1].line, 5);
        assert_eq!(result[2].line, 8);
    }

    #[test]
    fn test_names_in_code_blocks_ignored_when_disabled() {
        let rule = MD044ProperNames::new(
            vec!["JavaScript".to_string()],
            false, // code_blocks = false means skip code blocks
        );

        let content = r#"```
javascript in code block
```"#;

        let ctx = create_context(content);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(
            result.len(),
            0,
            "Should not flag javascript in code blocks when code_blocks is false"
        );
    }

    #[test]
    fn test_names_in_inline_code_checked_by_default() {
        let rule = MD044ProperNames::new(vec!["JavaScript".to_string()], true);

        let content = "This is `javascript` in inline code and javascript outside.";
        let ctx = create_context(content);
        let result = rule.check(&ctx).unwrap();

        // When code_blocks=true, inline code should be checked
        assert_eq!(result.len(), 2, "Should flag javascript inside and outside inline code");
        assert_eq!(result[0].column, 10); // javascript in inline code
        assert_eq!(result[1].column, 41); // javascript outside
    }

    #[test]
    fn test_multiple_names_in_same_line() {
        let rule = MD044ProperNames::new(
            vec!["JavaScript".to_string(), "TypeScript".to_string(), "React".to_string()],
            true,
        );

        let content = "I use javascript, typescript, and react in my projects.";
        let ctx = create_context(content);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 3, "Should flag all three incorrect names");
        assert_eq!(result[0].message, "Proper name 'javascript' should be 'JavaScript'");
        assert_eq!(result[1].message, "Proper name 'typescript' should be 'TypeScript'");
        assert_eq!(result[2].message, "Proper name 'react' should be 'React'");
    }

    #[test]
    fn test_case_sensitivity() {
        let rule = MD044ProperNames::new(vec!["JavaScript".to_string()], true);

        let content = "JAVASCRIPT, Javascript, javascript, and JavaScript variations.";
        let ctx = create_context(content);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 3, "Should flag all incorrect case variations");
        // JavaScript (correct) should not be flagged
        assert!(result.iter().all(|w| w.message.contains("should be 'JavaScript'")));
    }

    #[test]
    fn test_configuration_with_custom_name_list() {
        let config = MD044Config {
            names: vec!["GitHub".to_string(), "GitLab".to_string(), "DevOps".to_string()],
            code_blocks: true,
            html_elements: true,
            html_comments: true,
        };
        let rule = MD044ProperNames::from_config_struct(config);

        let content = "We use github, gitlab, and devops for our workflow.";
        let ctx = create_context(content);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 3, "Should flag all custom names");
        assert_eq!(result[0].message, "Proper name 'github' should be 'GitHub'");
        assert_eq!(result[1].message, "Proper name 'gitlab' should be 'GitLab'");
        assert_eq!(result[2].message, "Proper name 'devops' should be 'DevOps'");
    }

    #[test]
    fn test_empty_configuration() {
        let rule = MD044ProperNames::new(vec![], true);

        let content = "This has javascript and typescript but no configured names.";
        let ctx = create_context(content);
        let result = rule.check(&ctx).unwrap();

        assert!(result.is_empty(), "Should not flag anything with empty configuration");
    }

    #[test]
    fn test_names_with_special_characters() {
        let rule = MD044ProperNames::new(
            vec!["Node.js".to_string(), "ASP.NET".to_string(), "C++".to_string()],
            true,
        );

        let content = "We use nodejs, asp.net, ASP.NET, and c++ in our stack.";
        let ctx = create_context(content);
        let result = rule.check(&ctx).unwrap();

        // nodejs should match Node.js (dotless variation)
        // asp.net should be flagged (wrong case)
        // ASP.NET should not be flagged (correct)
        // c++ should be flagged
        assert_eq!(result.len(), 3, "Should handle special characters correctly");

        let messages: Vec<&str> = result.iter().map(|w| w.message.as_str()).collect();
        assert!(messages.contains(&"Proper name 'nodejs' should be 'Node.js'"));
        assert!(messages.contains(&"Proper name 'asp.net' should be 'ASP.NET'"));
        assert!(messages.contains(&"Proper name 'c++' should be 'C++'"));
    }

    #[test]
    fn test_word_boundaries() {
        let rule = MD044ProperNames::new(vec!["Java".to_string(), "Script".to_string()], true);

        let content = "JavaScript is not java or script, but Java and Script are separate.";
        let ctx = create_context(content);
        let result = rule.check(&ctx).unwrap();

        // Should only flag lowercase "java" and "script" as separate words
        assert_eq!(result.len(), 2, "Should respect word boundaries");
        assert!(result.iter().any(|w| w.column == 19)); // "java" position
        assert!(result.iter().any(|w| w.column == 27)); // "script" position
    }

    #[test]
    fn test_fix_method() {
        let rule = MD044ProperNames::new(
            vec![
                "JavaScript".to_string(),
                "TypeScript".to_string(),
                "Node.js".to_string(),
            ],
            true,
        );

        let content = "I love javascript, typescript, and nodejs!";
        let ctx = create_context(content);
        let fixed = rule.fix(&ctx).unwrap();

        assert_eq!(fixed, "I love JavaScript, TypeScript, and Node.js!");
    }

    #[test]
    fn test_fix_multiple_occurrences() {
        let rule = MD044ProperNames::new(vec!["Python".to_string()], true);

        let content = "python is great. I use python daily. PYTHON is powerful.";
        let ctx = create_context(content);
        let fixed = rule.fix(&ctx).unwrap();

        assert_eq!(fixed, "Python is great. I use Python daily. Python is powerful.");
    }

    #[test]
    fn test_fix_checks_code_blocks_by_default() {
        let rule = MD044ProperNames::new(vec!["JavaScript".to_string()], true);

        let content = r#"I love javascript.

```
const lang = "javascript";
```

More javascript here."#;

        let ctx = create_context(content);
        let fixed = rule.fix(&ctx).unwrap();

        let expected = r#"I love JavaScript.

```
const lang = "JavaScript";
```

More JavaScript here."#;

        assert_eq!(fixed, expected);
    }

    #[test]
    fn test_multiline_content() {
        let rule = MD044ProperNames::new(vec!["Rust".to_string(), "Python".to_string()], true);

        let content = r#"First line with rust.
Second line with python.
Third line with RUST and PYTHON."#;

        let ctx = create_context(content);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 4, "Should flag all incorrect occurrences");
        assert_eq!(result[0].line, 1);
        assert_eq!(result[1].line, 2);
        assert_eq!(result[2].line, 3);
        assert_eq!(result[3].line, 3);
    }

    #[test]
    fn test_default_config() {
        let config = MD044Config::default();
        assert!(config.names.is_empty());
        assert!(!config.code_blocks);
        assert!(config.html_elements);
        assert!(config.html_comments);
    }

    #[test]
    fn test_default_config_checks_html_comments() {
        let config = MD044Config {
            names: vec!["JavaScript".to_string()],
            ..MD044Config::default()
        };
        let rule = MD044ProperNames::from_config_struct(config);

        let content = "# Guide\n\n<!-- javascript mentioned here -->\n";
        let ctx = create_context(content);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 1, "Default config should check HTML comments");
        assert_eq!(result[0].line, 3);
    }

    #[test]
    fn test_default_config_skips_code_blocks() {
        let config = MD044Config {
            names: vec!["JavaScript".to_string()],
            ..MD044Config::default()
        };
        let rule = MD044ProperNames::from_config_struct(config);

        let content = "# Guide\n\n```\njavascript in code\n```\n";
        let ctx = create_context(content);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 0, "Default config should skip code blocks");
    }

    #[test]
    fn test_standalone_html_comment_checked() {
        let config = MD044Config {
            names: vec!["Test".to_string()],
            ..MD044Config::default()
        };
        let rule = MD044ProperNames::from_config_struct(config);

        let content = "# Heading\n\n<!-- this is a test example -->\n";
        let ctx = create_context(content);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 1, "Should flag proper name in standalone HTML comment");
        assert_eq!(result[0].line, 3);
    }

    #[test]
    fn test_inline_config_comments_not_flagged() {
        let config = MD044Config {
            names: vec!["RUMDL".to_string()],
            ..MD044Config::default()
        };
        let rule = MD044ProperNames::from_config_struct(config);

        // Lines 1, 3, 4, 6 are inline config comments — should not be flagged.
        // Lines 2, 5 contain "rumdl" in regular text — flagged by rule.check(),
        // but would be suppressed by the linting engine's inline config filtering.
        let content = "<!-- rumdl-disable MD044 -->\nSome rumdl text here.\n<!-- rumdl-enable MD044 -->\n<!-- markdownlint-disable -->\nMore rumdl text.\n<!-- markdownlint-enable -->\n";
        let ctx = create_context(content);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 2, "Should only flag body lines, not config comments");
        assert_eq!(result[0].line, 2);
        assert_eq!(result[1].line, 5);
    }

    #[test]
    fn test_html_comment_skipped_when_disabled() {
        let config = MD044Config {
            names: vec!["Test".to_string()],
            code_blocks: true,
            html_elements: true,
            html_comments: false,
        };
        let rule = MD044ProperNames::from_config_struct(config);

        let content = "# Heading\n\n<!-- this is a test example -->\n\nRegular test here.\n";
        let ctx = create_context(content);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(
            result.len(),
            1,
            "Should only flag 'test' outside HTML comment when html_comments=false"
        );
        assert_eq!(result[0].line, 5);
    }

    #[test]
    fn test_fix_corrects_html_comment_content() {
        let config = MD044Config {
            names: vec!["JavaScript".to_string()],
            ..MD044Config::default()
        };
        let rule = MD044ProperNames::from_config_struct(config);

        let content = "# Guide\n\n<!-- javascript mentioned here -->\n";
        let ctx = create_context(content);
        let fixed = rule.fix(&ctx).unwrap();

        assert_eq!(fixed, "# Guide\n\n<!-- JavaScript mentioned here -->\n");
    }

    #[test]
    fn test_fix_does_not_modify_inline_config_comments() {
        let config = MD044Config {
            names: vec!["RUMDL".to_string()],
            ..MD044Config::default()
        };
        let rule = MD044ProperNames::from_config_struct(config);

        let content = "<!-- rumdl-disable -->\nSome rumdl text.\n<!-- rumdl-enable -->\n";
        let ctx = create_context(content);
        let fixed = rule.fix(&ctx).unwrap();

        // Config comments should be untouched
        assert!(fixed.contains("<!-- rumdl-disable -->"));
        assert!(fixed.contains("<!-- rumdl-enable -->"));
        // Body text inside disable block should NOT be fixed (rule is disabled)
        assert!(
            fixed.contains("Some rumdl text."),
            "Line inside rumdl-disable block should not be modified by fix()"
        );
    }

    #[test]
    fn test_fix_respects_inline_disable_partial() {
        let config = MD044Config {
            names: vec!["RUMDL".to_string()],
            ..MD044Config::default()
        };
        let rule = MD044ProperNames::from_config_struct(config);

        let content =
            "<!-- rumdl-disable MD044 -->\nSome rumdl text.\n<!-- rumdl-enable MD044 -->\n\nSome rumdl text outside.\n";
        let ctx = create_context(content);
        let fixed = rule.fix(&ctx).unwrap();

        // Line inside disable block should be preserved
        assert!(
            fixed.contains("Some rumdl text.\n<!-- rumdl-enable"),
            "Line inside disable block should not be modified"
        );
        // Line outside disable block should be fixed
        assert!(
            fixed.contains("Some RUMDL text outside."),
            "Line outside disable block should be fixed"
        );
    }

    #[test]
    fn test_performance_with_many_names() {
        let mut names = vec![];
        for i in 0..50 {
            names.push(format!("ProperName{i}"));
        }

        let rule = MD044ProperNames::new(names, true);

        let content = "This has propername0, propername25, and propername49 incorrectly.";
        let ctx = create_context(content);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 3, "Should handle many configured names efficiently");
    }

    #[test]
    fn test_large_name_count_performance() {
        // Verify MD044 can handle large numbers of names without regex limitations
        // This test confirms that fancy-regex handles large patterns well
        let names = (0..1000).map(|i| format!("ProperName{i}")).collect::<Vec<_>>();

        let rule = MD044ProperNames::new(names, true);

        // The combined pattern should be created successfully
        assert!(rule.combined_pattern.is_some());

        // Should be able to check content without errors
        let content = "This has propername0 and propername999 in it.";
        let ctx = create_context(content);
        let result = rule.check(&ctx).unwrap();

        // Should detect both incorrect names
        assert_eq!(result.len(), 2, "Should handle 1000 names without issues");
    }

    #[test]
    fn test_cache_behavior() {
        let rule = MD044ProperNames::new(vec!["JavaScript".to_string()], true);

        let content = "Using javascript here.";
        let ctx = create_context(content);

        // First check
        let result1 = rule.check(&ctx).unwrap();
        assert_eq!(result1.len(), 1);

        // Second check should use cache
        let result2 = rule.check(&ctx).unwrap();
        assert_eq!(result2.len(), 1);

        // Results should be identical
        assert_eq!(result1[0].line, result2[0].line);
        assert_eq!(result1[0].column, result2[0].column);
    }

    #[test]
    fn test_html_comments_not_checked_when_disabled() {
        let config = MD044Config {
            names: vec!["JavaScript".to_string()],
            code_blocks: true,    // Check code blocks
            html_elements: true,  // Check HTML elements
            html_comments: false, // Don't check HTML comments
        };
        let rule = MD044ProperNames::from_config_struct(config);

        let content = r#"Regular javascript here.
<!-- This javascript in HTML comment should be ignored -->
More javascript outside."#;

        let ctx = create_context(content);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 2, "Should only flag javascript outside HTML comments");
        assert_eq!(result[0].line, 1);
        assert_eq!(result[1].line, 3);
    }

    #[test]
    fn test_html_comments_checked_when_enabled() {
        let config = MD044Config {
            names: vec!["JavaScript".to_string()],
            code_blocks: true,   // Check code blocks
            html_elements: true, // Check HTML elements
            html_comments: true, // Check HTML comments
        };
        let rule = MD044ProperNames::from_config_struct(config);

        let content = r#"Regular javascript here.
<!-- This javascript in HTML comment should be checked -->
More javascript outside."#;

        let ctx = create_context(content);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(
            result.len(),
            3,
            "Should flag all javascript occurrences including in HTML comments"
        );
    }

    #[test]
    fn test_multiline_html_comments() {
        let config = MD044Config {
            names: vec!["Python".to_string(), "JavaScript".to_string()],
            code_blocks: true,    // Check code blocks
            html_elements: true,  // Check HTML elements
            html_comments: false, // Don't check HTML comments
        };
        let rule = MD044ProperNames::from_config_struct(config);

        let content = r#"Regular python here.
<!--
This is a multiline comment
with javascript and python
that should be ignored
-->
More javascript outside."#;

        let ctx = create_context(content);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 2, "Should only flag names outside HTML comments");
        assert_eq!(result[0].line, 1); // python
        assert_eq!(result[1].line, 7); // javascript
    }

    #[test]
    fn test_fix_preserves_html_comments_when_disabled() {
        let config = MD044Config {
            names: vec!["JavaScript".to_string()],
            code_blocks: true,    // Check code blocks
            html_elements: true,  // Check HTML elements
            html_comments: false, // Don't check HTML comments
        };
        let rule = MD044ProperNames::from_config_struct(config);

        let content = r#"javascript here.
<!-- javascript in comment -->
More javascript."#;

        let ctx = create_context(content);
        let fixed = rule.fix(&ctx).unwrap();

        let expected = r#"JavaScript here.
<!-- javascript in comment -->
More JavaScript."#;

        assert_eq!(
            fixed, expected,
            "Should not fix names inside HTML comments when disabled"
        );
    }

    #[test]
    fn test_proper_names_in_link_text_are_flagged() {
        let rule = MD044ProperNames::new(
            vec!["JavaScript".to_string(), "Node.js".to_string(), "Python".to_string()],
            true,
        );

        let content = r#"Check this [javascript documentation](https://javascript.info) for info.

Visit [node.js homepage](https://nodejs.org) and [python tutorial](https://python.org).

Real javascript should be flagged.

Also see the [typescript guide][ts-ref] for more.

Real python should be flagged too.

[ts-ref]: https://typescript.org/handbook"#;

        let ctx = create_context(content);
        let result = rule.check(&ctx).unwrap();

        // Link text should be checked, URLs should not be checked
        // Line 1: [javascript documentation] - "javascript" should be flagged
        // Line 3: [node.js homepage] - "node.js" should be flagged (matches "Node.js")
        // Line 3: [python tutorial] - "python" should be flagged
        // Line 5: standalone javascript
        // Line 9: standalone python
        assert_eq!(result.len(), 5, "Expected 5 warnings: 3 in link text + 2 standalone");

        // Verify line numbers for link text warnings
        let line_1_warnings: Vec<_> = result.iter().filter(|w| w.line == 1).collect();
        assert_eq!(line_1_warnings.len(), 1);
        assert!(
            line_1_warnings[0]
                .message
                .contains("'javascript' should be 'JavaScript'")
        );

        let line_3_warnings: Vec<_> = result.iter().filter(|w| w.line == 3).collect();
        assert_eq!(line_3_warnings.len(), 2); // node.js and python

        // Standalone warnings
        assert!(result.iter().any(|w| w.line == 5 && w.message.contains("'javascript'")));
        assert!(result.iter().any(|w| w.line == 9 && w.message.contains("'python'")));
    }

    #[test]
    fn test_link_urls_not_flagged() {
        let rule = MD044ProperNames::new(vec!["JavaScript".to_string()], true);

        // URL contains "javascript" but should NOT be flagged
        let content = r#"[Link Text](https://javascript.info/guide)"#;

        let ctx = create_context(content);
        let result = rule.check(&ctx).unwrap();

        // URL should not be checked
        assert!(result.is_empty(), "URLs should not be checked for proper names");
    }

    #[test]
    fn test_proper_names_in_image_alt_text_are_flagged() {
        let rule = MD044ProperNames::new(vec!["JavaScript".to_string()], true);

        let content = r#"Here is a ![javascript logo](javascript.png "javascript icon") image.

Real javascript should be flagged."#;

        let ctx = create_context(content);
        let result = rule.check(&ctx).unwrap();

        // Image alt text should be checked, URL and title should not be checked
        // Line 1: ![javascript logo] - "javascript" should be flagged
        // Line 3: standalone javascript
        assert_eq!(result.len(), 2, "Expected 2 warnings: 1 in alt text + 1 standalone");
        assert!(result[0].message.contains("'javascript' should be 'JavaScript'"));
        assert!(result[0].line == 1); // "![javascript logo]"
        assert!(result[1].message.contains("'javascript' should be 'JavaScript'"));
        assert!(result[1].line == 3); // "Real javascript should be flagged."
    }

    #[test]
    fn test_image_urls_not_flagged() {
        let rule = MD044ProperNames::new(vec!["JavaScript".to_string()], true);

        // URL contains "javascript" but should NOT be flagged
        let content = r#"![Logo](https://javascript.info/logo.png)"#;

        let ctx = create_context(content);
        let result = rule.check(&ctx).unwrap();

        // Image URL should not be checked
        assert!(result.is_empty(), "Image URLs should not be checked for proper names");
    }

    #[test]
    fn test_reference_link_text_flagged_but_definition_not() {
        let rule = MD044ProperNames::new(vec!["JavaScript".to_string(), "TypeScript".to_string()], true);

        let content = r#"Check the [javascript guide][js-ref] for details.

Real javascript should be flagged.

[js-ref]: https://javascript.info/typescript/guide"#;

        let ctx = create_context(content);
        let result = rule.check(&ctx).unwrap();

        // Link text should be checked, reference definitions should not
        // Line 1: [javascript guide] - should be flagged
        // Line 3: standalone javascript - should be flagged
        // Line 5: reference definition - should NOT be flagged
        assert_eq!(result.len(), 2, "Expected 2 warnings: 1 in link text + 1 standalone");
        assert!(result.iter().any(|w| w.line == 1 && w.message.contains("'javascript'")));
        assert!(result.iter().any(|w| w.line == 3 && w.message.contains("'javascript'")));
    }

    #[test]
    fn test_reference_definitions_not_flagged() {
        let rule = MD044ProperNames::new(vec!["JavaScript".to_string()], true);

        // Reference definition should NOT be flagged
        let content = r#"[js-ref]: https://javascript.info/guide"#;

        let ctx = create_context(content);
        let result = rule.check(&ctx).unwrap();

        // Reference definition URLs should not be checked
        assert!(result.is_empty(), "Reference definitions should not be checked");
    }

    #[test]
    fn test_wikilinks_text_is_flagged() {
        let rule = MD044ProperNames::new(vec!["JavaScript".to_string()], true);

        // WikiLinks [[destination]] should have their text checked
        let content = r#"[[javascript]]

Regular javascript here.

[[JavaScript|display text]]"#;

        let ctx = create_context(content);
        let result = rule.check(&ctx).unwrap();

        // Line 1: [[javascript]] - should be flagged (WikiLink text)
        // Line 3: standalone javascript - should be flagged
        // Line 5: [[JavaScript|display text]] - correct capitalization, no flag
        assert_eq!(result.len(), 2, "Expected 2 warnings: 1 in WikiLink + 1 standalone");
        assert!(
            result
                .iter()
                .any(|w| w.line == 1 && w.column == 3 && w.message.contains("'javascript'"))
        );
        assert!(result.iter().any(|w| w.line == 3 && w.message.contains("'javascript'")));
    }

    #[test]
    fn test_url_link_text_not_flagged() {
        let rule = MD044ProperNames::new(vec!["GitHub".to_string()], true);

        // Link text that is itself a URL should not be flagged
        let content = r#"[https://github.com/org/repo](https://github.com/org/repo)

[http://github.com/org/repo](http://github.com/org/repo)

[www.github.com/org/repo](https://www.github.com/org/repo)"#;

        let ctx = create_context(content);
        let result = rule.check(&ctx).unwrap();

        assert!(
            result.is_empty(),
            "URL-like link text should not be flagged, got: {result:?}"
        );
    }

    #[test]
    fn test_url_link_text_with_leading_space_not_flagged() {
        let rule = MD044ProperNames::new(vec!["GitHub".to_string()], true);

        // Leading/trailing whitespace in link text should be trimmed before URL check
        let content = r#"[ https://github.com/org/repo](https://github.com/org/repo)"#;

        let ctx = create_context(content);
        let result = rule.check(&ctx).unwrap();

        assert!(
            result.is_empty(),
            "URL-like link text with leading space should not be flagged, got: {result:?}"
        );
    }

    #[test]
    fn test_url_link_text_uppercase_scheme_not_flagged() {
        let rule = MD044ProperNames::new(vec!["GitHub".to_string()], true);

        let content = r#"[HTTPS://GITHUB.COM/org/repo](https://github.com/org/repo)"#;

        let ctx = create_context(content);
        let result = rule.check(&ctx).unwrap();

        assert!(
            result.is_empty(),
            "URL-like link text with uppercase scheme should not be flagged, got: {result:?}"
        );
    }

    #[test]
    fn test_non_url_link_text_still_flagged() {
        let rule = MD044ProperNames::new(vec!["GitHub".to_string()], true);

        // Link text that is NOT a URL should still be flagged
        let content = r#"[github.com/org/repo](https://github.com/org/repo)

[Visit github](https://github.com/org/repo)

[//github.com/org/repo](//github.com/org/repo)

[ftp://github.com/org/repo](ftp://github.com/org/repo)"#;

        let ctx = create_context(content);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 4, "Non-URL link text should be flagged, got: {result:?}");
        assert!(result.iter().any(|w| w.line == 1)); // github.com (no protocol)
        assert!(result.iter().any(|w| w.line == 3)); // Visit github
        assert!(result.iter().any(|w| w.line == 5)); // //github.com (protocol-relative)
        assert!(result.iter().any(|w| w.line == 7)); // ftp://github.com
    }

    #[test]
    fn test_url_link_text_fix_not_applied() {
        let rule = MD044ProperNames::new(vec!["GitHub".to_string()], true);

        let content = "[https://github.com/org/repo](https://github.com/org/repo)\n";

        let ctx = create_context(content);
        let result = rule.fix(&ctx).unwrap();

        assert_eq!(result, content, "Fix should not modify URL-like link text");
    }

    #[test]
    fn test_mixed_url_and_regular_link_text() {
        let rule = MD044ProperNames::new(vec!["GitHub".to_string()], true);

        // Mix of URL link text (should skip) and regular text (should flag)
        let content = r#"[https://github.com/org/repo](https://github.com/org/repo)

Visit [github documentation](https://github.com/docs) for details.

[www.github.com/pricing](https://www.github.com/pricing)"#;

        let ctx = create_context(content);
        let result = rule.check(&ctx).unwrap();

        // Only line 3 should be flagged ("github documentation" is not a URL)
        assert_eq!(
            result.len(),
            1,
            "Only non-URL link text should be flagged, got: {result:?}"
        );
        assert_eq!(result[0].line, 3);
    }

    #[test]
    fn test_html_attribute_values_not_flagged() {
        // Matches inside HTML tag attributes (between `<` and `>`) are not flagged.
        // Attribute values are not prose — they hold URLs, class names, data values, etc.
        let rule = MD044ProperNames::new(vec!["Test".to_string()], true);
        let content = "# Heading\n\ntest\n\n<img src=\"www.example.test/test_image.png\">\n";
        let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Nothing on line 5 should be flagged — everything is inside the `<img ...>` tag
        let line5_violations: Vec<_> = result.iter().filter(|w| w.line == 5).collect();
        assert!(
            line5_violations.is_empty(),
            "Should not flag anything inside HTML tag attributes: {line5_violations:?}"
        );

        // Plain text on line 3 is still flagged
        let line3_violations: Vec<_> = result.iter().filter(|w| w.line == 3).collect();
        assert_eq!(line3_violations.len(), 1, "Plain 'test' on line 3 should be flagged");
    }

    #[test]
    fn test_html_text_content_still_flagged() {
        // Text between HTML tags (not inside `<...>`) is still checked.
        let rule = MD044ProperNames::new(vec!["Test".to_string()], true);
        let content = "# Heading\n\n<a href=\"https://example.test/page\">test link</a>\n";
        let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // "example.test" in the href attribute → not flagged (inside `<...>`)
        // "test link" in the anchor text → flagged (between `>` and `<`)
        assert_eq!(
            result.len(),
            1,
            "Should flag only 'test' in anchor text, not in href: {result:?}"
        );
        assert_eq!(result[0].column, 37, "Should flag col 37 ('test link' in anchor text)");
    }

    #[test]
    fn test_html_attribute_various_not_flagged() {
        // All attribute types are ignored: src, href, alt, class, data-*, title, etc.
        let rule = MD044ProperNames::new(vec!["Test".to_string()], true);
        let content = concat!(
            "# Heading\n\n",
            "<img src=\"test.png\" alt=\"test image\">\n",
            "<span class=\"test-class\" data-test=\"value\">test content</span>\n",
        );
        let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Only "test content" (between tags on line 4) should be flagged
        assert_eq!(
            result.len(),
            1,
            "Should flag only 'test content' between tags: {result:?}"
        );
        assert_eq!(result[0].line, 4);
    }

    #[test]
    fn test_plain_text_underscore_boundary_unchanged() {
        // Plain text (outside HTML tags) still uses original word boundary semantics where
        // underscore is a boundary character, matching markdownlint's behavior via AST splitting.
        let rule = MD044ProperNames::new(vec!["Test".to_string()], true);
        let content = "# Heading\n\ntest_image is here and just_test ends here\n";
        let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Both "test_image" (test at start) and "just_test" (test at end) are flagged
        // because in plain text, "_" is a word boundary
        assert_eq!(
            result.len(),
            2,
            "Should flag 'test' in both 'test_image' and 'just_test': {result:?}"
        );
        let cols: Vec<usize> = result.iter().map(|w| w.column).collect();
        assert!(cols.contains(&1), "Should flag col 1 (test_image): {cols:?}");
        assert!(cols.contains(&29), "Should flag col 29 (just_test): {cols:?}");
    }

    #[test]
    fn test_frontmatter_yaml_keys_not_flagged() {
        // YAML keys in frontmatter should NOT be checked for proper name violations.
        // Only values should be checked.
        let rule = MD044ProperNames::new(vec!["Test".to_string()], true);

        let content = "---\ntitle: Heading\ntest: Some Test value\n---\n\nTest\n";
        let ctx = create_context(content);
        let result = rule.check(&ctx).unwrap();

        // "test" in the YAML key (line 3) should NOT be flagged
        // "Test" in the YAML value (line 3) is correct capitalization, no flag
        // "Test" in body (line 6) is correct capitalization, no flag
        assert!(
            result.is_empty(),
            "Should not flag YAML keys or correctly capitalized values: {result:?}"
        );
    }

    #[test]
    fn test_frontmatter_yaml_values_flagged() {
        // Incorrectly capitalized names in YAML values should be flagged.
        let rule = MD044ProperNames::new(vec!["Test".to_string()], true);

        let content = "---\ntitle: Heading\nkey: a test value\n---\n\nTest\n";
        let ctx = create_context(content);
        let result = rule.check(&ctx).unwrap();

        // "test" in the YAML value (line 3) SHOULD be flagged
        assert_eq!(result.len(), 1, "Should flag 'test' in YAML value: {result:?}");
        assert_eq!(result[0].line, 3);
        assert_eq!(result[0].column, 8); // "key: a " = 7 chars, then "test" at column 8
    }

    #[test]
    fn test_frontmatter_key_matches_name_not_flagged() {
        // A YAML key that happens to match a configured name should NOT be flagged.
        let rule = MD044ProperNames::new(vec!["Test".to_string()], true);

        let content = "---\ntest: other value\n---\n\nBody text\n";
        let ctx = create_context(content);
        let result = rule.check(&ctx).unwrap();

        assert!(
            result.is_empty(),
            "Should not flag YAML key that matches configured name: {result:?}"
        );
    }

    #[test]
    fn test_frontmatter_empty_value_not_flagged() {
        // YAML key with no value should be skipped entirely.
        let rule = MD044ProperNames::new(vec!["Test".to_string()], true);

        let content = "---\ntest:\ntest: \n---\n\nBody text\n";
        let ctx = create_context(content);
        let result = rule.check(&ctx).unwrap();

        assert!(
            result.is_empty(),
            "Should not flag YAML keys with empty values: {result:?}"
        );
    }

    #[test]
    fn test_frontmatter_nested_yaml_key_not_flagged() {
        // Nested/indented YAML keys should also be skipped.
        let rule = MD044ProperNames::new(vec!["Test".to_string()], true);

        let content = "---\nparent:\n  test: nested value\n---\n\nBody text\n";
        let ctx = create_context(content);
        let result = rule.check(&ctx).unwrap();

        // "test" as a nested key should NOT be flagged
        assert!(result.is_empty(), "Should not flag nested YAML keys: {result:?}");
    }

    #[test]
    fn test_frontmatter_list_items_checked() {
        // YAML list items are values and should be checked for proper names.
        let rule = MD044ProperNames::new(vec!["Test".to_string()], true);

        let content = "---\ntags:\n  - test\n  - other\n---\n\nBody text\n";
        let ctx = create_context(content);
        let result = rule.check(&ctx).unwrap();

        // "test" as a list item value SHOULD be flagged
        assert_eq!(result.len(), 1, "Should flag 'test' in YAML list item: {result:?}");
        assert_eq!(result[0].line, 3);
    }

    #[test]
    fn test_frontmatter_value_with_multiple_colons() {
        // For "key: value: more", key is before first colon.
        let rule = MD044ProperNames::new(vec!["Test".to_string()], true);

        let content = "---\ntest: description: a test thing\n---\n\nBody text\n";
        let ctx = create_context(content);
        let result = rule.check(&ctx).unwrap();

        // "test" as key should NOT be flagged
        // "test" in value portion ("description: a test thing") SHOULD be flagged
        assert_eq!(
            result.len(),
            1,
            "Should flag 'test' in value after first colon: {result:?}"
        );
        assert_eq!(result[0].line, 2);
        assert!(result[0].column > 6, "Violation column should be in value portion");
    }

    #[test]
    fn test_frontmatter_does_not_affect_body() {
        // Body text after frontmatter should still be fully checked.
        let rule = MD044ProperNames::new(vec!["Test".to_string()], true);

        let content = "---\ntitle: Heading\n---\n\ntest should be flagged here\n";
        let ctx = create_context(content);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 1, "Should flag 'test' in body text: {result:?}");
        assert_eq!(result[0].line, 5);
    }

    #[test]
    fn test_frontmatter_fix_corrects_values_preserves_keys() {
        // Fix should correct YAML values but preserve keys.
        let rule = MD044ProperNames::new(vec!["Test".to_string()], true);

        let content = "---\ntest: a test value\n---\n\ntest here\n";
        let ctx = create_context(content);
        let fixed = rule.fix(&ctx).unwrap();

        // Key "test" should remain lowercase; value "test" should become "Test"
        assert_eq!(fixed, "---\ntest: a Test value\n---\n\nTest here\n");
    }

    #[test]
    fn test_frontmatter_multiword_value_flagged() {
        // Multiple proper names in a single YAML value should all be flagged.
        let rule = MD044ProperNames::new(vec!["JavaScript".to_string(), "TypeScript".to_string()], true);

        let content = "---\ndescription: Learn javascript and typescript\n---\n\nBody\n";
        let ctx = create_context(content);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 2, "Should flag both names in YAML value: {result:?}");
        assert!(result.iter().all(|w| w.line == 2));
    }

    #[test]
    fn test_frontmatter_yaml_comments_not_checked() {
        // YAML comments inside frontmatter should be skipped entirely.
        let rule = MD044ProperNames::new(vec!["Test".to_string()], true);

        let content = "---\n# test comment\ntitle: Heading\n---\n\nBody text\n";
        let ctx = create_context(content);
        let result = rule.check(&ctx).unwrap();

        assert!(result.is_empty(), "Should not flag names in YAML comments: {result:?}");
    }

    #[test]
    fn test_frontmatter_delimiters_not_checked() {
        // Frontmatter delimiter lines (--- or +++) should never be checked.
        let rule = MD044ProperNames::new(vec!["Test".to_string()], true);

        let content = "---\ntitle: Heading\n---\n\ntest here\n";
        let ctx = create_context(content);
        let result = rule.check(&ctx).unwrap();

        // Only the body "test" on line 5 should be flagged
        assert_eq!(result.len(), 1, "Should only flag body text: {result:?}");
        assert_eq!(result[0].line, 5);
    }

    #[test]
    fn test_frontmatter_continuation_lines_checked() {
        // Continuation lines (indented, no colon) are value content and should be checked.
        let rule = MD044ProperNames::new(vec!["Test".to_string()], true);

        let content = "---\ndescription: >\n  a test value\n  continued here\n---\n\nBody\n";
        let ctx = create_context(content);
        let result = rule.check(&ctx).unwrap();

        // "test" on the continuation line should be flagged
        assert_eq!(result.len(), 1, "Should flag 'test' in continuation line: {result:?}");
        assert_eq!(result[0].line, 3);
    }

    #[test]
    fn test_frontmatter_quoted_values_checked() {
        // Quoted YAML values should have their content checked (inside the quotes).
        let rule = MD044ProperNames::new(vec!["Test".to_string()], true);

        let content = "---\ntitle: \"a test title\"\n---\n\nBody\n";
        let ctx = create_context(content);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 1, "Should flag 'test' in quoted YAML value: {result:?}");
        assert_eq!(result[0].line, 2);
    }

    #[test]
    fn test_frontmatter_single_quoted_values_checked() {
        // Single-quoted YAML values should have their content checked.
        let rule = MD044ProperNames::new(vec!["Test".to_string()], true);

        let content = "---\ntitle: 'a test title'\n---\n\nBody\n";
        let ctx = create_context(content);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(
            result.len(),
            1,
            "Should flag 'test' in single-quoted YAML value: {result:?}"
        );
        assert_eq!(result[0].line, 2);
    }

    #[test]
    fn test_frontmatter_fix_multiword_values() {
        // Fix should correct all proper names in frontmatter values.
        let rule = MD044ProperNames::new(vec!["JavaScript".to_string(), "TypeScript".to_string()], true);

        let content = "---\ndescription: Learn javascript and typescript\n---\n\nBody\n";
        let ctx = create_context(content);
        let fixed = rule.fix(&ctx).unwrap();

        assert_eq!(
            fixed,
            "---\ndescription: Learn JavaScript and TypeScript\n---\n\nBody\n"
        );
    }

    #[test]
    fn test_frontmatter_fix_preserves_yaml_structure() {
        // Fix should preserve YAML structure while correcting values.
        let rule = MD044ProperNames::new(vec!["Test".to_string()], true);

        let content = "---\ntags:\n  - test\n  - other\ntitle: a test doc\n---\n\ntest body\n";
        let ctx = create_context(content);
        let fixed = rule.fix(&ctx).unwrap();

        assert_eq!(
            fixed,
            "---\ntags:\n  - Test\n  - other\ntitle: a Test doc\n---\n\nTest body\n"
        );
    }

    #[test]
    fn test_frontmatter_toml_delimiters_not_checked() {
        // TOML frontmatter with +++ delimiters should also be handled.
        let rule = MD044ProperNames::new(vec!["Test".to_string()], true);

        let content = "+++\ntitle = \"a test title\"\n+++\n\ntest body\n";
        let ctx = create_context(content);
        let result = rule.check(&ctx).unwrap();

        // "title" as TOML key should NOT be flagged
        // "test" in TOML quoted value SHOULD be flagged (line 2)
        // "test" in body SHOULD be flagged (line 5)
        assert_eq!(result.len(), 2, "Should flag TOML value and body: {result:?}");
        let fm_violations: Vec<_> = result.iter().filter(|w| w.line == 2).collect();
        assert_eq!(fm_violations.len(), 1, "Should flag 'test' in TOML value: {result:?}");
        let body_violations: Vec<_> = result.iter().filter(|w| w.line == 5).collect();
        assert_eq!(body_violations.len(), 1, "Should flag body 'test': {result:?}");
    }

    #[test]
    fn test_frontmatter_toml_key_not_flagged() {
        // TOML keys should NOT be flagged, only values.
        let rule = MD044ProperNames::new(vec!["Test".to_string()], true);

        let content = "+++\ntest = \"other value\"\n+++\n\nBody text\n";
        let ctx = create_context(content);
        let result = rule.check(&ctx).unwrap();

        assert!(
            result.is_empty(),
            "Should not flag TOML key that matches configured name: {result:?}"
        );
    }

    #[test]
    fn test_frontmatter_toml_fix_preserves_keys() {
        // Fix should correct TOML values but preserve keys.
        let rule = MD044ProperNames::new(vec!["Test".to_string()], true);

        let content = "+++\ntest = \"a test value\"\n+++\n\ntest here\n";
        let ctx = create_context(content);
        let fixed = rule.fix(&ctx).unwrap();

        // Key "test" should remain lowercase; value "test" should become "Test"
        assert_eq!(fixed, "+++\ntest = \"a Test value\"\n+++\n\nTest here\n");
    }

    #[test]
    fn test_frontmatter_list_item_mapping_key_not_flagged() {
        // In "- test: nested value", "test" is a YAML key within a list-item mapping.
        // The key should NOT be flagged; only the value should be checked.
        let rule = MD044ProperNames::new(vec!["Test".to_string()], true);

        let content = "---\nitems:\n  - test: nested value\n---\n\nBody text\n";
        let ctx = create_context(content);
        let result = rule.check(&ctx).unwrap();

        assert!(
            result.is_empty(),
            "Should not flag YAML key in list-item mapping: {result:?}"
        );
    }

    #[test]
    fn test_frontmatter_list_item_mapping_value_flagged() {
        // In "- key: test value", the value portion should be checked.
        let rule = MD044ProperNames::new(vec!["Test".to_string()], true);

        let content = "---\nitems:\n  - key: a test value\n---\n\nBody text\n";
        let ctx = create_context(content);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(
            result.len(),
            1,
            "Should flag 'test' in list-item mapping value: {result:?}"
        );
        assert_eq!(result[0].line, 3);
    }

    #[test]
    fn test_frontmatter_bare_list_item_still_flagged() {
        // Bare list items without a colon (e.g., "- test") are values and should be flagged.
        let rule = MD044ProperNames::new(vec!["Test".to_string()], true);

        let content = "---\ntags:\n  - test\n  - other\n---\n\nBody text\n";
        let ctx = create_context(content);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 1, "Should flag 'test' in bare list item: {result:?}");
        assert_eq!(result[0].line, 3);
    }

    #[test]
    fn test_frontmatter_flow_mapping_not_flagged() {
        // Flow mappings like {test: value} contain YAML keys that should not be flagged.
        // The entire flow construct should be skipped.
        let rule = MD044ProperNames::new(vec!["Test".to_string()], true);

        let content = "---\nflow_map: {test: value, other: test}\n---\n\nBody text\n";
        let ctx = create_context(content);
        let result = rule.check(&ctx).unwrap();

        assert!(
            result.is_empty(),
            "Should not flag names inside flow mappings: {result:?}"
        );
    }

    #[test]
    fn test_frontmatter_flow_sequence_not_flagged() {
        // Flow sequences like [test, other] should also be skipped.
        let rule = MD044ProperNames::new(vec!["Test".to_string()], true);

        let content = "---\nitems: [test, other, test]\n---\n\nBody text\n";
        let ctx = create_context(content);
        let result = rule.check(&ctx).unwrap();

        assert!(
            result.is_empty(),
            "Should not flag names inside flow sequences: {result:?}"
        );
    }

    #[test]
    fn test_frontmatter_list_item_mapping_fix_preserves_key() {
        // Fix should correct values in list-item mappings but preserve keys.
        let rule = MD044ProperNames::new(vec!["Test".to_string()], true);

        let content = "---\nitems:\n  - test: a test value\n---\n\ntest here\n";
        let ctx = create_context(content);
        let fixed = rule.fix(&ctx).unwrap();

        // "test" as list-item key should remain lowercase;
        // "test" in value portion should become "Test"
        assert_eq!(fixed, "---\nitems:\n  - test: a Test value\n---\n\nTest here\n");
    }

    #[test]
    fn test_frontmatter_backtick_code_not_flagged() {
        // Names inside backticks in frontmatter should NOT be flagged when code_blocks=false.
        let config = MD044Config {
            names: vec!["GoodApplication".to_string()],
            code_blocks: false,
            ..MD044Config::default()
        };
        let rule = MD044ProperNames::from_config_struct(config);

        let content = "---\ntitle: \"`goodapplication` CLI\"\n---\n\nIntroductory `goodapplication` CLI text.\n";
        let ctx = create_context(content);
        let result = rule.check(&ctx).unwrap();

        // Neither the frontmatter nor the body backtick-wrapped name should be flagged
        assert!(
            result.is_empty(),
            "Should not flag names inside backticks in frontmatter or body: {result:?}"
        );
    }

    #[test]
    fn test_frontmatter_unquoted_backtick_code_not_flagged() {
        // Exact case from issue #513: unquoted YAML frontmatter with backticks
        let config = MD044Config {
            names: vec!["GoodApplication".to_string()],
            code_blocks: false,
            ..MD044Config::default()
        };
        let rule = MD044ProperNames::from_config_struct(config);

        let content = "---\ntitle: `goodapplication` CLI\n---\n\nIntroductory `goodapplication` CLI text.\n";
        let ctx = create_context(content);
        let result = rule.check(&ctx).unwrap();

        assert!(
            result.is_empty(),
            "Should not flag names inside backticks in unquoted YAML frontmatter: {result:?}"
        );
    }

    #[test]
    fn test_frontmatter_bare_name_still_flagged_with_backtick_nearby() {
        // Names outside backticks in frontmatter should still be flagged.
        let config = MD044Config {
            names: vec!["GoodApplication".to_string()],
            code_blocks: false,
            ..MD044Config::default()
        };
        let rule = MD044ProperNames::from_config_struct(config);

        let content = "---\ntitle: goodapplication `goodapplication` CLI\n---\n\nBody\n";
        let ctx = create_context(content);
        let result = rule.check(&ctx).unwrap();

        // Only the bare "goodapplication" (before backticks) should be flagged
        assert_eq!(
            result.len(),
            1,
            "Should flag bare name but not backtick-wrapped name: {result:?}"
        );
        assert_eq!(result[0].line, 2);
        assert_eq!(result[0].column, 8); // "title: " = 7 chars, name at column 8
    }

    #[test]
    fn test_frontmatter_backtick_code_with_code_blocks_true() {
        // When code_blocks=true, names inside backticks ARE checked.
        let config = MD044Config {
            names: vec!["GoodApplication".to_string()],
            code_blocks: true,
            ..MD044Config::default()
        };
        let rule = MD044ProperNames::from_config_struct(config);

        let content = "---\ntitle: \"`goodapplication` CLI\"\n---\n\nBody\n";
        let ctx = create_context(content);
        let result = rule.check(&ctx).unwrap();

        // With code_blocks=true, backtick-wrapped name SHOULD be flagged
        assert_eq!(
            result.len(),
            1,
            "Should flag backtick-wrapped name when code_blocks=true: {result:?}"
        );
        assert_eq!(result[0].line, 2);
    }

    #[test]
    fn test_frontmatter_fix_preserves_backtick_code() {
        // Fix should NOT change names inside backticks in frontmatter.
        let config = MD044Config {
            names: vec!["GoodApplication".to_string()],
            code_blocks: false,
            ..MD044Config::default()
        };
        let rule = MD044ProperNames::from_config_struct(config);

        let content = "---\ntitle: \"`goodapplication` CLI\"\n---\n\nIntroductory `goodapplication` CLI text.\n";
        let ctx = create_context(content);
        let fixed = rule.fix(&ctx).unwrap();

        // Neither backtick-wrapped occurrence should be changed
        assert_eq!(
            fixed, content,
            "Fix should not modify names inside backticks in frontmatter"
        );
    }

    // --- Angle-bracket URL tests (issue #457) ---

    #[test]
    fn test_angle_bracket_url_in_html_comment_not_flagged() {
        // Angle-bracket URLs inside HTML comments should be skipped
        let config = MD044Config {
            names: vec!["Test".to_string()],
            ..MD044Config::default()
        };
        let rule = MD044ProperNames::from_config_struct(config);

        let content = "---\ntitle: Level 1 heading\n---\n\n<https://www.example.test>\n\n<!-- This is a Test https://www.example.test -->\n<!-- This is a Test <https://www.example.test> -->\n";
        let ctx = create_context(content);
        let result = rule.check(&ctx).unwrap();

        // Line 7: "Test" in comment prose before bare URL -- already correct capitalization
        // Line 7: "test" in bare URL (not in angle brackets) -- but "test" is in URL domain, not prose.
        //   However, .example.test has "test" at a word boundary (after '.'), so it IS flagged.
        // Line 8: "Test" in comment prose -- correct capitalization, not flagged
        // Line 8: "test" in <https://www.example.test> -- inside angle-bracket URL, NOT flagged

        // The key assertion: line 8's angle-bracket URL should NOT produce a warning
        let line8_warnings: Vec<_> = result.iter().filter(|w| w.line == 8).collect();
        assert!(
            line8_warnings.is_empty(),
            "Should not flag names inside angle-bracket URLs in HTML comments: {line8_warnings:?}"
        );
    }

    #[test]
    fn test_bare_url_in_html_comment_still_flagged() {
        // Bare URLs (not in angle brackets) inside HTML comments should still be checked
        let config = MD044Config {
            names: vec!["Test".to_string()],
            ..MD044Config::default()
        };
        let rule = MD044ProperNames::from_config_struct(config);

        let content = "<!-- This is a test https://www.example.test -->\n";
        let ctx = create_context(content);
        let result = rule.check(&ctx).unwrap();

        // "test" appears as prose text before URL and also in the bare URL domain
        // At minimum, the prose "test" should be flagged
        assert!(
            !result.is_empty(),
            "Should flag 'test' in prose text of HTML comment with bare URL"
        );
    }

    #[test]
    fn test_angle_bracket_url_in_regular_markdown_not_flagged() {
        // Angle-bracket URLs in regular markdown are already handled by the link parser,
        // but the angle-bracket check provides a safety net
        let rule = MD044ProperNames::new(vec!["Test".to_string()], true);

        let content = "<https://www.example.test>\n";
        let ctx = create_context(content);
        let result = rule.check(&ctx).unwrap();

        assert!(
            result.is_empty(),
            "Should not flag names inside angle-bracket URLs in regular markdown: {result:?}"
        );
    }

    #[test]
    fn test_multiple_angle_bracket_urls_in_one_comment() {
        let config = MD044Config {
            names: vec!["Test".to_string()],
            ..MD044Config::default()
        };
        let rule = MD044ProperNames::from_config_struct(config);

        let content = "<!-- See <https://test.example.com> and <https://www.example.test> for details -->\n";
        let ctx = create_context(content);
        let result = rule.check(&ctx).unwrap();

        // Both URLs are inside angle brackets, so "test" inside them should NOT be flagged
        assert!(
            result.is_empty(),
            "Should not flag names inside multiple angle-bracket URLs: {result:?}"
        );
    }

    #[test]
    fn test_angle_bracket_non_url_still_flagged() {
        // <Test> is NOT a URL (no scheme), so is_in_angle_bracket_url does NOT protect it.
        // Whether it gets flagged depends on HTML tag detection, not on our URL check.
        assert!(
            !MD044ProperNames::is_in_angle_bracket_url("<test> which is not a URL.", 1),
            "is_in_angle_bracket_url should return false for non-URL angle brackets"
        );
    }

    #[test]
    fn test_angle_bracket_mailto_url_not_flagged() {
        let config = MD044Config {
            names: vec!["Test".to_string()],
            ..MD044Config::default()
        };
        let rule = MD044ProperNames::from_config_struct(config);

        let content = "<!-- Contact <mailto:test@example.com> for help -->\n";
        let ctx = create_context(content);
        let result = rule.check(&ctx).unwrap();

        assert!(
            result.is_empty(),
            "Should not flag names inside angle-bracket mailto URLs: {result:?}"
        );
    }

    #[test]
    fn test_angle_bracket_ftp_url_not_flagged() {
        let config = MD044Config {
            names: vec!["Test".to_string()],
            ..MD044Config::default()
        };
        let rule = MD044ProperNames::from_config_struct(config);

        let content = "<!-- Download from <ftp://test.example.com/file> -->\n";
        let ctx = create_context(content);
        let result = rule.check(&ctx).unwrap();

        assert!(
            result.is_empty(),
            "Should not flag names inside angle-bracket FTP URLs: {result:?}"
        );
    }

    #[test]
    fn test_angle_bracket_url_fix_preserves_url() {
        // Fix should not modify text inside angle-bracket URLs
        let config = MD044Config {
            names: vec!["Test".to_string()],
            ..MD044Config::default()
        };
        let rule = MD044ProperNames::from_config_struct(config);

        let content = "<!-- test text <https://www.example.test> -->\n";
        let ctx = create_context(content);
        let fixed = rule.fix(&ctx).unwrap();

        // "test" in prose should be fixed, URL should be preserved
        assert!(
            fixed.contains("<https://www.example.test>"),
            "Fix should preserve angle-bracket URLs: {fixed}"
        );
        assert!(
            fixed.contains("Test text"),
            "Fix should correct prose 'test' to 'Test': {fixed}"
        );
    }

    #[test]
    fn test_is_in_angle_bracket_url_helper() {
        // Direct tests of the helper function
        let line = "text <https://example.test> more text";

        // Inside the URL
        assert!(MD044ProperNames::is_in_angle_bracket_url(line, 5)); // '<'
        assert!(MD044ProperNames::is_in_angle_bracket_url(line, 6)); // 'h'
        assert!(MD044ProperNames::is_in_angle_bracket_url(line, 15)); // middle of URL
        assert!(MD044ProperNames::is_in_angle_bracket_url(line, 26)); // '>'

        // Outside the URL
        assert!(!MD044ProperNames::is_in_angle_bracket_url(line, 0)); // 't' at start
        assert!(!MD044ProperNames::is_in_angle_bracket_url(line, 4)); // space before '<'
        assert!(!MD044ProperNames::is_in_angle_bracket_url(line, 27)); // space after '>'

        // Non-URL angle brackets
        assert!(!MD044ProperNames::is_in_angle_bracket_url("<notaurl>", 1));

        // mailto scheme
        assert!(MD044ProperNames::is_in_angle_bracket_url(
            "<mailto:test@example.com>",
            10
        ));

        // ftp scheme
        assert!(MD044ProperNames::is_in_angle_bracket_url(
            "<ftp://test.example.com>",
            10
        ));
    }

    #[test]
    fn test_is_in_angle_bracket_url_uppercase_scheme() {
        // RFC 3986: URI schemes are case-insensitive
        assert!(MD044ProperNames::is_in_angle_bracket_url(
            "<HTTPS://test.example.com>",
            10
        ));
        assert!(MD044ProperNames::is_in_angle_bracket_url(
            "<Http://test.example.com>",
            10
        ));
    }

    #[test]
    fn test_is_in_angle_bracket_url_uncommon_schemes() {
        // ssh scheme
        assert!(MD044ProperNames::is_in_angle_bracket_url(
            "<ssh://test@example.com>",
            10
        ));
        // file scheme
        assert!(MD044ProperNames::is_in_angle_bracket_url("<file:///test/path>", 10));
        // data scheme (no authority, just colon)
        assert!(MD044ProperNames::is_in_angle_bracket_url("<data:text/plain;test>", 10));
    }

    #[test]
    fn test_is_in_angle_bracket_url_unclosed() {
        // Unclosed angle bracket should NOT match
        assert!(!MD044ProperNames::is_in_angle_bracket_url(
            "<https://test.example.com",
            10
        ));
    }

    #[test]
    fn test_vale_inline_config_comments_not_flagged() {
        let config = MD044Config {
            names: vec!["Vale".to_string(), "JavaScript".to_string()],
            ..MD044Config::default()
        };
        let rule = MD044ProperNames::from_config_struct(config);

        let content = "\
<!-- vale off -->
Some javascript text here.
<!-- vale on -->
<!-- vale Style.Rule = NO -->
More javascript text.
<!-- vale Style.Rule = YES -->
<!-- vale JavaScript.Grammar = NO -->
";
        let ctx = create_context(content);
        let result = rule.check(&ctx).unwrap();

        // Only the body text lines (2, 5) should be flagged for "javascript"
        assert_eq!(result.len(), 2, "Should only flag body lines, not Vale config comments");
        assert_eq!(result[0].line, 2);
        assert_eq!(result[1].line, 5);
    }

    #[test]
    fn test_remark_lint_inline_config_comments_not_flagged() {
        let config = MD044Config {
            names: vec!["JavaScript".to_string()],
            ..MD044Config::default()
        };
        let rule = MD044ProperNames::from_config_struct(config);

        let content = "\
<!-- lint disable remark-lint-some-rule -->
Some javascript text here.
<!-- lint enable remark-lint-some-rule -->
<!-- lint ignore remark-lint-some-rule -->
More javascript text.
";
        let ctx = create_context(content);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(
            result.len(),
            2,
            "Should only flag body lines, not remark-lint config comments"
        );
        assert_eq!(result[0].line, 2);
        assert_eq!(result[1].line, 5);
    }

    #[test]
    fn test_fix_does_not_modify_vale_remark_lint_comments() {
        let config = MD044Config {
            names: vec!["JavaScript".to_string(), "Vale".to_string()],
            ..MD044Config::default()
        };
        let rule = MD044ProperNames::from_config_struct(config);

        let content = "\
<!-- vale off -->
Some javascript text.
<!-- vale on -->
<!-- lint disable remark-lint-some-rule -->
More javascript text.
<!-- lint enable remark-lint-some-rule -->
";
        let ctx = create_context(content);
        let fixed = rule.fix(&ctx).unwrap();

        // Config directive lines must be preserved unchanged
        assert!(fixed.contains("<!-- vale off -->"));
        assert!(fixed.contains("<!-- vale on -->"));
        assert!(fixed.contains("<!-- lint disable remark-lint-some-rule -->"));
        assert!(fixed.contains("<!-- lint enable remark-lint-some-rule -->"));
        // Body text should be fixed
        assert!(fixed.contains("Some JavaScript text."));
        assert!(fixed.contains("More JavaScript text."));
    }

    #[test]
    fn test_mixed_tool_directives_all_skipped() {
        let config = MD044Config {
            names: vec!["JavaScript".to_string(), "Vale".to_string()],
            ..MD044Config::default()
        };
        let rule = MD044ProperNames::from_config_struct(config);

        let content = "\
<!-- rumdl-disable MD044 -->
Some javascript text.
<!-- markdownlint-disable -->
More javascript text.
<!-- vale off -->
Even more javascript text.
<!-- lint disable some-rule -->
Final javascript text.
<!-- rumdl-enable MD044 -->
<!-- markdownlint-enable -->
<!-- vale on -->
<!-- lint enable some-rule -->
";
        let ctx = create_context(content);
        let result = rule.check(&ctx).unwrap();

        // Only body text lines should be flagged (lines 2, 4, 6, 8)
        assert_eq!(
            result.len(),
            4,
            "Should only flag body lines, not any tool directive comments"
        );
        assert_eq!(result[0].line, 2);
        assert_eq!(result[1].line, 4);
        assert_eq!(result[2].line, 6);
        assert_eq!(result[3].line, 8);
    }

    #[test]
    fn test_vale_remark_lint_edge_cases_not_matched() {
        let config = MD044Config {
            names: vec!["JavaScript".to_string(), "Vale".to_string()],
            ..MD044Config::default()
        };
        let rule = MD044ProperNames::from_config_struct(config);

        // These are regular HTML comments, NOT tool directives:
        // - "<!-- vale -->" is not a valid Vale directive (no action keyword)
        // - "<!-- vale is a tool -->" starts with "vale" but is prose, not a directive
        // - "<!-- valedictorian javascript -->" does not start with "<!-- vale "
        // - "<!-- linting javascript tips -->" does not start with "<!-- lint "
        // - "<!-- vale javascript -->" starts with "vale" but has no action keyword
        // - "<!-- lint your javascript code -->" starts with "lint" but has no action keyword
        let content = "\
<!-- vale -->
<!-- vale is a tool for writing -->
<!-- valedictorian javascript -->
<!-- linting javascript tips -->
<!-- vale javascript -->
<!-- lint your javascript code -->
";
        let ctx = create_context(content);
        let result = rule.check(&ctx).unwrap();

        // Line 1: "<!-- vale -->" contains "vale" (wrong case for "Vale") -> flagged
        // Line 2: "<!-- vale is a tool for writing -->" contains "vale" -> flagged
        // Line 3: "<!-- valedictorian javascript -->" contains "javascript" -> flagged
        // Line 4: "<!-- linting javascript tips -->" contains "javascript" -> flagged
        // Line 5: "<!-- vale javascript -->" contains "vale" and "javascript" -> flagged for both
        // Line 6: "<!-- lint your javascript code -->" contains "javascript" -> flagged
        assert_eq!(
            result.len(),
            7,
            "Should flag proper names in non-directive HTML comments: got {result:?}"
        );
        assert_eq!(result[0].line, 1); // "vale" in <!-- vale -->
        assert_eq!(result[1].line, 2); // "vale" in <!-- vale is a tool -->
        assert_eq!(result[2].line, 3); // "javascript" in <!-- valedictorian javascript -->
        assert_eq!(result[3].line, 4); // "javascript" in <!-- linting javascript tips -->
        assert_eq!(result[4].line, 5); // "vale" in <!-- vale javascript -->
        assert_eq!(result[5].line, 5); // "javascript" in <!-- vale javascript -->
        assert_eq!(result[6].line, 6); // "javascript" in <!-- lint your javascript code -->
    }

    #[test]
    fn test_vale_style_directives_skipped() {
        let config = MD044Config {
            names: vec!["JavaScript".to_string(), "Vale".to_string()],
            ..MD044Config::default()
        };
        let rule = MD044ProperNames::from_config_struct(config);

        // These ARE valid Vale directives and should be skipped:
        let content = "\
<!-- vale style = MyStyle -->
<!-- vale styles = Style1, Style2 -->
<!-- vale MyRule.Name = YES -->
<!-- vale MyRule.Name = NO -->
Some javascript text.
";
        let ctx = create_context(content);
        let result = rule.check(&ctx).unwrap();

        // Only line 5 (body text) should be flagged
        assert_eq!(
            result.len(),
            1,
            "Should only flag body lines, not Vale style/rule directives: got {result:?}"
        );
        assert_eq!(result[0].line, 5);
    }

    // --- is_in_backtick_code_in_line unit tests ---

    #[test]
    fn test_backtick_code_single_backticks() {
        let line = "hello `world` bye";
        // 'w' is at index 7, inside the backtick span (content between backticks at 6 and 12)
        assert!(MD044ProperNames::is_in_backtick_code_in_line(line, 7));
        // 'h' at index 0 is outside
        assert!(!MD044ProperNames::is_in_backtick_code_in_line(line, 0));
        // 'b' at index 14 is outside
        assert!(!MD044ProperNames::is_in_backtick_code_in_line(line, 14));
    }

    #[test]
    fn test_backtick_code_double_backticks() {
        let line = "a ``code`` b";
        // 'c' is at index 4, inside ``...``
        assert!(MD044ProperNames::is_in_backtick_code_in_line(line, 4));
        // 'a' at index 0 is outside
        assert!(!MD044ProperNames::is_in_backtick_code_in_line(line, 0));
        // 'b' at index 11 is outside
        assert!(!MD044ProperNames::is_in_backtick_code_in_line(line, 11));
    }

    #[test]
    fn test_backtick_code_unclosed() {
        let line = "a `code b";
        // No closing backtick, so nothing is a code span
        assert!(!MD044ProperNames::is_in_backtick_code_in_line(line, 3));
    }

    #[test]
    fn test_backtick_code_mismatched_count() {
        // Single backtick opening, double backtick is not a match
        let line = "a `code`` b";
        // The single ` at index 2 doesn't match `` at index 7-8
        // So 'c' at index 3 is NOT in a code span
        assert!(!MD044ProperNames::is_in_backtick_code_in_line(line, 3));
    }

    #[test]
    fn test_backtick_code_multiple_spans() {
        let line = "`first` and `second`";
        // 'f' at index 1 (inside first span)
        assert!(MD044ProperNames::is_in_backtick_code_in_line(line, 1));
        // 'a' at index 8 (between spans)
        assert!(!MD044ProperNames::is_in_backtick_code_in_line(line, 8));
        // 's' at index 13 (inside second span)
        assert!(MD044ProperNames::is_in_backtick_code_in_line(line, 13));
    }

    #[test]
    fn test_backtick_code_on_backtick_boundary() {
        let line = "`code`";
        // Position 0 is the opening backtick itself, not inside the span
        assert!(!MD044ProperNames::is_in_backtick_code_in_line(line, 0));
        // Position 5 is the closing backtick, not inside the span
        assert!(!MD044ProperNames::is_in_backtick_code_in_line(line, 5));
        // Position 1-4 are inside the span
        assert!(MD044ProperNames::is_in_backtick_code_in_line(line, 1));
        assert!(MD044ProperNames::is_in_backtick_code_in_line(line, 4));
    }
}
