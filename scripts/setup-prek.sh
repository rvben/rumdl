#!/bin/bash

# Setup prek hooks for rumdl development
# This script installs prek and sets up the hooks

set -e

echo "Setting up prek hooks for rumdl..."

# Check if prek is installed
if ! command -v prek &> /dev/null; then
    echo "Installing prek..."
    if command -v uv &> /dev/null; then
        uv tool install prek
    elif command -v cargo &> /dev/null; then
        cargo install prek
    else
        echo "Error: Neither uv nor cargo found. Please install one of them first."
        exit 1
    fi
else
    echo "prek is already installed"
fi

# Install the hooks
echo "Installing prek hooks..."
prek install
prek install --hook-type commit-msg
prek install --hook-type pre-push

# Run hooks on all files to ensure everything is set up correctly
echo "Running prek hooks on all files..."
prek run --all-files || {
    echo "Some hooks failed, but that's normal for first run."
    echo "The hooks will automatically fix formatting issues."
    echo "You may need to stage and commit the changes."
}

echo ""
echo "✅ prek hooks are now set up!"
echo ""
echo "The following hooks will run on every commit:"
echo "  - cargo fmt (automatic formatting)"
echo "  - cargo metadata --locked (Cargo.lock validation)"
echo "  - File quality checks (trailing whitespace, etc.)"
echo "  - rumdl markdown linting"
echo ""
echo "The following hooks run on pre-push:"
echo "  - make lint (clippy linting)"
echo "  - make test-push (full CI test profile)"
echo ""
echo "To manually run all hooks: prek run --all-files"
echo "To skip hooks for emergency commits: git commit --no-verify"
