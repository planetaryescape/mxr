# Typing a known contact in To crashes the web app

**Status:** open · **Found:** 2026-10-22 on `origin/main` b3717dbf2
(v0.6.142), reproduced on the v0.6.140 release against the demo mailbox ·
**Severity:** high: compose is unusable once a suggestion matches

## What happens

Typing a name that matches a contact in a compose To, Cc or Bcc field
replaces the app with "Something went wrong! Cannot read properties of
undefined (reading 'toLowerCase')".

## Why

The bridge's contacts autocomplete and its two readers disagree on the
field name:

- `SenderSummaryData` names the address `sender_email`
  (`crates/protocol/src/types/platform.rs:26`).
- `contacts_autocomplete` reads `sender.get("email")`
  (`crates/web/src/routes_v6.rs`, in the `ListSenders` loop), so the
  address is always empty and only display names match. It then returns
  the raw sender object, which has no `email`.
- `RecipientField.tsx:79` calls `item.email.toLowerCase()` on each
  suggestion, which throws on the first match.

## Fix

Read `sender_email` in the route and return a typed suggestion
(`{ name, email }`) instead of the raw sender value, so the web type is
generated rather than assumed. Add a bridge test that a query matching an
address returns it, and a Vitest test that a suggestion renders and is
picked.
