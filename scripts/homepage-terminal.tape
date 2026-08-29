Require uvx

# Run from the repository root with: vhs scripts/homepage-terminal.tape

Set Shell "bash"
Set FontSize 24
Set FontFamily "Geist Mono"
Set Width 1120
Set Height 480
Set LineHeight 1.2
Set LetterSpacing 0
Set Theme "Catppuccin Mocha"
Set Padding 34
Set MarginFill "#0b0c10"
Set Margin 26
Set BorderRadius 16
Set WindowBar "Rings"
Set WindowBarSize 44
Set TypingSpeed 16ms

Hide
Type `export RUMDL_CAPTURE_ROOT="$PWD"; export RUMDL_CAPTURE_DEMO="${TMPDIR:-/tmp}/rumdl-homepage-demo"; rm -rf "$RUMDL_CAPTURE_DEMO"; mkdir -p "$RUMDL_CAPTURE_DEMO/docs"`
Enter
Type `printf '#Welcome to rumdl\n\nFast docs.\n' > "$RUMDL_CAPTURE_DEMO/README.md"`
Enter
Type `printf '# Guide\n\nThis line is deliberately longer than eighty characters so rumdl can show its precise diagnostics in this terminal session.\n' > "$RUMDL_CAPTURE_DEMO/docs/guide.md"`
Enter
Type `cd "$RUMDL_CAPTURE_DEMO" && export PS1='❯ ' && clear`
Enter
Show

Type "uvx rumdl check ."
Enter
Wait+Screen@30s /Found 2 issues/
Sleep 400ms
Screenshot "homepage-terminal.png"

Hide
Type `cp homepage-terminal.png "$RUMDL_CAPTURE_ROOT/docs/images/homepage-terminal.png"; cd "$RUMDL_CAPTURE_ROOT"; rm -rf "$RUMDL_CAPTURE_DEMO"`
Enter
