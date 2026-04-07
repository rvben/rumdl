use rumdl_lib::MD046CodeBlockStyle;
use rumdl_lib::lint_context::LintContext;
use rumdl_lib::rule::Rule;
use rumdl_lib::rules::CodeBlockStyle;

#[test]
fn test_consistent_fenced_blocks() {
    let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Fenced);
    let content = "# Code blocks\n\n```\ncode here\n```\n\n```rust\nmore code\n```";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_consistent_indented_blocks() {
    let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Indented);
    let content = "# Code blocks\n\n    code here\n    more code\n\n    another block\n    continues";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_mixed_blocks_prefer_fenced() {
    let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Fenced);
    let content = "# Mixed blocks\n\n```\nfenced block\n```\n\n    indented block";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);
    let fixed = rule.fix(&ctx).unwrap();
    assert!(fixed.contains("# Mixed blocks"), "Should preserve headings");
    assert!(
        fixed.contains("```\nfenced block\n```"),
        "Should preserve fenced blocks"
    );
    assert!(
        fixed.contains("```\nindented block"),
        "Should convert indented blocks to fenced"
    );
}

#[test]
fn test_mixed_blocks_prefer_indented() {
    let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Indented);
    let content = "# Mixed blocks\n\n```\nfenced block\n```\n\n    indented block";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(
        result.len(),
        1,
        "Should detect the fenced code block when indented style is preferred"
    );
    let fixed = rule.fix(&ctx).unwrap();
    assert!(fixed.contains("# Mixed blocks"), "Should preserve headings");
    assert!(
        fixed.contains("    fenced block") && !fixed.contains("```\nfenced block\n```"),
        "Should convert fenced blocks to indented"
    );
    assert!(fixed.contains("    indented block"), "Should preserve indented blocks");
}

#[test]
fn test_consistent_style_fenced_first() {
    let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Consistent);
    let content = "# Mixed blocks\n\n```\nfenced block\n```\n\n    indented block";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(!result.is_empty(), "Should detect inconsistent code blocks");
    let fixed = rule.fix(&ctx).unwrap();
    assert!(fixed.contains("# Mixed blocks"), "Should preserve headings");
    assert!(
        fixed.contains("```\nfenced block\n```"),
        "Should preserve fenced blocks"
    );
    assert!(
        fixed.contains("```\nindented block"),
        "Should convert indented blocks to fenced"
    );
}

#[test]
fn test_consistent_style_indented_first() {
    let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Consistent);
    // One indented and one fenced block - tie prefers fenced
    let content = "# Mixed blocks\n\n    indented block\n\n```\nfenced block\n```";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(!result.is_empty(), "Should detect inconsistent code blocks");
    let fixed = rule.fix(&ctx).unwrap();
    assert!(fixed.contains("# Mixed blocks"), "Should preserve headings");
    assert!(
        fixed.contains("```\nfenced block\n```"),
        "Should preserve fenced blocks"
    );
    assert!(
        fixed.contains("```\nindented block\n```") && !fixed.contains("    indented block"),
        "Should convert indented blocks to fenced"
    );
}

#[test]
fn test_empty_content() {
    let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Consistent);
    let content = "";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_no_code_blocks() {
    let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Consistent);
    let content = "# Just a heading\n\nSome regular text\n\n> A blockquote";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_fenced_with_language() {
    let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Fenced);
    let content = "```rust\nlet x = 42;\n```\n\n```python\nprint('hello')\n```";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_convert_indented_preserves_content() {
    let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Fenced);
    let content = "    let x = 42;\n    println!(\"{}\", x);";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(!result.is_empty(), "Should detect indented code blocks");
    let fixed = rule.fix(&ctx).unwrap();
    assert!(
        fixed.contains("```\nlet x = 42;\nprintln!"),
        "Should convert indented to fenced and preserve content"
    );
}

#[test]
fn test_markdown_example_blocks() {
    let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Consistent);
    // This is a common pattern in markdown documentation showing markdown examples
    let content = r#"# Documentation

## Example Usage

```markdown
Here's how to use code blocks:

```python
def hello():
    print("Hello, world!")
```

And here's another example:

```javascript
console.log("Hello");
```
```

The above shows proper markdown syntax.
"#;

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // Should not detect any issues with markdown example blocks
    assert!(
        result.is_empty(),
        "Should not flag code blocks inside markdown examples as unclosed"
    );

    // Test that fix doesn't break markdown examples
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(content, fixed, "Should not modify markdown example blocks");
}

#[test]
fn test_unclosed_code_block() {
    let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Consistent);
    let content = "# Test\n\n```python\ndef hello():\n    print('world')\n\nThis content is inside an unclosed block.";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1, "Should detect exactly one unclosed code block");
    assert!(
        result[0].message.contains("never closed"),
        "Should mention that the block is unclosed"
    );
    assert_eq!(result[0].line, 3, "Should point to the opening fence line");

    let fixed = rule.fix(&ctx).unwrap();
    assert!(fixed.contains("```python"), "Should preserve the opening fence");
    assert!(fixed.ends_with("```"), "Should add closing fence at the end");
}

#[test]
fn test_unclosed_tilde_code_block() {
    let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Consistent);
    let content = "# Test\n\n~~~javascript\nfunction test() {\n  return 42;\n}\n\nMore content here.";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1, "Should detect exactly one unclosed code block");
    assert!(
        result[0].message.contains("never closed"),
        "Should mention that the block is unclosed"
    );
    assert!(
        result[0].message.contains("~~~"),
        "Should mention the tilde fence marker"
    );

    let fixed = rule.fix(&ctx).unwrap();
    assert!(fixed.contains("~~~javascript"), "Should preserve the opening fence");
    assert!(fixed.ends_with("~~~"), "Should add closing tilde fence at the end");
}

#[test]
fn test_nested_code_block_not_detected_by_md046() {
    // Nested fence collision detection is handled by MD070, not MD046
    let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Fenced);
    let content = "# Test\n\n```bash\n\n```markdown\n\n# Hello world\n\n```\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // MD046 should not produce "should be closed before" warnings
    let nested_warnings: Vec<_> = result
        .iter()
        .filter(|w| w.message.contains("should be closed before"))
        .collect();
    assert!(
        nested_warnings.is_empty(),
        "MD046 should not detect nested fences (MD070 handles this)"
    );
}

#[test]
fn test_nested_code_block_different_languages_no_md046_warning() {
    // Nested fence collision is handled by MD070, not MD046
    let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Fenced);
    let content = "```python\n\n```javascript\ncode here\n```\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    let nested_warnings: Vec<_> = result
        .iter()
        .filter(|w| w.message.contains("should be closed before"))
        .collect();
    assert!(nested_warnings.is_empty(), "MD046 should not detect nested fences");
}

#[test]
fn test_nested_markdown_blocks_no_md046_warning() {
    // Nested fence collision is handled by MD070, not MD046
    let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Fenced);
    let content = "```bash\n\n```markdown\n# Example\n```\n```\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    let nested_warnings: Vec<_> = result
        .iter()
        .filter(|w| w.message.contains("should be closed before"))
        .collect();
    assert!(nested_warnings.is_empty(), "MD046 should not detect nested fences");
}

#[test]
fn test_markdown_block_with_multiple_languages() {
    // Test markdown block containing multiple nested code blocks
    let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Consistent);
    let content = r#"```markdown
# Example

Here's Python:
```python
print("Hello")
```

And JavaScript:
```javascript
console.log("Hello");
```

And unclosed:
```rust
fn main() {
```
```"#; // Close the markdown block

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // Should not flag anything - all nested blocks are inside markdown documentation
    assert!(
        result.is_empty(),
        "Nested blocks in markdown documentation should be allowed"
    );
}

#[test]
fn test_nested_same_language_no_md046_warning() {
    // Nested fence collision is handled by MD070, not MD046
    let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Fenced);
    let content = "```python\n\n```python\nprint('nested')\n```";

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    let nested_warnings: Vec<_> = result
        .iter()
        .filter(|w| w.message.contains("should be closed before"))
        .collect();
    assert!(nested_warnings.is_empty(), "MD046 should not detect nested fences");
}

#[test]
fn test_deeply_nested_markdown_blocks() {
    // Test markdown blocks inside markdown blocks
    let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Consistent);
    let content = r#"````markdown
# Outer markdown

```markdown
## Inner markdown

```python
print("code")
```
```

More content
````"#;

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    assert!(result.is_empty(), "Deeply nested markdown blocks should be allowed");
}

#[test]
fn test_mixed_fence_lengths() {
    // Test with different fence lengths
    let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Fenced);
    let content = "````python\n\n```javascript\ncode\n```\n\n````";

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // Should work correctly with 4-backtick fences
    assert!(result.is_empty(), "Different fence lengths should work correctly");
}

#[test]
fn test_adjacent_code_blocks() {
    // Test code blocks that are adjacent (not nested)
    let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Consistent);
    let content = "```rust\ncode1\n```\n```python\ncode2\n```";

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    assert!(result.is_empty(), "Adjacent blocks should not be flagged as nested");
}

#[test]
fn test_markdown_block_at_end() {
    // Test markdown block that ends the document with a proper closing fence.
    // Per CommonMark spec, ```rust inside the markdown block is content (not a closing fence
    // because it has "rust" after), and the final ``` closes the markdown block.
    let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Consistent);
    let content = "# Doc\n\n```markdown\n## Example\n\n```rust\nfn test() {}\n```";

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // The markdown block is properly closed by the final ```
    // The ```rust is content (not a closing fence because "rust" follows)
    assert!(
        result.is_empty(),
        "Should not flag markdown block that is properly closed"
    );
}

#[test]
fn test_fence_in_list_context() {
    // Test code blocks in list items
    let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Fenced);
    let content = r#"1. First item
   ```rust
   fn in_list() {}
   ```

2. Second item
   ```python
   def also_in_list():
       pass
   ```"#;

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    assert!(result.is_empty(), "Code blocks in lists should not cause issues");
}

#[test]
fn test_issue_257_fence_on_list_marker_line() {
    // Issue 257: Fence marker immediately following list marker on same line
    // The closing fence is indented to match list content, not the opening fence
    let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Fenced);

    // Pattern 1: Fence immediately after list marker
    let content = r#"- Sample code:
- ```java
      List<Map<String,String>> inputs = new List<Map<String,String>>();
  ```"#;

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // Should NOT report unclosed block - the indented ``` closes the block
    let unclosed_warnings: Vec<_> = result.iter().filter(|w| w.message.contains("never closed")).collect();
    assert!(
        unclosed_warnings.is_empty(),
        "List-indented closing fence should be recognized: {unclosed_warnings:?}"
    );

    // Pattern 2: Multiple list items with inline fences
    let content2 = r#"- ```python
  print("hello")
  ```
- ```rust
  fn main() {}
  ```"#;

    let ctx2 = LintContext::new(content2, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result2 = rule.check(&ctx2).unwrap();

    let unclosed2: Vec<_> = result2.iter().filter(|w| w.message.contains("never closed")).collect();
    assert!(
        unclosed2.is_empty(),
        "Multiple inline fences should all be recognized as closed: {unclosed2:?}"
    );

    // Pattern 3: Nested list with inline fence
    let content3 = r#"- Outer item
  - ```bash
    echo "nested"
    ```"#;

    let ctx3 = LintContext::new(content3, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result3 = rule.check(&ctx3).unwrap();

    let unclosed3: Vec<_> = result3.iter().filter(|w| w.message.contains("never closed")).collect();
    assert!(
        unclosed3.is_empty(),
        "Nested list inline fence should be recognized as closed: {unclosed3:?}"
    );

    // Pattern 4: Tilde fences in list context
    let content4 = r#"- ~~~python
  print("tilde fence")
  ~~~"#;

    let ctx4 = LintContext::new(content4, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result4 = rule.check(&ctx4).unwrap();

    let unclosed4: Vec<_> = result4.iter().filter(|w| w.message.contains("never closed")).collect();
    assert!(
        unclosed4.is_empty(),
        "Tilde fence in list should be recognized as closed: {unclosed4:?}"
    );

    // Pattern 5: Longer fence in list (4+ backticks)
    let content5 = r#"- ````markdown
  ```python
  nested code
  ```
  ````"#;

    let ctx5 = LintContext::new(content5, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result5 = rule.check(&ctx5).unwrap();

    let unclosed5: Vec<_> = result5.iter().filter(|w| w.message.contains("never closed")).collect();
    assert!(
        unclosed5.is_empty(),
        "Longer fence in list should be recognized as closed: {unclosed5:?}"
    );
}

#[test]
fn test_python_traceback_tilde_markers() {
    // Test that Python traceback decoration lines are not treated as code fences
    let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Consistent);
    let content = r#"# Python Traceback Example

```pycon
>>> swap_ends([])
Traceback (most recent call last):
  File "/home/trey/swap_ends.py", line 4, in swap_ends
    sequence[0], sequence[-1] = sequence[-1], sequence[0]
                                ~~~~~~~~^^^^
IndexError: list index out of range

During handling of the above exception, another exception occurred:

Traceback (most recent call last):
  File "<stdin>", line 1, in <module>
  File "/home/trey/swap_ends.py", line 7, in swap_ends
    raise ValueError("Empty sequence not allowed")
ValueError: Empty sequence not allowed
```

The `~~~~~~~~^^^^` markers show where the error occurred.
"#;

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    assert!(
        result.is_empty(),
        "Python traceback tilde markers should not be treated as code fences"
    );
}

#[test]
fn test_various_tilde_patterns_in_code() {
    // Test various patterns with tildes that should not be treated as fences
    let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Consistent);
    let content = r#"# Various Tilde Patterns

```python
# ASCII art dividers
print("~" * 50)  # ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
print("~=~=~=~=~=~=~=~=~=~")

# Python traceback style
#     sequence[0], sequence[-1] = sequence[-1], sequence[0]
#                                 ~~~~~~~~^^^^
# IndexError: list index out of range

# Other patterns
# ~~~~ This is not a fence ~~~~
# ~!@#$%^&*()  # Random characters after tildes
```

All the above should be fine.
"#;

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    assert!(
        result.is_empty(),
        "Tilde patterns inside code blocks should not be treated as fences"
    );
}

#[test]
fn test_valid_tilde_fences() {
    // Test that valid tilde fences still work correctly
    let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Consistent);
    let content = r#"# Valid Tilde Fences

~~~
Simple fence
~~~

~~~ python
Code with language
~~~

~~~~
Four tildes
~~~~

~~~~~
Five tildes with trailing spaces
~~~~~
"#;

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    assert!(result.is_empty(), "Valid tilde fences should work correctly");
}

#[test]
fn test_invalid_fence_patterns() {
    // Test patterns that look like fences but aren't valid
    let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Consistent);
    let content = r#"# Invalid Fence Patterns

```
# These lines inside the code block should not be treated as fences:
~~~notafence
```~~~stillnotafence
~~~!@#$%
~~~~~~~~^^^^
```

The code block above should be properly closed.
"#;

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    assert!(
        result.is_empty(),
        "Invalid fence patterns inside code blocks should be ignored"
    );
}

#[test]
fn test_issue_27_html_blocks_not_indented_code() {
    // Test for issue #27 - HTML blocks with indentation should not be treated as indented code blocks
    let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Fenced);
    let content = r#"<div>
<ul>
    <li>Variables are pointers in Python</li>
    <li>Assignment versus Mutation in Python</li>
</ul>
</div>

```pycon
>>> numbers = [2, 1, 3, 4, 7]
>>> numbers2 = [11, 18, 29]
>>> name = "Trey"
```"#;

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    assert!(
        result.is_empty(),
        "HTML blocks with indentation should not be flagged as indented code blocks (issue #27)"
    );
}

#[test]
fn test_issue_118_content_inside_fenced_blocks() {
    // Regression test for Issue #118
    // MD046 should not flag content inside fenced code blocks
    let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Fenced);
    let content = r#"Here is **example 1**:

```bash
$ python one_up.py
What's your favorite number? 7
I can one up that.
Traceback (most recent call last):
  File "/home/trey/one_up.py", line 3, in <module>
    print(favorite_number+1)
          ~~~~~~~~~~~~~~~^~
TypeError: can only concatenate str (not "int") to str
```"#;

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // Should not flag the ~~~ or ^ characters inside the fenced block
    assert_eq!(
        result.len(),
        0,
        "MD046 should not flag content inside fenced code blocks (issue #118)"
    );
}

#[test]
fn test_issue_285_blockquote_indented_content() {
    // Issue #285: Indented content inside blockquotes should not be flagged as indented code blocks
    // When content after `>` has 4+ spaces, it's blockquote content, not a document-level code block
    let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Fenced);
    let content = r#"# Issue 285 - MD046 false positive

But as we see in the history file, the `-L` and `-S` flags are used:
>      -S socket-path
>                    Specify a full alternative path to the server socket.
"#;

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    assert!(
        result.is_empty(),
        "Indented content inside blockquotes should not be flagged as indented code blocks (issue #285)"
    );
}

#[test]
fn test_blockquote_with_heavily_indented_content() {
    // Test various levels of indentation inside blockquotes
    let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Fenced);
    let content = r#"# Blockquote Indentation Tests

> Normal blockquote content
>     Four spaces (would be code block outside blockquote)
>         Eight spaces
>              Twelve spaces
"#;

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    assert!(
        result.is_empty(),
        "Heavily indented blockquote content should not trigger MD046"
    );
}

#[test]
fn test_blockquote_code_block_inside() {
    // Actual fenced code blocks INSIDE blockquotes should still be detected properly
    let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Fenced);
    let content = r#"# Fenced Code Inside Blockquote

> Here's some code:
>
> ```python
> def hello():
>     print("Hello from blockquote")
> ```
>
> That was the code.
"#;

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    assert!(
        result.is_empty(),
        "Fenced code blocks inside blockquotes should work correctly"
    );
}

#[test]
fn test_nested_blockquote_with_indentation() {
    // Test nested blockquotes with indented content
    let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Fenced);
    let content = r#"# Nested Blockquotes

> Outer blockquote
>> Inner blockquote
>>     Indented content in inner blockquote
>>          More indentation
> Back to outer
>     Indented in outer
"#;

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    assert!(
        result.is_empty(),
        "Nested blockquotes with indentation should not trigger MD046"
    );
}

#[test]
fn test_blockquote_then_indented_code() {
    // Test that actual indented code blocks AFTER blockquotes are still detected
    let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Fenced);
    let content = r#"# Blockquote Then Code

> This is a blockquote
> with multiple lines

    This is actual indented code (after the blockquote)
"#;

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // Should detect the indented code block after the blockquote
    assert_eq!(
        result.len(),
        1,
        "Indented code block after blockquote should still be detected"
    );
}

#[test]
fn test_mixed_blockquote_and_code() {
    // Test mix of blockquotes and code blocks
    let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Fenced);
    let content = r#"# Mixed Content

> Blockquote with indented content:
>     -L flag description
>     -S socket-path description

```python
# This is actual fenced code
print("hello")
```

> Another blockquote
>     More indented content
"#;

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    assert!(
        result.is_empty(),
        "Mixed blockquotes and fenced code blocks should work correctly"
    );
}

/// Helper: run check → fix → check again and assert no violations remain after fix.
/// This is the canonical test for check/fix alignment.
fn assert_fix_resolves_violations(rule: &MD046CodeBlockStyle, content: &str) {
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let before = rule.check(&ctx).unwrap();
    assert!(
        !before.is_empty(),
        "Expected violations before fix, got none. Content:\n{content}"
    );

    let fixed = rule.fix(&ctx).unwrap();
    let fixed_ctx = LintContext::new(&fixed, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let after = rule.check(&fixed_ctx).unwrap();
    assert!(
        after.is_empty(),
        "Violations remain after fix.\nOriginal violations: {before:?}\nAfter fix violations: {after:?}\nFixed content:\n{fixed}"
    );
}

#[test]
fn test_roundtrip_multiple_indented_blocks_to_fenced() {
    // Multiple separate indented code blocks - fix must convert ALL of them
    let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Fenced);
    let content =
        "# Doc\n\n    first block\n    continues\n\nSome text.\n\n    second block\n\nMore text.\n\n    third block\n";
    assert_fix_resolves_violations(&rule, content);
}

#[test]
fn test_roundtrip_multiple_fenced_blocks_to_indented() {
    // Multiple fenced blocks - fix must convert ALL of them
    let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Indented);
    let content = "# Doc\n\n```\nfirst block\n```\n\nSome text.\n\n```python\nsecond block\n```\n\nMore text.\n\n```\nthird block\n```\n";
    assert_fix_resolves_violations(&rule, content);
}

#[test]
fn test_roundtrip_consistent_mixed_prefers_fenced() {
    // Consistent mode: 2 fenced, 1 indented → should fix to all fenced
    let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Consistent);
    let content = "```\nblock one\n```\n\nText.\n\n```\nblock two\n```\n\nText.\n\n    indented block\n";
    assert_fix_resolves_violations(&rule, content);
}

#[test]
fn test_roundtrip_consistent_mixed_prefers_indented() {
    // Consistent mode: 2 indented, 1 fenced → should fix to all indented
    let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Consistent);
    let content = "    indented one\n\nText.\n\n    indented two\n\nText.\n\n```\nfenced block\n```\n";
    assert_fix_resolves_violations(&rule, content);
}

#[test]
fn test_roundtrip_indented_block_preceded_by_paragraph() {
    // Indented block immediately after paragraph text (no blank line before) is NOT a code block per CommonMark.
    // When it IS a code block (preceded by blank line), fix should handle it correctly.
    let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Fenced);
    let content = "Paragraph text.\n\n    code line one\n    code line two\n    code line three\n";
    assert_fix_resolves_violations(&rule, content);
}

#[test]
fn test_roundtrip_fenced_with_info_string_to_indented() {
    // Fenced block with info string (```python) - info string must be dropped in conversion
    let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Indented);
    let content = "```python\ndef hello():\n    print('world')\n```\n";
    assert_fix_resolves_violations(&rule, content);
}

#[test]
fn test_roundtrip_longer_fence_to_indented() {
    // Four-backtick fence - fix must correctly identify the closing fence
    // and not confuse a ``` line inside the block as the closer
    let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Indented);
    let content = "````python\n```not-a-close\ncode here\n```\n````\n";
    assert_fix_resolves_violations(&rule, content);
}

#[test]
fn test_roundtrip_tilde_fence_to_indented() {
    // Tilde-style fence - fix must match tilde closing fence, not backtick
    let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Indented);
    let content = "~~~\ncode content\n~~~\n";
    assert_fix_resolves_violations(&rule, content);
}

#[test]
fn test_fix_longer_fence_closing_detection() {
    // When a fenced block opens with ```` (4 backticks), fix() must use the full
    // opener length to find the closer, not just ```.
    // A ``` line inside the block is CONTENT, not a closer.
    let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Indented);
    let content = "````\n```\ncode here\n```\n````\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let fixed = rule.fix(&ctx).unwrap();

    // The fixed output should have exactly one indented block (no fences remain)
    let fixed_ctx = LintContext::new(&fixed, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let after = rule.check(&fixed_ctx).unwrap();
    assert!(
        after.is_empty(),
        "Violations remain after fix for 4-backtick fence.\nFixed content:\n{fixed}\nViolations: {after:?}"
    );

    // The content inside should be preserved (``` line should appear as indented content)
    assert!(
        fixed.contains("    ```"),
        "The inner ``` should be preserved as indented content, got:\n{fixed}"
    );
}
