#!/usr/bin/env bash
# Regenerates site/public/openapi.json from the live mxr-web crate.
#
# Update: dump-openapi-spec.sh [output.json]
# Check without changing the artifact: dump-openapi-spec.sh --check [output.json]
# Update mode retains the committed spec when Cargo is unavailable (for
# npm-only hosting). Check mode requires Cargo and never takes that fallback.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
mode=update
if [[ "${1:-}" == --check ]]; then
  mode=check
  shift
fi
if [[ $# -gt 1 || "${1:-}" == --* ]]; then
  echo "usage: $0 [--check] [output.json]" >&2
  exit 2
fi
OUT="${1:-$REPO_ROOT/site/public/openapi.json}"
# Resolve relative destinations before changing to the repository root.
if [[ "$OUT" != /* ]]; then OUT="$PWD/$OUT"; fi

if [[ "$mode" == check && ! -f "$OUT" ]]; then
  echo "[dump-openapi-spec] missing artifact: $OUT" >&2
  exit 1
fi

if ! command -v cargo >/dev/null 2>&1; then
  if [[ "$mode" == check ]]; then
    echo "[dump-openapi-spec] Cargo is required for --check; refusing the publishing fallback" >&2
    exit 2
  fi
  if [ -f "$OUT" ]; then
    echo "[dump-openapi-spec] cargo not found; using existing $OUT"
    exit 0
  fi
  echo "[dump-openapi-spec] cargo not found and no committed spec at $OUT — API explorer will be empty"
  exit 0
fi

scratch="$(mktemp -d "$(dirname "$OUT")/.mxr-openapi.XXXXXX")"
trap 'rm -rf "$scratch"' EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
if [[ "$mode" == check ]]; then cp "$OUT" "$scratch/before.json"; fi

cd "$REPO_ROOT"
# The queued Cargo wrapper can print scheduler status on stdout. Run the built
# exporter separately so only the JSON document reaches the generated artifact.
cargo build --example dump_openapi_spec -p mxr-web --locked --quiet >&2
target_dir="$(cargo metadata --locked --format-version 1 --no-deps | node -p 'JSON.parse(require("fs").readFileSync(0, "utf8")).target_directory')"
"$target_dir/debug/examples/dump_openapi_spec" > "$scratch/spec.json"
node "$REPO_ROOT/site/scripts/openapi-artifact.mjs" "$mode" "$OUT" "$scratch/spec.json" "$scratch/before.json"
