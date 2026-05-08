#!/usr/bin/env bash
# Regenerates site/public/openapi.json from the live mxr-web crate.
#
# Used by `npm run generate` so the API explorer (Scalar) and any
# direct curl of /openapi.json always reflect the current daemon
# surface. Falls back gracefully when cargo is unavailable (CI build
# servers without Rust, npm-only contributors): keeps any committed
# spec in place and prints a warning.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
OUT="$REPO_ROOT/site/public/openapi.json"

if ! command -v cargo >/dev/null 2>&1; then
  if [ -f "$OUT" ]; then
    echo "[dump-openapi-spec] cargo not found; using existing $OUT"
    exit 0
  fi
  echo "[dump-openapi-spec] cargo not found and no committed spec at $OUT — API explorer will be empty"
  exit 0
fi

cd "$REPO_ROOT"
cargo run --example dump_openapi_spec -p mxr-web --quiet > "$OUT"
echo "[dump-openapi-spec] wrote $(wc -c < "$OUT") bytes to $OUT"
