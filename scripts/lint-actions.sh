#!/usr/bin/env bash
# Check repository-owned Actions files without scanning local worktrees or fixtures.
set -euo pipefail

files=()
while IFS= read -r -d '' file; do
  files+=("$file")
done < <(git ls-files -z -- \
  '.github/workflows/*.yml' '.github/workflows/*.yaml' \
  ':(glob)**/action.yml' ':(glob)**/action.yaml' \
  ':(glob)**/dependabot.yml' ':(glob)**/dependabot.yaml')

if [ "${#files[@]}" -eq 0 ]; then
  exit 0
fi

jactionlint --no-online "$@" "${files[@]}"
