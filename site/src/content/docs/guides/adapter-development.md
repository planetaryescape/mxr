---
title: Adapter development
description: Build an mxr provider adapter against mxr-core.
---

## Contract

Adapters implement one or both traits from `mxr-core`:

- `MailSyncProvider`
- `MailSendProvider`

Keep provider-specific logic inside the adapter crate. Map native provider state into the mxr internal model instead of leaking provider concepts upward.

## Recommended workflow

1. Start from `examples/adapter-skeleton/`.
2. Depend on `mxr-core`.
3. Implement the sync/send traits.
4. Use `mxr-provider-fake::conformance` in your test suite.
5. Compare your mapping against the fake provider and the first-party IMAP adapter.

## Design rules

- Preserve provider IDs needed for round-trips.
- Normalize labels/folders/flags into mxr types.
- Keep auth and transport details inside the adapter crate.
- Report unsupported capabilities honestly.

## References

- `crates/provider-fake/`
- `crates/provider-imap/`
- `examples/adapter-skeleton/`
- `site/src/content/docs/reference/conformance.md`
