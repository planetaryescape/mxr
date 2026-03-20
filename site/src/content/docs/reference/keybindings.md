---
title: Keybindings
description: Default keybindings for the mxr TUI.
---

## Global

| Key | Action |
|-----|--------|
| `Ctrl-p` | Open command palette |
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
| `/` | Inline search |
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

## Search screen

| Key | Action |
|-----|--------|
| `/` | Edit query |
| `Enter` | Run query or open selected result |
| `o` | Open selected result in mailbox |
| `j` / `k` | Move through results |
| `Esc` | Return to mailbox |

## Rules screen

| Key | Action |
|-----|--------|
| `j` / `k` | Move rule selection |
| `n` | New rule |
| `E` | Edit rule |
| `e` | Enable / disable rule |
| `D` | Dry-run selected rule |
| `H` | Show history |
| `#` | Delete rule |

## Diagnostics screen

| Key | Action |
|-----|--------|
| `r` | Refresh diagnostics |
| `b` | Generate bug report |

## Accounts screen

| Key | Action |
|-----|--------|
| `n` | New IMAP/SMTP account |
| `Enter` | Edit selected account |
| `t` | Test selected account |
| `d` | Set selected account as default |
| `r` | Refresh account inventory |

## Modal controls

| Context | Keys |
|---------|------|
| Help | `j` / `k`, `Ctrl-d`, `Ctrl-u`, `Esc` |
| Command palette | typing, `j` / `k`, `Enter`, `Esc` |
| Label picker | typing, `j` / `k`, `Enter`, `Esc` |
| Attachments | `j` / `k`, `Enter` / `o`, `d`, `Esc` |
| Bulk confirm | `Enter` / `y` confirm, `Esc` / `n` cancel |
