# Recipient briefing sends a throwaway account id

## Where

`crates/tui/src/runner.rs`, the `BriefingRequest::Recipient` arm that builds `Request::GetRecipientBriefing`.

## Problem

The TUI builds `account_id` with `mxr_core::AccountId::new()`, a fresh random id, and relies on the daemon handler to validate it against the contacts table. A random id matches no account. A comment there said the TUI "uses the default account on the client side" and called the placeholder temporary. The code does not do that.

## Smallest fix

Send the account of the focused envelope (the same source the thread briefing uses), and have the daemon reject an unknown id. Add a test that the request carries the focused message's account.
