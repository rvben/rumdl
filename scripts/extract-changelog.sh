#!/bin/bash

# Extract release notes for a specific version from CHANGELOG.md

VERSION=${1:-$(git describe --tags --abbrev=0 | sed 's/^v//')}

if [ ! -f CHANGELOG.md ]; then
    echo "CHANGELOG.md not found"
    exit 1
fi

# Extract the section for this version
awk -v version="$VERSION" '
    /^## \['"$VERSION"'\]/ { found = 1; next }
    found && /^## \[/ { exit }
    found { print }
' CHANGELOG.md