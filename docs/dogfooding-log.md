# Dogfooding log

Five working days of real use of the web app, required by the experience
rubric (E2) before any section-A criterion can score 3. Write it as you go.
Short, honest notes beat complete ones.

## How to use it

Use the web app as your main email client (`mxr web`). Each day, fill in the
three questions below. When you reach for another client (Gmail, the TUI,
the CLI), note why. That is the most useful signal.

## Days

### Day 1 (date: )

- **What worked:**
- **What didn't:**
- **When I reached for another client, and why:**

### Day 2 (date: )

- **What worked:**
- **What didn't:**
- **When I reached for another client, and why:**

### Day 3 (date: )

- **What worked:**
- **What didn't:**
- **When I reached for another client, and why:**

### Day 4 (date: )

- **What worked:**
- **What didn't:**
- **When I reached for another client, and why:**

### Day 5 (date: )

- **What worked:**
- **What didn't:**
- **When I reached for another client, and why:**

## One-off checks

### Gist accuracy (A11)

Open your desk with a local model enabled. For ten people rows, compare the
gist and "Asks:" line with the email.

| Row | Gist right? | Ask right? | Notes |
|---|---|---|---|
| 1 | | | |
| 2 | | | |
| 3 | | | |
| 4 | | | |
| 5 | | | |
| 6 | | | |
| 7 | | | |
| 8 | | | |
| 9 | | | |
| 10 | | | |

### Copy review (C3)

Read the desk, Reading, Paper trail, focus mode, an empty desk, the daemon
stopped banner and a refused action. Note any wording that nags, scolds,
counts backlog, or would make you feel behind.

-

### Sound (B4) and touch (B9)

- Sound on real speakers (Settings, Sound): volume, the low tide figure,
  whether anything grates.
- Swipes on a phone: thresholds, vertical scrolling, whether a fast flick
  ever trashes.

-

## Key-only run, web, 2026-10-29, automated on demo mail

Blueprint 35's K3 acceptance asks for a day of triage using only keys. This
run was a stand-in for that day, not a replacement: an agent drove the web app
with Playwright keyboard input only, no clicks, against the e2e daemon's
FakeProvider demo mailbox (`scripts/e2e-server.mjs`). The app was built from
`origin/main` at `7c6828dce` (v0.6.194) in Chromium at 1440×900, with a second
pass at 390×844. It covered Now, Messages, To do, Updates, Reading, Archive,
Inbox, Search, Snoozed, Focus & reply, Settings → Keyboard, the palette, help
and the `g` and `*` guides. Each fix landed on `fix/key-only-web-dogfood` with
a Vitest test.

### What broke and how it was fixed

| Place | Pressed | What happened | Expected | Fix |
|---|---|---|---|---|
| Messages | `r`, then typed a reply | The reply opened but focus stayed on the page. The typed words ran as shortcuts: `s` pinned Noor, `t` opened Make a to-do, `u` undid. Only the first reply after a page load did this. | The cursor in the reply body | `863892fd1`: autofocus waits for the lazily loaded editor to mount |
| Messages | `b`, `2` (Not now, the weekend) | Toast said "back Saturday", and the person stayed listed. The daemon had already dropped them. | The person leaves Messages | `282ec30a5`: mail changes refresh Messages' queries |
| Now | `?` | "Now (this view)" left out `b` and `X`, which work there. "Mail actions" came second, with `e` listed as Archive. | Now's own keys, `e` as Done | `ca08e97cc` |
| Messages | `p`, then `Esc`, `q`, `h` | `p` focused the topic picker. Nothing brought focus back, and `Enter` then opened the picker. | `Esc` returns to the people | `563e1ae73` |
| Messages, Settings | `g f`, then `Esc` | Focus stayed on the filter or tab, and `Enter` toggled the filter. | `Esc` steps back to the pane | `e24cebb0b` |
| Keybindings page | — | The catch-up and Expired intros said `C` and `E` open them. Both moved to `2` and `3`, which their moved-key hints confirm. | The digits | `d2a217fc1`: intros read the definition |
| Any mode after `h` | `h`, `g a`, `j j` | `j` moved the sidebar's hidden cursor, and All Mail's list got none. Dialogs closed onto the sidebar. | The new page has the keys | `d7668aa07`: navigating by key releases the sidebar |
| Links, Correct this to-do, pickers | `q` | Nothing happened. An open mail dialog suspends page keys, and the q-closes rule only ran when nothing was suspended. | Closes, as `Esc` does | `c91b253da` |
| Reading reader | `"` with nothing selected | "Select some text first, then press h". | `"` | `683041ff4`, which also fixed Snoozed's "Press Z" (now `b`) and the reader's "press y again" (summary is in the palette) |
| To do, Updates, Archive | `o`, then `Esc` | The reader closed and focus fell to `<body>`. Inbox returns focus to the list. | Focus on the mode | `617d9d293` |
| Archive | `/`, typed "hotel", `Enter`, `Esc`, `j` | `Esc` cleared the search but moved focus into the Ask box, so `j` typed "j". | Cleared, keys on the list | `cbd963973` |
| Any list | `g`, pause | The guide showed the first 20 chords in registry order. To do (`g x`) and Reading (`g r`) fell past the cut, while `g 0` took a row of its own. | Places and modes first | `13eefd262`: one row per action, ordered as the `g` table |
| Inbox | `/`, `↓`, `Enter` | The result opened, but the list kept the keys. `j` jumped to another conversation, and `e` would have archived the list's lit row. | The reader has the keys | `0bc3e347c` |
| Every page | `e`, `x x e` | After two ordinary archives, "Archive recovery (2)" sat in the corner of every page. | Recovery only for unfinished archives | `124da2e3e`: the panel lists unaccepted, running or unconfirmed operations; the journal is unchanged |
| Settings → Keyboard | pencil, `Enter`, Save or `Esc` | Save dropped focus to the page, so the next change started from the top. `Esc` in the field did nothing. | `Esc` cancels; focus back on the pencil | `b51ef4464` |
| Settings → Keyboard | single keys off, then `?` | Nothing happened. The keybindings page said "`?` then says so". | See the decisions below | `d2a217fc1` |
| Updates | `P`, `j`, `↓` | Focus stayed on the first Tune choice. Only Tab moved. | `j`, `k` and the arrows move | `bdf629980` |

### What worked

These did what help and the definition say, with focus back on the pane
afterwards:

- In Now: `j`, `k`, `Enter`, `e`, `u`, `t`, `X`, `q`.
- In Messages: `e` with advance, `.` and `u` cancelling it, `f`, `X`, and `⌘↵` with the 10-second Undo window. `z` cancelled a send.
- In To do: the digits `1` to `3`, `X`, `,`, `e`, `o`. Old `C` and `E` say where they went.
- In Updates: `Space` peek, `e` (it previews when the source has several updates), `E` with its dry run and `n`.
- In Reading: `Enter`, `b` then `Enter` for Later, the digits, `Esc`.
- In Archive: `Enter` opens the PDF, and the `,` dialog.
- In the Inbox: `j`, `Enter`, `s`, `l` with typing and `⌘↵`, `e` advancing the reader, `h`, `x`, `* u` and bulk `e` with `u`.
- `g n` then `w` woke a snoozed conversation, and `g F` then `q` left Focus & reply.
- The palette's "Next time: s" toast, the `*` and `g` guides, and help's filter.
- With single keys off, `e` did nothing and `⌘K` still ran Archive.
- At 390×844, the same list, reader and Messages flows worked, and the guide fits the screen.

### Calls made without asking

These follow blueprint 35's rules. BK said not to ask.

- `?` stays off with single keys off. It is one character, and dictation can
  type it, which is why WCAG 2.1.4 wants such keys off. Help opened from the
  palette already says the switch is off. The keybindings page and blueprint 35
  now say that instead of "`?` says so" (`d2a217fc1`).
- Now has no `a` or `f`, though the verb table lists reply all and forward for
  Now. Adding them needs handlers in both clients, because the definition's
  parity check allows a difference only when a client can't deliver the key or
  lacks the feature. The TUI side is being worked on in parallel
  (`fix/key-only-tui-dogfood`), so this is left as a follow-up for both
  clients rather than made web-only.
- To do's `Space` peek is in the modes table but in neither client; Details is
  palette-only. It is a feature to build, not a broken key, so it is left as
  blueprint work.
- Reading highlights need a text selection, which keyboard users can't make
  in the reader without caret browsing. `"` with no selection could highlight
  the paragraph under the cursor, but the Reading reader has no paragraph
  cursor (`j` and `k` scroll). That is a feature, so it is noted, not built.
- Settings' section links are one Tab stop each: 10 Tabs from the Keyboard link to
  the single-key switch. The blueprint keeps header links as links, so this
  stays. `g f` and the arrows move between them.
- A reply starts in vim normal mode, so `i` comes before typing. This is the
  default editor setting, and a `normal` badge shows it. It stays.
- `⌘⌫` (discard) does nothing inside the editor, where it deletes to the
  start of the line. This is by design: Tab out first.
- The key editor's preview spells a sequence and a list differently:
  "Archive will be e %" is the sequence `e` then `%`, while "e, %" is two
  keys. The field's placeholder and hint say keys are comma-separated. It
  stays.
- `y` on an Updates line with no code does nothing and shows no toast. It is
  minor and stays.

## Key-only run, TUI, 2026-10-29, automated on demo mail

The TUI half of the same stand-in for K3's day of key-only triage. An agent
drove the TUI through tmux `send-keys` only, at 160×48, against an isolated
`mxr demo --messages 2000` profile (its own `HOME`, socket and binary name),
with a script as `$EDITOR` so replies ran headless. The binary was a debug
build of `origin/main` at `7c6828dce` (v0.6.194). It covered Now, Messages
(open a person, `r`, `d`, resume, `e`, `b`, `X`), To do and its tabs, Updates
with the `Space` peek, Reading and its reader, Archive, the mail list (`j`,
`k`, open, `e`, `s`, `l`, `x`, `*`), Search, Snoozed and `w`, Focus & reply,
the reader's `o`, `;`, `]`, `J`, `n`, help, the palette (`:` and `Ctrl-k`),
the `g`, `*` and `i` guides, `q` and `Esc`, a `keys.toml` remap and the
single-key switch. Each fix landed on `fix/key-only-tui-dogfood` with a TUI
runner test, or a daemon test where the daemon was wrong.

### What broke and how it was fixed

| Place | Pressed | What happened | Expected | Fix |
|---|---|---|---|---|
| Now | `Enter` on a person | The reader hints and "View: Original HTML" appeared and the email was marked read, but nothing on screen changed: the email opened in a reader the Now page never draws, and every key went to it. | Messages on that person, as the web's Enter does | Enter opens Messages with the person lit and their page on that conversation |
| Now, Messages, Updates, Reading, Archive | `o` (the email) | The same invisible reader. Paper trail, calendar invites and Now's Not-sure and Reading rows took the same path. | The email, on screen | An email opened from a mode opens full width; `Esc` comes back to the mode, and `F` keeps it full width |
| Now | (while loading) | The key line read "now.open open in its mode  now.done done here …". | Keys, not action ids | The compiled-in fallback resolves keys from the definition |
| Now's Updates card, Updates header | — | "A let go of this digest", "A let go all". `A` retired; the strip under it said `E`. | `E` | Both read the key from the definition |
| Messages | — | The strip said "[ ] topic"; topics step with `{` and `}` since K3. | `{ } topic` | The strip reads the definition |
| Now's arrivals list | `Enter` on the arrivals line | The footer said "K sender"; the key is `P` and `K` does nothing there. | `P sender` | The footer reads the definition |
| Help | `?` | "How do I…" questions ran into their answers: "Repair an unhealthy accountgC → …". | A gap | The first column fits its longest entry |
| Send review | — | "Press Ctrl-A to add the top suggestion to Cc." `Ctrl-a` is GNU screen's prefix, which a TUI default never takes. | A key every terminal sends | `c` adds to Cc; `Ctrl-a` says where it went until 0.6.225. A keymap test now fails on any TUI binding whose only keys a terminal can't send |
| Messages | `r`, `d` (save draft), `r` | The second `r` started a new, empty reply; the row didn't know about the first. | The saved reply | Saving or sending a reply refetches Messages, so the row names the draft and `r` continues it |
| Messages | `r` with two drafts on the conversation | The row read "Draft: Thanks, sounds good." but `r` opened the older, empty draft. | The draft the row shows | The daemon resumes the oldest draft with text first, the rule the row already used |
| Not now | `b` at 18:32 | "2 Tonight (18:00)" was offered; picking it snoozes to tomorrow evening. | Not offered, as in the web app | Tonight leaves once the evening hour has passed, and the digits number what is shown |
| Snoozed, any list | `b`, `1` on snoozed mail | "Mutation Failed … UNIQUE constraint failed: snoozed.message_id". Snoozed's own `b` ("sends it away again") failed every time. | A new wake time | The daemon moves the wake time and keeps the labels from before the first snooze, so waking still restores the inbox |
| To do | `m` | Opened "Might need you". `m` wasn't in the definition, so help and the palette never showed it, and it borrowed the mail verb Mark read and archive. | Rules 1 and 8 | "Review what might need you" is a palette action; `m` says so until 0.6.225 |
| Updates | `e` on a source | The preview was titled "Let go of this digest" for one source. | "Let go of this source" | The title follows the scope |
| `g` guide | `g`, pause | "1 Saved search 1" to "9 Saved search 9" with four saved searches: five dead keys. | The saved searches there are | The guide names them and leaves out empty digits |
| Any place | `g i` (also `g t`, `g d`, `g #`, `g !`) | Nothing. The demo's system labels are `Inbox`, `Sent`, …; the TUI looked for `INBOX` exactly. | The mailbox | System labels match whatever their case |
| Starred | `g s` | Starred mail listed under the title "All Mail Threads (36)". | "Starred" | A mailbox listed by role is titled by its role |
| Search | `/`, a query, `Enter` | At 160 columns the subject column was four characters ("Re:… +1"). | Readable subjects | A list under 100 columns gives the sender 14 columns, not 22 |
| Palette | `:` | Ten of the eighteen groups (Compose, Triage, Move, …) showed a `?` glyph beside their command, which reads as the help key. | No stray key | A neutral `·` |
| Startup, inside tmux | the first key | The first key after the TUI started was always lost. ratatui-image's capability query goes through tmux passthrough, which tmux drops when `allow-passthrough` is off (its default) or the pane isn't visible (this run's detached session). The query times out, and its reader thread stays blocked on stdin and takes the first key. | Every key arrives | After a timed-out query the TUI asks for a plain device status report, which tmux answers, and the stranded reader stops. The first `?` opened help in the rerun |

### What worked

These did what help and the definition say:

- Now: `j`, `k`, `X` on a Not-sure row then `m`, `u` undoing it, `?`.
- Messages: `Enter`, `}` and `{`, `r` into `$EDITOR` and the send review, `d`, `s`, `e` with its toast and hint, `b` then `Enter` (into the reply queue), `X`, `f`, `Esc` dismissing the hint.
- To do: `1`, `2`, `3`, `e` and `u`, `o` (To do draws its own reader).
- Updates: `Space` peek, `e` and `E` previews.
- Reading: `Enter`, `"`, `2`, `3` with a highlight, `Esc`.
- Archive: `g f` cycling kinds, `→` card, `Esc`.
- Mail list and reader: `j`, `k`, `Enter`, `o` and `;` folding, `]` archiving then opening the next, `J`, `n`, `s` with its confirm, `l`, `x`, `* u`, `Esc` clearing, `q` closing the reader.
- Search: `/`, typing, `Enter`, `j`, `Enter`, `Esc` back out.
- Snoozed: `g n`, `w` ("Back in your inbox").
- Focus & reply: `g F`, `j`, `r`, `s` moving to the next, `Esc` back to where it opened.
- Help (`?`, `q`, `?` toggling), the palette (`:`, `Ctrl-k`, filtering, `Esc`), the `g`, `*` and `i` guides, `g z` saying "g z is not a shortcut".
- `mxr keys set mail.archive %` showed `%:Archive` in the list after a restart, with Now's `e` still Done. `mxr keys single-keys off` stopped `?` and `e`, the status line said "Ctrl-k opens the palette", and `Ctrl-k` still opened the palette.

### Decisions

Made under blueprint 35's rules, without asking, as BK asked:

- Now's `Enter` on a person goes to Messages (rule 8, the web's behaviour); `o` still opens the email itself.
- To do's "Might need you" gets no single key. Rule 2 gives a new verb the palette until it proves daily, and rule 1 keeps `m` for Mark read and archive. The web lists these under the runway with no key, so the definition records the action as TUI-only with that note.
- Add-to-Cc takes `c`, the letter the overlay grammar prints on the option. An undeliverable alias beside a key every terminal sends (`Alt-Enter` beside `Ctrl-j`, `Ctrl-s` beside `Enter`) stays, as the blueprint keeps those aliases; the new test fails only when a binding has no key a terminal can send.
- With single keys off, the strips and the hint bar still name single keys. The web app does the same, help's title says the switch is off, and each press says `Ctrl-k` opens the palette. It stays.
- `q` in a mode quits the TUI. The modes are root places, as the mailbox is (`below_root`), so this is the documented "quits only at the root". It stays.

### Not fixed

- `mxr demo --no-tui` in a debug build overflowed the main thread's 8 MB stack at "Seeding demo surfaces"; it ran with `ulimit -s 65520`. Not checked in a release build. It is outside the TUI and is left for a demo-seeding pass.
- Reading's `3` once showed no Highlights panel and no message right after `"` saved a highlight. A fresh session showed the panel every time, so it is noted, not changed.
- Small copy points, left as they are: Archive's `g f` cycles kinds though help calls it "Filters…"; the `X` picker in Messages doesn't mark `m` as "keep it here" as Now's does for Updates; the reply-later queue's title says `↑/↓` though `j` and `k` work.
