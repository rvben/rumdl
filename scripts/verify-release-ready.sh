#!/usr/bin/env bash
# Script to verify that a release is ready to be tagged and pushed.
# With --fix, automatically fixes what it can (versions, rule counts, etc.)

set -euo pipefail

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

FIX_MODE=false
if [[ "${1:-}" == "--fix" ]]; then
    FIX_MODE=true
fi

echo "🔍 Verifying release readiness..."
if $FIX_MODE; then
    echo "   (--fix mode: will auto-fix where possible)"
fi
echo ""

ERRORS=0
FIXED=0

# Get cargo version early (needed by multiple checks)
CARGO_VERSION=$(grep '^version =' Cargo.toml | head -1 | sed 's/.*"\(.*\)".*/\1/')

# Check 1: Verify Cargo.lock is up-to-date
echo -n "Checking Cargo.lock is up-to-date... "
if mise exec -- cargo check --locked &>/dev/null; then
    echo -e "${GREEN}✓${NC}"
else
    if $FIX_MODE; then
        mise exec -- cargo check &>/dev/null
        echo -e "${GREEN}✓${NC} (fixed)"
        ((FIXED++))
    else
        echo -e "${RED}✗${NC}"
        echo -e "${RED}ERROR: Cargo.lock is out of date or missing${NC}"
        echo "Run: mise exec -- cargo check"
        echo "Then commit the updated Cargo.lock"
        ((ERRORS++))
    fi
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

# Check 4: Verify CHANGELOG.md has entry for current version with actual content
echo -n "Checking CHANGELOG.md for v$CARGO_VERSION... "
if grep -q "## \[${CARGO_VERSION}\]" CHANGELOG.md; then
    # Verify the section has content (not just an empty header)
    SECTION_CONTENT=$(sed -n "/## \[${CARGO_VERSION}\]/,/## \[/p" CHANGELOG.md | sed '1d;$d' | grep -v '^$' || true)
    if [[ -n "$SECTION_CONTENT" ]]; then
        echo -e "${GREEN}✓${NC}"
    else
        echo -e "${RED}✗${NC}"
        echo -e "${RED}ERROR: CHANGELOG entry for v${CARGO_VERSION} is empty${NC}"
        echo "Add release notes under the ## [${CARGO_VERSION}] header before releasing"
        ((ERRORS++))
    fi
else
    echo -e "${RED}✗${NC}"
    echo -e "${RED}ERROR: No CHANGELOG entry found for v${CARGO_VERSION}${NC}"
    echo "Add a ## [${CARGO_VERSION}] section to CHANGELOG.md before releasing"
    ((ERRORS++))
fi

# Check 5: Verify README.md has correct pre-commit version
echo -n "Checking README.md pre-commit version... "
README_VERSIONS=$(grep -o "rev: v[0-9.]*" README.md | sort -u)
EXPECTED_REV="rev: v$CARGO_VERSION"
if echo "$README_VERSIONS" | grep -q "^$EXPECTED_REV$" && \
   [[ -z $(grep "rev: v[0-9.]*" README.md | grep -v "$EXPECTED_REV" || true) ]]; then
    echo -e "${GREEN}✓${NC}"
else
    if $FIX_MODE; then
        sed -i '' "s/rev: v[0-9.]*/rev: v$CARGO_VERSION/g" README.md
        echo -e "${GREEN}✓${NC} (fixed)"
        ((FIXED++))
    else
        echo -e "${RED}✗${NC}"
        echo -e "${RED}ERROR: README.md pre-commit version not updated${NC}"
        echo "Expected: $EXPECTED_REV"
        echo "Found: $README_VERSIONS"
        ((ERRORS++))
    fi
fi

# Check 6: Verify npm package versions match Cargo.toml
echo -n "Checking npm package versions... "
if [[ -d "npm" ]]; then
    NPM_OK=true
    MAIN_NPM_VERSION=$(jq -r '.version // empty' npm/rumdl/package.json 2>/dev/null || echo "")
    if [[ "$MAIN_NPM_VERSION" != "$CARGO_VERSION" ]]; then
        NPM_OK=false
    fi

    for pkg in npm/cli-*/package.json; do
        PKG_VERSION=$(jq -r '.version // empty' "$pkg" 2>/dev/null || echo "")
        if [[ "$PKG_VERSION" != "$CARGO_VERSION" ]]; then
            NPM_OK=false
            break
        fi
    done

    if $NPM_OK; then
        echo -e "${GREEN}✓${NC}"
    elif $FIX_MODE; then
        scripts/update-npm-versions.sh >/dev/null
        echo -e "${GREEN}✓${NC} (fixed)"
        ((FIXED++))
    else
        echo -e "${RED}✗${NC}"
        echo -e "${RED}ERROR: npm package version mismatch${NC}"
        echo "Run: scripts/update-npm-versions.sh"
        ((ERRORS++))
    fi
else
    echo -e "${YELLOW}⚠${NC} (npm directory not found)"
fi

# Check 7: Verify README.md has correct mise version
echo -n "Checking README.md mise version... "
if grep -q "mise use rumdl@" README.md; then
    README_MISE_VERSION=$(grep -o "mise use rumdl@[0-9.]*" README.md | sed 's/mise use rumdl@//')
    if [[ "$README_MISE_VERSION" == "$CARGO_VERSION" ]]; then
        echo -e "${GREEN}✓${NC}"
    elif $FIX_MODE; then
        sed -i '' "s/mise use rumdl@[0-9.]*/mise use rumdl@$CARGO_VERSION/g" README.md
        echo -e "${GREEN}✓${NC} (fixed)"
        ((FIXED++))
    else
        echo -e "${RED}✗${NC}"
        echo -e "${RED}ERROR: README.md mise version not updated${NC}"
        echo "Expected: mise use rumdl@$CARGO_VERSION"
        echo "Found: mise use rumdl@$README_MISE_VERSION"
        ((ERRORS++))
    fi
else
    echo -e "${YELLOW}⚠${NC} (no mise example found)"
fi

# Check 8: Verify we're on main branch
echo -n "Checking current branch... "
CURRENT_BRANCH=$(git branch --show-current)
if [[ "$CURRENT_BRANCH" == "main" ]]; then
    echo -e "${GREEN}✓${NC} (main)"
else
    echo -e "${YELLOW}⚠${NC}"
    echo -e "${YELLOW}WARNING: Not on main branch (currently on: $CURRENT_BRANCH)${NC}"
fi

# Check 9: Verify tag doesn't already exist
echo -n "Checking if tag v$CARGO_VERSION exists... "
if git rev-parse "v$CARGO_VERSION" &>/dev/null; then
    echo -e "${RED}✗${NC}"
    echo -e "${RED}ERROR: Tag v$CARGO_VERSION already exists${NC}"
    echo "Delete with: git tag -d v$CARGO_VERSION && git push origin --delete v$CARGO_VERSION"
    ((ERRORS++))
else
    echo -e "${GREEN}✓${NC}"
fi

# Check 10: Verify CONTRIBUTING.md Rust version matches rust-toolchain.toml
echo -n "Checking CONTRIBUTING.md Rust version... "
TOOLCHAIN_VERSION=$(grep '^channel' rust-toolchain.toml | sed 's/.*"\(.*\)".*/\1/')
CONTRIB_RUST_VERSION=$(grep -oE 'Rust.*[0-9]+\.[0-9]+\.[0-9]+' CONTRIBUTING.md | grep -oE '[0-9]+\.[0-9]+\.[0-9]+' | head -1)

if [[ "$CONTRIB_RUST_VERSION" == "$TOOLCHAIN_VERSION" ]]; then
    echo -e "${GREEN}✓${NC} ($TOOLCHAIN_VERSION)"
elif $FIX_MODE; then
    sed -i '' "s/${CONTRIB_RUST_VERSION}/${TOOLCHAIN_VERSION}/g" CONTRIBUTING.md
    echo -e "${GREEN}✓${NC} (fixed to $TOOLCHAIN_VERSION)"
    ((FIXED++))
else
    echo -e "${RED}✗${NC}"
    echo -e "${RED}ERROR: CONTRIBUTING.md Rust version mismatch${NC}"
    echo "rust-toolchain.toml: $TOOLCHAIN_VERSION"
    echo "CONTRIBUTING.md: $CONTRIB_RUST_VERSION"
    ((ERRORS++))
fi

# Check 11: Verify documented rule count matches actual rule count
echo -n "Checking rule count in docs... "
ACTUAL_RULE_COUNT=$(grep -c 'name: "MD[0-9]' src/rules/mod.rs)
DOCS_MISMATCHES=""

# Check docs/index.md
while read -r DOCS_COUNT; do
    if [[ "$DOCS_COUNT" != "$ACTUAL_RULE_COUNT" ]]; then
        DOCS_MISMATCHES="${DOCS_MISMATCHES}docs/index.md says $DOCS_COUNT, "
    fi
done < <(grep -oE '[0-9]+ lint(ing)? rules' docs/index.md | grep -oE '[0-9]+')

# Check docs/rules.md
while read -r DOCS_COUNT; do
    if [[ "$DOCS_COUNT" != "$ACTUAL_RULE_COUNT" ]]; then
        DOCS_MISMATCHES="${DOCS_MISMATCHES}docs/rules.md says $DOCS_COUNT, "
    fi
done < <(grep -oE 'implements [0-9]+ rules' docs/rules.md | grep -oE '[0-9]+')

# Check README.md
while read -r DOCS_COUNT; do
    if [[ "$DOCS_COUNT" != "$ACTUAL_RULE_COUNT" ]]; then
        DOCS_MISMATCHES="${DOCS_MISMATCHES}README.md says $DOCS_COUNT, "
    fi
done < <(grep -oE '[0-9]+ lint(ing)? rules' README.md | grep -oE '[0-9]+')
while read -r DOCS_COUNT; do
    if [[ "$DOCS_COUNT" != "$ACTUAL_RULE_COUNT" ]]; then
        DOCS_MISMATCHES="${DOCS_MISMATCHES}README.md says 'implements $DOCS_COUNT', "
    fi
done < <(grep -oE 'implements [0-9]+ lint rules' README.md | grep -oE '[0-9]+')

if [[ -z "$DOCS_MISMATCHES" ]]; then
    echo -e "${GREEN}✓${NC} ($ACTUAL_RULE_COUNT rules)"
elif $FIX_MODE; then
    # Fix docs/index.md: replace any "N lint rules" or "N linting rules"
    sed -i '' -E "s/[0-9]+ lint(ing)? rules/$ACTUAL_RULE_COUNT lint rules/g" docs/index.md
    # Fix docs/rules.md: replace "implements N rules"
    sed -i '' -E "s/implements [0-9]+ rules/implements $ACTUAL_RULE_COUNT rules/g" docs/rules.md
    # Fix README.md: replace "N lint rules" and "implements N lint rules"
    sed -i '' -E "s/[0-9]+ lint(ing)? rules/$ACTUAL_RULE_COUNT lint rules/g" README.md
    echo -e "${GREEN}✓${NC} (fixed to $ACTUAL_RULE_COUNT rules)"
    ((FIXED++))
else
    echo -e "${RED}✗${NC}"
    echo -e "${RED}ERROR: Rule count mismatch in documentation${NC}"
    echo "Actual rules: $ACTUAL_RULE_COUNT"
    echo "Mismatches: ${DOCS_MISMATCHES%, }"
    ((ERRORS++))
fi

# Check 12: Verify rules.json is up-to-date
echo -n "Checking rules.json is up-to-date... "
if [[ -f "rules.json" ]]; then
    TEMP_RULES=$(mktemp)
    if ./target/release/rumdl rule -o json > "$TEMP_RULES" 2>/dev/null || \
       cargo run --release -- rule -o json > "$TEMP_RULES" 2>/dev/null; then
        if diff -q rules.json "$TEMP_RULES" &>/dev/null; then
            echo -e "${GREEN}✓${NC}"
        elif $FIX_MODE; then
            cp "$TEMP_RULES" rules.json
            echo -e "${GREEN}✓${NC} (fixed)"
            ((FIXED++))
        else
            echo -e "${RED}✗${NC}"
            echo -e "${RED}ERROR: rules.json is out of date${NC}"
            echo "Run: ./target/release/rumdl rule -o json > rules.json"
            ((ERRORS++))
        fi
    else
        echo -e "${YELLOW}⚠${NC} (could not generate rules.json)"
    fi
    rm -f "$TEMP_RULES"
else
    echo -e "${RED}✗${NC}"
    echo -e "${RED}ERROR: rules.json not found${NC}"
    echo "Run: ./target/release/rumdl rule -o json > rules.json"
    ((ERRORS++))
fi

# Check 13: Check if schema changed since last release (SchemaStore reminder)
echo -n "Checking if schema changed since last release... "
LAST_TAG=$(git describe --tags --abbrev=0 2>/dev/null || echo "")
if [[ -n "$LAST_TAG" ]]; then
    if git diff --quiet "$LAST_TAG" -- rumdl.schema.json 2>/dev/null; then
        echo -e "${GREEN}✓${NC} (unchanged)"
    else
        echo -e "${YELLOW}⚠${NC}"
        echo -e "${YELLOW}WARNING: rumdl.schema.json has changed since $LAST_TAG${NC}"
        echo "After releasing, submit a PR to update SchemaStore:"
        echo "  https://github.com/SchemaStore/schemastore"
        echo "  File: src/schemas/json/rumdl.json"
    fi
else
    echo -e "${YELLOW}⚠${NC} (no previous tag found)"
fi

# Check 14: Verify opt-in rules are documented
echo -n "Checking opt-in rules are documented... "
# Find rules with enabled: false as default (opt-in rules)
# Pattern 1: explicit "enabled: false" in Default impl (but not fix_enabled, etc.)
OPT_IN_EXPLICIT=$(grep -rlE '[^_]enabled: false' src/rules/ 2>/dev/null | \
    grep -oE "md[0-9]+" | tr '[:lower:]' '[:upper:]' | sort -u)

# Pattern 2: fn default_enabled() -> bool { false }
OPT_IN_FN=""
while IFS= read -r file; do
    if grep -A1 "fn default_enabled" "$file" 2>/dev/null | grep -q "false"; then
        OPT_IN_FN="$OPT_IN_FN $(dirname "$file" | grep -oE "md[0-9]+" | tr '[:lower:]' '[:upper:]')"
    fi
done < <(grep -rl "fn default_enabled" src/rules/ 2>/dev/null)
OPT_IN_FN=$(echo "$OPT_IN_FN" | tr ' ' '\n' | sort -u | grep -v "^$")

# Pattern 3: comment says "default: false - opt-in rule"
OPT_IN_COMMENT=$(grep -rl "default: false.*opt-in\|opt-in.*default.*false" src/rules/ 2>/dev/null | \
    grep -oE "md[0-9]+" | tr '[:lower:]' '[:upper:]' | sort -u)

OPT_IN_RULES=$(echo -e "$OPT_IN_EXPLICIT\n$OPT_IN_FN\n$OPT_IN_COMMENT" | sort -u | grep -v "^$")

# Check which are documented in rules.md opt-in table
MISSING_DOCS=""
for RULE in $OPT_IN_RULES; do
    if ! grep -q "\[${RULE}\]" docs/rules.md | grep -A20 "## Opt-in Rules" &>/dev/null; then
        # More precise check: look in the opt-in section specifically
        OPT_IN_SECTION=$(sed -n '/## Opt-in Rules/,/## /p' docs/rules.md | head -20)
        if ! echo "$OPT_IN_SECTION" | grep -qi "\[${RULE}\]"; then
            MISSING_DOCS="${MISSING_DOCS}${RULE} "
        fi
    fi
done

if [[ -z "$MISSING_DOCS" ]]; then
    echo -e "${GREEN}✓${NC}"
else
    echo -e "${RED}✗${NC}"
    echo -e "${RED}ERROR: Opt-in rules missing from docs/rules.md opt-in table:${NC}"
    echo "  $MISSING_DOCS"
    echo "Add them to the '## Opt-in Rules' section in docs/rules.md"
    ((ERRORS++))
fi

# Check 15: Verify no config validation warnings for rule options
echo -n "Checking config validation for rule options... "
# Create a test config with all configurable rules enabled
TEMP_CONFIG=$(mktemp)
cat > "$TEMP_CONFIG" << 'CONFIGEOF'
# Test config to verify all rule options are recognized
[MD060]
enabled = true
style = "aligned"
column-align = "auto"
column-align-header = "center"
column-align-body = "left"
loose-last-column = true
max-width = 80

[MD073]
enabled = true
min-level = 2
max-level = 4
indent = 2
enforce-order = true
CONFIGEOF

TEMP_MD=$(mktemp)
echo "# Test" > "$TEMP_MD"

# Run rumdl and capture stderr for config warnings
CONFIG_WARNINGS=$(./target/release/rumdl check --no-cache --config "$TEMP_CONFIG" "$TEMP_MD" 2>&1 | grep -i "Unknown option" || true)
rm -f "$TEMP_CONFIG" "$TEMP_MD"

if [[ -z "$CONFIG_WARNINGS" ]]; then
    echo -e "${GREEN}✓${NC}"
else
    echo -e "${RED}✗${NC}"
    echo -e "${RED}ERROR: Config validation warnings found:${NC}"
    echo "$CONFIG_WARNINGS"
    echo ""
    echo "This usually means a rule's default_config_section() doesn't include all valid config keys."
    echo "Fix: Ensure all config keys (including optional ones) are included in the schema."
    ((ERRORS++))
fi

# Summary
echo ""
echo "════════════════════════════════════════"
if $FIX_MODE && [[ $FIXED -gt 0 ]]; then
    echo -e "${GREEN}Fixed $FIXED issue(s) automatically.${NC}"
    echo "Review and commit the changes, then run again without --fix to verify."
    echo ""
fi
if [[ $ERRORS -eq 0 ]]; then
    echo -e "${GREEN}✅ Release is ready!${NC}"
    echo ""
    echo "Optional: Check for new notable projects using rumdl:"
    echo "  uv run scripts/update-used-by.py"
    echo ""
    echo "To create and push the release:"
    echo "  git tag -a v$CARGO_VERSION -m \"v$CARGO_VERSION\""
    echo "  git push origin main v$CARGO_VERSION"
else
    echo -e "${RED}❌ Release is NOT ready ($ERRORS errors)${NC}"
    echo "Fix the errors above before tagging"
    exit 1
fi
