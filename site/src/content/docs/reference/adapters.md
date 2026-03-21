---
title: Adapters
description: Provider model, current adapter surface, and what each adapter is responsible for.
---

mxr keeps provider-specific logic in adapters. The rest of the app works against one internal model.

## Current adapter surface

| Adapter | Sync | Send | Labels / folders | Mutations | Notes |
|---|---|---|---|---|---|
| Gmail | yes | yes | labels | yes | direct Gmail adapter |
| IMAP | yes | no | folders | yes | usually paired with SMTP |
| SMTP | no | yes | no | no | send-only adapter |
| Fake | yes | yes | fixture labels | yes | tests and local development |

## Why this boundary matters

- Store and search do not need provider-specific code.
- The daemon can swap providers without being rewritten.
- New adapters have one contract to satisfy instead of a pile of hidden assumptions.

## Contract

The daemon talks to providers through `MailSyncProvider` and `MailSendProvider`.

That means:

- core defines the traits
- adapters depend on core
- daemon uses the traits, not provider-specific client types

## Conformance

`mxr-provider-fake` exports conformance helpers and canonical fixtures so adapter authors can test behavior before plugging anything into the daemon.

That suite is there to keep adapter work boring in the best way. If an adapter passes the contract, the rest of the system should not have to care where the mail came from.

## Writing a new adapter

1. Create a crate that depends only on `mxr-core`.
2. Implement one or both provider traits.
3. Map provider state into the mxr internal model.
4. Run the conformance and fixture tests.

For the detailed contract, see:

- [Conformance tests](/reference/conformance/)
- [Adapter development](/guides/adapter-development/)
- [Blueprint provider design](https://github.com/planetaryescape/mxr/blob/main/docs/blueprint/03-providers.md)
