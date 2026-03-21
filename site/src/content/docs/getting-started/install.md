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

## Build from source (requires Rust 1.75+)

`cargo install mxr` is temporarily unavailable while the crates.io publish path is being fixed.

```bash
git clone https://github.com/planetaryescape/mxr
cd mxr
cargo install --path crates/daemon --locked
```

## Verify installation

```bash
mxr version
mxr doctor --check
mxr --help
```

## Next

- [Gmail setup](/getting-started/gmail-setup/) for Gmail accounts
- [IMAP / SMTP setup](/getting-started/imap-smtp-setup/) for any other provider
- [First sync](/getting-started/first-sync/)
