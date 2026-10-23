#!/usr/bin/env bash
set -euo pipefail

python3 - <<'PY'
from pathlib import Path
import sys
import tomllib

ROOT = Path.cwd()

ALLOW = {
    # The in-app assistant's loop, gate and prompt. It reaches mail only
    # through the tools the daemon hands it, so it may never name a store,
    # protocol or provider crate.
    "mxr-assistant": {"mxr-llm"},
    "mxr-core": set(),
    "mxr-compose": {"mxr-core", "mxr-mail-parse", "mxr-outbound"},
    "mxr-config": {"mxr-core"},
    "mxr-export": {"mxr-core", "mxr-reader"},
    "mxr-humanizer": {"mxr-core"},
    "mxr-keychain": set(),
    # Every shortcut in every client (chapter 35). Plain data, read by the
    # daemon and the clients, so it may name no other mxr crate.
    "mxr-keymap": set(),
    "mxr-llm": set(),
    "mxr-mail-parse": {"mxr-core"},
    # Companion executable, not a core crate. The empty set is the boundary:
    # mxr-mailmerge composes with mxr by spawning the `mxr` CLI, so it can
    # never reach the store, credentials, or a provider. Adding any internal
    # dependency here fails CI, which is the point.
    "mxr-mailmerge": set(),
    "mxr-outbound": {"mxr-core"},
    "mxr-protocol": {"mxr-core"},
    "mxr-reader": set(),
    "mxr-records": {"mxr-core", "mxr-reader", "mxr-store", "mxr-todo"},
    # Reading: extraction, fade and ranking rules, and the guarded article
    # fetch. Plain data in and out; the daemon owns the store.
    "mxr-reading": {"mxr-llm", "mxr-reader"},
    "mxr-relationship": {"mxr-core", "mxr-llm", "mxr-reader", "mxr-store"},
    "mxr-rules": {"mxr-core"},
    "mxr-safety": {"mxr-core", "mxr-outbound", "mxr-reader", "mxr-relationship"},
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
    "mxr-todo": {"mxr-core", "mxr-reader", "mxr-store"},
    "mxr-updates": set(),
    "mxr-tui": {
        "mxr-client",
        "mxr-compose",
        "mxr-config",
        "mxr-core",
        "mxr-keymap",
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
