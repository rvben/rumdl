# Language server protocol (LSP)

rumdl includes a built-in LSP server for real-time Markdown linting in your editor.

## Starting the server

```bash
# Default: use stdio (for editor integration)
rumdl server

# With custom config
rumdl server --config .rumdl.toml

# Verbose logging (for debugging)
rumdl server --verbose

# TCP mode (for debugging)
rumdl server --port 9257
```

## Capabilities

The rumdl LSP server provides:

- **Diagnostics**: Real-time linting as you type
- **Code actions**: Quick fixes for auto-fixable issues
- **Document formatting**: Format entire document (`rumdl fmt`)
- **Range formatting**: Format selected text
- **Completion**: Language suggestions for fenced code blocks, plus file paths and heading anchors inside link targets
- **Link navigation**: Hover preview, go-to-definition, find-references, and rename for Markdown links

### Code block language completion

When typing a fenced code block, rumdl provides intelligent completions for language labels.
Type `` ```py `` and completions will appear for languages starting with "py" (Python, etc.).

The completion uses GitHub Linguist data (799+ languages) and respects your MD040 configuration:

```toml
[MD040]
# Only suggest these languages
allowed-languages = ["Python", "JavaScript", "Rust"]

# Or exclude specific languages
disallowed-languages = ["HTML"]

# Prefer specific aliases
preferred-aliases = { Python = "py", JavaScript = "js" }
```

Features:

- Triggers after `` ``` `` or `~~~` fence markers
- Supports extended fences (4+ backticks for nested blocks)
- Filters by `allowed-languages` and `disallowed-languages`
- Prioritizes `preferred-aliases` in results
- Shows canonical language name in completion details

### Link path and anchor completion

Inside a Markdown link target, rumdl suggests workspace file paths after `](`
and heading anchors after `#` (for example `](../guide.md#` lists the headings
in `guide.md`). This is driven by a workspace index of Markdown files and their
headings.

If you use another language server for link completion (for example a PKM/notes
LSP) and do not want rumdl's suggestions, disable it with the
`enableLinkCompletions` setting (see [LSP settings](#lsp-settings)). When
disabled, rumdl returns no link suggestions and does not register the
link-target trigger characters (`(`, `#`, `/`, `.`, `-`), so it is not invoked on
them; fenced code-block language completion still works. Linting, formatting,
and code actions are unaffected. The related navigation features (hover,
go-to-definition, references, rename) are controlled separately by
`enableLinkNavigation`.

## Editor configuration

### Neovim (nvim-lspconfig)

Add to your Neovim configuration:

```lua
-- if you do not use nvim-lspconfig, add this rumdl config
vim.lsp.config("rumdl", {
  cmd = { "rumdl", "server" },
  filetypes = { "markdown" },
  root_markers = { ".git" },
})

vim.lsp.enable("rumdl")
```

### Helix

Add to `languages.toml`:

```toml
[language-server.rumdl]
command = "rumdl"
args = ["server"]

[[language]]
name = "markdown"
language-servers = ["rumdl"]
formatter = { command = "rumdl", args = ["check", "--fix", "--stdin"] }
```

> **Note:** The `[[language]]` block replaces the Helix defaults. Add any other
> language servers you use (e.g., `marksman`) to the `language-servers` list.
> rumdl was merged into Helix's built-in config after the 25.07.1 release,
> so manual configuration will not be needed once the next Helix version ships.

### VS Code

Install the [rumdl VS Code extension](https://marketplace.visualstudio.com/items?itemName=rvben.rumdl) from the marketplace.

The extension automatically manages the LSP server.

### Zed

Add to your Zed settings:

```json
{
  "lsp": {
    "rumdl": {
      "binary": {
        "path": "rumdl",
        "arguments": ["server"]
      }
    }
  },
  "languages": {
    "Markdown": {
      "language_servers": ["rumdl"]
    }
  }
}
```

### Sublime Text

For information on configuring Sublime Text with rumdl as a language server, see the
[LSP for Sublime Text documentation](https://lsp.sublimetext.io/language_servers/#rumdl).

### Emacs (lsp-mode)

Add to your Emacs configuration:

```elisp
(with-eval-after-load 'lsp-mode
  (add-to-list 'lsp-language-id-configuration '(markdown-mode . "markdown"))
  (lsp-register-client
   (make-lsp-client
    :new-connection (lsp-stdio-connection '("rumdl" "server"))
    :major-modes '(markdown-mode)
    :server-id 'rumdl)))
```

### Emacs (eglot)

Add to your Emacs configuration:

```elisp
(with-eval-after-load 'eglot
  (add-to-list 'eglot-server-programs
               '(markdown-mode . ("rumdl" "server"))))
```

## Configuration

The LSP server uses the same configuration as the CLI. It automatically discovers `.rumdl.toml` or `pyproject.toml` in your project.

You can override the config path:

```bash
rumdl server --config /path/to/.rumdl.toml
```

Or use built-in defaults only:

```bash
rumdl server --no-config
```

### LSP settings

Beyond the config file, editors can pass settings to the server as LSP
initialization options (or `workspace/didChangeConfiguration`). These are
top-level keys in camelCase, following Ruff's LSP convention:

| Setting                        | Default  | Description                                                                                                                                 |
| ------------------------------ | -------- | ------------------------------------------------------------------------------------------------------------------------------------------- |
| `enableLinting`                | `true`   | Real-time diagnostics as you type                                                                                                           |
| `enableAutoFix`                | `false`  | Apply auto-fixes on save                                                                                                                    |
| `enableLinkCompletions`        | `true`   | File-path and heading-anchor completions inside link targets. Set to `false` to keep linting while letting another LSP own link completion. |
| `enableLinkNavigation`         | `true`   | Hover, go-to-definition, find-references, and rename for links. Set to `false` to avoid conflicts with another LSP that provides these.     |
| `linkCompletionContentRoots`   | `[]`     | Roots for absolute-style link completion (e.g. `/img/01.webp`); defaults to the workspace roots.                                            |
| `configPath`                   | (auto)   | Explicit path to a rumdl config file                                                                                                        |
| `disableRules` / `enableRules` | (config) | Override which rules run                                                                                                                    |

For example, to keep rumdl's linting but turn off its link completion and
navigation (so your own LSP owns those), in Neovim:

```lua
vim.lsp.config("rumdl", {
  cmd = { "rumdl", "server" },
  filetypes = { "markdown" },
  root_markers = { ".git" },
  init_options = {
    enableLinkCompletions = false,
    enableLinkNavigation = false,
  },
})
```

These keys are read from the server's initialization options, so any editor that
can pass `initializationOptions` to a language server can set them.

## Troubleshooting

### Enable verbose logging

```bash
rumdl server --verbose
```

This outputs detailed logs to stderr, which most editors capture in their LSP logs.

### Check server is working

Test the server manually:

```bash
echo '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"capabilities":{}}}' | rumdl server
```

You should see a JSON response with server capabilities.

### Common issues

**Diagnostics not appearing:**

- Ensure the file is recognized as Markdown (check file extension)
- Check that rumdl is in your PATH
- Look at your editor's LSP logs for errors

**Wrong config being used:**

- Use `--verbose` to see which config file is loaded
- Use `--config` to specify an explicit path
- Use `--no-config` to ignore all config files

## See also

- [Configuration guide](global-settings.md)
- [Rules reference](rules.md)
- [VS Code extension](https://marketplace.visualstudio.com/items?itemName=rvben.rumdl)
