# Registering an alias does not reclassify mail already stored from it

**Status:** fixed · **Found:** cross-model review of PR #631 (round 2) · **Severity:** should-fix

Fixed on `fix/alias-reclassify-stored-mail` (PR #665). The address commands,
the config reload and the sync loop now reclassify the account's stored mail
against its committed owned set.

## What happened

Sync stores each message's direction when it arrives. Reclassification
(`Store::reclassify_unknown_directions`, `crates/store/src/analytics.rs`) only
revisits rows whose direction is Unknown. Adding an alias
(`add_account_address` in `crates/daemon/src/handler/diagnostics/mod.rs`)
rebuilt the registry but did not revisit stored rows. So mail sent from an
alias before registering it stayed Inbound, and removing an alias left mail
from it Outbound.

## How it is fixed

- `add`, `remove` and `set-primary` take the account's sweep gate, write, then
  reclassify the account from the committed store set. Inbound rows from an
  owned sender become outbound, and outbound rows from an unowned sender become
  inbound. Unknown rows are left to the existing repair.
- The gate also makes an in-flight Reading or Paper trail sweep recheck its
  selection after the change, so it does not archive mail that is now sent.
- Each flipped reply's `reply_pairs` row is rebuilt from its new direction, with
  its business-hours latency recomputed and any `reply_pair_pending` row cleared.
- Arrival history is not touched. See the decision below.
- The contacts refresh zeroes direction counters for contacts that the
  aggregate no longer produces, so a stale outbound total does not survive.
- Each address change owes its account a reconciliation. The owed set is in
  memory, and every stored account starts owed after a restart. After the
  account's next successful sync pass, the daemon reclassifies it again. That
  catches a batch prepared with the old set, and a reclassify that failed.
- The config reload path does the same for each changed account.
- The Wrapped cache is cleared when a change applies. The web app and the TUI
  refresh their mail and mode views on `AccountAddressesChanged`.
- The response and CLI output report `to_outbound` and `to_inbound`. Add and
  remove take `dry_run`, and `--dry-run` previews the same counts without
  writing anything.

## Decision: arrival history is not rewritten

`arrivals` records what arrived at sync time, and a user correction on an
arrival must survive. An address change therefore does not delete or insert
arrival rows. Consequences:

- Mail the account sent from an alias before it was registered can still be
  counted in an old arrival record.
- Removing an alias creates no arrival for its recent mail, because the arrival
  trigger fires only on insert. Those messages are missing from arrival counts.

## Residual risks

- Set-primary runs the same account-wide reconciliation. Its counts can be
  non-zero when rows from an earlier change are stale.
- Sync repairs a batch only after its pass finishes. A pass that is in flight
  while the change applies is corrected at that point, not before.
- Relationship profiles, resolution suggestions and some cached summaries are
  not refreshed by an address change. See
  `docs/issues/address-change-does-not-refresh-relationship-profiles-and-resolution-suggestions.md`.
- A config reload that fails its reclassify is retried on the account's next
  successful sync pass, not immediately.
