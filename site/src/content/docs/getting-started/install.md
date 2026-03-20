---
title: Installation
description: How to install and verify mxr.
---

## Requirements

- macOS or Linux (Unix domain sockets required)
- A truecolor terminal (recommended)
- `$EDITOR` set to your preferred editor (vim, neovim, helix, etc.)

SQLite is bundled — no separate install needed. Rust is only needed if building from source.

## Homebrew (macOS / Linux)

```bash
brew tap planetaryescape/mxr
brew install mxr
```

## Pre-built binaries

Download from [GitHub Releases](https://github.com/planetaryescape/mxr/releases/latest):

- macOS Apple Silicon (aarch64)
- macOS Intel (x86_64)
- Linux x86_64

Extract and place `mxr` in your `$PATH`:

```bash
tar xzf mxr-v*.tar.gz
cp mxr ~/.local/bin/  # or /usr/local/bin
```

## Cargo (from git, requires Rust)

```bash
cargo install --git https://github.com/planetaryescape/mxr
```

## Build from source (requires Rust 1.75+)

```bash
git clone https://github.com/planetaryescape/mxr
cd mxr
cargo build --release
cp target/release/mxr ~/.local/bin/
```

## Verify installation

```bash
mxr version
mxr doctor --check
mxr --help
```

## Next

- [Gmail Setup](/getting-started/gmail-setup/) for Gmail accounts
- [IMAP / SMTP Setup](/getting-started/imap-smtp-setup/) for any other provider
- [First Sync](/getting-started/first-sync/)
