---
title: Adapters
description: Provider model and current first-party adapter surface.
---

## Current adapters

- Gmail sync and send
- IMAP sync
- SMTP send
- Fake provider for tests and local development

## Contract

The daemon only talks to providers through `MailSyncProvider` and `MailSendProvider`. Provider-specific logic stays in adapter crates.

## Why this matters

- Local state stays provider-agnostic.
- Adapters can be swapped without rewriting search, store, or TUI code.
- Fake-provider fixtures exercise real daemon and sync flows in tests.

## Writing a new adapter

1. Create a new crate that depends only on `mxr-core`.
2. Implement one or both provider traits.
3. Map native provider state into the mxr internal model.
4. Validate behavior against the conformance and fixture suite.

See `CONTRIBUTING.md` and `docs/blueprint/03-providers.md` for the detailed contract.
