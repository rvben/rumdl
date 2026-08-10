#!/bin/sh
# Reject a conventional-commit scope that names a rule in lowercase.
#
# vership copies the scope verbatim into CHANGELOG.md, so `fix(md077):` renders
# as **md077** beside the established **MD077** entries. Scopes that are not
# rule names are unaffected and stay lowercase.
set -eu

msg_file="${1:?usage: check-commit-scope.sh <commit-msg-file>}"

# The subject is the first line that is not a scissors/comment line.
subject=$(grep -v '^#' "$msg_file" | sed -n '1p')

# Generated subjects carry no authored scope.
case "$subject" in
    Merge\ * | Revert\ * | fixup!* | squash!* | amend!*) exit 0 ;;
esac

scope=$(printf '%s\n' "$subject" |
    sed -n 's/^[a-zA-Z][a-zA-Z]*(\([^)]*\))!\{0,1\}:.*/\1/p')
[ -n "$scope" ] || exit 0

# A scope may list several names. Keep the rule-shaped tokens that are not
# already the uppercase spelling.
wrong=$(printf '%s\n' "$scope" |
    tr -c '[:alnum:]' '\n' |
    grep -E '^[Mm][Dd][0-9]{3}$' |
    grep -Ev '^MD[0-9]{3}$' |
    tr '\n' ' ' || true)
[ -n "$wrong" ] || exit 0

correct=$(printf '%s\n' "$wrong" | tr '[:lower:]' '[:upper:]' | sed 's/ *$//')

cat >&2 <<EOF
Commit scope must name rules in uppercase.

  subject: $subject
  found:  $(printf '%s' "$wrong" | sed 's/ *$//')
  use:    $correct

vership copies the scope verbatim into CHANGELOG.md, so a lowercase scope
renders as **md077** beside 571 established **MD###** entries. Non-rule
scopes (ci, config, lsp, tables) stay lowercase.
EOF
exit 1
