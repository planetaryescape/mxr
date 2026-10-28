# Keys one client has and the other lacks after K3

Status: open. Found 2026-10-28 while emptying the hand-kept web/TUI difference list (branch `feat/keymap-k3-parity`, blueprint 35 K3).

The parity check in `crates/keymap/src/parity.rs` now compares the clients from `keymap.toml` alone. What it still accepts falls into two groups. Each group hides a real gap, not a platform fact.

## Web gaps behind the TUI pass-through entries

The TUI shows panes side by side, so a key pressed in one pane can act in another. `[[tui_pass_through]]` entries let those keys through without a web match. On the web the same keys do nothing:

- `h` and `←` in the modes, which in the TUI move back to the list.
- The `*` selection prefix in the reader, the sidebar and the modes.
- The `i` invitation prefix in the sidebar and the modes.
- List keys pressed from the sidebar (`A`, `F`, `V`, `b`, `x`, `Ctrl-d`/`u`/`f`).

Each one either earns a web binding or leaves the TUI. Then its pass-through entry goes.

## TUI gaps behind web-only actions

These are marked web-only in the definition, each with a `note`, because the TUI has no matching screen or behaviour:

- The Focus page (`g F`). The TUI starts focus from the reply queue.
- The Snoozed list (`g n`).
- Folding quoted text and collapsing messages in the reader (`o`, `[`, `]`).
- Archive and step to the next one in the reader (`Q`, `;`).
- Peeking at an Updates item (`Space`).
- The row action `w` (wake) on the Snoozed and reply-queue pages. Blueprint 35 lists `w` as free, and nobody has decided what wake gets instead.

## Overlay grammar leftovers

- The TUI prompt to resume a new account's saved draft throws the draft away on `n`. Fixing it needs a named option that discards.
- A bare `Enter` does nothing in the TUI send review.
- Unsubscribe's "and archive" option uses `a` in both clients. The Pick rule makes it `e`, so the change needs moved-key entries.
- Overlay keys are still hand-written in TUI help and in `reference/keybindings.md`'s Overlays table. Moving them into the definition needs about 20 overlay contexts and a probe for each.
