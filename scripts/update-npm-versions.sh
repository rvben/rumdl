#!/usr/bin/env bash
# Update all npm package versions to match Cargo.toml version
# This is automatically run during CI release, but can be run manually for testing

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

cd "$PROJECT_ROOT"

# Get version from Cargo.toml
VERSION=$(grep '^version = ' Cargo.toml | head -1 | sed 's/.*"\(.*\)".*/\1/')
echo "Updating npm packages to version $VERSION"

# Check if jq is available
if ! command -v jq &> /dev/null; then
    echo "Error: jq is required but not installed"
    exit 1
fi

# Validate version format (semver)
if ! [[ "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+(-[a-zA-Z0-9.]+)?$ ]]; then
    echo "Error: Invalid version format: $VERSION"
    exit 1
fi

# Update all platform package.json files
PLATFORM_PACKAGES=(
    "npm/cli-darwin-x64/package.json"
    "npm/cli-darwin-arm64/package.json"
    "npm/cli-linux-x64/package.json"
    "npm/cli-linux-arm64/package.json"
    "npm/cli-linux-x64-musl/package.json"
    "npm/cli-linux-arm64-musl/package.json"
    "npm/cli-win32-x64/package.json"
)

for pkg in "${PLATFORM_PACKAGES[@]}"; do
    if [[ -f "$pkg" ]]; then
        echo "  Updating $pkg"
        jq --arg v "$VERSION" '.version = $v' "$pkg" > tmp.json && mv tmp.json "$pkg"
    else
        echo "  Warning: $pkg not found, skipping"
    fi
done

# Update main package.json and its optionalDependencies
MAIN_PKG="npm/rumdl/package.json"
if [[ -f "$MAIN_PKG" ]]; then
    echo "  Updating $MAIN_PKG"
    jq --arg v "$VERSION" '
        .version = $v |
        .optionalDependencies |= with_entries(.value = $v)
    ' "$MAIN_PKG" > tmp.json && mv tmp.json "$MAIN_PKG"
else
    echo "Error: Main package.json not found at $MAIN_PKG"
    exit 1
fi

# Verify updates
echo ""
echo "Verification:"
for pkg in "${PLATFORM_PACKAGES[@]}" "$MAIN_PKG"; do
    if [[ -f "$pkg" ]]; then
        PKG_VERSION=$(jq -r '.version' "$pkg")
        if [[ "$PKG_VERSION" == "$VERSION" ]]; then
            echo "  ✓ $pkg: $PKG_VERSION"
        else
            echo "  ✗ $pkg: expected $VERSION, got $PKG_VERSION"
            exit 1
        fi
    fi
done

echo ""
echo "Done! All packages updated to version $VERSION"
