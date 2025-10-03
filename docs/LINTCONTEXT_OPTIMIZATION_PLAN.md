# LintContext Optimization Implementation Plan

## Overview

This document outlines the plan to optimize 54 rules that aren't using LintContext's early-exit optimization methods. The optimizations replace O(n) string scanning with O(1) cached character
frequency lookups.

## Analysis Summary

- **Total rules analyzed**: 54 rules without LintContext optimizations
- **Agent analysis completed**: 10 representative rules across different categories
- **Expected impact**: 10-100x speedup for documents without relevant markdown elements

## Optimization Categories

### Category 1: Direct Replacements (High Impact, Low Risk)

These rules can directly replace string scanning with LintContext methods.

#### 1. MD003 - Heading Style

**File**: `src/rules/md003_heading_style.rs`
**Current**: `!ctx.lines.iter().any(|line| line.heading.is_some())`
**Optimized**: `!ctx.likely_has_headings()`
**Location**: Line 237-240 in `should_skip()`
**Impact**: O(n) line iteration → O(1) character frequency check

#### 2. MD004 - Unordered List Style

**File**: `src/rules/md004_unordered_list_style.rs`
**Current**: `!ctx.content.contains(['*', '-', '+'])`
**Optimized**: `!ctx.likely_has_lists()`
**Locations**:

- Line 107-109 in `check()`
- Line 317 in `should_skip()`
**Impact**: O(n) string scan → O(1) integer comparison

#### 3. MD033 - No Inline HTML

**File**: `src/rules/md033_no_inline_html.rs`
**Current**: `!has_html_tags(content)` (custom function)
**Optimized**: `!ctx.likely_has_html()`
**Locations**:

- Line 287 in `check()`
- Line 438 in `should_skip()`
**Impact**: Eliminates redundant '<' character scanning

#### 4. MD042 - No Empty Links

**File**: `src/rules/md042_no_empty_links.rs`
**Current**: `!content.contains('[')`
**Optimized**: `!ctx.likely_has_links_or_images()`
**Location**: `should_skip()` method
**Impact**: O(n) string scan → O(1) bracket count check

#### 5. MD049 - Emphasis Style

**File**: `src/rules/md049_emphasis_style.rs`
**Current**: `!ctx.content.contains('*') && !ctx.content.contains('_')`
**Optimized**: `!ctx.likely_has_emphasis()`
**Locations**:

- Line 113-115 in `check()`
- Line 247 in `should_skip()`

**Impact**: Two O(n) scans → O(1) check (asterisk_count > 1 || underscore_count > 1)

#### 6. MD050 - Strong Style

**File**: `src/rules/md050_strong_style.rs`
**Current**: Similar to MD049 (checks for ** and __)
**Optimized**: Create `ctx.likely_has_strong()` or use `likely_has_emphasis()`
**Impact**: Same as MD049

#### 7. MD055 - Table Pipe Style

**File**: `src/rules/md055_table_pipe_style.rs`
**Current**: `!ctx.content.contains('|')`
**Optimized**: `!ctx.likely_has_tables()`
**Locations**:

- Line 174-177 in `should_skip()`
- Line 185-187 in `check()` (remove - redundant)
**Impact**: O(n) scan → O(1) check (pipe_count > 2)

#### 8. MD027 - Multiple Spaces in Blockquote

**File**: `src/rules/md027_multiple_spaces_blockquote.rs`
**Current**: `!ctx.content.contains('>')`
**Optimized**: `!ctx.likely_has_blockquotes()`
**Location**: Line 192 in `should_skip()`
**Impact**: O(n) scan → O(1) check (gt_count > 0)

### Category 2: Compound Checks (Medium Impact)

#### 9. MD040 - Fenced Code Language

**File**: `src/rules/md040_fenced_code_language.rs`
**Current**: `!content.contains("```") && !content.contains("~~~")`
**Optimized**: `!ctx.likely_has_code() && !ctx.has_char('~')`
**Location**: Line 341-344 in `should_skip()`
**Impact**: Two O(n) substring scans → O(1) + O(n) character check
**Note**: `likely_has_code()` only checks backticks; need separate tilde check

#### 10. MD031 - Blanks Around Fences

**File**: `src/rules/md031_blanks_around_fences.rs`
**Current**: TBD (needs analysis)
**Optimized**: Use `ctx.likely_has_code()`
**Impact**: TBD

#### 11. MD046 - Code Block Style

**File**: `src/rules/md046_code_block_style.rs`
**Current**: TBD (needs analysis)
**Optimized**: Use `ctx.likely_has_code()`
**Impact**: TBD

#### 12. MD048 - Code Fence Style

**File**: `src/rules/md048_code_fence_style.rs`
**Current**: TBD (needs analysis)
**Optimized**: Use `ctx.likely_has_code()`
**Impact**: TBD

### Category 3: Heading-Related Rules

Apply `ctx.likely_has_headings()` to these rules:

- MD001 - Heading Increment
- MD002 - First Heading H1
- MD018 - No Missing Space ATX
- MD019 - No Multiple Space ATX
- MD020 - No Missing Space Closed ATX
- MD021 - No Multiple Space Closed ATX
- MD022 - Blanks Around Headings
- MD023 - Heading Start Left
- MD024 - No Duplicate Heading
- MD025 - Single Title
- MD026 - No Trailing Punctuation
- MD041 - First Line Heading
- MD043 - Required Headings

### Category 4: List-Related Rules

Apply `ctx.likely_has_lists()` to these rules:

- MD005 - List Indent
- MD006 - Start Bullets
- MD007 - UL Indent
- MD029 - Ordered List Prefix
- MD030 - List Marker Space
- MD032 - Blanks Around Lists

### Category 5: Link/Image-Related Rules

Apply `ctx.likely_has_links_or_images()` to these rules:

- MD011 - No Reversed Links
- MD034 - No Bare URLs
- MD039 - No Space in Links
- MD045 - No Alt Text
- MD051 - Link Fragments
- MD052 - Reference Links/Images
- MD053 - Link Image Reference Definitions
- MD054 - Link Image Style
- MD057 - Existing Relative Links

### Category 6: Emphasis/Strong-Related Rules

Apply `ctx.likely_has_emphasis()` to these rules:

- MD036 - No Emphasis Only First
- MD037 - Spaces Around Emphasis
- MD038 - No Space in Code

### Category 7: Blockquote-Related Rules

Apply `ctx.likely_has_blockquotes()` to these rules:

- MD028 - No Blanks in Blockquote

### Category 8: Table-Related Rules

Apply `ctx.likely_has_tables()` to these rules:

- MD056 - Table Column Count
- MD058 - Blanks Around Tables

### Category 9: Simple Character Checks

These rules need custom optimization using `has_char()`:

- MD009 - Trailing Spaces (already optimized, uses `has_char(' ')`)
- MD010 - No Hard Tabs (use `has_char('\t')`, consider adding tab_count to CharFrequency)
- MD012 - No Multiple Blanks (check for '\n')
- MD035 - HR Style (check for '-', '*', '_')
- MD047 - Single Trailing Newline (check for '\n')

### Category 10: Special Cases

- MD014 - Commands Show Output (check for '```' or code blocks)
- MD044 - Proper Names (no early exit possible - must scan all text)

## Architectural Improvements

### Add tab_count to CharFrequency

**File**: `src/lint_context.rs`

Add to CharFrequency struct (around line 289):

```rust
/// Count of \t characters (tabs)
pub tab_count: usize,
```

Update `compute_char_frequency()` (around line 2240):

```rust
'\t' => frequency.tab_count += 1,
```

Update `has_char()` (around line 644):

```rust
'\t' => self.char_frequency.tab_count > 0,
```

Update `char_count()` (around line 663):

```rust
'\t' => self.char_frequency.tab_count,
```

## Implementation Strategy

### Phase 1: Quick Wins (Category 1 - 8 rules)

Implement direct replacements with minimal risk:

- MD003, MD004, MD033, MD042, MD049, MD055, MD027, MD040

### Phase 2: Systematic Rollout (Categories 3-8 - ~40 rules)

Apply pattern-based optimizations to rule categories:

- Heading rules → `likely_has_headings()`
- List rules → `likely_has_lists()`
- Link/image rules → `likely_has_links_or_images()`
- Emphasis rules → `likely_has_emphasis()`
- Blockquote rules → `likely_has_blockquotes()`
- Table rules → `likely_has_tables()`

### Phase 3: Architecture (Category 9)

- Add `tab_count` to CharFrequency
- Add `space_count` to CharFrequency (optional)
- Add `newline_count` to CharFrequency (optional)

### Phase 4: Validation

- Run full test suite after each phase
- Benchmark performance improvements
- Verify no false negatives introduced

## Expected Performance Impact

### Documents Without Relevant Elements

- **Before**: O(n) string/line scanning on every rule
- **After**: O(1) character frequency check
- **Speedup**: 10-100x depending on document size

### Documents With Relevant Elements

- **Before**: O(n) scan + rule processing
- **After**: O(1) check + rule processing
- **Speedup**: Minimal (rule still needs to run)

### Overall Impact

- **Typical workload**: 10-30% faster (mix of documents with/without elements)
- **Best case**: 100x faster (processing large files without relevant markdown)
- **Worst case**: No regression (documents with elements still process normally)

## Testing Strategy

For each optimized rule:

1. Ensure existing tests pass
2. Add test for empty document (should skip)
3. Add test for document without relevant elements (should skip)
4. Verify no false negatives (all violations still caught)

## Rollout Plan

1. **Week 1**: Implement Phase 1 (8 quick wins)
2. **Week 2**: Implement Phase 2 (40 systematic rollouts)
3. **Week 3**: Implement Phase 3 (architecture improvements)
4. **Week 4**: Validation and benchmarking

## Success Metrics

- All 54 rules use appropriate LintContext optimization methods
- Test suite passes with 100% success
- No false negatives introduced
- Measurable performance improvement (>10% on typical workloads)
- Code consistency across all rules
