---
icon: lucide/code
---

# Editor Integration

Integrate rumdl with your favorite editor for real-time linting.

## VS Code / Cursor / Windsurf

The easiest way to integrate rumdl:

```bash
rumdl vscode
```

This automatically detects your editor and installs the appropriate extension.

**Features:**

- Real-time linting as you type
- Quick fixes for common issues
- Code formatting on save
- Hover tooltips with rule documentation

See [VS Code Integration](../integrations/vscode.md) for detailed setup.

## Neovim

### Using nvim-lspconfig

```lua
require('lspconfig').rumdl.setup{}
```

### Using null-ls / none-ls

```lua
local null_ls = require("null-ls")
null_ls.setup({
  sources = {
    null_ls.builtins.diagnostics.rumdl,
    null_ls.builtins.formatting.rumdl,
  },
})
```

### Manual LSP Setup

```lua
vim.api.nvim_create_autocmd("FileType", {
  pattern = "markdown",
  callback = function()
    vim.lsp.start({
      name = "rumdl",
      cmd = { "rumdl", "server" },
      root_dir = vim.fs.dirname(vim.fs.find({".rumdl.toml", ".git"}, { upward = true })[1]),
    })
  end,
})
```

## Vim

### Using ALE

```vim
" .vimrc
let g:ale_linters = {'markdown': ['rumdl']}
let g:ale_fixers = {'markdown': ['rumdl']}
```

### Format Selection

```vim
" Format selection
:'<,'>!rumdl fmt - --quiet

" Format entire buffer
:%!rumdl fmt - --quiet
```

## Helix

Add to `languages.toml`:

```toml
[[language]]
name = "markdown"
language-servers = ["rumdl"]

[language-server.rumdl]
command = "rumdl"
args = ["server"]
```

## Zed

Add to `settings.json`:

```json
{
  "languages": {
    "Markdown": {
      "language_servers": ["rumdl"]
    }
  },
  "lsp": {
    "rumdl": {
      "binary": {
        "path": "rumdl",
        "arguments": ["server"]
      }
    }
  }
}
```

## Sublime Text

Using LSP package:

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

## Generic LSP

Any editor supporting LSP can use rumdl:

```bash
rumdl server
```

The server communicates via stdin/stdout using the Language Server Protocol.

## Format on Save

Most editors can be configured to format on save using rumdl's stdin/stdout mode:

```bash
rumdl fmt - --quiet
```

This reads from stdin and outputs formatted content to stdout.
