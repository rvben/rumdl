---
description: "Find images and other asset files that no Markdown link, image, HTML attribute, or filename mention in your sources refers to, with rumdl check-assets."
---

# Find unreferenced assets

`rumdl check-assets` reports files that have neither a parsed Markdown
reference nor a literal filename mention in the selected source directories.
It does not read CSS, themes, site configuration, or generated pages. Use
`--keep` for assets used by those files. The command never deletes or modifies
files.

```bash
rumdl check-assets --root docs --source docs --assets docs/images
```

Three options say where to look:

- `--root` is the URL root: a leading-slash link such as `/images/logo.png`
  resolves from this directory, and reported paths are relative to it. With
  `--root docs`, `/images/logo.png` names `docs/images/logo.png`.
- `--source` is a directory of Markdown files to read.
- `--assets` is a directory of files to report on. It may be the same
  directory as a `--source`.

Use complete source directories, including any other directories that
reference these assets. `--source` and `--assets` can each be repeated. All
directories must be within `--root`. Directory arguments are relative to the
current working directory. Sites with other URL mappings need keep patterns.

When assets are stored next to the Markdown that uses them, give the same
directory to all three options:

```bash
rumdl check-assets --root docs --source docs --assets docs
```

The command scans every Markdown file recursively, including hidden and
ignored files. It uses rumdl's Markdown extension list, shown in the report.
Only Markdown sources are decoded as UTF-8; binary assets are never decoded.
Overlapping scan roots are deduplicated. Markdown files found in an asset tree
must also be covered by a `--source` directory; otherwise the scan stops and
asks you to add that directory. Empty source scans, unreadable
sources or directories, invalid UTF-8 sources, symlinks, and special files
stop the scan with exit code 2 and no findings. Symlinks encountered in either
scan tree are unsupported. A symlinked ancestor of an explicit directory
argument is resolved before scanning.

## References and mentions

The scan recognizes standard Markdown links, images, and reference definitions
(including unused definitions). Each source is read in the Markdown flavor
`rumdl check` would use for it, so images inside MkDocs admonitions and content
tabs, MyST and Pandoc blocks, and MDX components are found. The flavor comes
from `--flavor`, the configuration's `flavor` and `per-file-flavor`, and the
file extension, in the same order as `rumdl check`. Configuration is read for
the flavor only; lint rules, exclusions, and other settings do not affect the
scan. `--config`, `--no-config`, and `--isolated` behave as they do for
`rumdl check`, and a configuration that cannot be loaded stops the scan with
exit code 2. In flavors without these block syntaxes, an indented block is
code and its images are not references. The scan also reads HTML `href`, `src`, `poster`,
`data`, `data-src`, and `srcset` attributes, including multiline tags.
Relative paths resolve against the source file's directory; query strings and
fragments are removed before percent-decoding. External URLs are ignored.
Filesystem case sensitivity applies, so results can differ between operating
systems.

The scan does not render Markdown extensions, template expressions, MDX
imports, wiki links, or shortcodes. An asset whose complete filename occurs literally
in a source, or in its uppercase percent-encoded spelling, is kept in a
separate `mentioned_but_unresolved` group. This includes mentions in code and
frontmatter. A mention counts only at filename boundaries: `logo.png` is
mentioned by `"logo.png"`, `(logo.png)`, `images/logo.png`, `logo.png?v=1`, and
a sentence ending in `logo.png.`, but not by `readme_logo.png`, `my-logo.png`,
or `logo.png.bak`. A mention is conservative evidence, so it can hide an actually
unreferenced file; it is never presented as a resolved link. Assets with the
same filename can all be kept by one mention. Very short or extensionless
filenames can match ordinary prose, making this fallback especially broad.
Mention encoding covers
non-ASCII, controls, spaces, `#`, `?`, `%`, `"`, `<`, and `>`. Other escapes
and lowercase hex must resolve as parsed references to be kept. Choose narrow
source directories: scanning a whole repository can encounter symlinks in
dependency or build trees.

## Select assets and output

By default every non-Markdown file in the asset directories is considered,
including hidden files. `--include` and `--keep` accept repeatable,
root-relative globs. `/` separates path components; `**` matches across
directories. Glob metacharacters in literal filenames need character classes,
for example `[[]` to match `[`. Use keep patterns for tooling files such as
`--keep '**/.DS_Store' --keep '**/.gitkeep'` when needed.

```bash
rumdl check-assets --root docs --source docs --assets docs \
  --include '**/*.{png,jpg,svg}' --keep 'images/theme/**' \
  --output-format json
```

JSON contains `schema_version`, `source_extensions`, `sources_scanned`,
`assets_considered`, `unreferenced`, and `mentioned_but_unresolved`. Paths use
`/` separators and are sorted lexically. The asset count excludes files
filtered by include and keep patterns. A glob matching nothing is clean. Exit
code 0 means no unreferenced assets, 1 means findings, and 2 means an
incomplete or invalid scan. Mentions alone do not fail the command.

This is an explicit project scan: it has no cache, per-file lint invocation,
stdin, watch, format, or language-server mode. Its source coverage does not
depend on lint rule selection, exclusions, or whether MD057 is enabled.
