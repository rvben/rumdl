---
description: "Which config files rumdl reads, the order of precedence between them, and how to tell which one is actually active for a given file."
---

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
- It does not walk up into your home directory: a config file sitting in `$HOME`
  itself is treated as [user-level configuration](#user-level-configuration), not as
  the project config of everything under your home. The working directory itself is
  always checked, though - even when it *is* `$HOME` (some CI services, such as
  pre-commit.ci, run with `HOME` set to the checkout directory).
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

1. Your platform's rumdl config directory - `~/.config/rumdl/` on Linux and macOS
   (or `$XDG_CONFIG_HOME/rumdl/` when set), `%APPDATA%\rumdl\` on Windows - checked
   as `.rumdl.toml`, then `rumdl.toml`, then `pyproject.toml` (which must contain a
   `[tool.rumdl]` section).
2. **Windows only:** `~\.config\rumdl\`, with the same three names in the same
   order. This is the location most cross-platform tools use for XDG-style
   configuration.
3. Home-directory dotfiles: `~/.rumdl.toml`, then `~/rumdl.toml`.
4. **Windows only, when `HOME` names a different directory than your user
   profile:** the same locations under the profile directory, in the same order:
   `%USERPROFILE%\.config\rumdl\` (three names), then `%USERPROFILE%\.rumdl.toml`,
   then `%USERPROFILE%\rumdl.toml`. A config that rumdl found in the profile
   before it honored `HOME` therefore keeps applying; it only loses to a file in
   one of the earlier locations. The tier is skipped when `HOME` and the profile
   are the same directory (compared case-insensitively, ignoring `/` versus `\`
   and trailing separators). While this tier applies, the profile directory is
   a home directory for the project search too: the search does not climb into
   it, so `%USERPROFILE%\.rumdl.toml` is only ever a user-level config and ranks
   last. A project below the profile (`%USERPROFILE%\proj\.rumdl.toml`) is still
   found normally.

The first file found wins; the others are not merged. On Linux and macOS
`~/.config/rumdl/` is already step 1, so steps 2 and 4 do not exist there.

A project config always takes precedence over user-level config, and when one is
found the user-level search is skipped entirely. A markdownlint config found in the
project is applied on top of the user-level config, so settings the markdownlint
file does not express still come from your user config. `--config` uses only the
file you name, and `--no-config` / `--isolated` use the built-in defaults.

### What `~` means

On Linux and macOS `~` is your home directory (`HOME`, then the user database).

On Windows rumdl uses `HOME` when it is set to a native Windows path: a drive path
(`C:\Users\me` or `C:/Users/me`), a UNC path (`\\server\share\me`) or a verbatim
path (`\\?\C:\Users\me`). Otherwise it uses your user profile directory
(`USERPROFILE`). A `HOME` that is empty, relative, drive-relative (`C:foo`) or in
MSYS style (`/c/Users/me`, as set by Git Bash and Cygwin) is ignored and rumdl
falls back to the profile directory. `--explain` (below) says when this happens.
The same home is used for the user-level config, `~/` in `extends` and path
patterns, and the boundary the project search does not climb past. Only the
user-level config search additionally looks in the profile directory (step 4
above) and the project search stops before it; `~` never means the profile
once `HOME` is honored. `--explain` reports which of the two the search reached
("home directory" or "profile directory (fallback home)").

`%APPDATA%` is independent of `HOME`, and `XDG_CONFIG_HOME` is not consulted on
Windows.

### Finding out which config was used

`rumdl config file` prints the path of the config in effect. `rumdl config file
--explain` prints where rumdl started, which home directory it used and where that
came from, every file it looked for and what it found, why the upward search
stopped, which file was selected and which lower-priority files it shadows. It
never prints file contents. Locations that belong to the profile fallback tier are
labeled `profile fallback` in the text output and carry `"profile_fallback": true`
in the JSON, so you can tell when a file was found there and why.
Add `--output json` for a machine-readable form.
A config file that cannot be parsed is reported as an error, as for every other
command, and no trace is printed for that run.

PowerShell:

```powershell
rumdl config file --explain
```

Git Bash (note that `HOME` here is an MSYS path, which rumdl reports and ignores):

```bash
rumdl config file --explain
```

```text
start directory: C:\Users\me\project
ignored HOME="/c/Users/me": it is an MSYS-style path (/c/Users/...), not a native Windows path
home directory: C:\Users\me (from USERPROFILE or the user profile API)
...
selected user config: C:\Users\me\.config\rumdl\rumdl.toml
```

Running with `RUST_LOG=debug` logs the same decisions.

## markdownlint compatibility

If you don't have a rumdl config but **do** have a
[markdownlint](https://github.com/DavidAnson/markdownlint) config
(`.markdownlint.json`, `.markdownlint.yaml`, `.markdownlint-cli2.yaml`, and
similar), rumdl discovers and applies it automatically through the same upward
search - so existing markdownlint setups work without conversion. Settings that the
markdownlint format can't express (such as `flavor`) fall back to rumdl's defaults
or your user-level config.

Nested CLI2 configs merge rule maps within the invocation directory (or the LSP
workspace root). Each nearer rule entry replaces its parent's whole entry.
Plain `.markdownlint.*` files replace rule settings and win over CLI2 `config`
in the same directory. CLI2 `ignores` still apply, including to explicitly named
files and formatting; `--no-exclude` disables that filtering.

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
