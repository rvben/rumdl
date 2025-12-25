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

Add to `~/.config/helix/languages.toml`:

```toml
[language-server.rumdl]
command = "rumdl"
args = ["server"]

[[language]]
name = "markdown"
language-servers = ["rumdl"]
```

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

### Sublime Text (LSP package)

Add to LSP settings (`Preferences > Package Settings > LSP > Settings`):

```json
{
  "clients": {
    "rumdl": {
      "enabled": true,
      "command": ["rumdl", "server"],
      "selector": "text.html.markdown"
    }
  }
}
```

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
- [Rules reference](RULES.md)
- [VS Code extension](https://marketplace.visualstudio.com/items?itemName=rvben.rumdl)
