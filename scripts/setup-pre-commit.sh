#!/bin/bash

# Setup pre-commit hooks for rumdl development
# This script installs pre-commit and sets up the hooks

set -e

echo "Setting up pre-commit hooks for rumdl..."

# Check if pre-commit is installed
if ! command -v pre-commit &> /dev/null; then
    echo "Installing pre-commit..."
    if command -v uv &> /dev/null; then
        uv tool install pre-commit
    elif command -v pip &> /dev/null; then
        pip install pre-commit
    else
        echo "Error: Neither uv nor pip found. Please install one of them first."
        exit 1
    fi
else
    echo "pre-commit is already installed"
fi

# Install the hooks
echo "Installing pre-commit hooks..."
pre-commit install

# Run hooks on all files to ensure everything is set up correctly
echo "Running pre-commit hooks on all files..."
pre-commit run --all-files || {
    echo "Some hooks failed, but that's normal for first run."
    echo "The hooks will automatically fix formatting issues."
    echo "You may need to stage and commit the changes."
}

echo ""
echo "✅ Pre-commit hooks are now set up!"
echo ""
echo "The following hooks will run on every commit:"
echo "  - cargo fmt (automatic formatting)"
echo "  - make lint (clippy linting)"
echo "  - make test-quick (fast tests)"
echo "  - cargo check (compilation check)"
echo "  - File quality checks (trailing whitespace, etc.)"
echo "  - rumdl markdown linting"
echo ""
echo "To manually run all hooks: pre-commit run --all-files"
echo "To skip hooks for emergency commits: git commit --no-verify"
