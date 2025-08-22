#!/bin/bash
# Pre-release validation script
# Run this before creating a release tag to ensure everything will pass in CI

set -e  # Exit on any error

echo "🔍 Pre-Release Validation Starting..."
echo "====================================="

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Track if any checks fail
FAILED=0

# Function to run a check
run_check() {
    local name="$1"
    local command="$2"
    local show_progress="${3:-false}"

    echo -n "Running $name... "
    if [ "$show_progress" = "true" ]; then
        echo ""  # New line for progress
        if eval "$command"; then
            echo -e "${GREEN}✓ $name passed${NC}"
        else
            echo -e "${RED}✗ $name failed${NC}"
            FAILED=1
        fi
    else
        if eval "$command" > /tmp/pre-release-check.log 2>&1; then
            echo -e "${GREEN}✓${NC}"
        else
            echo -e "${RED}✗${NC}"
            echo -e "${RED}Error in $name:${NC}"
            tail -20 /tmp/pre-release-check.log
            FAILED=1
        fi
    fi
}

# 1. Check Rust version matches CI
echo "1. Checking Rust version..."
EXPECTED_VERSION="1.89.0"
ACTUAL_VERSION=$(rustc --version | grep -oE '[0-9]+\.[0-9]+\.[0-9]+')
if [ "$ACTUAL_VERSION" != "$EXPECTED_VERSION" ]; then
    echo -e "${YELLOW}Warning: Local Rust version ($ACTUAL_VERSION) differs from CI ($EXPECTED_VERSION)${NC}"
    echo "   Run: rustup update && rustup default $EXPECTED_VERSION"
fi

# 2. Run clippy with exact CI flags
echo "2. Running Clippy (all targets, all features)..."
echo "   This may take 1-2 minutes..."
run_check "Clippy" "cargo clippy --all-targets --all-features -- -D warnings -D clippy::uninlined_format_args" "false"

# 3. Check formatting
echo "3. Checking code formatting..."
run_check "Format check" "cargo fmt --all -- --check"

# 4. Run tests (quick version for pre-release)
echo "4. Running tests (quick mode)..."
run_check "Tests" "make test-quick"

# 5. Build release binary
echo "5. Building release binary..."
run_check "Release build" "cargo build --release"

# 6. Check documentation builds
echo "6. Checking documentation..."
run_check "Documentation" "cargo doc --no-deps"

# 7. Verify Cargo.toml version (pyproject.toml uses dynamic versioning from Cargo.toml)
echo "7. Checking version consistency..."
CARGO_VERSION=$(grep '^version' Cargo.toml | head -1 | cut -d'"' -f2)
# pyproject.toml uses dynamic = ["version"] to read from Cargo.toml
if grep -q 'dynamic = \["version"\]' pyproject.toml; then
    echo -e "${GREEN}✓${NC} Version $CARGO_VERSION (pyproject.toml uses dynamic versioning)"
else
    # Fallback: check if there's a static version in pyproject.toml
    PY_VERSION=$(grep '^version' pyproject.toml | head -1 | cut -d'"' -f2)
    if [ -z "$PY_VERSION" ]; then
        echo -e "${YELLOW}⚠${NC} pyproject.toml should use dynamic = [\"version\"] for version"
        # Not a failure, just a warning
    elif [ "$CARGO_VERSION" != "$PY_VERSION" ]; then
        echo -e "${RED}Version mismatch: Cargo.toml ($CARGO_VERSION) vs pyproject.toml ($PY_VERSION)${NC}"
        FAILED=1
    else
        echo -e "${GREEN}✓${NC} Version $CARGO_VERSION is consistent"
    fi
fi

# 8. Check for uncommitted changes
echo "8. Checking for uncommitted changes..."
if ! git diff --quiet || ! git diff --cached --quiet; then
    echo -e "${RED}✗ Uncommitted changes detected${NC}"
    git status --short
    FAILED=1
else
    echo -e "${GREEN}✓${NC} Working directory clean"
fi

# 9. Verify we're on main branch
echo "9. Checking branch..."
CURRENT_BRANCH=$(git branch --show-current)
if [ "$CURRENT_BRANCH" != "main" ]; then
    echo -e "${YELLOW}Warning: Not on main branch (current: $CURRENT_BRANCH)${NC}"
fi

# 10. Check if tag already exists
echo "10. Checking tag availability..."
TAG_VERSION="v$CARGO_VERSION"
if git tag -l | grep -q "^$TAG_VERSION$"; then
    echo -e "${RED}✗ Tag $TAG_VERSION already exists${NC}"
    FAILED=1
else
    echo -e "${GREEN}✓${NC} Tag $TAG_VERSION is available"
fi

# 11. Verify Cargo.lock is up to date
echo "11. Verifying Cargo.lock is up to date..."
if cargo update --workspace --dry-run 2>&1 | grep -q "Updating"; then
    echo -e "${RED}✗ Cargo.lock needs updating${NC}"
    echo "   Run: cargo update --workspace"
    FAILED=1
else
    echo -e "${GREEN}✓${NC} Cargo.lock is up to date"
fi

# 12. Test cargo publish with --dry-run
echo "12. Testing cargo publish (dry run)..."
run_check "Cargo publish dry-run" "cargo publish --dry-run --locked"

# 13. Check mise version in GitHub Actions workflow
echo "13. Checking mise version in CI workflow..."
MISE_VERSION=$(grep -A2 "mise-action@v2" .github/workflows/release.yml | grep "version:" | sed 's/.*version: *//' | tr -d ' ')
if [ -n "$MISE_VERSION" ]; then
    echo -n "Verifying mise version $MISE_VERSION exists... "
    # Check if the release exists on GitHub
    if curl -s -f "https://api.github.com/repos/jdx/mise/releases/tags/v${MISE_VERSION}" > /dev/null 2>&1; then
        echo -e "${GREEN}✓${NC} mise version $MISE_VERSION exists"
    else
        echo -e "${RED}✗${NC}"
        echo -e "${RED}mise version $MISE_VERSION does not exist on GitHub!${NC}"
        echo "Check available versions at: https://github.com/jdx/mise/releases"
        LATEST_MISE=$(curl -s https://api.github.com/repos/jdx/mise/releases/latest | grep '"tag_name"' | cut -d'"' -f4 | sed 's/^v//')
        echo "Latest available version: $LATEST_MISE"
        FAILED=1
    fi
else
    echo -e "${YELLOW}⚠${NC} No explicit mise version found in workflow (will use latest)"
fi

echo ""
echo "====================================="
if [ $FAILED -eq 0 ]; then
    echo -e "${GREEN}✅ All pre-release checks passed!${NC}"
    echo ""
    echo "Ready to release $TAG_VERSION. Run:"
    echo "  git tag $TAG_VERSION"
    echo "  git push origin $TAG_VERSION"
    exit 0
else
    echo -e "${RED}❌ Pre-release checks failed!${NC}"
    echo "Please fix the issues above before creating a release."
    exit 1
fi
