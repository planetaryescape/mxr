---
title: Keybindings
description: Default keybindings for the mxr TUI and web app.
---

These tables show defaults. To remap TUI keys in mail panes or in a mode,
create [`keys.toml`](/reference/config/#custom-keybindings), run
`mxr keys check`, and restart the TUI.
Help and the command palette show your effective shortcuts in each mail view;
the web app keeps its own defaults.

Every default in both clients comes from one definition. `mxr keys` lists
it, and `mxr keys explain <key> --context <place>` says what a key does
there, for example `mxr keys explain e --context todo`. The Raycast
extension reads the same definition: its shortcuts are Cmd plus the web app's
letter, except where Raycast keeps its own key. See [Raycast](/guides/raycast/#what-each-command-shows).

When a default moves, the old key keeps a reminder for 30 releases: pressing
it shows "<action> is now <key>" in the status bar and does nothing else.

:::tip[The 10 keys that get you 80% of mxr]
If you only learn ten, learn these:

| Key | Action |
|-----|--------|
| `:` | Command palette — searchable surface for everything below (`Ctrl-k` too) |
| `?` | Help modal (context-aware) |
| `j` / `k` | Move down / up |
| `Enter` | Open selected message or thread |
| `e` | Archive |
| `r` | Reply |
| `c` | Compose |
| `s` | Star / unstar |
| `b` | Not now: Later with no time, a preset (`1`–`4`) or a typed time |
| `/` | Search |

Everything else is discoverable from the palette (`:`) and the
help modal (`?`).
:::

## Look up and check keys

### Find keys while you work (web and desktop)

- Press `g` and pause to see the available next keys for the current page.
  The same guide appears for selection prefixes such as `*`. Fast sequences
  run immediately without opening the guide. `Esc` cancels; clicking or
  entering a text field cancels too.
- `g f` reaches filters in mail and Search. On tool and settings pages it
  focuses the selected tab or first filter. The key is shown beside the controls.
- Once a tab or segmented choice has focus, arrow keys move between choices;
  `Home` and `End` jump to the ends. `Tab` leaves the group. Settings and
  Reading section links keep normal link behavior: arrows move focus, `Enter` opens.
- In Messages, `f` cycles the four views without focusing the controls.
  In mail lists and the email reader, `f` still means Forward.

Shortcuts do not take letters from text fields, editors or open dialogs.

Ask the daemon which key does what, in which place, instead of searching these
tables. Each row is the client, the place, the key, the action id, its label and,
for the TUI, the action the dispatcher runs:

```bash
mxr keys explain e                       # what e does in every place
mxr keys explain e --context todo        # what e does in To do
mxr keys --client tui --context todo     # every key live in To do
mxr keys --client web --format json      # the web app's keys, for a script
```

```text
$ mxr keys explain e --client tui
tui  assistant             e  tui.assistant.steps        Open or fold the steps  (inline:toggle_steps)
tui  catchup               e  todo.catchup-let-go        Let go of this one  (CatchupLetGo)
tui  compose-assist-proof  e  compose.proof.accept-edit  Accept proof and edit  (ComposeProofAcceptEdit)
tui  list                  e  mail.archive               Archive  (Archive)
tui  messages              e  messages.done              Done here  (MessagesDone)
tui  now                   e  now.done                   Done here  (NowDone)
tui  place                 e  place.done                 Done here: let go of this in Updates or Reading  (Archive)
tui  reader                e  mail.archive               Archive  (Archive)
tui  reading               e  reading.let-go             Let go  (ReadingLetGo)
tui  reading-reader        e  reading.let-go             Let go  (ReadingLetGo)
tui  rules                 e  tui.rules.toggle           Turn rule on or off  (ToggleRuleEnabled)
tui  search                e  mail.archive               Archive  (Archive)
tui  search-preview        e  mail.archive               Archive  (Archive)
tui  todo                  e  todo.done                  Done  (TodoDone)
tui  updates               e  updates.let-go-source      Let go of this source  (UpdatesLetGoSource)
```

`--client` is `tui` or `web`. `--context` takes a TUI context or a web scope
such as `todo`, and a place with no keys prints `No keys match.` Use the action
ids from this listing as the right-hand side of a remap in
[`keys.toml`](/reference/config/#remapping-a-modes-keys).

After you edit `keys.toml`, check it before you restart the TUI. `mxr keys check`
reads the file beside your config, or the path you give it, and names every entry
the TUI would ignore. It exits non-zero when it finds one:

```bash
mxr keys check
mxr keys check ~/dotfiles/mxr/keys.toml
```

```text
$ mxr keys check keys.toml
keys.toml:
  [todo] `Ctrl+Shift+Zzz`: most terminals send Ctrl+Shift+ as plain Ctrl+
  [todo] `Q`: no action todo.nope; `mxr keys --client tui --context todo` lists them
Error: 2 problems in keys.toml
```

With no `keys.toml` beside your config, it reports `No <path>/keys.toml; every key is the default.` and exits 0.

## Terminal (TUI)

The tables below are generated from the key definition, so they list exactly
what the TUI does. Help (`?`) shows the same keys for the place you are in,
with your `keys.toml` remaps.

- `Esc` goes back one level and never destroys anything. `q` closes the
  innermost layer (a menu, the reader, a sub-view, a screen) and quits only at
  the mailbox root. `Ctrl-z` suspends the TUI to the shell; `fg` brings it back.
- `g` and a lowercase letter goes to a mail place or mode; `g` and a capital
  opens a tool. The digits that used to open the tabs say where each went
  until 0.6.180.
- Pasted text types into the focused text field. With no field focused, the
  TUI drops it, so a paste never runs as shortcuts. In terminals that support
  the kitty keyboard protocol, `Esc` also acts at once instead of waiting to
  see whether it starts an `Alt` key; every key works without it.
- Digits count rows only in visual line mode (`V`, `3`, `j` selects 4 rows),
  and `Esc` with a count typed drops the count. Before a multi-message action
  runs, the confirm lists the messages it will touch.
- Some actions have no key and are in the command palette (`:`): draft
  assist, a new draft for this sender, commitments, the voice profile,
  **Edit config**, **Export as Markdown**, **Send to Microsoft To Do** and
  **Sync the linked Microsoft To Do task** in To do, **Sweep this sender out of
  Updates**, and Archive's export of the records in view.
- In To do, `,` reviews a waiting resolution before it edits fields.
  [Review later evidence](/guides/todo-resolution/) lists the preview's own
  keys.
- On any conversation, `X` then `e` files it in Archive. **Pass to a mode** in
  the palette shows the card it would file first, and `Enter` files it.

<!-- tui-keys:start (generated from crates/keymap/keymap.toml; run UPDATE_KEYMAP=1 cargo test -p mxr-keymap) -->

### Everywhere in mail

In the mail list, the reader, the sidebar and every mode.

| Key | Action |
|-----|--------|
| `:`, `Ctrl+k`, `Ctrl+p` | Command palette |
| `?` | Keyboard help |
| `g h` | Go to Now |
| `g m` | Go to Messages |
| `g u` | Go to Updates |
| `g e` | Go to Archive |
| `g i`, `g 0` | Go to Inbox |
| `g s` | Go to Starred |
| `g t` | Go to Sent |
| `g d` | Go to Drafts |
| `g a` | Go to All Mail |
| `g l` | Go to label |
| `g A` | Analytics |
| `g Y` | Activity log |
| `g L` | Daemon logs |
| `g C` | Settings |
| `g q` | Reply later |
| `g o` | Owed replies |
| `g v` | Calendar invites |
| `g r` | Reading |
| `g S` | Screener |
| `g R` | Rules |
| `g D` | Diagnostics |
| `g P` | Deliveries |
| `i a` | Accept invite |
| `i m` | Maybe (invite) |
| `i d` | Decline invite |
| `i A` | Accept with comment |
| `i M` | Maybe with comment |
| `i D` | Decline with comment |
| `* a` | Select all |
| `* n` | Select none |
| `* r` | Select read |
| `* u` | Select unread |
| `* s` | Select starred |
| `g x` | To do |
| `@` | Show or hide the assistant |
| `* t` | Pattern select thread |
| `g 1` … `g 9` | Open saved search 1 to 9, in sidebar order |

### List and reader

In the mail list, the reader, and the Reading and Paper trail lists. Mail verbs act on the selected conversations, or on the open one.

| Key | Action |
|-----|--------|
| `/` | Search mail |
| `c` | Compose |
| `u` | Undo last action |
| `m` | Mark read and archive |
| `#` | Move to Trash |
| `!` | Mark as spam |
| `I` | Mark read |
| `U` | Mark unread |
| `l` | Labels… |
| `v` | Move to label… |
| `b` | Not now… |
| `t` | Make a to-do from this |
| `r` | Reply |
| `a` | Reply all |
| `f` | Forward |
| `B` | Thread briefing |
| `L` | Links in this message |
| `x` | Select and move down |
| `V` | Visual line mode |
| `R` | Reader view, or back to plain |
| `O` | Open original in a new tab |
| `F` | Full-width reader |
| `X` | Move to… |
| `N` | Prev search result |
| `Tab` | Switch pane |
| `n` | Next search result |

### Mail list

Inbox, labels and saved searches.

| Key | Action |
|-----|--------|
| `e` | Archive |
| `s` | Star or unstar |
| `D` | Unsubscribe… |
| `p` | Sender profile |
| `A` | Attachments |
| `↓`, `j` | Next conversation |
| `↑`, `k` | Previous conversation |
| `g g` | First conversation |
| `G` | Last conversation |
| `Ctrl+d` | Half page down |
| `Ctrl+u` | Half page up |
| `→`, `o`, `Enter` | Open conversation |
| `←`, `h` | Go to sidebar |
| `Esc` | Clear selection, then close |
| `Ctrl+f` | Filter this list |
| `S` | Show or hide signatures |
| `P` | Send everything from this sender to… |
| `q` | Quit view |

### Reader

An open conversation.

| Key | Action |
|-----|--------|
| `e` | Archive |
| `s` | Star or unstar |
| `D` | Unsubscribe… |
| `p` | Sender profile |
| `A` | Attachments |
| `Enter` | Open conversation |
| `Ctrl+f` | Filter this list |
| `↓`, `j` | Scroll down |
| `↑`, `k` | Scroll up |
| `Ctrl+d` | Page down |
| `Ctrl+u` | Page up |
| `g g` | Top of thread |
| `G` | End of thread |
| `J` | Next message |
| `K` | Previous message |
| `←`, `h` | Back to list |
| `Esc`, `q` | Close conversation |
| `H` | Formatted (HTML) view, or back to plain |
| `M` | Load remote images |
| `S` | Show or hide signatures |

### Sidebar

Places, labels and saved searches, on the left.

| Key | Action |
|-----|--------|
| `/` | Search mail |
| `b` | Not now… |
| `A` | Attachments |
| `↓`, `j` | Next item |
| `↑`, `k` | Previous item |
| `Enter`, `→`, `l`, `o` | Open |
| `{` | Previous section |
| `}` | Next section |
| `g g` | First conversation |
| `G` | Last conversation |
| `Ctrl+d` | Half page down |
| `Ctrl+u` | Half page up |
| `x` | Select and move down |
| `V` | Visual line mode |
| `Esc` | Clear selection, then close |
| `Ctrl+f` | Filter this list |
| `F` | Full-width reader |
| `N` | Prev search result |
| `Tab` | Switch pane |
| `q` | Quit view |
| `c` | New saved search |

### Reading and Paper trail lists

Newsletters and automated mail, bundled by sender, opened from the sidebar.

| Key | Action |
|-----|--------|
| `g g` | First conversation |
| `G` | Last conversation |
| `Ctrl+d` | Half page down |
| `Ctrl+u` | Half page up |
| `←`, `h` | Go to sidebar |
| `Esc` | Clear selection, then close |
| `↓`, `j` | Next |
| `↑`, `k` | Previous |
| `→`, `Enter`, `o` | Open, or expand a bundle |
| `e` | Done here: let go of this in Updates or Reading |
| `s` | Pin or unpin (a sweep leaves pins) |
| `E` | Sweep the whole place… |
| `P` | Move sender to… |
| `D` | Unsubscribe… |
| `q` | Quit view |

### Now

The front page the TUI opens on. Acting on a row does it in that row's own mode.

| Key | Action |
|-----|--------|
| `/` | Search mail |
| `u` | Undo last action |
| `b` | Not now… |
| `g g` | First conversation |
| `G` | Last conversation |
| `Ctrl+d` | Half page down |
| `Ctrl+u` | Half page up |
| `←`, `h` | Go to sidebar |
| `↓`, `j` | Next |
| `↑`, `k` | Previous |
| `Enter` | Open in its mode |
| `e` | Done here |
| `r` | Reply |
| `t` | Make it a to-do |
| `o` | Open the email |
| `E` | Let go of the Updates digest… |
| `Esc` | Dismiss the hint |
| `X` | Move to… |
| `P` | Send everything from this sender to… |
| `q` | Quit view |

### Messages

People as rows with their topics inside. See [Messages](/guides/messages/).

| Key | Action |
|-----|--------|
| `/` | Search mail |
| `u` | Undo last action |
| `g g` | First conversation |
| `G` | Last conversation |
| `Ctrl+d` | Half page down |
| `Ctrl+u` | Half page up |
| `←`, `h` | Go to sidebar |
| `↓`, `j` | Next person |
| `↑`, `k` | Previous person |
| `Enter` | Open the person |
| `r` | Reply |
| `a` | Reply all |
| `.` | Got it: a short thanks, after a countdown |
| `e` | Done here |
| `t` | Make it a to-do… |
| `b` | Not now: reply later… |
| `s` | Pin or unpin this person |
| `c` | New topic with this person |
| `{` | Previous topic |
| `}` | Next topic |
| `p` | Go to the person page |
| `o` | Show the message as sent |
| `f` | Cycle Messages views |
| `Ctrl+f` | Filter the people list: turn, unread, past your pace |
| `Esc` | Dismiss the hint, or back to the people |
| `X` | Move to… |
| `P` | Send everything from this sender to… |
| `q` | Quit view |

### To do

See [Act on To dos](/guides/todo/).

| Key | Action |
|-----|--------|
| `/` | Search mail |
| `u` | Undo last action |
| `g g` | First conversation |
| `G` | Last conversation |
| `Ctrl+d` | Half page down |
| `Ctrl+u` | Half page up |
| `←`, `h` | Go to sidebar |
| `↓`, `j` | Next |
| `↑`, `k` | Previous |
| `Enter` | Do it: what the row's button says |
| `e` | Done |
| `b` | Not now: schedule… |
| `,` | Edit what mxr read… |
| `X` | Correct to-do |
| `o` | Open the email it came from |
| `1` | Runway |
| `3` | Expired list |
| `2` | Catch-up |
| `q` | Quit view |

### To do: catch-up

The first run's older undated tasks: keep each one, or let it go to the Expired list.

| Key | Action |
|-----|--------|
| `/` | Search mail |
| `u` | Undo last action |
| `g g` | First conversation |
| `G` | Last conversation |
| `Ctrl+d` | Half page down |
| `Ctrl+u` | Half page up |
| `o` | Open the email it came from |
| `1` | Runway |
| `3` | Expired list |
| `2` | Catch-up |
| `↓`, `j` | Next |
| `↑`, `k` | Previous |
| `Enter` | Keep: it still needs me |
| `e` | Let go of this one |
| `E` | Let go of all… |
| `Esc`, `q` | Back to To do |

### To do: Expired

Tasks that expired or were let go.

| Key | Action |
|-----|--------|
| `/` | Search mail |
| `g g` | First conversation |
| `G` | Last conversation |
| `Ctrl+d` | Half page down |
| `Ctrl+u` | Half page up |
| `o` | Open the email it came from |
| `1` | Runway |
| `3` | Expired list |
| `2` | Catch-up |
| `↓`, `j` | Next |
| `↑`, `k` | Previous |
| `Enter`, `u` | Restore to To do |
| `Esc`, `q` | Back to To do |

### Updates

The briefing: the latest digest, one line per source, with what arrived since below the cut.

| Key | Action |
|-----|--------|
| `/` | Search mail |
| `u` | Undo last action |
| `g g` | First conversation |
| `G` | Last conversation |
| `Ctrl+d` | Half page down |
| `Ctrl+u` | Half page up |
| `←`, `h` | Go to sidebar |
| `↓`, `j` | Next |
| `↑`, `k` | Previous |
| `Enter` | Show routine |
| `E` | Let go of the digest… |
| `e` | Let go of this source |
| `t` | This needs me: make it a to-do |
| `P` | Tune this source… |
| `L` | Open the link |
| `y` | Copy the one-time code |
| `o` | Open the email |
| `s` | Pin or unpin this source's latest email |
| `Esc` | Dismiss the hint |
| `X` | Move to… |
| `q` | Quit view |

### Reading

The [edition](/guides/reading/). Mail keys such as archive, star and reply don't apply here.

| Key | Action |
|-----|--------|
| `/` | Search mail |
| `u` | Undo last action |
| `t` | Make a to-do from this |
| `g g` | First conversation |
| `G` | Last conversation |
| `Ctrl+d` | Half page down |
| `Ctrl+u` | Half page up |
| `←`, `h` | Go to sidebar |
| `↓`, `j` | Next |
| `↑`, `k` | Previous |
| `Enter` | Read |
| `L` | Fetch the linked article |
| `b` | Not now: later or snooze… |
| `e` | Let go |
| `D` | Unsubscribe… |
| `R` | Read in the sender's layout |
| `E` | Let go of everything shown… |
| `o` | Open the email as sent |
| `P` | This sender here: move to… |
| `1` | Edition |
| `2` | Later |
| `3` | Highlights |
| `O` | Open in Marginalia |
| `X` | Move to… |
| `q` | Quit view |

### Reading: the issue

One item from the edition, open.

| Key | Action |
|-----|--------|
| `/` | Search mail |
| `u` | Undo last action |
| `t` | Make a to-do from this |
| `g g` | First conversation |
| `G` | Last conversation |
| `Ctrl+d` | Half page down |
| `Ctrl+u` | Half page up |
| `Enter` | Read |
| `L` | Fetch the linked article |
| `b` | Not now: later or snooze… |
| `e` | Let go |
| `D` | Unsubscribe… |
| `R` | Read in the sender's layout |
| `E` | Let go of everything shown… |
| `o` | Open the email as sent |
| `P` | This sender here: move to… |
| `1` | Edition |
| `2` | Later |
| `3` | Highlights |
| `O` | Open in Marginalia |
| `"` | Highlight the selection |
| `←`, `Esc`, `q` | Back to the edition |
| `X` | Move to… |

### Archive

The records lens, with the answer box focused.

| Key | Action |
|-----|--------|
| `u` | Undo last action |
| `g g` | First conversation |
| `G` | Last conversation |
| `Ctrl+d` | Half page down |
| `Ctrl+u` | Half page up |
| `←`, `h` | Go to sidebar |
| `↓`, `j` | Next record |
| `↑`, `k` | Previous record |
| `/` | Ask Archive |
| `y` | Copy the reference |
| `Y` | Copy the amount |
| `Enter` | Open the document |
| `o` | Open the email |
| `p` | This issuer's records |
| `{` | Previous year |
| `}` | Next year |
| `,` | Fix a field… |
| `X` | Not a record |
| `t` | Make a to-do from this record |
| `Esc` | Close |
| `q` | Quit view |
| `→` | Records open card |
| `g f` | Records next kind |

### Screener queue

New senders waiting for a decision.

| Key | Action |
|-----|--------|
| `a` | Allow sender |
| `d` | Deny sender |
| `f` | Send to feed |
| `p` | Send to paper trail |
| `↓`, `j` | Next sender |
| `↑`, `k` | Previous sender |
| `Esc` | Close screener modal |

### Calendar invites

The calendar invites list, from the sidebar. The `i` keys above answer an invite.

| Key | Action |
|-----|--------|
| `Enter`, `o`, `→` | Open the invite's email |
| `/` | Search mail |
| `u` | Undo the RSVP while it can still be stopped |
| `h`, `←` | Go to sidebar |

### Search results

The Search screen's results.

| Key | Action |
|-----|--------|
| `c` | Compose |
| `u` | Undo last action |
| `e` | Archive |
| `m` | Mark read and archive |
| `#` | Move to Trash |
| `!` | Mark as spam |
| `s` | Star or unstar |
| `I` | Mark read |
| `U` | Mark unread |
| `l` | Labels… |
| `v` | Move to label… |
| `D` | Unsubscribe… |
| `t` | Make a to-do from this |
| `r` | Reply |
| `a` | Reply all |
| `f` | Forward |
| `B` | Thread briefing |
| `p` | Sender profile |
| `L` | Links in this message |
| `R` | Reader view, or back to plain |
| `S` | Show or hide signatures |
| `O` | Open original in a new tab |
| `Esc` | Back to the mailbox |
| `j`, `↓` | Next row |
| `k`, `↑` | Previous row |
| `/` | Edit the query |
| `Enter`, `o`, `→` | Open the result in the preview |

### Search preview

The thread previewed beside the results.

| Key | Action |
|-----|--------|
| `c` | Compose |
| `u` | Undo last action |
| `e` | Archive |
| `m` | Mark read and archive |
| `#` | Move to Trash |
| `!` | Mark as spam |
| `s` | Star or unstar |
| `I` | Mark read |
| `U` | Mark unread |
| `l` | Labels… |
| `v` | Move to label… |
| `D` | Unsubscribe… |
| `t` | Make a to-do from this |
| `r` | Reply |
| `a` | Reply all |
| `f` | Forward |
| `B` | Thread briefing |
| `p` | Sender profile |
| `L` | Links in this message |
| `R` | Reader view, or back to plain |
| `S` | Show or hide signatures |
| `O` | Open original in a new tab |
| `/` | Edit the query |
| `h`, `←`, `Esc` | Back to the results |
| `j`, `↓` | Next message in the thread |
| `k`, `↑` | Previous message in the thread |
| `Ctrl+d` | Scroll the message down |
| `Ctrl+u` | Scroll the message up |
| `G` | End of the message |
| `H` | Original HTML |
| `M` | Remote images |

### Search query

While you type the query. Letters type.

| Key | Action |
|-----|--------|
| `Enter` | Run the search |
| `Esc` | Stop editing the query |

### Rules

| Key | Action |
|-----|--------|
| `Esc` | Back to the mailbox |
| `j`, `↓` | Next row |
| `k`, `↑` | Previous row |
| `c` | New rule |
| `E` | Edit rule |
| `e` | Turn rule on or off |
| `Enter`, `o` | Refresh the rule's overview |
| `D` | Dry run: what the rule would change |
| `PageDown` | Next page of the dry run |
| `PageUp` | Previous page of the dry run |
| `A` | Apply the sorting preview |
| `H` | Rule history |
| `#` | Delete rule (asks first) |

### Rule form

Letters type into the field; these keys don't.

| Key | Action |
|-----|--------|
| `Ctrl+d` | Dry run: what the rule would change |
| `Ctrl+s`, `Enter` | Save the rule |
| `Tab` | Next field |
| `Shift+Tab` | Previous field |
| `Esc` | Close the form |

### Accounts

Accounts, and the TUI's settings.

| Key | Action |
|-----|--------|
| `Esc` | Back to the mailbox |
| `j`, `↓` | Next row |
| `k`, `↑` | Previous row |
| `c` | Add account |
| `Enter`, `o` | Edit account |
| `t` | Test account |
| `d` | Make it the default account |
| `r` | Refresh accounts, or repair the selected one when it is unhealthy |
| `R` | Refresh accounts |
| `O` | Details of the last test |

### Account form

While a field is being edited, letters type into it, and `Enter` or `Esc` stops editing.

| Key | Action |
|-----|--------|
| `t` | Test account |
| `o` | Details of the last test |
| `Esc` | Close the form (a new account keeps its draft) |
| `j`, `↓`, `Tab` | Next field |
| `k`, `↑`, `Shift+Tab` | Previous field |
| `l`, `→` | Next account type |
| `h`, `←` | Previous account type |
| `Enter`, `i`, `Space` | Edit or toggle the field |
| `r` | Sign in to Gmail again |
| `s` | Save the account |
| `Ctrl+k` | Command palette |

### Diagnostics

| Key | Action |
|-----|--------|
| `Esc` | Back to the mailbox |
| `j`, `↓`, `Tab`, `→` | Next section |
| `k`, `↑`, `Shift+Tab`, `←` | Previous section |
| `Ctrl+d`, `PageDown` | Scroll the section down |
| `Ctrl+u`, `PageUp` | Scroll the section up |
| `Enter`, `o` | Full screen for the section, or back |
| `/` | Search the events, logs or activity section |
| `d` | Details of the section |
| `r` | Refresh diagnostics |
| `b` | Generate a bug report |
| `c` | Edit config |
| `L` | Open logs |
| `a` | Preview activating the restored archive |
| `A` | Activate the restored archive |
| `p` | Preview the recovered draft |
| `P` | Restore the recovered draft |
| `]` | Next recovered draft |
| `[` | Previous recovered draft |

### Analytics

These work in every view.

| Key | Action |
|-----|--------|
| `Esc` | Back to the mailbox |
| `j`, `↓` | Next row |
| `k`, `↑` | Previous row |
| `Tab` | Next view |
| `Shift+Tab` | Previous view |
| `r` | Refresh the view |
| `f` | Filter: every CLI flag for the view |
| `Enter` | Drill down: a sender to a search, a thread to the conversation |

### Analytics: Storage

| Key | Action |
|-----|--------|
| `m` | Breakdown or largest messages |
| `b` | Group the breakdown by sender, type or label |

### Analytics: Stale threads

| Key | Action |
|-----|--------|
| `p` | Mine or theirs |
| `[` | Older than: 7 days less |
| `]` | Older than: 7 days more |
| `{` | Within: 30 days less |
| `}` | Within: 30 days more |

### Analytics: Contacts

| Key | Action |
|-----|--------|
| `m` | Asymmetry, decay or refresh |
| `R` | Rebuild the contacts table |

### Analytics: Response time

| Key | Action |
|-----|--------|
| `d` | Clock or business hours |

### Analytics: Subscriptions

| Key | Action |
|-----|--------|
| `o` | Rank by volume or open rate |
| `u` | Unsubscribe from the selected sender (asks first) |

### Analytics: Wrapped

The year in review moves between tiles instead of rows.

| Key | Action |
|-----|--------|
| `Esc` | Back to the mailbox |
| `Tab` | Next view |
| `Shift+Tab` | Previous view |
| `r` | Refresh the view |
| `f` | Filter: every CLI flag for the view |
| `Enter` | Drill down: a sender to a search, a thread to the conversation |
| `h`, `←` | Tile to the left |
| `l`, `→` | Tile to the right |
| `j`, `↓` | Tile below |
| `k`, `↑` | Tile above |
| `y` | A year back |
| `Y` | A year forward |
| `t` | Year to date, a year, or the last days |

### Deliveries

[Tracked packages](/guides/deliveries/) detected in your mail.

| Key | Action |
|-----|--------|
| `j`, `↓` | Next row |
| `k`, `↑` | Previous row |
| `Enter`, `o`, `→` | Open the source email in a split preview |
| `Esc` | Close the preview |
| `Ctrl+d` | Scroll the open email down |
| `Ctrl+u` | Scroll the open email up |
| `r` | Resolve: mark it delivered |
| `d` | Dismiss a false positive |
| `D` | Active, delivered or all |
| `R` | Refresh the list |

### Assistant pane

`@` opens the pane and gives it the keys. While it has them, nothing you type reaches the screen behind it. See [the assistant guide](/guides/assistant/#in-the-terminal).

| Key | Action |
|-----|--------|
| `Esc` | Give the keys back to the screen |
| `i`, `Enter` | Write a message |
| `@` | Hide the assistant |
| `Ctrl+k`, `Ctrl+p` | Command palette |
| `Ctrl+c` | Stop the running turn |
| `j`, `↓` | Scroll down |
| `k`, `↑` | Scroll up |
| `Ctrl+d`, `PageDown` | Page down |
| `Ctrl+u`, `PageUp` | Page up |
| `G` | Latest message |
| `e` | Open or fold the steps |
| `1`–`9` | Open the reply's link by number |
| `N` | New conversation |
| `L` | Past conversations |
| `M` | Choose the model |
| `T` | Auto-approve reversible changes in this conversation |
| `#` | Delete the conversation (shows what goes first) |

### Assistant: message box

| Key | Action |
|-----|--------|
| `Ctrl+k`, `Ctrl+p` | Command palette |
| `Ctrl+c` | Stop the running turn |
| `Enter` | Send |
| `Alt+Enter`, `Ctrl+j` | New line |
| `Esc` | To the pane keys |

### Assistant: a change waiting for approval

| Key | Action |
|-----|--------|
| `y` | Approve the waiting change |
| `n` | Reject it, with a reason |
| `x` | Show or hide its exact call |
| `a` | Show all or fewer of its messages |

### Assistant: a question waiting for an answer

| Key | Action |
|-----|--------|
| `1`–`9` | Choose an answer by number |
| `o` | Write your own answer |
| `Tab` | Next question |
| `Shift+Tab` | Previous question |
| `Enter` | Answer |
| `s`, `Esc` | Skip the question |

### Writing proof

The proof shown for a draft written in your voice, before it goes into your editor.

| Key | Action |
|-----|--------|
| `y` | Accept writing proof |
| `e` | Accept proof and edit |
| `n` | Revise writing proof |
| `m` | Try writing proof with another model |
| `Esc` | Keep my draft |
| `o` | Open next voice example |
| `←` | Previous writing proof |
| `→` | Next writing proof |
| `↑`, `k` | Scroll writing proof up |
| `↓`, `j` | Scroll writing proof down |
| `PageUp` | Page up in writing proof |
| `PageDown` | Page down in writing proof |
| `Home` | Top of writing proof |
| `End` | Bottom of writing proof |

<!-- tui-keys:end -->

### Overlays

Pickers, confirms and other overlays still read their own keys, so they are
not in the generated tables yet. Each one prints its keys in its footer.

| Context | Keys |
|---------|------|
| Help | `j` / `k`, `Ctrl-d`, `Ctrl-u`, `o`, `Esc` |
| Command palette | typing, `j` / `k`, `Enter`, `Esc` |
| Label picker | typing, `j` / `k`, `Enter`, `Esc` |
| Attachments | `j` / `k`, `Enter` / `o`, `d`, `Esc` |
| Bulk confirm | `Enter` / `y` confirm, `Esc` / `n` cancel |
| Not now (`b`) | Later first, then the presets on `1`–`4`, then Custom; `j` / `k` move, `Enter` picks, `Esc` / `q` close |
| Not now (custom time) | typing (live preview), `Tab` other reading, `Enter` set, `Backspace`, `Esc` back to the list |
| Move to (`X`, `P`) | `m` `x` `u` `r` `e` move this email, `M` `U` `R` the sender (on a new sender, `D` blocks); `j` / `k` move, `Enter` picks, `Esc` / `q` close |
| Move sender to (Paper trail, Reading) | `m` People, `r` Reading, `u` Paper trail, `d` Screened out, `a` Automatic; `Esc` / `q` close |
| Reply queue | `j` / `k`, `Enter` / `r` reply, `F` focus & reply, `Esc` close |
| Snippets browser | `j` / `k`, `Esc` close |
| Sender profile | `j` / `k` select other sender email, `Enter` / `o` open selected email, `W` who is this sender, `Esc` close |
| Welcome / setup | `d` demo, `g` Gmail, `i` IMAP, `Enter` open form, `Esc` dismiss |
| Saved-search form | typing fields, `Tab` / `Shift-Tab` move, `Ctrl-s` save, `Esc` cancel |
| Saved-search delete | `Enter` / `y` confirm delete, `Esc` / `n` cancel |
| Compose send-confirm | `s` send, `d` save as draft, `e` re-edit, `Esc` cancel; on a blocked draft, `O` overrides and sends once (`Ctrl-O` also works) |
| Unsubscribe confirm | `u` unsubscribe + archive, `U` unsubscribe + trash, `a` archive only, `A` archive all from sender, `Esc` cancel |
| Analytics filter modal | typing fields, `Tab` / `Shift-Tab` move, `Enter` apply, `Esc` cancel |
| Error modal | `j` / `k`, `Ctrl-d` / `Ctrl-u` scroll, `q` / `x` / `Esc` close |

## Web app

The web app shares the TUI's default mail shortcuts. One dispatcher reads every key
from the shared action registry (`apps/web/src/lib/actions/`), so these
tables, the in-app help (`?`), the command palette and **Settings →
Keybindings** always agree. Where the web deliberately differs from the TUI,
the note says why, and a test fails if the two clients bind the same key
to different things without a listed reason. Keys never fire while you
type in a field or while compose is open, except `⌘K` / `Ctrl+K` and the
assistant's `⌘.` / `Ctrl+.`.

:::note[Keys that moved]
Tools open with `g` and a capital in both clients: `g R` Rules, `g D`
Diagnostics, `g P` Deliveries, `g C` Settings (was `g c` and `0`) and `g Y`
Activity (was `g y`). The digit tabs, `8`, `9`, `g E`, `g p`, the list's `H`
and `M`, and raw headers' `g H` retired; raw headers are in the palette. The
assistant is `@` as well as `⌘.`, and Summarize folded into the briefing
(`B`). Each old key says where it went until 0.6.180.

The next wave moved these, each saying where it went until 0.6.185: Export
(`E`, in the mail list and Archive) and Updates' sender sweep (`S`) are in the
palette; Archive's mark checked (`v`) is in the `,` dialog; To do's details
(`v`) is in the palette and on the row; Focus skips with `j` (was `s`); the
sidebar folds a section with `Enter` on its header (was `[` and `]`) and
jumps between sections with `{` and `}`; and the assistant's history deletes
with `#` (was `d`). Every mode now takes `g g`, `G`, `Ctrl+d` and `Ctrl+u`.
:::

<!-- web-keys:start (generated from the action registry; run UPDATE_KEY_DOCS=1 npm test) -->

### Everywhere

Work on every page except while typing in a field or compose.

| Key | Action | Note |
|-----|--------|------|
| `⌘K / Ctrl+K`, `:`, `Ctrl+P` | Command palette | Ctrl+P works on macOS only, where it is not the browser's Print; the TUI's Mod+K is Ctrl+k |
| `/` | Search mail |  |
| `?` | Keyboard help |  |
| `c` | Compose |  |
| `u`, `z` | Undo last action | z also undoes, for Gmail muscle memory |
| `@`, `⌘. / Ctrl+.` | Show or hide the assistant | Mod+. also works from a text field; terminals don't pass it through |

### Go to

`g` then a letter, as in the TUI: lowercase for mail places and modes, a capital for tools.

| Key | Action | Note |
|-----|--------|------|
| `g 1` … `g 9` | Open saved search 1 to 9 | In sidebar order, as the TUI's tab strip |
| `g h` | Go to Now |  |
| `g m` | Go to Messages |  |
| `g u` | Go to Updates |  |
| `g e` | Go to Archive |  |
| `g w` | Waiting on | A lane of the desk lens in the TUI |
| `g i`, `g 0` | Go to Inbox |  |
| `g s` | Go to Starred |  |
| `g t` | Go to Sent |  |
| `g d` | Go to Drafts | One page here; in the TUI g d is the Drafts mailbox, and its local drafts are in the palette |
| `g a` | Go to All Mail |  |
| `g l` | Go to label |  |
| `g A` | Analytics |  |
| `g Y` | Activity log |  |
| `g L` | Daemon logs |  |
| `g C` | Settings | The TUI opens its Accounts screen, where its settings live; Edit config is in its palette |
| `g n` | Go to Snoozed | Web only |
| `g #` | Go to Trash | Web only |
| `g !` | Go to Spam | Web only |
| `g q` | Reply later |  |
| `g o` | Owed replies |  |
| `g v` | Calendar invites |  |
| `g r` | Reading |  |
| `g S` | Screener |  |
| `g R` | Rules |  |
| `g D` | Diagnostics |  |
| `g P` | Deliveries |  |
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
| `b` | Not now… |  |
| `D` | Unsubscribe… |  |
| `t` | Make a to-do from this |  |
| `r` | Reply |  |
| `a` | Reply all |  |
| `f` | Forward |  |
| `B` | Thread briefing |  |
| `p` | Sender profile |  |
| `L` | Links in this message |  |
| `A` | Attachments |  |
| `i a` | Accept invite |  |
| `i m` | Maybe (invite) |  |
| `i d` | Decline invite |  |
| `i A` | Accept with comment |  |
| `i M` | Maybe with comment |  |
| `i D` | Decline with comment |  |
| `X` | Move to… |  |

### Mail list

| Key | Action | Note |
|-----|--------|------|
| `j`, `ArrowDown` | Next conversation |  |
| `k`, `ArrowUp` | Previous conversation |  |
| `g g`, `Home` | First conversation |  |
| `G`, `End` | Last conversation |  |
| `Ctrl+D`, `PageDown` | Half page down |  |
| `Ctrl+U`, `PageUp` | Half page up |  |
| `Enter`, `o`, `ArrowRight` | Open conversation |  |
| `h`, `ArrowLeft` | Go to sidebar |  |
| `w` | This list's row action | Web only |
| `x` | Select and move down |  |
| `V` | Visual line mode |  |
| `* a`, `⌘A / Ctrl+A` | Select all |  |
| `* n` | Select none |  |
| `* r` | Select read |  |
| `* u` | Select unread |  |
| `* s` | Select starred |  |
| `Esc` | Clear selection, then close |  |
| `g f`, `Ctrl+F` | Filter this list | TUI Ctrl-f; Ctrl+F works on macOS only, where it is not the browser's Find |
| `P` | Send everything from this sender to… |  |

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
| `o` | Expand or collapse message | The TUI shows every message in full, so o does nothing there yet |
| `;` | Expand or collapse all | Web only |
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
| `O` | Open original in a new tab |  |
| `F` | Full-width reader |  |

### Sidebar

| Key | Action | Note |
|-----|--------|------|
| `j`, `ArrowDown` | Next item |  |
| `k`, `ArrowUp` | Previous item |  |
| `g g` | First item |  |
| `G` | Last item |  |
| `Enter`, `o`, `l`, `ArrowRight` | Open |  |
| `{` | Previous section |  |
| `}` | Next section |  |

### Page controls

On tool and settings pages, focus tabs and filters with `g f`; arrow keys then change the choice.

| Key | Action | Note |
|-----|--------|------|
| `g f` | Focus page tabs and filters |  |

### Screener

| Key | Action | Note |
|-----|--------|------|
| `1`, `a` | Allow sender |  |
| `2`, `d` | Deny sender |  |
| `3`, `f` | Send to feed |  |
| `4`, `p` | Send to paper trail |  |
| `j`, `ArrowDown` | Next sender |  |
| `k`, `ArrowUp` | Previous sender |  |

### Focus & reply

One conversation at a time. In the reply, ⌘Enter sends and moves on; Tab out of the reply (or Esc in the rich-text editor) to use these keys.

| Key | Action | Note |
|-----|--------|------|
| `⌘Enter / Ctrl+Enter` | Send and next |  |
| `j` | Skip for now | Goes to the end of the queue |
| `e` | Done, no reply needed | e on the desk in the TUI |
| `b` | Not now… |  |
| `w` | Send, remind me if nobody replies… |  |
| `d` | Draft in your voice |  |
| `r` | Back to the reply |  |
| `Esc` | Leave focus mode |  |

### Now

On a lit new sender, `X` then `M`, `U`, `R` or `D` answers its question.

| Key | Action | Note |
|-----|--------|------|
| `g g`, `Home` | First conversation |  |
| `G`, `End` | Last conversation |  |
| `Ctrl+D`, `PageDown` | Half page down |  |
| `Ctrl+U`, `PageUp` | Half page up |  |
| `j`, `ArrowDown` | Next |  |
| `k`, `ArrowUp` | Previous |  |
| `Enter` | Open in its mode |  |
| `e` | Done here |  |
| `r` | Reply |  |
| `t` | Make it a to-do |  |
| `o` | Open the email |  |
| `E` | Let go of the Updates digest… | Previews the daemon's dry run first; undo afterwards |
| `Esc` | Dismiss the hint |  |
| `P` | Send everything from this sender to… |  |

### Messages

On a lit new sender, `X` then `M`, `U`, `R` or `D` answers its question.

| Key | Action | Note |
|-----|--------|------|
| `g g`, `Home` | First conversation |  |
| `G`, `End` | Last conversation |  |
| `Ctrl+D`, `PageDown` | Half page down |  |
| `Ctrl+U`, `PageUp` | Half page up |  |
| `j`, `ArrowDown` | Next person |  |
| `k`, `ArrowUp` | Previous person |  |
| `Enter` | Open the person |  |
| `r` | Reply |  |
| `a` | Reply all |  |
| `.` | Got it: a short thanks, after a countdown | Shows the exact text and counts down; u cancels before it sends |
| `e` | Done here |  |
| `t` | Make it a to-do… |  |
| `b` | Not now: reply later… |  |
| `s` | Pin or unpin this person |  |
| `c` | New topic with this person |  |
| `{` | Previous topic |  |
| `}` | Next topic |  |
| `p` | Go to the person page |  |
| `o` | Show the message as sent |  |
| `g f` | Filter the people list: turn, unread, past your pace | Ctrl+f in the TUI opens the same picker; the web filters with g f because Ctrl+f is the browser's Find |
| `f` | Cycle Messages views | Needs attention, All conversations, Your turn, Waiting on them; keeps Unread and Past your pace |
| `⌘Enter / Ctrl+Enter` | Send, then the next person whose turn it is |  |
| `Esc` | Dismiss the hint, or back to the people |  |
| `P` | Send everything from this sender to… |  |

### To do

| Key | Action | Note |
|-----|--------|------|
| `g g`, `Home` | First conversation |  |
| `G`, `End` | Last conversation |  |
| `Ctrl+D`, `PageDown` | Half page down |  |
| `Ctrl+U`, `PageUp` | Half page up |  |
| `j`, `ArrowDown` | Next |  |
| `k`, `ArrowUp` | Previous |  |
| `Enter` | Do it: what the row's button says | The TUI opens the source email; the web opens it or a reply for a promise. Links stay in the email. |
| `e` | Done |  |
| `b` | Not now: schedule… |  |
| `,` | Edit what mxr read… |  |
| `X` | Correct to-do |  |
| `o` | Open the email it came from |  |
| `1` | Runway |  |
| `3` | Expired list |  |
| `2` | Catch-up |  |
| `Esc` | Dismiss the hint |  |

### To do: catch-up

The first run's older undated tasks: keep each one, or let it go to the Expired list. `C` on To do opens it.

| Key | Action | Note |
|-----|--------|------|
| `g g`, `Home` | First conversation |  |
| `G`, `End` | Last conversation |  |
| `Ctrl+D`, `PageDown` | Half page down |  |
| `Ctrl+U`, `PageUp` | Half page up |  |
| `1` | Runway |  |
| `3` | Expired list |  |
| `2` | Catch-up |  |
| `j`, `ArrowDown` | Next |  |
| `k`, `ArrowUp` | Previous |  |
| `Enter` | Keep: it still needs me |  |
| `e` | Let go of this one |  |
| `E` | Let go of all… | Previews the daemon's dry run first; undo afterwards |
| `Esc` | Back to To do |  |

### To do: Expired list

Tasks that expired or were let go. `E` on To do opens it.

| Key | Action | Note |
|-----|--------|------|
| `g g`, `Home` | First conversation |  |
| `G`, `End` | Last conversation |  |
| `Ctrl+D`, `PageDown` | Half page down |  |
| `Ctrl+U`, `PageUp` | Half page up |  |
| `1` | Runway |  |
| `3` | Expired list |  |
| `2` | Catch-up |  |
| `j`, `ArrowDown` | Next |  |
| `k`, `ArrowUp` | Previous |  |
| `Enter` | Restore to To do |  |
| `Esc` | Back to To do |  |

### Updates

| Key | Action | Note |
|-----|--------|------|
| `g g`, `Home` | First conversation |  |
| `G`, `End` | Last conversation |  |
| `Ctrl+D`, `PageDown` | Half page down |  |
| `Ctrl+U`, `PageUp` | Half page up |  |
| `j`, `ArrowDown` | Next |  |
| `k`, `ArrowUp` | Previous |  |
| `Enter` | Show routine |  |
| `E` | Let go of the digest… | Previews the daemon's dry run first; undo afterwards |
| `e` | Let go of this source |  |
| `t` | This needs me: make it a to-do |  |
| `P` | Tune this source… |  |
| `L` | Open the link |  |
| `y` | Copy the one-time code |  |
| `o` | Open the email |  |
| `Space` | Peek at this update under its line |  |
| `s` | Pin or unpin this source's latest email |  |
| `Esc` | Dismiss the hint |  |

### Reading edition

Newsletters as an edition. `b`, `e`, `D` and `A` work on the item or link under the cursor; `D` and `A` preview first.

| Key | Action | Note |
|-----|--------|------|
| `g g`, `Home` | First conversation |  |
| `G`, `End` | Last conversation |  |
| `Ctrl+D`, `PageDown` | Half page down |  |
| `Ctrl+U`, `PageUp` | Half page up |  |
| `j`, `ArrowDown` | Next |  |
| `k`, `ArrowUp` | Previous |  |
| `Enter` | Read |  |
| `L` | Fetch the linked article |  |
| `b` | Not now: later or snooze… |  |
| `e` | Let go |  |
| `D` | Unsubscribe… | Shows the evidence and the method first; nothing is sent until you confirm |
| `R` | Read in the sender's layout |  |
| `E` | Let go of everything shown… | Previews the daemon's dry run first; undo afterwards |
| `o` | Open the email as sent |  |
| `P` | This sender here: move to… |  |
| `1` | Edition |  |
| `2` | Later |  |
| `3` | Highlights |  |
| `O` | Open in Marginalia |  |
| `Esc` | Dismiss the hint |  |

### Reading reader

One item in a 66-character column. The article is fetched only on `L` or the Article tab.

| Key | Action | Note |
|-----|--------|------|
| `1` | Edition |  |
| `2` | Later |  |
| `3` | Highlights |  |
| `j`, `ArrowDown` | Scroll down |  |
| `k`, `ArrowUp` | Scroll up |  |
| `L` | Article or issue: switch between them |  |
| `b` | Not now: later or snooze… |  |
| `e` | Let go and go back |  |
| `n` | Next item |  |
| `R` | Sender's layout or cleaned |  |
| `"` | Highlight the selection |  |
| `D` | Unsubscribe… |  |
| `o` | Open the email as sent |  |
| `O` | Open in Marginalia |  |
| `Esc` | Back to the edition |  |

### Archive

| Key | Action | Note |
|-----|--------|------|
| `g g`, `Home` | First conversation |  |
| `G`, `End` | Last conversation |  |
| `Ctrl+D`, `PageDown` | Half page down |  |
| `Ctrl+U`, `PageUp` | Half page up |  |
| `j`, `ArrowDown` | Next record |  |
| `k`, `ArrowUp` | Previous record |  |
| `/` | Ask Archive |  |
| `y` | Copy the reference |  |
| `Y` | Copy the amount |  |
| `Enter` | Open the document |  |
| `o` | Open the email |  |
| `p` | This issuer's records |  |
| `{` | Previous year |  |
| `}` | Next year |  |
| `,` | Fix a field… |  |
| `X` | Not a record |  |
| `t` | Make a to-do from this record |  |
| `g f` | Filters… |  |
| `Esc` | Close |  |

### Arrivals

The emails behind one of Now's arrival counts. `X` (in Mail actions) moves the email under the cursor.

| Key | Action | Note |
|-----|--------|------|
| `P` | Send everything from this sender to… |  |
| `j`, `ArrowDown` | Next |  |
| `k`, `ArrowUp` | Previous |  |
| `Enter`, `o` | Open conversation |  |

### Compose

While writing a message. ⌘ and Ctrl both work, on every platform.

| Key | Action | Note |
|-----|--------|------|
| `⌘Enter / Ctrl+Enter` | Send |  |
| `⌘⇧Enter / Ctrl+Shift+Enter` | Send and archive |  |
| `⌘⇧L / Ctrl+Shift+L` | Send later… |  |
| `⌘S / Ctrl+S` | Save draft |  |
| `⌘⇧S / Ctrl+Shift+S` | Save to server draft |  |
| `⌘⇧R / Ctrl+Shift+R` | Refresh from daemon |  |
| `⌘Backspace / Ctrl+Backspace`, `⌘⇧Backspace / Ctrl+Shift+Backspace` | Discard draft |  |
| `⌘⇧C / Ctrl+Shift+C` | Add Cc |  |
| `⌘⇧B / Ctrl+Shift+B` | Add Bcc |  |
| `⌘⇧A / Ctrl+Shift+A` | Attach files |  |
| `⌘⇧G / Ctrl+Shift+G` | Insert signature |  |
| `⌘; / Ctrl+;` | Insert snippet |  |
| `⌘J / Ctrl+J` | Draft for me |  |
| `⌘⇧F / Ctrl+Shift+F` | Toggle fullscreen |  |

<!-- web-keys:end -->

### How the web app reads keys

- **Input methods.** Keys that belong to an input method's composition are
  ignored. `Enter` that picks a candidate does not submit a field or run a
  shortcut, and `Escape` that cancels a composition does not close the
  highlight note.
- **Other layouts.** A character you type with `Option` on macOS or `AltGr`
  elsewhere matches its binding as that character. `#` typed as `Option-3` on a
  UK Mac works as `#`, and `[` `]` `{` `}` typed with `AltGr` on German
  and French layouts work. `Alt` with a plain letter or digit stays a chord.
- **Mistyped prefixes.** After `g`, a key that completes no shortcut cancels the
  sequence and runs nothing. The status bar says `g U is not a shortcut` for two
  seconds. A modifier chord such as `⌘K` still runs.
- **Held keys.** Holding `j` or `k` repeats in every mode, not only the mail
  list. Every other action runs once per press.
- **Menus and dialogs.** A shortcut pressed right after `Esc` closes a menu
  works at once. When Help or a dialog closes, focus returns to the pane you were
  in.

### Customization

The web keymap is built into the app; there is no `keys.toml` equivalent
yet.
