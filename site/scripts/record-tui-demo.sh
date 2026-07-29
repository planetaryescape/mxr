#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$repo_root"
target_dir="$(cargo metadata --no-deps --format-version 1 | sed -n 's/.*"target_directory":"\([^"]*\)".*/\1/p')"
mxr_bin="$target_dir/debug/mxr"

command -v vhs >/dev/null || {
  echo "vhs is required: brew install charmbracelet/tap/vhs" >&2
  exit 1
}
command -v ffmpeg >/dev/null || {
  echo "ffmpeg is required: brew install ffmpeg" >&2
  exit 1
}

cargo build --bin mxr

recording_home="$(mktemp -d /tmp/mxr-demo.XXXXXX)"
cleanup() {
  status=$?
  HOME="$recording_home" \
    XDG_CONFIG_HOME="$recording_home/config" \
    XDG_DATA_HOME="$recording_home/data" \
    "$mxr_bin" demo stop >/dev/null 2>&1 || true
  if [ "$status" -eq 0 ] && [ "${MXR_KEEP_RECORDING_PROFILE:-0}" != "1" ]; then
    rm -rf -- "$recording_home"
  else
    echo "Recording profile kept: $recording_home" >&2
  fi
  return "$status"
}
trap cleanup EXIT

export HOME="$recording_home"
export XDG_CONFIG_HOME="$recording_home/config"
export XDG_DATA_HOME="$recording_home/data"

"$mxr_bin" demo --no-tui

PATH="$target_dir/debug:$PATH" vhs site/scripts/mxr-tui.tape
ffmpeg -y -ss 6 -i site/public/mxr-tui.webm -frames:v 1 -update 1 site/public/mxr-tui-poster.jpg

echo "Wrote site/public/mxr-tui.webm and site/public/mxr-tui-poster.jpg"
