#!/bin/bash
# Generate a markdown table for release downloads

VERSION="${1:-}"
if [ -z "$VERSION" ]; then
    echo "Usage: $0 <version>"
    echo "Example: $0 v0.0.111"
    exit 1
fi

# Start the markdown table
cat << 'EOF'
## Downloads

| File | Platform | Checksum |
|------|----------|----------|
EOF

# Generate table rows for each platform
generate_row() {
    local target="$1"
    local platform="$2"

    # Determine file extension based on platform
    if [[ "$target" == *"windows"* ]]; then
        ext="zip"
    else
        ext="tar.gz"
    fi

    filename="rumdl-${VERSION}-${target}.${ext}"
    checksum_file="${filename}.sha256"

    # Generate download URLs
    download_url="https://github.com/rvben/rumdl/releases/download/${VERSION}/${filename}"
    checksum_url="https://github.com/rvben/rumdl/releases/download/${VERSION}/${checksum_file}"

    # Output table row
    echo "| [${filename}](${download_url}) | ${platform} | [checksum](${checksum_url}) |"
}

# Generate rows for each platform
generate_row "x86_64-unknown-linux-gnu" "Linux x86_64"
generate_row "x86_64-unknown-linux-musl" "Linux x86_64 (musl)"
generate_row "aarch64-unknown-linux-gnu" "Linux ARM64"
generate_row "aarch64-unknown-linux-musl" "Linux ARM64 (musl)"
generate_row "x86_64-apple-darwin" "macOS x86_64"
generate_row "aarch64-apple-darwin" "macOS ARM64 (Apple Silicon)"
generate_row "x86_64-pc-windows-msvc" "Windows x86_64"

echo ""
echo "## Installation"
echo ""
echo "### Using uv (Recommended)"
echo '```bash'
echo 'uv tool install rumdl'
echo '```'
echo ""
echo "### Using pip"
echo '```bash'
echo 'pip install rumdl'
echo '```'
echo ""
echo "### Using pipx"
echo '```bash'
echo 'pipx install rumdl'
echo '```'
echo ""
echo "### Direct Download"
echo "Download the appropriate binary for your platform from the table above, extract it, and add it to your PATH."
