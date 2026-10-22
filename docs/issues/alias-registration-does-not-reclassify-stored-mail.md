# Registering an alias does not reclassify mail already stored from it

**Status:** open · **Found:** cross-model review of PR #631 (round 2) · **Severity:** should-fix

## What happens

Sync stores each message's direction when it arrives. Reclassification
(`Store::reclassify_unknown_directions`, `crates/store/src/analytics.rs`) only
revisits rows whose direction is Unknown. Adding an alias
(`add_account_address` in `crates/daemon/src/handler/diagnostics/mod.rs`)
rebuilds the registry but does not revisit stored rows. So mail you sent from
an alias before registering it stays Inbound for good.

The same outcome follows when an alias is registered while a sync pass or a
reclassify is running, whether or not the registry rebuild then fails: the
in-flight work finishes with the set it started with. PR #631 limits the
failure case (a stale registry stops new passes and repairs the Unknown rows it
caused) but cannot correct rows already written as Inbound.

## Fix sketch

After an alias is added, or the primary moves, reclassify that account's
Inbound rows whose From is now owned, through the same lookup, and Outbound
rows whose From is no longer owned after a removal. Preview the count first
(`--dry-run`), since direction drives screener routing and analytics.
