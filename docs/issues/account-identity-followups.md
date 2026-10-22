# Account identity: follow-ups from review round 2

**Status:** open · **Found:** review of `feat/account-identity` (blueprint 33)
· **Severity:** low to medium · Logged, not fixed, in this change.

## The identity write saves an environment override as config

`mxr accounts identity` loads the config with environment overrides applied,
then saves the whole config back. If `MXR_DEFAULT_ACCOUNT` is set, the file
gets `general.default_account` from the environment, which is not what the
user typed.

- Writer: `crates/daemon/src/handler/account_config.rs:403-405`
  (`load_config` then `save_config` in `set_account_identity`).
- Cause: `apply_env_overrides` runs inside `load_config_from_path`
  (`crates/config/src/resolve.rs:660-669`), so the shared writer serializes
  the effective config.
- The same shared writer also drops comments and writes the file
  non-atomically. That predates this change and is not specific to identity.

Fix direction: have the writer load the file without environment overrides.
That belongs in the shared config writer, not in identity code.

## A racing account insert can leave a refused override in place

The identity plan is computed before the config lock is taken
(`crates/daemon/src/handler/account_config.rs:376` plans, `:402-405` saves
under the lock). If another account is inserted between the two, the plan
can accept a tag the new account has just claimed. The stored tags are
unique by convention, not by the database: `accounts.identity_tag` has no
`UNIQUE` constraint (`crates/store/src/pool.rs:1274`).

Fix direction: re-run the clash check inside the locked section against the
store, and consider a partial unique index on `identity_tag`.

## The backfill scans every account on every store open

`Store::backfill_account_identity` (`crates/store/src/account.rs:342`) reads
all accounts and runs on every open (`crates/store/src/pool.rs:199`). It is
cheap at realistic account counts. It becomes worth a guard (for example, a
`WHERE identity_tag IS NULL OR identity_color IS NULL` query) only if the
account table grows far beyond a handful of rows.

## A committed phase that then fails is recorded as a failure

The activity seam records an entry only when the response is ok
(`crates/daemon/src/handler/mod.rs`, `response_succeeded`, near line 2824).
`AccountOperationResult.ok == false` also covers changes that were already
committed and then failed, such as a config save that succeeded followed by a
runtime reload or cached-mail processing that did not. Those changes lose
their activity entry, although the config really changed.

Fix direction: report the committed phases separately (for example, a
`saved` flag beside `ok`) and record activity when a save happened.
