---
title: Keybindings
description: Default keybindings for the mxr TUI and web app.
---

:::tip[The 10 keys that get you 80% of mxr]
If you only learn ten, learn these:

| Key | Action |
|-----|--------|
| `Ctrl-p` | Command palette — searchable surface for everything below |
| `?` | Help modal (context-aware) |
| `j` / `k` | Move down / up |
| `Enter` | Open selected message or thread |
| `e` | Archive |
| `r` | Reply |
| `c` | Compose |
| `s` | Star / unstar |
| `Z` | Snooze (preset list + custom-time entry) |
| `/` | Search |

Everything else is discoverable from the palette (`Ctrl-p`) and the
help modal (`?`).
:::

## Global

| Key | Action |
|-----|--------|
| `1`–`7` | Switch Mailbox / Search / Rules / Accounts / Diagnostics / Analytics / Deliveries |
| `Ctrl-p` | Open command palette |
| `gc` | Edit config |
| `gL` | Open logs |
| `?` | Toggle help modal |
| `Esc` | Back, close modal, dismiss pane, or clear selection |
| `q` | Quit current view or exit |

## Mail list

### Navigation

| Key | Action |
|-----|--------|
| `j` / `↓` | Move down |
| `k` / `↑` | Move up |
| `gg` | Jump to top |
| `G` | Jump to bottom |
| `Ctrl-d` | Page down |
| `Ctrl-u` | Page up |
| `H` / `M` | Viewport top / middle |
| `zz` | Center current item |
| `Enter` / `o` | Open selected row |
| `Tab` | Switch pane |
| `F` | Toggle fullscreen / full reader layout |
| `/` | Open full-index Search |
| `Ctrl-f` | Filter current mailbox only |
| `n` / `N` | Next / previous search result |

### Mail actions

| Key | Action |
|-----|--------|
| `c` | Compose |
| `r` | Reply |
| `a` | Reply all |
| `f` | Forward |
| `y` | Summarize thread in the background |
| `p` | Sender profile |
| `B` | Thread briefing |
| `W` | Who is this sender? |
| `L` | Links in the previewed message |
| `u` | Undo the last change |
| `e` | Archive (on the desk: Done, put it away) |
| `m` | Mark read + archive (on the desk: Done) |
| `#` | Trash |
| `!` | Mark spam |
| `s` | Star / unstar |
| `I` | Mark read |
| `U` | Mark unread |
| `l` | Apply label |
| `v` | Move to label |
| `D` | Unsubscribe |
| `Z` | Snooze |
| `b` | Reply later, at a time (Enter with no time: the queue now) |
| `O` | Open in browser |
| `R` | Toggle reader mode |
| `H` | Toggle HTML view |
| `M` | Toggle remote content (HTML images) |
| `S` | Toggle signature display |
| `E` | Export thread |

### Selection

| Key | Action |
|-----|--------|
| `x` | Toggle row selection |
| `V` | Visual line selection |
| `Esc` | Clear selection |

### Tabs

| Key | Action |
|-----|--------|
| `1` | Mailbox |
| `2` | Search |
| `3` | Rules |
| `4` | Accounts |
| `5` | Diagnostics |
| `6` | Analytics |
| `7` | Deliveries |

### Go-to

| Key | Action |
|-----|--------|
| `gh` | Go to Now (the front page) |
| `gm` | Messages |
| `gx` | To do |
| `gu` | Updates (Paper trail, early version) |
| `gr` | Reading |
| `ge` | Archive (records are coming) |
| `gi` | Go to Inbox |
| `gs` | Go to Starred |
| `gt` | Go to Sent |
| `gd` | Go to Drafts |
| `gE` | Local drafts and scheduled sends |
| `ga` | Go to All Mail |
| `gl` | Go to Label (picker) |
| `gp` | Updates, as `gu` |
| `gq` | Reply queue |
| `go` | Owed replies |
| `gv` | Calendar invites |
| `gS` | Screener |
| `gA` | Analytics |
| `gy` | Activity log |
| `gc` | Edit config (opens `$EDITOR`) |
| `gL` | Show recent logs |
| `g 1`–`g 9` | Jump to saved-search 1–9 |
| `g 0` | Return to default inbox (clear saved-search filter) |

Draft assist, a new draft for this sender, commitments and the voice
profile are in the command palette only (`Ctrl-p`). Help once listed
`gA`, `gD`, `gC` and `gV` for them, but those chords never ran them;
`gA` opens Analytics.

## Message view

| Key | Action |
|-----|--------|
| `j` / `k` | Scroll body |
| `gg` / `G` | Top / end of the message |
| `J` / `K` | Next / previous message in the thread |
| `R` | Toggle reader mode |
| `H` | Toggle HTML view |
| `M` | Toggle remote content (HTML images) |
| `S` | Toggle signature display |
| `O` | Open in browser |
| `A` | Open attachment modal |
| `L` | Open links modal (jump to any URL in the body) |
| `F` | Toggle fullscreen / full reader layout |
| `r` | Reply |
| `a` | Reply all |
| `f` | Forward |
| `y` | Summarize thread in the background |
| `p` | Sender profile |
| `e` | Archive |
| `m` | Mark read + archive |
| `#` | Trash |
| `!` | Mark spam |
| `s` | Star / unstar |
| `I` | Mark read |
| `U` | Mark unread |
| `1`–`7` | Switch primary tab (Mailbox / Search / Rules / Accounts / Diagnostics / Analytics / Deliveries) |
| `gc` | Edit config |
| `gL` | Show recent logs |
| `D` | Unsubscribe |

## Thread view

| Key | Action |
|-----|--------|
| `j` / `k` | Scroll the thread |
| `J` / `K` | Move to the next / previous message |
| `r` | Reply to focused message |
| `a` | Reply all to focused message |
| `f` | Forward focused message |
| `y` | Summarize thread in the background |
| `A` | Open attachment modal |
| `L` | Open links modal |
| `F` | Toggle fullscreen / full reader layout |
| `R` | Toggle reader mode |
| `H` | Toggle HTML view |
| `M` | Toggle remote content |
| `S` | Toggle signature |
| `E` | Export thread |
| `O` | Open in browser |
| `e` | Archive |
| `m` | Mark read + archive |
| `#` | Trash |
| `!` | Mark spam |
| `s` | Star / unstar |
| `I` | Mark read |
| `U` | Mark unread |
| `D` | Unsubscribe |
| `1`–`7` | Switch primary tab |
| `gc` / `gL` | Edit config / show logs |

## Sidebar

| Key | Action |
|-----|--------|
| `[` / `]` | Collapse / expand the focused sidebar section |
| `n` | New saved search (when the sidebar's saved-searches list is focused) |
| `e` | Edit the focused saved search |
| `d` | Delete the focused saved search (with confirm) |
| `g 1`–`g 9` | Jump to saved-search 1–9 |
| `g 0` | Clear saved-search filter (return to default inbox) |

## Now

The lens the TUI opens on (`gh`). Acting on a row does it in that row's own
mode.

| Key | Action |
|-----|--------|
| `j` / `k` | Next / previous row |
| `Enter` | Open the row in its mode |
| `e` | Done here (Messages for a person, To do for a due row; on the Updates card, as `A`) |
| `r` | Reply to a person |
| `o` | Open the email |
| `t` | Make a to-do from the row's email |
| `A` | Let go of the Updates card (previews first) |
| `1`–`4` | Answer a new sender's question, on a row that asks one |
| `Esc` | Dismiss the hint, when one shows |
| `u` | Undo |

## Messages

People as rows with their topics inside (`gm`). See
[Messages](/guides/messages/).

| Key | Action |
|-----|--------|
| `Enter` | Open the person |
| `r` / `a` | Reply / reply all on the selected topic |
| `.` | Got it: a short acknowledgement after a countdown; `u` cancels |
| `e` | Done here, until they write again |
| `t` | Make a to-do from the topic |
| `b` | Reply later |
| `s` | Pin or unpin the person |
| `c` | New topic with this person |
| `[` / `]` | Previous / next topic |
| `p` | The person's page |
| `o` / `v` | The selected message as sent |

## Updates and Reading

Lenses in the sidebar (`gu`, `gr`; `gp` opens Updates too). Mail keys act
on the message under the cursor.

| Key | Action |
|-----|--------|
| `e` | Done here: let go of the message's conversation in this mode |
| `p` | Pin or unpin the message |
| `K` | Move the sender to another place |
| `S` | Sweep this sender's bundle (previews first) |
| `A` | Sweep the whole place (previews first, opens on Cancel: `Tab`, then `Enter`) |
| `>` / `+` | More senders / more from this sender |

## Archive

The records lens (`ge`), with the answer box focused. Archive has no done:
records stay, so `e` opens the email like `o`.

| Key | Action |
|-----|--------|
| `/` | Ask: type what you remember, `Enter` answers |
| `j` / `k` | Next / previous record |
| `y` / `Y` | Copy the reference / the amount |
| `Enter` | Open the record's document (its PDF first) |
| `o`, `e` | Open the email the record came from |
| `p` | The issuer's page: every record from them |
| `[` / `]` | Previous / next year |
| `,` | Fix a field or confirm it (previews first) |
| `v` | Mark the card checked: confirm every unchecked amount and date |
| `X` | Not a record (the email is untouched) |
| `E` | Export as CSV, after a preview of rows, totals, unchecked rows and missing PDFs |
| `t` | Make a to-do from the record's email |
| `g f` | Filters: kind, issuer, year |
| `u` | Undo |
| `?` | What Archive is, then every key |

On any conversation, in the mailbox or the reader, `T` passes it to
another mode. This release offers Archive: it shows the card it would file,
and `Enter` files it. `F` stays the full-width reader.

## Calendar invites lens

Open from the **Calendar invites** sidebar item. The list pane shows every
detected invite with inline RSVP.

| Key | Action |
|-----|--------|
| `j` / `k` | Move down / up |
| `a` | Accept |
| `t` or `m` | Tentative |
| `d` | Decline |
| `A` / `T` (or `M`) / `D` | Accept / Tentative / Decline **with a comment** (opens compose) |
| `u` | Undo the just-issued RSVP (within its send window) |
| `Enter` / `o` | Open the underlying message |
| `h` | Back to the sidebar |

## Analytics screen

The Analytics screen has six views. Cycle them with `Tab` / `Shift-Tab`; refresh the active view with `r`.

### View-specific keys

| View | Key | Action |
|------|-----|--------|
| Storage | `m` | Toggle Breakdown ↔ Largest-Messages mode |
| Storage | `g` | Cycle `group_by` (sender / mimetype / label) in Breakdown mode |
| Stale Threads | `p` | Toggle perspective (mine ↔ theirs) |
| Stale Threads | `[` / `]` | ±7 days on `older_than_days` |
| Stale Threads | `{` / `}` | ±30 days on `within_days` |
| Contacts | `m` | Cycle sub-mode (asymmetry / decay / refresh) |
| Contacts | `R` | Refresh the materialized contacts table |
| Response Time | `d` | Toggle direction (clock ↔ business hours) |
| Subscriptions | `o` | Toggle ranking (volume ↔ open-rate) |
| Subscriptions | `u` | Open the unsubscribe-confirm modal for the selected row |
| Wrapped | `h` / `j` / `k` / `l` | Move between dashboard tiles |
| Wrapped | `y` / `Y` | Step year (back / forward) |
| Wrapped | `t` | Cycle window kind (YTD → Year → SinceDays) |

### Cross-view

| Key | Action |
|-----|--------|
| `Tab` / `Shift-Tab` | Cycle views |
| `r` | Refresh active view |
| `Enter` | Drill down (sender → search filter; thread row → open conversation) |
| `f` | Open the filter modal — every CLI flag for the active view as an editable field |
| `Esc` | Return to Mailbox |

## Deliveries screen

Open with `7`. Lists [tracked packages](/guides/deliveries/) detected in your mail.

| Key | Action |
|-----|--------|
| `j` / `k` | Move selection |
| `o` / `Enter` | Open the source email inline in a split preview |
| `Ctrl-d` / `Ctrl-u` | Scroll the open email (when the preview is showing) |
| `r` | Resolve (mark delivered/done) |
| `d` | Dismiss (hide a false positive) |
| `D` | Cycle filter: active → delivered → all |
| `g` | Refresh the list |
| `Esc` | Close the preview, then return to Mailbox |

## Search query editor

| Key | Action |
|-----|--------|
| `Enter` | Run search now |
| `Tab` | Change lexical / hybrid / semantic mode |
| `Esc` | Stop editing query |

## Search results

| Key | Action |
|-----|--------|
| `j` / `k` | Move through results |
| `Enter` / `o` / `l` | Open selected result in preview |
| `/` | Edit query |
| `Tab` | Switch to preview |
| `Esc` | Return to mailbox |

## Search preview

| Key | Action |
|-----|--------|
| `j` / `k` | Move through messages in the previewed thread |
| `h` / `Esc` | Return to results |
| `Tab` | Switch back to results |
| `R` | Toggle reader mode |
| `A` | Open attachments |
| `L` | Open links |
| `r` / `a` / `f` / `e` | Reply / reply all / forward / archive |

## Rules screen

| Key | Action |
|-----|--------|
| `j` / `k` | Move rule selection |
| `Enter` / `o` | Refresh selected rule overview |
| `n` | New rule |
| `E` | Edit rule |
| `e` | Enable / disable rule |
| `D` | Dry-run selected rule |
| `H` | Show history |
| `Ctrl-s` | Save rule form |
| `#` | Delete rule |

## Diagnostics screen

| Key | Action |
|-----|--------|
| `Enter` / `o` | Toggle fullscreen for the selected pane |
| `d` | Open selected section details |
| `r` | Refresh diagnostics |
| `b` | Generate bug report |
| `c` | Edit config |
| `L` | Open logs |

## Accounts screen

| Key | Action |
|-----|--------|
| `j` / `k` | Move account selection |
| `n` | New IMAP/SMTP account |
| `Enter` / `o` | Edit selected account |
| `t` | Test selected account |
| `d` | Set selected account as default |
| `c` | Edit config |
| `r` | Refresh account inventory |

## Modal controls

| Context | Keys |
|---------|------|
| Help | `j` / `k`, `Ctrl-d`, `Ctrl-u`, `o`, `Esc` |
| Command palette | typing, `j` / `k`, `Enter`, `Esc` |
| Label picker | typing, `j` / `k`, `Enter`, `Esc` |
| Attachments | `j` / `k`, `Enter` / `o`, `d`, `Esc` |
| Bulk confirm | `Enter` / `y` confirm, `Esc` / `n` cancel |
| Snooze (preset list) | `j` / `k` move, `Enter` confirm, `Esc` close |
| Snooze (custom mode) | typing (live preview), `Tab` other reading, `Enter` snooze, `Backspace`, `Esc` back to presets |
| Reply queue | `j` / `k`, `Enter` / `r` reply, `F` focus & reply, `Esc` close |
| Snippets browser | `j` / `k`, `Esc` close |
| Sender profile | `j` / `k` select other sender email, `Enter` / `o` open selected email, `Esc` close |
| Screener queue | `j` / `k` navigate, `a` allow, `d` deny, `f` feed, `p` paper-trail, `Esc` close |
| Welcome / setup | `d` demo, `g` Gmail, `i` IMAP, `Enter` open form, `Esc` dismiss |
| Saved-search form | typing fields, `Tab` / `Shift-Tab` move, `Ctrl-s` save, `Esc` cancel |
| Saved-search delete | `Enter` / `y` confirm delete, `Esc` / `n` cancel |
| Compose send-confirm | `s` send, `d` save as draft, `e` re-edit, `Esc` cancel |
| Unsubscribe confirm | `u` unsubscribe + archive, `U` unsubscribe + trash, `a` archive only, `A` archive all from sender, `Esc` cancel |
| Analytics filter modal | typing fields, `Tab` / `Shift-Tab` move, `Enter` apply, `Esc` cancel |
| Error modal | `j` / `k`, `Ctrl-d` / `Ctrl-u` scroll, `q` / `x` / `Esc` close |

## Web app

The web app follows the TUI's live bindings. One dispatcher reads every key
from the shared action registry (`apps/web/src/lib/actions/`), so these
tables, the in-app help (`?`), the command palette and **Settings →
Keybindings** always agree. Where the web deliberately differs from the TUI,
the note says why, and a test fails if the two clients bind the same key
to different things without a listed reason. Keys never fire while you
type in a field or while compose is open, except `⌘K` / `Ctrl+K`.

:::note[Changed in 0.6.41]
Web keys now match the TUI's: `g h` opens the desk (was `g d`), `g d`
opens Drafts (`g E` still does), `g r` / `g p` open Reading / Paper trail
(were `g R` / `g P`, which keep working until 0.6.42), `g H` shows raw
headers (was `g h`), and in Reading and Paper trail `S` sweeps a sender's
bundle and `A` the whole place (were `s` / `S`). Rules is `3` or the
palette; `g r` no longer opens it.
:::

<!-- web-keys:start (generated from the action registry; run UPDATE_KEY_DOCS=1 npm test) -->

### Everywhere

Work on every page except while typing in a field or compose.

| Key | Action | Note |
|-----|--------|------|
| `⌘K / Ctrl+K`, `:`, `Ctrl+P` | Command palette | Ctrl+P works on macOS; elsewhere it stays the browser's Print |
| `/` | Search mail |  |
| `?` | Keyboard help |  |
| `c` | Compose |  |
| `u`, `z` | Undo last action | z also undoes, for Gmail muscle memory |

### Go to

`g` then a letter, as in the TUI. Digits match the TUI's tabs.

| Key | Action | Note |
|-----|--------|------|
| `g 1` … `g 9` | Open saved search 1 to 9 | In sidebar order, as the TUI's tab strip |
| `g h` | Go to Now |  |
| `g m` | Go to Messages |  |
| `g u`, `g p` | Go to Updates |  |
| `g e` | Go to Archive |  |
| `g w` | Waiting on | A lane of the desk lens in the TUI |
| `g i`, `g 0` | Go to Inbox |  |
| `g s` | Go to Starred |  |
| `g t` | Go to Sent |  |
| `g d`, `g E` | Go to Drafts | One page here; in the TUI g d is the Drafts mailbox and g E its local drafts |
| `g a` | Go to All Mail |  |
| `g l` | Go to label |  |
| `g A` | Analytics |  |
| `g y` | Activity log |  |
| `g L` | Daemon logs |  |
| `g c`, `0` | Settings | The TUI opens config.toml in $EDITOR; the web opens Settings |
| `g n` | Go to Snoozed | Web only |
| `g #` | Go to Trash | Web only |
| `g !` | Go to Spam | Web only |
| `g q` | Reply queue |  |
| `g o` | Owed replies |  |
| `g v`, `9` | Calendar invites |  |
| `g r` | Reading |  |
| `g S`, `8` | Screener |  |
| `1` | Mail |  |
| `2` | Search page |  |
| `3` | Rules |  |
| `4` | Accounts |  |
| `5` | Diagnostics |  |
| `6` | Analytics (tab) |  |
| `7` | Deliveries |  |
| `g F` | Focus & reply | F in the TUI's reply queue |
| `g x` | To do |  |

### Mail actions

In the mail list they act on the selection or the row under the cursor; in the reader, on the open conversation.

| Key | Action | Note |
|-----|--------|------|
| `e` | Archive |  |
| `m` | Mark read and archive | Same as the TUI; read/unread are I and U |
| `#`, `Delete`, `Backspace` | Move to Trash |  |
| `!` | Mark as spam |  |
| `s` | Star or unstar |  |
| `I` | Mark read |  |
| `U` | Mark unread |  |
| `l` | Labels… |  |
| `v` | Move to label… |  |
| `Z` | Snooze… |  |
| `D` | Unsubscribe… |  |
| `b` | Reply later… |  |
| `t` | Make a to-do from this |  |
| `T` | Pass to a mode… |  |
| `r` | Reply |  |
| `a` | Reply all |  |
| `f` | Forward |  |
| `E` | Export as Markdown |  |
| `B` | Thread briefing |  |
| `W` | Who is this sender? |  |
| `p` | Sender profile |  |
| `L` | Links in this message |  |
| `A` | Attachments |  |
| `i a` | Accept invite |  |
| `i m` | Maybe (invite) |  |
| `i d` | Decline invite |  |
| `i A` | Accept with comment |  |
| `i M` | Maybe with comment |  |
| `i D` | Decline with comment |  |

### Mail list

| Key | Action | Note |
|-----|--------|------|
| `j`, `ArrowDown` | Next conversation |  |
| `k`, `ArrowUp` | Previous conversation |  |
| `g g`, `Home` | First conversation |  |
| `G`, `End` | Last conversation |  |
| `Ctrl+D`, `PageDown` | Half page down |  |
| `Ctrl+U`, `PageUp` | Half page up |  |
| `H` | Top of screen |  |
| `M` | Middle of screen | L (bottom of screen) opens links, as in the TUI's list pane |
| `Enter`, `o`, `ArrowRight` | Open conversation |  |
| `h`, `ArrowLeft` | Go to sidebar |  |
| `w` | This list's row action | Web only |
| `x` | Select and move down |  |
| `V` | Visual line mode |  |
| `* a`, `⌘A / Ctrl+A` | Select all |  |
| `* n` | Select none |  |
| `* r` | Select read | Web only |
| `* u` | Select unread | Web only |
| `* s` | Select starred | Web only |
| `Esc` | Clear selection, then close |  |
| `g f`, `Ctrl+F` | Filter this list | TUI Ctrl-f; Ctrl+F stays the browser's Find off macOS, so g f works everywhere |

### Reader

| Key | Action | Note |
|-----|--------|------|
| `j`, `ArrowDown` | Scroll down |  |
| `k`, `ArrowUp` | Scroll up |  |
| `Ctrl+D`, `PageDown`, `Space` | Page down |  |
| `Ctrl+U`, `PageUp`, `Shift+Space` | Page up |  |
| `g g`, `Home` | Top of thread |  |
| `G`, `End` | End of thread |  |
| `J` | Next message |  |
| `K` | Previous message |  |
| `o` | Expand or collapse message | The TUI shows every message, and its o opens the original, like O |
| `X` | Expand or collapse all | Web only |
| `n` | Next conversation | The TUI steps results with n/N; the web steps conversations |
| `N` | Previous conversation |  |
| `]` | Archive, open next | Web only |
| `[` | Archive, open previous | Web only |
| `h`, `ArrowLeft` | Back to list |  |
| `Esc`, `q` | Close conversation |  |
| `R` | Reader view, or back to plain |  |
| `H` | Formatted (HTML) view, or back to plain |  |
| `M` | Load remote images |  |
| `S` | Show or hide signatures |  |
| `Q` | Show or hide quoted text | Web only |
| `g H` | Raw headers | CLI `mxr cat --view headers`; not in the TUI |
| `O` | Open original in a new tab |  |
| `y` | Summarize thread |  |
| `F` | Full-width reader |  |

### Sidebar

| Key | Action | Note |
|-----|--------|------|
| `j`, `ArrowDown` | Next item |  |
| `k`, `ArrowUp` | Previous item |  |
| `g g` | First item |  |
| `G` | Last item |  |
| `Enter`, `o`, `l`, `ArrowRight` | Open |  |
| `[` | Collapse section |  |
| `]` | Expand section |  |

### Screener

| Key | Action | Note |
|-----|--------|------|
| `a` | Allow sender |  |
| `d` | Deny sender |  |
| `f` | Send to feed |  |
| `p` | Send to paper trail |  |
| `j`, `ArrowDown` | Next sender |  |
| `k`, `ArrowUp` | Previous sender |  |

### Focus & reply

One conversation at a time. In the reply, ⌘Enter sends and moves on; Tab out of the reply (or Esc in the rich-text editor) to use these keys.

| Key | Action | Note |
|-----|--------|------|
| `⌘ENTER / Ctrl+ENTER` | Send and next |  |
| `s` | Skip for now | Goes to the end of the queue |
| `e` | Done, no reply needed | e on the desk in the TUI |
| `Z` | Snooze… |  |
| `w` | Send, remind me if nobody replies… |  |
| `d` | Draft in your voice |  |
| `r` | Back to the reply |  |
| `Esc` | Leave focus mode |  |

### Reading and Paper trail

Bundles of mail that isn't from people. The reader keeps its own keys.

| Key | Action | Note |
|-----|--------|------|
| `j`, `ArrowDown` | Next |  |
| `k`, `ArrowUp` | Previous |  |
| `Enter`, `o` | Open, or expand a bundle |  |
| `e` | Done here: let go of this in Updates or Reading |  |
| `p` | Pin or unpin (a sweep leaves pins) |  |
| `S` | Sweep this sender's bundle… | Previews the daemon's dry run first; undo afterwards |
| `A` | Sweep the whole place… | Everything unpinned here; previews first and opens on Cancel (Tab, then Enter) |
| `K` | Move sender to… | In the reader K is the previous message; use the palette or the line under the thread |
| `D` | Unsubscribe… |  |

### Reading edition

Newsletters as an edition. `b`, `e`, `D` and `A` work on the item or link under the cursor; `D` and `A` preview first.

| Key | Action | Note |
|-----|--------|------|
| `j`, `ArrowDown` | Next |  |
| `k`, `ArrowUp` | Previous |  |
| `Enter` | Read |  |
| `L` | Fetch the linked article |  |
| `b` | Save for later |  |
| `e` | Let go |  |
| `D` | Unsubscribe… | Shows the evidence and the method first; nothing is sent until you confirm |
| `R` | Read in the sender's layout |  |
| `A` | Let go of everything shown… | Previews the daemon's dry run first; undo afterwards |
| `o` | Open the email as sent |  |
| `K` | This sender here: move to… |  |
| `B` | Open Later |  |
| `Esc` | Dismiss the hint |  |

### Reading reader

One item in a 66-character column. The article is fetched only on `L` or the Article tab.

| Key | Action | Note |
|-----|--------|------|
| `j`, `ArrowDown` | Scroll down |  |
| `k`, `ArrowUp` | Scroll up |  |
| `L` | Article: fetch and read it |  |
| `I` | Issue: the email itself |  |
| `b` | Save for later |  |
| `e` | Let go and go back |  |
| `n` | Next item |  |
| `R` | Sender's layout or cleaned |  |
| `h` | Highlight the selection |  |
| `D` | Unsubscribe… |  |
| `o` | Open the email as sent |  |
| `Esc` | Back to the edition |  |

<!-- web-keys:end -->

### Customization

The web keymap is built into the app; there is no `keys.toml` equivalent
yet.
