#!/usr/bin/env bash
# Script to verify that a release is ready to be tagged and pushed
# This checks all the conditions that would cause a release to fail in CI

set -euo pipefail

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

echo "🔍 Verifying release readiness..."
echo ""

ERRORS=0

# Check 1: Verify Cargo.lock is up-to-date
echo -n "Checking Cargo.lock is up-to-date... "
if mise exec -- cargo check --locked &>/dev/null; then
    echo -e "${GREEN}✓${NC}"
else
    echo -e "${RED}✗${NC}"
    echo -e "${RED}ERROR: Cargo.lock is out of date or missing${NC}"
    echo "Run: mise exec -- cargo check"
    echo "Then commit the updated Cargo.lock"
    ((ERRORS++))
fi

# Check 2: Verify no uncommitted changes to tracked files
echo -n "Checking for uncommitted changes... "
if [[ -z $(git status --porcelain -uno) ]]; then
    echo -e "${GREEN}✓${NC}"
else
    echo -e "${RED}✗${NC}"
    echo -e "${RED}ERROR: There are uncommitted changes to tracked files${NC}"
    git status --short -uno
    ((ERRORS++))
fi

# Check 3: Verify version consistency
echo -n "Checking version consistency... "
CARGO_VERSION=$(grep '^version =' Cargo.toml | head -1 | sed 's/.*"\(.*\)".*/\1/')
LOCK_VERSION=$(grep -A 1 '^name = "rumdl"' Cargo.lock | grep '^version' | head -1 | sed 's/.*"\(.*\)".*/\1/')

if [[ "$CARGO_VERSION" == "$LOCK_VERSION" ]]; then
    echo -e "${GREEN}✓${NC} (v$CARGO_VERSION)"
else
    echo -e "${RED}✗${NC}"
    echo -e "${RED}ERROR: Version mismatch!${NC}"
    echo "Cargo.toml: $CARGO_VERSION"
    echo "Cargo.lock: $LOCK_VERSION"
    ((ERRORS++))
fi

# Check 4: Verify CHANGELOG.md has entry for current version
echo -n "Checking CHANGELOG.md for v$CARGO_VERSION... "
if grep -q "## \[${CARGO_VERSION}\]" CHANGELOG.md; then
    echo -e "${GREEN}✓${NC}"
else
    echo -e "${YELLOW}⚠${NC}"
    echo -e "${YELLOW}WARNING: No CHANGELOG entry found for v${CARGO_VERSION}${NC}"
    echo "Consider adding a CHANGELOG entry before releasing"
fi

# Check 5: Verify README.md has correct pre-commit version
echo -n "Checking README.md pre-commit version... "
README_VERSIONS=$(grep -o "rev: v[0-9.]*" README.md | sort -u)
EXPECTED_REV="rev: v$CARGO_VERSION"
if echo "$README_VERSIONS" | grep -q "^$EXPECTED_REV$"; then
    # Check all occurrences match
    MISMATCHED=$(grep "rev: v[0-9.]*" README.md | grep -v "$EXPECTED_REV" || true)
    if [[ -z "$MISMATCHED" ]]; then
        echo -e "${GREEN}✓${NC}"
    else
        echo -e "${RED}✗${NC}"
        echo -e "${RED}ERROR: README.md has inconsistent pre-commit versions${NC}"
        echo "Expected: $EXPECTED_REV"
        echo "Found mismatches:"
        echo "$MISMATCHED"
        ((ERRORS++))
    fi
else
    echo -e "${RED}✗${NC}"
    echo -e "${RED}ERROR: README.md pre-commit version not updated${NC}"
    echo "Expected: $EXPECTED_REV"
    echo "Found: $README_VERSIONS"
    echo "Run: sed -i '' 's/rev: v[0-9.]*/rev: v$CARGO_VERSION/' README.md"
    ((ERRORS++))
fi

# Check 6: Verify we're on main branch
echo -n "Checking current branch... "
CURRENT_BRANCH=$(git branch --show-current)
if [[ "$CURRENT_BRANCH" == "main" ]]; then
    echo -e "${GREEN}✓${NC} (main)"
else
    echo -e "${YELLOW}⚠${NC}"
    echo -e "${YELLOW}WARNING: Not on main branch (currently on: $CURRENT_BRANCH)${NC}"
fi

# Check 7: Verify tag doesn't already exist
echo -n "Checking if tag v$CARGO_VERSION exists... "
if git rev-parse "v$CARGO_VERSION" &>/dev/null; then
    echo -e "${RED}✗${NC}"
    echo -e "${RED}ERROR: Tag v$CARGO_VERSION already exists${NC}"
    echo "Delete with: git tag -d v$CARGO_VERSION && git push origin --delete v$CARGO_VERSION"
    ((ERRORS++))
else
    echo -e "${GREEN}✓${NC}"
fi

# Summary
echo ""
echo "════════════════════════════════════════"
if [[ $ERRORS -eq 0 ]]; then
    echo -e "${GREEN}✅ Release is ready!${NC}"
    echo ""
    echo "To create and push the release:"
    echo "  git tag v$CARGO_VERSION"
    echo "  git push origin main v$CARGO_VERSION"
else
    echo -e "${RED}❌ Release is NOT ready ($ERRORS errors)${NC}"
    echo "Fix the errors above before tagging"
    exit 1
fi
