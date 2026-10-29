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
