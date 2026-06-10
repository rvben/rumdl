#!/usr/bin/env bash
# Install mise-managed tools with retries, a per-attempt timeout, and
# recovery from corrupted rustup downloads.
#
# Usage: mise-install.sh [tool ...]
#   No arguments installs everything from .mise.toml; arguments are passed
#   to `mise install` verbatim (e.g. rust python cargo:maturin).
#
# Why this exists: plain `mise install` in CI has failed two distinct ways.
# A corrupted partial download in ~/.rustup/downloads makes every retry fail
# with "could not rename 'downloaded' file", and a stalled download hangs the
# job until the 6-hour workflow timeout. Each attempt therefore runs under a
# timeout, and the rustup download cache is cleared before retrying.
set -u

ATTEMPTS=3
ATTEMPT_TIMEOUT_SECS=600
RETRY_DELAY_SECS=10

# GNU timeout when available (Linux, git-bash); gtimeout on macOS runners
# (coreutils); otherwise run unguarded rather than fail.
run_with_timeout() {
  if command -v timeout >/dev/null 2>&1; then
    timeout "$ATTEMPT_TIMEOUT_SECS" "$@"
  elif command -v gtimeout >/dev/null 2>&1; then
    gtimeout "$ATTEMPT_TIMEOUT_SECS" "$@"
  else
    "$@"
  fi
}

for attempt in $(seq 1 "$ATTEMPTS"); do
  run_with_timeout mise install "$@" --yes
  status=$?
  if [ "$status" -eq 0 ]; then
    exit 0
  fi
  if [ "$status" -eq 124 ]; then
    echo "Attempt $attempt timed out after ${ATTEMPT_TIMEOUT_SECS}s" >&2
  else
    echo "Attempt $attempt failed (exit $status)" >&2
  fi
  # A failed or killed download can leave .partial files that poison every
  # subsequent attempt; clear them so the retry starts clean.
  rm -rf "${HOME}/.rustup/downloads" 2>/dev/null || true
  if [ "$attempt" -lt "$ATTEMPTS" ]; then
    echo "Retrying in ${RETRY_DELAY_SECS}s..." >&2
    sleep "$RETRY_DELAY_SECS"
  fi
done

echo "All $ATTEMPTS install attempts failed" >&2
exit 1
