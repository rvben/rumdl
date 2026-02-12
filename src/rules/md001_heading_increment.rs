use crate::HeadingStyle;
use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::rules::front_matter_utils::FrontMatterUtils;
use crate::rules::heading_utils::HeadingUtils;
use crate::utils::range_utils::calculate_heading_range;
use regex::Regex;

/// Rule MD001: Heading levels should only increment by one level at a time
///
/// See [docs/md001.md](../../docs/md001.md) for full documentation, configuration, and examples.
///
/// This rule enforces a fundamental principle of document structure: heading levels
/// should increase by exactly one level at a time to maintain a proper document hierarchy.
///
/// ## Purpose
///
/// Proper heading structure creates a logical document outline and improves:
/// - Readability for humans
/// - Accessibility for screen readers
/// - Navigation in rendered documents
/// - Automatic generation of tables of contents
///
/// ## Examples
///
/// ### Correct Heading Structure
/// ```markdown
/// # Heading 1
/// ## Heading 2
/// ### Heading 3
/// ## Another Heading 2
/// ```
///
/// ### Incorrect Heading Structure
/// ```markdown
/// # Heading 1
/// ### Heading 3 (skips level 2)
/// #### Heading 4
/// ```
///
/// ## Behavior
///
/// This rule:
/// - Tracks the heading level throughout the document
/// - Validates that each new heading is at most one level deeper than the previous heading
/// - Allows heading levels to decrease by any amount (e.g., going from ### to #)
/// - Works with both ATX (`#`) and Setext (underlined) heading styles
///
/// ## Fix Behavior
///
/// When applying automatic fixes, this rule:
/// - Changes the level of non-compliant headings to be one level deeper than the previous heading
/// - Preserves the original heading style (ATX or Setext)
/// - Maintains indentation and other formatting
///
/// ## Rationale
///
/// Skipping heading levels (e.g., from `h1` to `h3`) can confuse readers and screen readers
/// by creating gaps in the document structure. Consistent heading increments create a proper
/// hierarchical outline essential for well-structured documents.
///
/// ## Front Matter Title Support
///
/// When `front_matter_title` is enabled (default: true), this rule recognizes a `title:` field
/// in YAML/TOML frontmatter as an implicit level-1 heading. This allows documents like:
///
/// ```markdown
/// ---
/// title: My Document
/// ---
///
/// ## First Section
/// ```
///
/// Without triggering a warning about skipping from H1 to H2, since the frontmatter title
/// counts as the H1.
///
#[derive(Debug, Clone)]
pub struct MD001HeadingIncrement {
    /// Whether to treat frontmatter title field as an implicit H1
    pub front_matter_title: bool,
    /// Optional regex pattern to match custom title fields in frontmatter
    pub front_matter_title_pattern: Option<Regex>,
}

impl Default for MD001HeadingIncrement {
    fn default() -> Self {
        Self {
            front_matter_title: true,
            front_matter_title_pattern: None,
        }
    }
}

/// Result of computing the fix for a single heading
struct HeadingFixInfo {
    /// The level after fixing (may equal original if no fix needed)
    fixed_level: usize,
    /// The heading style to use for the replacement
    style: HeadingStyle,
    /// Whether this heading needs a fix
    needs_fix: bool,
}

impl MD001HeadingIncrement {
    /// Create a new instance with specified settings
    pub fn new(front_matter_title: bool) -> Self {
        Self {
            front_matter_title,
            front_matter_title_pattern: None,
        }
    }

    /// Create a new instance with a custom pattern for matching title fields
    pub fn with_pattern(front_matter_title: bool, pattern: Option<String>) -> Self {
        let front_matter_title_pattern = pattern.and_then(|p| match Regex::new(&p) {
            Ok(regex) => Some(regex),
            Err(e) => {
                log::warn!("Invalid front_matter_title_pattern regex for MD001: {e}");
                None
            }
        });

        Self {
            front_matter_title,
            front_matter_title_pattern,
        }
    }

    /// Check if the document has a front matter title field
    fn has_front_matter_title(&self, content: &str) -> bool {
        if !self.front_matter_title {
            return false;
        }

        // If we have a custom pattern, use it to search front matter content
        if let Some(ref pattern) = self.front_matter_title_pattern {
            let front_matter_lines = FrontMatterUtils::extract_front_matter(content);
            for line in front_matter_lines {
                if pattern.is_match(line) {
                    return true;
                }
            }
            return false;
        }

        // Default behavior: check for "title:" field
        FrontMatterUtils::has_front_matter_field(content, "title:")
    }

    /// Single source of truth for heading level computation and style mapping.
    ///
    /// Returns `(HeadingFixInfo, new_prev_level)`. Both `check()` and `fix()` call
    /// this, making it structurally impossible for them to diverge.
    fn compute_heading_fix(
        prev_level: Option<usize>,
        heading: &crate::lint_context::HeadingInfo,
    ) -> (HeadingFixInfo, Option<usize>) {
        let level = heading.level as usize;

        let (fixed_level, needs_fix) = if let Some(prev) = prev_level
            && level > prev + 1
        {
            (prev + 1, true)
        } else {
            (level, false)
        };

        // Map heading style, adjusting Setext variant based on the fixed level
        let style = match heading.style {
            crate::lint_context::HeadingStyle::ATX => HeadingStyle::Atx,
            crate::lint_context::HeadingStyle::Setext1 | crate::lint_context::HeadingStyle::Setext2 => {
                if fixed_level == 1 {
                    HeadingStyle::Setext1
                } else {
                    HeadingStyle::Setext2
                }
            }
        };

        let info = HeadingFixInfo {
            fixed_level,
            style,
            needs_fix,
        };
        (info, Some(fixed_level))
    }
}

impl Rule for MD001HeadingIncrement {
    fn name(&self) -> &'static str {
        "MD001"
    }

    fn description(&self) -> &'static str {
        "Heading levels should only increment by one level at a time"
    }

    fn check(&self, ctx: &crate::lint_context::LintContext) -> LintResult {
        let mut warnings = Vec::new();

        let mut prev_level: Option<usize> = if self.has_front_matter_title(ctx.content) {
            Some(1)
        } else {
            None
        };

        for valid_heading in ctx.valid_headings() {
            let heading = valid_heading.heading;
            let line_info = valid_heading.line_info;

            let level = heading.level as usize;

            let (fix_info, new_prev) = Self::compute_heading_fix(prev_level, heading);
            prev_level = new_prev;

            if fix_info.needs_fix {
                let line_content = line_info.content(ctx.content);
                let original_indent = &line_content[..line_info.indent];
                let replacement =
                    HeadingUtils::convert_heading_style(&heading.raw_text, fix_info.fixed_level as u32, fix_info.style);

                let (start_line, start_col, end_line, end_col) =
                    calculate_heading_range(valid_heading.line_num, line_content);

                warnings.push(LintWarning {
                    rule_name: Some(self.name().to_string()),
                    line: start_line,
                    column: start_col,
                    end_line,
                    end_column: end_col,
                    message: format!(
                        "Expected heading level {}, but found heading level {}",
                        fix_info.fixed_level, level
                    ),
                    severity: Severity::Error,
                    fix: Some(Fix {
                        range: ctx.line_index.line_content_range(valid_heading.line_num),
                        replacement: format!("{original_indent}{replacement}"),
                    }),
                });
            }
        }

        Ok(warnings)
    }

    fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, LintError> {
        let mut fixed_lines = Vec::new();

        let mut prev_level: Option<usize> = if self.has_front_matter_title(ctx.content) {
            Some(1)
        } else {
            None
        };

        let mut skip_next = false;
        for line_info in ctx.lines.iter() {
            if skip_next {
                skip_next = false;
                continue;
            }

            if let Some(heading) = line_info.heading.as_deref() {
                if !heading.is_valid {
                    fixed_lines.push(line_info.content(ctx.content).to_string());
                    continue;
                }

                let is_setext = matches!(
                    heading.style,
                    crate::lint_context::HeadingStyle::Setext1 | crate::lint_context::HeadingStyle::Setext2
                );

                let (fix_info, new_prev) = Self::compute_heading_fix(prev_level, heading);
                prev_level = new_prev;

                if fix_info.needs_fix {
                    let replacement = HeadingUtils::convert_heading_style(
                        &heading.raw_text,
                        fix_info.fixed_level as u32,
                        fix_info.style,
                    );
                    let line = line_info.content(ctx.content);
                    let original_indent = &line[..line_info.indent];
                    fixed_lines.push(format!("{original_indent}{replacement}"));

                    // Setext headings span two lines (text + underline). The replacement
                    // already includes both lines, so skip the underline line.
                    if is_setext {
                        skip_next = true;
                    }
                } else {
                    // Heading is valid — preserve original content exactly
                    fixed_lines.push(line_info.content(ctx.content).to_string());
                }
            } else {
                fixed_lines.push(line_info.content(ctx.content).to_string());
            }
        }

        let mut result = fixed_lines.join("\n");
        if ctx.content.ends_with('\n') && !result.ends_with('\n') {
            result.push('\n');
        }
        Ok(result)
    }

    fn category(&self) -> RuleCategory {
        RuleCategory::Heading
    }

    fn should_skip(&self, ctx: &crate::lint_context::LintContext) -> bool {
        // Fast path: check if document likely has headings
        if ctx.content.is_empty() || !ctx.likely_has_headings() {
            return true;
        }
        // Verify valid headings actually exist
        !ctx.has_valid_headings()
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn from_config(config: &crate::config::Config) -> Box<dyn Rule>
    where
        Self: Sized,
    {
        // Get MD001 config section
        let (front_matter_title, front_matter_title_pattern) = if let Some(rule_config) = config.rules.get("MD001") {
            let fmt = rule_config
                .values
                .get("front-matter-title")
                .or_else(|| rule_config.values.get("front_matter_title"))
                .and_then(|v| v.as_bool())
                .unwrap_or(true);

            let pattern = rule_config
                .values
                .get("front-matter-title-pattern")
                .or_else(|| rule_config.values.get("front_matter_title_pattern"))
                .and_then(|v| v.as_str())
                .filter(|s: &&str| !s.is_empty())
                .map(String::from);

            (fmt, pattern)
        } else {
            (true, None)
        };

        Box::new(MD001HeadingIncrement::with_pattern(
            front_matter_title,
            front_matter_title_pattern,
        ))
    }

    fn default_config_section(&self) -> Option<(String, toml::Value)> {
        Some((
            "MD001".to_string(),
            toml::toml! {
                front-matter-title = true
            }
            .into(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lint_context::LintContext;

    #[test]
    fn test_basic_functionality() {
        let rule = MD001HeadingIncrement::default();

        // Test with valid headings
        let content = "# Heading 1\n## Heading 2\n### Heading 3";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty());

        // Test with invalid headings: H1 → H3 → H4
        // H3 skips level 2, and H4 is > fixed(H3=H2) + 1, so both are flagged
        let content = "# Heading 1\n### Heading 3\n#### Heading 4";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].line, 2);
        assert_eq!(result[1].line, 3);
    }

    #[test]
    fn test_frontmatter_title_counts_as_h1() {
        let rule = MD001HeadingIncrement::default();

        // Frontmatter with title, followed by H2 - should pass
        let content = "---\ntitle: My Document\n---\n\n## First Section";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "H2 after frontmatter title should not trigger warning"
        );

        // Frontmatter with title, followed by H3 - should warn (skips H2)
        let content = "---\ntitle: My Document\n---\n\n### Third Level";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1, "H3 after frontmatter title should warn");
        assert!(result[0].message.contains("Expected heading level 2"));
    }

    #[test]
    fn test_frontmatter_without_title() {
        let rule = MD001HeadingIncrement::default();

        // Frontmatter without title, followed by H2 - first heading has no predecessor
        // so it should pass (no increment check for the first heading)
        let content = "---\nauthor: John\n---\n\n## First Section";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "First heading after frontmatter without title has no predecessor"
        );
    }

    #[test]
    fn test_frontmatter_title_disabled() {
        let rule = MD001HeadingIncrement::new(false);

        // Frontmatter with title, but feature disabled - H2 has no predecessor
        let content = "---\ntitle: My Document\n---\n\n## First Section";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "With front_matter_title disabled, first heading has no predecessor"
        );
    }

    #[test]
    fn test_frontmatter_title_with_subsequent_headings() {
        let rule = MD001HeadingIncrement::default();

        // Complete document with frontmatter title
        let content = "---\ntitle: My Document\n---\n\n## Introduction\n\n### Details\n\n## Conclusion";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Valid heading progression after frontmatter title");
    }

    #[test]
    fn test_frontmatter_title_fix() {
        let rule = MD001HeadingIncrement::default();

        // Frontmatter with title, H3 should be fixed to H2
        let content = "---\ntitle: My Document\n---\n\n### Third Level";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        assert!(
            fixed.contains("## Third Level"),
            "H3 should be fixed to H2 when frontmatter has title"
        );
    }

    #[test]
    fn test_toml_frontmatter_title() {
        let rule = MD001HeadingIncrement::default();

        // TOML frontmatter with title
        let content = "+++\ntitle = \"My Document\"\n+++\n\n## First Section";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "TOML frontmatter title should count as H1");
    }

    #[test]
    fn test_no_frontmatter_no_h1() {
        let rule = MD001HeadingIncrement::default();

        // No frontmatter, starts with H2 - first heading has no predecessor, so no warning
        let content = "## First Section\n\n### Subsection";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "First heading (even if H2) has no predecessor to compare against"
        );
    }

    #[test]
    fn test_fix_preserves_attribute_lists() {
        let rule = MD001HeadingIncrement::default();

        // H1 followed by H3 with attribute list - fix should preserve { #custom-id }
        let content = "# Heading 1\n\n### Heading 3 { #custom-id .special }";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);

        // Verify fix() preserves attribute list
        let fixed = rule.fix(&ctx).unwrap();
        assert!(
            fixed.contains("## Heading 3 { #custom-id .special }"),
            "fix() should preserve attribute list, got: {fixed}"
        );

        // Verify check() fix output also preserves attribute list
        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 1);
        let fix = warnings[0].fix.as_ref().expect("Should have a fix");
        assert!(
            fix.replacement.contains("{ #custom-id .special }"),
            "check() fix should preserve attribute list, got: {}",
            fix.replacement
        );
    }

    #[test]
    fn test_check_single_skip_with_repeated_level() {
        let rule = MD001HeadingIncrement::default();

        // H1 followed by two H3s: only the first H3 is flagged.
        // After fixing H3a to H2 (prev+1), H3b at level 3 = 2+1 is valid.
        let content = "# H1\n### H3a\n### H3b";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);

        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 1, "Only first H3 should be flagged: got {warnings:?}");
        assert!(warnings[0].message.contains("Expected heading level 2"));

        // Verify check()+apply_all_fixes produces idempotent output
        let fixed = rule.fix(&ctx).unwrap();
        let ctx_fixed = LintContext::new(&fixed, crate::config::MarkdownFlavor::Standard, None);
        let warnings_after = rule.check(&ctx_fixed).unwrap();
        assert!(
            warnings_after.is_empty(),
            "After fix, no warnings should remain: {fixed:?}, warnings: {warnings_after:?}"
        );
    }

    #[test]
    fn test_check_cascading_skip_produces_idempotent_fix() {
        let rule = MD001HeadingIncrement::default();

        // H1 → H4 → H5: both are flagged.
        // H4: prev=1, expected=2. Fixed level tracked as 2.
        // H5: prev=2, expected=3.
        // Both fixes applied in one pass produce clean output.
        let content = "# Title\n#### Deep\n##### Deeper";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);

        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(
            warnings.len(),
            2,
            "Both deep headings should be flagged for idempotent fix"
        );
        assert!(warnings[0].message.contains("Expected heading level 2"));
        assert!(warnings[1].message.contains("Expected heading level 3"));

        // Verify single-pass idempotent fix
        let fixed = rule.fix(&ctx).unwrap();
        let ctx_fixed = LintContext::new(&fixed, crate::config::MarkdownFlavor::Standard, None);
        let warnings_after = rule.check(&ctx_fixed).unwrap();
        assert!(
            warnings_after.is_empty(),
            "Fixed content should have no warnings: {fixed:?}"
        );
    }

    #[test]
    fn test_check_level_decrease_resets_tracking() {
        let rule = MD001HeadingIncrement::default();

        // H1 → H3 (flagged) → H1 (decrease, always allowed) → H3 (flagged again)
        let content = "# Title\n### Sub\n# Another\n### Sub2";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);

        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(
            warnings.len(),
            2,
            "Both H3 headings should be flagged (each follows an H1)"
        );

        // Verify single-pass idempotent fix
        let fixed = rule.fix(&ctx).unwrap();
        let ctx_fixed = LintContext::new(&fixed, crate::config::MarkdownFlavor::Standard, None);
        assert!(
            rule.check(&ctx_fixed).unwrap().is_empty(),
            "Fixed content should pass: {fixed:?}"
        );
    }

    /// Core invariant: for every warning with a Fix, the replacement text must
    /// match what fix() produces for that same line.
    #[test]
    fn test_check_and_fix_produce_identical_replacements() {
        let rule = MD001HeadingIncrement::default();

        let inputs = [
            "# H1\n### H3\n",
            "# H1\n#### H4\n##### H5\n",
            "# H1\n### H3\n# H1b\n### H3b\n",
            "# H1\n\n### H3 { #custom-id }\n",
            "---\ntitle: Doc\n---\n\n### Deep\n",
        ];

        for input in &inputs {
            let ctx = LintContext::new(input, crate::config::MarkdownFlavor::Standard, None);
            let warnings = rule.check(&ctx).unwrap();
            let fixed = rule.fix(&ctx).unwrap();
            let fixed_lines: Vec<&str> = fixed.lines().collect();

            for warning in &warnings {
                if let Some(ref fix) = warning.fix {
                    // Extract the fixed line from fix() output for the same line number
                    let line_idx = warning.line - 1;
                    assert!(
                        line_idx < fixed_lines.len(),
                        "Warning line {} out of range for fixed output (input: {input:?})",
                        warning.line,
                    );
                    let fix_output_line = fixed_lines[line_idx];
                    assert_eq!(
                        fix.replacement, fix_output_line,
                        "check() fix and fix() output diverge at line {} (input: {input:?})",
                        warning.line,
                    );
                }
            }
        }
    }

    /// Setext H1 followed by deep ATX heading: Setext heading is untouched,
    /// ATX heading is fixed to H2.
    #[test]
    fn test_setext_headings_mixed_with_atx_cascading() {
        let rule = MD001HeadingIncrement::default();

        let content = "Setext Title\n============\n\n#### Deep ATX\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);

        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].message.contains("Expected heading level 2"));

        let fixed = rule.fix(&ctx).unwrap();
        assert!(
            fixed.contains("## Deep ATX"),
            "H4 after Setext H1 should be fixed to ATX H2, got: {fixed}"
        );

        // Verify idempotency
        let ctx_fixed = LintContext::new(&fixed, crate::config::MarkdownFlavor::Standard, None);
        assert!(
            rule.check(&ctx_fixed).unwrap().is_empty(),
            "Fixed content should produce no warnings"
        );
    }

    /// fix(fix(x)) == fix(x) for various inputs
    #[test]
    fn test_fix_idempotent_applied_twice() {
        let rule = MD001HeadingIncrement::default();

        let inputs = [
            "# H1\n### H3\n#### H4\n",
            "## H2\n##### H5\n###### H6\n",
            "# A\n### B\n# C\n### D\n##### E\n",
            "# H1\nH2\n--\n#### H4\n",
            // Setext edge cases
            "Title\n=====\n",
            "Title\n=====\n\n#### Deep\n",
            "Sub\n---\n\n#### Deep\n",
            "T1\n==\nT2\n--\n#### Deep\n",
        ];

        for input in &inputs {
            let ctx1 = LintContext::new(input, crate::config::MarkdownFlavor::Standard, None);
            let fixed_once = rule.fix(&ctx1).unwrap();

            let ctx2 = LintContext::new(&fixed_once, crate::config::MarkdownFlavor::Standard, None);
            let fixed_twice = rule.fix(&ctx2).unwrap();

            assert_eq!(
                fixed_once, fixed_twice,
                "fix() is not idempotent for input: {input:?}\nfirst:  {fixed_once:?}\nsecond: {fixed_twice:?}"
            );
        }
    }

    /// Setext underline must not be duplicated: fix() should produce the same
    /// number of lines as the input for valid documents.
    #[test]
    fn test_setext_fix_no_underline_duplication() {
        let rule = MD001HeadingIncrement::default();

        // Setext H1 only — no fix needed, output must be identical
        let content = "Title\n=====\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, content, "Valid Setext H1 should be unchanged");

        // Setext H2 only — no fix needed
        let content = "Sub\n---\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, content, "Valid Setext H2 should be unchanged");

        // Two consecutive Setext headings — valid H1 then H2
        let content = "Title\n=====\nSub\n---\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, content, "Valid consecutive Setext headings should be unchanged");

        // Setext H1 at end of file without trailing newline
        let content = "Title\n=====";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, content, "Setext H1 at EOF without newline should be unchanged");

        // Setext H2 followed by deep ATX heading
        let content = "Sub\n---\n\n#### Deep\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        assert!(
            fixed.contains("### Deep"),
            "H4 after Setext H2 should become H3, got: {fixed}"
        );
        assert_eq!(
            fixed.matches("---").count(),
            1,
            "Underline should not be duplicated, got: {fixed}"
        );

        // Underline longer than text must not be normalized for valid headings
        let content = "Hi\n==========\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(
            fixed, content,
            "Valid Setext with long underline must be preserved exactly, got: {fixed}"
        );

        // Underline shorter than text must not be normalized
        let content = "Long Title Here\n===\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(
            fixed, content,
            "Valid Setext with short underline must be preserved exactly, got: {fixed}"
        );
    }
}
