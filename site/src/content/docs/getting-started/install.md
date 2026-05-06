---
title: Installation
description: How to install and verify mxr.
---

## Requirements

- macOS or Linux (Unix domain sockets required)
- A truecolor terminal (recommended)
- `$EDITOR` set to your preferred editor (vim, neovim, helix, etc.)

SQLite is bundled, no separate install needed. Rust is only needed if building from source.

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

## Cargo (from source)

mxr is not published to crates.io. Install directly from the git repo:

```bash
# Latest main
cargo install --git https://github.com/planetaryescape/mxr --locked mxr

# A specific release tag (replace vX.Y.Z with the latest from the releases page)
cargo install --git https://github.com/planetaryescape/mxr --tag vX.Y.Z --locked mxr

# Or clone and install locally
git clone https://github.com/planetaryescape/mxr
cd mxr
cargo install --path . --locked
```

Building from source requires Rust 1.88+ (see `Cargo.toml` for the current MSRV).

## Verify installation

```bash
mxr --version
mxr doctor --check
mxr --help
```

## Next

- [Gmail setup](/getting-started/gmail-setup/) for Gmail accounts
- [IMAP / SMTP setup](/getting-started/imap-smtp-setup/) for any other provider
- [First sync](/getting-started/first-sync/)
