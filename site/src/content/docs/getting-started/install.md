---
title: Installation
description: How to install mxr
---

## From source

```bash
git clone https://github.com/planetaryescape/mxr
cd mxr
cargo install --path crates/daemon
```

## Prerequisites

- Rust 1.75+
- SQLite 3.35+

## Verify installation

```bash
mxr version
mxr doctor
```
