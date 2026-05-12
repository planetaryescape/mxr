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
| `1` / `2` / `3` / `4` / `5` | Switch Mailbox / Search / Rules / Accounts / Diagnostics |
| `Ctrl-p` | Open command palette |
| `gc` | Edit config |
| `gL` | Open logs |
| `?` | Toggle help modal |
| `Esc` | Back, close modal, dismiss pane, or clear selection |
| `q` | Quit current view or exit |

The Analytics screen has no default digit key; open it via `Ctrl-p` → "Analytics" or rebind `open_tab_6` to a key in `~/.config/mxr/keys.toml` (see [Custom keybindings](/reference/config/#custom-keybindings)).

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
| _(unbound)_ | Analytics — via `Ctrl-p` → "Analytics" |

### Go-to

| Key | Action |
|-----|--------|
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
| `e` | Archive |
| `m` | Mark read + archive |
| `#` | Trash |
| `!` | Mark spam |
| `s` | Star / unstar |
| `I` | Mark read |
| `U` | Mark unread |
| `1`–`5` | Switch primary tab (Mailbox / Search / Rules / Accounts / Diagnostics) |
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
| `1`–`5` | Switch primary tab |
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
| Snooze (custom mode) | typing, `Enter` parse + snooze, `Backspace`, `Esc` back to presets |
| Reply queue | `j` / `k`, `Esc` close |
| Snippets browser | `j` / `k`, `Esc` close |
| Sender profile | `j` / `k` select other sender email, `Enter` / `o` open selected email, `Esc` close |
| Screener queue | `j` / `k` navigate, `a` allow, `d` deny, `f` feed, `p` paper-trail, `Esc` close |
| Thread summary | `Esc` close |
| Welcome / setup | `d` demo, `g` Gmail, `i` IMAP, `Enter` open form, `Esc` dismiss |
| Saved-search form | typing fields, `Tab` / `Shift-Tab` move, `Ctrl-s` save, `Esc` cancel |
| Saved-search delete | `Enter` / `y` confirm delete, `Esc` / `n` cancel |
| Compose send-confirm | `s` send, `d` save as draft, `e` re-edit, `Esc` cancel |
| Unsubscribe confirm | `u` unsubscribe + archive, `U` unsubscribe + trash, `a` archive only, `A` archive all from sender, `Esc` cancel |
| Analytics filter modal | typing fields, `Tab` / `Shift-Tab` move, `Enter` apply, `Esc` cancel |
| Error modal | `j` / `k`, `Ctrl-d` / `Ctrl-u` scroll, `q` / `x` / `Esc` close |
