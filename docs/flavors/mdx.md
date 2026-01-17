# MDX Flavor

For projects using [MDX](https://mdxjs.com/) (Markdown with JSX).

## Supported Patterns

### JSX Components

Capitalized tags are treated as JSX components, not HTML:

```markdown
<Button onClick={handleClick}>Click me</Button>

<Card title="Example">
  Content inside component.
</Card>

<MyCustomComponent prop={value} />
```

**Affected rules**: MD033 (inline HTML)

### JSX Attributes

Elements with JSX-specific attributes are recognized as JSX, not HTML:

```markdown
<!-- These use JSX attributes and are not flagged -->
<div className="container">...</div>
<label htmlFor="input-id">Label</label>
<button onClick={handler}>Click</button>
<input onChange={handleChange} />
```

JSX-specific attributes include: `className`, `htmlFor`, `onClick`, `onChange`, `onSubmit`, `dangerouslySetInnerHTML`, and other camelCase event handlers.

**Affected rules**: MD033 (inline HTML)

### JSX Expressions

JSX expressions and comments are recognized:

```markdown
The value is {computedValue}.

{/* This is a JSX comment */}

<Component prop={expression} />
```

**Affected rules**: MD037 (no space in emphasis), MD039 (no space in links), MD044 (proper names), MD049 (emphasis style)

### ESM Imports/Exports

MDX 2.0+ supports ESM anywhere in the document:

```markdown
import { Component } from './component'
export const metadata = { title: 'Page' }

# Heading

<Component />
```

**Affected rules**: MD013 (line length - ESM lines can be longer)

## Rule Behavior Changes

| Rule  | Standard Behavior      | MDX Behavior                            |
| ----- | ---------------------- | --------------------------------------- |
| MD013 | Check all line lengths | Allow longer ESM import/export lines    |
| MD033 | Flag all inline HTML   | Allow JSX components and JSX attributes |
| MD037 | Check emphasis spacing | Skip JSX expressions                    |
| MD039 | Check link spacing     | Skip JSX expressions                    |
| MD044 | Check proper names     | Skip inside JSX expressions             |
| MD049 | Check emphasis style   | Skip JSX expressions                    |

## Limitations

- Complex nested JSX expressions may not be fully parsed
- MDX compile-time expressions are treated as runtime expressions

## Configuration

```toml
[global]
flavor = "mdx"
```

Or auto-detect by file extension:

```toml
[per-file-flavor]
"**/*.mdx" = "mdx"
```

Note: `.mdx` files are auto-detected as MDX flavor by default.

## When to Use

Use the MDX flavor when:

- Building sites with Docusaurus
- Using Next.js with MDX
- Writing interactive documentation with React components
- Using any MDX-based framework

## See Also

- [Flavors Overview](../flavors.md) - Compare all flavors
- [MDX Documentation](https://mdxjs.com/)
