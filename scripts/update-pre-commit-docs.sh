#!/bin/bash
# Script to update rumdl-pre-commit version references in documentation

set -e

# Get the latest release version
LATEST_VERSION=$(gh release list --repo rvben/rumdl --limit 1 | awk '{print $1}')

if [ -z "$LATEST_VERSION" ]; then
    echo "Error: Could not determine latest version"
    exit 1
fi

echo "Latest rumdl version: $LATEST_VERSION"

# Update version in all documentation files
FILES=(
    "README.md"
    "docs/global-settings.md"
    "../rumdl-pre-commit/README.md"
)

for file in "${FILES[@]}"; do
    if [ -f "$file" ]; then
        echo "Updating $file..."
        # Update rev: vX.X.X patterns
        sed -i.bak -E "s/rev: v[0-9]+\.[0-9]+\.[0-9]+/rev: $LATEST_VERSION/g" "$file"
        # Clean up backup files
        rm -f "${file}.bak"
    else
        echo "Warning: $file not found"
    fi
done

echo "Documentation updated to use $LATEST_VERSION"
