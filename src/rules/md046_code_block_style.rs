use crate::rule::{Fix, FixCapability, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::rule_config_serde::{FlavorOverrideNotice, option_is_explicit};
use crate::utils::calculate_indentation_width_default;
use crate::utils::mdg;
use crate::utils::mkdocs_admonitions;
use crate::utils::mkdocs_tabs;
use crate::utils::range_utils::calculate_line_range;
use toml;

mod md046_config;
pub use md046_config::CodeBlockStyle;
use md046_config::MD046Config;

/// Reports the MDG style override once per process; see [`FlavorOverrideNotice`].
static MDG_STYLE_OVERRIDE: FlavorOverrideNotice = FlavorOverrideNotice::new();

/// Pre-computed context arrays for indented code block detection.
struct IndentContext<'a> {
    in_list_context: &'a [bool],
    in_tab_context: &'a [bool],
    in_admonition_context: &'a [bool],
    /// Lines belonging to a non-code container whose body can legitimately be
    /// indented by 4+ spaces or contain verbatim fence markers: HTML/MDX
    /// comments, raw HTML blocks, JSX blocks, mkdocstrings blocks, footnote
    /// definitions, and blockquotes.
    ///
    /// These lines are excluded from `detect_style`'s style tally, from
    /// `is_indented_code_block_with_context`, and from
    /// `categorize_indented_blocks`'s fence rewriting — keeping detection in
    /// lockstep with the warning-side skip list used in `check`.
    in_comment_or_html: &'a [bool],
    /// Per-line content column of the most recent list item this line
    /// belongs to (in list continuation), or None if not in list context.
    ///
    /// CommonMark places an indented code block within a list item only when
    /// the line's indent is at least `baseline + 4`. Without this, every
    /// continuation line gets the conservative "skip in list context" treatment
    /// — silently turning real list-internal code blocks into fmt no-ops.
    /// With this, the rule recognizes them, and the fence converter can emit
    /// fences at `baseline` spaces so the block stays attached to the bullet.
    list_item_baseline: &'a [Option<usize>],
}

/// Owned backing storage for [`IndentContext`], built once per `check`/`fix`
/// invocation by [`MD046CodeBlockStyle::build_indent_context`].
struct OwnedIndentContext {
    in_list_context: Vec<bool>,
    in_tab_context: Vec<bool>,
    in_admonition_context: Vec<bool>,
    in_comment_or_html: Vec<bool>,
    list_item_baseline: Vec<Option<usize>>,
}

impl OwnedIndentContext {
    fn borrow(&self) -> IndentContext<'_> {
        IndentContext {
            in_list_context: &self.in_list_context,
            in_tab_context: &self.in_tab_context,
            in_admonition_context: &self.in_admonition_context,
            in_comment_or_html: &self.in_comment_or_html,
            list_item_baseline: &self.list_item_baseline,
        }
    }
}

/// Rule MD046: Code block style
///
/// See [docs/md046.md](../../docs/md046.md) for full documentation, configuration, and examples.
///
/// This rule is triggered when code blocks do not use a consistent style (either fenced or indented).
#[derive(Clone)]
pub struct MD046CodeBlockStyle {
    config: MD046Config,
    /// Whether `style` came from the configuration rather than from the
    /// default. MDG enforces fenced either way; this only decides whether the
    /// user is told that the style they asked for was not adopted.
    style_explicit: bool,
}

impl MD046CodeBlockStyle {
    /// The fence `fix` opens when it converts an indented block.
    const FENCE: &'static str = "```";

    pub fn new(style: CodeBlockStyle) -> Self {
        Self {
            config: MD046Config { style },
            style_explicit: true,
        }
    }

    pub fn from_config_struct(config: MD046Config) -> Self {
        Self {
            config,
            style_explicit: false,
        }
    }

    /// Check if line has valid fence indentation per CommonMark spec (0-3 spaces)
    ///
    /// Per CommonMark 0.31.2: "An opening code fence may be indented 0-3 spaces."
    /// 4+ spaces of indentation makes it an indented code block instead.
    fn has_valid_fence_indent(line: &str) -> bool {
        calculate_indentation_width_default(line) < 4
    }

    /// Check fence indentation relative to an enclosing list item's content
    /// column. CommonMark applies the container prefix before its 0-3-space
    /// fence rule, so a nested fence can have 4+ leading spaces in the source.
    fn has_valid_fence_indent_at(line: &str, baseline: usize) -> bool {
        let indent = calculate_indentation_width_default(line);
        indent >= baseline && indent - baseline < 4
    }

    /// Check if a line is a valid fenced code block start per CommonMark spec
    ///
    /// Per CommonMark 0.31.2: "A code fence is a sequence of at least three consecutive
    /// backtick characters (`) or tilde characters (~). An opening code fence may be
    /// indented 0-3 spaces."
    ///
    /// This means 4+ spaces of indentation makes it an indented code block instead,
    /// where the fence characters become literal content.
    fn is_fenced_code_block_start(&self, line: &str) -> bool {
        if !Self::has_valid_fence_indent(line) {
            return false;
        }

        let trimmed = line.trim_start();
        trimmed.starts_with("```") || trimmed.starts_with("~~~")
    }

    fn is_fenced_code_block_start_at(&self, line: &str, baseline: usize) -> bool {
        if baseline == 0 {
            return self.is_fenced_code_block_start(line);
        }

        Self::has_valid_fence_indent_at(line, baseline)
            && (line.trim_start().starts_with("```") || line.trim_start().starts_with("~~~"))
    }

    fn is_closing_fence(line: &str, fence_char: char, opener_len: usize, baseline: usize) -> bool {
        if !Self::has_valid_fence_indent_at(line, baseline) {
            return false;
        }

        let trimmed = line.trim_start();
        let closer_len = trimmed.chars().take_while(|&ch| ch == fence_char).count();
        closer_len >= opener_len && closer_len > 0 && trimmed[closer_len..].trim().is_empty()
    }

    /// Remove up to `columns` visual columns of leading Markdown indentation.
    /// Tabs advance to four-column tab stops; when the boundary falls inside a
    /// tab, retain the unconsumed part as spaces so the payload column stays
    /// unchanged.
    fn strip_indentation_columns(line: &str, columns: usize) -> String {
        if columns == 0 {
            return line.to_string();
        }

        let mut width = 0usize;
        let mut consumed = 0usize;

        for (byte_index, ch) in line.char_indices() {
            let next_width = match ch {
                ' ' => width + 1,
                '\t' => ((width / 4) + 1) * 4,
                _ => break,
            };
            consumed = byte_index + ch.len_utf8();

            if next_width >= columns {
                let remainder = next_width - columns;
                let mut stripped = String::with_capacity(remainder + line.len() - consumed);
                stripped.extend(std::iter::repeat_n(' ', remainder));
                stripped.push_str(&line[consumed..]);
                return stripped;
            }

            width = next_width;
        }

        line[consumed..].to_string()
    }

    fn is_list_item(&self, line: &str) -> bool {
        let trimmed = line.trim_start();
        if trimmed.starts_with("- ") || trimmed.starts_with("* ") || trimmed.starts_with("+ ") {
            return true;
        }
        // Ordered list item: one or more leading digits immediately followed by
        // ". " or ") ". Checking the delimiter right after the digit run avoids
        // misclassifying prose like "2 results. More info." (which merely
        // contains ". ") as a list item.
        let after_digits = trimmed.trim_start_matches(|c: char| c.is_ascii_digit());
        after_digits.len() < trimmed.len() && (after_digits.starts_with(". ") || after_digits.starts_with(") "))
    }

    /// Check if a line is a footnote definition according to CommonMark footnote extension spec
    ///
    /// # Specification Compliance
    /// Based on commonmark-hs footnote extension and GitHub's implementation:
    /// - Format: `[^label]: content`
    /// - Labels cannot be empty or whitespace-only
    /// - Labels cannot contain line breaks (unlike regular link references)
    /// - Labels typically contain alphanumerics, hyphens, underscores (though some parsers are more permissive)
    ///
    /// # Examples
    /// Valid:
    /// - `[^1]: Footnote text`
    /// - `[^foo-bar]: Content`
    /// - `[^test_123]: More content`
    ///
    /// Invalid:
    /// - `[^]: No label`
    /// - `[^ ]: Whitespace only`
    /// - `[^]]: Extra bracket`
    fn is_footnote_definition(&self, line: &str) -> bool {
        let trimmed = line.trim_start();
        if !trimmed.starts_with("[^") || trimmed.len() < 5 {
            return false;
        }

        if let Some(close_bracket_pos) = trimmed.find("]:")
            && close_bracket_pos > 2
        {
            let label = &trimmed[2..close_bracket_pos];

            if label.trim().is_empty() {
                return false;
            }

            // Per spec: labels cannot contain line breaks (check for \r since \n can't appear in a single line)
            if label.contains('\r') {
                return false;
            }

            // Validate characters per GitHub's behavior: alphanumeric, hyphens, underscores only
            if label.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_') {
                return true;
            }
        }

        false
    }

    /// Pre-compute which lines are in block continuation context (lists, footnotes) with a single forward pass
    ///
    /// # Specification-Based Context Tracking
    /// This function implements CommonMark-style block continuation semantics:
    ///
    /// ## List Items
    /// - List items can contain multiple paragraphs and blocks
    /// - Content continues if indented appropriately
    /// - Context ends at structural boundaries (headings, horizontal rules) or column-0 paragraphs
    ///
    /// ## Footnotes
    /// Per commonmark-hs footnote extension and GitHub's implementation:
    /// - Footnote content continues as long as it's indented
    /// - Blank lines within footnotes don't terminate them (if next content is indented)
    /// - Non-indented content terminates the footnote
    /// - Similar to list items but can span more content
    ///
    /// # Performance
    /// O(n) single forward pass, replacing O(n²) backward scanning
    ///
    /// # Returns
    /// Boolean vector where `true` indicates the line is part of a list/footnote continuation
    fn precompute_block_continuation_context(&self, lines: &[&str]) -> Vec<bool> {
        let mut in_continuation_context = vec![false; lines.len()];
        let mut last_list_item_line: Option<usize> = None;
        let mut last_footnote_line: Option<usize> = None;
        let mut blank_line_count = 0;

        for (i, line) in lines.iter().enumerate() {
            let trimmed = line.trim_start();
            let indent_len = line.len() - trimmed.len();

            // Check if this is a list item
            if self.is_list_item(line) {
                last_list_item_line = Some(i);
                last_footnote_line = None; // List item ends any footnote context
                blank_line_count = 0;
                in_continuation_context[i] = true;
                continue;
            }

            // Check if this is a footnote definition
            if self.is_footnote_definition(line) {
                last_footnote_line = Some(i);
                last_list_item_line = None; // Footnote ends any list context
                blank_line_count = 0;
                in_continuation_context[i] = true;
                continue;
            }

            // Handle empty lines
            if line.trim().is_empty() {
                // Blank lines within continuations are allowed
                if last_list_item_line.is_some() || last_footnote_line.is_some() {
                    blank_line_count += 1;
                    in_continuation_context[i] = true;

                    // Per spec: multiple consecutive blank lines might terminate context
                    // GitHub allows multiple blank lines within footnotes if next content is indented
                    // We'll check on the next non-blank line
                }
                continue;
            }

            // Non-empty line - check for structural breaks or continuation
            if indent_len == 0 && !trimmed.is_empty() {
                // Content at column 0 (not indented)

                // Headings definitely end all contexts
                if trimmed.starts_with('#') {
                    last_list_item_line = None;
                    last_footnote_line = None;
                    blank_line_count = 0;
                    continue;
                }

                // Horizontal rules end all contexts
                if trimmed.starts_with("---") || trimmed.starts_with("***") {
                    last_list_item_line = None;
                    last_footnote_line = None;
                    blank_line_count = 0;
                    continue;
                }

                // Non-indented paragraph/content terminates contexts
                // But be conservative: allow some distance for lists
                if let Some(list_line) = last_list_item_line
                    && (i - list_line > 5 || blank_line_count > 1)
                {
                    last_list_item_line = None;
                }

                // For footnotes, non-indented content always terminates
                if last_footnote_line.is_some() {
                    last_footnote_line = None;
                }

                blank_line_count = 0;

                // If no active context, this is a regular line
                if last_list_item_line.is_none() && last_footnote_line.is_some() {
                    last_footnote_line = None;
                }
                continue;
            }

            // Indented content - part of continuation if we have active context
            if indent_len > 0 && (last_list_item_line.is_some() || last_footnote_line.is_some()) {
                in_continuation_context[i] = true;
                blank_line_count = 0;
            }
        }

        in_continuation_context
    }

    /// Per-line content column of the most recent list item this line
    /// belongs to (in list continuation), or None.
    ///
    /// Mirrors the iteration in `precompute_block_continuation_context` but
    /// captures the parsed list item's `content_column` from `LineInfo`.
    /// `is_indented_code_block_with_context` consults this so list-internal
    /// indented blocks are recognized iff their indent crosses
    /// `baseline + 4` — the CommonMark threshold for an indented code block
    /// inside a list item. The fix loop reuses the baseline to anchor the
    /// generated fences at the list-item content column.
    fn precompute_list_item_baseline(
        &self,
        ctx: &crate::lint_context::LintContext,
        lines: &[&str],
    ) -> Vec<Option<usize>> {
        let mut baselines = vec![None; lines.len()];
        let mut last_baseline: Option<usize> = None;
        let mut last_list_item_line: Option<usize> = None;
        let mut blank_line_count = 0usize;

        for (i, line) in lines.iter().enumerate() {
            let trimmed = line.trim_start();
            let indent_len = line.len() - trimmed.len();

            // List item line — read the parsed content column directly.
            if let Some(item) = ctx.line_info(i + 1).and_then(|li| li.list_item.as_ref()) {
                last_baseline = Some(item.content_column);
                last_list_item_line = Some(i);
                blank_line_count = 0;
                baselines[i] = last_baseline;
                continue;
            }

            // Blank line within continuation — propagate baseline.
            if line.trim().is_empty() {
                if last_baseline.is_some() {
                    blank_line_count += 1;
                    baselines[i] = last_baseline;
                }
                continue;
            }

            // Non-empty unindented content. Headings/HRs always end the list;
            // otherwise mirror the >5-line / >1-blank heuristic from
            // `precompute_block_continuation_context`.
            if indent_len == 0 {
                if trimmed.starts_with('#') || trimmed.starts_with("---") || trimmed.starts_with("***") {
                    last_baseline = None;
                    last_list_item_line = None;
                } else if let Some(list_line) = last_list_item_line
                    && (i - list_line > 5 || blank_line_count > 1)
                {
                    last_baseline = None;
                    last_list_item_line = None;
                }
                blank_line_count = 0;
                continue;
            }

            // Indented continuation — keep the baseline.
            if last_baseline.is_some() {
                baselines[i] = last_baseline;
                blank_line_count = 0;
            }
        }

        baselines
    }

    /// Check if a line is an indented code line using pre-computed context
    /// arrays. `prev_is_code` says whether the line above was classified as
    /// one, which is what lets a block continue past its first line.
    fn is_indented_code_block_with_context(
        &self,
        lines: &[&str],
        i: usize,
        is_mkdocs: bool,
        ctx: &IndentContext,
        prev_is_code: bool,
    ) -> bool {
        if i >= lines.len() {
            return false;
        }

        let line = lines[i];

        // A blank line is blank however wide its whitespace is: CommonMark
        // never opens an indented code block on one. Whether it sits INSIDE a
        // block is decided by `indented_block_lines`, from the code lines
        // around it.
        if line.trim().is_empty() {
            return false;
        }

        // Check if indented by at least 4 columns (accounting for tab expansion)
        let indent = calculate_indentation_width_default(line);
        if indent < 4 {
            return false;
        }

        // List/footnote continuation: only treat as a code block when the
        // indent crosses the list-item content baseline + 4. Without a
        // baseline (e.g. footnote definition continuation), keep the
        // conservative skip — those containers don't expose a column we can
        // anchor a fence to.
        if ctx.in_list_context[i] {
            let crosses_baseline = ctx
                .list_item_baseline
                .get(i)
                .copied()
                .flatten()
                .is_some_and(|base| indent >= base + 4);
            if !crosses_baseline {
                return false;
            }
        }

        // Skip if this is MkDocs tab content (pre-computed)
        if is_mkdocs && ctx.in_tab_context[i] {
            return false;
        }

        // Skip if this is MkDocs admonition content (pre-computed)
        // Admonitions are supported in MkDocs and other extended Markdown processors
        if is_mkdocs && ctx.in_admonition_context[i] {
            return false;
        }

        // Skip if inside an HTML/MDX comment, raw HTML block, JSX block,
        // mkdocstrings block, footnote definition, or blockquote. These
        // containers can legitimately hold 4+ space indented text that is
        // not a code block. Counting them would desync style detection from
        // the warning-side skip list in `check`.
        if ctx.in_comment_or_html.get(i).copied().unwrap_or(false) {
            return false;
        }

        // An indented code block starts after a blank line or continues from
        // a code line directly above. An indented line straight after a
        // paragraph line is a lazy continuation of that paragraph, however
        // deep its indent, and so is every indented line that follows it:
        // the answer for the line above has to be the classified one, not its
        // raw indent, or a run of continuation lines turns into code from its
        // second line on.
        let has_blank_line_before = i == 0 || lines[i - 1].trim().is_empty();
        has_blank_line_before || prev_is_code
    }

    /// First line at or after `start` that `block_lines` still counts as
    /// indented code, bounded by the byte offset `block_end`, or `None` when
    /// the block holds no such line.
    ///
    /// Under MDG a Gherkin table row is dropped from the block even though
    /// CommonMark counts it as indented code, so a block reported by the
    /// parser can start on a line the fix will leave alone. `check` reports
    /// the first line the fix actually converts, which keeps the two in step.
    fn first_code_block_line(
        ctx: &crate::lint_context::LintContext,
        block_lines: &[bool],
        start: usize,
        block_end: usize,
    ) -> Option<usize> {
        (start..block_lines.len())
            .take_while(|&idx| ctx.line_offsets.get(idx).is_some_and(|&offset| offset < block_end))
            .find(|&idx| block_lines[idx])
    }

    /// Per-line membership of the indented code blocks that style detection,
    /// block categorization and the fix all operate on.
    ///
    /// A code line is one `is_indented_code_block_with_context` accepts. A
    /// blank line belongs to a block only when a code line of that block
    /// precedes it and another follows it, with nothing but blank lines in
    /// between: CommonMark keeps interior blank lines inside an indented code
    /// block and leaves the blank lines before and after it outside. So
    /// `    a`, an empty line and `    b` form one block, and a whitespace-only
    /// line on its own is no block at all.
    ///
    /// Under MDG, Gherkin tables are dropped from the result. This is the only
    /// place that decision is made: `check`, `detect_style` and `fix` all read
    /// the array returned here, so they cannot disagree about what MD046
    /// converts.
    fn indented_block_lines(
        &self,
        lines: &[&str],
        is_mkdocs: bool,
        ictx: &IndentContext<'_>,
        ctx: &crate::lint_context::LintContext,
    ) -> Vec<bool> {
        let mut member = vec![false; lines.len()];
        for i in 0..lines.len() {
            let prev_is_code = i > 0 && member[i - 1];
            member[i] = self.is_indented_code_block_with_context(lines, i, is_mkdocs, ictx, prev_is_code);
        }

        // Fencing a run of Gherkin table rows would delete the table from the
        // Gherkin document, so they are withheld from indented-code detection.
        // That happens before blank lines are folded in, so each
        // blank-line-delimited run is judged on its own rows: an Examples table
        // followed by a blank line and a paragraph must keep the table out of
        // the block instead of being outvoted by the paragraph.
        if ctx.flavor == crate::config::MarkdownFlavor::MDG {
            let mut i = 0;
            while i < member.len() {
                if !member[i] {
                    i += 1;
                    continue;
                }
                let start = i;
                while i < member.len() && member[i] {
                    i += 1;
                }
                if lines[start..i].iter().all(|line| mdg::is_table_row(line)) {
                    member[start..i].fill(false);
                }
            }
        }

        let mut i = 0;
        while i < lines.len() {
            if !member[i] {
                i += 1;
                continue;
            }
            let mut next = i + 1;
            while next < lines.len() && lines[next].trim().is_empty() {
                next += 1;
            }
            if next < lines.len() && member[next] {
                member[i + 1..next].fill(true);
            }
            i = next;
        }

        member
    }

    /// Pre-compute which lines sit inside a non-code container whose body may
    /// legitimately be indented by 4+ spaces without being an indented code
    /// block: HTML comments, raw HTML blocks, JSX blocks, MDX comments,
    /// mkdocstrings blocks, footnote definitions, blockquotes, and front-matter.
    ///
    /// This mirrors the skip list used in `check` when emitting indented
    /// code-block warnings, keeping style detection and warning emission in
    /// lockstep.
    fn precompute_comment_or_html_context(ctx: &crate::lint_context::LintContext, line_count: usize) -> Vec<bool> {
        (0..line_count)
            .map(|i| {
                ctx.line_info(i + 1).is_some_and(|info| {
                    info.in_html_comment
                        || info.in_mdx_comment
                        || info.in_html_block
                        || info.in_jsx_block
                        || info.in_mkdocstrings
                        || info.in_footnote_definition
                        || info.blockquote.is_some()
                        || info.in_front_matter
                })
            })
            .collect()
    }

    /// Pre-compute which lines are in MkDocs tab context with a single forward pass
    fn precompute_mkdocs_tab_context(&self, lines: &[&str]) -> Vec<bool> {
        let mut in_tab_context = vec![false; lines.len()];
        let mut current_tab_indent: Option<usize> = None;

        for (i, line) in lines.iter().enumerate() {
            // Check if this is a tab marker
            if mkdocs_tabs::is_tab_marker(line) {
                let tab_indent = mkdocs_tabs::get_tab_indent(line).unwrap_or(0);
                current_tab_indent = Some(tab_indent);
                in_tab_context[i] = true;
                continue;
            }

            // If we have a current tab, check if this line is tab content
            if let Some(tab_indent) = current_tab_indent {
                if mkdocs_tabs::is_tab_content(line, tab_indent) {
                    in_tab_context[i] = true;
                } else if !line.trim().is_empty() && calculate_indentation_width_default(line) < 4 {
                    // Non-indented, non-empty line ends tab context
                    current_tab_indent = None;
                } else {
                    // Empty or indented line maintains tab context
                    in_tab_context[i] = true;
                }
            }
        }

        in_tab_context
    }

    /// Pre-compute which lines are in MkDocs admonition context with a single forward pass
    ///
    /// MkDocs admonitions use `!!!` or `???` markers followed by a type, and their content
    /// is indented by 4 spaces. This function marks all admonition markers and their
    /// indented content as being in an admonition context, preventing them from being
    /// incorrectly flagged as indented code blocks.
    ///
    /// Supports nested admonitions by maintaining a stack of active admonition contexts.
    fn precompute_mkdocs_admonition_context(&self, lines: &[&str]) -> Vec<bool> {
        let mut in_admonition_context = vec![false; lines.len()];
        // Stack of active admonition indentation levels (supports nesting)
        let mut admonition_stack: Vec<usize> = Vec::new();

        for (i, line) in lines.iter().enumerate() {
            let line_indent = calculate_indentation_width_default(line);

            // Check if this is an admonition marker
            if mkdocs_admonitions::is_admonition_start(line) {
                let adm_indent = mkdocs_admonitions::get_admonition_indent(line).unwrap_or(0);

                // Pop any admonitions that this one is not nested within
                while let Some(&top_indent) = admonition_stack.last() {
                    // New admonition must be indented more than parent to be nested
                    if adm_indent <= top_indent {
                        admonition_stack.pop();
                    } else {
                        break;
                    }
                }

                // Push this admonition onto the stack
                admonition_stack.push(adm_indent);
                in_admonition_context[i] = true;
                continue;
            }

            // Handle empty lines - they're valid within admonitions
            if line.trim().is_empty() {
                if !admonition_stack.is_empty() {
                    in_admonition_context[i] = true;
                }
                continue;
            }

            // For non-empty lines, check if we're still in any admonition context
            // Pop admonitions where the content indent requirement is not met
            while let Some(&top_indent) = admonition_stack.last() {
                // Content must be indented at least 4 spaces from the admonition marker
                if line_indent >= top_indent + 4 {
                    // This line is valid content for the top admonition (or one below)
                    break;
                } else {
                    // Not indented enough for this admonition - pop it
                    admonition_stack.pop();
                }
            }

            // If we're still in any admonition context, mark this line
            if !admonition_stack.is_empty() {
                in_admonition_context[i] = true;
            }
        }

        in_admonition_context
    }

    /// Build the pre-computed per-line context arrays that indented code
    /// block detection consults. One call per `check`/`fix` invocation.
    ///
    /// The list, tab, and admonition trackers here intentionally differ from
    /// the `LineInfo` flags (`in_list_block`, `in_content_tab`,
    /// `in_admonition`) that `LintContext` computes: the admonition tracker
    /// supports nesting via an indent stack, and the list tracker applies a
    /// conservative continuation heuristic (a list context survives up to 5
    /// unindented lines or one blank) tuned to avoid rewriting list
    /// continuations as code blocks. Only `in_comment_or_html` and the list
    /// item baselines project straight from `LintContext`.
    fn build_indent_context(
        &self,
        ctx: &crate::lint_context::LintContext,
        lines: &[&str],
        is_mkdocs: bool,
    ) -> OwnedIndentContext {
        OwnedIndentContext {
            in_list_context: self.precompute_block_continuation_context(lines),
            in_tab_context: if is_mkdocs {
                self.precompute_mkdocs_tab_context(lines)
            } else {
                vec![false; lines.len()]
            },
            in_admonition_context: if is_mkdocs {
                self.precompute_mkdocs_admonition_context(lines)
            } else {
                vec![false; lines.len()]
            },
            in_comment_or_html: Self::precompute_comment_or_html_context(ctx, lines.len()),
            list_item_baseline: self.precompute_list_item_baseline(ctx, lines),
        }
    }

    /// Categorize indented blocks for fix behavior
    ///
    /// Returns two vectors:
    /// - `is_misplaced`: Lines that are part of a complete misplaced fenced block (dedent only)
    /// - `contains_fences`: Lines that contain fence markers but aren't a complete block (skip fixing)
    ///
    /// A misplaced fenced block is a contiguous indented block that:
    /// 1. Starts with a valid fence opener (``` or ~~~)
    /// 2. Ends with a matching fence closer
    ///
    /// An unsafe block contains fence markers but isn't complete - wrapping would create invalid markdown.
    fn categorize_indented_blocks(&self, lines: &[&str], block_lines: &[bool]) -> (Vec<bool>, Vec<bool>) {
        let mut is_misplaced = vec![false; lines.len()];
        let mut contains_fences = vec![false; lines.len()];

        // Find contiguous indented blocks and categorize them
        let mut i = 0;
        while i < lines.len() {
            // Find the start of an indented block
            if !block_lines[i] {
                i += 1;
                continue;
            }

            // Found start of an indented block - collect all contiguous lines
            let block_start = i;
            let mut block_end = i;

            while block_end < lines.len() && block_lines[block_end] {
                block_end += 1;
            }

            // Now we have an indented block from block_start to block_end (exclusive)
            if block_end > block_start {
                let first_line = lines[block_start].trim_start();
                let last_line = lines[block_end - 1].trim_start();

                // Check if first line is a fence opener
                let is_backtick_fence = first_line.starts_with("```");
                let is_tilde_fence = first_line.starts_with("~~~");

                if is_backtick_fence || is_tilde_fence {
                    let fence_char = if is_backtick_fence { '`' } else { '~' };
                    let opener_len = first_line.chars().take_while(|&c| c == fence_char).count();

                    // Check if last line is a matching fence closer
                    let closer_fence_len = last_line.chars().take_while(|&c| c == fence_char).count();
                    let after_closer = &last_line[closer_fence_len..];

                    if closer_fence_len >= opener_len && after_closer.trim().is_empty() {
                        // Complete misplaced fenced block - safe to dedent
                        is_misplaced[block_start..block_end].fill(true);
                    } else {
                        // Incomplete fenced block - unsafe to wrap (would create nested fences)
                        contains_fences[block_start..block_end].fill(true);
                    }
                } else {
                    // Check if ANY line in the block contains fence markers
                    // If so, wrapping would create invalid markdown
                    let has_fence_markers = (block_start..block_end).any(|j| {
                        let trimmed = lines[j].trim_start();
                        trimmed.starts_with("```") || trimmed.starts_with("~~~")
                    });

                    if has_fence_markers {
                        contains_fences[block_start..block_end].fill(true);
                    }
                }
            }

            i = block_end;
        }

        (is_misplaced, contains_fences)
    }

    fn check_unclosed_code_blocks(&self, ctx: &crate::lint_context::LintContext) -> Vec<LintWarning> {
        let mut warnings = Vec::new();
        let lines = ctx.raw_lines();

        // Check if any fenced block has a markdown/md language tag
        let has_markdown_doc_block = ctx.code_block_details.iter().any(|d| {
            if !d.is_fenced {
                return false;
            }
            let lang = d.info_string.to_lowercase();
            lang.starts_with("markdown") || lang.starts_with("md")
        });

        // Skip unclosed block detection if document contains markdown documentation blocks
        // (they have nested fence examples that pulldown-cmark misparses)
        if has_markdown_doc_block {
            return warnings;
        }

        for detail in &ctx.code_block_details {
            if !detail.is_fenced {
                continue;
            }

            // Only check blocks that extend to EOF
            if detail.end != ctx.content.len() {
                continue;
            }

            // Find the line index for this block's start
            let opening_line_idx = match ctx.line_offsets.binary_search(&detail.start) {
                Ok(idx) => idx,
                Err(idx) => idx.saturating_sub(1),
            };

            // Determine fence marker from the actual line content
            let line = lines.get(opening_line_idx).unwrap_or(&"");
            let trimmed = line.trim();
            let fence_marker = if let Some(pos) = trimmed.find("```") {
                let count = trimmed[pos..].chars().take_while(|&c| c == '`').count();
                "`".repeat(count)
            } else if let Some(pos) = trimmed.find("~~~") {
                let count = trimmed[pos..].chars().take_while(|&c| c == '~').count();
                "~".repeat(count)
            } else {
                "```".to_string()
            };

            // Check if the last non-empty line is a valid closing fence
            let last_non_empty_line = lines.iter().rev().find(|l| !l.trim().is_empty()).unwrap_or(&"");
            let last_trimmed = last_non_empty_line.trim();
            let fence_char = fence_marker.chars().next().unwrap_or('`');

            let has_closing_fence = if fence_char == '`' {
                last_trimmed.starts_with("```") && {
                    let fence_len = last_trimmed.chars().take_while(|&c| c == '`').count();
                    last_trimmed[fence_len..].trim().is_empty()
                }
            } else {
                last_trimmed.starts_with("~~~") && {
                    let fence_len = last_trimmed.chars().take_while(|&c| c == '~').count();
                    last_trimmed[fence_len..].trim().is_empty()
                }
            };

            if !has_closing_fence {
                // Skip if inside HTML comment
                if ctx
                    .lines
                    .get(opening_line_idx)
                    .is_some_and(|info| info.in_html_comment || info.in_mdx_comment)
                {
                    continue;
                }

                let (start_line, start_col, end_line, end_col) = calculate_line_range(opening_line_idx + 1, line);

                warnings.push(LintWarning {
                    rule_name: Some(self.name().to_string()),
                    line: start_line,
                    column: start_col,
                    end_line,
                    end_column: end_col,
                    message: format!("Code block opened with '{fence_marker}' but never closed"),
                    severity: Severity::Warning,
                    fix: Some(Fix::new(
                        ctx.content.len()..ctx.content.len(),
                        format!("\n{fence_marker}"),
                    )),
                });
            }
        }

        warnings
    }

    /// Resolve the style MD046 should converge on.
    ///
    /// A Gherkin Doc String is only ever a backtick fence, so an indented block
    /// can never be one, and a configuration demanding indented code cannot be
    /// satisfied in this flavor. MDG therefore always converges on fenced:
    /// `consistent` resolves to fenced rather than to whichever style happens
    /// to be more common, and an explicit `indented` is not adopted.
    fn effective_target_style(
        &self,
        ctx: &crate::lint_context::LintContext,
        detect: impl FnOnce() -> CodeBlockStyle,
    ) -> CodeBlockStyle {
        if ctx.flavor == crate::config::MarkdownFlavor::MDG {
            self.warn_once_about_overridden_style();
            return CodeBlockStyle::Fenced;
        }

        match self.config.style {
            CodeBlockStyle::Consistent => {
                let detected = detect();
                if detected == CodeBlockStyle::Indented
                    && ctx.code_block_details.iter().any(|detail| {
                        detail.is_fenced
                            && !detail.info_string.trim().is_empty()
                            && Self::code_block_is_style_eligible(ctx, detail)
                    })
                {
                    // Indented blocks cannot carry a fence's info string. In
                    // consistent mode, choose the lossless direction even when
                    // indented blocks are more prevalent.
                    CodeBlockStyle::Fenced
                } else {
                    detected
                }
            }
            style => style,
        }
    }

    /// Whether a parsed code block participates in MD046 style selection.
    /// Keep this aligned with the container exclusions in `detect_style` and
    /// `check` so metadata in an ignored block cannot steer unrelated blocks.
    fn code_block_is_style_eligible(
        ctx: &crate::lint_context::LintContext,
        detail: &crate::utils::code_block_utils::CodeBlockDetail,
    ) -> bool {
        let Some(line_idx) = Self::code_block_start_line(ctx, detail) else {
            return false;
        };

        !ctx.lines.get(line_idx).is_some_and(|info| {
            info.in_html_comment
                || info.in_mdx_comment
                || info.in_html_block
                || info.in_jsx_block
                || info.in_mkdocstrings
                || info.in_footnote_definition
                || info.blockquote.is_some()
                || info.in_front_matter
        })
    }

    fn code_block_start_line(
        ctx: &crate::lint_context::LintContext,
        detail: &crate::utils::code_block_utils::CodeBlockDetail,
    ) -> Option<usize> {
        if detail.start >= ctx.content.len() {
            return None;
        }

        Some(match ctx.line_offsets.binary_search(&detail.start) {
            Ok(idx) => idx,
            Err(idx) => idx.saturating_sub(1),
        })
    }

    /// Fences that must remain as separators between otherwise adjacent code
    /// blocks. Converting every block in such a pair to indented form would
    /// merge two semantic blocks into one.
    fn fenced_separator_lines(ctx: &crate::lint_context::LintContext) -> std::collections::HashSet<usize> {
        let mut lines = std::collections::HashSet::new();

        for pair in ctx.code_block_details.windows(2) {
            let [previous, next] = pair else {
                continue;
            };
            if previous.end > next.start || next.start > ctx.content.len() {
                continue;
            }
            if !ctx.content[previous.end..next.start].trim().is_empty() {
                continue;
            }

            for detail in [previous, next] {
                if detail.is_fenced
                    && let Some(line) = Self::code_block_start_line(ctx, detail)
                {
                    lines.insert(line);
                }
            }
        }

        lines
    }

    /// Empty fenced blocks and blocks whose first or last payload line is
    /// blank. Indented code blocks cannot represent either shape: Markdown
    /// treats boundary blanks as ordinary whitespace outside the block, so
    /// these fences must remain.
    fn fenced_boundary_blank_lines(
        ctx: &crate::lint_context::LintContext,
        lines: &[&str],
        ictx: &IndentContext,
    ) -> std::collections::HashSet<usize> {
        let mut boundary_blank_lines = std::collections::HashSet::new();

        for detail in ctx.code_block_details.iter().filter(|detail| detail.is_fenced) {
            let Some(start) = Self::code_block_start_line(ctx, detail) else {
                continue;
            };
            let Some(opener) = lines.get(start) else {
                continue;
            };
            let baseline = ictx.list_item_baseline.get(start).copied().flatten().unwrap_or(0);
            let trimmed = opener.trim_start();
            if !Self::has_valid_fence_indent_at(opener, baseline) {
                continue;
            }
            let fence_char = if trimmed.starts_with("```") {
                '`'
            } else if trimmed.starts_with("~~~") {
                '~'
            } else {
                // A fence on the list-marker line is deliberately left alone
                // by the converter; it needs no boundary-blank preflight.
                continue;
            };
            let opener_len = trimmed.chars().take_while(|&ch| ch == fence_char).count();

            let mut block_end = start + 1;
            let mut closer = None;
            while block_end < lines.len()
                && ctx
                    .line_offsets
                    .get(block_end)
                    .is_some_and(|&offset| offset < detail.end)
            {
                if Self::is_closing_fence(lines[block_end], fence_char, opener_len, baseline) {
                    closer = Some(block_end);
                    break;
                }
                block_end += 1;
            }

            let payload_end = closer.unwrap_or(block_end);
            if start + 1 == payload_end
                || (start + 1 < payload_end
                    && (lines[start + 1].trim().is_empty() || lines[payload_end - 1].trim().is_empty()))
            {
                boundary_blank_lines.insert(start);
            }
        }

        boundary_blank_lines
    }

    /// Tell the user once that MDG did not adopt the style they configured.
    ///
    /// Only `indented` is worth reporting: it is the one setting MDG cannot
    /// satisfy. `consistent` asks for no particular form, and fenced is what
    /// MDG picks for it anyway.
    fn warn_once_about_overridden_style(&self) {
        if !self.style_explicit || self.config.style != CodeBlockStyle::Indented {
            return;
        }

        MDG_STYLE_OVERRIDE.report(
            "MD046",
            "style",
            "indented",
            "fenced",
            "a Gherkin Doc String is only ever a backtick fence",
        );
    }

    fn detect_style(
        &self,
        ctx: &crate::lint_context::LintContext,
        lines: &[&str],
        is_mkdocs: bool,
        ictx: &IndentContext,
    ) -> Option<CodeBlockStyle> {
        if lines.is_empty() {
            return None;
        }

        let block_lines = self.indented_block_lines(lines, is_mkdocs, ictx, ctx);

        let mut fenced_count = 0;
        let mut indented_count = 0;

        // Count all code block occurrences (prevalence-based approach).
        //
        // Both counts must ignore fence markers and indented text that live
        // inside a non-code container (HTML/MDX comments, raw HTML/JSX
        // blocks, mkdocstrings, footnote definitions, blockquotes) so that
        // the detected style stays in lockstep with the warning-side skip
        // list in `check`. Without this, a document that contains a single
        // real code block plus a fake fence or indented paragraph nested in
        // a comment is wrongly classified and the real block gets flagged.
        let mut in_fenced = false;
        let mut prev_was_indented = false;

        for (i, line) in lines.iter().enumerate() {
            let in_container = ictx.in_comment_or_html.get(i).copied().unwrap_or(false);

            // Lines inside Azure DevOps colon code fences are verbatim content.
            // Any fence markers they contain are not real block delimiters and
            // must not influence the fenced/indented style tally.
            if ctx.flavor.supports_colon_code_fences() && ctx.lines.get(i).is_some_and(|l| l.in_code_block) {
                prev_was_indented = false;
                continue;
            }

            // Lines inside MyST colon directives are structural containers, not code blocks.
            if ctx.flavor.supports_myst_directives() && ctx.lines.get(i).is_some_and(|l| l.in_myst_directive) {
                prev_was_indented = false;
                continue;
            }

            let baseline = ictx.list_item_baseline.get(i).copied().flatten().unwrap_or(0);
            if self.is_fenced_code_block_start_at(line, baseline) {
                if in_container {
                    // Fence marker inside a container — not a real fence,
                    // don't flip state or count it.
                    prev_was_indented = false;
                    continue;
                }
                if !in_fenced {
                    // Opening fence
                    fenced_count += 1;
                    in_fenced = true;
                } else {
                    // Closing fence
                    in_fenced = false;
                }
                prev_was_indented = false;
            } else if !in_fenced && block_lines[i] {
                // Count each continuous indented block once
                if !prev_was_indented {
                    indented_count += 1;
                }
                prev_was_indented = true;
            } else {
                prev_was_indented = false;
            }
        }

        if fenced_count == 0 && indented_count == 0 {
            None
        } else if fenced_count > 0 && indented_count == 0 {
            Some(CodeBlockStyle::Fenced)
        } else if fenced_count == 0 && indented_count > 0 {
            Some(CodeBlockStyle::Indented)
        } else if fenced_count >= indented_count {
            Some(CodeBlockStyle::Fenced)
        } else {
            Some(CodeBlockStyle::Indented)
        }
    }
}

impl Rule for MD046CodeBlockStyle {
    fn name(&self) -> &'static str {
        "MD046"
    }

    fn description(&self) -> &'static str {
        "Code blocks should use a consistent style"
    }

    fn check(&self, ctx: &crate::lint_context::LintContext) -> LintResult {
        // Early return for empty content
        if ctx.content.is_empty() {
            return Ok(Vec::new());
        }

        // Quick check for code blocks before processing
        if !ctx.content.contains("```")
            && !ctx.content.contains("~~~")
            && !ctx.content.contains("    ")
            && !ctx.content.contains('\t')
        {
            return Ok(Vec::new());
        }

        // First, always check for unclosed code blocks
        let unclosed_warnings = self.check_unclosed_code_blocks(ctx);

        // If we found unclosed blocks, return those warnings first
        if !unclosed_warnings.is_empty() {
            return Ok(unclosed_warnings);
        }

        // Check for code block style consistency
        let lines = ctx.raw_lines();
        let mut warnings = Vec::new();

        let is_mkdocs = ctx.flavor == crate::config::MarkdownFlavor::MkDocs;

        // Determine the target style
        let target_style = self.effective_target_style(ctx, || {
            let owned = self.build_indent_context(ctx, lines, is_mkdocs);
            let detected = self.detect_style(ctx, lines, is_mkdocs, &owned.borrow());
            detected.unwrap_or(CodeBlockStyle::Fenced)
        });

        // Under MDG, `indented_block_lines` is the single source of truth for
        // which indented lines are code and which are Gherkin Data/Examples
        // table rows. Reading the array `fix` converts from — rather than
        // re-deciding it here — is what keeps the two paths in agreement.
        let mdg_block_lines = (ctx.flavor == crate::config::MarkdownFlavor::MDG
            && ctx.code_block_details.iter().any(|detail| !detail.is_fenced))
        .then(|| {
            let owned = self.build_indent_context(ctx, lines, is_mkdocs);
            self.indented_block_lines(lines, is_mkdocs, &owned.borrow(), ctx)
        });

        // Iterate code_block_details directly (O(k) where k is number of blocks)
        let mut reported_indented_lines: std::collections::HashSet<usize> = std::collections::HashSet::new();

        for detail in &ctx.code_block_details {
            if detail.start >= ctx.content.len() || detail.end > ctx.content.len() {
                continue;
            }

            let start_line_idx = match ctx.line_offsets.binary_search(&detail.start) {
                Ok(idx) => idx,
                Err(idx) => idx.saturating_sub(1),
            };

            if detail.is_fenced {
                if target_style == CodeBlockStyle::Indented {
                    let line = lines.get(start_line_idx).unwrap_or(&"");

                    if ctx
                        .lines
                        .get(start_line_idx)
                        .is_some_and(|info| info.in_html_comment || info.in_mdx_comment || info.in_footnote_definition)
                    {
                        continue;
                    }

                    let (start_line, start_col, end_line, end_col) = calculate_line_range(start_line_idx + 1, line);
                    warnings.push(LintWarning {
                        rule_name: Some(self.name().to_string()),
                        line: start_line,
                        column: start_col,
                        end_line,
                        end_column: end_col,
                        message: "Use indented code blocks".to_string(),
                        severity: Severity::Warning,
                        fix: None,
                    });
                }
            } else {
                // Indented code block
                if target_style == CodeBlockStyle::Fenced {
                    // Under MDG the block may open on Gherkin table rows that
                    // are not code; the line to report is the first one the fix
                    // will fence, and a block of nothing but rows is no code
                    // block at all.
                    let start_line_idx = match &mdg_block_lines {
                        Some(block_lines) => {
                            match Self::first_code_block_line(ctx, block_lines, start_line_idx, detail.end) {
                                Some(idx) => idx,
                                None => continue,
                            }
                        }
                        None => start_line_idx,
                    };

                    if reported_indented_lines.contains(&start_line_idx) {
                        continue;
                    }

                    let line = lines.get(start_line_idx).unwrap_or(&"");

                    // Skip blocks in contexts that aren't real indented code blocks
                    if ctx.lines.get(start_line_idx).is_some_and(|info| {
                        info.in_html_comment
                            || info.in_mdx_comment
                            || info.in_html_block
                            || info.in_jsx_block
                            || info.in_mkdocstrings
                            || info.in_footnote_definition
                            || info.blockquote.is_some()
                            || info.in_front_matter
                    }) {
                        continue;
                    }

                    // Use pre-computed LineInfo for MkDocs container context
                    if is_mkdocs
                        && ctx
                            .lines
                            .get(start_line_idx)
                            .is_some_and(|info| info.in_admonition || info.in_content_tab)
                    {
                        continue;
                    }

                    reported_indented_lines.insert(start_line_idx);

                    let (start_line, start_col, end_line, end_col) = calculate_line_range(start_line_idx + 1, line);
                    warnings.push(LintWarning {
                        rule_name: Some(self.name().to_string()),
                        line: start_line,
                        column: start_col,
                        end_line,
                        end_column: end_col,
                        message: "Use fenced code blocks".to_string(),
                        severity: Severity::Warning,
                        fix: None,
                    });
                }
            }
        }

        // Sort warnings by line number for consistent output
        warnings.sort_by_key(|w| (w.line, w.column));

        Ok(warnings)
    }

    fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, LintError> {
        let content = ctx.content;
        if content.is_empty() {
            return Ok(String::new());
        }

        let lines = ctx.raw_lines();

        // Determine target style
        let is_mkdocs = ctx.flavor == crate::config::MarkdownFlavor::MkDocs;

        let owned = self.build_indent_context(ctx, lines, is_mkdocs);
        let ictx = owned.borrow();

        // The unclosed-fence repair at the end of this function is a repair
        // rather than a style conversion: `check` reports it before any style is
        // resolved, so the loop below has to run even when no block needs
        // converting.
        let target_style = self.effective_target_style(ctx, || {
            self.detect_style(ctx, lines, is_mkdocs, &ictx)
                .unwrap_or(CodeBlockStyle::Fenced)
        });

        let block_lines = self.indented_block_lines(lines, is_mkdocs, &ictx, ctx);
        let fenced_separator_lines = if target_style == CodeBlockStyle::Indented {
            Self::fenced_separator_lines(ctx)
        } else {
            std::collections::HashSet::new()
        };
        let fenced_boundary_blank_lines = if target_style == CodeBlockStyle::Indented {
            Self::fenced_boundary_blank_lines(ctx, lines, &ictx)
        } else {
            std::collections::HashSet::new()
        };
        // Trust the parser for opener identity. In particular, a fence may
        // open on a list-marker line (`- ```); its later closer must never be
        // mistaken for a fresh opener merely because it starts with backticks.
        let fenced_start_lines: std::collections::HashSet<usize> = ctx
            .code_block_details
            .iter()
            .filter(|detail| detail.is_fenced)
            .filter_map(|detail| Self::code_block_start_line(ctx, detail))
            .collect();
        let has_unsupported_fence_opener = ctx
            .code_block_details
            .iter()
            .filter(|detail| detail.is_fenced && Self::code_block_is_style_eligible(ctx, detail))
            .filter_map(|detail| Self::code_block_start_line(ctx, detail))
            .any(|line_index| {
                let Some(line) = lines.get(line_index) else {
                    return true;
                };
                let baseline = ictx.list_item_baseline.get(line_index).copied().flatten().unwrap_or(0);
                !self.is_fenced_code_block_start_at(line, baseline)
            });

        // Categorize indented blocks:
        // - misplaced_fence_lines: complete fenced blocks that were over-indented (safe to dedent)
        // - unsafe_fence_lines: contain fence markers but aren't complete (skip fixing to avoid broken output)
        let (misplaced_fence_lines, unsafe_fence_lines) = self.categorize_indented_blocks(lines, &block_lines);

        let mut result = String::with_capacity(content.len());
        let mut in_fenced_block = false;
        // Tracks the opening fence: (fence_char, opener_length).
        // Per CommonMark spec, the closing fence must use the same character and have
        // at least as many characters as the opener, with no info string.
        let mut fenced_fence_opener: Option<(char, usize)> = None;
        let mut in_indented_block = false;
        // Indent string emitted on the opening fence of the current
        // indented→fenced conversion (e.g. "  " for an indented block inside
        // a `- ` list item, "" at top level). Reused on close so the closing
        // fence sits at the same column as the opener.
        let mut current_block_fence_indent = String::new();

        // Track whether the current fenced block must be preserved. Inline
        // config can disable the rule, and indented code blocks have no
        // representation for a fence's info string.
        let mut current_block_must_stay_fenced = false;
        let mut current_fence_indent = 0usize;
        let mut current_fence_baseline = 0usize;
        let mut current_block_indented_prefix = String::from("    ");
        let mut converted_fenced_to_indented = false;
        let mut retained_structurally_unsafe_fence =
            target_style == CodeBlockStyle::Indented && has_unsupported_fence_opener;

        for (i, line) in lines.iter().enumerate() {
            let line_num = i + 1;
            let trimmed = line.trim_start();
            let list_baseline = ictx.list_item_baseline.get(i).copied().flatten();
            let fence_baseline = list_baseline.unwrap_or(0);

            // Handle fenced code blocks
            // Per CommonMark: fence must have 0-3 spaces of indentation
            if !in_fenced_block
                && fenced_start_lines.contains(&i)
                && Self::has_valid_fence_indent_at(line, fence_baseline)
                && (trimmed.starts_with("```") || trimmed.starts_with("~~~"))
            {
                // Check if inline config disables this rule for the opening fence
                let block_disabled = ctx.inline_config().is_rule_disabled(self.name(), line_num);
                in_fenced_block = true;
                let fence_char = if trimmed.starts_with("```") { '`' } else { '~' };
                let opener_len = trimmed.chars().take_while(|&c| c == fence_char).count();
                fenced_fence_opener = Some((fence_char, opener_len));
                current_fence_indent = calculate_indentation_width_default(line);
                current_fence_baseline = fence_baseline;
                current_block_indented_prefix = " ".repeat(fence_baseline + 4);
                let follows_list_item = i
                    .checked_sub(1)
                    .and_then(|previous| ictx.list_item_baseline.get(previous))
                    .copied()
                    .flatten()
                    .is_some();
                let would_become_list_prose = target_style == CodeBlockStyle::Indented
                    && list_baseline.is_none()
                    && (ictx.in_list_context.get(i).copied().unwrap_or(false) || follows_list_item);
                let would_interrupt_paragraph = target_style == CodeBlockStyle::Indented
                    && i > 0
                    && !lines[i - 1].trim().is_empty()
                    && ctx
                        .lines
                        .get(i - 1)
                        .is_some_and(crate::lint_context::LineInfo::is_paragraph_context)
                    && crate::lint_context::is_paragraph_text_line(lines[i - 1]);
                let would_merge_code_blocks = fenced_separator_lines.contains(&i);
                let would_lose_boundary_blanks = fenced_boundary_blank_lines.contains(&i);
                current_block_must_stay_fenced = block_disabled
                    || !trimmed[opener_len..].trim().is_empty()
                    || would_become_list_prose
                    || would_interrupt_paragraph
                    || would_merge_code_blocks
                    || would_lose_boundary_blanks;
                retained_structurally_unsafe_fence |= would_become_list_prose
                    || would_interrupt_paragraph
                    || would_merge_code_blocks
                    || would_lose_boundary_blanks;

                if current_block_must_stay_fenced {
                    // Inline config disables this rule, or converting would
                    // discard the fence's info string — preserve original.
                    result.push_str(line);
                    result.push('\n');
                } else if target_style == CodeBlockStyle::Indented {
                    // Skip the opening fence
                    in_indented_block = true;
                    converted_fenced_to_indented = true;
                } else {
                    // Keep the fenced block
                    result.push_str(line);
                    result.push('\n');
                }
            } else if in_fenced_block && fenced_fence_opener.is_some() {
                let (fence_char, opener_len) = fenced_fence_opener.unwrap();
                // Per CommonMark: closing fence uses the same character, has at least as
                // many characters as the opener, and has no info string (only optional trailing spaces).
                let is_closer = Self::is_closing_fence(line, fence_char, opener_len, current_fence_baseline);
                if is_closer {
                    in_fenced_block = false;
                    fenced_fence_opener = None;
                    in_indented_block = false;

                    if current_block_must_stay_fenced {
                        result.push_str(line);
                        result.push('\n');
                    } else if target_style == CodeBlockStyle::Indented {
                        // Skip the closing fence
                    } else {
                        // Keep the fenced block
                        result.push_str(line);
                        result.push('\n');
                    }
                    current_block_must_stay_fenced = false;
                    current_fence_indent = 0;
                    current_fence_baseline = 0;
                    current_block_indented_prefix.clear();
                } else if current_block_must_stay_fenced {
                    // Preserve every line of a block whose opener was kept.
                    result.push_str(line);
                    result.push('\n');
                } else if target_style == CodeBlockStyle::Indented {
                    // Convert content inside fenced block to indented.
                    // IMPORTANT: Preserve the original line content (including internal indentation);
                    // don't use trimmed, as that would strip internal code indentation.
                    // Leave blank lines empty so we don't emit "    " (trailing
                    // whitespace), which MD009 would flag and which would break
                    // idempotency on a second fix pass.
                    if !line.is_empty() {
                        // CommonMark removes up to the opening fence's indent
                        // from each body line. Remove the same source prefix
                        // before adding the indented-code prefix so parsed code
                        // content remains byte-for-byte equivalent.
                        let body = Self::strip_indentation_columns(line, current_fence_indent);
                        result.push_str(&current_block_indented_prefix);
                        result.push_str(&body);
                    }
                    result.push('\n');
                } else {
                    // Keep fenced block content as is
                    result.push_str(line);
                    result.push('\n');
                }
            } else if block_lines[i] {
                // This is an indented code block

                // Respect inline disable comments
                if ctx.inline_config().is_rule_disabled(self.name(), line_num) {
                    result.push_str(line);
                    result.push('\n');
                    continue;
                }

                // Check if we need to start a new fenced block
                let prev_line_is_indented = i > 0 && block_lines[i - 1];

                if target_style == CodeBlockStyle::Fenced {
                    // Anchor fences at the list-item content baseline when
                    // converting a list-internal indented block (e.g. column
                    // 2 for `- `), so the new fenced block stays attached
                    // to the bullet. Top-level indented blocks have no
                    // baseline → fences sit at column 0.
                    let baseline = ictx.list_item_baseline.get(i).copied().flatten().unwrap_or(0);
                    // Per CommonMark, the indented-code prefix is exactly 4
                    // spaces past the surrounding container's content
                    // column. Strip those 4 spaces (not all leading
                    // whitespace) so any internal indentation past that
                    // point is preserved verbatim in the fenced body. An
                    // interior blank line carries no content, so it is
                    // emitted empty rather than as leftover whitespace.
                    let body = if line.trim().is_empty() {
                        String::new()
                    } else {
                        Self::strip_indentation_columns(line, 4)
                    };

                    // Check if this line is part of a misplaced fenced block
                    // (pre-computed block-level analysis, not per-line)
                    if misplaced_fence_lines[i] {
                        // Just remove the indentation - this is a complete misplaced fenced block
                        result.push_str(line.trim_start());
                        result.push('\n');
                    } else if unsafe_fence_lines[i] {
                        // This block contains fence markers but isn't a complete fenced block
                        // Wrapping would create invalid nested fences - keep as-is (don't fix)
                        result.push_str(line);
                        result.push('\n');
                    } else if !prev_line_is_indented && !in_indented_block {
                        // Start of a new indented block that should be fenced
                        current_block_fence_indent = " ".repeat(baseline);
                        result.push_str(&current_block_fence_indent);
                        result.push_str(Self::FENCE);
                        result.push('\n');
                        result.push_str(&body);
                        result.push('\n');
                        in_indented_block = true;
                    } else {
                        // Inside an indented block
                        result.push_str(&body);
                        result.push('\n');
                    }

                    // Check if this is the end of the indented block
                    let next_line_is_indented = i < lines.len() - 1 && block_lines[i + 1];
                    // Don't close if this is an unsafe block (kept as-is)
                    if !next_line_is_indented
                        && in_indented_block
                        && !misplaced_fence_lines[i]
                        && !unsafe_fence_lines[i]
                    {
                        result.push_str(&current_block_fence_indent);
                        result.push_str(Self::FENCE);
                        result.push('\n');
                        in_indented_block = false;
                        current_block_fence_indent.clear();
                    }
                } else {
                    // Keep indented block as is
                    result.push_str(line);
                    result.push('\n');
                }
            } else {
                // Regular line
                if in_indented_block && target_style == CodeBlockStyle::Fenced {
                    result.push_str(&current_block_fence_indent);
                    result.push_str(Self::FENCE);
                    result.push('\n');
                    in_indented_block = false;
                    current_block_fence_indent.clear();
                }

                result.push_str(line);
                result.push('\n');
            }
        }

        // Close any remaining blocks
        if in_indented_block && target_style == CodeBlockStyle::Fenced {
            result.push_str(&current_block_fence_indent);
            result.push_str(Self::FENCE);
            result.push('\n');
        }

        // Close any unclosed fenced blocks.
        // Only close if check() also confirms this block is unclosed. The line-by-line
        // fence scanner in fix() can disagree with pulldown-cmark on block boundaries
        // (e.g., markdown documentation blocks with nested fence examples), so we use
        // check_unclosed_code_blocks() as the authoritative source of truth.
        if let Some((fence_char, opener_len)) = fenced_fence_opener
            && in_fenced_block
        {
            let has_unclosed_violation = !self.check_unclosed_code_blocks(ctx).is_empty();
            // A converted untagged block needs no closer: the indentation is
            // its complete delimiter. Preserved/tagged fences still need the
            // missing closer repaired.
            if has_unclosed_violation && (target_style != CodeBlockStyle::Indented || current_block_must_stay_fenced) {
                let closer: String = std::iter::repeat_n(fence_char, opener_len).collect();
                result.push_str(&closer);
                result.push('\n');
            }
        }

        // Remove trailing newline if original didn't have one
        if !content.ends_with('\n') && result.ends_with('\n') {
            result.pop();
        }

        if retained_structurally_unsafe_fence && self.config.style == CodeBlockStyle::Consistent {
            return Self::new(CodeBlockStyle::Fenced).fix(ctx);
        }

        if converted_fenced_to_indented {
            let reparsed_block_count = crate::utils::CodeBlockUtils::detect_code_blocks(&result).len();
            if reparsed_block_count != ctx.code_block_details.len() {
                // A fenced block can interrupt structures that an indented
                // block cannot. In consistent mode, fenced is the only
                // lossless way to converge; an explicit indented preference
                // is instead left unchanged.
                if self.config.style == CodeBlockStyle::Consistent {
                    return Self::new(CodeBlockStyle::Fenced).fix(ctx);
                }

                return Ok(content.to_string());
            }
        }

        Ok(result)
    }

    /// Get the category of this rule for selective processing
    fn category(&self) -> RuleCategory {
        RuleCategory::CodeBlock
    }

    fn fix_capability(&self) -> FixCapability {
        // Tagged fences and conversions that would change CommonMark block
        // structure are intentionally retained rather than fixed lossily.
        FixCapability::ConditionallyFixable
    }

    /// Check if this rule should be skipped
    fn should_skip(&self, ctx: &crate::lint_context::LintContext) -> bool {
        // Skip if content is empty or unlikely to contain code blocks
        // Note: indented code blocks use 4 spaces, can't optimize that easily
        ctx.content.is_empty() || (!ctx.likely_has_code() && !ctx.has_char('~') && !ctx.content.contains("    "))
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    crate::impl_rule_config_sections!(MD046Config);

    fn from_config(config: &crate::config::Config) -> Box<dyn Rule>
    where
        Self: Sized,
    {
        let rule_config = crate::rule_config_serde::load_rule_config::<MD046Config>(config);
        let style_explicit = option_is_explicit(config, "MD046", "style");

        Box::new(Self {
            config: rule_config,
            style_explicit,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lint_context::LintContext;

    /// Test helper: detect_style with automatic context computation.
    ///
    /// The container context (HTML/MDX comments, HTML/JSX blocks,
    /// mkdocstrings, footnote definitions, blockquotes) is not populated by
    /// this helper — callers that need to exercise those paths should go
    /// through the full `rule.check(&ctx)` entry point so the real LineInfo
    /// is computed from a `LintContext`.
    ///
    /// Colon fence exclusion is also not active here: tests that need Azure
    /// DevOps colon fence skipping must use the full `check` entry point with
    /// an `AzureDevOps` flavor `LintContext`.
    fn detect_style_from_content(rule: &MD046CodeBlockStyle, content: &str, is_mkdocs: bool) -> Option<CodeBlockStyle> {
        let flavor = if is_mkdocs {
            crate::config::MarkdownFlavor::MkDocs
        } else {
            crate::config::MarkdownFlavor::Standard
        };
        let ctx = LintContext::new(content, flavor, None);
        let lines: Vec<&str> = content.lines().collect();
        let in_list_context = rule.precompute_block_continuation_context(&lines);
        let in_tab_context = if is_mkdocs {
            rule.precompute_mkdocs_tab_context(&lines)
        } else {
            vec![false; lines.len()]
        };
        let in_admonition_context = if is_mkdocs {
            rule.precompute_mkdocs_admonition_context(&lines)
        } else {
            vec![false; lines.len()]
        };
        let in_comment_or_html = vec![false; lines.len()];
        // List baseline is None for every line: this helper preserves the
        // pre-baseline behavior where any list-context line is conservatively
        // skipped. Tests that need list-internal indented code blocks
        // recognized must drive the rule through `check`/`fix` with a real
        // `LintContext`.
        let list_item_baseline: Vec<Option<usize>> = vec![None; lines.len()];
        let ictx = IndentContext {
            in_list_context: &in_list_context,
            in_tab_context: &in_tab_context,
            in_admonition_context: &in_admonition_context,
            in_comment_or_html: &in_comment_or_html,
            list_item_baseline: &list_item_baseline,
        };
        rule.detect_style(&ctx, &lines, is_mkdocs, &ictx)
    }

    #[test]
    fn test_fenced_code_block_detection() {
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Fenced);
        assert!(rule.is_fenced_code_block_start("```"));
        assert!(rule.is_fenced_code_block_start("```rust"));
        assert!(rule.is_fenced_code_block_start("~~~"));
        assert!(rule.is_fenced_code_block_start("~~~python"));
        assert!(rule.is_fenced_code_block_start("  ```"));
        assert!(!rule.is_fenced_code_block_start("``"));
        assert!(!rule.is_fenced_code_block_start("~~"));
        assert!(!rule.is_fenced_code_block_start("Regular text"));
    }

    #[test]
    fn test_fix_capability_is_conditional() {
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Indented);
        assert_eq!(rule.fix_capability(), FixCapability::ConditionallyFixable);
    }

    #[test]
    fn test_consistent_style_with_fenced_blocks() {
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Consistent);
        let content = "```\ncode\n```\n\nMore text\n\n```\nmore code\n```";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // All blocks are fenced, so consistent style should be OK
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_consistent_style_with_indented_blocks() {
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Consistent);
        let content = "Text\n\n    code\n    more code\n\nMore text\n\n    another block";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // All blocks are indented, so consistent style should be OK
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_consistent_style_mixed() {
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Consistent);
        let content = "```\nfenced code\n```\n\nText\n\n    indented code\n\nMore";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Mixed styles should be flagged
        assert!(!result.is_empty());
    }

    #[test]
    fn test_fenced_style_with_indented_blocks() {
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Fenced);
        let content = "Text\n\n    indented code\n    more code\n\nMore text";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Indented blocks should be flagged when fenced style is required
        assert!(!result.is_empty());
        assert!(result[0].message.contains("Use fenced code blocks"));
    }

    #[test]
    fn test_fenced_style_with_tab_indented_blocks() {
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Fenced);
        let content = "Text\n\n\ttab indented code\n\tmore code\n\nMore text";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Tab-indented blocks should also be flagged when fenced style is required
        assert!(!result.is_empty());
        assert!(result[0].message.contains("Use fenced code blocks"));
    }

    #[test]
    fn test_fenced_style_with_mixed_whitespace_indented_blocks() {
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Fenced);
        // 2 spaces + tab = 4 columns due to tab expansion (tab goes to column 4)
        let content = "Text\n\n  \tmixed indent code\n  \tmore code\n\nMore text";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Mixed whitespace indented blocks should also be flagged
        assert!(
            !result.is_empty(),
            "Mixed whitespace (2 spaces + tab) should be detected as indented code"
        );
        assert!(result[0].message.contains("Use fenced code blocks"));
    }

    #[test]
    fn test_fenced_style_with_one_space_tab_indent() {
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Fenced);
        // 1 space + tab = 4 columns (tab expands to next tab stop at column 4)
        let content = "Text\n\n \ttab after one space\n \tmore code\n\nMore text";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert!(!result.is_empty(), "1 space + tab should be detected as indented code");
        assert!(result[0].message.contains("Use fenced code blocks"));
    }

    #[test]
    fn test_indented_style_with_fenced_blocks() {
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Indented);
        let content = "Text\n\n```\nfenced code\n```\n\nMore text";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Fenced blocks should be flagged when indented style is required
        assert!(!result.is_empty());
        assert!(result[0].message.contains("Use indented code blocks"));
    }

    #[test]
    fn test_unclosed_code_block() {
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Fenced);
        let content = "```\ncode without closing fence";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("never closed"));
    }

    #[test]
    fn test_nested_code_blocks() {
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Fenced);
        let content = "```\nouter\n```\n\ninner text\n\n```\ncode\n```";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // This should parse as two separate code blocks
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_fix_indented_to_fenced() {
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Fenced);
        let content = "Text\n\n    code line 1\n    code line 2\n\nMore text";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        assert!(fixed.contains("```\ncode line 1\ncode line 2\n```"));
    }

    #[test]
    fn test_fix_fenced_to_indented() {
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Indented);
        let content = "Text\n\n```\ncode line 1\ncode line 2\n```\n\nMore text";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        assert!(fixed.contains("    code line 1\n    code line 2"));
        assert!(!fixed.contains("```"));
    }

    #[test]
    fn test_fix_fenced_to_indented_blank_lines_have_no_trailing_spaces() {
        // A blank line inside a fenced block must become an empty line, not
        // "    " (four trailing spaces), which would violate MD009 and break
        // idempotency on the second fix pass.
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Indented);
        let content = "Text\n\n```\ncode line 1\n\ncode line 2\n```\n\nMore text";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        for line in fixed.lines() {
            assert!(
                line.is_empty() || !line.trim_end().is_empty() || line == line.trim_end(),
                "no line may have trailing whitespace, got {line:?}"
            );
            assert_ne!(line, "    ", "blank line was indented to trailing spaces");
        }
        // The blank line between the two code lines is preserved as empty.
        assert!(fixed.contains("    code line 1\n\n    code line 2"));
    }

    #[test]
    fn test_is_list_item_requires_delimiter_after_digits() {
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Consistent);
        // Real ordered list items.
        assert!(rule.is_list_item("1. First"));
        assert!(rule.is_list_item("42) Item"));
        assert!(rule.is_list_item("  3. Indented item"));
        // Bullet list items.
        assert!(rule.is_list_item("- bullet"));
        assert!(rule.is_list_item("* bullet"));
        // Prose starting with a digit but containing ". " or ") " mid-sentence
        // is NOT a list item.
        assert!(!rule.is_list_item("2 results. More info."));
        assert!(!rule.is_list_item("3 options (a, b) here"));
        assert!(!rule.is_list_item("100 items in stock. Buy now"));
    }

    #[test]
    fn test_fix_fenced_to_indented_preserves_internal_indentation() {
        // Issue #270: When converting fenced code to indented, internal indentation must be preserved
        // HTML templates, Python, etc. rely on proper indentation
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Indented);
        let content = r#"# Test

```
<!doctype html>
<html>
  <head>
    <title>Test</title>
  </head>
</html>
```
"#;
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        // The internal indentation (2 spaces for <head>, 4 for <title>) must be preserved
        // Each line gets 4 spaces prepended for the indented code block
        assert!(
            fixed.contains("      <head>"),
            "Expected 6 spaces before <head> (4 for code block + 2 original), got:\n{fixed}"
        );
        assert!(
            fixed.contains("        <title>"),
            "Expected 8 spaces before <title> (4 for code block + 4 original), got:\n{fixed}"
        );
        assert!(!fixed.contains("```"), "Fenced markers should be removed");
    }

    #[test]
    fn test_fix_fenced_to_indented_preserves_python_indentation() {
        // Issue #270: Python is indentation-sensitive - must preserve internal structure
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Indented);
        let content = r#"# Python Example

```
def greet(name):
    if name:
        print(f"Hello, {name}!")
    else:
        print("Hello, World!")
```
"#;
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        // Python indentation must be preserved exactly
        assert!(
            fixed.contains("    def greet(name):"),
            "Function def should have 4 spaces (code block indent)"
        );
        assert!(
            fixed.contains("        if name:"),
            "if statement should have 8 spaces (4 code + 4 Python)"
        );
        assert!(
            fixed.contains("            print"),
            "print should have 12 spaces (4 code + 8 Python)"
        );
    }

    #[test]
    fn test_fix_fenced_to_indented_preserves_yaml_indentation() {
        // Issue #270: YAML is also indentation-sensitive
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Indented);
        let content = r#"# Config

```
server:
  host: localhost
  port: 8080
  ssl:
    enabled: true
    cert: /path/to/cert
```
"#;
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        assert!(fixed.contains("    server:"), "Root key should have 4 spaces");
        assert!(fixed.contains("      host:"), "First level should have 6 spaces");
        assert!(fixed.contains("      ssl:"), "ssl key should have 6 spaces");
        assert!(fixed.contains("        enabled:"), "Nested ssl should have 8 spaces");
    }

    #[test]
    fn test_fix_fenced_to_indented_preserves_empty_lines() {
        // Blank lines within a converted code block stay blank: they keep their
        // place but must not gain the 4-space prefix (that would be trailing
        // whitespace).
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Indented);
        let content = "```\nline1\n\nline2\n```\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        // Content lines are indented; the blank line between them stays empty.
        assert!(fixed.contains("    line1"), "line1 should be indented");
        assert!(fixed.contains("    line2"), "line2 should be indented");
        assert!(
            fixed.contains("    line1\n\n    line2"),
            "blank line must stay empty, got {fixed:?}"
        );
    }

    #[test]
    fn test_fix_fenced_to_indented_multiple_blocks() {
        // Multiple fenced blocks should all preserve their indentation
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Indented);
        let content = r#"# Doc

```
def foo():
    pass
```

Text between.

```
key:
  value: 1
```
"#;
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        assert!(fixed.contains("    def foo():"), "Python def should be indented");
        assert!(fixed.contains("        pass"), "Python body should have 8 spaces");
        assert!(fixed.contains("    key:"), "YAML root should have 4 spaces");
        assert!(fixed.contains("      value:"), "YAML nested should have 6 spaces");
        assert!(!fixed.contains("```"), "No fence markers should remain");
    }

    #[test]
    fn test_fix_unclosed_block() {
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Fenced);
        let content = "```\ncode without closing";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        // Should add closing fence
        assert!(fixed.ends_with("```"));
    }

    #[test]
    fn test_code_block_in_list() {
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Fenced);
        let content = "- List item\n    code in list\n    more code\n- Next item";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Code in lists should not be flagged
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_detect_style_fenced() {
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Consistent);
        let content = "```\ncode\n```";
        let style = detect_style_from_content(&rule, content, false);

        assert_eq!(style, Some(CodeBlockStyle::Fenced));
    }

    #[test]
    fn test_detect_style_indented() {
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Consistent);
        let content = "Text\n\n    code\n\nMore";
        let style = detect_style_from_content(&rule, content, false);

        assert_eq!(style, Some(CodeBlockStyle::Indented));
    }

    #[test]
    fn test_detect_style_none() {
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Consistent);
        let content = "No code blocks here";
        let style = detect_style_from_content(&rule, content, false);

        assert_eq!(style, None);
    }

    #[test]
    fn test_tilde_fence() {
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Fenced);
        let content = "~~~\ncode\n~~~";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Tilde fences should be accepted as fenced blocks
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_language_specification() {
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Fenced);
        let content = "```rust\nfn main() {}\n```";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_empty_content() {
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Fenced);
        let content = "";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_default_config() {
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Consistent);
        let (name, _config) = rule.default_config_section().unwrap();
        assert_eq!(name, "MD046");
    }

    #[test]
    fn test_markdown_documentation_block() {
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Fenced);
        let content = "```markdown\n# Example\n\n```\ncode\n```\n\nText\n```";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Nested code blocks in markdown documentation should be allowed
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_preserve_trailing_newline() {
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Fenced);
        let content = "```\ncode\n```\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        assert_eq!(fixed, content);
    }

    #[test]
    fn test_mkdocs_tabs_not_flagged_as_indented_code() {
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Fenced);
        let content = r#"# Document

=== "Python"

    This is tab content
    Not an indented code block

    ```python
    def hello():
        print("Hello")
    ```

=== "JavaScript"

    More tab content here
    Also not an indented code block"#;

        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::MkDocs, None);
        let result = rule.check(&ctx).unwrap();

        // Should not flag tab content as indented code blocks
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_mkdocs_tabs_with_actual_indented_code() {
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Fenced);
        let content = r#"# Document

=== "Tab 1"

    This is tab content

Regular text

    This is an actual indented code block
    Should be flagged"#;

        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::MkDocs, None);
        let result = rule.check(&ctx).unwrap();

        // Should flag the actual indented code block but not the tab content
        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("Use fenced code blocks"));
    }

    #[test]
    fn test_mkdocs_tabs_detect_style() {
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Consistent);
        let content = r#"=== "Tab 1"

    Content in tab
    More content

=== "Tab 2"

    Content in second tab"#;

        // In MkDocs mode, tab content should not be detected as indented code blocks
        let style = detect_style_from_content(&rule, content, true);
        assert_eq!(style, None); // No code blocks detected

        // In standard mode, it would detect indented code blocks
        let style = detect_style_from_content(&rule, content, false);
        assert_eq!(style, Some(CodeBlockStyle::Indented));
    }

    #[test]
    fn test_mkdocs_nested_tabs() {
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Fenced);
        let content = r#"# Document

=== "Outer Tab"

    Some content

    === "Nested Tab"

        Nested tab content
        Should not be flagged"#;

        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::MkDocs, None);
        let result = rule.check(&ctx).unwrap();

        // Nested tabs should not be flagged
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_mkdocs_admonitions_not_flagged_as_indented_code() {
        // Issue #269: MkDocs admonitions have indented bodies that should NOT be
        // treated as indented code blocks when style = "fenced"
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Fenced);
        let content = r#"# Document

!!! note
    This is normal admonition content, not a code block.
    It spans multiple lines.

??? warning "Collapsible Warning"
    This is also admonition content.

???+ tip "Expanded Tip"
    And this one too.

Regular text outside admonitions."#;

        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::MkDocs, None);
        let result = rule.check(&ctx).unwrap();

        // Admonition content should not be flagged
        assert_eq!(
            result.len(),
            0,
            "Admonition content in MkDocs mode should not trigger MD046"
        );
    }

    #[test]
    fn test_mkdocs_admonition_with_actual_indented_code() {
        // After an admonition ends, regular indented code blocks SHOULD be flagged
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Fenced);
        let content = r#"# Document

!!! note
    This is admonition content.

Regular text ends the admonition.

    This is actual indented code (should be flagged)"#;

        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::MkDocs, None);
        let result = rule.check(&ctx).unwrap();

        // Should only flag the actual indented code block
        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("Use fenced code blocks"));
    }

    #[test]
    fn test_admonition_in_standard_mode_flagged() {
        // In standard Markdown mode, admonitions are not recognized, so the
        // indented content should be flagged as indented code
        // Note: A blank line is required before indented code blocks per CommonMark
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Fenced);
        let content = r#"# Document

!!! note

    This looks like code in standard mode.

Regular text."#;

        // In Standard mode, admonitions are not recognized
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // The indented content should be flagged in standard mode
        assert_eq!(
            result.len(),
            1,
            "Admonition content in Standard mode should be flagged as indented code"
        );
    }

    #[test]
    fn test_mkdocs_admonition_with_fenced_code_inside() {
        // Issue #269: Admonitions can contain fenced code blocks - must handle correctly
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Fenced);
        let content = r#"# Document

!!! note "Code Example"
    Here's some code:

    ```python
    def hello():
        print("world")
    ```

    More text after code.

Regular text."#;

        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::MkDocs, None);
        let result = rule.check(&ctx).unwrap();

        // Should not flag anything - the fenced block inside admonition is valid
        assert_eq!(result.len(), 0, "Fenced code blocks inside admonitions should be valid");
    }

    #[test]
    fn test_mkdocs_nested_admonitions() {
        // Nested admonitions are valid MkDocs syntax
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Fenced);
        let content = r#"# Document

!!! note "Outer"
    Outer content.

    !!! warning "Inner"
        Inner content.
        More inner content.

    Back to outer.

Regular text."#;

        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::MkDocs, None);
        let result = rule.check(&ctx).unwrap();

        // Nested admonitions should not trigger MD046
        assert_eq!(result.len(), 0, "Nested admonitions should not be flagged");
    }

    #[test]
    fn test_mkdocs_admonition_fix_does_not_wrap() {
        // The fix function should not wrap admonition content in fences
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Fenced);
        let content = r#"!!! note
    Content that should stay as admonition content.
    Not be wrapped in code fences.
"#;

        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::MkDocs, None);
        let fixed = rule.fix(&ctx).unwrap();

        // Fix should not add fence markers to admonition content
        assert!(
            !fixed.contains("```\n    Content"),
            "Admonition content should not be wrapped in fences"
        );
        assert_eq!(fixed, content, "Content should remain unchanged");
    }

    #[test]
    fn test_mkdocs_empty_admonition() {
        // Empty admonitions (marker only) should not cause issues
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Fenced);
        let content = r#"!!! note

Regular paragraph after empty admonition.

    This IS an indented code block (after blank + non-indented line)."#;

        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::MkDocs, None);
        let result = rule.check(&ctx).unwrap();

        // The indented code block after the paragraph should be flagged
        assert_eq!(result.len(), 1, "Indented code after admonition ends should be flagged");
    }

    #[test]
    fn test_mkdocs_indented_admonition() {
        // Admonitions can themselves be indented (e.g., inside list items)
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Fenced);
        let content = r#"- List item

    !!! note
        Indented admonition content.
        More content.

- Next item"#;

        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::MkDocs, None);
        let result = rule.check(&ctx).unwrap();

        // Admonition inside list should not be flagged
        assert_eq!(
            result.len(),
            0,
            "Indented admonitions (e.g., in lists) should not be flagged"
        );
    }

    #[test]
    fn test_footnote_indented_paragraphs_not_flagged() {
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Fenced);
        let content = r#"# Test Document with Footnotes

This is some text with a footnote[^1].

Here's some code:

```bash
echo "fenced code block"
```

More text with another footnote[^2].

[^1]: Really interesting footnote text.

    Even more interesting second paragraph.

[^2]: Another footnote.

    With a second paragraph too.

    And even a third paragraph!"#;

        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Indented paragraphs in footnotes should not be flagged as code blocks
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_footnote_definition_detection() {
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Fenced);

        // Valid footnote definitions (per CommonMark footnote extension spec)
        // Reference: https://github.com/jgm/commonmark-hs/blob/master/commonmark-extensions/test/footnotes.md
        assert!(rule.is_footnote_definition("[^1]: Footnote text"));
        assert!(rule.is_footnote_definition("[^foo]: Footnote text"));
        assert!(rule.is_footnote_definition("[^long-name]: Footnote text"));
        assert!(rule.is_footnote_definition("[^test_123]: Mixed chars"));
        assert!(rule.is_footnote_definition("    [^1]: Indented footnote"));
        assert!(rule.is_footnote_definition("[^a]: Minimal valid footnote"));
        assert!(rule.is_footnote_definition("[^123]: Numeric label"));
        assert!(rule.is_footnote_definition("[^_]: Single underscore"));
        assert!(rule.is_footnote_definition("[^-]: Single hyphen"));

        // Invalid: empty or whitespace-only labels (spec violation)
        assert!(!rule.is_footnote_definition("[^]: No label"));
        assert!(!rule.is_footnote_definition("[^ ]: Whitespace only"));
        assert!(!rule.is_footnote_definition("[^  ]: Multiple spaces"));
        assert!(!rule.is_footnote_definition("[^\t]: Tab only"));

        // Invalid: malformed syntax
        assert!(!rule.is_footnote_definition("[^]]: Extra bracket"));
        assert!(!rule.is_footnote_definition("Regular text [^1]:"));
        assert!(!rule.is_footnote_definition("[1]: Not a footnote"));
        assert!(!rule.is_footnote_definition("[^")); // Too short
        assert!(!rule.is_footnote_definition("[^1:")); // Missing closing bracket
        assert!(!rule.is_footnote_definition("^1]: Missing opening bracket"));

        // Invalid: disallowed characters in label
        assert!(!rule.is_footnote_definition("[^test.name]: Period"));
        assert!(!rule.is_footnote_definition("[^test name]: Space in label"));
        assert!(!rule.is_footnote_definition("[^test@name]: Special char"));
        assert!(!rule.is_footnote_definition("[^test/name]: Slash"));
        assert!(!rule.is_footnote_definition("[^test\\name]: Backslash"));

        // Edge case: line breaks not allowed in labels
        // (This is a string test, actual multiline would need different testing)
        assert!(!rule.is_footnote_definition("[^test\r]: Carriage return"));
    }

    #[test]
    fn test_footnote_with_blank_lines() {
        // Spec requirement: blank lines within footnotes don't terminate them
        // if next content is indented (matches GitHub's implementation)
        // Reference: commonmark-hs footnote extension behavior
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Fenced);
        let content = r#"# Document

Text with footnote[^1].

[^1]: First paragraph.

    Second paragraph after blank line.

    Third paragraph after another blank line.

Regular text at column 0 ends the footnote."#;

        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // The indented paragraphs in the footnote should not be flagged as code blocks
        assert_eq!(
            result.len(),
            0,
            "Indented content within footnotes should not trigger MD046"
        );
    }

    #[test]
    fn test_footnote_multiple_consecutive_blank_lines() {
        // Edge case: multiple consecutive blank lines within a footnote
        // Should still work if next content is indented
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Fenced);
        let content = r#"Text[^1].

[^1]: First paragraph.



    Content after three blank lines (still part of footnote).

Not indented, so footnote ends here."#;

        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // The indented content should not be flagged
        assert_eq!(
            result.len(),
            0,
            "Multiple blank lines shouldn't break footnote continuation"
        );
    }

    #[test]
    fn test_footnote_terminated_by_non_indented_content() {
        // Spec requirement: non-indented content always terminates the footnote
        // Reference: commonmark-hs footnote extension
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Fenced);
        let content = r#"[^1]: Footnote content.

    More indented content in footnote.

This paragraph is not indented, so footnote ends.

    This should be flagged as indented code block."#;

        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // The last indented block should be flagged (it's after the footnote ended)
        assert_eq!(
            result.len(),
            1,
            "Indented code after footnote termination should be flagged"
        );
        assert!(
            result[0].message.contains("Use fenced code blocks"),
            "Expected MD046 warning for indented code block"
        );
        assert!(result[0].line >= 7, "Warning should be on the indented code block line");
    }

    #[test]
    fn test_footnote_terminated_by_structural_elements() {
        // Spec requirement: headings and horizontal rules terminate footnotes
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Fenced);
        let content = r#"[^1]: Footnote content.

    More content.

## Heading terminates footnote

    This indented content should be flagged.

---

    This should also be flagged (after horizontal rule)."#;

        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Both indented blocks after structural elements should be flagged
        assert_eq!(
            result.len(),
            2,
            "Both indented blocks after termination should be flagged"
        );
    }

    #[test]
    fn test_footnote_with_code_block_inside() {
        // Spec behavior: footnotes can contain fenced code blocks
        // The fenced code must be properly indented within the footnote
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Fenced);
        let content = r#"Text[^1].

[^1]: Footnote with code:

    ```python
    def hello():
        print("world")
    ```

    More footnote text after code."#;

        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Should have no warnings - the fenced code block is valid
        assert_eq!(result.len(), 0, "Fenced code blocks within footnotes should be allowed");
    }

    #[test]
    fn test_footnote_with_8_space_indented_code() {
        // Edge case: code blocks within footnotes need 8 spaces (4 for footnote + 4 for code)
        // This should NOT be flagged as it's properly nested indented code
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Fenced);
        let content = r#"Text[^1].

[^1]: Footnote with nested code.

        code block
        more code"#;

        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // The 8-space indented code is valid within footnote
        assert_eq!(
            result.len(),
            0,
            "8-space indented code within footnotes represents nested code blocks"
        );
    }

    #[test]
    fn test_multiple_footnotes() {
        // Spec behavior: each footnote definition starts a new block context
        // Previous footnote ends when new footnote begins
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Fenced);
        let content = r#"Text[^1] and more[^2].

[^1]: First footnote.

    Continuation of first.

[^2]: Second footnote starts here, ending the first.

    Continuation of second."#;

        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // All indented content is part of footnotes
        assert_eq!(
            result.len(),
            0,
            "Multiple footnotes should each maintain their continuation context"
        );
    }

    #[test]
    fn test_list_item_ends_footnote_context() {
        // Spec behavior: list items and footnotes are mutually exclusive contexts
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Fenced);
        let content = r#"[^1]: Footnote.

    Content in footnote.

- List item starts here (ends footnote context).

    This indented content is part of the list, not the footnote."#;

        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // List continuation should not be flagged
        assert_eq!(
            result.len(),
            0,
            "List items should end footnote context and start their own"
        );
    }

    #[test]
    fn test_footnote_vs_actual_indented_code() {
        // Critical test: verify we can still detect actual indented code blocks outside footnotes
        // This ensures the fix doesn't cause false negatives
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Fenced);
        let content = r#"# Heading

Text with footnote[^1].

[^1]: Footnote content.

    Part of footnote (should not be flagged).

Regular paragraph ends footnote context.

    This is actual indented code (MUST be flagged)
    Should be detected as code block"#;

        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Should flag the indented code after the regular paragraph
        assert_eq!(
            result.len(),
            1,
            "Must still detect indented code blocks outside footnotes"
        );
        assert!(
            result[0].message.contains("Use fenced code blocks"),
            "Expected MD046 warning for indented code"
        );
        assert!(
            result[0].line >= 11,
            "Warning should be on the actual indented code line"
        );
    }

    #[test]
    fn test_spec_compliant_label_characters() {
        // Spec requirement: labels must contain only alphanumerics, hyphens, underscores
        // Reference: commonmark-hs footnote extension
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Fenced);

        // Valid according to spec
        assert!(rule.is_footnote_definition("[^test]: text"));
        assert!(rule.is_footnote_definition("[^TEST]: text"));
        assert!(rule.is_footnote_definition("[^test-name]: text"));
        assert!(rule.is_footnote_definition("[^test_name]: text"));
        assert!(rule.is_footnote_definition("[^test123]: text"));
        assert!(rule.is_footnote_definition("[^123]: text"));
        assert!(rule.is_footnote_definition("[^a1b2c3]: text"));

        // Invalid characters (spec violations)
        assert!(!rule.is_footnote_definition("[^test.name]: text")); // Period
        assert!(!rule.is_footnote_definition("[^test name]: text")); // Space
        assert!(!rule.is_footnote_definition("[^test@name]: text")); // At sign
        assert!(!rule.is_footnote_definition("[^test#name]: text")); // Hash
        assert!(!rule.is_footnote_definition("[^test$name]: text")); // Dollar
        assert!(!rule.is_footnote_definition("[^test%name]: text")); // Percent
    }

    #[test]
    fn test_code_block_inside_html_comment() {
        // Regression test: code blocks inside HTML comments should not be flagged
        // Found in denoland/deno test fixture during sanity testing
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Fenced);
        let content = r#"# Document

Some text.

<!--
Example code block in comment:

```typescript
console.log("Hello");
```

More comment text.
-->

More content."#;

        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(
            result.len(),
            0,
            "Code blocks inside HTML comments should not be flagged as unclosed"
        );
    }

    #[test]
    fn test_unclosed_fence_inside_html_comment() {
        // Even an unclosed fence inside an HTML comment should be ignored
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Fenced);
        let content = r#"# Document

<!--
Example with intentionally unclosed fence:

```
code without closing
-->

More content."#;

        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(
            result.len(),
            0,
            "Unclosed fences inside HTML comments should be ignored"
        );
    }

    #[test]
    fn test_multiline_html_comment_with_indented_code() {
        // Indented code inside HTML comments should also be ignored
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Fenced);
        let content = r#"# Document

<!--
Example:

    indented code
    more code

End of comment.
-->

Regular text."#;

        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(
            result.len(),
            0,
            "Indented code inside HTML comments should not be flagged"
        );
    }

    #[test]
    fn test_code_block_after_html_comment() {
        // Code blocks after HTML comments should still be detected
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Fenced);
        let content = r#"# Document

<!-- comment -->

Text before.

    indented code should be flagged

More text."#;

        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(
            result.len(),
            1,
            "Code blocks after HTML comments should still be detected"
        );
        assert!(result[0].message.contains("Use fenced code blocks"));
    }

    #[test]
    fn test_consistent_style_indented_html_comment() {
        // Under the default `Consistent` style, indented content inside an
        // HTML comment must not contribute to the document's code-block style
        // tally. Otherwise a single fenced block alongside an indented HTML
        // comment flips the detected style to `Indented`, emitting a spurious
        // "Use indented code blocks" warning against the only real code block.
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Consistent);
        let content = "# MD046 false-positive reproduction\n\
                       \n\
                       <!--\n    \
                       This is just an indented comment, not a code block.\n\
                       \n    \
                       A second line is required to trigger the false-positive.\n\
                       \n    \
                       Actually, three lines are required.\n\
                       -->\n\
                       \n\
                       ```md\n\
                       This should be fine, since it's the only code block and therefore consistent.\n\
                       ```\n";

        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(
            result,
            vec![],
            "A single fenced block and an indented HTML comment must produce no MD046 warnings",
        );
    }

    #[test]
    fn test_consistent_style_indented_html_block() {
        // Indented content inside a raw HTML block (e.g. a `<div>` tag pair)
        // must not count as an indented code block when `detect_style` picks
        // the document's predominant style.
        //
        // Per CommonMark, a type-6 HTML block is terminated by a blank line,
        // so the content here is kept contiguous to remain inside the block.
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Consistent);
        let content = "# Heading\n\
                       \n\
                       <div class=\"note\">\n    \
                       line one of indented html content\n    \
                       line two of indented html content\n    \
                       line three of indented html content\n\
                       </div>\n\
                       \n\
                       ```md\n\
                       real fenced block\n\
                       ```\n";

        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(
            result,
            vec![],
            "Indented content inside a raw HTML block must not influence MD046 style detection",
        );
    }

    #[test]
    fn test_consistent_style_fake_fence_inside_html_comment() {
        // Fence markers inside an HTML comment must not contribute to the
        // fenced count during style detection. Otherwise a document whose
        // only real code block is indented gets flagged "Use fenced code
        // blocks" under `Consistent` because the verbatim ``` inside the
        // comment ties the count.
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Consistent);
        let content = "# Title\n\
                       \n\
                       <!--\n\
                       ```\n\
                       fake fence inside comment\n\
                       ```\n\
                       -->\n\
                       \n    \
                       real indented code block line 1\n    \
                       real indented code block line 2\n";

        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(
            result,
            vec![],
            "Fence markers inside an HTML comment must not influence MD046 style detection",
        );
    }

    #[test]
    fn test_consistent_style_indented_footnote_definition() {
        // Footnote-definition continuation lines are commonly indented by 4+
        // spaces. They must not be counted as indented code blocks during
        // style detection under `Consistent`.
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Consistent);
        let content = "# Heading\n\
                       \n\
                       Reference to a footnote[^note].\n\
                       \n\
                       [^note]: First line of the footnote.\n    \
                       Second indented continuation line.\n    \
                       Third indented continuation line.\n    \
                       Fourth indented continuation line.\n\
                       \n\
                       ```md\n\
                       real fenced block\n\
                       ```\n";

        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(
            result,
            vec![],
            "Footnote-definition continuation content must not influence MD046 style detection",
        );
    }

    #[test]
    fn test_consistent_style_indented_blockquote() {
        // Indented content inside a blockquote (`>     foo`) must not be
        // counted as an indented code block by `detect_style`. The check-side
        // skip list already excludes `blockquote.is_some()` for indented
        // warnings, so detection must match to keep `Consistent` stable.
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Consistent);
        let content = "# Heading\n\
                       \n\
                       >     line one of quoted indented content\n\
                       >\n\
                       >     line two of quoted indented content\n\
                       >\n\
                       >     line three of quoted indented content\n\
                       \n\
                       ```md\n\
                       real fenced block\n\
                       ```\n";

        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(
            result,
            vec![],
            "Indented content inside a blockquote must not influence MD046 style detection",
        );
    }

    #[test]
    fn test_consistent_style_genuine_indented_block_detected_as_indented() {
        // A top-level indented code block that is not inside any container
        // must still count toward the Indented tally under `Consistent` style.
        // This guards against over-filtering: the `in_comment_or_html` skip
        // must not suppress real indented code blocks.
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Consistent);
        let content = "# Heading\n\
                       \n\
                       Some prose.\n\
                       \n    \
                       real indented code line 1\n    \
                       real indented code line 2\n";

        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Only one indented block exists; Consistent must detect it as Indented and
        // produce no warnings (the detected style matches the only real block).
        assert_eq!(
            result,
            vec![],
            "A genuine top-level indented block must be detected as Indented style under Consistent",
        );
    }

    #[test]
    fn test_consistent_style_skipped_lines_dont_override_real_block() {
        // Two indented-but-skipped regions (inside HTML comments) plus one
        // genuine indented code block and no fenced blocks: the skipped lines
        // must be excluded from the tally, leaving indented_count=1, fenced_count=0,
        // so Consistent still selects Indented and emits no warnings.
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Consistent);
        let content = "# Heading\n\
                       \n\
                       <!--\n    \
                       skipped indented comment line 1\n    \
                       skipped indented comment line 2\n\
                       -->\n\
                       \n\
                       <!--\n    \
                       second skipped region\n    \
                       also skipped\n\
                       -->\n\
                       \n    \
                       real indented code line\n";

        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(
            result,
            vec![],
            "Skipped container lines must not outweigh the single real indented block",
        );
    }

    #[test]
    fn test_consistent_style_fenced_wins_over_skipped_indented() {
        // One real fenced block plus two indented-but-skipped regions: after
        // filtering the skipped lines the tally is fenced=1, indented=0, so
        // Consistent selects Fenced and emits no warnings.
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Consistent);
        let content = "# Heading\n\
                       \n\
                       <!--\n    \
                       skipped indented region one\n    \
                       more of region one\n\
                       -->\n\
                       \n\
                       <!--\n    \
                       skipped indented region two\n    \
                       more of region two\n\
                       -->\n\
                       \n\
                       ```md\n\
                       real fenced block\n\
                       ```\n";

        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(
            result,
            vec![],
            "Fenced block must win when all indented lines are inside skipped containers",
        );
    }

    #[test]
    fn test_four_space_indented_fence_is_not_valid_fence() {
        // Per CommonMark 0.31.2: "An opening code fence may be indented 0-3 spaces."
        // 4+ spaces means it's NOT a valid fence opener - it becomes an indented code block
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Fenced);

        // Valid fences (0-3 spaces)
        assert!(rule.is_fenced_code_block_start("```"));
        assert!(rule.is_fenced_code_block_start(" ```"));
        assert!(rule.is_fenced_code_block_start("  ```"));
        assert!(rule.is_fenced_code_block_start("   ```"));

        // Invalid fences (4+ spaces) - these are indented code blocks instead
        assert!(!rule.is_fenced_code_block_start("    ```"));
        assert!(!rule.is_fenced_code_block_start("     ```"));
        assert!(!rule.is_fenced_code_block_start("        ```"));

        // Tab counts as 4 spaces per CommonMark
        assert!(!rule.is_fenced_code_block_start("\t```"));
    }

    #[test]
    fn test_issue_237_indented_fenced_block_detected_as_indented() {
        // Issue #237: User has fenced code block indented by 4 spaces
        // Per CommonMark, this should be detected as an INDENTED code block
        // because 4+ spaces of indentation makes the fence invalid
        //
        // Reference: https://github.com/rvben/rumdl/issues/237
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Fenced);

        // This is the exact test case from issue #237
        let content = r#"## Test

    ```js
    var foo = "hello";
    ```
"#;

        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Should flag this as an indented code block that should use fenced style
        assert_eq!(
            result.len(),
            1,
            "4-space indented fence should be detected as indented code block"
        );
        assert!(
            result[0].message.contains("Use fenced code blocks"),
            "Expected 'Use fenced code blocks' message"
        );
    }

    #[test]
    fn test_issue_276_indented_code_in_list() {
        // Issue #276: Indented code blocks inside lists should be detected
        // Reference: https://github.com/rvben/rumdl/issues/276
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Fenced);

        let content = r#"1. First item
2. Second item with code:

        # This is a code block in a list
        print("Hello, world!")

4. Third item"#;

        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Should flag the indented code block inside the list
        assert!(
            !result.is_empty(),
            "Indented code block inside list should be flagged when style=fenced"
        );
        assert!(
            result[0].message.contains("Use fenced code blocks"),
            "Expected 'Use fenced code blocks' message"
        );
    }

    #[test]
    fn test_three_space_indented_fence_is_valid() {
        // 3 spaces is the maximum allowed per CommonMark - should be recognized as fenced
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Fenced);

        let content = r#"## Test

   ```js
   var foo = "hello";
   ```
"#;

        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // 3-space indent is valid for fenced blocks - should pass
        assert_eq!(
            result.len(),
            0,
            "3-space indented fence should be recognized as valid fenced code block"
        );
    }

    #[test]
    fn test_indented_style_with_deeply_indented_fenced() {
        // When style=indented, a 4-space indented "fenced" block should still be detected
        // as an indented code block (which is what we want!)
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Indented);

        let content = r#"Text

    ```js
    var foo = "hello";
    ```

More text
"#;

        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // When target style is "indented", 4-space indented content is correct
        // The fence markers become literal content in the indented code block
        assert_eq!(
            result.len(),
            0,
            "4-space indented content should be valid when style=indented"
        );
    }

    #[test]
    fn test_fix_misplaced_fenced_block() {
        // Issue #237: When a fenced code block is accidentally indented 4+ spaces,
        // the fix should just remove the indentation, not wrap in more fences
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Fenced);

        let content = r#"## Test

    ```js
    var foo = "hello";
    ```
"#;

        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        // The fix should just remove the 4-space indentation
        let expected = r#"## Test

```js
var foo = "hello";
```
"#;

        assert_eq!(fixed, expected, "Fix should remove indentation, not add more fences");
    }

    #[test]
    fn test_fix_regular_indented_block() {
        // Regular indented code blocks (without fence markers) should still be
        // wrapped in fences when converted
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Fenced);

        let content = r#"Text

    var foo = "hello";
    console.log(foo);

More text
"#;

        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        // Should wrap in fences
        assert!(fixed.contains("```\nvar foo"), "Should add opening fence");
        assert!(fixed.contains("console.log(foo);\n```"), "Should add closing fence");
    }

    #[test]
    fn test_fix_indented_block_with_fence_like_content() {
        // If an indented block contains fence-like content but doesn't form a
        // complete fenced block, we should NOT autofix it because wrapping would
        // create invalid nested fences. The block is left unchanged.
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Fenced);

        let content = r#"Text

    some code
    ```not a fence opener
    more code
"#;

        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        // Block should be left unchanged to avoid creating invalid nested fences
        assert!(fixed.contains("    some code"), "Unsafe block should be left unchanged");
        assert!(!fixed.contains("```\nsome code"), "Should NOT wrap unsafe block");
    }

    #[test]
    fn test_fix_mixed_indented_and_misplaced_blocks() {
        // Mixed blocks: regular indented code followed by misplaced fenced block
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Fenced);

        let content = r#"Text

    regular indented code

More text

    ```python
    print("hello")
    ```
"#;

        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        // First block should be wrapped
        assert!(
            fixed.contains("```\nregular indented code\n```"),
            "First block should be wrapped in fences"
        );

        // Second block should be dedented (not wrapped)
        assert!(
            fixed.contains("\n```python\nprint(\"hello\")\n```"),
            "Second block should be dedented, not double-wrapped"
        );
        // Should NOT have nested fences
        assert!(
            !fixed.contains("```\n```python"),
            "Should not have nested fence openers"
        );
    }

    #[test]
    fn test_md046_front_matter() {
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Fenced);
        let content = "---\nmetadata:\n\n    description: Indented\n---\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_md046_fix_front_matter() {
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Fenced);
        let content = "---\nmetadata:\n\n    description: Indented\n---\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, content);
    }

    #[test]
    fn test_whitespace_only_line_is_not_an_indented_code_block() {
        // A line holding four spaces and nothing else is a blank line to
        // CommonMark. The fix used to wrap it in a fence of its own, so a
        // document with one real indented block elsewhere gained an empty
        // fenced block where a blank line stood.
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Fenced);
        let content = "# T\n\nPara\n\n    \nMore\n\n    real code\n\nEnd\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, "# T\n\nPara\n\n    \nMore\n\n```\nreal code\n```\n\nEnd\n");
    }

    #[test]
    fn test_interior_blank_line_keeps_indented_block_together() {
        // CommonMark keeps a blank line between two indented code lines inside
        // the block, so `a`, the blank and `b` are one block and convert to one
        // fence with an empty line in it, not two fences.
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Fenced);
        let content = "# T\n\nPara\n\n    a\n\n    b\n\nAfter\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, "# T\n\nPara\n\n```\na\n\nb\n```\n\nAfter\n");
    }

    #[test]
    fn test_consistent_style_counts_a_block_with_interior_blank_once() {
        // Style detection counts blocks. Splitting `a` / blank / `b` in two made
        // one indented block outvote one fenced block, and the fenced block was
        // reported instead of the indented one.
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Consistent);
        let content = "# T\n\n```\nfenced\n```\n\nPara\n\n    a\n\n    b\n\nEnd\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        let reported: Vec<(usize, &str)> = result.iter().map(|w| (w.line, w.message.as_str())).collect();
        assert_eq!(reported, vec![(9, "Use fenced code blocks")]);
    }

    #[test]
    fn test_indented_lazy_continuation_lines_are_not_code() {
        // Indented lines directly under a paragraph line continue that
        // paragraph, and so does every indented line after them. Classifying
        // the second line by the raw indent of the first turned the run into
        // code from its second line on, and the fix fenced the tail of a
        // paragraph.
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Fenced);
        let content = "# T\n\nPara\n    lazy one\n    lazy two\n    lazy three\n\n    real code\n\nEnd\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(
            fixed,
            "# T\n\nPara\n    lazy one\n    lazy two\n    lazy three\n\n```\nreal code\n```\n\nEnd\n"
        );
    }

    #[test]
    fn test_misplaced_fence_with_interior_blank_dedents_as_one_block() {
        // An over-indented fenced block whose body has a blank line is still
        // one complete fenced block, so it is dedented as a whole. Split at the
        // blank, neither half had both fences and the block was left alone.
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Fenced);
        let content = "# T\n\nPara\n\n    ```python\n    x = 1\n\n    y = 2\n    ```\n\nAfter\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, "# T\n\nPara\n\n```python\nx = 1\n\ny = 2\n```\n\nAfter\n");
    }
    #[test]
    fn test_mdg_overrides_indented_style_to_fenced() {
        // A Gherkin Doc String is only ever a backtick fence, so a
        // configuration demanding indented code cannot be satisfied in this
        // flavor. MDG does not adopt it: the Doc String keeps its fence instead
        // of being unwrapped into an indented block that deletes it.
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Indented);
        let content = "# Feature: Payloads\n\n## Scenario: JSON payload\n\n* Given this payload\n\n  ```json\n  {\"ok\": true}\n  ```\n";

        let mdg_ctx = LintContext::new(content, crate::config::MarkdownFlavor::MDG, None);
        assert!(rule.check(&mdg_ctx).unwrap().is_empty());
        assert_eq!(rule.fix(&mdg_ctx).unwrap(), content);

        // Standard still reports the configured style mismatch, but cannot
        // apply it without discarding the JSON info string.
        let standard_ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let standard_warnings = rule.check(&standard_ctx).unwrap();
        assert_eq!(standard_warnings.len(), 1);
        assert_eq!(standard_warnings[0].message, "Use indented code blocks");
        assert_eq!(rule.fix(&standard_ctx).unwrap(), content);
    }

    #[test]
    fn test_mdg_indented_style_still_fences_indented_blocks() {
        // The override is not merely a refusal to unwrap fences: MDG enforces
        // fenced, so an indented block is converted even though the
        // configuration asked for indented code.
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Indented);
        let content =
            "# Feature: Payloads\n\n## Scenario: Plain payload\n\nSome description.\n\n      ordinary indented code\n";

        let mdg_ctx = LintContext::new(content, crate::config::MarkdownFlavor::MDG, None);
        let warnings = rule.check(&mdg_ctx).unwrap();
        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].message, "Use fenced code blocks");

        let fixed = rule.fix(&mdg_ctx).unwrap();
        assert_eq!(
            fixed,
            "# Feature: Payloads\n\n## Scenario: Plain payload\n\nSome description.\n\n```\n  ordinary indented code\n```\n"
        );

        let fixed_ctx = LintContext::new(&fixed, crate::config::MarkdownFlavor::MDG, None);
        assert!(rule.check(&fixed_ctx).unwrap().is_empty());
        assert_eq!(rule.fix(&fixed_ctx).unwrap(), fixed, "MDG fix should be idempotent");

        // Standard honours `indented`: the block is already indented, so there
        // is nothing to report and nothing to change.
        let standard_ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        assert!(rule.check(&standard_ctx).unwrap().is_empty());
        assert_eq!(rule.fix(&standard_ctx).unwrap(), content);
    }

    #[test]
    fn test_mdg_steers_indented_code_to_fenced() {
        // Under MDG a code block is expected to be a backtick fence, so an
        // indented block is corrected rather than preserved — whichever style
        // the configuration names.
        let content = "# Feature: Payloads\n\n## Scenario: Plain payload\n\n* Given this payload\n\n      ordinary indented code\n";

        for rule in [
            MD046CodeBlockStyle::new(CodeBlockStyle::Fenced),
            MD046CodeBlockStyle::new(CodeBlockStyle::Consistent),
            MD046CodeBlockStyle::new(CodeBlockStyle::Indented),
        ] {
            let ctx = LintContext::new(content, crate::config::MarkdownFlavor::MDG, None);
            let warnings = rule.check(&ctx).unwrap();
            assert_eq!(warnings.len(), 1);
            assert_eq!(warnings[0].message, "Use fenced code blocks");

            let fixed = rule.fix(&ctx).unwrap();
            assert!(fixed.contains("```"), "MDG must fence the block: {fixed:?}");

            let fixed_ctx = LintContext::new(&fixed, crate::config::MarkdownFlavor::MDG, None);
            assert!(rule.check(&fixed_ctx).unwrap().is_empty());
            assert_eq!(rule.fix(&fixed_ctx).unwrap(), fixed, "MDG fix should be idempotent");
        }
    }

    #[test]
    fn test_mdg_consistent_style_ignores_indented_prevalence() {
        // Standard resolves `consistent` by prevalence; MDG always resolves it
        // to fenced because only a backtick fence can be a Doc String.
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Consistent);
        let indented_majority = "# Feature: Payloads\n\n## Scenario: Mixed payloads\n\n* Given this payload\n\n```\n{\"ok\": true}\n```\n\nFirst ordinary example:\n\n    one\n\nSecond ordinary example:\n\n    two\n";

        let standard_ctx = LintContext::new(indented_majority, crate::config::MarkdownFlavor::Standard, None);
        let standard_warnings = rule.check(&standard_ctx).unwrap();
        assert_eq!(standard_warnings.len(), 1);
        assert_eq!(standard_warnings[0].message, "Use indented code blocks");

        let mdg_ctx = LintContext::new(indented_majority, crate::config::MarkdownFlavor::MDG, None);
        let mdg_warnings = rule.check(&mdg_ctx).unwrap();
        assert_eq!(mdg_warnings.len(), 2);
        assert!(
            mdg_warnings
                .iter()
                .all(|warning| warning.message == "Use fenced code blocks")
        );
    }

    #[test]
    fn test_mdg_repairs_unclosed_fence_like_standard() {
        // The unclosed-fence repair is flavor independent now that MDG no
        // longer takes a bespoke fix path.
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Fenced);
        let content = "# Feature: Payloads\n\n```json\n{\"ok\": true}\n";

        let mdg_ctx = LintContext::new(content, crate::config::MarkdownFlavor::MDG, None);
        let warnings = rule.check(&mdg_ctx).unwrap();
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].message.contains("never closed"));

        let standard_ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        assert_eq!(
            rule.fix(&mdg_ctx).unwrap(),
            rule.fix(&standard_ctx).unwrap(),
            "MDG must not differ from Standard"
        );
    }

    #[test]
    fn test_mdg_table_above_prose_is_never_fenced() {
        // The Examples table and the paragraph below it sit in one CommonMark
        // indented code block, split by a blank line. `check` and `fix` read
        // the same per-line membership, so the table stays a table and only the
        // paragraph is fenced — reporting the block and fencing all of it (or
        // skipping the block and fencing it anyway) would delete the table.
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Fenced);
        let content = "# Feature: Eating\n\n#### Examples:\n\n    | start | eat | left |\n    | ----- | --- | ---- |\n\n    a note about the data\n\n## Scenario: Other\n\n      unrelated indented code\n";

        let mdg_ctx = LintContext::new(content, crate::config::MarkdownFlavor::MDG, None);
        let reported: Vec<usize> = rule.check(&mdg_ctx).unwrap().iter().map(|w| w.line).collect();
        assert_eq!(reported, vec![8, 12]);

        let fixed = rule.fix(&mdg_ctx).unwrap();
        assert_eq!(
            fixed,
            "# Feature: Eating\n\n#### Examples:\n\n    | start | eat | left |\n    | ----- | --- | ---- |\n\n```\na note about the data\n```\n\n## Scenario: Other\n\n```\n  unrelated indented code\n```\n"
        );

        let fixed_ctx = LintContext::new(&fixed, crate::config::MarkdownFlavor::MDG, None);
        assert!(
            rule.check(&fixed_ctx).unwrap().is_empty(),
            "MDG check must have nothing left to report after its own fix"
        );
        assert_eq!(rule.fix(&fixed_ctx).unwrap(), fixed, "MDG fix should be idempotent");

        // Standard has no Gherkin tables, so the whole block is code there.
        let standard_ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let standard_reported: Vec<usize> = rule.check(&standard_ctx).unwrap().iter().map(|w| w.line).collect();
        assert_eq!(standard_reported, vec![5, 12]);
        assert!(rule.fix(&standard_ctx).unwrap().contains("```\n| start | eat | left |"));
    }

    #[test]
    fn test_mdg_repairs_unclosed_fence_under_indented_style() {
        // MDG does not adopt the configured `indented` style, but closing an
        // unclosed fence is a repair rather than a conversion: `check` reports
        // it before any style is resolved, so `fix` has to resolve it too.
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Indented);
        let content = "# Feature: Payloads\n\n```json\n{\"ok\": true}\n";

        let mdg_ctx = LintContext::new(content, crate::config::MarkdownFlavor::MDG, None);
        let warnings = rule.check(&mdg_ctx).unwrap();
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].message.contains("never closed"));

        let fixed = rule.fix(&mdg_ctx).unwrap();
        assert_eq!(fixed, "# Feature: Payloads\n\n```json\n{\"ok\": true}\n```\n");

        let fixed_ctx = LintContext::new(&fixed, crate::config::MarkdownFlavor::MDG, None);
        assert!(rule.check(&fixed_ctx).unwrap().is_empty());

        // Standard also preserves the tagged fence because conversion would
        // discard its info string, while still repairing the missing closer.
        let standard_ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        assert_eq!(rule.fix(&standard_ctx).unwrap(), fixed);
    }

    #[test]
    fn test_mdg_tab_indented_table_is_not_code() {
        // Gherkin matches table rows on `\s`, so two tabs — or a space and a
        // tab — indent a table just as two spaces do, even though both expand
        // past the 4-column indented-code threshold.
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Fenced);
        for indent in ["\t\t", " \t"] {
            let content = format!(
                "# Feature: Eating\n\n#### Examples:\n\n{indent}| start | eat |\n{indent}| ----- | --- |\n\n## Scenario: Other\n\n      code here\n"
            );

            let mdg_ctx = LintContext::new(&content, crate::config::MarkdownFlavor::MDG, None);
            let reported: Vec<usize> = rule.check(&mdg_ctx).unwrap().iter().map(|w| w.line).collect();
            assert_eq!(reported, vec![10], "tab-indented rows are a table, not code");

            let fixed = rule.fix(&mdg_ctx).unwrap();
            assert!(
                fixed.contains(&format!("{indent}| start | eat |\n{indent}| ----- | --- |")),
                "MDG must leave the tab-indented table alone: {fixed:?}"
            );

            let standard_ctx = LintContext::new(&content, crate::config::MarkdownFlavor::Standard, None);
            let standard_reported: Vec<usize> = rule.check(&standard_ctx).unwrap().iter().map(|w| w.line).collect();
            assert_eq!(standard_reported, vec![5, 10]);
        }
    }

    #[test]
    fn test_from_config_records_whether_style_was_configured() {
        // The MDG override applies either way, but the warning is only for a
        // style the user actually asked for, so a configured style has to be
        // told apart from a defaulted one.
        use crate::config::Config;
        use std::collections::BTreeMap;

        let mut values = BTreeMap::new();
        values.insert("style".to_string(), toml::Value::String("indented".to_string()));
        let mut config = Config::default();
        config.rules.insert(
            "MD046".to_string(),
            crate::config::RuleConfig { severity: None, values },
        );

        let configured = MD046CodeBlockStyle::from_config(&config);
        let configured = configured.as_any().downcast_ref::<MD046CodeBlockStyle>().unwrap();
        assert_eq!(configured.config.style, CodeBlockStyle::Indented);
        assert!(configured.style_explicit);

        let defaulted = MD046CodeBlockStyle::from_config(&Config::default());
        let defaulted = defaulted.as_any().downcast_ref::<MD046CodeBlockStyle>().unwrap();
        assert!(!defaulted.style_explicit);

        // The override does not depend on the warning: a defaulted `indented`
        // is enforced as fenced just the same.
        let indented = MD046CodeBlockStyle::from_config_struct(MD046Config {
            style: CodeBlockStyle::Indented,
        });
        let content = "# Feature: F\n\nText.\n\n      code here\n";
        let mdg_ctx = LintContext::new(content, crate::config::MarkdownFlavor::MDG, None);
        assert!(indented.fix(&mdg_ctx).unwrap().contains("```\n  code here\n```"));
    }

    #[test]
    fn test_mdg_indented_style_keeps_tables_out_of_code() {
        // Enforcing fenced does not widen what MDG counts as code: a
        // Data/Examples table is still not an indented code block.
        let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Indented);
        let content = "# Feature: Eating\n\n#### Examples:\n\n    | start | eat | left |\n    | ----- | --- | ---- |\n";

        let mdg_ctx = LintContext::new(content, crate::config::MarkdownFlavor::MDG, None);
        assert!(rule.check(&mdg_ctx).unwrap().is_empty());
        assert_eq!(rule.fix(&mdg_ctx).unwrap(), content);

        // Standard has no Gherkin tables, so the rows are code — and `indented`
        // is honoured there, so they are already in the requested form.
        let standard_ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        assert!(rule.check(&standard_ctx).unwrap().is_empty());
        assert_eq!(rule.fix(&standard_ctx).unwrap(), content);
    }
}
