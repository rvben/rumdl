# MD013 List Handling Refactor Plan: Proper HTML Support

## Current Architecture Overview

### How MD013 Currently Processes List Items

The MD013 rule processes multi-line list items through several stages:

1. **Line Collection** (lines 480-597): Collects all lines belonging to a list item
   - Handles nested lists, code blocks, and continuation lines
   - Tracks indentation levels to determine list item boundaries

2. **Block Classification** (lines 603-731): Categorizes content into block types
   - `Block::Paragraph(Vec<String>)`: Regular text content
   - `Block::Code { lines, has_preceding_blank }`: Code blocks (fenced or indented)
   - `Block::NestedList(Vec<(String, usize)>)`: Nested list items
   - `Block::SemanticLine(String)`: Special markers like NOTE:, WARNING:

3. **Segment Processing** (lines 840-900): Handles paragraphs with hard breaks
   - Splits paragraphs at hard break boundaries (`\\` or two trailing spaces)
   - Each segment is reflowed independently
   - Hard breaks are preserved after reflow

4. **The Problem: Line 875** - THE ROOT CAUSE

   ```rust
   let segment_text = segment_for_reflow.join(" ").trim().to_string();
   ```

   This joins ALL lines in a segment with a single space. For HTML content:

   ```markdown
   <details>
       <summary>Title</summary>
       Content here.
   </details>
   ```

   Becomes: `<details> <summary>Title</summary> Content here. </details>`

   This destroys:
   - HTML tag structure and indentation
   - Block-level tag semantics
   - Proper nesting hierarchy

## Current Workaround (Commit 34ae286)

**Location**: src/rules/md013_line_length.rs:806-812

```rust
// Check if list item contains HTML tags
let has_html_tags = (list_start..i).any(|line_idx| {
    lines[line_idx].contains('<') && lines[line_idx].contains('>')
});

// Skip auto-fix for list items with HTML - joining with spaces breaks structure
if has_html_tags {
    continue;
}
```

**Limitations**:

- Crude detection: Any `<` and `>` characters trigger skip
- False positives: `Generic<T>` or `x < 5` would skip auto-fix
- No auto-fix at all for HTML-containing items
- Doesn't solve the underlying architectural problem

## Proper Refactor Plan

### Goal: Preserve HTML Structure During Reflow

The refactor needs to:

1. Detect HTML blocks within list items
2. Preserve HTML structure (tags, indentation, hierarchy)
3. Only reflow text content between/around HTML blocks
4. Handle mixed content (text + HTML + text)

### Architecture Changes

#### 1. Add HTML Block Detection to Block Classification

**New Block Type**:

```rust
enum Block {
    Paragraph(Vec<String>),
    Code { lines: Vec<(String, usize)>, has_preceding_blank: bool },
    NestedList(Vec<(String, usize)>),
    SemanticLine(String),
    HtmlBlock(Vec<String>), // NEW: Preserve HTML structure
}
```

**HTML Block Detection Logic** (add to lines 624-718):

```rust
fn is_html_opening_tag(line: &str) -> Option<String> {
    // Match opening tags like <details>, <div class="foo">, etc.
    // Return the tag name (details, div, etc.)
    let trimmed = line.trim();
    if trimmed.starts_with('<') && !trimmed.starts_with("</") {
        // Extract tag name from <tagname ...>
        if let Some(end) = trimmed[1..].find(|c: char| c.is_whitespace() || c == '>') {
            return Some(trimmed[1..1+end].to_lowercase());
        }
    }
    None
}

fn is_html_closing_tag(line: &str, tag_name: &str) -> bool {
    let trimmed = line.trim();
    trimmed.starts_with(&format!("</{}>", tag_name)) ||
    trimmed.starts_with(&format!("</{} ", tag_name))
}

fn is_self_closing_tag(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed.ends_with("/>") ||
    matches!(
        trimmed,
        s if s.starts_with("<br") ||
             s.starts_with("<hr") ||
             s.starts_with("<img") ||
             s.starts_with("<input")
    )
}
```

**Integration into Block Classification**:

```rust
let mut in_html = false;
let mut html_tag_stack: Vec<String> = Vec::new();
let mut current_html_block: Vec<String> = Vec::new();

for line in &list_item_lines {
    match line {
        LineType::Content(content) => {
            // Check if this starts an HTML block
            if let Some(tag_name) = is_html_opening_tag(content) {
                if !in_html {
                    // Flush current paragraph if any
                    if !current_paragraph.is_empty() {
                        blocks.push(Block::Paragraph(current_paragraph.clone()));
                        current_paragraph.clear();
                    }
                    in_html = true;
                }

                if !is_self_closing_tag(content) {
                    html_tag_stack.push(tag_name);
                }
                current_html_block.push(content.clone());
            }
            // Check if this closes an HTML block
            else if in_html {
                current_html_block.push(content.clone());

                // Check for closing tags
                if let Some(last_tag) = html_tag_stack.last() {
                    if is_html_closing_tag(content, last_tag) {
                        html_tag_stack.pop();

                        // If stack is empty, HTML block is complete
                        if html_tag_stack.is_empty() {
                            blocks.push(Block::HtmlBlock(current_html_block.clone()));
                            current_html_block.clear();
                            in_html = false;
                        }
                    }
                }
            }
            // Regular paragraph content
            else {
                current_paragraph.push(content.clone());
            }
        }
        // ... handle other line types
    }
}
```

#### 2. Modify Reflow Logic to Handle HTML Blocks

**Location**: After line 840 in the block iteration loop

```rust
for (block_idx, block) in blocks.iter().enumerate() {
    match block {
        Block::Paragraph(para_lines) => {
            // Current paragraph reflow logic (unchanged)
            // Lines 842-900
        }
        Block::Code { lines, has_preceding_blank } => {
            // Current code block preservation logic (unchanged)
            // Lines 901-932
        }
        Block::NestedList(nested_items) => {
            // Current nested list preservation logic (unchanged)
            // Lines 933-948
        }
        Block::SemanticLine(content) => {
            // Current semantic line preservation logic (unchanged)
            // Lines 949-960
        }
        Block::HtmlBlock(html_lines) => {
            // NEW: Preserve HTML structure exactly as-is
            for (idx, line) in html_lines.iter().enumerate() {
                if is_first_block && idx == 0 {
                    result.push(format!("{marker}{line}"));
                    is_first_block = false;
                } else {
                    result.push(format!("{expected_indent}{line}"));
                }
            }
        }
    }
}
```

#### 3. Handle Mixed Content (Text + HTML + Text)

The block classification naturally handles this:

```markdown
- Text paragraph one.
  Text paragraph two.
  <details>
      <summary>Title</summary>
      Content here.
  </details>
  Text paragraph three.
  Text paragraph four.
```

Becomes:

1. `Block::Paragraph(["Text paragraph one.", "Text paragraph two."])`
2. `Block::HtmlBlock(["<details>", "    <summary>Title</summary>", "    Content here.", "</details>"])`
3. `Block::Paragraph(["Text paragraph three.", "Text paragraph four."])`

Each paragraph gets reflowed independently, HTML block is preserved as-is.

### Benefits of This Approach

1. **Accurate HTML Detection**: Uses proper tag parsing instead of crude `contains('<')`
2. **Structure Preservation**: HTML blocks maintain original formatting
3. **Selective Reflow**: Only text content is reflowed, HTML is untouched
4. **Mixed Content Support**: Handles text before/after/between HTML naturally
5. **No False Positives**: `Generic<T>` stays in paragraph, gets reflowed normally
6. **Nested Tags**: Tag stack handles nested HTML like `<div><span>...</span></div>`

### Implementation Checklist

- [ ] Add `Block::HtmlBlock(Vec<String>)` variant
- [ ] Implement `is_html_opening_tag()` function
- [ ] Implement `is_html_closing_tag()` function
- [ ] Implement `is_self_closing_tag()` function
- [ ] Add HTML block detection to line classification loop (lines 624-718)
- [ ] Handle tag stack for nested HTML
- [ ] Add HTML block preservation to reflow logic (after line 840)
- [ ] Remove workaround from lines 806-812
- [ ] Add comprehensive tests:
  - [ ] Simple HTML block in list item
  - [ ] Nested HTML tags (`<div><span>...</span></div>`)
  - [ ] Mixed content (text + HTML + text)
  - [ ] Self-closing tags (`<br/>`, `<img src="...">`)
  - [ ] HTML with attributes (`<div class="foo" id="bar">`)
  - [ ] Inline HTML (shouldn't be treated as block)
  - [ ] False positives: `Generic<T>`, `x < 5 && y > 3`
- [ ] Update documentation

### Edge Cases to Consider

1. **Inline HTML**: `This is <em>emphasized</em> text.`
   - Should NOT be treated as HTML block
   - Stays in paragraph, gets reflowed normally
   - Detection: Tag and closing tag on same line

2. **HTML Comments**: `<!-- This is a comment -->`
   - Should be preserved as HTML block
   - Special case in tag detection

3. **Malformed HTML**: `<div>Text with no closing tag`
   - Current workaround skips these
   - Proper fix: Timeout/limit on tag stack depth
   - If no closing tag found by end of list item, treat as malformed HTML block

4. **Generic Types**: `Vec<String>` or `Result<T, E>`
   - Should NOT trigger HTML detection
   - Detection: Opening `<` not at start of line/word boundary
   - Or: No `>` on same line means not a tag

5. **Comparison Operators**: `if x < 5 && y > 3`
   - Should NOT trigger HTML detection
   - Detection: `<` and `>` surrounded by spaces/alphanumeric

### Testing Strategy

**Test Files**:

- `tests/rules/md013_html_in_lists_test.rs`

**Test Cases**:

```rust
#[test]
fn test_html_block_preserved() {
    // HTML structure maintained exactly
}

#[test]
fn test_mixed_text_and_html() {
    // Text reflowed, HTML preserved
}

#[test]
fn test_nested_html_tags() {
    // Nested tags handled correctly
}

#[test]
fn test_generic_types_not_html() {
    // Vec<String> stays in paragraph
}

#[test]
fn test_comparison_operators_not_html() {
    // x < 5 && y > 3 stays in paragraph
}

#[test]
fn test_inline_html_reflowed() {
    // <em>text</em> stays in paragraph
}
```

### Performance Considerations

**Current Workaround**: O(n) scan for `<` and `>` characters
**Proper Solution**: O(n) with tag stack tracking

Performance impact is minimal:

- Tag detection is linear scan (same as current)
- Tag stack operations are O(1) push/pop
- No regex or complex parsing needed

### Backward Compatibility

**Breaking Changes**: None

- Existing non-HTML list items behave identically
- HTML list items that were skipped now get better handling
- No configuration changes needed

**Migration**: Automatic

- Users with HTML in list items will see improved formatting
- No manual intervention required

## Implementation Priority

**Phase 1**: Core HTML block detection

- Add `Block::HtmlBlock` variant
- Implement tag detection functions
- Basic integration into classification loop

**Phase 2**: Reflow logic integration

- Add HTML block preservation to reflow
- Handle mixed content properly
- Remove workaround code

**Phase 3**: Edge cases and polish

- Inline HTML detection
- HTML comments
- Malformed HTML handling
- Generic types disambiguation

**Phase 4**: Comprehensive testing

- All test cases from testing strategy
- Real-world markdown files (PyO3, etc.)
- Performance benchmarks

## Related Issues

- Current issue: PyO3's `guide/src/class/protocols.md:135:1`
- Root cause: Line 875's `.join(" ")` operation
- Workaround: Commit 34ae286 (skip HTML-containing items)
- Proper fix: This refactor plan

## References

- MD013 implementation: `src/rules/md013_line_length.rs`
- Text reflow utilities: `src/utils/text_reflow.rs`
- HTML block detection: `src/lint_context.rs` (for inspiration)
- CommonMark spec: HTML blocks section
