#!/usr/bin/env bash
set -euo pipefail

python3 - <<'PY'
from pathlib import Path
import sys
import tomllib

ROOT = Path.cwd()

ALLOW = {
    "mxr-core": set(),
    "mxr-compose": {"mxr-core", "mxr-mail-parse", "mxr-outbound"},
    "mxr-config": {"mxr-core"},
    "mxr-export": {"mxr-core", "mxr-reader"},
    "mxr-humanizer": {"mxr-core"},
    "mxr-keychain": set(),
    "mxr-llm": set(),
    "mxr-mail-parse": {"mxr-core"},
    "mxr-outbound": {"mxr-core"},
    "mxr-protocol": {"mxr-core"},
    "mxr-reader": set(),
    "mxr-relationship": {"mxr-core", "mxr-llm", "mxr-reader", "mxr-store"},
    "mxr-rules": {"mxr-core"},
    "mxr-safety": {"mxr-core", "mxr-reader", "mxr-relationship"},
    "mxr-store": {"mxr-core"},
    "mxr-search": {"mxr-core"},
    "mxr-semantic": {"mxr-core", "mxr-config", "mxr-reader", "mxr-store"},
    "mxr-sync": {"mxr-core", "mxr-store", "mxr-search"},
    "mxr-provider-fake": {"mxr-core"},
    "mxr-provider-gmail": {"mxr-core", "mxr-mail-parse", "mxr-outbound"},
    "mxr-provider-imap": {"mxr-core", "mxr-mail-parse"},
    "mxr-provider-outlook": {"mxr-core", "mxr-mail-parse", "mxr-outbound"},
    "mxr-provider-smtp": {"mxr-core", "mxr-outbound"},
    "mxr-test-support": set(),
    "mxr-tui": {
        "mxr-client",
        "mxr-compose",
        "mxr-config",
        "mxr-core",
        "mxr-mail-parse",
        "mxr-protocol",
        "mxr-reader",
    },
    "mxr-web": {
        "mxr-client",
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
