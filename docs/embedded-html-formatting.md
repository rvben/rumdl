# Embedded HTML Formatting

rumdl includes built-in support for checking and formatting HTML blocks embedded inside your Markdown files. It can also format JavaScript and TypeScript code blocks inside `<script>` tags within
those HTML blocks.

All formatting is done directly in Rust via library calls (using the `markup_fmt` and `dprint-plugin-typescript` crates), without spawning any external subprocesses.

## Overview

- **Lint Mode** (`rumdl check`): Validates that HTML blocks and `<script>` blocks inside them are correctly formatted, emitting warnings if not.
- **HTML Formatting**: Handled by `markup_fmt`.
- **Script Formatting**: Handled by `dprint-plugin-typescript` (supports JS, TS, JSX, TSX).
- **Subprocess-free**: Fast execution, entirely integrated into the binary.

## Configuration

Configure HTML formatting in the `[html]` section of your `rumdl.toml` (or `pyproject.toml` under `[tool.rumdl.html]`):

```toml
[html]
# Master switch for HTML checking and formatting (default: true)
enabled = true

# Target line width for the HTML formatter (default: 80)
print-width = 80

# Indentation size for the HTML formatter (default: 2)
indent-width = 2

# Use tab characters for indentation (default: false)
use-tabs = false

# Quote character preference for HTML attributes: "double" or "single" (default: "double")
quotes = "double"

[html.script]
# Whether to format JavaScript/TypeScript inside <script> tags (default: true)
enabled = true

# Semicolon style preference: "always", "prefer", or "asi" (default: "prefer")
semi-colons = "prefer"

# Quote style preference: "always-double", "always-single", "prefer-double", or "prefer-single" (default: "prefer-double")
quote-style = "prefer-double"
```

## Behavior

- **Block-level Only**: rumdl only formats HTML *blocks* (blocks of HTML parsed by the Markdown parser, such as a `<div>` starting on its own line). Inline HTML tags (like `This is <b>bold</b>`) are
    ignored to prevent layout disruptions.
- **JSX/Component Skipping**: In MDX documents, component tags (tags starting with an uppercase letter, e.g., `<Header />` or `<MyComponent>`) are recognized as JSX and automatically skipped,
    preventing the HTML parser from failing on non-standard tags.
- **Error Resiliency**: If an HTML block contains malformed HTML that fails to parse, rumdl skips formatting that block rather than failing the entire linting run.

## Example

Given the following unformatted Markdown:

```markdown
Some text.

<div class="container">
<p>Hello World</p>
  <script>
const a=1;
  const b = "double";
  </script>
</div>
```

Running `rumdl check` will emit warnings pointing to the HTML block starting at `<div class="container">` and the script block inside it.

Running `rumdl check --fix` will format the block in-place:

```markdown
Some text.

<div class="container">
  <p>Hello World</p>
  <script>
    const a = 1;
    const b = 'double';
  </script>
</div>
```
