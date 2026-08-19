# rumdl-wasm

Fast markdown linter with 60+ rules, compiled to WebAssembly.

## Installation

```bash
npm install rumdl-wasm
```

## Quick Start

```javascript
import init, { lint_markdown, apply_all_fixes } from 'rumdl-wasm';

// Initialize the WASM module
await init();

// Lint markdown content
const content = '# Hello World\n\nSome text here...';
const warnings = JSON.parse(lint_markdown(content));

// Apply all auto-fixes
const fixed = apply_all_fixes(content);
```

## API Reference

### `init()`

Initialize the WASM module. Must be called before using other functions.

```javascript
await init();
```

### `lint_markdown(content: string): string`

Lint markdown content and return warnings as JSON.

```javascript
const warnings = JSON.parse(lint_markdown(content));
```

Returns a JSON array of warnings (see Warning Format below).

### `apply_all_fixes(content: string): string`

Apply all available auto-fixes to the content.

```javascript
const fixed = apply_all_fixes(content);
```

Returns the fixed content string.

### `apply_fix(content: string, fix_json: string): string`

Apply a single fix to the content.

```javascript
const fix = JSON.stringify({ start: 0, end: 5, replacement: 'Hello' });
const fixed = apply_fix(content, fix);
```

### `get_version(): string`

Get the rumdl version.

```javascript
const version = get_version(); // e.g., "0.0.185"
```

### `get_available_rules(): string`

Get list of available rules as JSON.

```javascript
const rules = JSON.parse(get_available_rules());
// [{ name: "MD001", description: "Heading levels should only increment by one level at a time" }, ...]
```

## Configuration

`lint_markdown` and `apply_all_fixes` use rumdl's defaults. For configured
linting, create a `Linter` with an options object that mirrors the `[global]`
section of `.rumdl.toml` plus per-rule tables:

```javascript
import init, { Linter } from 'rumdl-wasm';

await init();

const linter = new Linter({
  disable: ['MD041'],
  'line-length': 120,
  flavor: 'mkdocs',            // "standard" (default), "mkdocs", "obsidian", ...
  MD013: { 'line-length': 100 },
});

const warnings = JSON.parse(linter.check(content, 'docs/guide.md'));  // path is optional
const fixed = linter.fix(content, 'docs/guide.md');

JSON.parse(linter.get_config());           // the effective configuration
JSON.parse(linter.get_config_warnings());  // e.g. unknown rule options
```

Passing a path lets `exclude` and `per-file-ignores` patterns apply.

### Loading `.rumdl.toml` files, including `extends`

A flat options object cannot follow `extends`, so `new Linter(...)` reports an
`extends` key as a config warning and ignores it. To load config files the way
the CLI does (same parsing, `extends` resolution, and merge order), hand rumdl
the file contents and let it tell you which file it needs next. This works
from any host that can read files, including ones that can only do so
asynchronously:

```javascript
import init, { Linter, resolve_config_chain } from 'rumdl-wasm';
import { readFile } from 'fs/promises';

await init();

const root = '.rumdl.toml';
const files = { [root]: await readFile(root, 'utf-8') };
const request = () => ({ root, files, env: process.env, home: process.env.HOME });

for (;;) {
  const r = JSON.parse(resolve_config_chain(request()));
  if (r.status === 'need-file') {
    // r.path is resolved already: relative to the declaring file's directory,
    // with `$VAR` expanded from `env` and `~/` from `home`. Store null for a
    // file that does not exist.
    files[r.path] = await readFile(r.path, 'utf-8').catch(() => null);
  } else if (r.status === 'error') {
    throw new Error(r.message);   // parse error, missing target, cycle, ...
  } else {
    console.log('config chain:', r.files);   // root first
    break;
  }
}

const linter = Linter.from_config_files({ ...request(), 'default-flavor': 'mkdocs' });
```

`root` may also be a `pyproject.toml` (its `[tool.rumdl]` table is used).
`default-flavor` applies only when no file in the chain sets a flavor.

## Warning Format

Each warning object contains:

```typescript
interface Warning {
  rule: string;        // Rule name (e.g., "MD001")
  message: string;     // Warning message
  line: number;        // 1-indexed line number
  column: number;      // 1-indexed column number
  end_line: number;    // 1-indexed end line
  end_column: number;  // 1-indexed end column
  severity: string;    // "Error" or "Warning"
  fix?: {              // Optional auto-fix
    start: number;     // Byte offset start
    end: number;       // Byte offset end
    replacement: string;
  };
}
```

## Browser Usage

### ES Module

```html
<script type="module">
  import init, { lint_markdown } from './rumdl_wasm.js';

  async function main() {
    await init();
    const warnings = JSON.parse(lint_markdown('# Test'));
    console.log(warnings);
  }

  main();
</script>
```

### With CDN (unpkg)

```html
<script type="module">
  import init, { lint_markdown } from 'https://unpkg.com/rumdl-wasm/rumdl_wasm.js';

  await init();
  console.log(JSON.parse(lint_markdown('# Test')));
</script>
```

## Node.js Usage

```javascript
import init, { lint_markdown, apply_all_fixes } from 'rumdl-wasm';
import { readFile } from 'fs/promises';

await init();

const content = await readFile('README.md', 'utf-8');
const warnings = JSON.parse(lint_markdown(content));

for (const w of warnings) {
  console.log(`${w.line}:${w.column} ${w.rule} ${w.message}`);
}
```

## Bundler Usage

### Vite

```javascript
// vite.config.js
export default {
  optimizeDeps: {
    exclude: ['rumdl-wasm']
  }
};
```

```javascript
// main.js
import init, { lint_markdown } from 'rumdl-wasm';

await init();
const warnings = JSON.parse(lint_markdown(content));
```

### Webpack 5

```javascript
// webpack.config.js
module.exports = {
  experiments: {
    asyncWebAssembly: true
  }
};
```

## TypeScript

TypeScript definitions are included. The package exports all functions with proper types.

```typescript
import init, { lint_markdown, apply_all_fixes, get_version } from 'rumdl-wasm';

await init();

const warnings: Warning[] = JSON.parse(lint_markdown(content));
const fixed: string = apply_all_fixes(content);
const version: string = get_version();
```

## License

MIT
