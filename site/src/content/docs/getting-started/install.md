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
brew install planetaryescape/mxr/mxr
```

Equivalent to `brew tap planetaryescape/mxr && brew install mxr` if you prefer to tap explicitly.

## Pre-built binaries

Download from [GitHub Releases](https://github.com/planetaryescape/mxr/releases/latest):

- macOS Apple Silicon (aarch64)
- Linux x86_64

Extract and place `mxr` in your `$PATH`:

```bash
tar xzf mxr-v*.tar.gz
cp mxr ~/.local/bin/  # or /usr/local/bin
```

### macOS Gatekeeper

V1 release binaries may be unsigned. macOS can show "Apple could not verify
`mxr`" on first run. That is accepted for v1 distribution; it means the binary
is not notarized with Apple, not that mxr phones home. If you trust the GitHub
Release you downloaded, remove the quarantine bit once:

```bash
xattr -d com.apple.quarantine ~/.local/bin/mxr
mxr --version
```

Homebrew and `cargo install` avoid some of this friction because they build or
install through a path macOS already knows.

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
