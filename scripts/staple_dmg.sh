#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 1 ]]; then
    echo "usage: $0 <dmg-path>" >&2
    exit 2
fi
dmg="$1"
trap 'exit 130' INT
trap 'exit 143' TERM

for attempt in 1 2 3; do
    if xcrun stapler staple "$dmg"; then
        xcrun stapler validate "$dmg"
        exit 0
    else
        status=$?
    fi
    if [[ "$status" -ne 68 || "$attempt" -eq 3 ]]; then
        exit "$status"
    fi
    if [[ "$attempt" -eq 1 ]]; then delay=5; else delay=15; fi
    echo "stapler staple exited 68; retrying after ${delay}s (attempt ${attempt}/3)" >&2
    sleep "$delay"
done
