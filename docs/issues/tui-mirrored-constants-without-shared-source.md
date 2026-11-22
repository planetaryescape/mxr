# TUI copies values from the web app and the CLI by hand

## Where

- `crates/tui/src/ui/assistant_pane.rs`: the slow-step threshold equals `SLOW_STEP_SECONDS` in `apps/web/src/features/assistant/wire.ts`.
- `crates/tui/src/ui/updates_peek.rs`: `LONG_FACT`, the email-preview limits and the line patterns copy the web's `UpdateLineRow.tsx` and `emailPreview.ts`.
- `crates/tui/src/ui/todo_peek.rs`: field labels and order copy the web's `todoRows.ts` (`FIELD_LABELS`).
- `crates/tui/src/theme.rs`: account fills and ink copy the web's `--account-*` tokens.
- `crates/tui/src/app/analytics_actions.rs`, `wrapped_window_to_request`: copies `crates/daemon/src/commands/wrapped.rs` window logic.
- `crates/tui/src/app/list_advance.rs`: the next-row rule copies `apps/web/src/lib/listAdvance.ts`.
- `crates/tui/src/app/input.rs`: the four-second prefix wait matches the web app.

## Problem

Each value must match another client. Only a comment said so. If one side changes, the clients drift with no failing test.

## Smallest fix

Put each shared value in `crates/keymap` or the protocol crate, or add one test per value that reads the web or CLI constant and compares it.
