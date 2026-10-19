# Define a lossless title-length policy for linked Microsoft tasks

Status: open, nonblocking hardening. Source: independent review of
`19aaadfc70472c29a0a0a8defc56852469ae24f7` (DeepSeek V4 Pro), should-fix 3.

## Concrete trigger

Edit a linked title in Microsoft To Do to more than 120 characters, then sync
into Mixer. `handler/microsoft_todo.rs::local_update` preserves it, rejecting only
empty titles and titles larger than 32,768 bytes. The local create/edit paths in
`crates/daemon/src/handler/todos.rs` clip to 120 characters. This is a local input
policy, not a database constraint: migration 059 stores `title TEXT NOT NULL`.
TUI row rendering already truncates to its viewport width, and web renders title
text with wrapping; storage growth is bounded but the policies differ.

## Required follow-up

Decide whether linked titles should preserve the longer remote value or pause
with a visible explanation above a shared limit. Do not blindly clip inbound
titles: the three-way merge baseline contains the full remote title, so clipping
would create a local change on the next sync and could send truncated data back
to Microsoft. Any policy change needs round-trip tests at 120/121 characters,
multibyte byte/character boundaries, unchanged follow-up sync, and concurrent
local/remote edits. Confirm the desired product behavior before implementation.

Current behavior is deliberately unchanged; no title data is truncated by the
review closeout. No real Microsoft tasks were used for this investigation.
