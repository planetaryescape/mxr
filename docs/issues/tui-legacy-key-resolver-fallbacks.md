# TUI key handling keeps a legacy resolver beside the key registry

## Where

- `crates/tui/src/app/input.rs`: the chord handling that replays unmatched keys "into the pure legacy key resolver", and the retired-alias handling next to it.
- `crates/tui/src/keybindings.rs`: the default tables that still hold keys the display registry does not list (undo is one).

## Problem

Two systems resolve keys. The registry (`crates/keymap/keymap.toml`) is the source for help, the hint bar and the palette. The legacy resolver still answers some keys: saved-search digits, invite comment chords, aliases that ignore modifiers (`#`, `!`). Rules that keep the two from fighting lived in comments: a remapped suffix must not steal a default chord, a retired destructive shortcut must not come back, a retired chord must not retry itself as a fresh prefix. Nothing but tests enforces them.

## Smallest fix

Move the remaining legacy keys into the registry, delete the fallback aliases, and keep the three rules as named tests.
