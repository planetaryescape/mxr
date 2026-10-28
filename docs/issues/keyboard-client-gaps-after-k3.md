# Keys one client has and the other lacks after K3

Status: keys done 2026-10-28 (branch `feat/keymap-k3-followups`); Snoozed, wake and the Updates peek done 2026-10-28 (branch `fix/tui-snoozed`); the other TUI feature gaps below stay open. Found 2026-10-28 while emptying the hand-kept web/TUI difference list (branch `feat/keymap-k3-parity`, blueprint 35 K3).

The parity check in `crates/keymap/src/parity.rs` compares the clients from `keymap.toml` alone. It now accepts no `[[tui_pass_through]]` entry, and it compares the overlays both clients share too.

## Web gaps behind the TUI pass-through entries: done

All three pass-through entries are deleted.

- `h` and `←` in the modes: done. `list.sidebar` is live in Now, Messages, To do, Updates, Reading and Archive in the web too, and gives the keyboard to the sidebar, which is where the TUI's `h` goes from a mode.
- The `*` selection prefix: done in the reader and the sidebar. The web's keys there reach the mail list beside them (`apps/web/src/lib/actions/registry.ts`, the pane-beside fallback), and the TUI's `*` there selects in the list instead of saying it can't. In the modes neither client has a selection, so `*` left the TUI's modes.
- The `i` invitation prefix: done in the sidebar, which answers the list's lit invite in both clients. In the modes the TUI's `i` never had a message to answer, so it left them.
- List keys from the sidebar (`A`, `F`, `V`, `b`, `x`, `Ctrl-d`/`u`/`f`, `PageDown`/`PageUp`, `Esc`): done, through the same pane-beside fallback. The web's list also takes the reader's `F`, `O`, `R` and `S`. The TUI reader's `Enter`, which only reopened the open conversation, left the TUI.

None of the keys that left the TUI got a moved-key entry: none of them did anything there.

## TUI gaps behind web-only actions: partly done

These are features, not keys. They are marked web-only in the definition, each with a `note`, because the TUI has no matching screen or behaviour:

- The Focus page (`g F`). The TUI starts focus from the reply queue.
- The Snoozed list (`g n`): done (branch `fix/tui-snoozed`). It is the mail list over `ListSnoozed` and the envelopes it names, the requests the web's page makes, waking soonest first, one row per conversation with the wake time in place of the date and "Back <time>" in place of the snippet. `Enter` opens the message, and the list's keys (archive, star, label, `b`, `x`, `*`, `V`) act on the lit conversation as in any list.
- Folding quoted text and collapsing messages in the reader (`o`, `[`, `]`).
- Archive and step to the next one in the reader (`Q`, `;`).
- Peeking at an Updates item (`Space`): done (branch `fix/tui-snoozed`). `Space` opens the lit line's details under it, as the web's peek: the whole fact when the row cuts it, when the latest email arrived and its first lines (fetched as the reader fetches them, plain text, picked by the web's rules), why it is here, the numbers, a tracker's state, a parcel's items and references, the time, and where `L` and `o` go. `Space` again or `Esc` closes it.
- The row action `w` (wake) on the Snoozed and reply-queue pages: done (branch `fix/tui-snoozed`). `w` is Wake in blueprint 35's verb table: in Snoozed it wakes every snoozed message in the lit conversation (`Unsnooze`, as `mxr unsnooze`), and in the TUI's reply queue it takes the lit message out of the queue (`SetReplyLater` off), as the web's page does. The daemon's wake has no undo; `b` sets either again. The web's desk (`/desk`) no longer runs Done on `w`: Done there is `e`, and `w` says so until 0.6.206 (a move on `mail.archive` in the `desk` place).

## Overlay grammar leftovers: done

- The resume-account-form prompt: done. It offers "Resume draft" (lit) and "Discard draft" (named), so only `y` or `Enter` on "Discard draft" throws the form away; `n`, `Esc` and `q` keep it for later, and `c` still resumes.
- A bare `Enter` in the TUI send review: done. It runs the lit option, marked `›`: Send, or Edit again when the safety check found something or the draft can't go as it is, the way the web's send confirm opens on "Back to draft".
- Unsubscribe's "and archive" option: done. It is `e` (and `E`) in both clients; `a` and `A` say where it went until 0.6.204.
- Overlay keys: done. Every overlay is a context in the definition (`OVERLAY_CONTEXTS` in `crates/keymap/src/lib.rs`), so TUI help, the keybindings page's TUI tables and its web "Dialogs" tables are generated, and the hand-written tables are gone. `runner::tests::overlay_keys` holds each TUI overlay's handler to its entries. The web's dialogs read their keys by id (`apps/web/src/lib/keys/overlayKeys.ts`), except keys the browser or the dialog primitive gives, which the definition marks `web_native`.

The web's help, palette, label picker, Categories and Reply later are filter lists where letters type, and its bulk and sweep previews are two-button dialogs, so those overlays are listed per client and not compared (`SHARED_OVERLAYS` in `parity.rs` says why).
