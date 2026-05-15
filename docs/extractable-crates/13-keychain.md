---
candidate: keychain
status: skip
decision: skip
mxr_source: crates/keychain/
last_reviewed: 2026-05-15
---

# `mxr-keychain` — **Skip**

> OS-keychain credential storage (macOS Security.framework, Linux Secret
> Service, Windows Credential Manager).

## Decision: **Skip**

The ecosystem already has a well-maintained crate that covers this
problem completely. Publishing a competitor would be a vanity exercise
that fragments the space without adding value.

## What mxr has today

**Source:** `crates/keychain/`

A small wrapper providing:

```rust
pub fn get_password(service: &str, account: &str) -> Result<String, KeychainError>;
pub fn set_password(service: &str, account: &str, password: &str)
    -> Result<(), KeychainError>;
pub enum KeychainError { /* ... */ }
```

Internally uses `security-framework` on macOS and the `keyring` crate
elsewhere. Adds typed errors and minor logging. ~100 lines of Rust.

The crate is well-written and useful inside mxr. It is not differentiated
externally.

## Ecosystem state

| Crate | Status |
|---|---|
| [`keyring`](https://github.com/hwchen/keyring-rs) | Active, multi-platform, broad adoption |
| [`keyring-core`](https://github.com/open-source-cooperative/keyring-core) | New foundation maintained by the OSS Cooperative |
| `security-framework` | Apple-specific, low-level — keyring sits on top |
| `secret-service` | Linux-specific D-Bus interface — keyring sits on top |

The space is healthy. `keyring` is the canonical choice for Rust
applications needing OS-keychain access. There is no gap.

## Why ours doesn't justify a separate crate

- **Same primitive abstraction.** `get_password` / `set_password` is
  what `keyring` already exposes. Our wrapper isn't a different shape.
- **No platform we cover that `keyring` doesn't.**
- **No additional features.** Our error type is mxr-flavoured; users
  reaching for a keychain crate don't need our errors.
- **Maintenance overhead without payoff.** Publishing this means owning
  cross-platform keychain bugs forever for no audience benefit.

## What we'd be doing

If we published, our crate would compete with `keyring` while offering
strictly less. Users would (correctly) pick `keyring`. We'd accumulate
"why not just use keyring?" issues. Net negative.

## What to do instead

**Inside mxr**, the `mxr-keychain` workspace crate can either:

1. **Stay as-is.** It's a small adapter with typed errors specific to
   mxr. Workspace crates marked `publish = false` are fine.
2. **Get replaced by direct `keyring` usage**, removing the wrapper
   entirely. The wrapper's only value is the typed error enum; if that's
   not load-bearing, simplify and delete.

Neither requires publishing anything.

## When to re-evaluate

Trigger conditions to revisit:

1. `keyring` becomes unmaintained. (`keyring-core` formation suggests the
   opposite is happening — the OSS Cooperative is consolidating, not
   fracturing.)
2. mxr develops keychain features that don't fit `keyring`'s model
   (multi-credential rotation, secure-enclave attestation, etc.) and
   those features are generally useful. Currently none planned.
3. A platform appears that `keyring` doesn't support and we do. Currently
   none.

## Naming

Not applicable — we're not publishing.

## TL;DR

Use `keyring`. Done.
