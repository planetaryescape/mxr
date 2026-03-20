---
title: Installation
description: How to install and verify mxr.
---

## Requirements

- Rust 1.75+
- SQLite 3.35+
- a Unix-like system with Unix domain sockets
- a truecolor terminal recommended
- an editor set through `$EDITOR`

## Install from source

```bash
git clone https://github.com/planetaryescape/mxr
cd mxr
cargo install --path crates/daemon
```

## Install from release artifacts

```bash
./install.sh v0.1.0
```

Release assets are also structured for:

- Homebrew installs
- `cargo binstall` installs from prebuilt assets

## Development run

```bash
cargo run -- daemon --foreground
cargo run
```

## Verify installation

```bash
mxr doctor --check
mxr status
mxr --help
```

## Next

- [Gmail Setup](/getting-started/gmail-setup/)
- [First Sync](/getting-started/first-sync/)
