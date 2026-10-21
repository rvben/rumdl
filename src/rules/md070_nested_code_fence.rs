use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::utils::code_block_utils::{Enclosure, continued_depth, item_runs, strip_enclosure};

/// Rule MD070: Nested code fence collision detection
///
/// Detects when a fenced code block contains fence markers that would cause
/// premature closure. Suggests using longer fences to avoid this issue.
///
/// Checks languages where triple backtick sequences commonly appear:
/// markdown, Python, JavaScript, shell, Rust, Go, and others with multiline
/// strings, heredocs, template literals, or doc comments.
///
/// See [docs/md070.md](../../docs/md070.md) for full documentation.
#[derive(Clone, Default)]
pub struct MD070NestedCodeFence;

impl MD070NestedCodeFence {
    pub fn new() -> Self {
        Self
    }

    /// Check if the given language should be checked for nested fences.
    /// Covers languages where triple backtick sequences commonly appear in source:
    /// multiline strings with embedded markdown, heredocs, doc comments, template
    /// literals, and data formats with multiline string values.
    fn should_check_language(lang: &str) -> bool {
        let base = lang.split_whitespace().next().unwrap_or("");
        matches!(
            base.to_ascii_lowercase().as_str(),
            // Documentation / markup
            ""
                | "markdown"
                | "md"
                | "mdx"
                | "text"
                | "txt"
                | "plain"
                // Multiline strings / docstrings
                | "python"
                | "py"
                | "ruby"
                | "rb"
                | "perl"
                | "pl"
                | "php"
                | "lua"
                | "r"
                | "rmd"
                | "rmarkdown"
                // Template literals / raw strings
                | "javascript"
                | "js"
                | "jsx"
                | "mjs"
                | "cjs"
                | "typescript"
                | "ts"
                | "tsx"
                | "mts"
                | "rust"
                | "rs"
                | "go"
                | "golang"
                | "swift"
                | "kotlin"
                | "kt"
                | "kts"
                | "java"
                | "csharp"
                | "cs"
                | "c#"
                | "scala"
                // Shell heredocs
                | "shell"
                | "sh"
                | "bash"
                | "zsh"
                | "fish"
                | "powershell"
                | "ps1"
                | "pwsh"
                // Data / config formats
                | "yaml"
                | "yml"
                | "toml"
                | "json"
                | "jsonc"
                | "json5"
                // Template engines
                | "jinja"
                | "jinja2"
                | "handlebars"
                | "hbs"
                | "liquid"
                | "nunjucks"
                | "njk"
                | "ejs"
                // Terminal output
                | "console"
                | "terminal"
        )
    }

    /// A fence-like line of the block's container, with its indentation
    /// measured from the container's content.
    fn fence_line<'l>(line: &'l str, enclosing: &[Enclosure]) -> Option<FenceLine<'l>> {
        let stripped = strip_enclosure(line, enclosing)?;
        let (indent, text) = stripped.indent(line);
        let run_start = text?;
        // Per CommonMark a fence has 0-3 columns of indentation; more is code.
        if indent > 3 {
            return None;
        }
        let ch = line[run_start..].chars().next().filter(|&c| c == '`' || c == '~')?;
        let len = line[run_start..].chars().take_while(|&c| c == ch).count();
        if len < 3 {
            return None;
        }
        Some(FenceLine {
            indent,
            ch,
            len,
            run_start,
            rest: &line[run_start + len..],
        })
    }

    /// A line that opens a fence of its own: the character, a run, then an info
    /// string (a backtick fence's info string holds no backtick).
    fn is_fence_like(fence: &FenceLine<'_>) -> bool {
        let info = fence.rest.trim_matches([' ', '\t']);
        if info.is_empty() {
            return true;
        }
        (fence.ch != '`' || !info.contains('`'))
            && fence.rest.chars().next().is_some_and(|c| c.is_alphabetic() || c == '{')
    }

    /// A line that opens an independent block of its own: it carries an info
    /// string (a backtick fence's info string holds no backtick).
    fn opens_block(fence: &FenceLine<'_>) -> bool {
        let info = fence.rest.trim_matches([' ', '\t']);
        !info.is_empty() && (fence.ch != '`' || !info.contains('`'))
    }

    /// A line CommonMark closes a block of `ch` fences of at least `min_length`
    /// on: only spaces and tabs follow the run.
    fn is_closer(fence: &FenceLine<'_>, ch: char, min_length: usize) -> bool {
        fence.ch == ch && fence.len >= min_length && fence.rest.trim_matches([' ', '\t']).is_empty()
    }

    /// Whether fences are found from the line text instead of the CommonMark
    /// blocks. MDX parses JSX and expressions itself, so a fence inside a JSX
    /// element has no CommonMark block (the parser reads the region as HTML)
    /// and its container structure is not available.
    fn blocks_from_line_scan(ctx: &crate::lint_context::LintContext) -> bool {
        ctx.flavor == crate::config::MarkdownFlavor::MDX
    }

    /// The closed fenced blocks, from the parser.
    fn blocks_from_parser(ctx: &crate::lint_context::LintContext) -> Vec<FenceBlock> {
        let lines = ctx.raw_lines();
        ctx.code_block_details
            .iter()
            .filter(|detail| detail.is_fenced && detail.closed)
            .filter_map(|detail| {
                let ch = ctx.content[detail.start..]
                    .chars()
                    .next()
                    .filter(|&c| c == '`' || c == '~')?;
                let len = ctx.content[detail.start..].chars().take_while(|&c| c == ch).count();
                let open_line = Self::line_of(ctx, detail.start);
                let close_line = Self::line_of(ctx, detail.end.saturating_sub(1));
                (close_line > open_line && close_line < lines.len()).then(|| FenceBlock {
                    start: detail.start,
                    ch,
                    len,
                    info: detail.info_string.trim().to_string(),
                    open_line,
                    close_line,
                    enclosing: detail.enclosing.clone(),
                })
            })
            .collect()
    }

    /// The closed fenced blocks of a document the parser cannot see through,
    /// found from the line text: top-level fences only, minus those the parser
    /// ended without a closing fence (a container ending ends the block).
    fn blocks_from_lines(ctx: &crate::lint_context::LintContext) -> Vec<FenceBlock> {
        let lines = ctx.raw_lines();
        let unclosed: std::collections::HashSet<usize> = ctx
            .code_block_details
            .iter()
            .filter(|detail| detail.is_fenced && !detail.closed)
            .map(|detail| Self::line_of(ctx, detail.start))
            .collect();
        let mut blocks = Vec::new();
        let mut i = 0;
        while i < lines.len() {
            // A line after a code line is its content or its closer.
            if i > 0 && ctx.lines.get(i - 1).is_some_and(|prev| prev.in_code_block) {
                i += 1;
                continue;
            }
            let Some(open) = Self::fence_line(lines[i], &[]) else {
                i += 1;
                continue;
            };
            // Markup that is not Markdown source opens no fence.
            if Self::is_ignored_context(&ctx.lines[i]) {
                i += 1;
                continue;
            }
            if unclosed.contains(&i) {
                i += 1;
                continue;
            }
            let close = (i + 1..lines.len()).find(|&j| {
                Self::fence_line(lines[j], &[]).is_some_and(|fence| Self::is_closer(&fence, open.ch, open.len))
            });
            let Some(close_line) = close else {
                i += 1;
                continue;
            };
            blocks.push(FenceBlock {
                start: ctx.lines[i].byte_offset + open.run_start,
                ch: open.ch,
                len: open.len,
                info: open.rest.trim().to_string(),
                open_line: i,
                close_line,
                enclosing: Vec::new(),
            });
            i = close_line + 1;
        }
        blocks
    }

    /// Whether a line sits in front matter, a comment or an HTML block.
    fn is_ignored_context(line: &crate::lint_context::LineInfo) -> bool {
        line.in_front_matter || line.in_html_comment || line.in_mdx_comment || line.in_html_block
    }

    /// The container structure the intended-close search needs. The line scan
    /// sees no containers, so it gets no openers.
    fn nesting<'c>(ctx: &'c crate::lint_context::LintContext) -> Nesting<'c> {
        let mut openers: Vec<_> = if Self::blocks_from_line_scan(ctx) {
            Vec::new()
        } else {
            ctx.code_block_details
                .iter()
                .filter(|detail| detail.is_fenced)
                .map(|detail| (Self::line_of(ctx, detail.start), detail.enclosing.as_slice()))
                .collect()
        };
        openers.sort_by_key(|&(open, _)| open);

        let lines = ctx.raw_lines();
        let mut continued = vec![usize::MAX; lines.len()];
        let mut nearest = None;
        let mut next = 0;
        for (i, line) in lines.iter().enumerate() {
            while let Some(&(_, enclosing)) = openers.get(next).filter(|&&(open, _)| open <= i) {
                nearest = Some((enclosing, item_runs(enclosing)));
                next += 1;
            }
            if let Some((enclosing, runs)) = &nearest {
                continued[i] = continued_depth(line, enclosing, runs);
            }
        }

        let mut next_shallower = vec![lines.len(); lines.len()];
        let mut deeper: Vec<usize> = Vec::new();
        for i in (0..lines.len()).rev() {
            while deeper.last().is_some_and(|&k| continued[k] >= continued[i]) {
                deeper.pop();
            }
            if let Some(&k) = deeper.last() {
                next_shallower[i] = k;
            }
            deeper.push(i);
        }

        Nesting {
            openers,
            continued,
            next_shallower,
        }
    }

    /// Zero-based line holding byte `offset`.
    fn line_of(ctx: &crate::lint_context::LintContext, offset: usize) -> usize {
        ctx.lines
            .partition_point(|line| line.byte_offset <= offset)
            .saturating_sub(1)
    }

    /// The warning for one fenced block, when a fence-like line inside it makes
    /// the parser end it before the author meant it to end.
    fn check_block(
        &self,
        ctx: &crate::lint_context::LintContext,
        block: &FenceBlock,
        nesting: &Nesting,
    ) -> Option<(LintWarning, usize)> {
        let openers = &nesting.openers;
        let lines = ctx.raw_lines();
        let language = block.info.split_whitespace().next().unwrap_or("");
        if !Self::should_check_language(language) {
            return None;
        }

        // The first fence-like line inside the extent the parser gave the block.
        let collision = (block.open_line + 1..block.close_line).find(|&j| {
            Self::fence_line(lines[j], &block.enclosing)
                .is_some_and(|fence| fence.ch == block.ch && fence.len >= block.len && Self::is_fence_like(&fence))
        })?;

        // Where the author meant the block to end: the last bare fence before
        // the container ends, a line stops being part of it, or another fence
        // with an info string begins at the opener's own indentation.
        let line_start = ctx.lines[block.open_line].byte_offset;
        let opener_indent = strip_enclosure(&lines[block.open_line][..block.start - line_start], &block.enclosing)
            .map_or(0, |prefix| {
                prefix.indent(&lines[block.open_line][..block.start - line_start]).0
            });
        let bound = block.enclosing.last().map(Enclosure::end);
        let depth = block.enclosing.len();
        let mut intended_close = block.close_line;
        let mut next_opener = openers.partition_point(|&(open, _)| open <= block.close_line);
        let mut j = block.close_line + 1;
        while let Some(line) = lines.get(j) {
            if bound.is_some_and(|end| ctx.lines[j].byte_offset >= end) {
                break;
            }
            // A line that does not continue every container ends the search.
            if strip_enclosure(line, &block.enclosing).is_none() {
                break;
            }
            // A quote or list item nested inside the block's container holds
            // its own blocks, and its fences open and close those, so the
            // search resumes after it. Any fence of such a container comes at
            // or after the opener of its first fenced block. A lazy line in it
            // continues it but not the block's own containers, and ends the
            // search as it does anywhere else.
            while openers.get(next_opener).is_some_and(|&(open, _)| open < j) {
                next_opener += 1;
            }
            if let Some(&(_, enclosing)) = openers.get(next_opener).filter(|&&(open, _)| open == j)
                && let Some(nested) = enclosing.get(depth)
            {
                let end = Self::line_of(ctx, nested.end().saturating_sub(1)).max(j);
                let mut k = j;
                while k <= end && nesting.continued[k] >= depth {
                    k = nesting.next_shallower[k];
                }
                if k <= end {
                    break;
                }
                j = end + 1;
                continue;
            }
            // A blank line or text decides nothing: the container goes on.
            if let Some(fence) = Self::fence_line(line, &block.enclosing) {
                if Self::is_closer(&fence, block.ch, block.len) {
                    intended_close = j;
                } else if fence.ch == block.ch && fence.indent <= opener_indent && Self::opens_block(&fence) {
                    break;
                }
            }
            j += 1;
        }

        // The longest fence inside what the author meant to hold.
        let longest = (block.open_line + 1..intended_close)
            .filter_map(|j| Self::fence_line(lines[j], &block.enclosing))
            .filter(|fence| fence.ch == block.ch && Self::is_fence_like(fence))
            .map(|fence| fence.len)
            .max()
            .unwrap_or(0);
        let suggested_fence: String = std::iter::repeat_n(block.ch, longest.max(block.len) + 1).collect();

        let closer = Self::fence_line(lines[intended_close], &block.enclosing)?;
        let closer_start = ctx.lines[intended_close].byte_offset + closer.run_start;
        let fix = Fix::with_additional_edits(
            block.start..block.start + block.len,
            suggested_fence.clone(),
            vec![Fix::new(
                closer_start..closer_start + closer.len,
                suggested_fence.clone(),
            )],
        );
        let collision_line_num = collision + 1;
        Some((
            LintWarning {
                rule_name: Some(self.name().to_string()),
                message: format!(
                    "Code block contains fence markers at line {collision_line_num} that interfere with block parsing — use {suggested_fence} for outer fence"
                ),
                line: block.open_line + 1,
                column: 1,
                end_line: intended_close + 1,
                end_column: lines[intended_close].chars().count() + 1,
                severity: Severity::Warning,
                fix: Some(fix),
            },
            intended_close,
        ))
    }
}

/// A closed fenced block to judge, wherever it was found.
struct FenceBlock {
    /// Byte offset of the opening fence's first character.
    start: usize,
    ch: char,
    /// Length of the opening fence.
    len: usize,
    info: String,
    open_line: usize,
    /// The line the parser ends the block on.
    close_line: usize,
    enclosing: Vec<Enclosure>,
}

/// The containers of a document, as the intended-close search sees them.
struct Nesting<'c> {
    /// The opening line and containers of every fenced block the parser found,
    /// closed or not, in document order.
    openers: Vec<(usize, &'c [Enclosure])>,
    /// For each line, how many containers of the nearest fenced block opening
    /// at or before it the line continues. Every container a search skips
    /// holds that opener, so the count answers for the skipping block's own
    /// containers too. `usize::MAX` before the first opener.
    continued: Vec<usize>,
    /// For each line, the next line that continues fewer containers.
    next_shallower: Vec<usize>,
}

struct FenceLine<'l> {
    /// Columns of indentation past the container's prefix.
    indent: usize,
    ch: char,
    len: usize,
    /// Byte offset of the first fence character in the line.
    run_start: usize,
    /// What follows the run.
    rest: &'l str,
}

impl Rule for MD070NestedCodeFence {
    fn native_format_policy(&self) -> Option<crate::format_proof::NativeFormatPolicy> {
        Some(crate::format_proof::NativeFormatPolicy::for_rule::<Self>(70))
    }

    fn name(&self) -> &'static str {
        "MD070"
    }

    fn description(&self) -> &'static str {
        "Nested code fence collision - use longer fence to avoid premature closure"
    }

    fn check(&self, ctx: &crate::lint_context::LintContext) -> LintResult {
        let mut warnings = Vec::new();
        let blocks = if Self::blocks_from_line_scan(ctx) {
            Self::blocks_from_lines(ctx)
        } else {
            Self::blocks_from_parser(ctx)
        };
        let nesting = Self::nesting(ctx);

        // Blocks the author's intended closing fence already covers are skipped,
        // so no two fixes touch the same lines.
        let mut covered_until = 0;
        for block in &blocks {
            if block.open_line < covered_until {
                continue;
            }
            // Skip contexts that are not Markdown source.
            let Some(line_info) = ctx.lines.get(block.open_line) else {
                continue;
            };
            if Self::is_ignored_context(line_info)
                || block.enclosing.iter().any(|e| matches!(e, Enclosure::Opaque { .. }))
            {
                continue;
            }
            if let Some((warning, intended_close)) = self.check_block(ctx, block, &nesting) {
                covered_until = intended_close + 1;
                warnings.push(warning);
            }
        }

        Ok(warnings)
    }

    fn fixes_from_diagnostics(&self) -> bool {
        true
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
        crate::utils::fix_utils::apply_warning_fixes(ctx.content, &warnings).map_err(LintError::FixFailed)
    }

    fn category(&self) -> RuleCategory {
        RuleCategory::CodeBlock
    }

    fn should_skip(&self, ctx: &crate::lint_context::LintContext) -> bool {
        ctx.content.is_empty() || (!ctx.likely_has_code() && !ctx.has_char('~'))
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn from_config(_config: &crate::config::Config) -> Box<dyn Rule>
    where
        Self: Sized,
    {
        Box::new(MD070NestedCodeFence::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lint_context::LintContext;

    fn run_check(content: &str) -> LintResult {
        let rule = MD070NestedCodeFence::new();
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        rule.check(&ctx)
    }

    /// Apply a Fix's primary and additional edits as one LSP workspace edit does.
    fn apply_fix_edits(content: &str, fix: &Fix) -> String {
        let mut edits: Vec<&Fix> = std::iter::once(fix).chain(fix.additional_edits.iter()).collect();
        edits.sort_by_key(|edit| std::cmp::Reverse(edit.range.start));
        let mut text = content.to_string();
        for edit in edits {
            text.replace_range(edit.range.clone(), &edit.replacement);
        }
        text
    }

    fn run_fix(content: &str) -> Result<String, LintError> {
        let rule = MD070NestedCodeFence::new();
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        rule.fix(&ctx)
    }

    #[test]
    fn test_no_collision_simple() {
        let content = "```python\nprint('hello')\n```\n";
        let result = run_check(content).unwrap();
        assert!(result.is_empty(), "Simple code block should not trigger warning");
    }

    #[test]
    fn test_no_collision_unchecked_language() {
        // C is not checked for nested fences (triple backticks don't appear in C source)
        let content = "```c\n```bash\necho hello\n```\n```\n";
        let result = run_check(content).unwrap();
        assert!(result.is_empty(), "Unchecked language should not trigger");
    }

    #[test]
    fn test_collision_python_language() {
        // Python is checked — triple-quoted strings commonly contain markdown
        let content = "```python\n```json\n{}\n```\n```\n";
        let result = run_check(content).unwrap();
        assert_eq!(result.len(), 1, "Python should be checked for nested fences");
        assert!(result[0].message.contains("````"));
    }

    #[test]
    fn test_collision_javascript_language() {
        let content = "```javascript\n```html\n<div></div>\n```\n```\n";
        let result = run_check(content).unwrap();
        assert_eq!(result.len(), 1, "JavaScript should be checked for nested fences");
    }

    #[test]
    fn test_collision_shell_language() {
        let content = "```bash\n```yaml\nkey: val\n```\n```\n";
        let result = run_check(content).unwrap();
        assert_eq!(result.len(), 1, "Shell should be checked for nested fences");
    }

    #[test]
    fn test_collision_rust_language() {
        let content = "```rust\n```toml\n[dep]\n```\n```\n";
        let result = run_check(content).unwrap();
        assert_eq!(result.len(), 1, "Rust should be checked for nested fences");
    }

    #[test]
    fn test_no_collision_assembly_language() {
        // Assembly, C, SQL etc. should NOT be checked
        for lang in ["asm", "c", "cpp", "sql", "css", "fortran"] {
            let content = format!("```{lang}\n```inner\ncontent\n```\n```\n");
            let result = run_check(&content).unwrap();
            assert!(result.is_empty(), "{lang} should not be checked for nested fences");
        }
    }

    #[test]
    fn test_collision_markdown_language() {
        let content = "```markdown\n```python\ncode()\n```\n```\n";
        let result = run_check(content).unwrap();
        assert_eq!(result.len(), 1, "Should emit single warning for collision");
        assert!(result[0].message.contains("fence markers at line"));
        assert!(result[0].message.contains("interfere with block parsing"));
        assert!(result[0].message.contains("use ````"));
    }

    #[test]
    fn test_collision_empty_language() {
        // Empty language (no language specified) is checked
        let content = "```\n```python\ncode()\n```\n```\n";
        let result = run_check(content).unwrap();
        assert_eq!(result.len(), 1, "Empty language should be checked");
    }

    #[test]
    fn test_no_collision_longer_outer_fence() {
        let content = "````markdown\n```python\ncode()\n```\n````\n";
        let result = run_check(content).unwrap();
        assert!(result.is_empty(), "Longer outer fence should not trigger warning");
    }

    #[test]
    fn test_tilde_fence_ignores_backticks() {
        // Tildes and backticks don't conflict
        let content = "~~~markdown\n```python\ncode()\n```\n~~~\n";
        let result = run_check(content).unwrap();
        assert!(result.is_empty(), "Different fence types should not collide");
    }

    #[test]
    fn test_tilde_collision() {
        let content = "~~~markdown\n~~~python\ncode()\n~~~\n~~~\n";
        let result = run_check(content).unwrap();
        assert_eq!(result.len(), 1, "Same fence type should collide");
        assert!(result[0].message.contains("~~~~"));
    }

    #[test]
    fn test_fix_increases_fence_length() {
        let content = "```markdown\n```python\ncode()\n```\n```\n";
        let fixed = run_fix(content).unwrap();
        assert!(fixed.starts_with("````markdown"), "Should increase to 4 backticks");
        assert!(
            fixed.contains("````\n") || fixed.ends_with("````"),
            "Closing should also be 4 backticks"
        );
    }

    #[test]
    fn test_fix_handles_longer_inner_fence() {
        // Inner fence has 5 backticks, so outer needs 6
        let content = "```markdown\n`````python\ncode()\n`````\n```\n";
        let fixed = run_fix(content).unwrap();
        assert!(fixed.starts_with("``````markdown"), "Should increase to 6 backticks");
    }

    #[test]
    fn test_backticks_in_code_not_fence() {
        // Template literals in JS shouldn't trigger
        let content = "```markdown\nconst x = `template`;\n```\n";
        let result = run_check(content).unwrap();
        assert!(result.is_empty(), "Inline backticks should not be detected as fences");
    }

    #[test]
    fn test_preserves_info_string() {
        let content = "```markdown {.highlight}\n```python\ncode()\n```\n```\n";
        let fixed = run_fix(content).unwrap();
        assert!(
            fixed.contains("````markdown {.highlight}"),
            "Should preserve info string attributes"
        );
    }

    #[test]
    fn test_md_language_alias() {
        let content = "```md\n```python\ncode()\n```\n```\n";
        let result = run_check(content).unwrap();
        assert_eq!(result.len(), 1, "md should be recognized as markdown");
    }

    #[test]
    fn test_real_world_docs_case() {
        // This is the actual pattern from docs/md031.md that triggered the PR
        let content = r#"```markdown
1. First item

   ```python
   code_in_list()
   ```

1. Second item

```
"#;
        let result = run_check(content).unwrap();
        assert_eq!(result.len(), 1, "Should emit single warning for nested fence issue");
        assert!(result[0].message.contains("line 4")); // The nested ``` is on line 4

        let fixed = run_fix(content).unwrap();
        assert!(fixed.starts_with("````markdown"), "Should fix with longer fence");
    }

    #[test]
    fn test_empty_code_block() {
        let content = "```markdown\n```\n";
        let result = run_check(content).unwrap();
        assert!(result.is_empty(), "Empty code block should not trigger");
    }

    #[test]
    fn test_multiple_code_blocks() {
        // The markdown block has a collision (inner ```python closes it prematurely).
        // The orphan closing fence (line 9) is NOT treated as a new opening fence
        // because the context correctly detects it as part of the markdown block.
        let content = r#"```python
safe code
```

```markdown
```python
collision
```
```

```javascript
also safe
```
"#;
        let result = run_check(content).unwrap();
        // Only 1 warning for the markdown block collision.
        // The orphan fence is correctly ignored (not parsed as new opening fence).
        assert_eq!(result.len(), 1, "Should emit single warning for collision");
        assert!(result[0].message.contains("line 6")); // The nested ```python is on line 6
    }

    #[test]
    fn test_single_collision_properly_closed() {
        // When the outer fence is properly longer, only the intended block triggers
        let content = r#"```python
safe code
```

````markdown
```python
collision
```
````

```javascript
also safe
```
"#;
        let result = run_check(content).unwrap();
        assert!(result.is_empty(), "Properly fenced blocks should not trigger");
    }

    #[test]
    fn test_indented_code_block_in_list() {
        let content = r#"- List item
  ```markdown
  ```python
  nested
  ```
  ```
"#;
        let result = run_check(content).unwrap();
        assert_eq!(result.len(), 1, "Should detect collision in indented block");
        assert!(result[0].message.contains("````"));
    }

    #[test]
    fn test_unclosed_fence_ends_with_its_list_item() {
        // CommonMark ends a fence that is never closed with the list item that
        // holds it, so the paragraph and the fenced block after the list are not
        // part of it, and the fence after the list cannot collide with it.
        let content = "# Title\n\n- Item with a nested example:\n\n  ```yaml\n  key: |\n    raw\n\nAfter the list.\n\n```text\nplain\n```\n";
        let result = run_check(content).unwrap();
        assert!(
            result.is_empty(),
            "A fence ended by its list item must not collide: {result:?}"
        );
        assert_eq!(run_fix(content).unwrap(), content);
    }

    #[test]
    fn test_no_false_positive_list_indented_block() {
        // 4-space indented code blocks in list context (GFM extension) should not
        // cause false positives. The closing fence with 3-space indent should not
        // be parsed as a new opening fence.
        let content = r#"1. List item with code:

    ```json
    {"key": "value"}
    ```

2. Another item

   ```python
   code()
   ```
"#;
        let result = run_check(content).unwrap();
        // No collision - these are separate, well-formed code blocks
        assert!(
            result.is_empty(),
            "List-indented code blocks should not trigger false positives"
        );
    }

    // ==================== Comprehensive Edge Case Tests ====================

    #[test]
    fn test_case_insensitive_language() {
        // MARKDOWN, Markdown, MD should all be checked
        for lang in ["MARKDOWN", "Markdown", "MD", "Md", "mD"] {
            let content = format!("```{lang}\n```python\ncode()\n```\n```\n");
            let result = run_check(&content).unwrap();
            assert_eq!(result.len(), 1, "{lang} should be recognized as markdown");
        }
    }

    #[test]
    fn test_unclosed_outer_fence() {
        // If outer fence is never closed, no collision can be detected
        let content = "```markdown\n```python\ncode()\n```\n";
        let result = run_check(content).unwrap();
        // The outer fence finds ```python as its closing fence (premature close)
        // Then ```\n at the end becomes orphan - but context would handle this
        assert!(result.len() <= 1, "Unclosed fence should not cause issues");
    }

    #[test]
    fn test_deeply_nested_fences() {
        // Multiple levels of nesting require progressively longer fences
        let content = r#"```markdown
````markdown
```python
code()
```
````
```
"#;
        let result = run_check(content).unwrap();
        // The outer ``` sees ```` as collision (4 >= 3)
        assert_eq!(result.len(), 1, "Deep nesting should trigger warning");
        assert!(result[0].message.contains("`````")); // Needs 5 to be safe
    }

    #[test]
    fn test_very_long_fences() {
        // 10 backtick fences should work correctly
        let content = "``````````markdown\n```python\ncode()\n```\n``````````\n";
        let result = run_check(content).unwrap();
        assert!(result.is_empty(), "Very long outer fence should not trigger warning");
    }

    #[test]
    fn test_blockquote_with_fence() {
        // The quote's own closer ends the block early, so the outer fence
        // needs to be longer; the fix touches only the two fence runs.
        let content = "> ```markdown\n> ```python\n> code()\n> ```\n> ```\n";
        let result = run_check(content).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!((result[0].line, result[0].end_line), (1, 5));
        assert_eq!(
            run_fix(content).unwrap(),
            "> ````markdown\n> ```python\n> code()\n> ```\n> ````\n"
        );
    }

    #[test]
    fn test_fence_with_attributes() {
        // Info string with attributes like {.class #id}
        let content = "```markdown {.highlight #example}\n```python\ncode()\n```\n```\n";
        let result = run_check(content).unwrap();
        assert_eq!(
            result.len(),
            1,
            "Attributes in info string should not prevent detection"
        );

        let fixed = run_fix(content).unwrap();
        assert!(
            fixed.contains("````markdown {.highlight #example}"),
            "Attributes should be preserved in fix"
        );
    }

    #[test]
    fn test_trailing_whitespace_in_info_string() {
        let content = "```markdown   \n```python\ncode()\n```\n```\n";
        let result = run_check(content).unwrap();
        assert_eq!(result.len(), 1, "Trailing whitespace should not affect detection");
    }

    #[test]
    fn test_only_closing_fence_pattern() {
        // Content that has only closing fence patterns (no language)
        let content = "```markdown\nsome text\n```\nmore text\n```\n";
        let result = run_check(content).unwrap();
        // The first ``` closes, second ``` is outside
        assert!(result.is_empty(), "Properly closed block should not trigger");
    }

    #[test]
    fn test_fence_at_end_of_file_no_newline() {
        let content = "```markdown\n```python\ncode()\n```\n```";
        let result = run_check(content).unwrap();
        assert_eq!(result.len(), 1, "Should detect collision even without trailing newline");

        let fixed = run_fix(content).unwrap();
        assert!(!fixed.ends_with('\n'), "Should preserve lack of trailing newline");
    }

    #[test]
    fn test_empty_lines_between_fences() {
        let content = "```markdown\n\n\n```python\n\ncode()\n\n```\n\n```\n";
        let result = run_check(content).unwrap();
        assert_eq!(result.len(), 1, "Empty lines should not affect collision detection");
    }

    #[test]
    fn test_tab_indented_opening_fence() {
        // A tab is four columns of indentation, so `\t```markdown` is an
        // indented code block, not a fence: nothing to collide with.
        let content = "\t```markdown\n```python\ncode()\n```\n```\n";
        let result = run_check(content).unwrap();
        assert!(result.is_empty(), "tab-indented line is indented code: {result:?}");
    }

    #[test]
    fn test_mixed_fence_types_no_collision() {
        // Backticks outer, tildes inner - should never collide
        let content = "```markdown\n~~~python\ncode()\n~~~\n```\n";
        let result = run_check(content).unwrap();
        assert!(result.is_empty(), "Different fence chars should not collide");

        // Tildes outer, backticks inner
        let content2 = "~~~markdown\n```python\ncode()\n```\n~~~\n";
        let result2 = run_check(content2).unwrap();
        assert!(result2.is_empty(), "Different fence chars should not collide");
    }

    #[test]
    fn test_frontmatter_not_confused_with_fence() {
        // YAML frontmatter uses --- which shouldn't be confused with fences
        let content = "---\ntitle: Test\n---\n\n```markdown\n```python\ncode()\n```\n```\n";
        let result = run_check(content).unwrap();
        assert_eq!(result.len(), 1, "Should detect collision after frontmatter");
    }

    #[test]
    fn test_html_comment_with_fence_inside() {
        // Fences inside HTML comments should be ignored
        let content = "<!-- ```markdown\n```python\ncode()\n``` -->\n\n```markdown\nreal content\n```\n";
        let result = run_check(content).unwrap();
        // The fences inside HTML comment should be skipped
        assert!(result.is_empty(), "Fences in HTML comments should be ignored");
    }

    #[test]
    fn test_consecutive_code_blocks() {
        // Multiple consecutive markdown blocks, each with collision
        let content = r#"```markdown
```python
a()
```
```

```markdown
```ruby
b()
```
```
"#;
        let result = run_check(content).unwrap();
        // Each markdown block has its own collision
        assert!(!result.is_empty(), "Should detect collision in first block");
    }

    #[test]
    fn test_numeric_info_string() {
        // Numbers after fence - some parsers treat this differently
        let content = "```123\n```456\ncode()\n```\n```\n";
        let result = run_check(content).unwrap();
        // "123" is not "markdown" or "md", so should not check
        assert!(result.is_empty(), "Numeric info string is not markdown");
    }

    #[test]
    fn test_collision_at_exact_length() {
        // An empty ``` is the closing fence, not a collision.
        // For a collision, the inner fence must have content that looks like an opening fence.
        let content = "```markdown\n```python\ncode()\n```\n```\n";
        let result = run_check(content).unwrap();
        assert_eq!(
            result.len(),
            1,
            "Same-length fence with language should trigger collision"
        );

        // Inner fence one shorter than outer - not a collision
        let content2 = "````markdown\n```python\ncode()\n```\n````\n";
        let result2 = run_check(content2).unwrap();
        assert!(result2.is_empty(), "Shorter inner fence should not collide");

        // Empty markdown block followed by another fence - not a collision
        let content3 = "```markdown\n```\n";
        let result3 = run_check(content3).unwrap();
        assert!(result3.is_empty(), "Empty closing fence is not a collision");
    }

    #[test]
    fn test_fix_preserves_content_exactly() {
        // Fix should not modify the content between fences
        let content = "```markdown\n```python\n  indented\n\ttabbed\nspecial: !@#$%\n```\n```\n";
        let fixed = run_fix(content).unwrap();
        assert!(fixed.contains("  indented"), "Indentation should be preserved");
        assert!(fixed.contains("\ttabbed"), "Tabs should be preserved");
        assert!(fixed.contains("special: !@#$%"), "Special chars should be preserved");
    }

    #[test]
    fn test_warning_line_numbers_accurate() {
        let content = "# Title\n\nParagraph\n\n```markdown\n```python\ncode()\n```\n```\n";
        let result = run_check(content).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].line, 5, "Warning should be on opening fence line");
        assert!(result[0].message.contains("line 6"), "Collision line should be line 6");
    }

    #[test]
    fn test_should_skip_optimization() {
        let rule = MD070NestedCodeFence::new();

        // No code-like content
        let ctx1 = LintContext::new("Just plain text", crate::config::MarkdownFlavor::Standard, None);
        assert!(
            rule.should_skip(&ctx1),
            "Should skip content without backticks or tildes"
        );

        // Has backticks
        let ctx2 = LintContext::new("Has `code`", crate::config::MarkdownFlavor::Standard, None);
        assert!(!rule.should_skip(&ctx2), "Should not skip content with backticks");

        // Has tildes
        let ctx3 = LintContext::new("Has ~~~", crate::config::MarkdownFlavor::Standard, None);
        assert!(!rule.should_skip(&ctx3), "Should not skip content with tildes");

        // Empty
        let ctx4 = LintContext::new("", crate::config::MarkdownFlavor::Standard, None);
        assert!(rule.should_skip(&ctx4), "Should skip empty content");
    }

    #[test]
    fn test_python_triplestring_fence_collision_fix() {
        // Reproduces GitHub issue #518: Python triple-quoted strings with embedded
        // markdown cause premature fence closure
        let content = "# Test\n\n```python\ndef f():\n    text = \"\"\"\n```json\n{}\n```\n\"\"\"\n```\n";
        let result = run_check(content).unwrap();
        assert_eq!(result.len(), 1, "Should detect collision in python block");
        assert!(result[0].fix.is_some(), "Warning should be marked as fixable");

        let fixed = run_fix(content).unwrap();
        assert!(
            fixed.contains("````python"),
            "Should upgrade opening fence to 4 backticks"
        );
        assert!(
            fixed.contains("````\n") || fixed.ends_with("````"),
            "Should upgrade closing fence to 4 backticks"
        );
        // Content between fences should be preserved
        assert!(fixed.contains("```json"), "Inner fences should be preserved as content");
    }

    #[test]
    fn test_warning_is_fixable() {
        // All MD070 warnings must have fix.is_some() so the fix coordinator calls fix()
        let content = "```markdown\n```python\ncode()\n```\n```\n";
        let result = run_check(content).unwrap();
        assert_eq!(result.len(), 1);
        assert!(
            result[0].fix.is_some(),
            "MD070 warnings must be marked fixable for the fix coordinator"
        );
    }

    #[test]
    fn test_fix_via_warning_struct_is_safe() {
        // The Fix on warnings is used directly by the LSP code action path.
        // It must produce valid output (not delete the fence or corrupt the file).
        let content = "```markdown\n```python\ncode()\n```\n```\n";
        let result = run_check(content).unwrap();
        assert_eq!(result.len(), 1);

        // Apply every edit of the Fix the way an LSP code action does.
        let fixed = apply_fix_edits(content, result[0].fix.as_ref().unwrap());

        // The fixed content should have upgraded fences
        assert!(
            fixed.contains("````markdown"),
            "Direct Fix application should upgrade opening fence, got: {fixed}"
        );
        assert!(
            fixed.contains("````\n") || fixed.ends_with("````"),
            "Direct Fix application should upgrade closing fence, got: {fixed}"
        );
        // Content should be preserved
        assert!(
            fixed.contains("```python"),
            "Inner content should be preserved, got: {fixed}"
        );
    }

    #[test]
    fn test_fix_via_warning_struct_python_block() {
        // Test the LSP code action path for a Python block where CommonMark's
        // closing fence differs from the user's intended closing fence.
        // CommonMark sees: ```python (line 1) closed by bare ``` (line 6).
        // User intended: ```python (line 1) closed by ``` (line 10).
        let content = "```python\ndef f():\n    text = \"\"\"\n```json\n{}\n```\n\"\"\"\n    print(text)\nf()\n```\n";
        let result = run_check(content).unwrap();
        assert_eq!(result.len(), 1);

        let fixed = apply_fix_edits(content, result[0].fix.as_ref().unwrap());

        // The Fix must cover the full intended block (lines 1-10), not just
        // the CommonMark-visible block (lines 1-6). Verify the fixed content
        // has one code block containing ALL the Python code.
        assert!(
            fixed.starts_with("````python\n"),
            "Should upgrade opening fence, got:\n{fixed}"
        );
        assert!(
            fixed.contains("````\n") || fixed.trim_end().ends_with("````"),
            "Should upgrade closing fence, got:\n{fixed}"
        );
        // ALL Python code must be between the fences
        let fence_start = fixed.find("````python\n").unwrap();
        let after_open = fence_start + "````python\n".len();
        let close_pos = fixed[after_open..]
            .find("\n````\n")
            .or_else(|| fixed[after_open..].find("\n````"));
        assert!(
            close_pos.is_some(),
            "Should have closing fence after content, got:\n{fixed}"
        );
        let block_content = &fixed[after_open..after_open + close_pos.unwrap()];
        assert!(
            block_content.contains("print(text)"),
            "print(text) must be inside the code block, got block:\n{block_content}"
        );
        assert!(
            block_content.contains("f()"),
            "f() must be inside the code block, got block:\n{block_content}"
        );
        assert!(
            block_content.contains("```json"),
            "Inner fences must be preserved as content, got block:\n{block_content}"
        );
    }

    #[test]
    fn test_fix_via_apply_warning_fixes() {
        // End-to-end test of the LSP fix path using apply_warning_fixes
        let content = "```markdown\n```python\ncode()\n```\n```\n";
        let result = run_check(content).unwrap();
        assert_eq!(result.len(), 1);

        let fixed = crate::utils::fix_utils::apply_warning_fixes(content, &result).unwrap();
        assert!(
            fixed.contains("````markdown"),
            "apply_warning_fixes should upgrade opening fence"
        );
        assert!(
            fixed.contains("````\n") || fixed.ends_with("````"),
            "apply_warning_fixes should upgrade closing fence"
        );

        // Re-check should find no issues
        let ctx2 = LintContext::new(&fixed, crate::config::MarkdownFlavor::Standard, None);
        let rule = MD070NestedCodeFence::new();
        let result2 = rule.check(&ctx2).unwrap();
        assert!(
            result2.is_empty(),
            "Re-check after LSP fix should find no issues, got: {:?}",
            result2.iter().map(|w| &w.message).collect::<Vec<_>>()
        );
    }

    /// Helper: run fix() then check() on the result, asserting 0 violations remain
    fn assert_fix_roundtrip(content: &str, label: &str) {
        let fixed = run_fix(content).unwrap();
        let rule = MD070NestedCodeFence::new();
        let ctx = LintContext::new(&fixed, crate::config::MarkdownFlavor::Standard, None);
        let remaining = rule.check(&ctx).unwrap();
        assert!(
            remaining.is_empty(),
            "[{label}] fix() should resolve all violations, but {n} remain: {msgs:?}\nFixed content:\n{fixed}",
            n = remaining.len(),
            msgs = remaining.iter().map(|w| &w.message).collect::<Vec<_>>(),
        );
    }

    #[test]
    fn test_fix_roundtrip_basic() {
        assert_fix_roundtrip("```markdown\n```python\ncode()\n```\n```\n", "basic collision");
    }

    #[test]
    fn test_fix_roundtrip_longer_inner_fence() {
        assert_fix_roundtrip("```markdown\n`````python\ncode()\n`````\n```\n", "longer inner fence");
    }

    #[test]
    fn test_fix_roundtrip_tilde_collision() {
        assert_fix_roundtrip("~~~markdown\n~~~python\ncode()\n~~~\n~~~\n", "tilde collision");
    }

    #[test]
    fn test_fix_roundtrip_info_string_attrs() {
        assert_fix_roundtrip(
            "```markdown {.highlight}\n```python\ncode()\n```\n```\n",
            "info string with attrs",
        );
    }

    #[test]
    fn test_fix_roundtrip_no_trailing_newline() {
        assert_fix_roundtrip("```markdown\n```python\ncode()\n```\n```", "no trailing newline");
    }

    #[test]
    fn test_fix_roundtrip_python_triple_string() {
        assert_fix_roundtrip(
            "# Test\n\n```python\ndef f():\n    text = \"\"\"\n```json\n{}\n```\n\"\"\"\n```\n",
            "python triple string",
        );
    }

    #[test]
    fn test_fix_roundtrip_deeply_nested() {
        assert_fix_roundtrip(
            "```markdown\n````markdown\n```python\ncode()\n```\n````\n```\n",
            "deeply nested fences",
        );
    }

    #[test]
    fn test_fix_roundtrip_real_world_docs() {
        let content = r#"```markdown
1. First item

   ```python
   code_in_list()
   ```

1. Second item

```
"#;
        assert_fix_roundtrip(content, "real world docs case");
    }

    #[test]
    fn test_fix_roundtrip_empty_lines() {
        assert_fix_roundtrip(
            "```markdown\n\n\n```python\n\ncode()\n\n```\n\n```\n",
            "empty lines between fences",
        );
    }

    #[test]
    fn test_fix_no_change_when_no_violations() {
        let content = "````markdown\n```python\ncode()\n```\n````\n";
        let fixed = run_fix(content).unwrap();
        assert_eq!(fixed, content, "fix() should not modify content with no violations");
    }

    #[test]
    fn test_fix_roundtrip_consecutive_collisions() {
        let content = r#"```markdown
```python
a()
```
```

```md
```ruby
b()
```
```
"#;
        // Fix and verify each collision is resolved
        let fixed = run_fix(content).unwrap();
        let rule = MD070NestedCodeFence::new();
        let ctx = LintContext::new(&fixed, crate::config::MarkdownFlavor::Standard, None);
        let remaining = rule.check(&ctx).unwrap();
        // At minimum the first block should be fixed; consecutive blocks may
        // require multiple passes but the first pass must not make things worse
        assert!(
            remaining.len() < 2,
            "fix() should resolve at least one collision, remaining: {remaining:?}",
        );
    }

    // ---- container-relative behavior -------------------------------------

    const IN_ITEM_VALID_CLOSER: &str = "- a\n\n  ```md\n  x\n     ```\n\n```text\ny\n```\n";

    #[test]
    fn test_closer_valid_relative_to_list_item_is_not_a_collision() {
        // Five raw spaces is three columns past the item's content column.
        assert!(run_check(IN_ITEM_VALID_CLOSER).unwrap().is_empty());
        assert_eq!(run_fix(IN_ITEM_VALID_CLOSER).unwrap(), IN_ITEM_VALID_CLOSER);
    }

    #[test]
    fn test_inner_fence_four_columns_inside_item_is_code() {
        let content = "- a\n\n  ```markdown\n      ```bash\n  ```\n";
        assert!(run_check(content).unwrap().is_empty());
    }

    fn assert_fixes(name: &str, content: &str, expected: &str, lines: (usize, usize)) {
        let warnings = run_check(content).unwrap();
        assert_eq!(warnings.len(), 1, "{name}: {warnings:?}");
        assert_eq!((warnings[0].line, warnings[0].end_line), lines, "{name}: range");
        let fixed = run_fix(content).unwrap();
        assert_eq!(fixed, expected, "{name}: fix output");
        assert!(run_check(&fixed).unwrap().is_empty(), "{name}: fixed output is clean");
        assert_eq!(run_fix(&fixed).unwrap(), fixed, "{name}: idempotent");
    }

    #[test]
    fn test_wide_ordered_item() {
        assert_fixes(
            "wide ordered item",
            "10. a\n\n    ```markdown\n    ```bash\n    x\n    ```\n    ```\n",
            "10. a\n\n    ````markdown\n    ```bash\n    x\n    ```\n    ````\n",
            (3, 7),
        );
    }

    #[test]
    fn test_nested_list_item() {
        assert_fixes(
            "nested list item",
            "- a\n  - b\n\n    ```markdown\n    ```bash\n    x\n    ```\n    ```\n",
            "- a\n  - b\n\n    ````markdown\n    ```bash\n    x\n    ```\n    ````\n",
            (4, 8),
        );
    }

    #[test]
    fn test_quote_in_list_item_and_list_item_in_quote() {
        assert_fixes(
            "quote in item",
            "- a\n\n  > ```markdown\n  > ```bash\n  > x\n  > ```\n  > ```\n",
            "- a\n\n  > ````markdown\n  > ```bash\n  > x\n  > ```\n  > ````\n",
            (3, 7),
        );
        assert_fixes(
            "item in quote",
            "> - a\n>\n>   ```markdown\n>   ```bash\n>   x\n>   ```\n>   ```\n",
            "> - a\n>\n>   ````markdown\n>   ```bash\n>   x\n>   ```\n>   ````\n",
            (3, 7),
        );
    }

    #[test]
    fn test_item_width_is_relative_to_a_quote_whose_indent_varies() {
        assert_fixes(
            "varying quote indent",
            "   > - a\n   >\n   >   ```md\n>   ```python\n>   x\n>   ```\n>   ```\n",
            "   > - a\n   >\n   >   ````md\n>   ```python\n>   x\n>   ```\n>   ````\n",
            (3, 7),
        );
    }

    #[test]
    fn test_task_list_item_width_comes_from_the_marker_line() {
        assert_fixes(
            "task item",
            "-  [ ] a\n\n   ```markdown\n   ```python\n   x\n   ```\n   ```\n",
            "-  [ ] a\n\n   ````markdown\n   ```python\n   x\n   ```\n   ````\n",
            (3, 7),
        );
    }

    #[test]
    fn test_tab_indented_item_content() {
        assert_fixes(
            "tab item",
            "-\ta\n\n\t```markdown\n\t```bash\n\tx\n\t```\n\t```\n",
            "-\ta\n\n\t````markdown\n\t```bash\n\tx\n\t```\n\t````\n",
            (3, 7),
        );
    }

    #[test]
    fn test_fence_opened_on_the_marker_line() {
        assert_fixes(
            "marker line opener",
            "- ```markdown\n  ```bash\n  x\n  ```\n  ```\n",
            "- ````markdown\n  ```bash\n  x\n  ```\n  ````\n",
            (1, 5),
        );
    }

    #[test]
    fn test_tilde_fences_in_a_quote() {
        assert_fixes(
            "tilde quote",
            "> ~~~markdown\n> ~~~bash\n> x\n> ~~~\n> ~~~\n",
            "> ~~~~markdown\n> ~~~bash\n> x\n> ~~~\n> ~~~~\n",
            (1, 5),
        );
    }

    #[test]
    fn test_search_stays_inside_the_enclosing_item() {
        // The bare fence in the next sibling item is not this block's closer.
        let content = "- a\n  - b\n\n    ```markdown\n    ```bash\n    x\n    ```\n- c\n\n  ```\n";
        assert_fixes(
            "sibling item",
            content,
            "- a\n  - b\n\n    ````markdown\n    ```bash\n    x\n    ````\n- c\n\n  ```\n",
            (4, 7),
        );
    }

    #[test]
    fn test_search_stays_inside_the_quote() {
        assert_fixes(
            "quote bound",
            "> ```markdown\n> ```bash\n> x\n> ```\n\n```\n",
            "> ````markdown\n> ```bash\n> x\n> ````\n\n```\n",
            (1, 4),
        );
    }

    #[test]
    fn test_lazy_line_ends_the_search() {
        assert_fixes(
            "lazy line",
            "- a\n\n  ```markdown\n  ```bash\n  x\n  ```\nlazy\n  ```\n",
            "- a\n\n  ````markdown\n  ```bash\n  x\n  ````\nlazy\n  ```\n",
            (3, 6),
        );
    }

    #[test]
    fn test_unicode_space_after_a_fence_does_not_close_it() {
        // U+00A0 after the run is info-string text, so it cannot be the closer.
        let content = "> ```markdown\n> ```python\n> x\n> ```\n> ```\u{a0}\n> tail\n";
        assert_fixes(
            "nbsp",
            content,
            "> ````markdown\n> ```python\n> x\n> ````\n> ```\u{a0}\n> tail\n",
            (1, 4),
        );
    }

    #[test]
    fn test_fix_edits_only_the_two_fence_runs() {
        let content = "- a\n\n  ```markdown   title  \n  ```bash\n  x\n  ```\n  ```   \n";
        let rule = MD070NestedCodeFence::new();
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 1);
        let fix = warnings[0].fix.as_ref().unwrap();
        let mut edits: Vec<_> = std::iter::once(fix).chain(fix.additional_edits.iter()).collect();
        edits.sort_by_key(|edit| edit.range.start);
        assert_eq!(edits.len(), 2, "opener run and closer run only");
        for edit in &edits {
            assert_eq!(&content[edit.range.clone()], "```");
            assert_eq!(edit.replacement, "````");
        }
        assert_eq!(
            rule.fix(&ctx).unwrap(),
            "- a\n\n  ````markdown   title  \n  ```bash\n  x\n  ```\n  ````   \n"
        );
    }

    #[test]
    fn test_crlf_and_missing_final_newline_are_preserved() {
        let rule = MD070NestedCodeFence::new();
        for content in [
            "> ```markdown\r\n> ```bash\r\n> x\r\n> ```\r\n> ```\r\n",
            "> ```markdown\r\n> ```bash\r\n> x\r\n> ```\r\n> ```",
        ] {
            let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
            let fixed = rule.fix(&ctx).unwrap();
            let expected = content.replacen("```markdown", "````markdown", 1);
            let last = expected.rfind("> ```").unwrap();
            let expected = format!("{}> ````{}", &expected[..last], &expected[last + 5..]);
            assert_eq!(fixed, expected);
        }
    }

    #[test]
    fn test_definition_body_is_not_judged() {
        // The definition ends before `Outside`; the unrelated fence is not a closer.
        let content = "Term\n: ```md\n  ```python\n  x\n  ```\n\nOutside\n\n```\n";
        assert!(run_check(content).unwrap().is_empty());
    }

    #[test]
    fn test_mdx_jsx_element_fences_are_still_checked() {
        let content = "<Comp>\n```markdown\n```python\nx\n```\n```\n</Comp>\n";
        let rule = MD070NestedCodeFence::new();
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::MDX, None);
        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 1, "{warnings:?}");
        assert_eq!(
            rule.fix(&ctx).unwrap(),
            "<Comp>\n````markdown\n```python\nx\n```\n````\n</Comp>\n"
        );
    }

    #[test]
    fn test_search_stops_at_any_independent_fence_with_an_info_string() {
        // A spaced or numeric info string opens an independent block; it is not absorbed.
        for opener in ["``` json", "```123"] {
            let content = format!("```markdown\n```python\nx\n```\n{opener}\ny\n```\n");
            let rule = MD070NestedCodeFence::new();
            let ctx = LintContext::new(&content, crate::config::MarkdownFlavor::Standard, None);
            let warnings = rule.check(&ctx).unwrap();
            assert_eq!(warnings.len(), 1, "{opener}: {warnings:?}");
            assert_eq!(
                warnings[0].end_line, 4,
                "{opener}: the block ends at the first bare fence"
            );
        }
    }

    #[test]
    fn test_search_skips_the_fences_of_a_nested_container() {
        // Each item's block ends at its own closer: the child item's closer
        // belongs to the child's block, not to the parent's.
        let content = "- item\n\n  ```markdown\n  ```python\n  x\n  ```\n\n  - item\n\n    ```markdown\n    ```python\n    x\n    ```\n";
        let warnings = run_check(content).unwrap();
        let spans: Vec<_> = warnings.iter().map(|w| (w.line, w.end_line)).collect();
        assert_eq!(spans, [(3, 6), (10, 13)], "{warnings:?}");
    }

    #[test]
    fn test_lazy_line_in_a_skipped_container_ends_the_search() {
        let content = "> ```md\n> ```py\n> x\n> ```\n> > ```js\n> > y\n> > ```\n> > p\nlazy\n> ```\n> tail\n> ```\n";
        let warnings = run_check(content).unwrap();
        let spans: Vec<_> = warnings.iter().map(|w| (w.line, w.end_line)).collect();
        assert_eq!(spans, [(1, 4)], "{warnings:?}");
    }

    #[test]
    fn test_mdx_commented_fence_does_not_hide_a_later_collision() {
        let content = "<!--\n```md\n-->\n\n```markdown\n```python\nx\n```\n```\n";
        let rule = MD070NestedCodeFence::new();
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::MDX, None);
        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 1, "{warnings:?}");
        assert_eq!(warnings[0].line, 5);
    }

    #[test]
    fn test_many_unclosed_fences_in_items_are_linear() {
        // Each item's fence is ended by its item; none can collide.
        let content = (0..2000).fold(String::new(), |mut acc, i| {
            acc.push_str(&format!("- {i}\n\n  ```md\n  x\n\n"));
            acc
        });
        let started = std::time::Instant::now();
        assert!(run_check(&content).unwrap().is_empty());
        assert!(started.elapsed() < std::time::Duration::from_secs(5));
    }
}
