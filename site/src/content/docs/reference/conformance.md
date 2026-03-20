---
title: Conformance Tests
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
- initial sync returns messages with bodies
- cursors advance
- attachments can be fetched when present
- mutation methods succeed
- label management works when label support is advertised

## Send conformance

```rust
use mxr_provider_fake::conformance;

conformance::run_send_conformance(&provider).await;
```

The send checks validate:

- sending returns a receipt
- timestamps are sane
- draft save does not fail

## Canonical fixtures

`mxr-provider-fake::fixtures` also exports:

- canonical fixture dataset
- sample draft
- sample from-address

Use those to build provider-specific tests around your adapter.
