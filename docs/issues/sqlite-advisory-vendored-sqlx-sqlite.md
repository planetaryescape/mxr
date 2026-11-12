# The bundled SQLite advisory needs a vendored sqlx-sqlite until sqlx 0.9.1

Status: fixed with a vendored patch, seen 2026-11-11. Delete the patch once a sqlx release (0.9.1 or later) with upstream commit 10462fa4414c is on crates.io and at least seven days old.

## What happened

RUSTSEC-2026-0459 (SQLite CVE-2026-119375) covers every `libsqlite3-sys` from 0.6.0 to 0.38.2. Their bundled SQLite (3.13.0 to 3.53.2) is affected. `libsqlite3-sys` 0.39.0 bundles SQLite 3.54.0 and 0.39.1 bundles 3.54.1. mxr built 0.37.0 into its release binaries through sqlx 0.9.0, and `cargo deny check advisories` failed on main.

The advisory reaches only a process that opens a database file, with its `-wal`, that a less trusted party could have written. mxr opens its own local store, so normal use was not exposed.

## Why a vendored copy

`sqlx-sqlite` 0.9.0, the newest release, requires `libsqlite3-sys >=0.30.1, <0.38.0`. Upstream widened that to `<0.40.0` in launchbadge/sqlx #4502 (commit 10462fa4414c, merged 2026-11-08), and that commit changes only the version range. No release has it yet. A git dependency on sqlx main would take three-day-old code and every other unreleased change since 0.9.0.

`vendor/sqlx-sqlite` is the crates.io 0.9.0 package, byte-identical to the tarball whose checksum the lock recorded (`488e99c3…`), with that one range changed. The workspace `[patch.crates-io]` points at it, and the lock pins `libsqlite3-sys` 0.39.1, released 2026-10-25.

Two tools have to skip the vendored crate's version, the same as `vendor/hnsw_rs`:

- `scripts/check_lockfile_versions.sh` lists both vendored crates in `excluded`.
- The Cargo.lock jsonpath in `release-please-config.json` skips both names. Without that, the next release would rewrite the copy's 0.9.0 to the workspace version and break every `--locked` build.

## SQLite 3.54 planned one query differently

`auto_reminders::tests::reply_lookups_use_indexes_not_scans` failed on SQLite 3.54.1. The thread branch of a reminder's reply lookup walked `idx_messages_account (account_id=? AND rowid>?)`, which covers every newer message in the account, instead of `idx_messages_thread`. `crates/store/src/auto_reminders.rs` now writes the account match as `+r.account_id = s.account_id`. The unary `+` keeps that term off any index, so the thread index is the only path.

## Removing it

1. Check that sqlx 0.9.1 or later is at least seven days old and that its `sqlx-sqlite` allows `libsqlite3-sys` 0.39.
2. Delete `vendor/sqlx-sqlite`, its `exclude` and `[patch.crates-io]` entries in `Cargo.toml`, and `sqlx-sqlite` from `scripts/check_lockfile_versions.sh`, its test and `release-please-config.json`.
3. Run `cargo update -p sqlx` and `cargo deny check advisories`.
