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
