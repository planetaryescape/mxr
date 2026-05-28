---
title: Conformance tests
description: Reusable adapter checks exported from mxr-provider-fake.
---

## Purpose

`mxr-provider-fake` exports conformance helpers so adapter authors can verify that a provider satisfies the mxr trait contract before wiring it into the daemon.

## Sync conformance

```rust
use mxr_provider_fake::conformance;

conformance::run_sync_conformance(&provider).await;
```

The sync checks validate:

- `sync_labels` returns usable labels
- Initial sync returns messages with bodies
- Cursors advance
- Attachments can be fetched when present
- Mutation methods succeed
- Label management works when label support is advertised

## Send conformance

```rust
use mxr_provider_fake::conformance;

conformance::run_send_conformance(&provider).await;
```

The send checks validate:

- Sending returns a receipt
- Timestamps are sane
- Draft save does not fail

## Canonical fixtures

`mxr-provider-fake::fixtures` also exports:

- Canonical fixture dataset
- Sample draft
- Sample from-address

Use those to build provider-specific tests around your adapter.

## Package conformance

Some mxr behavior now comes from standalone public crates. Their conformance
tests live with the package repo, not inside the adapter suite. The package
repo is the source of truth for the portable contract; mxr owns only the local
execution policy around that contract.

| Behavior | Package | Source repo | mxr consumer |
|---|---|---|---|
| RFC 5256 / JWZ threading | [`mail-threading`](https://crates.io/crates/mail-threading) | [`planetaryescape/mail-threading`](https://github.com/planetaryescape/mail-threading) | `mxr-sync` |
| RFC 2369 / RFC 8058 unsubscribe headers | [`list-unsubscribe`](https://crates.io/crates/list-unsubscribe) | [`planetaryescape/list-unsubscribe`](https://github.com/planetaryescape/list-unsubscribe) | `mxr-mail-parse` |
| Gmail-style search parser and AST | [`mail-query`](https://crates.io/crates/mail-query) | [`planetaryescape/mail-query`](https://github.com/planetaryescape/mail-query) | `mxr-search` |
| mbox and Maildir formats | [`mailbox-formats`](https://crates.io/crates/mailbox-formats) | [`planetaryescape/mailbox-formats`](https://github.com/planetaryescape/mailbox-formats) | `mxr-export` |

Check which package backs a local behavior:

```bash
cargo tree -p mxr-sync -i mail-threading
cargo tree -p mxr-mail-parse -i list-unsubscribe
cargo tree -p mxr-search -i mail-query
cargo tree -p mxr-export -i mailbox-formats
```

Use the package repos for their portable JSON fixtures and coverage matrices.
Use `mxr-provider-fake::conformance` for provider adapter behavior.
