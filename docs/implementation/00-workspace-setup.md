# 00 — Workspace Setup

> **Current Layout Note**
> This plan describes the historical workspace bootstrap. Current code still uses a Cargo workspace, but `mxr-*` names under `crates/` are real internal crates again, with the repo-root package `mxr` as the install surface.

## Goal

Establish the Cargo workspace, toolchain, CI pipeline, and project scaffolding. After this step, `cargo check --workspace` passes on an empty workspace with all crate stubs.

## Prerequisites

- Rust stable toolchain installed
- Git initialized

## Implementation Steps

### Step 1: Toolchain and Config Files

**Files to create:**

`rust-toolchain.toml`:
```toml
[toolchain]
channel = "stable"
```

`.gitignore`:
```
/target
*.db
*.db-wal
*.db-shm
search_index/
.env
```

`LICENSE-MIT`: Standard MIT license text with "mxr contributors" as copyright holder.

`LICENSE-APACHE`: Standard Apache-2.0 license text.

`.github/workflows/ci.yml`:
```yaml
name: CI

on:
  push:
    branches: [main]
  pull_request:

env:
  CARGO_TERM_COLOR: always
  RUSTFLAGS: -Dwarnings

jobs:
  check:
    name: Check
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
      - run: cargo check --workspace --all-targets

  fmt:
    name: Format
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: rustfmt
      - run: cargo fmt --all -- --check

  clippy:
    name: Clippy
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: clippy
      - uses: Swatinem/rust-cache@v2
      - run: cargo clippy --workspace --all-targets -- -D warnings

  test:
    name: Test
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
      - run: cargo test --workspace

  build:
    name: Build
    strategy:
      matrix:
        os: [ubuntu-latest, macos-latest]
    runs-on: ${{ matrix.os }}
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
      - run: cargo build --workspace --release
```

### Step 2: Workspace Cargo.toml

**File:** `Cargo.toml` (workspace root, virtual manifest)

```toml
[workspace]
resolver = "2"
members = [
    "crates/core",
    "crates/store",
    "crates/search",
    "crates/protocol",
    "crates/provider-fake",
    "crates/sync",
    "crates/daemon",
    "crates/tui",
]

[workspace.package]
version = "0.1.0"
edition = "2021"
license = "MIT OR Apache-2.0"
rust-version = "1.75"

[workspace.dependencies]
# Internal crates
mxr-core = { path = "crates/core" }
mxr-store = { path = "crates/store" }
mxr-search = { path = "crates/search" }
mxr-protocol = { path = "crates/protocol" }
mxr-provider-fake = { path = "crates/provider-fake" }
mxr-sync = { path = "crates/sync" }

# Async
tokio = { version = "1", features = ["full"] }

# Serialization
serde = { version = "1", features = ["derive"] }
serde_json = "1"

# Database
sqlx = { version = "0.8", features = ["runtime-tokio", "sqlite", "uuid", "chrono", "migrate"] }

# Search
tantivy = "0.22"

# TUI
ratatui = "0.29"
crossterm = "0.28"

# Types
uuid = { version = "1", features = ["v7", "serde"] }
chrono = { version = "0.4", features = ["serde"] }
bitflags = { version = "2", features = ["serde"] }

# Error handling
thiserror = "2"
anyhow = "1"

# Logging
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }

# Async traits
async-trait = "0.1"

# IPC framing
tokio-util = { version = "0.7", features = ["codec"] }
bytes = "1"

# CLI
clap = { version = "4", features = ["derive"] }

# Directories
dirs = "6"
```

**Note**: Pin major versions only. Let Cargo resolve exact minor/patch versions. Update if specific features require newer versions.

### Step 3: Crate Stubs

Create each crate with a minimal `Cargo.toml` and `src/lib.rs` (or `src/main.rs` for binaries).

**For each library crate** (`core`, `store`, `search`, `protocol`, `provider-fake`, `sync`):

```
crates/{name}/Cargo.toml
crates/{name}/src/lib.rs    # just `// TODO: implement`
```

Example `crates/core/Cargo.toml`:
```toml
[package]
name = "mxr-core"
version.workspace = true
edition.workspace = true
license.workspace = true

[dependencies]
serde = { workspace = true }
serde_json = { workspace = true }
uuid = { workspace = true }
chrono = { workspace = true }
bitflags = { workspace = true }
thiserror = { workspace = true }
async-trait = { workspace = true }
```

**For daemon** (binary crate that also has a library):

`crates/daemon/Cargo.toml`:
```toml
[package]
name = "mxr-daemon"
version.workspace = true
edition.workspace = true
license.workspace = true

[[bin]]
name = "mxr"
path = "src/main.rs"

[dependencies]
mxr-core = { workspace = true }
mxr-store = { workspace = true }
mxr-search = { workspace = true }
mxr-protocol = { workspace = true }
mxr-provider-fake = { workspace = true }
mxr-sync = { workspace = true }
tokio = { workspace = true }
tracing = { workspace = true }
tracing-subscriber = { workspace = true }
anyhow = { workspace = true }
serde_json = { workspace = true }
tokio-util = { workspace = true }
bytes = { workspace = true }
clap = { workspace = true }
dirs = { workspace = true }
```

`crates/daemon/src/main.rs`:
```rust
fn main() {
    println!("mxr daemon stub");
}
```

**For TUI** — in Phase 0, the TUI is part of the same binary. The daemon crate produces the `mxr` binary which dispatches on subcommands. The TUI module lives inside the daemon crate initially, or in a separate `tui` library crate that the daemon binary imports.

Approach: Keep `crates/tui/` as a **library crate** with TUI rendering logic. The `mxr` binary in `crates/daemon/` imports it and runs TUI when no subcommand is given.

`crates/tui/Cargo.toml`:
```toml
[package]
name = "mxr-tui"
version.workspace = true
edition.workspace = true
license.workspace = true

[dependencies]
mxr-core = { workspace = true }
mxr-protocol = { workspace = true }
ratatui = { workspace = true }
crossterm = { workspace = true }
tokio = { workspace = true }
anyhow = { workspace = true }
tracing = { workspace = true }
```

### Step 4: README.md

**File:** `README.md`

Content: Project name, one-line pitch, architecture diagram (ASCII from blueprint), current status ("Phase 0: proving the architecture"), build instructions (`cargo build --workspace`), run instructions (`cargo run -- daemon --foreground` then `cargo run` in another terminal), license (MIT OR Apache-2.0), link to blueprint docs.

## Definition of Done

- `cargo check --workspace` passes
- `cargo fmt --all -- --check` passes
- `cargo clippy --workspace -- -D warnings` passes
- All crate stubs exist with correct dependencies declared
- CI workflow file exists
- README, licenses, gitignore, toolchain config exist

## Risks and Mitigations

| Risk | Mitigation |
|------|-----------|
| Dependency version conflicts between crates | Use workspace-level dependency declarations exclusively |
| sqlx requires DATABASE_URL at compile time for `query!` macro | Use runtime queries in Phase 0, defer compile-time checking to Phase 1 |
| Tantivy version incompatibility | Pin to 0.22.x (well-established), test schema creation early |
