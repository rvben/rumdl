#!/bin/sh
set -eu

repo_root=$(git rev-parse --show-toplevel)
raw_frame=/tmp/rumdl-homepage-terminal-raw.png

cleanup() {
    rm -f "$raw_frame"
}
trap cleanup EXIT

cd "$repo_root"
vhs scripts/homepage-terminal.tape
ffmpeg \
    -loglevel error \
    -y \
    -i "$raw_frame" \
    -vf "crop=iw-52:ih-82:26:56,pad=iw:ih+24:0:12:color=#1e1e2e" \
    -frames:v 1 \
    docs/images/homepage-terminal.png
