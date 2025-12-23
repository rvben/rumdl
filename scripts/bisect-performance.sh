#!/bin/bash
# Git bisect script for finding performance regressions
#
# Usage:
#   git bisect start
#   git bisect bad <slow-commit>
#   git bisect good <known-fast-commit>
#   git bisect run ./scripts/bisect-performance.sh
#
# This script:
# 1. Builds rumdl in release mode
# 2. Generates a test document matching issue #148 pattern
# 3. Measures execution time
# 4. Returns exit 0 (good) if < threshold, exit 1 (bad) if >= threshold
#
# Environment variables:
#   BISECT_THRESHOLD - Time threshold in seconds (default: 5.0)
#   BISECT_ENTRIES   - Number of entries in test document (default: 500)

set -e

# Configuration
THRESHOLD="${BISECT_THRESHOLD:-5.0}"
ENTRIES="${BISECT_ENTRIES:-500}"
TEST_FILE="/tmp/bisect_perf_test.md"

echo "=== Performance Bisect Script ==="
echo "Threshold: ${THRESHOLD}s"
echo "Test entries: ${ENTRIES}"
echo ""

# Build in release mode
echo "Building release binary..."
if ! cargo build --release --quiet 2>/dev/null; then
    echo "ERROR: Build failed"
    # Skip this commit (neither good nor bad)
    exit 125
fi

RUMDL="./target/release/rumdl"

if [ ! -x "$RUMDL" ]; then
    echo "ERROR: Binary not found at $RUMDL"
    exit 125
fi

# Generate test document matching issue #148 pattern
# This pattern caused O(n²) behavior in has_mixed_list_nesting()
echo "Generating test document with $ENTRIES entries..."
cat > "$TEST_FILE" << 'HEADER'
# Work Log

HEADER

for i in $(seq 1 "$ENTRIES"); do
    day=$((i % 28 + 1))
    {
        printf -- "- day-%d: 2025-06-%02d\n" "$i" "$day"
        echo "  - task: 09:00-10:00"
        echo ">  Extra space after marker"
        echo "    - fix: add field"
        printf "    - fix: \"json_tag\": \"[%d]\"\n" "$i"
        echo "    - fix: \"local_field\": [\"record_id\"]"
    } >> "$TEST_FILE"
done

FILE_SIZE=$(wc -c < "$TEST_FILE" | tr -d ' ')
LINE_COUNT=$(wc -l < "$TEST_FILE" | tr -d ' ')
echo "Test file: $LINE_COUNT lines, $FILE_SIZE bytes"

# Measure execution time
echo "Running rumdl check..."

# Use 'time' to measure wall clock time
START=$(date +%s.%N)
$RUMDL check --no-cache "$TEST_FILE" > /dev/null 2>&1 || true
END=$(date +%s.%N)

# Calculate duration
DURATION=$(echo "$END - $START" | bc)

echo "Execution time: ${DURATION}s"

# Clean up
rm -f "$TEST_FILE"

# Compare against threshold
if (( $(echo "$DURATION >= $THRESHOLD" | bc -l) )); then
    echo ""
    echo "REGRESSION DETECTED: ${DURATION}s >= ${THRESHOLD}s threshold"
    echo "This commit is BAD"
    exit 1
else
    echo ""
    echo "OK: ${DURATION}s < ${THRESHOLD}s threshold"
    echo "This commit is GOOD"
    exit 0
fi
