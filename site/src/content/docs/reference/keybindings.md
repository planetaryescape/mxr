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
| `H` / `M` / `L` | Viewport top / middle / bottom |
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
| `e` | Archive |
| `m` | Mark read + archive |
| `#` | Trash |
| `!` | Mark spam |
| `s` | Star / unstar |
| `I` | Mark read |
| `U` | Mark unread |
| `l` | Apply label |
| `v` | Move to label |
| `D` | Unsubscribe |
| `Z` | Snooze |
| `b` | Bookmark for reply-later |
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
| `gh` | Go to the Desk (what needs you) |
| `gi` | Go to Inbox |
| `gs` | Go to Starred |
| `gt` | Go to Sent |
| `gd` | Go to Drafts |
| `ga` | Go to All Mail |
| `gl` | Go to Label (picker) |
| `gc` | Edit config (opens `$EDITOR`) |
| `gL` | Show recent logs |
| `g 1`–`g 9` | Jump to saved-search 1–9 |
| `g 0` | Return to default inbox (clear saved-search filter) |

## Message view

| Key | Action |
|-----|--------|
| `j` / `k` | Scroll body |
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
| `j` / `k` | Move focused message in thread |
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
| Reply queue | `j` / `k`, `Esc` close |
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
the note says why. Keys never fire while you type in a field or while
compose is open, except `⌘K` / `Ctrl+K`.

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
| `g d` | Go to Desk | The TUI opens the desk with g h and keeps g d for Drafts |
| `g w` | Waiting on | A lane of the desk lens in the TUI |
| `g i`, `g 0` | Go to Inbox |  |
| `g s` | Go to Starred |  |
| `g t` | Go to Sent |  |
| `g E` | Go to Drafts | g d opens the desk here; in the TUI g d is the Drafts lens |
| `g a` | Go to All Mail |  |
| `g l` | Go to label |  |
| `g A` | Analytics |  |
| `g y` | Activity log |  |
| `g L` | Daemon logs |  |
| `g c`, `0` | Settings | The TUI opens config.toml in $EDITOR; the web opens Settings |
| `g n` | Go to Snoozed | Web only |
| `g #` | Go to Trash | Web only |
| `g !` | Go to Spam | Web only |
| `g q` | Reply queue | Palette only in the TUI |
| `g o` | Owed replies | Sidebar lens in the TUI |
| `g v`, `9` | Calendar invites | Sidebar lens in the TUI |
| `g R` | Reading |  |
| `g P` | Paper trail |  |
| `g u` | Subscriptions | Sidebar lens in the TUI |
| `g S`, `8` | Screener | Palette only in the TUI |
| `1` | Mail |  |
| `2` | Search page |  |
| `3`, `g r` | Rules |  |
| `4` | Accounts |  |
| `5` | Diagnostics |  |
| `6` | Analytics (tab) |  |
| `7` | Deliveries |  |
| `g F` | Focus & reply | F in the TUI's reply queue |

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
| `b` | Reply later |  |
| `r` | Reply |  |
| `a` | Reply all |  |
| `f` | Forward |  |
| `E` | Export as Markdown |  |
| `B` | Thread briefing |  |
| `W` | Who is this sender? |  |
| `p` | Sender profile | Palette only in the TUI |
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
| `o` | Expand or collapse message | Web only; the TUI shows every message |
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
| `g h` | Raw headers | CLI `mxr cat --view headers`; not in the TUI |
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
| `p` | Pin or unpin (a sweep leaves pins) |  |
| `s` | Sweep this sender's bundle… | Previews the daemon's dry run first; undo afterwards |
| `S` | Sweep the whole place… | Everything unpinned here; previews first |
| `K` | Move sender to… | In the reader K is the previous message; use the palette or the line under the thread |
| `D` | Unsubscribe… |  |

<!-- web-keys:end -->

### Customization

The web keymap is built into the app; there is no `keys.toml` equivalent
yet.
