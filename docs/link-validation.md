# Link and Nav Validation

rumdl provides offline-only validation for documentation links and MkDocs navigation
entries. All validation is local filesystem only — rumdl never makes network requests.

This guide covers the three rules that work together:

| Rule              | Purpose                                    | Default                                 |
| ----------------- | ------------------------------------------ | --------------------------------------- |
| [MD057](md057.md) | Validate relative file links exist         | Enabled                                 |
| [MD051](md051.md) | Validate anchor fragments resolve          | Enabled                                 |
| [MD074](md074.md) | Validate MkDocs `mkdocs.yml` nav entries   | Enabled (requires `flavor = "mkdocs"`)  |

## How the Rules Work Together

MD057 and MD051 cooperate on links that contain both a file path and a fragment:

```markdown
[Guide](guide.md#installation)
```

- **MD057** checks that `guide.md` exists on disk.
- **MD051** checks that `#installation` matches a heading in `guide.md`.

This split means you get clear, separate diagnostics for missing files vs. missing headings.

## Link Validation

### Relative links (MD057)

MD057 checks that relative links in your markdown files point to files that actually
exist. This catches broken links from typos, moved files, or deleted documents.

```markdown
[Installation Guide](install.md)          <!-- Validated: does install.md exist? -->
[Contributing](../CONTRIBUTING.md)         <!-- Validated: does ../CONTRIBUTING.md exist? -->
[GitHub](https://github.com/org/repo)      <!-- Skipped: external URL -->
[Email](mailto:help@example.com)           <!-- Skipped: non-file scheme -->
```

**What gets validated:**

- Inline links: `[text](path.md)`
- Reference-style links: `[ref]: path.md`
- Links with fragments: `[text](path.md#section)` (MD057 checks file, MD051 checks fragment)

**What gets skipped:**

- External URLs (`http://`, `https://`)
- Special schemes (`mailto:`, `tel:`, `javascript:`)
- Same-document anchors (`#section`) — handled by MD051

### Absolute links (MD057)

By default, absolute links like `/api/reference` are ignored because they typically
represent routes on the published site, not filesystem paths.

You can change this behavior:

```toml
[MD057]
absolute-links = "ignore"            # Default: skip validation
absolute-links = "warn"              # Report a warning
absolute-links = "relative_to_docs"  # MkDocs: resolve relative to docs_dir
```

The `relative_to_docs` option is useful for MkDocs projects where `/getting-started/`
should resolve to `docs/getting-started/index.md`:

```toml
[global]
flavor = "mkdocs"

[MD057]
absolute-links = "relative_to_docs"
```

### Anchor links (MD051)

MD051 checks that fragment links point to existing headings. This works for both
same-document anchors and cross-file fragments:

```markdown
# Introduction

## Getting Started

[Back to top](#introduction)                <!-- Same-file: validated -->
[Next section](#getting-started)            <!-- Same-file: validated -->
[See install guide](install.md#quickstart)  <!-- Cross-file: validated -->
[See FAQ](#faq)                             <!-- Invalid: no FAQ heading -->
```

Cross-file fragment validation requires linting multiple files together (the default
when running `rumdl check` on a directory or workspace).

MD051 supports multiple anchor generation styles to match your Markdown processor:

| Style                        | Use with              |
| ---------------------------- | --------------------- |
| `github` (default)           | GitHub, GitLab        |
| `python-markdown` / `mkdocs` | MkDocs                |
| `kramdown`                   | Jekyll (kramdown)     |
| `kramdown-gfm` / `jekyll`    | Jekyll with GFM input |

When using `--flavor mkdocs`, the anchor style automatically defaults to
`python-markdown`.

```toml
[MD051]
anchor-style = "python-markdown"
```

See the [MD051 documentation](md051.md) for detailed anchor style differences.

## Nav Validation (MkDocs)

MD074 validates the `nav` section of your `mkdocs.yml`. It only runs when the flavor
is set to `mkdocs`. rumdl finds `mkdocs.yml` by walking up from the file being checked.

```toml
[global]
flavor = "mkdocs"

[MD074]
not-found = "warn"       # Nav entries pointing to missing files (default: warn)
omitted-files = "ignore" # Files in docs_dir not in nav (default: ignore)
absolute-links = "ignore" # Absolute paths in nav entries (default: ignore)
```

### `not-found`

Detects nav entries that reference files which don't exist in `docs_dir`.

```yaml
# mkdocs.yml
nav:
  - Home: index.md
  - Guide: missing-guide.md  # Warning: file not found
```

### `omitted-files`

Detects markdown files under `docs_dir` that exist but aren't referenced in the nav.
Useful for catching orphaned pages.

```toml
[MD074]
omitted-files = "warn"
```

### `absolute-links`

Detects absolute paths (starting with `/`) in nav entries.

## Controlling Severity

You can control whether link validation warnings fail your build using per-rule severity
overrides and the `--fail-on` flag:

```toml
[MD057]
severity = "info"        # Report broken links but don't fail build

[MD051]
severity = "warning"     # Fragment validation fails the build

[MD074]
severity = "info"        # Nav issues are informational only
```

```bash
rumdl check --fail-on warning   # Only fail on warning+ severity (default: any)
```

Available `--fail-on` values: `any` (default), `warning`, `error`, `never`.

## Handling Special Cases

### Build-generated files

Documentation generators compile `.md` to `.html` during build. MD057 automatically
checks for a corresponding markdown source when a link points to `.html` or `.htm`:

```markdown
[Guide](guide.html)   <!-- Passes if guide.md exists -->
```

### Documentation generators with non-standard layouts

For generators that place source files in different locations (mdBook, Jekyll, Hugo),
use `per-file-ignores`:

```toml
[per-file-ignores]
"book/**/*.md" = ["MD057"]       # mdBook
"_posts/**/*.md" = ["MD057"]     # Jekyll
"content/**/*.md" = ["MD057"]    # Hugo
```

### Dynamically generated files (mkdocs-gen-files, macros)

Some MkDocs plugins like `mkdocs-gen-files` or `mkdocs-macros` create markdown files at
build time that don't exist in the source tree. Links to these files will be flagged by
MD057 since the targets don't exist during linting.

Options for handling this:

**Per-file ignores** — disable MD057 for files that link to generated content:

```toml
[per-file-ignores]
"docs/reference/**/*.md" = ["MD057"]
```

**Inline suppression** — disable for a section of links:

```markdown
<!-- rumdl-disable MD057 -->
[Generated API Reference](api/generated-ref.md)
[Another Generated Page](api/models.md)
<!-- rumdl-enable MD057 -->
```

**File-level suppression** — for files that primarily link to generated content:

```markdown
<!-- rumdl-disable-file MD057 -->
```

See the [Inline Configuration](inline-configuration.md) guide for all suppression
options.

## Full Configuration Example

```toml
# .rumdl.toml
[global]
flavor = "mkdocs"

# Relative link validation
[MD057]
absolute-links = "relative_to_docs"
severity = "warning"

# Anchor validation (auto-detects python-markdown style with mkdocs flavor)
[MD051]
severity = "warning"

# Nav validation
[MD074]
not-found = "warn"
omitted-files = "warn"
absolute-links = "warn"

# Exclude generated content from link checks
[per-file-ignores]
"docs/generated/**/*.md" = ["MD057"]
```

## Related Documentation

- [MD057 — Relative link validation](md057.md)
- [MD051 — Anchor link validation](md051.md)
- [MD074 — MkDocs nav validation](md074.md)
- [Inline Configuration](inline-configuration.md) — Suppress rules per-line or per-file
- [Global Settings](global-settings.md) — Project-wide configuration
- [MkDocs Flavor](flavors/mkdocs.md) — MkDocs-specific features
