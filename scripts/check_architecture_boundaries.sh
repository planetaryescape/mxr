#!/usr/bin/env bash
set -euo pipefail

python3 - <<'PY'
from pathlib import Path
import sys
import tomllib

ROOT = Path.cwd()

ALLOW = {
    "mxr-core": set(),
    "mxr-protocol": {"mxr-core"},
    "mxr-store": {"mxr-core"},
    "mxr-search": {"mxr-core"},
    "mxr-semantic": {"mxr-core", "mxr-config", "mxr-reader", "mxr-store"},
    "mxr-sync": {"mxr-core", "mxr-store", "mxr-search"},
    "mxr-provider-fake": {"mxr-core"},
    "mxr-provider-gmail": {"mxr-core", "mxr-mail-parse", "mxr-outbound"},
    "mxr-provider-imap": {"mxr-core", "mxr-mail-parse"},
    "mxr-provider-smtp": {"mxr-core", "mxr-outbound"},
    "mxr-tui": {
        "mxr-compose",
        "mxr-config",
        "mxr-core",
        "mxr-mail-parse",
        "mxr-protocol",
        "mxr-reader",
    },
    "mxr-web": {
        "mxr-compose",
        "mxr-config",
        "mxr-core",
        "mxr-mail-parse",
        "mxr-protocol",
    },
}

errors = []
for cargo_toml in [ROOT / "Cargo.toml", *sorted((ROOT / "crates").glob("*/Cargo.toml"))]:
    data = tomllib.loads(cargo_toml.read_text())
    package = data.get("package", {}).get("name")
    if package not in ALLOW:
        continue
    deps = {
        name
        for name in data.get("dependencies", {})
        if name.startswith("mxr-")
    }
    disallowed = sorted(deps - ALLOW[package])
    if disallowed:
        rel = cargo_toml.relative_to(ROOT)
        errors.append(f"{rel}: {package} has disallowed internal deps: {', '.join(disallowed)}")

if errors:
    print("Architecture boundary violations:", file=sys.stderr)
    for error in errors:
        print(f"  - {error}", file=sys.stderr)
    sys.exit(1)

print("Architecture boundaries ok")
PY
