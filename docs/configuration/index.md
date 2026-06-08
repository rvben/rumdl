# Configuration Files

rumdl discovers its configuration automatically - you don't have to point it at a
file. This page covers **which files rumdl reads, in what order, and how to tell
which one is active**.

Looking for something more specific?

- For the settings that go *inside* a config file, see
  [Global Settings](../global-settings.md).
- For per-document overrides via HTML comments, see
  [Inline Configuration](../inline-configuration.md).

## Supported config files

rumdl looks for these files, in this order of precedence:

| Priority | File                 | Format               | Notes                                                                                     |
| -------- | -------------------- | -------------------- | ----------------------------------------------------------------------------------------- |
| 1        | `.rumdl.toml`        | TOML, top level      | The default. Created by `rumdl init`.                                                     |
| 2        | `rumdl.toml`         | TOML, top level      | Same format, no leading dot - if you prefer a visible config file.                        |
| 3        | `.config/rumdl.toml` | TOML, top level      | Keeps the project root tidy ([config-dir convention](https://github.com/pi0/config-dir)). |
| 4        | `pyproject.toml`     | TOML, `[tool.rumdl]` | For Python projects. Only used if it contains a `[tool.rumdl]` section.                   |

At each directory level, rumdl uses the **first** file that exists and ignores the
rest. `.rumdl.toml` and `rumdl.toml` are identical in format; in `pyproject.toml`,
settings live under the `[tool.rumdl]` table.

### Which file should I use?

| You want...                          | Use                                                           |
| ------------------------------------ | ------------------------------------------------------------- |
| The simplest setup                   | `.rumdl.toml` (run `rumdl init`)                              |
| A visible, non-hidden config file    | `rumdl.toml`                                                  |
| To keep the project root uncluttered | `.config/rumdl.toml`                                          |
| One config file for a Python project | `[tool.rumdl]` in `pyproject.toml` (`rumdl init --pyproject`) |

All four are equivalent in capability - the choice is about where you want the file
to live.

## How rumdl finds your config

rumdl searches **upward** from the current directory - like `git`, `ruff`, and
`eslint` - so you can run it from any subdirectory and it still finds the config at
your project root.

- It walks up from the working directory, checking each level for the files above;
  the first match wins.
- It stops at the project root (a directory containing `.git`) or after 100 levels.
- When you lint a tree (for example `rumdl check .`), configuration is resolved
  **per directory** - a subdirectory with its own config uses that config. See
  [Per-Directory Configuration](../global-settings.md#per-directory-configuration).

To see exactly which file rumdl loaded:

```bash
rumdl config file
```

### When several configs exist in one directory

If one directory contains more than one rumdl config (for example both
`.rumdl.toml` and `rumdl.toml`), the higher-precedence file wins and rumdl prints a
warning so the shadowing is never silent:

```text
[config warning] multiple rumdl config files in /path/to/project: using .rumdl.toml, ignoring rumdl.toml
```

Remove the shadowed file (or consolidate into one) to clear the warning.

## User-level configuration

When **no project config is found**, rumdl falls back to a user-level config, so
you can set personal defaults that apply across all your projects:

- Your platform's rumdl config directory - `~/.config/rumdl/` on Linux and macOS,
  `%APPDATA%\rumdl\` on Windows - checked as `.rumdl.toml`, then `rumdl.toml`, then
  `pyproject.toml`.
- Home-directory dotfiles: `~/.rumdl.toml`, then `~/rumdl.toml`.

A project config always takes precedence over user-level config.

## markdownlint compatibility

If you don't have a rumdl config but **do** have a
[markdownlint](https://github.com/DavidAnson/markdownlint) config
(`.markdownlint.json`, `.markdownlint.yaml`, `.markdownlint-cli2.yaml`, and
similar), rumdl discovers and applies it automatically through the same upward
search - so existing markdownlint setups work without conversion. Settings that the
markdownlint format can't express (such as `flavor`) fall back to rumdl's defaults
or your user-level config.

See [markdownlint Comparison](../markdownlint-comparison.md) for the mapping
between markdownlint and rumdl options.

## Turning discovery off

```bash
# Use one explicit config file for every linted file (disables per-directory resolution)
rumdl check --config path/to/rumdl.toml .

# Ignore all config files and use built-in defaults only
rumdl check --no-config .
```

## See also

- [Global Settings](../global-settings.md) - every setting you can put in `[global]`
- [Inline Configuration](../inline-configuration.md) - per-document overrides via HTML comments
- [Markdown Flavors](../flavors.md) - tune rumdl for GFM, MkDocs, MDX, and more
