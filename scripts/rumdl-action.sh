#!/usr/bin/env bash

set -eou pipefail

# Version: use input or default to latest
rumdl_version="${GHA_RUMDL_VERSION:-}"

echo
if [ -n "$rumdl_version" ]; then
    echo "Installing rumdl (v$rumdl_version)"
    pip install rumdl=="$rumdl_version"
else
    echo "Installing rumdl (latest)"
    pip install rumdl
fi

echo
echo "Linting markdown with rumdl"
echo "Working directory: $(pwd)"
echo "Lint path: ${GHA_RUMDL_PATH:-$GITHUB_WORKSPACE}"

# Path: use input or default to workspace root
lint_path="${GHA_RUMDL_PATH:-$GITHUB_WORKSPACE}"

# Build rumdl command arguments
rumdl_args=()

# Config file - convert to absolute path for compatibility with all rumdl versions
if [ -n "${GHA_RUMDL_CONFIG:-}" ]; then
    config_path="$GHA_RUMDL_CONFIG"
    if [[ ! "$config_path" = /* ]]; then
        config_path="$(pwd)/$config_path"
    fi
    echo "Config file: $config_path"
    rumdl_args+=("--config" "$config_path")
fi

# Output format
case "$GHA_RUMDL_REPORT_TYPE" in
"logs")
    rumdl_args+=("--output-format" "full")
    ;;
"annotations")
    rumdl_args+=("--output-format" "github")
    ;;
*)
    echo
    echo "::error:: invalid report type: $GHA_RUMDL_REPORT_TYPE"
    echo "report type should be one of: logs, annotations"
    exit 1
    ;;
esac

# Validate fail-on-error input early (fail-fast)
fail_on_error="${GHA_RUMDL_FAIL_ON_ERROR:-true}"
if [ "$fail_on_error" != "true" ] && [ "$fail_on_error" != "false" ]; then
    echo "::error::Invalid fail-on-error value: $fail_on_error (must be 'true' or 'false')"
    exit 1
fi

# Log settings for visibility
if [ "$fail_on_error" = "false" ]; then
    echo "Informational mode: violations will not fail the workflow"
fi
if [ -n "${GHA_RUMDL_OUTPUT_FILE:-}" ]; then
    echo "Output file: $GHA_RUMDL_OUTPUT_FILE"
fi

# Run rumdl and capture output
set +e
results=$(rumdl check "$lint_path" "${rumdl_args[@]}" 2>&1)
exit_code=$?
set -e

# Always print output
echo "$results"

# Write to output file if requested
if [ -n "${GHA_RUMDL_OUTPUT_FILE:-}" ]; then
    output_dir=$(dirname "$GHA_RUMDL_OUTPUT_FILE")
    if [ "$output_dir" != "." ] && [ ! -d "$output_dir" ]; then
        mkdir -p "$output_dir"
    fi
    if ! echo "$results" > "$GHA_RUMDL_OUTPUT_FILE"; then
        echo "::error::Failed to write results to: $GHA_RUMDL_OUTPUT_FILE"
        exit 1
    fi
    echo "Results written to: $GHA_RUMDL_OUTPUT_FILE"
fi

# For annotations mode, re-print annotations for GitHub to pick up
if [ "$GHA_RUMDL_REPORT_TYPE" = "annotations" ] && [ $exit_code -ne 0 ]; then
    echo "$results" | grep '::' || true
fi

# Control exit behavior based on fail-on-error setting
if [ "$fail_on_error" = "true" ]; then
    exit $exit_code
else
    # Informational mode: always exit 0, but report if violations were found
    if [ $exit_code -ne 0 ]; then
        echo "::notice::Lint violations found (informational mode, not failing workflow)"
    fi
    exit 0
fi
