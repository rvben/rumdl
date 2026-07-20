use crate::utils::fast_hash;
use crate::utils::regex_cache::{escape_regex, get_cached_regex};

use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::utils::range_utils::byte_to_char_count;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

mod md044_config;
pub(super) use md044_config::MD044Config;

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
    // Memoizes name violations keyed by content hash. Deliberately behind an
    // `Arc<Mutex<..>>` so it is SHARED across clones: rule instances are cloned
    // per config group and recreated for inline-config overrides, and the same
    // file's content is frequently re-checked (check then fix), so a shared
    // cache avoids recomputing. `check()` stays observationally pure (same ctx
    // in, same warnings out); the cache only affects how fast that answer is
    // produced. The lock is held only for the map get/insert, never across the
    // regex scan.
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
            let fm_value_span = if line_info.in_front_matter {
                Self::frontmatter_value_span(line)
            } else {
                None
            };

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

                // Skip if inside a Markdown inline link URL in contexts where
                // pulldown-cmark doesn't parse Markdown syntax (HTML comments,
                // HTML blocks, frontmatter).
                if (line_info.in_html_comment || line_info.in_html_block || line_info.in_front_matter)
                    && Self::is_in_markdown_link_url(line, start_pos)
                {
                    continue;
                }

                // Skip if inside the URL portion of a WikiLink followed by a
                // parenthesised destination — [[text]](url). pulldown-cmark
                // registers [[text]] as a WikiLink in ctx.links but leaves the
                // (url) as plain text, so is_in_link() misses those bytes.
                if Self::is_in_wikilink_url(ctx, byte_pos) {
                    continue;
                }

                // Skip if inside a bare URL (https://foo.com in plain prose).
                // Bare URLs are not in ctx.links (flagging them is MD034's
                // domain), but a URL is still a URL: domains match
                // case-insensitively but paths are case-sensitive, so a
                // proper-name "fix" inside one can break the link.
                if Self::is_in_bare_url(ctx, byte_pos) {
                    continue;
                }

                // Skip if inside a file path within a frontmatter value. Domains
                // match case-insensitively but paths are case-sensitive, so
                // rewriting a name inside one breaks the reference it points at.
                // Body prose is deliberately never consulted here; see
                // `is_in_path_like_token` for why the exemption stops at the
                // frontmatter boundary.
                if let Some(fm_value) = fm_value_span
                    && Self::is_in_path_like_token(line, start_pos, fm_value)
                {
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

    /// Check if a byte position is within a bare URL detected by the shared
    /// lint-context parser (the same detection MD034 consumes).
    fn is_in_bare_url(ctx: &crate::lint_context::LintContext, byte_pos: usize) -> bool {
        let bare_urls = ctx.bare_urls();
        // Binary search (sorted by byte_offset) for the candidate containing byte_pos
        let idx = bare_urls.partition_point(|url| url.byte_offset <= byte_pos);
        idx > 0 && byte_pos < bare_urls[idx - 1].byte_end
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

                // If position is within the text portion, skip only if text is a URL.
                // WikiLinks use the page name as both text and url; never treat them
                // as bare-domain URLs regardless of whether the name contains dots.
                if byte_pos >= text_start && byte_pos < text_end {
                    let is_wikilink = matches!(link.link_type, LinkType::WikiLink { .. });
                    return Self::link_text_is_url(&link.text)
                        || (!is_wikilink && Self::link_text_matches_link_url(&link.text, &link.url));
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
    fn link_text_is_url(text: &str) -> bool {
        let lower = text.trim().to_ascii_lowercase();
        lower.starts_with("http://")
            || lower.starts_with("https://")
            || lower.starts_with("www.")
            || lower.starts_with("//")
    }

    /// Check if link text is the bare hostname/path of its destination URL.
    ///
    /// When the display text is the URL with the scheme stripped (e.g.,
    /// `[example.github.io](https://example.github.io)`), the text is a domain
    /// label, not a prose reference to a product, and should not be corrected.
    ///
    /// Requires the text to contain a dot, which distinguishes domain-like display
    /// text from single-word WikiLink targets (e.g. `[[javascript]]`) where
    /// `url == text` but neither is a domain name. Dotted WikiLink targets are
    /// excluded separately via the `!is_wikilink` guard in `is_in_link`. Comparison
    /// is case-insensitive because URL schemes and hostnames are case-insensitive.
    fn link_text_matches_link_url(text: &str, url: &str) -> bool {
        let text = text.trim();
        // Only domain-like text (containing a dot) can be a bare hostname.
        if !text.contains('.') {
            return false;
        }
        let url_lower = url.to_ascii_lowercase();
        let url_without_scheme = url_lower
            .strip_prefix("https://")
            .or_else(|| url_lower.strip_prefix("http://"))
            .or_else(|| url_lower.strip_prefix("//"))
            .unwrap_or(&url_lower);
        let text_lower = text.to_ascii_lowercase();
        // Exact match: text equals the URL with the scheme removed.
        if url_without_scheme == text_lower.as_str() {
            return true;
        }
        // Prefix match: text is the hostname portion and the URL has a path/query/fragment.
        url_without_scheme.len() > text_lower.len()
            && url_without_scheme.starts_with(text_lower.as_str())
            && matches!(
                url_without_scheme.as_bytes().get(text_lower.len()),
                Some(b'/') | Some(b'?') | Some(b'#')
            )
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

    /// Check if `byte_pos` falls inside the URL of a `[[text]](url)` construct.
    ///
    /// pulldown-cmark with WikiLinks enabled parses `[[text]]` as a WikiLink and
    /// records it in `ctx.links`, but the immediately following `(url)` is left as
    /// plain text and is therefore absent from `ctx.links`. This function detects
    /// that gap by looking for a WikiLink entry whose `byte_end` falls exactly on a
    /// `(` in the raw content, then checking whether `byte_pos` lies inside the
    /// matching parenthesised URL span.
    ///
    /// Unlike `is_in_markdown_link_url`, this function is anchored to real parser
    /// output (`ctx.links`) and will not suppress violations in text that merely
    /// looks like a link (e.g. `[foo](github x)` with a space in the URL).
    fn is_in_wikilink_url(ctx: &crate::lint_context::LintContext, byte_pos: usize) -> bool {
        use pulldown_cmark::LinkType;
        let content = ctx.content.as_bytes();

        // ctx.links is sorted by byte_offset; only links that start at or before
        // byte_pos can have a URL that encloses it.
        let end = ctx.links.partition_point(|l| l.byte_offset <= byte_pos);

        for link in &ctx.links[..end] {
            if !matches!(link.link_type, LinkType::WikiLink { .. }) {
                continue;
            }
            let wiki_end = link.byte_end;
            // The WikiLink must end before byte_pos and be immediately followed by '('.
            if wiki_end >= byte_pos || wiki_end >= content.len() || content[wiki_end] != b'(' {
                continue;
            }
            // Scan to the matching ')' tracking nested parens and backslash escapes.
            // Per CommonMark, an unquoted inline link destination cannot contain
            // spaces, tabs, or newlines. If we encounter one, this is parenthesised
            // prose rather than a URL, and pulldown-cmark will not parse it as a link.
            let mut depth: u32 = 1;
            let mut k = wiki_end + 1;
            let mut valid_destination = true;
            while k < content.len() && depth > 0 {
                match content[k] {
                    b'\\' => {
                        k += 1; // skip escaped character
                    }
                    b'(' => depth += 1,
                    b')' => depth -= 1,
                    b' ' | b'\t' | b'\n' | b'\r' => {
                        valid_destination = false;
                        break;
                    }
                    _ => {}
                }
                k += 1;
            }
            // byte_pos is inside the URL if it falls between '(' and the matching ')'
            // and the destination is valid (no unescaped whitespace).
            if valid_destination && depth == 0 && byte_pos > wiki_end && byte_pos < k {
                return true;
            }
        }
        false
    }

    /// Check if a position within a line falls inside a Markdown link's
    /// non-text portion (URL or reference label).
    ///
    /// Used as a text-level fallback for HTML comments, HTML blocks, and
    /// frontmatter where pulldown-cmark skips link parsing entirely. Operates on
    /// raw line bytes and therefore cannot distinguish real links from text that
    /// merely resembles link syntax; do not call on regular markdown lines.
    /// - `[text](url)` — returns true if `pos` is within `(...)`
    /// - `[text][ref]` — returns true if `pos` is within the second `[...]`
    fn is_in_markdown_link_url(line: &str, pos: usize) -> bool {
        let bytes = line.as_bytes();
        let len = bytes.len();
        let mut i = 0;

        while i < len {
            // Look for unescaped '[' (handle double-escaped \\[ as unescaped)
            if bytes[i] == b'[' && (i == 0 || bytes[i - 1] != b'\\' || (i >= 2 && bytes[i - 2] == b'\\')) {
                // Find matching ']' handling nested brackets
                let mut depth: u32 = 1;
                let mut j = i + 1;
                while j < len && depth > 0 {
                    match bytes[j] {
                        b'\\' => {
                            j += 1; // skip escaped char
                        }
                        b'[' => depth += 1,
                        b']' => depth -= 1,
                        _ => {}
                    }
                    j += 1;
                }

                // j is now one past the ']'
                if depth == 0 && j < len {
                    if bytes[j] == b'(' {
                        // Inline link: [text](url)
                        let url_start = j;
                        let mut paren_depth: u32 = 1;
                        let mut k = j + 1;
                        while k < len && paren_depth > 0 {
                            match bytes[k] {
                                b'\\' => {
                                    k += 1; // skip escaped char
                                }
                                b'(' => paren_depth += 1,
                                b')' => paren_depth -= 1,
                                _ => {}
                            }
                            k += 1;
                        }

                        if paren_depth == 0 {
                            if pos > url_start && pos < k {
                                return true;
                            }
                            i = k;
                            continue;
                        }
                    } else if bytes[j] == b'[' {
                        // Reference link: [text][ref]
                        let ref_start = j;
                        let mut ref_depth: u32 = 1;
                        let mut k = j + 1;
                        while k < len && ref_depth > 0 {
                            match bytes[k] {
                                b'\\' => {
                                    k += 1;
                                }
                                b'[' => ref_depth += 1,
                                b']' => ref_depth -= 1,
                                _ => {}
                            }
                            k += 1;
                        }

                        if ref_depth == 0 {
                            if pos > ref_start && pos < k {
                                return true;
                            }
                            i = k;
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

    /// Whether the frontmatter value starting at `value_start` is a quoted
    /// scalar: the character immediately preceding `value_start` is a quote.
    /// Call this with the span start returned by `frontmatter_value_span`,
    /// which always lands just past the opening quote for quoted values.
    /// The raw `frontmatter_value_offset` does not carry that guarantee: its
    /// helper `kv_value_offset` only skips the opening quote when the whole
    /// trimmed remainder of the line starts and ends with the same quote
    /// character, so a trailing comment or an unterminated quote leaves the
    /// offset pointing AT the quote instead of past it.
    fn value_is_quoted(line: &str, value_start: usize) -> bool {
        matches!(line[..value_start].chars().next_back(), Some('\'') | Some('"'))
    }

    /// Byte span of the semantic value on a frontmatter line: the checkable
    /// content with a trailing comment excluded. For a quoted scalar the span
    /// ends at the closing quote (or the trimmed end of line if the quote is
    /// unterminated), so `#` and spaces inside it are literal, and the quote
    /// characters themselves are never part of the span. `None` when the line
    /// carries no checkable value, including an empty quoted value (`''`).
    fn frontmatter_value_span(line: &str) -> Option<(usize, usize)> {
        let start = Self::frontmatter_value_offset(line);
        if start == usize::MAX || start >= line.len() {
            return None;
        }

        // `frontmatter_value_offset` sometimes points past the opening quote
        // already, and sometimes points AT it (see `value_is_quoted` docs).
        // Detect the quote from either position so both cases converge on a
        // `content_start` that is always just past the opening quote.
        let before = line[..start].chars().next_back();
        let at = line[start..].chars().next();
        let (content_start, quote) = match (before, at) {
            (Some(q @ ('\'' | '"')), _) => (start, Some(q)),
            (_, Some(q @ ('\'' | '"'))) => (start + q.len_utf8(), Some(q)),
            _ => (start, None),
        };

        let end = if let Some(quote) = quote {
            let rest = &line[content_start..];
            match rest.find(quote) {
                Some(i) => content_start + i,
                None => content_start + rest.trim_end().len(),
            }
        } else {
            let rest = &line[content_start..];
            let raw_end = match rest.find(" #") {
                Some(i) => content_start + i,
                None => line.len(),
            };
            line[..raw_end].trim_end().len()
        };

        if end <= content_start {
            None
        } else {
            Some((content_start, end))
        }
    }

    /// Delimiters that wrap a token from the outside (quotes, brackets,
    /// parens, angle brackets) rather than appearing inside a path. Used only
    /// by the edge-trimming pass: these characters legitimately occur inside
    /// real paths (Next.js route groups `(marketing)/`, dynamic segments
    /// `[slug]`, disambiguated filenames `myapp(1).md`), so they must not act
    /// as mid-token boundaries, only as leading/trailing punctuation to peel
    /// off prose wrapping such as `See (docs/a.md) here.`.
    const PATH_TOKEN_WRAPPERS: &'static [char] = &['\'', '"', '`', '(', ')', '[', ']', '<', '>'];

    /// Bounds of the whitespace-delimited token containing `pos`, clamped to
    /// `[value_start, value_end)`. The clamp is what keeps this search inside
    /// a single frontmatter value: it can never walk past the value's own
    /// boundaries, so it can never wander into Markdown link syntax on the
    /// same line (frontmatter has none) or onto a neighboring line.
    fn value_token_bounds(line: &str, pos: usize, value_start: usize, value_end: usize) -> (usize, usize) {
        let before = &line[value_start..pos];
        let start = before.rfind(char::is_whitespace).map_or(value_start, |i| {
            value_start + i + before[i..].chars().next().unwrap().len_utf8()
        });

        let after = &line[pos..value_end];
        let end = after.find(char::is_whitespace).map_or(value_end, |i| pos + i);

        (start, end)
    }

    /// Strip wrapping delimiters, then trailing sentence punctuation, repeating
    /// both passes until a full pass leaves the bounds unchanged. Punctuation
    /// removal can expose a wrapper underneath it (`"docs/myapp.md",` sheds the
    /// comma to reveal a trailing quote), so a single sequential pass is not
    /// enough to reach a stable result.
    fn trim_token_bounds(line: &str, mut start: usize, mut end: usize) -> (usize, usize) {
        const WRAPPERS: &[char] = MD044ProperNames::PATH_TOKEN_WRAPPERS;
        const TRAILING: &[char] = &['.', ',', ';', ':', '!', '?'];
        while start < end && line[start..end].starts_with(WRAPPERS) {
            start += line[start..].chars().next().unwrap().len_utf8();
        }
        loop {
            let before = (start, end);
            while end > start && line[start..end].ends_with(WRAPPERS) {
                end -= line[..end].chars().next_back().unwrap().len_utf8();
            }
            while end > start && line[start..end].ends_with(TRAILING) {
                end -= line[..end].chars().next_back().unwrap().len_utf8();
            }
            if (start, end) == before {
                break;
            }
        }
        (start, end)
    }

    /// Whether the match at `match_start` sits inside a file path, which must
    /// not be rewritten. `fm_value` is the semantic value span of the
    /// frontmatter line the match was found on.
    ///
    /// This exemption is deliberately scoped to frontmatter values only and
    /// is never applied to body prose. In frontmatter the value span is known
    /// exactly (`frontmatter_value_span`), so token bounds can be clamped to
    /// it with no risk of crossing into unrelated syntax. Body prose has no
    /// such known span: a token there must be delimited by scanning the raw
    /// line for whitespace and Markdown punctuation, and that scan
    /// unavoidably collides with Markdown link/image/wikilink syntax (see the
    /// module-level history of defects from trying this). Reusing this
    /// function for body text is a structural mismatch, not a missing edge
    /// case, so `fm_value` is required rather than optional: a caller cannot
    /// accidentally invoke this for a line that has no known value span.
    ///
    /// A slash is mandatory: without it a bare extension rule would swallow
    /// dotted proper names such as `Node.js`.
    ///
    /// The 3+ segment sole-value signal only fires for a genuinely
    /// single-token value. A quoted value that collapsed from several
    /// whitespace-separated words (`"myapp/gitlab github/bitbucket"`) must
    /// instead satisfy the path-prefix or file-extension signal; otherwise
    /// two unrelated slash-pairs joined by a space would vacuously look like
    /// a 3-segment path. This means an extensionless path containing a
    /// literal space (`docs/My App/myapp`) is no longer exempt, a deliberate
    /// narrowing rather than an oversight.
    fn is_in_path_like_token(line: &str, match_start: usize, fm_value: (usize, usize)) -> bool {
        let (value_start, value_end) = fm_value;
        if match_start < value_start || match_start >= value_end {
            return false;
        }

        // A quoted scalar is one token even with spaces in it, but only when
        // every whitespace-separated word in it carries a slash (e.g. a path
        // containing a space, `docs/My App/myapp`). A quoted sentence with
        // ordinary prose words (`"We support github/gitlab/bitbucket now"`)
        // falls back to per-word tokenization instead, otherwise quoting
        // alone would make the token span the whole value and vacuously
        // satisfy the sole-value check below.
        let quoted_words: Vec<&str> = if Self::value_is_quoted(line, value_start) {
            line[value_start..value_end].split_whitespace().collect()
        } else {
            Vec::new()
        };
        let is_single_quoted_path = !quoted_words.is_empty() && quoted_words.iter().all(|word| word.contains('/'));
        // Collapsing several whitespace-separated words into one token is
        // only safe evidence for signals (a) and (b): a shared slash prefix
        // or a real extension on the last segment. It is not evidence for
        // the segment-count signal below, which assumes a single word split
        // into path segments by '/'; two unrelated slash-pairs joined by a
        // space (`"myapp/gitlab github/bitbucket"`) would otherwise satisfy
        // that count vacuously.
        let is_multi_word_collapse = is_single_quoted_path && quoted_words.len() > 1;

        let (raw_start, raw_end) = if is_single_quoted_path {
            (value_start, value_end)
        } else {
            Self::value_token_bounds(line, match_start, value_start, value_end)
        };

        let (start, end) = Self::trim_token_bounds(line, raw_start, raw_end);
        if match_start < start || match_start >= end {
            return false;
        }

        let token = &line[start..end];
        if !token.contains('/') {
            return false;
        }
        if token.starts_with('/') || token.starts_with("./") || token.starts_with("../") || token.starts_with("~/") {
            return true;
        }
        if token.rsplit('/').next().is_some_and(|seg| seg.contains('.')) {
            return true;
        }

        if is_multi_word_collapse {
            return false;
        }

        // Three or more segments is only a path signal when the token is the
        // entire frontmatter value. In prose, `github/gitlab/bitbucket` is
        // shorthand, not a path.
        let sole_value = {
            let (ts, te) = Self::trim_token_bounds(line, value_start, value_end);
            ts == start && te == end
        };
        sole_value && token.split('/').filter(|s| !s.is_empty()).count() >= 3
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

    fn category(&self) -> RuleCategory {
        RuleCategory::Other
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
                self.get_proper_name_for(&found_name).map(|proper_name| {
                    // `column` is a 1-indexed byte offset into the line (from regex .start() + 1).
                    // Build the Fix range directly in bytes to avoid the character-based
                    // line_col_to_byte_range_with_length function, which would misinterpret
                    // the byte offset as a character count on lines with multi-byte content.
                    let line_start = line_index.get_line_start_byte(line).unwrap_or(0);
                    let byte_start = line_start + (column - 1);
                    let byte_end = byte_start + found_name.len();
                    // The displayed columns are character offsets; convert from the byte
                    // offset within the line so they are correct on multi-byte lines.
                    let line_text = ctx.line_info(line).map_or("", |li| li.content(ctx.content));
                    let char_col = byte_to_char_count(line_text, column - 1);
                    LintWarning {
                        rule_name: Some(self.name().to_string()),
                        line,
                        column: char_col,
                        end_line: line,
                        end_column: char_col + found_name.chars().count(),
                        message: format!("Proper name '{found_name}' should be '{proper_name}'"),
                        severity: Severity::Warning,
                        fix: Some(Fix::new(byte_start..byte_end, proper_name)),
                    }
                })
            })
            .collect();

        Ok(warnings)
    }

    fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, LintError> {
        if self.should_skip(ctx) {
            return Ok(ctx.content.to_string());
        }
        let warnings = self.check(ctx)?;
        if warnings.is_empty() {
            return Ok(ctx.content.to_string());
        }
        let warnings =
            crate::utils::fix_utils::filter_warnings_by_inline_config(warnings, ctx.inline_config(), self.name());
        crate::utils::fix_utils::apply_warning_fixes(ctx.content, &warnings)
            .map_err(crate::rule::LintError::InvalidInput)
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    crate::impl_rule_config_methods!(MD044Config);
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
    fn test_bare_urls_not_flagged() {
        let rule = MD044ProperNames::new(vec!["Foo".to_string(), "JavaScript".to_string()], true);

        // Bare URLs are not links in ctx.links, but a proper-name "fix"
        // inside a domain or case-sensitive path would break the link.
        let content =
            "https://foo.com\n\nSee https://javascript.info/foo/guide for details.\n\nMail foo@foo.com about it.\n";

        let ctx = create_context(content);
        let result = rule.check(&ctx).unwrap();

        assert!(
            result.is_empty(),
            "Bare URLs and emails should not be checked for proper names: {result:?}"
        );
    }

    #[test]
    fn test_prose_around_bare_url_still_flagged() {
        let rule = MD044ProperNames::new(vec!["Foo".to_string()], true);

        // The word before and after the URL must still be flagged; only the
        // URL bytes themselves are exempt.
        let content = "Use foo at https://foo.com because foo is great.\n";

        let ctx = create_context(content);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(
            result.len(),
            2,
            "Prose occurrences around a bare URL must still be flagged: {result:?}"
        );
        assert!(result.iter().all(|w| w.message.contains("'foo' should be 'Foo'")));
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

        // Only prose descriptions in link text should be flagged.
        // Bare-domain, protocol-relative, and scheme-prefixed link texts that
        // match the destination URL are all URLs and must not be corrected.
        let content = r#"[github.com/org/repo](https://github.com/org/repo)

[Visit github](https://github.com/org/repo)

[//github.com/org/repo](//github.com/org/repo)

[ftp://github.com/org/repo](ftp://github.com/org/repo)"#;

        let ctx = create_context(content);
        let result = rule.check(&ctx).unwrap();

        // Line 1: bare-domain text matches destination — not flagged
        // Line 3: prose description — flagged
        // Line 5: protocol-relative URL text — not flagged
        // Line 7: ftp:// URL text matches destination — not flagged
        assert_eq!(
            result.len(),
            1,
            "Only prose link text should be flagged, got: {result:?}"
        );
        assert!(
            result.iter().any(|w| w.line == 3),
            "Expected 'Visit github' on line 3 to be flagged"
        );
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

    // Double-bracket WikiLink + URL: [[text]](url)
    // pulldown-cmark parses [[text]] as a WikiLink but leaves the (url)
    // as plain text, so ctx.links does not cover the URL portion.
    // MD044 must fall back to is_in_markdown_link_url for all lines.

    #[test]
    fn test_double_bracket_link_url_not_flagged() {
        let rule = MD044ProperNames::new(vec!["GitHub".to_string()], false);
        // Exact reproduction from issue #564
        let content = "[[rumdl]](https://github.com/rvben/rumdl)";
        let ctx = create_context(content);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "URL inside [[text]](url) must not be flagged, got: {result:?}"
        );
    }

    #[test]
    fn test_double_bracket_link_url_not_fixed() {
        let rule = MD044ProperNames::new(vec!["GitHub".to_string()], false);
        let content = "[[rumdl]](https://github.com/rvben/rumdl)\n";
        let ctx = create_context(content);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(
            fixed, content,
            "fix() must leave the URL inside [[text]](url) unchanged"
        );
    }

    #[test]
    fn test_double_bracket_link_text_still_flagged() {
        let rule = MD044ProperNames::new(vec!["GitHub".to_string()], false);
        // The link text portion [[github]](url) should still be checked.
        let content = "[[github]](https://example.com)";
        let ctx = create_context(content);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(
            result.len(),
            1,
            "Incorrect name in [[text]] link text should still be flagged, got: {result:?}"
        );
        assert_eq!(result[0].message, "Proper name 'github' should be 'GitHub'");
    }

    #[test]
    fn test_double_bracket_link_mixed_line() {
        let rule = MD044ProperNames::new(vec!["GitHub".to_string()], false);
        // URL must be skipped, standalone text must be flagged.
        let content = "See [[rumdl]](https://github.com/rvben/rumdl) and github for more.";
        let ctx = create_context(content);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(
            result.len(),
            1,
            "Only the standalone 'github' after the link should be flagged, got: {result:?}"
        );
        assert!(result[0].message.contains("'github'"));
        // "See " (4) + "[[rumdl]](https://github.com/rvben/rumdl)" (42) + " and " (4) = column 51
        assert_eq!(
            result[0].column, 51,
            "Flagged column should be the trailing 'github', not the one in the URL"
        );
    }

    #[test]
    fn test_regular_link_url_still_not_flagged() {
        // Confirm existing [text](url) behavior is unaffected by the fix.
        let rule = MD044ProperNames::new(vec!["GitHub".to_string()], false);
        let content = "[rumdl](https://github.com/rvben/rumdl)";
        let ctx = create_context(content);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "URL inside regular [text](url) must still not be flagged, got: {result:?}"
        );
    }

    #[test]
    fn test_link_like_text_in_code_span_still_flagged_when_code_blocks_enabled() {
        // When code-blocks = true the user explicitly opts into checking code spans.
        // A code span containing link-like text (`[foo](https://github.com)`) must
        // NOT be silently suppressed by is_in_markdown_link_url: the content is
        // literal characters, not a real Markdown link.
        let rule = MD044ProperNames::new(vec!["GitHub".to_string()], true);
        let content = "`[foo](https://github.com/org/repo)`";
        let ctx = create_context(content);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(
            result.len(),
            1,
            "Proper name inside a code span must be flagged when code-blocks=true, got: {result:?}"
        );
        assert!(result[0].message.contains("'github'"));
    }

    #[test]
    fn test_malformed_link_not_treated_as_url() {
        // [text](url with spaces) is NOT a valid Markdown link; pulldown-cmark
        // does not parse it, so the name inside must still be flagged.
        let rule = MD044ProperNames::new(vec!["GitHub".to_string()], false);
        let content = "See [rumdl](github repo) for details.";
        let ctx = create_context(content);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(
            result.len(),
            1,
            "Name inside malformed [text](url with spaces) must still be flagged, got: {result:?}"
        );
        assert!(result[0].message.contains("'github'"));
    }

    #[test]
    fn test_wikilink_followed_by_prose_parens_still_flagged() {
        // [[note]](github repo) — WikiLink followed by parenthesised prose, NOT
        // a valid link URL (space in destination). pulldown-cmark does not parse
        // it as a link, so the name inside must still be flagged.
        let rule = MD044ProperNames::new(vec!["GitHub".to_string()], false);
        let content = "[[note]](github repo)";
        let ctx = create_context(content);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(
            result.len(),
            1,
            "Name inside [[wikilink]](prose with spaces) must still be flagged, got: {result:?}"
        );
        assert!(result[0].message.contains("'github'"));
    }

    /// Roundtrip safety: fix() output must produce zero warnings on re-check.
    #[test]
    fn test_roundtrip_fix_then_check_basic() {
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
        let ctx2 = create_context(&fixed);
        let warnings = rule.check(&ctx2).unwrap();
        assert!(
            warnings.is_empty(),
            "Re-check after fix should produce zero warnings, got: {warnings:?}"
        );
    }

    /// Roundtrip safety: fix() output must produce zero warnings for multiline content.
    #[test]
    fn test_roundtrip_fix_then_check_multiline() {
        let rule = MD044ProperNames::new(vec!["Rust".to_string(), "Python".to_string()], true);
        let content = "First line with rust.\nSecond line with python.\nThird line with RUST and PYTHON.\n";
        let ctx = create_context(content);
        let fixed = rule.fix(&ctx).unwrap();
        let ctx2 = create_context(&fixed);
        let warnings = rule.check(&ctx2).unwrap();
        assert!(
            warnings.is_empty(),
            "Re-check after fix should produce zero warnings, got: {warnings:?}"
        );
    }

    /// Roundtrip safety: fix() with inline config disable blocks.
    #[test]
    fn test_roundtrip_fix_then_check_inline_config() {
        let config = MD044Config {
            names: vec!["RUMDL".to_string()],
            ..MD044Config::default()
        };
        let rule = MD044ProperNames::from_config_struct(config);
        let content =
            "<!-- rumdl-disable MD044 -->\nSome rumdl text.\n<!-- rumdl-enable MD044 -->\n\nSome rumdl text outside.\n";
        let ctx = create_context(content);
        let fixed = rule.fix(&ctx).unwrap();
        // The disabled block should be preserved, the outside text fixed
        assert!(
            fixed.contains("Some rumdl text.\n"),
            "Disabled block text should be preserved"
        );
        assert!(
            fixed.contains("Some RUMDL text outside."),
            "Outside text should be fixed"
        );
    }

    /// Roundtrip safety: fix() with HTML comment content.
    #[test]
    fn test_roundtrip_fix_then_check_html_comments() {
        let config = MD044Config {
            names: vec!["JavaScript".to_string()],
            ..MD044Config::default()
        };
        let rule = MD044ProperNames::from_config_struct(config);
        let content = "# Guide\n\n<!-- javascript mentioned here -->\n\njavascript outside\n";
        let ctx = create_context(content);
        let fixed = rule.fix(&ctx).unwrap();
        let ctx2 = create_context(&fixed);
        let warnings = rule.check(&ctx2).unwrap();
        assert!(
            warnings.is_empty(),
            "Re-check after fix should produce zero warnings, got: {warnings:?}"
        );
    }

    /// Roundtrip safety: fix() preserves content when no violations exist.
    #[test]
    fn test_roundtrip_no_op_when_correct() {
        let rule = MD044ProperNames::new(vec!["JavaScript".to_string(), "TypeScript".to_string()], true);
        let content = "This uses JavaScript and TypeScript correctly.\n";
        let ctx = create_context(content);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, content, "Fix should be a no-op when content is already correct");
    }

    // --- Bare-domain link text: display text is the destination URL with scheme stripped ---

    #[test]
    fn test_bare_domain_link_text_not_flagged() {
        // `[ravencentric.github.io](https://ravencentric.github.io)` — the display text
        // is the URL with the scheme stripped; "github" here is a domain label, not a
        // reference to "GitHub" the product, and must not be corrected.
        let rule = MD044ProperNames::new(vec!["GitHub".to_string()], false);
        let content = "My site is [ravencentric.github.io](https://ravencentric.github.io).\n";
        let ctx = create_context(content);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Should not flag 'github' in a bare-domain link text that matches the link URL: {result:?}"
        );
    }

    #[test]
    fn test_bare_domain_link_text_not_fixed() {
        // fix() must not rewrite the link text when it is the bare URL hostname.
        let rule = MD044ProperNames::new(vec!["GitHub".to_string()], false);
        let content = "My site is [ravencentric.github.io](https://ravencentric.github.io).\n";
        let ctx = create_context(content);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(
            fixed, content,
            "fix() must not alter bare-domain link text that matches the destination URL"
        );
    }

    #[test]
    fn test_bare_domain_link_text_with_path_not_flagged() {
        // Display text is the hostname only; destination has a path.
        let rule = MD044ProperNames::new(vec!["GitHub".to_string()], false);
        let content = "Visit [ravencentric.github.io](https://ravencentric.github.io/projects).\n";
        let ctx = create_context(content);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Should not flag 'github' when bare-domain text is the hostname of its destination URL: {result:?}"
        );
    }

    #[test]
    fn test_bare_domain_link_text_full_path_not_flagged() {
        // Display text is the full URL-without-scheme including a path.
        let rule = MD044ProperNames::new(vec!["GitHub".to_string()], false);
        let content = "See [ravencentric.github.io/blog](https://ravencentric.github.io/blog).\n";
        let ctx = create_context(content);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Should not flag 'github' when link text is the full URL path without scheme: {result:?}"
        );
    }

    #[test]
    fn test_github_product_name_in_link_text_still_flagged() {
        // `[github pages](https://pages.github.com)` — the display text is a human
        // description, not a bare domain; "github" should still be corrected to "GitHub".
        let rule = MD044ProperNames::new(vec!["GitHub".to_string()], false);
        let content = "Hosted on [github pages](https://pages.github.com).\n";
        let ctx = create_context(content);
        let result = rule.check(&ctx).unwrap();
        assert!(
            !result.is_empty(),
            "Should still flag 'github' in descriptive link text that does not match the destination URL"
        );
    }

    #[test]
    fn test_protocol_relative_bare_domain_link_text_not_flagged() {
        // Protocol-relative URL `[github.io](//github.io)`.
        let rule = MD044ProperNames::new(vec!["GitHub".to_string()], false);
        let content = "See [github.io](//github.io).\n";
        let ctx = create_context(content);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Should not flag 'github' in bare-domain text matching a protocol-relative destination: {result:?}"
        );
    }

    #[test]
    fn test_dotted_wikilink_target_still_flagged() {
        // `[[node.js]]` is a WikiLink whose page name contains a dot.
        // The dot guard alone does not protect it because text == url == "node.js".
        // The is_in_link WikiLink guard must prevent bare-domain suppression,
        // so the improper capitalization is still caught.
        let rule = MD044ProperNames::new(vec!["Node.js".to_string()], false);
        let content = "See [[node.js]] for details.\n";
        let ctx = create_context(content);
        let result = rule.check(&ctx).unwrap();
        assert!(
            !result.is_empty(),
            "Should flag 'node.js' in a dotted WikiLink target: {result:?}"
        );
    }

    #[test]
    fn test_bare_domain_link_text_case_insensitive_url() {
        // URL with uppercase scheme `[github.io](HTTPS://github.io)` — the scheme is
        // case-insensitive, so the display text should still be recognised as a bare domain.
        let rule = MD044ProperNames::new(vec!["GitHub".to_string()], false);
        let content = "See [github.io](HTTPS://github.io).\n";
        let ctx = create_context(content);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Should not flag bare-domain text when destination URL has an uppercase scheme: {result:?}"
        );
    }

    #[test]
    fn test_frontmatter_value_span_strips_trailing_comment() {
        let line = "link: docs/guide/myapp # canonical path";
        let (s, e) = MD044ProperNames::frontmatter_value_span(line).unwrap();
        assert_eq!(&line[s..e], "docs/guide/myapp");
    }

    #[test]
    fn test_frontmatter_value_span_quoted_keeps_hash_and_spaces() {
        let line = "link: 'docs/My App/a#b'";
        let (s, e) = MD044ProperNames::frontmatter_value_span(line).unwrap();
        assert_eq!(&line[s..e], "docs/My App/a#b");
    }

    #[test]
    fn test_frontmatter_value_span_plain_value() {
        let line = "title: Heading for myapp";
        let (s, e) = MD044ProperNames::frontmatter_value_span(line).unwrap();
        assert_eq!(&line[s..e], "Heading for myapp");
    }

    #[test]
    fn test_frontmatter_value_span_none_for_key_only() {
        assert!(MD044ProperNames::frontmatter_value_span("seo:").is_none());
        assert!(MD044ProperNames::frontmatter_value_span("---").is_none());
    }

    #[test]
    fn test_frontmatter_value_span_quoted_strips_trailing_comment() {
        let line = "link: 'docs/guide' # canonical path";
        let (s, e) = MD044ProperNames::frontmatter_value_span(line).unwrap();
        assert_eq!(&line[s..e], "docs/guide");
    }

    #[test]
    fn test_frontmatter_value_span_empty_quoted_value_is_none() {
        assert!(MD044ProperNames::frontmatter_value_span("key: ''").is_none());
    }

    #[test]
    fn test_frontmatter_value_span_unterminated_quote_strips_leading_quote() {
        let line = "link: 'docs/a";
        let (s, e) = MD044ProperNames::frontmatter_value_span(line).unwrap();
        assert_eq!(&line[s..e], "docs/a");
    }

    /// Byte offset of `needle` in `line`, for locating the match under test.
    fn at(line: &str, needle: &str) -> usize {
        line.find(needle).expect("needle present")
    }

    #[test]
    fn test_path_like_exempts_single_token_frontmatter_paths() {
        for line in [
            "link: this/is/a/link/to/myapp.md",
            "link: docs/myapp.md",
            "link: /abs/path/myapp.md",
            "link: ./myapp.md",
            "link: ../shared/myapp.md",
        ] {
            let span = MD044ProperNames::frontmatter_value_span(line).unwrap();
            let pos = at(line, "myapp");
            assert!(
                MD044ProperNames::is_in_path_like_token(line, pos, span),
                "should treat as a path: {line}"
            );
        }
    }

    #[test]
    fn test_path_like_does_not_exempt_slash_conjunction_prose() {
        // Extra words around the slash-separated token mean it is not the
        // sole frontmatter value, so the 3+ segment path signal must not fire.
        let line = "description: We support github/gitlab/bitbucket imports.";
        let span = MD044ProperNames::frontmatter_value_span(line).unwrap();
        assert!(
            !MD044ProperNames::is_in_path_like_token(line, at(line, "github"), span),
            "slash-separated prose is not a path"
        );

        let line = "description: The javascript/typescript ecosystem is large.";
        let span = MD044ProperNames::frontmatter_value_span(line).unwrap();
        assert!(!MD044ProperNames::is_in_path_like_token(
            line,
            at(line, "javascript"),
            span
        ));
    }

    #[test]
    fn test_path_like_requires_a_slash_so_dotted_names_survive() {
        let line = "title: Use nodejs and myapp.md today.";
        let span = MD044ProperNames::frontmatter_value_span(line).unwrap();
        assert!(!MD044ProperNames::is_in_path_like_token(line, at(line, "myapp"), span));
    }

    #[test]
    fn test_path_like_no_slash_frontmatter_value_still_flagged() {
        // A frontmatter value with no slash at all is never a path signal,
        // regardless of it being the sole value; the mandatory slash is what
        // protects dotted proper names like `Node.js` without over-exempting
        // plain slugs.
        let line = "slug: myapp-guide";
        let span = MD044ProperNames::frontmatter_value_span(line).unwrap();
        assert!(!MD044ProperNames::is_in_path_like_token(line, at(line, "myapp"), span));
    }

    #[test]
    fn test_path_like_returns_false_outside_value_span() {
        // A match outside the frontmatter value span (e.g. in the key) is
        // rejected immediately, before any token-bound scanning happens.
        let line = "myapp: docs/guide/myapp";
        let span = MD044ProperNames::frontmatter_value_span(line).unwrap();
        let key_pos = 0;
        assert!(!MD044ProperNames::is_in_path_like_token(line, key_pos, span));
    }

    #[test]
    fn test_path_like_three_segments_only_as_sole_frontmatter_value() {
        let line = "link: docs/guide/myapp";
        let span = MD044ProperNames::frontmatter_value_span(line).unwrap();
        assert!(MD044ProperNames::is_in_path_like_token(line, at(line, "myapp"), span));

        let line = "description: We support github/gitlab/bitbucket now";
        let span = MD044ProperNames::frontmatter_value_span(line).unwrap();
        assert!(
            !MD044ProperNames::is_in_path_like_token(line, at(line, "github"), span),
            "multi-token value gets body treatment"
        );
    }

    #[test]
    fn test_path_like_quoted_value_with_spaces() {
        // The collapsed token has a real extension on its last segment, so
        // signal (b) exempts it regardless of the multi-word collapse.
        let line = "link: 'docs/My App/myapp.md'";
        let span = MD044ProperNames::frontmatter_value_span(line).unwrap();
        assert!(MD044ProperNames::is_in_path_like_token(line, at(line, "myapp"), span));
    }

    #[test]
    fn test_path_like_quoted_value_with_spaces_no_extension_not_exempt() {
        // Same shape as above but without an extension: the collapsed token
        // only has the 3+ segment signal available, which is deliberately
        // restricted to single-token values (see `is_multi_word_collapse` in
        // `is_in_path_like_token`). An extensionless path containing a
        // literal space is rare enough that this is an accepted narrowing,
        // not an oversight.
        let line = "link: 'docs/My App/myapp'";
        let span = MD044ProperNames::frontmatter_value_span(line).unwrap();
        assert!(!MD044ProperNames::is_in_path_like_token(line, at(line, "myapp"), span));
    }

    #[test]
    fn test_path_like_trailing_comment_is_still_sole_value() {
        let line = "link: docs/guide/myapp # canonical path";
        let span = MD044ProperNames::frontmatter_value_span(line).unwrap();
        assert!(MD044ProperNames::is_in_path_like_token(line, at(line, "myapp"), span));
    }

    #[test]
    fn test_path_like_trailing_punctuation_trimmed() {
        let line = "link: docs/myapp.md, then leave.";
        let span = MD044ProperNames::frontmatter_value_span(line).unwrap();
        assert!(MD044ProperNames::is_in_path_like_token(line, at(line, "myapp"), span));
    }

    #[test]
    fn test_trim_token_bounds_reaches_fixpoint_after_punctuation_exposes_wrapper() {
        let line = r#"See "docs/myapp.md", then leave."#;
        let raw_start = at(line, "\"docs");
        let raw_end = raw_start + r#""docs/myapp.md","#.len();
        assert_eq!(&line[raw_start..raw_end], r#""docs/myapp.md","#);
        let (start, end) = MD044ProperNames::trim_token_bounds(line, raw_start, raw_end);
        assert_eq!(&line[start..end], "docs/myapp.md");
    }

    #[test]
    fn test_trim_token_bounds_reaches_fixpoint_with_multiple_trailing_wrappers() {
        let line = r#"("docs/myapp.md")."#;
        let (start, end) = MD044ProperNames::trim_token_bounds(line, 0, line.len());
        assert_eq!(&line[start..end], "docs/myapp.md");
    }

    #[test]
    fn test_frontmatter_link_path_not_flagged() {
        let content = "---\ntitle: Heading for MyApp\nlink: 'this/is/a/link/to/myapp.md'\n---\n\nBody.\n";
        let rule = MD044ProperNames::new(vec!["MyApp".to_string()], false);
        let ctx = create_context(content);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "path in a frontmatter value must not be flagged: {result:?}"
        );
    }

    #[test]
    fn test_fix_does_not_corrupt_frontmatter_link_path() {
        let content = "---\nlink: 'this/is/a/link/to/myapp.md'\n---\n\nBody.\n";
        let rule = MD044ProperNames::new(vec!["MyApp".to_string()], false);
        let ctx = create_context(content);
        assert_eq!(rule.fix(&ctx).unwrap(), content, "fix must not rewrite a path");
    }

    // The next few tests document the deliberate frontmatter-only scope: a
    // body occurrence that sits inside text shaped like a file path (a
    // parenthesised disambiguator, a bracketed dynamic segment, a Next.js
    // catch-all route) is corrected exactly like any other prose occurrence,
    // and only that single word changes. Earlier attempts at exempting
    // body-prose paths corrupted these exact shapes (splitting mid-token on
    // wrapper characters, or on `[[`/`]]`/`](` sequences); asserting the
    // plain single-word correction here guards against that class of bug
    // reappearing without silently reintroducing the abandoned exemption.

    #[test]
    fn test_body_prose_parenthesized_disambiguator_is_case_corrected() {
        let content = "See docs/myapp(1).md here.\n";
        let rule = MD044ProperNames::new(vec!["MyApp".to_string()], false);
        let ctx = create_context(content);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1, "body occurrence is flagged: {result:?}");
        assert_eq!(rule.fix(&ctx).unwrap(), "See docs/MyApp(1).md here.\n");
    }

    #[test]
    fn test_body_prose_bracketed_dynamic_segment_is_case_corrected() {
        let content = "See docs/[myapp].md here.\n";
        let rule = MD044ProperNames::new(vec!["MyApp".to_string()], false);
        let ctx = create_context(content);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1, "body occurrence is flagged: {result:?}");
        assert_eq!(rule.fix(&ctx).unwrap(), "See docs/[MyApp].md here.\n");
    }

    #[test]
    fn test_body_prose_nextjs_catch_all_segment_is_case_corrected() {
        // `[[...myapp]]` is a Next.js optional catch-all route segment, not
        // WikiLink syntax; nothing in the parser treats it specially here.
        let content = "pages/[[...myapp]].tsx are catch-all routes.\n";
        let rule = MD044ProperNames::new(vec!["MyApp".to_string()], false);
        let ctx = create_context(content);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1, "body occurrence is flagged: {result:?}");
        assert_eq!(
            rule.fix(&ctx).unwrap(),
            "pages/[[...MyApp]].tsx are catch-all routes.\n"
        );
    }

    #[test]
    fn test_two_adjacent_whitespace_free_links_both_flagged() {
        // Two links with no whitespace between them, `[myapp](url)[github](url)`.
        // A body-prose tokenizer that split on markdown syntax previously let
        // the first link's boundary swallow the second, losing its flag.
        let content = "[myapp](https://a.com)[github](https://b.com)\n";
        let rule = MD044ProperNames::new(vec!["MyApp".to_string(), "GitHub".to_string()], false);
        let ctx = create_context(content);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 2, "both link texts must be flagged: {result:?}");
        assert!(result.iter().any(|w| w.message.contains("'myapp'")));
        assert!(result.iter().any(|w| w.message.contains("'github'")));
    }

    #[test]
    fn test_fix_does_not_corrupt_frontmatter_path_with_route_group_named_after_proper_name() {
        // Next.js-style route group: the parenthesised directory segment
        // itself is the proper name, e.g. `src/(myapp)/page.tsx`.
        let content = "---\nlink: src/(myapp)/page.tsx\n---\n\nBody.\n";
        let rule = MD044ProperNames::new(vec!["MyApp".to_string()], false);
        let ctx = create_context(content);
        assert_eq!(
            rule.fix(&ctx).unwrap(),
            content,
            "fix must not rewrite a frontmatter path whose route-group directory name is the proper name"
        );
    }

    #[test]
    fn test_quoted_frontmatter_value_slash_conjunction_prose_still_flagged() {
        // A quoted value is not automatically a single path token just
        // because it is quoted: "We" carries no slash, so per-word
        // tokenization applies and the slash-separated list is judged as
        // prose, not the sole value.
        let content = "---\ndescription: \"We support github/gitlab/bitbucket now\"\n---\n\nBody.\n";
        let rule = MD044ProperNames::new(vec!["GitHub".to_string()], false);
        let ctx = create_context(content);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(
            result.len(),
            1,
            "quoted prose value must still flag 'github': {result:?}"
        );
    }

    #[test]
    fn test_quoted_frontmatter_value_single_slash_word_with_unrelated_dot_still_flagged() {
        // One slash-bearing word plus an unrelated later dot (in "1.0" or
        // "e.g.") must not make the whole quoted sentence exempt as a path.
        let content = "---\ndescription: \"We use myapp/gitlab and version 1.0 e.g. weekly\"\n---\n\nBody.\n";
        let rule = MD044ProperNames::new(vec!["MyApp".to_string()], false);
        let ctx = create_context(content);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(
            result.len(),
            1,
            "quoted prose value must still flag 'myapp': {result:?}"
        );
    }

    #[test]
    fn test_quoted_toml_frontmatter_value_slash_conjunction_prose_still_flagged() {
        // TOML string values are essentially always quoted, so this class of
        // bug affects TOML frontmatter systematically.
        let content = "+++\ndescription = \"We support github/gitlab/bitbucket now\"\n+++\n\nBody.\n";
        let rule = MD044ProperNames::new(vec!["GitHub".to_string()], false);
        let ctx = create_context(content);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(
            result.len(),
            1,
            "TOML quoted prose value must still flag 'github': {result:?}"
        );
    }

    #[test]
    fn test_path_like_collapsed_multiword_no_extension_not_exempt() {
        // Every word in these quoted values carries a slash, so they collapse
        // to one token per `is_single_quoted_path`. None of the collapsed
        // tokens starts with a path prefix or ends in an extension, so the
        // 3+ segment sole-value signal must not exempt them either: it is
        // restricted to genuinely single-token values, not tokens formed by
        // collapsing several whitespace-separated words together.
        for line in [
            r#"description: "myapp/gitlab github/bitbucket""#,
            r#"description: "and/or this/that myapp/gitlab""#,
            r#"description: "he/him she/her myapp/gitlab""#,
        ] {
            let span = MD044ProperNames::frontmatter_value_span(line).unwrap();
            for needle in ["myapp", "gitlab"] {
                if let Some(byte_pos) = line.find(needle) {
                    assert!(
                        !MD044ProperNames::is_in_path_like_token(line, byte_pos, span),
                        "collapsed multi-word value must not exempt '{needle}': {line}"
                    );
                }
            }
        }
    }

    #[test]
    fn test_path_like_collapsed_multiword_no_extension_not_exempt_toml() {
        let line = r#"description = "myapp/gitlab github/bitbucket""#;
        let span = MD044ProperNames::frontmatter_value_span(line).unwrap();
        for needle in ["myapp", "gitlab", "github", "bitbucket"] {
            let byte_pos = at(line, needle);
            assert!(
                !MD044ProperNames::is_in_path_like_token(line, byte_pos, span),
                "collapsed multi-word TOML value must not exempt '{needle}'"
            );
        }
    }

    #[test]
    fn test_frontmatter_collapsed_multiword_names_all_flagged_yaml() {
        let content = "---\ndescription: \"myapp/gitlab github/bitbucket\"\n---\n\nBody.\n";
        let rule = MD044ProperNames::new(
            vec![
                "MyApp".to_string(),
                "GitLab".to_string(),
                "GitHub".to_string(),
                "Bitbucket".to_string(),
            ],
            false,
        );
        let ctx = create_context(content);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(
            result.len(),
            4,
            "all four names in the collapsed multi-word value must be flagged: {result:?}"
        );
    }

    #[test]
    fn test_frontmatter_collapsed_multiword_names_all_flagged_toml() {
        let content = "+++\ndescription = \"myapp/gitlab github/bitbucket\"\n+++\n\nBody.\n";
        let rule = MD044ProperNames::new(
            vec![
                "MyApp".to_string(),
                "GitLab".to_string(),
                "GitHub".to_string(),
                "Bitbucket".to_string(),
            ],
            false,
        );
        let ctx = create_context(content);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(
            result.len(),
            4,
            "all four names in the collapsed multi-word TOML value must be flagged: {result:?}"
        );
    }

    #[test]
    fn test_frontmatter_collapsed_multiword_conjunction_pairs_flagged() {
        let content = "---\ndescription: \"and/or this/that myapp/gitlab\"\n---\n\nBody.\n";
        let rule = MD044ProperNames::new(vec!["MyApp".to_string(), "GitLab".to_string()], false);
        let ctx = create_context(content);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(
            result.len(),
            2,
            "myapp and gitlab must both be flagged despite the surrounding slash pairs: {result:?}"
        );
    }

    #[test]
    fn test_frontmatter_collapsed_multiword_pronoun_pairs_flagged() {
        let content = "---\ndescription: \"he/him she/her myapp/gitlab\"\n---\n\nBody.\n";
        let rule = MD044ProperNames::new(vec!["MyApp".to_string(), "GitLab".to_string()], false);
        let ctx = create_context(content);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(
            result.len(),
            2,
            "myapp and gitlab must both be flagged despite the surrounding slash pairs: {result:?}"
        );
    }

    /// The path exemption is scoped to frontmatter only, by design: a body
    /// occurrence sitting inside what looks like a file path is still
    /// flagged and fixed like any other prose occurrence. This is a
    /// deliberate limit (see `is_in_path_like_token`), not an oversight, so a
    /// future reader does not "fix" it back into hand-rolled body tokenizing.
    #[test]
    fn test_body_prose_path_is_flagged_frontmatter_only_scope() {
        let content = "See docs/myapp.md for details about myapp.\n";
        let rule = MD044ProperNames::new(vec!["MyApp".to_string()], false);
        let ctx = create_context(content);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(
            result.len(),
            2,
            "both the path occurrence and the prose occurrence are flagged in body text: {result:?}"
        );
        assert_eq!(rule.fix(&ctx).unwrap(), "See docs/MyApp.md for details about MyApp.\n");
    }

    #[test]
    fn test_slash_conjunction_prose_still_flagged() {
        let content = "We support github/gitlab imports.\n";
        let rule = MD044ProperNames::new(vec!["GitHub".to_string()], false);
        let ctx = create_context(content);
        assert_eq!(rule.check(&ctx).unwrap().len(), 1);
    }
}
