# Inline Configuration Reference

This document describes how to disable or configure rumdl rules using inline HTML comments within your Markdown files.

## Overview

rumdl supports inline configuration comments that allow you to disable rules for specific sections, lines, or entire files. This is useful when you need to make exceptions for specific content without
changing your global configuration.

## Supported Comment Formats

rumdl uses its own comment prefix, with support for markdownlint comments for compatibility:

- `<!-- rumdl-... -->` - Primary rumdl syntax
- `<!-- markdownlint-... -->` - Supported for markdownlint compatibility

## Basic Usage

### Disable All Rules

Disable all rules from a specific point until re-enabled:

```markdown
This text is linted normally.

<!-- rumdl-disable -->
This text is not linted at all.
Rules violations here are ignored.
<!-- rumdl-enable -->

This text is linted again.
```

### Disable Specific Rules

Disable only specific rules:

```markdown
Normal linting applies here.

<!-- rumdl-disable MD013 MD033 -->
This content can have long lines (MD013) and inline HTML (MD033).
Other rules still apply.
<!-- rumdl-enable MD013 MD033 -->

Back to normal linting.
```

## Line-Specific Configuration

### Disable for Current Line

Disable rules for the line containing the comment:

```markdown
This is a really long line that would normally violate MD013 but it's allowed <!-- rumdl-disable-line MD013 -->

This line is checked normally.
```

### Disable for Next Line

Disable rules for the line following the comment:

```markdown
<!-- rumdl-disable-next-line MD013 -->
This very long line is allowed to exceed the normal line length limit without triggering a warning.

This line is checked normally.
```

### Prettier Compatibility

For compatibility with Prettier, rumdl also supports:

```markdown
<!-- prettier-ignore -->
This line won't be checked for any rules.
```

## File-Level Configuration

### Disable for Entire File

Place at the beginning of the file to disable rules for the entire document:

```markdown
<!-- rumdl-disable-file -->

# This Document

Nothing in this file will be linted.
```

Or disable specific rules for the entire file:

```markdown
<!-- rumdl-disable-file MD013 MD033 -->

# This Document

Long lines and inline HTML are allowed throughout this file.
Other rules still apply.
```

### Configure Rules for File

Configure specific rule settings for the entire file:

```markdown
<!-- rumdl-configure-file { "MD013": { "line_length": 120 } } -->

# This Document

This file uses a line length of 120 instead of the default.
```

Unlike the other comments, this one may span multiple lines, which keeps larger
configurations readable:

```markdown
<!-- rumdl-configure-file
{
  "MD013": { "line_length": 120 },
  "MD007": { "indent": 4 }
}
-->
```

A rule can also be given `true` or `false` instead of an options object, to turn
it on or off for the whole file:

```markdown
<!-- rumdl-configure-file { "MD012": false } -->
```

Rule names may be written as either an ID (`MD013`) or an alias
(`line-length`). Several `configure-file` comments may appear in one document;
each is applied, and a later one wins for any rule it repeats.

## Advanced Features

### Capture and Restore

Save and restore the current configuration state:

```markdown
Normal rules apply here.

<!-- rumdl-capture -->
<!-- rumdl-disable MD013 MD033 -->

Content with specific rules disabled.

<!-- rumdl-restore -->

Previous configuration state is restored.
```

## Examples

### Example 1: Documentation with Code Examples

````markdown
# API Documentation

The API follows standard REST conventions.

<!-- rumdl-disable MD013 -->

```bash
curl -X POST <https://api.example.com/v1/users/create> -H "Authorization: Bearer very-long-token-that-would-normally-violate-line-length" -d '{"name": "John Doe"}'
```

<!-- rumdl-enable MD013 -->

Regular documentation continues here.
````

### Example 2: Tables with Long Content

````markdown
## Configuration Options

<!-- rumdl-disable MD013 -->
| Option | Description | Default | Example |
|--------|-------------|---------|---------|
| `authentication_token` | A very long description about authentication tokens in the system | `null` | `Bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9...` |
<!-- rumdl-enable MD013 -->
````

### Example 3: HTML Content

````markdown
# Dashboard

<!-- rumdl-disable MD033 -->
<div class="alert alert-warning">
  <strong>Warning!</strong> This feature is experimental.
</div>
<!-- rumdl-enable MD033 -->

Continue with regular Markdown content.
````

### Example 4: Generated Content

````markdown
<!-- rumdl-disable-file -->
<!-- This file is auto-generated. Do not edit manually. -->

# Generated API Reference

[Auto-generated content that may not follow all linting rules]
````

## Important Notes

1. **Comment Placement**: Comments must be on their own line (except for `disable-line`)
2. **Code Blocks**: Comments inside code blocks are ignored and won't affect
   configuration, whether the block is fenced or indented. An indented block the
   document's flavor holds as structure is not code, so a comment in a MkDocs
   admonition or content-tab body configures the document like any other
3. **Case Insensitive**: Rule names are case-insensitive (MD013, md013, Md013 all work)
4. **Specificity**: More specific configurations override general ones
5. **Compatibility**: Both `rumdl` (primary) and `markdownlint` (for compatibility) prefixes work identically

## Rule Names Reference

When specifying rules in inline comments, you can use either:

- **Rule IDs**: `MD001`, `MD013`, `MD033`, etc.
- **Aliases**: `heading-increment`, `line-length`, `no-inline-html`, etc.

Both formats are case-insensitive and work identically:

````markdown
<!-- These are equivalent -->
<!-- rumdl-disable MD013 -->
<!-- rumdl-disable line-length -->

<!-- Multiple rules with mixed formats -->
<!-- rumdl-disable MD001 line-length no-inline-html -->
````

Each rule's alias is listed at the top of its documentation page. See the [Rules Reference](rules.md) for a complete list of available rules and their aliases.

## Comparison with Global Configuration

| Use Case              | Inline Configuration  | Global Configuration     |
| --------------------- | --------------------- | ------------------------ |
| Temporary exceptions  | ✅ Best choice        | ❌ Too permanent         |
| File-specific rules   | ✅ Good for few files | ✅ Better for many files |
| Generated content     | ✅ Use disable-file   | ✅ Use exclude patterns  |
| Project-wide settings | ❌ Too scattered      | ✅ Best choice           |

## Best Practices

1. **Use sparingly**: Inline configuration should be the exception, not the rule
2. **Document why**: Add a comment explaining why rules are disabled
3. **Be specific**: Disable only the specific rules needed, not all rules
4. **Re-enable quickly**: Re-enable rules as soon as the exception is no longer needed
5. **Consider alternatives**: Before disabling a rule, consider if the content can be restructured

Example with explanation:

````markdown
<!-- Disable MD013 for this command example as it cannot be wrapped -->
<!-- rumdl-disable-next-line MD013 -->
docker run -d --name myapp -p 8080:8080 -e DATABASE_URL=postgresql://user:pass@localhost:5432/db mycompany/myapp:latest

Regular content continues here.
````

### Finding comments that no longer suppress anything

A disable comment is written for one line at one moment, and nothing points it
out once the line changes and the comment stops mattering. The rule it names
stays off, so a finding that appears there later goes unreported.

[MD087](md087.md) reports a disable comment that removed no finding from the
run. It is off by default:

```toml
[global]
extend-enable = ["MD087"]
```

```text
file.md:1:15: [MD087] Unused disable-line comment: MD013
```

It judges only the rules the run actually carries, and stays quiet about a
comment that names no rule, since such a comment covers rules the run may not
have. A comment sitting inside the reach of a wider one is reported as well: with
the rule already off for the file, a `disable-line` naming it silences nothing of
its own.

## Troubleshooting

### Comments Not Working

1. Ensure comments are on their own line (except for `disable-line`)
2. Check that comments are not inside code blocks
3. Verify correct spelling of command and rule names
4. Make sure you're using either `rumdl` or `markdownlint` (for compatibility) prefix consistently

### Rules Still Triggering

1. Check if the rule is configured to ignore inline configuration (rare)
2. Verify the comment appears before the content it should affect
3. Ensure you're using the correct rule ID (check with `rumdl rule MD###`)

### Inline Enable Has No Effect

A rule that configuration leaves out cannot be re-enabled with an inline
comment. Rule selection is final: such a rule never runs, so an `enable`,
`enable-file`, or `configure-file` with `true` targeting it does nothing. rumdl
warns when this happens, naming the setting that left the rule out.

A rule disabled globally:

```text
[inline config warning] file.md:1: Rule MD012 is not enabled in configuration, so the inline enable comment enabling it has no effect
```

A rule that `per-file-ignores` excludes for this particular file:

```text
[inline config warning] CHANGELOG.md:1: Rule MD024 is ignored for this file by per-file-ignores, so the inline enable comment enabling it has no effect
```

To use the rule, enable it in your configuration (or drop it from the matching
`per-file-ignores` pattern) instead. Inline comments can only tighten
configuration (disable rules), not loosen it.

## See Also

- [Global Settings Reference](global-settings.md) - Configure rules globally
- [Rules Reference](rules.md) - Complete list of available rules
- [Configuration Guide](../README.md#configuration) - General configuration documentation
