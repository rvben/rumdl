#!/usr/bin/env bash

set -eou pipefail

# Version: use input or default to latest
rumdl_version="${GHA_RUMDL_VERSION:-}"
rumdl_cmd="rumdl"

install_via_pip() {
    if [ -n "$rumdl_version" ]; then
        echo "Installing rumdl (v$rumdl_version) via pip"
        pip install rumdl=="$rumdl_version"
    else
        echo "Installing rumdl (latest) via pip"
        pip install rumdl
    fi
    rumdl_cmd="rumdl"
}

# Prints "<target-triple> <archive-ext>" for this runner's OS/arch, or nothing if unmapped.
resolve_target() {
    local os_name arch_name platform_os platform_arch

    os_name="${RUNNER_OS:-}"
    case "$os_name" in
    Linux) platform_os="linux" ;;
    macOS) platform_os="macos" ;;
    Windows) platform_os="windows" ;;
    *)
        case "$(uname -s)" in
        Linux*) platform_os="linux" ;;
        Darwin*) platform_os="macos" ;;
        MINGW* | MSYS* | CYGWIN*) platform_os="windows" ;;
        *) platform_os="" ;;
        esac
        ;;
    esac

    arch_name="${RUNNER_ARCH:-}"
    case "$arch_name" in
    X64 | x86_64 | AMD64) platform_arch="x86_64" ;;
    ARM64 | arm64 | aarch64) platform_arch="aarch64" ;;
    *)
        case "$(uname -m)" in
        x86_64 | amd64) platform_arch="x86_64" ;;
        aarch64 | arm64) platform_arch="aarch64" ;;
        *) platform_arch="" ;;
        esac
        ;;
    esac

    case "${platform_os}-${platform_arch}" in
    linux-x86_64) echo "x86_64-unknown-linux-musl tar.gz" ;;
    linux-aarch64) echo "aarch64-unknown-linux-musl tar.gz" ;;
    macos-x86_64) echo "x86_64-apple-darwin tar.gz" ;;
    macos-aarch64) echo "aarch64-apple-darwin tar.gz" ;;
    windows-x86_64) echo "x86_64-pc-windows-msvc zip" ;;
    *) echo "" ;;
    esac
}

# Portable lowercase sha256 of $1: prefers sha256sum (Linux/Git Bash), falls back to
# shasum -a 256 (macOS default toolset does not guarantee sha256sum).
sha256_of() {
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum "$1" | awk '{print $1}'
    else
        shasum -a 256 "$1" | awk '{print $1}'
    fi
}

# Attempts the prebuilt-binary install for target triple $1 / archive extension $2.
# On success, sets rumdl_cmd to the binary's absolute path and returns 0.
# Returns 1 (graceful fallback to pip) only for the 3 defined cases: unparseable "latest"
# redirect, 404 on the asset, or 404 on its checksum. Any other failure (network error,
# unexpected HTTP status, checksum mismatch) is a hard exit — never silently falls back,
# since that would mask connectivity flakiness or a corrupted/tampered download as an
# unsupported-platform case.
try_install_binary() {
    local target="$1" ext="$2" tag effective_url curl_status

    if [ -n "$rumdl_version" ]; then
        tag="v${rumdl_version}"
    else
        curl_status=0
        effective_url=$(curl -fsSLo /dev/null --retry 3 -w '%{url_effective}' "https://github.com/rvben/rumdl/releases/latest") || curl_status=$?
        if [ "$curl_status" -ne 0 ]; then
            echo "::error::Could not reach GitHub to resolve the latest rumdl release (curl exit $curl_status)"
            exit 1
        fi
        if [[ "$effective_url" =~ /tag/(v[0-9][^/]*)$ ]]; then
            tag="${BASH_REMATCH[1]}"
        else
            echo "Latest-release redirect did not resolve to a parseable tag ($effective_url) — falling back to pip"
            return 1
        fi
    fi
    echo "Resolved release: $tag"

    local asset="rumdl-${tag}-${target}.${ext}"
    local checksum_asset="${asset}.sha256"
    local base_url="https://github.com/rvben/rumdl/releases/download/${tag}"
    local workdir
    workdir=$(mktemp -d) || exit 1

    local subshell_status=0
    (
        cd "$workdir" || exit 1

        echo "Downloading $asset"
        http_code=$(curl -sL --retry 3 -o "$asset" -w '%{http_code}' "${base_url}/${asset}" || echo "000")
        if [ "$http_code" = "404" ]; then
            echo "No prebuilt binary published for $target — falling back to pip"
            exit 2
        elif [ "$http_code" != "200" ]; then
            echo "::error::Failed to download $asset (HTTP $http_code)"
            exit 1
        fi

        echo "Downloading $checksum_asset"
        http_code=$(curl -sL --retry 3 -o "$checksum_asset" -w '%{http_code}' "${base_url}/${checksum_asset}" || echo "000")
        if [ "$http_code" = "404" ]; then
            echo "No checksum published for $asset — falling back to pip"
            exit 2
        elif [ "$http_code" != "200" ]; then
            echo "::error::Failed to download $checksum_asset (HTTP $http_code)"
            exit 1
        fi

        echo "Verifying checksum"
        expected_hash=$(awk '{print $1}' "$checksum_asset" | tr -d '\r' | tr '[:upper:]' '[:lower:]')
        actual_hash=$(sha256_of "$asset" | tr '[:upper:]' '[:lower:]')
        if [ "$expected_hash" != "$actual_hash" ]; then
            echo "::error::Checksum mismatch for $asset (expected $expected_hash, got $actual_hash)"
            exit 1
        fi

        echo "Extracting $asset"
        if [ "$ext" = "zip" ]; then
            if [ -x "/c/Windows/System32/tar.exe" ]; then
                /c/Windows/System32/tar.exe -xf "$asset"
            else
                powershell -NoProfile -Command "Expand-Archive -Path '$asset' -DestinationPath '.' -Force"
            fi
        else
            tar -xzf "$asset"
        fi
    ) || subshell_status=$?

    case "$subshell_status" in
    0) : ;;
    2) return 1 ;;
    *) exit "$subshell_status" ;;
    esac

    if [ "$ext" = "zip" ]; then
        rumdl_cmd="${workdir}/rumdl.exe"
    else
        rumdl_cmd="${workdir}/rumdl"
        chmod +x "$rumdl_cmd"
    fi
    echo "Installed rumdl binary: $rumdl_cmd"
    return 0
}

echo
target_info=$(resolve_target)
if [ -n "$target_info" ]; then
    if command -v curl >/dev/null 2>&1; then
        read -r target ext <<<"$target_info"
        if ! try_install_binary "$target" "$ext"; then
            install_via_pip
        fi
    else
        echo "curl not found — falling back to pip"
        install_via_pip
    fi
else
    echo "No prebuilt rumdl binary for RUNNER_OS='${RUNNER_OS:-}' RUNNER_ARCH='${RUNNER_ARCH:-}' — falling back to pip"
    install_via_pip
fi

echo
echo "Linting markdown with rumdl"
echo "Working directory: $(pwd)"
# Paths: split space-separated input into array, default to workspace root
read -ra lint_paths <<< "${GHA_RUMDL_PATH:-$GITHUB_WORKSPACE}"
echo "Lint path(s): ${lint_paths[*]}"

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

# Extra CLI arguments
extra_args=()
if [ -n "${GHA_RUMDL_ARGS:-}" ]; then
    read -ra extra_args <<< "$GHA_RUMDL_ARGS"
    rumdl_args+=("${extra_args[@]}")
    echo "Extra args: ${extra_args[*]}"
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
results=$("$rumdl_cmd" check "${lint_paths[@]}" "${rumdl_args[@]}" 2>&1)
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
