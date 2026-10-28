# Keys one client has and the other lacks after K3

Status: keys done 2026-10-28 (branch `feat/keymap-k3-followups`); the TUI feature gaps below stay open. Found 2026-10-28 while emptying the hand-kept web/TUI difference list (branch `feat/keymap-k3-parity`, blueprint 35 K3).

The parity check in `crates/keymap/src/parity.rs` compares the clients from `keymap.toml` alone. It now accepts no `[[tui_pass_through]]` entry, and it compares the overlays both clients share too.

## Web gaps behind the TUI pass-through entries: done

All three pass-through entries are deleted.

- `h` and `←` in the modes: done. `list.sidebar` is live in Now, Messages, To do, Updates, Reading and Archive in the web too, and gives the keyboard to the sidebar, which is where the TUI's `h` goes from a mode.
- The `*` selection prefix: done in the reader and the sidebar. The web's keys there reach the mail list beside them (`apps/web/src/lib/actions/registry.ts`, the pane-beside fallback), and the TUI's `*` there selects in the list instead of saying it can't. In the modes neither client has a selection, so `*` left the TUI's modes.
- The `i` invitation prefix: done in the sidebar, which answers the list's lit invite in both clients. In the modes the TUI's `i` never had a message to answer, so it left them.
- List keys from the sidebar (`A`, `F`, `V`, `b`, `x`, `Ctrl-d`/`u`/`f`, `PageDown`/`PageUp`, `Esc`): done, through the same pane-beside fallback. The web's list also takes the reader's `F`, `O`, `R` and `S`. The TUI reader's `Enter`, which only reopened the open conversation, left the TUI.

None of the keys that left the TUI got a moved-key entry: none of them did anything there.

## TUI gaps behind web-only actions: open

These are features, not keys. They are marked web-only in the definition, each with a `note`, because the TUI has no matching screen or behaviour:

- The Focus page (`g F`). The TUI starts focus from the reply queue.
- The Snoozed list (`g n`).
- Folding quoted text and collapsing messages in the reader: done. The keys are `o` (one message), `;` (all) and `Q` (quoted text), as in the web reader; this list had them swapped with archive-and-step's. The TUI folds quotes with `mxr_reader`'s detection in the reading view and its HTML quote markers in the HTML view, and lands the reader on the folded message.
- Archive and step to the next one in the reader: done. `]` and `[` run the list's archive (same request, confirm and undo) and open the next or previous conversation once the daemon has archived it.
- Peeking at an Updates item (`Space`).
- The row action `w` (wake) on the Snoozed and reply-queue pages. Blueprint 35 lists `w` as free, and nobody has decided what wake gets instead.

## Overlay grammar leftovers: done

- The resume-account-form prompt: done. It offers "Resume draft" (lit) and "Discard draft" (named), so only `y` or `Enter` on "Discard draft" throws the form away; `n`, `Esc` and `q` keep it for later, and `c` still resumes.
- A bare `Enter` in the TUI send review: done. It runs the lit option, marked `›`: Send, or Edit again when the safety check found something or the draft can't go as it is, the way the web's send confirm opens on "Back to draft".
- Unsubscribe's "and archive" option: done. It is `e` (and `E`) in both clients; `a` and `A` say where it went until 0.6.204.
- Overlay keys: done. Every overlay is a context in the definition (`OVERLAY_CONTEXTS` in `crates/keymap/src/lib.rs`), so TUI help, the keybindings page's TUI tables and its web "Dialogs" tables are generated, and the hand-written tables are gone. `runner::tests::overlay_keys` holds each TUI overlay's handler to its entries. The web's dialogs read their keys by id (`apps/web/src/lib/keys/overlayKeys.ts`), except keys the browser or the dialog primitive gives, which the definition marks `web_native`.

- The web keys #754 left out: done. The assistant's approval, question and history are overlays in the definition, compared between the clients (the web gained `a`, `o`, `s`, digits to 9, the arrows and `q` there; history deletes with `#` in both, and `d` says so); the web's Categories dialog and the popovers that close on `Esc` read their keys by id, the popovers as web-only `popover.close`.

The web's help, palette, label picker, Categories and Reply later are filter lists where letters type, and its bulk and sweep previews are two-button dialogs, so those overlays are listed per client and not compared (`SHARED_OVERLAYS` in `parity.rs` says why).
