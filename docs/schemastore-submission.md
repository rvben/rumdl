# SchemaStore Submission Guide

This document contains the information needed to submit rumdl's JSON schema to SchemaStore.

## Schema Location

The schema is hosted at:

```text
https://raw.githubusercontent.com/rvben/rumdl/main/rumdl.schema.json
```

## Catalog Entry

Add this entry to `src/api/json/catalog.json` in the SchemaStore repository:

```json
{
  "name": "rumdl",
  "description": "Configuration file for rumdl, a fast Markdown linter and formatter",
  "fileMatch": [".rumdl.toml", "rumdl.toml"],
  "url": "https://raw.githubusercontent.com/rvben/rumdl/main/rumdl.schema.json"
}
```

## Test Files

Create a directory `src/test/rumdl/` with valid test files.

**Example test file** (`src/test/rumdl/.rumdl.toml`):

```toml
[global]
disable = ["MD013", "MD033"]
exclude = [".git", "node_modules"]
respect_gitignore = true

[per-file-ignores]
"README.md" = ["MD033"]

[MD013]
line_length = 100
code_blocks = false
tables = false
```

You can copy from `rumdl.toml.example` for additional test cases.

## Submission Steps

1. Fork <https://github.com/SchemaStore/schemastore>
2. Add the catalog entry to `src/api/json/catalog.json` (alphabetically sorted by name)
3. Create test directory and files at `src/test/rumdl/`
4. Run validation tests (see SchemaStore CONTRIBUTING.md)
5. Create pull request

## Benefits

Once merged, editors like VS Code, IntelliJ IDEA, and others will automatically:

- Associate the schema with `.rumdl.toml` and `rumdl.toml` files
- Provide autocomplete for configuration options
- Validate configuration syntax
- Show inline documentation on hover

## Maintenance

The schema is automatically generated from Rust code and kept in sync via:

- `rumdl schema generate` or `make schema` - Generate/update schema
- `rumdl schema check` or `make check-schema` - Verify schema is current (runs in CI)
- `rumdl schema print` - Print schema to stdout

When configuration structures change in `src/config.rs`, regenerate the schema before releasing.
