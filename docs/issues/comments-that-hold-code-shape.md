# Comments that hold code shape

The delete-comment sweep kept some comments only because deleting them changes the code or trips a lint. Each one points at code whose shape should change, so the comment can go.

## Clippy

- `crates/daemon/src/handler/account_config.rs`, the default-account `if`/`else if`: both branches set `default_account`. Without the comment, `clippy::if_same_then_else` fires. Merge the two conditions into one.
- `crates/daemon/src/activity/mod.rs`, the `match write_entry_returning_id(...)` with an empty `None` arm: without the comment, `clippy::single_match` fires. Use `if let Some(id)`.
- `crates/daemon/src/logging.rs`, `Drop for Record`: the nested `if` holds only another `if`. Without the comment, `clippy::collapsible_if` fires. Collapse the conditions.
- `crates/tui/src/app/mod.rs`, `App::set_connection_state`: same `collapsible_if` case. See also `tui-connection-modal-matched-by-title.md`.

## Formatters

rustfmt or oxfmt reflows code when these go. The slice issues list them:

- `daemon-handler-comments-that-hold-rustfmt-formatting.md`
- The kept comments in `crates/compose/src/lib.rs` (`build_draft_file`), `crates/mail-parse/src/arrival.rs`, `crates/sync/src/engine.rs`, `crates/export/src/mbox.rs`, `crates/reader/src/html.rs`, `crates/rules/src/condition.rs`, `crates/web/src/tests.rs`, `crates/store/src/sync_cursor.rs`, `apps/web/src/features/messages/MessagesRoute.tsx`, `apps/web/scripts/gen-bridge-types.mjs`, `site/astro.config.mjs` and `scripts/check-npm-audit.mjs`.

Delete each comment together with the reshape that makes it unnecessary, then lower the ratchet.
