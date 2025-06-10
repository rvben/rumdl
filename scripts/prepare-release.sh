#!/bin/bash

# prepare-release.sh - Helper script to prepare release notes

set -e

# Colors for output
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

echo -e "${BLUE}=== Release Preparation Helper ===${NC}"

# Get the last tag
LAST_TAG=$(git describe --tags --abbrev=0 2>/dev/null || echo "v0.0.0")
echo -e "Last release: ${YELLOW}$LAST_TAG${NC}"

# Show commits since last tag
echo -e "\n${BLUE}Commits since $LAST_TAG:${NC}"
git log $LAST_TAG..HEAD --oneline

# Categorize commits
echo -e "\n${BLUE}Categorized changes:${NC}"

echo -e "\n${GREEN}Features/Added:${NC}"
git log $LAST_TAG..HEAD --oneline | grep -iE "^[a-f0-9]+ (feat|add|new)" || echo "  (none)"

echo -e "\n${GREEN}Changes/Refactoring:${NC}"
git log $LAST_TAG..HEAD --oneline | grep -iE "^[a-f0-9]+ (refactor|change|update|optimize|improve)" || echo "  (none)"

echo -e "\n${GREEN}Fixes:${NC}"
git log $LAST_TAG..HEAD --oneline | grep -iE "^[a-f0-9]+ (fix|bugfix|hotfix)" || echo "  (none)"

echo -e "\n${GREEN}Performance:${NC}"
git log $LAST_TAG..HEAD --oneline | grep -iE "^[a-f0-9]+ (perf|optimize|speed)" || echo "  (none)"

echo -e "\n${GREEN}Documentation:${NC}"
git log $LAST_TAG..HEAD --oneline | grep -iE "^[a-f0-9]+ (docs|doc)" || echo "  (none)"

# Check for breaking changes
echo -e "\n${YELLOW}Potential breaking changes:${NC}"
git log $LAST_TAG..HEAD --oneline | grep -iE "breaking|!\s*:" || echo "  (none detected)"

# Show file statistics
echo -e "\n${BLUE}File changes summary:${NC}"
git diff --stat $LAST_TAG..HEAD | tail -1

# Remind about manual steps
echo -e "\n${YELLOW}Release checklist:${NC}"
echo "1. Update CHANGELOG.md with categorized changes under [Unreleased]"
echo "2. Run tests: make test"
echo "3. Build release: make build"
echo "4. Create release: make release-patch (or release-minor/release-major)"
echo "5. The GitHub Action will automatically create release notes from the tag"

echo -e "\n${GREEN}Suggested CHANGELOG.md entry format:${NC}"
echo "### Added"
echo "- New features..."
echo ""
echo "### Changed"
echo "- Improvements..."
echo ""
echo "### Fixed"
echo "- Bug fixes..."
echo ""
echo "### Performance"
echo "- Optimizations..."