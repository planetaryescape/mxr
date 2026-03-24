---
title: Keybindings
description: Default keybindings for the mxr TUI.
---

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
| `F` | Toggle fullscreen |
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
| `#` | Trash |
| `!` | Mark spam |
| `s` | Star / unstar |
| `I` | Mark read |
| `U` | Mark unread |
| `l` | Apply label |
| `v` | Move to label |
| `D` | Unsubscribe |
| `Z` | Snooze |
| `O` | Open in browser |
| `R` | Toggle reader mode |
| `E` | Export thread |

### Selection

| Key | Action |
|-----|--------|
| `x` | Toggle row selection |
| `V` | Visual line selection |
| `Esc` | Clear selection |

### Go-to

| Key | Action |
|-----|--------|
| `gi` | Go to Inbox |
| `gs` | Go to Starred |
| `gt` | Go to Sent |
| `gd` | Go to Drafts |
| `ga` | Go to All Mail |
| `gl` | Go to Label (picker) |

## Message view

| Key | Action |
|-----|--------|
| `j` / `k` | Scroll body |
| `R` | Toggle reader mode |
| `O` | Open in browser |
| `A` | Open attachment modal |
| `r` | Reply |
| `a` | Reply all |
| `f` | Forward |
| `e` | Archive |
| `#` | Trash |
| `!` | Mark spam |
| `s` | Star / unstar |
| `I` | Mark read |
| `U` | Mark unread |
| `D` | Unsubscribe |

## Thread view

| Key | Action |
|-----|--------|
| `j` / `k` | Move focused message in thread |
| `r` | Reply to focused message |
| `a` | Reply all to focused message |
| `f` | Forward focused message |
| `A` | Open attachment modal |
| `R` | Toggle reader mode |
| `E` | Export thread |
| `O` | Open in browser |
| `e` | Archive |
| `#` | Trash |
| `!` | Mark spam |
| `s` | Star / unstar |
| `I` | Mark read |
| `U` | Mark unread |
| `D` | Unsubscribe |

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
