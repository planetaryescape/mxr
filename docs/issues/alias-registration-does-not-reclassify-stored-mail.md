# Registering an alias does not reclassify mail already stored from it

**Status:** fixed · **Found:** cross-model review of PR #631 (round 2) · **Severity:** should-fix

Fixed on `fix/alias-reclassify-stored-mail`: see the `fix:` commit that adds
`Store::reclassify_account_directions`. The address commands and the config
reload now reclassify the account's stored mail against its committed owned set.

## What happened

Sync stores each message's direction when it arrives. Reclassification
(`Store::reclassify_unknown_directions`, `crates/store/src/analytics.rs`) only
revisits rows whose direction is Unknown. Adding an alias
(`add_account_address` in `crates/daemon/src/handler/diagnostics/mod.rs`)
rebuilt the registry but did not revisit stored rows. So mail sent from an
alias before registering it stayed Inbound, and removing an alias left mail
from it Outbound.

## How it is fixed

- `add`, `remove` and `set-primary` refresh the registry, then reclassify the
  account from the committed store set. Inbound rows from an owned sender become
  outbound, and outbound rows from an unowned sender become inbound. Unknown
  rows are left to the existing repair.
- In the same writer transaction, each flipped reply's `reply_pairs` row is
  rebuilt from its new direction, and arrivals for mail now sent are dropped
  unless the user corrected them.
- The contacts table is queued for a refresh when any row changed.
- The config reload path reclassifies each account whose owned set changed.
- The response and CLI output report `to_outbound` and `to_inbound`. Add and
  remove also take `dry_run`, and `--dry-run` previews the same counts.
  `set-primary` never changes which addresses are owned, so its counts are zero.

## Residual risks

- Add and remove take `--dry-run`. It runs the same selection against the owned
  set the change would produce, and writes nothing. Set-primary has no dry run:
  it never changes membership.
- A sync pass that loaded the registry before the refresh can still write an
  Inbound row for an alias after the reclassify. Nothing repairs that row until
  the next alias change.
- Removing an alias does not create arrivals for its recent mail. The arrivals
  trigger fires only on insert, so those messages stay out of arrival counts.
  Creating them needs a decision on how old mail should place.
- A config reload that fails to reclassify logs a warning and is not retried.
