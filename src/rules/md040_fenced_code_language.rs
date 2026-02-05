use crate::linguist_data::{default_alias, get_aliases, is_valid_alias, resolve_canonical};
use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::rule_config_serde::{RuleConfig, load_rule_config};
use crate::utils::range_utils::calculate_line_range;
use pulldown_cmark::{CodeBlockKind, Event, Options, Parser, Tag};
use std::collections::HashMap;

/// Rule MD040: Fenced code blocks should have a language
///
/// See [docs/md040.md](../../docs/md040.md) for full documentation, configuration, and examples.
pub mod md040_config;

// ============================================================================
// MkDocs Superfences Attribute Detection
// ============================================================================

/// Prefixes that indicate MkDocs superfences attributes rather than language identifiers.
/// These are valid in MkDocs flavor without a language specification.
/// See: https://facelessuser.github.io/pymdown-extensions/extensions/superfences/
const MKDOCS_SUPERFENCES_ATTR_PREFIXES: &[&str] = &[
    "title=",    // Block title
    "hl_lines=", // Highlighted lines
    "linenums=", // Line numbers
    ".",         // CSS class (e.g., .annotate)
    "#",         // CSS id
];

/// Check if a string starts with a MkDocs superfences attribute prefix
#[inline]
fn is_superfences_attribute(s: &str) -> bool {
    MKDOCS_SUPERFENCES_ATTR_PREFIXES
        .iter()
        .any(|prefix| s.starts_with(prefix))
}
use md040_config::{LanguageStyle, MD040Config, UnknownLanguageAction};

struct FencedCodeBlock {
    /// 0-indexed line number where the code block starts
    line_idx: usize,
    /// The language/info string (empty if no language specified)
    language: String,
    /// The fence marker used (``` or ~~~)
    fence_marker: String,
}

#[derive(Debug, Clone, Default)]
pub struct MD040FencedCodeLanguage {
    config: MD040Config,
}

impl MD040FencedCodeLanguage {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_config(config: MD040Config) -> Self {
        Self { config }
    }

    /// Validate the configuration and return any errors
    fn validate_config(&self) -> Vec<String> {
        let mut errors = Vec::new();

        // Validate preferred-aliases: check that each alias is valid for its language
        for (canonical, alias) in &self.config.preferred_aliases {
            // Find the actual canonical name (case-insensitive)
            if let Some(actual_canonical) = resolve_canonical(canonical) {
                if !is_valid_alias(actual_canonical, alias)
                    && let Some(valid_aliases) = get_aliases(actual_canonical)
                {
                    let valid_list: Vec<_> = valid_aliases.iter().take(5).collect();
                    let valid_str = valid_list
                        .iter()
                        .map(|s| format!("'{s}'"))
                        .collect::<Vec<_>>()
                        .join(", ");
                    let suffix = if valid_aliases.len() > 5 { ", ..." } else { "" };
                    errors.push(format!(
                        "Invalid alias '{alias}' for language '{actual_canonical}'. Valid aliases include: {valid_str}{suffix}"
                    ));
                }
            } else {
                errors.push(format!(
                    "Unknown language '{canonical}' in preferred-aliases. Use GitHub Linguist canonical names."
                ));
            }
        }

        errors
    }

    /// Determine the preferred label for each canonical language in the document
    fn compute_preferred_labels(
        &self,
        blocks: &[FencedCodeBlock],
        disabled_ranges: &[(usize, usize)],
    ) -> HashMap<String, String> {
        // Group labels by canonical language
        let mut by_canonical: HashMap<String, Vec<&str>> = HashMap::new();

        for block in blocks {
            if is_line_disabled(disabled_ranges, block.line_idx) {
                continue;
            }
            if block.language.is_empty() {
                continue;
            }
            if let Some(canonical) = resolve_canonical(&block.language) {
                by_canonical
                    .entry(canonical.to_string())
                    .or_default()
                    .push(&block.language);
            }
        }

        // Determine winning label for each canonical language
        let mut result = HashMap::new();

        for (canonical, labels) in by_canonical {
            // Check for user override first (case-insensitive lookup)
            let winner = if let Some(preferred) = self
                .config
                .preferred_aliases
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case(&canonical))
                .map(|(_, v)| v.clone())
            {
                preferred
            } else {
                // Find most prevalent label
                let mut counts: HashMap<&str, usize> = HashMap::new();
                for label in &labels {
                    *counts.entry(*label).or_default() += 1;
                }

                let max_count = counts.values().max().copied().unwrap_or(0);
                let winners: Vec<_> = counts
                    .iter()
                    .filter(|(_, c)| **c == max_count)
                    .map(|(l, _)| *l)
                    .collect();

                if winners.len() == 1 {
                    winners[0].to_string()
                } else {
                    // Tie-break: use curated default if available, otherwise alphabetically first
                    default_alias(&canonical)
                        .filter(|default| winners.contains(default))
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| winners.into_iter().min().unwrap().to_string())
                }
            };

            result.insert(canonical, winner);
        }

        result
    }

    /// Check if a language is allowed based on config
    fn check_language_allowed(&self, canonical: Option<&str>, original_label: &str) -> Option<String> {
        // Allowlist takes precedence
        if !self.config.allowed_languages.is_empty() {
            let allowed = self.config.allowed_languages.join(", ");
            let Some(canonical) = canonical else {
                return Some(format!(
                    "Language '{original_label}' is not in the allowed list: {allowed}"
                ));
            };
            if !self
                .config
                .allowed_languages
                .iter()
                .any(|a| a.eq_ignore_ascii_case(canonical))
            {
                return Some(format!(
                    "Language '{original_label}' ({canonical}) is not in the allowed list: {allowed}"
                ));
            }
        } else if !self.config.disallowed_languages.is_empty()
            && canonical.is_some_and(|canonical| {
                self.config
                    .disallowed_languages
                    .iter()
                    .any(|d| d.eq_ignore_ascii_case(canonical))
            })
        {
            let canonical = canonical.unwrap_or("unknown");
            return Some(format!("Language '{original_label}' ({canonical}) is disallowed"));
        }
        None
    }

    /// Check for unknown language based on config
    fn check_unknown_language(&self, label: &str) -> Option<(String, Severity)> {
        if resolve_canonical(label).is_some() {
            return None;
        }

        match self.config.unknown_language_action {
            UnknownLanguageAction::Ignore => None,
            UnknownLanguageAction::Warn => Some((
                format!("Unknown language '{label}' (not in GitHub Linguist). Syntax highlighting may not work."),
                Severity::Warning,
            )),
            UnknownLanguageAction::Error => Some((
                format!("Unknown language '{label}' (not in GitHub Linguist)"),
                Severity::Error,
            )),
        }
    }
}

impl Rule for MD040FencedCodeLanguage {
    fn name(&self) -> &'static str {
        "MD040"
    }

    fn description(&self) -> &'static str {
        "Code blocks should have a language specified"
    }

    fn check(&self, ctx: &crate::lint_context::LintContext) -> LintResult {
        let content = ctx.content;
        let mut warnings = Vec::new();

        // Validate config and emit warnings for invalid configuration
        for error in self.validate_config() {
            warnings.push(LintWarning {
                rule_name: Some(self.name().to_string()),
                line: 0,
                column: 0,
                end_line: 0,
                end_column: 0,
                message: format!("[config error] {error}"),
                severity: Severity::Error,
                fix: None,
            });
        }

        // Use pulldown-cmark to detect fenced code blocks with language info
        let fenced_blocks = detect_fenced_code_blocks(content, &ctx.line_offsets);

        // Pre-compute disabled ranges for efficient lookup
        let disabled_ranges = compute_disabled_ranges(content, self.name());

        // Compute preferred labels for consistent mode
        let preferred_labels = if self.config.style == LanguageStyle::Consistent {
            self.compute_preferred_labels(&fenced_blocks, &disabled_ranges)
        } else {
            HashMap::new()
        };

        for block in &fenced_blocks {
            // Skip if this line is in a disabled range
            if is_line_disabled(&disabled_ranges, block.line_idx) {
                continue;
            }

            // Get the actual line content for additional checks
            let line = content.lines().nth(block.line_idx).unwrap_or("");
            let trimmed = line.trim();
            let after_fence = trimmed.strip_prefix(&block.fence_marker).unwrap_or("").trim();

            // Check if fence has MkDocs superfences attributes but no language
            let has_mkdocs_attrs_only =
                ctx.flavor == crate::config::MarkdownFlavor::MkDocs && is_superfences_attribute(after_fence);

            // Check for Quarto/RMarkdown code chunk syntax: {language} or {language, options}
            let has_quarto_syntax = ctx.flavor == crate::config::MarkdownFlavor::Quarto
                && after_fence.starts_with('{')
                && after_fence.contains('}');

            // Determine if this block needs a language specification
            // In MkDocs flavor, superfences attributes without language are acceptable
            let needs_language =
                !has_mkdocs_attrs_only && (block.language.is_empty() || is_superfences_attribute(&block.language));

            if needs_language && !has_quarto_syntax {
                let (start_line, start_col, end_line, end_col) = calculate_line_range(block.line_idx + 1, line);

                warnings.push(LintWarning {
                    rule_name: Some(self.name().to_string()),
                    line: start_line,
                    column: start_col,
                    end_line,
                    end_column: end_col,
                    message: "Code block (```) missing language".to_string(),
                    severity: Severity::Warning,
                    fix: Some(Fix {
                        range: {
                            let trimmed_start = line.len() - line.trim_start().len();
                            let fence_len = block.fence_marker.len();
                            let line_start_byte = ctx.line_offsets.get(block.line_idx).copied().unwrap_or(0);
                            let fence_start_byte = line_start_byte + trimmed_start;
                            let fence_end_byte = fence_start_byte + fence_len;
                            fence_start_byte..fence_end_byte
                        },
                        replacement: format!("{}text", block.fence_marker),
                    }),
                });
                continue;
            }

            // Skip further checks for special syntax
            if has_quarto_syntax {
                continue;
            }

            let canonical = resolve_canonical(&block.language);

            // Check language restrictions (allowlist/denylist)
            if let Some(msg) = self.check_language_allowed(canonical, &block.language) {
                let (start_line, start_col, end_line, end_col) = calculate_line_range(block.line_idx + 1, line);

                warnings.push(LintWarning {
                    rule_name: Some(self.name().to_string()),
                    line: start_line,
                    column: start_col,
                    end_line,
                    end_column: end_col,
                    message: msg,
                    severity: Severity::Warning,
                    fix: None,
                });
                continue;
            }

            // Check for unknown language (only if not handled by allowlist)
            if canonical.is_none() {
                if let Some((msg, severity)) = self.check_unknown_language(&block.language) {
                    let (start_line, start_col, end_line, end_col) = calculate_line_range(block.line_idx + 1, line);

                    warnings.push(LintWarning {
                        rule_name: Some(self.name().to_string()),
                        line: start_line,
                        column: start_col,
                        end_line,
                        end_column: end_col,
                        message: msg,
                        severity,
                        fix: None,
                    });
                }
                continue;
            }

            // Check consistency
            if self.config.style == LanguageStyle::Consistent
                && let Some(preferred) = preferred_labels.get(canonical.unwrap())
                && &block.language != preferred
            {
                let (start_line, start_col, end_line, end_col) = calculate_line_range(block.line_idx + 1, line);

                let fix = find_label_span(line, &block.fence_marker).map(|(label_start, label_end)| {
                    let line_start_byte = ctx.line_offsets.get(block.line_idx).copied().unwrap_or(0);
                    Fix {
                        range: (line_start_byte + label_start)..(line_start_byte + label_end),
                        replacement: preferred.clone(),
                    }
                });
                let lang = &block.language;
                let canonical = canonical.unwrap();

                warnings.push(LintWarning {
                    rule_name: Some(self.name().to_string()),
                    line: start_line,
                    column: start_col,
                    end_line,
                    end_column: end_col,
                    message: format!("Inconsistent language label '{lang}' for {canonical} (use '{preferred}')"),
                    severity: Severity::Warning,
                    fix,
                });
            }
        }

        Ok(warnings)
    }

    fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, LintError> {
        let content = ctx.content;

        // Use pulldown-cmark to detect fenced code blocks
        let fenced_blocks = detect_fenced_code_blocks(content, &ctx.line_offsets);

        // Pre-compute disabled ranges
        let disabled_ranges = compute_disabled_ranges(content, self.name());

        // Compute preferred labels for consistent mode
        let preferred_labels = if self.config.style == LanguageStyle::Consistent {
            self.compute_preferred_labels(&fenced_blocks, &disabled_ranges)
        } else {
            HashMap::new()
        };

        // Build a set of line indices that need fixing
        let mut lines_to_fix: std::collections::HashMap<usize, FixAction> = std::collections::HashMap::new();

        for block in &fenced_blocks {
            if is_line_disabled(&disabled_ranges, block.line_idx) {
                continue;
            }

            let line = content.lines().nth(block.line_idx).unwrap_or("");
            let trimmed = line.trim();
            let after_fence = trimmed.strip_prefix(&block.fence_marker).unwrap_or("").trim();

            let has_mkdocs_attrs_only =
                ctx.flavor == crate::config::MarkdownFlavor::MkDocs && is_superfences_attribute(after_fence);

            let has_quarto_syntax = ctx.flavor == crate::config::MarkdownFlavor::Quarto
                && after_fence.starts_with('{')
                && after_fence.contains('}');

            let needs_language =
                !has_mkdocs_attrs_only && (block.language.is_empty() || is_superfences_attribute(&block.language));

            if needs_language && !has_quarto_syntax {
                lines_to_fix.insert(
                    block.line_idx,
                    FixAction::AddLanguage {
                        fence_marker: block.fence_marker.clone(),
                        has_mkdocs_attrs_only,
                    },
                );
            } else if !has_quarto_syntax
                && self.config.style == LanguageStyle::Consistent
                && let Some(canonical) = resolve_canonical(&block.language)
                && let Some(preferred) = preferred_labels.get(canonical)
                && &block.language != preferred
            {
                lines_to_fix.insert(
                    block.line_idx,
                    FixAction::NormalizeLabel {
                        fence_marker: block.fence_marker.clone(),
                        new_label: preferred.clone(),
                    },
                );
            }
        }

        // Build the result by iterating through lines
        let mut result = String::new();
        for (i, line) in content.lines().enumerate() {
            if let Some(action) = lines_to_fix.get(&i) {
                match action {
                    FixAction::AddLanguage {
                        fence_marker,
                        has_mkdocs_attrs_only,
                    } => {
                        let indent = &line[..line.len() - line.trim_start().len()];
                        let trimmed = line.trim();
                        let after_fence = trimmed.strip_prefix(fence_marker).unwrap_or("").trim();

                        if *has_mkdocs_attrs_only {
                            result.push_str(&format!("{indent}{fence_marker}text {after_fence}\n"));
                        } else {
                            result.push_str(&format!("{indent}{fence_marker}text\n"));
                        }
                    }
                    FixAction::NormalizeLabel {
                        fence_marker,
                        new_label,
                    } => {
                        if let Some((label_start, label_end)) = find_label_span(line, fence_marker) {
                            result.push_str(&line[..label_start]);
                            result.push_str(new_label);
                            result.push_str(&line[label_end..]);
                            result.push('\n');
                        } else {
                            result.push_str(line);
                            result.push('\n');
                        }
                    }
                }
            } else {
                result.push_str(line);
                result.push('\n');
            }
        }

        // Remove trailing newline if the original content didn't have one
        if !content.ends_with('\n') {
            result.pop();
        }

        Ok(result)
    }

    /// Get the category of this rule for selective processing
    fn category(&self) -> RuleCategory {
        RuleCategory::CodeBlock
    }

    /// Check if this rule should be skipped
    fn should_skip(&self, ctx: &crate::lint_context::LintContext) -> bool {
        ctx.content.is_empty() || (!ctx.likely_has_code() && !ctx.has_char('~'))
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn default_config_section(&self) -> Option<(String, toml::Value)> {
        let default_config = MD040Config::default();
        let json_value = serde_json::to_value(&default_config).ok()?;
        let toml_value = crate::rule_config_serde::json_to_toml_value(&json_value)?;

        if let toml::Value::Table(table) = toml_value {
            if !table.is_empty() {
                Some((MD040Config::RULE_NAME.to_string(), toml::Value::Table(table)))
            } else {
                None
            }
        } else {
            None
        }
    }

    fn from_config(config: &crate::config::Config) -> Box<dyn Rule>
    where
        Self: Sized,
    {
        let rule_config: MD040Config = load_rule_config(config);
        Box::new(MD040FencedCodeLanguage::with_config(rule_config))
    }
}

#[derive(Debug, Clone)]
enum FixAction {
    AddLanguage {
        fence_marker: String,
        has_mkdocs_attrs_only: bool,
    },
    NormalizeLabel {
        fence_marker: String,
        new_label: String,
    },
}

/// Detect fenced code blocks using pulldown-cmark, returning info about each block's opening fence
fn detect_fenced_code_blocks(content: &str, line_offsets: &[usize]) -> Vec<FencedCodeBlock> {
    let mut blocks = Vec::new();
    let options = Options::all();
    let parser = Parser::new_ext(content, options).into_offset_iter();

    for (event, range) in parser {
        if let Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(info))) = event {
            // Find the line index for this byte offset
            let line_idx = line_idx_from_offset(line_offsets, range.start);

            // Determine fence marker from the actual line content
            let line_start = line_offsets.get(line_idx).copied().unwrap_or(0);
            let line_end = line_offsets.get(line_idx + 1).copied().unwrap_or(content.len());
            let line = content.get(line_start..line_end).unwrap_or("");
            let trimmed = line.trim();
            let fence_marker = if trimmed.starts_with('`') {
                let count = trimmed.chars().take_while(|&c| c == '`').count();
                "`".repeat(count)
            } else if trimmed.starts_with('~') {
                let count = trimmed.chars().take_while(|&c| c == '~').count();
                "~".repeat(count)
            } else {
                "```".to_string() // Fallback
            };

            // Extract just the language (first word of info string)
            let language = info.split_whitespace().next().unwrap_or("").to_string();

            blocks.push(FencedCodeBlock {
                line_idx,
                language,
                fence_marker,
            });
        }
    }

    blocks
}

#[inline]
fn line_idx_from_offset(line_offsets: &[usize], offset: usize) -> usize {
    match line_offsets.binary_search(&offset) {
        Ok(idx) => idx,
        Err(idx) => idx.saturating_sub(1),
    }
}

/// Compute disabled line ranges from disable/enable comments
fn compute_disabled_ranges(content: &str, rule_name: &str) -> Vec<(usize, usize)> {
    let mut ranges = Vec::new();
    let mut disabled_start: Option<usize> = None;

    for (i, line) in content.lines().enumerate() {
        let trimmed = line.trim();

        if let Some(rules) = crate::rule::parse_disable_comment(trimmed)
            && (rules.is_empty() || rules.contains(&rule_name))
            && disabled_start.is_none()
        {
            disabled_start = Some(i);
        }

        if let Some(rules) = crate::rule::parse_enable_comment(trimmed)
            && (rules.is_empty() || rules.contains(&rule_name))
            && let Some(start) = disabled_start.take()
        {
            ranges.push((start, i));
        }
    }

    // Handle unclosed disable
    if let Some(start) = disabled_start {
        ranges.push((start, usize::MAX));
    }

    ranges
}

/// Check if a line index is within a disabled range
fn is_line_disabled(ranges: &[(usize, usize)], line_idx: usize) -> bool {
    ranges.iter().any(|&(start, end)| line_idx >= start && line_idx < end)
}

/// Find the byte span of the language label in a fence line.
fn find_label_span(line: &str, fence_marker: &str) -> Option<(usize, usize)> {
    let trimmed_start = line.len() - line.trim_start().len();
    let after_indent = &line[trimmed_start..];
    if !after_indent.starts_with(fence_marker) {
        return None;
    }
    let after_fence = &after_indent[fence_marker.len()..];

    let label_start_rel = after_fence
        .char_indices()
        .find(|&(_, ch)| !ch.is_whitespace())
        .map(|(idx, _)| idx)?;
    let after_label = &after_fence[label_start_rel..];
    let label_end_rel = after_label
        .char_indices()
        .find(|&(_, ch)| ch.is_whitespace())
        .map(|(idx, _)| label_start_rel + idx)
        .unwrap_or(after_fence.len());

    Some((
        trimmed_start + fence_marker.len() + label_start_rel,
        trimmed_start + fence_marker.len() + label_end_rel,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lint_context::LintContext;

    fn run_check(content: &str) -> LintResult {
        let rule = MD040FencedCodeLanguage::default();
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        rule.check(&ctx)
    }

    fn run_check_with_config(content: &str, config: MD040Config) -> LintResult {
        let rule = MD040FencedCodeLanguage::with_config(config);
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        rule.check(&ctx)
    }

    fn run_fix(content: &str) -> Result<String, LintError> {
        let rule = MD040FencedCodeLanguage::default();
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        rule.fix(&ctx)
    }

    fn run_fix_with_config(content: &str, config: MD040Config) -> Result<String, LintError> {
        let rule = MD040FencedCodeLanguage::with_config(config);
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        rule.fix(&ctx)
    }

    fn run_check_mkdocs(content: &str) -> LintResult {
        let rule = MD040FencedCodeLanguage::default();
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::MkDocs, None);
        rule.check(&ctx)
    }

    // =========================================================================
    // Basic functionality tests
    // =========================================================================

    #[test]
    fn test_code_blocks_with_language_specified() {
        let content = r#"# Test

```python
print("Hello, world!")
```

```javascript
console.log("Hello!");
```
"#;
        let result = run_check(content).unwrap();
        assert!(result.is_empty(), "No warnings expected for code blocks with language");
    }

    #[test]
    fn test_code_blocks_without_language() {
        let content = r#"# Test

```
print("Hello, world!")
```
"#;
        let result = run_check(content).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].message, "Code block (```) missing language");
        assert_eq!(result[0].line, 3);
    }

    #[test]
    fn test_fix_method_adds_text_language() {
        let content = r#"# Test

```
code without language
```

```python
already has language
```

```
another block without
```
"#;
        let fixed = run_fix(content).unwrap();
        assert!(fixed.contains("```text"));
        assert!(fixed.contains("```python"));
        assert_eq!(fixed.matches("```text").count(), 2);
    }

    #[test]
    fn test_fix_preserves_indentation() {
        let content = r#"# Test

- List item
  ```
  indented code block
  ```
"#;
        let fixed = run_fix(content).unwrap();
        assert!(fixed.contains("  ```text"));
    }

    // =========================================================================
    // Consistent mode tests
    // =========================================================================

    #[test]
    fn test_consistent_mode_detects_inconsistency() {
        let content = r#"```bash
echo hi
```

```sh
echo there
```

```bash
echo again
```
"#;
        let config = MD040Config {
            style: LanguageStyle::Consistent,
            ..Default::default()
        };
        let result = run_check_with_config(content, config).unwrap();
        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("Inconsistent"));
        assert!(result[0].message.contains("sh"));
        assert!(result[0].message.contains("bash"));
    }

    #[test]
    fn test_consistent_mode_fix_normalizes() {
        let content = r#"```bash
echo hi
```

```sh
echo there
```

```bash
echo again
```
"#;
        let config = MD040Config {
            style: LanguageStyle::Consistent,
            ..Default::default()
        };
        let fixed = run_fix_with_config(content, config).unwrap();
        assert_eq!(fixed.matches("```bash").count(), 3);
        assert_eq!(fixed.matches("```sh").count(), 0);
    }

    #[test]
    fn test_consistent_mode_tie_break_uses_curated_default() {
        // When there's a tie (1 bash, 1 sh), should use curated default (bash)
        let content = r#"```bash
echo hi
```

```sh
echo there
```
"#;
        let config = MD040Config {
            style: LanguageStyle::Consistent,
            ..Default::default()
        };
        let fixed = run_fix_with_config(content, config).unwrap();
        // bash is the curated default for Shell
        assert_eq!(fixed.matches("```bash").count(), 2);
    }

    #[test]
    fn test_consistent_mode_with_preferred_alias() {
        let content = r#"```bash
echo hi
```

```sh
echo there
```
"#;
        let mut preferred = HashMap::new();
        preferred.insert("Shell".to_string(), "sh".to_string());

        let config = MD040Config {
            style: LanguageStyle::Consistent,
            preferred_aliases: preferred,
            ..Default::default()
        };
        let fixed = run_fix_with_config(content, config).unwrap();
        assert_eq!(fixed.matches("```sh").count(), 2);
        assert_eq!(fixed.matches("```bash").count(), 0);
    }

    #[test]
    fn test_consistent_mode_ignores_disabled_blocks() {
        let content = r#"```bash
echo hi
```
<!-- rumdl-disable MD040 -->
```sh
echo there
```
```sh
echo again
```
<!-- rumdl-enable MD040 -->
"#;
        let config = MD040Config {
            style: LanguageStyle::Consistent,
            ..Default::default()
        };
        let result = run_check_with_config(content, config).unwrap();
        assert!(result.is_empty(), "Disabled blocks should not affect consistency");
    }

    #[test]
    fn test_fix_preserves_attributes() {
        let content = "```sh {.highlight}\ncode\n```\n\n```bash\nmore\n```";
        let config = MD040Config {
            style: LanguageStyle::Consistent,
            ..Default::default()
        };
        let fixed = run_fix_with_config(content, config).unwrap();
        assert!(fixed.contains("```bash {.highlight}"));
    }

    #[test]
    fn test_fix_preserves_spacing_before_label() {
        let content = "```bash\ncode\n```\n\n```  sh {.highlight}\ncode\n```";
        let config = MD040Config {
            style: LanguageStyle::Consistent,
            ..Default::default()
        };
        let fixed = run_fix_with_config(content, config).unwrap();
        assert!(fixed.contains("```  bash {.highlight}"));
        assert!(!fixed.contains("```  sh {.highlight}"));
    }

    // =========================================================================
    // Allowlist/denylist tests
    // =========================================================================

    #[test]
    fn test_allowlist_blocks_unlisted() {
        let content = "```java\ncode\n```";
        let config = MD040Config {
            allowed_languages: vec!["Python".to_string(), "Shell".to_string()],
            ..Default::default()
        };
        let result = run_check_with_config(content, config).unwrap();
        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("not in the allowed list"));
    }

    #[test]
    fn test_allowlist_allows_listed() {
        let content = "```python\ncode\n```";
        let config = MD040Config {
            allowed_languages: vec!["Python".to_string()],
            ..Default::default()
        };
        let result = run_check_with_config(content, config).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_allowlist_blocks_unknown_language() {
        let content = "```mysterylang\ncode\n```";
        let config = MD040Config {
            allowed_languages: vec!["Python".to_string()],
            ..Default::default()
        };
        let result = run_check_with_config(content, config).unwrap();
        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("allowed list"));
    }

    #[test]
    fn test_allowlist_case_insensitive() {
        let content = "```python\ncode\n```";
        let config = MD040Config {
            allowed_languages: vec!["PYTHON".to_string()],
            ..Default::default()
        };
        let result = run_check_with_config(content, config).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_denylist_blocks_listed() {
        let content = "```java\ncode\n```";
        let config = MD040Config {
            disallowed_languages: vec!["Java".to_string()],
            ..Default::default()
        };
        let result = run_check_with_config(content, config).unwrap();
        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("disallowed"));
    }

    #[test]
    fn test_denylist_allows_unlisted() {
        let content = "```python\ncode\n```";
        let config = MD040Config {
            disallowed_languages: vec!["Java".to_string()],
            ..Default::default()
        };
        let result = run_check_with_config(content, config).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_allowlist_takes_precedence_over_denylist() {
        let content = "```python\ncode\n```";
        let config = MD040Config {
            allowed_languages: vec!["Python".to_string()],
            disallowed_languages: vec!["Python".to_string()], // Should be ignored
            ..Default::default()
        };
        let result = run_check_with_config(content, config).unwrap();
        assert!(result.is_empty());
    }

    // =========================================================================
    // Unknown language tests
    // =========================================================================

    #[test]
    fn test_unknown_language_ignore_default() {
        let content = "```mycustomlang\ncode\n```";
        let result = run_check(content).unwrap();
        assert!(result.is_empty(), "Unknown languages ignored by default");
    }

    #[test]
    fn test_unknown_language_warn() {
        let content = "```mycustomlang\ncode\n```";
        let config = MD040Config {
            unknown_language_action: UnknownLanguageAction::Warn,
            ..Default::default()
        };
        let result = run_check_with_config(content, config).unwrap();
        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("Unknown language"));
        assert!(result[0].message.contains("mycustomlang"));
        assert_eq!(result[0].severity, Severity::Warning);
    }

    #[test]
    fn test_unknown_language_error() {
        let content = "```mycustomlang\ncode\n```";
        let config = MD040Config {
            unknown_language_action: UnknownLanguageAction::Error,
            ..Default::default()
        };
        let result = run_check_with_config(content, config).unwrap();
        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("Unknown language"));
        assert_eq!(result[0].severity, Severity::Error);
    }

    // =========================================================================
    // Config validation tests
    // =========================================================================

    #[test]
    fn test_invalid_preferred_alias_detected() {
        let mut preferred = HashMap::new();
        preferred.insert("Shell".to_string(), "invalid_alias".to_string());

        let config = MD040Config {
            style: LanguageStyle::Consistent,
            preferred_aliases: preferred,
            ..Default::default()
        };
        let rule = MD040FencedCodeLanguage::with_config(config);
        let errors = rule.validate_config();
        assert_eq!(errors.len(), 1);
        assert!(errors[0].contains("Invalid alias"));
        assert!(errors[0].contains("invalid_alias"));
    }

    #[test]
    fn test_unknown_language_in_preferred_aliases_detected() {
        let mut preferred = HashMap::new();
        preferred.insert("NotARealLanguage".to_string(), "nope".to_string());

        let config = MD040Config {
            style: LanguageStyle::Consistent,
            preferred_aliases: preferred,
            ..Default::default()
        };
        let rule = MD040FencedCodeLanguage::with_config(config);
        let errors = rule.validate_config();
        assert_eq!(errors.len(), 1);
        assert!(errors[0].contains("Unknown language"));
    }

    #[test]
    fn test_valid_preferred_alias_accepted() {
        let mut preferred = HashMap::new();
        preferred.insert("Shell".to_string(), "bash".to_string());
        preferred.insert("JavaScript".to_string(), "js".to_string());

        let config = MD040Config {
            style: LanguageStyle::Consistent,
            preferred_aliases: preferred,
            ..Default::default()
        };
        let rule = MD040FencedCodeLanguage::with_config(config);
        let errors = rule.validate_config();
        assert!(errors.is_empty());
    }

    // =========================================================================
    // Linguist resolution tests
    // =========================================================================

    #[test]
    fn test_linguist_resolution() {
        assert_eq!(resolve_canonical("bash"), Some("Shell"));
        assert_eq!(resolve_canonical("sh"), Some("Shell"));
        assert_eq!(resolve_canonical("zsh"), Some("Shell"));
        assert_eq!(resolve_canonical("js"), Some("JavaScript"));
        assert_eq!(resolve_canonical("python"), Some("Python"));
        assert_eq!(resolve_canonical("unknown_lang"), None);
    }

    #[test]
    fn test_linguist_resolution_case_insensitive() {
        assert_eq!(resolve_canonical("BASH"), Some("Shell"));
        assert_eq!(resolve_canonical("Bash"), Some("Shell"));
        assert_eq!(resolve_canonical("Python"), Some("Python"));
        assert_eq!(resolve_canonical("PYTHON"), Some("Python"));
    }

    #[test]
    fn test_alias_validation() {
        assert!(is_valid_alias("Shell", "bash"));
        assert!(is_valid_alias("Shell", "sh"));
        assert!(is_valid_alias("Shell", "zsh"));
        assert!(!is_valid_alias("Shell", "python"));
        assert!(!is_valid_alias("Shell", "invalid"));
    }

    #[test]
    fn test_default_alias() {
        assert_eq!(default_alias("Shell"), Some("bash"));
        assert_eq!(default_alias("JavaScript"), Some("js"));
        assert_eq!(default_alias("Python"), Some("python"));
    }

    // =========================================================================
    // Edge case tests
    // =========================================================================

    #[test]
    fn test_mixed_case_labels_normalized() {
        let content = r#"```BASH
echo hi
```

```Bash
echo there
```

```bash
echo again
```
"#;
        let config = MD040Config {
            style: LanguageStyle::Consistent,
            ..Default::default()
        };
        // All should resolve to Shell, most prevalent should win
        let result = run_check_with_config(content, config).unwrap();
        // "bash" appears 1x, "Bash" appears 1x, "BASH" appears 1x
        // All are different strings, so there's a 3-way tie
        // Should pick curated default "bash" or alphabetically first
        assert!(result.len() >= 2, "Should flag at least 2 inconsistent labels");
    }

    #[test]
    fn test_multiple_languages_independent() {
        let content = r#"```bash
shell code
```

```python
python code
```

```sh
more shell
```

```python3
more python
```
"#;
        let config = MD040Config {
            style: LanguageStyle::Consistent,
            ..Default::default()
        };
        let result = run_check_with_config(content, config).unwrap();
        // Should have 2 warnings: one for sh (inconsistent with bash) and one for python3 (inconsistent with python)
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_tilde_fences() {
        let content = r#"~~~bash
echo hi
~~~

~~~sh
echo there
~~~
"#;
        let config = MD040Config {
            style: LanguageStyle::Consistent,
            ..Default::default()
        };
        let result = run_check_with_config(content, config.clone()).unwrap();
        assert_eq!(result.len(), 1);

        let fixed = run_fix_with_config(content, config).unwrap();
        assert!(fixed.contains("~~~bash"));
        assert!(!fixed.contains("~~~sh"));
    }

    #[test]
    fn test_longer_fence_markers_preserved() {
        let content = "````sh\ncode\n````\n\n```bash\ncode\n```";
        let config = MD040Config {
            style: LanguageStyle::Consistent,
            ..Default::default()
        };
        let fixed = run_fix_with_config(content, config).unwrap();
        assert!(fixed.contains("````bash"));
        assert!(fixed.contains("```bash"));
    }

    #[test]
    fn test_empty_document() {
        let result = run_check("").unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_no_code_blocks() {
        let content = "# Just a heading\n\nSome text.";
        let result = run_check(content).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_single_code_block_no_inconsistency() {
        let content = "```bash\necho hi\n```";
        let config = MD040Config {
            style: LanguageStyle::Consistent,
            ..Default::default()
        };
        let result = run_check_with_config(content, config).unwrap();
        assert!(result.is_empty(), "Single block has no inconsistency");
    }

    #[test]
    fn test_idempotent_fix() {
        let content = r#"```bash
echo hi
```

```sh
echo there
```
"#;
        let config = MD040Config {
            style: LanguageStyle::Consistent,
            ..Default::default()
        };
        let fixed1 = run_fix_with_config(content, config.clone()).unwrap();
        let fixed2 = run_fix_with_config(&fixed1, config).unwrap();
        assert_eq!(fixed1, fixed2, "Fix should be idempotent");
    }

    // =========================================================================
    // MkDocs superfences tests
    // =========================================================================

    #[test]
    fn test_mkdocs_superfences_title_only() {
        // title= attribute without language should not warn in MkDocs flavor
        let content = r#"```title="Example"
echo hi
```
"#;
        let result = run_check_mkdocs(content).unwrap();
        assert!(
            result.is_empty(),
            "MkDocs superfences with title= should not require language"
        );
    }

    #[test]
    fn test_mkdocs_superfences_hl_lines() {
        // hl_lines= attribute without language should not warn
        let content = r#"```hl_lines="1 2"
line 1
line 2
```
"#;
        let result = run_check_mkdocs(content).unwrap();
        assert!(
            result.is_empty(),
            "MkDocs superfences with hl_lines= should not require language"
        );
    }

    #[test]
    fn test_mkdocs_superfences_linenums() {
        // linenums= attribute without language should not warn
        let content = r#"```linenums="1"
line 1
line 2
```
"#;
        let result = run_check_mkdocs(content).unwrap();
        assert!(
            result.is_empty(),
            "MkDocs superfences with linenums= should not require language"
        );
    }

    #[test]
    fn test_mkdocs_superfences_class() {
        // Custom class (starting with .) should not warn
        let content = r#"```.my-class
some text
```
"#;
        let result = run_check_mkdocs(content).unwrap();
        assert!(
            result.is_empty(),
            "MkDocs superfences with .class should not require language"
        );
    }

    #[test]
    fn test_mkdocs_superfences_id() {
        // Custom ID (starting with #) should not warn
        let content = r#"```#my-id
some text
```
"#;
        let result = run_check_mkdocs(content).unwrap();
        assert!(
            result.is_empty(),
            "MkDocs superfences with #id should not require language"
        );
    }

    #[test]
    fn test_mkdocs_superfences_with_language() {
        // Language with superfences attributes should work fine
        let content = r#"```python title="Example" hl_lines="1"
print("hello")
```
"#;
        let result = run_check_mkdocs(content).unwrap();
        assert!(result.is_empty(), "Code block with language and attrs should pass");
    }

    #[test]
    fn test_standard_flavor_no_special_handling() {
        // In Standard flavor, title= should still warn
        let content = r#"```title="Example"
echo hi
```
"#;
        let result = run_check(content).unwrap();
        assert_eq!(
            result.len(),
            1,
            "Standard flavor should warn about title= without language"
        );
    }
}
