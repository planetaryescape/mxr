# mxr-core

Internal provider-agnostic types, IDs, and provider traits for mxr.

`mxr-core` is not published to crates.io. Use it inside the mxr
workspace when you are adding or changing provider adapters.

## What It Contains

- typed ids
- account, message, label, draft, and sync types
- provider traits for sync and send adapters
- shared error types

## Build an Adapter

Implement the provider traits from this crate:

- `MailSyncProvider` for inbox sync, read, and mutation support
- `MailSendProvider` for outbound mail

```rust
use mxr_core::{MailSendProvider, MailSyncProvider};
```

Keep provider-specific protocol behavior inside the provider crate. The
daemon should talk to the adapter through these traits.

`mxr-core` does not implement Gmail, IMAP, SMTP, SQLite, search, or
daemon IPC behavior.

## Verification

From the repository root, run the core tests and the reusable
fake-provider conformance checks:

```bash
scripts/cargo-test -p mxr-core --tests
scripts/cargo-test -p mxr-provider-fake --tests
cargo test --workspace provider_offline_smoke_
```

Main repository: <https://github.com/planetaryescape/mxr>
