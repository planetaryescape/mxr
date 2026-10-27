---
title: Web app
description: How the mxr web SPA is served, how it auto-authenticates on the local machine, and how to point it at a remote bridge.
---

The web app is mxr's human client for a daemon-backed mailbox. When it is
connected to the same runtime, it shares state with the CLI, TUI and agent
clients, while each client keeps its own authority. Start with [Now](/guides/now/)
or follow [Your day](/guides/your-day/) through the five modes.

The web app is a React SPA that talks to the daemon through its
HTTP/WebSocket bridge. The TUI and CLI talk to the same daemon over its
local socket. It's embedded into the
`mxr` binary when built with `--features web-ui` and served at the
daemon's bridge port.

## Quick start

```bash
# In one terminal
mxr daemon --foreground

# In another
mxr web
```

`mxr web` starts or reopens the detached local bridge, opens your
default browser to `http://mxr.localhost:42829`, then returns control to the
terminal. Run `mxr web` again to reopen it, or `mxr web stop` to stop
the detached bridge. On the same machine the SPA auto-authenticates
against the daemon, with **no token paste prompt**.

If you'd rather just see the URL, pass `--no-open` or `--print-url`.

Press `?` in the web app to open the view's shortcuts; type to filter the
list. The dialog links to the [account setup guide](/guides/accounts/).
Open **Settings > About** to check the connected daemon version. It says
mail is stored on the computer running mxr. If no version is available,
About shows **Unavailable**.

## Install mxr as an app, and keep extensions out of its keys

### Install it

Chrome, Edge and Brave offer to install the web app when it is open. While
the browser can install it, the status bar shows **install app** and the
command palette has **Install as app**. Installed, mxr opens in its own
window. Hide the offer with its **×**; it stays hidden after that. Firefox
and Safari do not fire the install event, so the offer does not show there.

### Stop extensions reading mxr's keys

Extensions that run on every page can take mxr's single keys (`j`, `k`,
`g`, `/`) before the app sees them. Give mxr's address its own rules.

- **Chrome:** Open the extension's **Details** from **Manage extensions**.
  Under **Permissions**, either set site access to **When you select the
  extension**, or remove `mxr.localhost` from **Allowed sites** with
  **More** → **Remove**. Chrome's own help uses these names.
- **Firefox:** Open the extension from **Add-ons and themes** → **Extensions**
  and turn off its access to data on websites. Mozilla's docs do not show
  a per-site setting for this, so check your version's options for one.
- **Safari:** Open **Safari Settings** → **Extensions**, choose the
  extension, and set `mxr.localhost` to **Deny** under its websites.

Keyboard-shortcut extensions such as [Vimium](https://github.com/philc/vimium)
clash with single-key shortcuts. In Vimium, add an excluded URL rule with
the pattern `*mxr.localhost*`. Vimium patterns are JavaScript regular
expressions, and `*` matches any run of characters. Leave the excluded keys
empty to turn Vimium off on that address. Vimium's options page and its
toolbar popup both edit these rules.

## Demo mode in the web app

`mxr web` works transparently while demo mode is active. After running
`mxr demo`, the bridge binds to its own demo port (namespaced by
`MXR_CONFIG_DIR`, so the real and demo bridges can coexist on different
ports), and the web app's topbar shows a small amber **DEMO** pill next to
the breadcrumb. The pill stays visible on every route, so a recording always
shows which profile is being demoed.

Behind the scenes the SPA polls `/api/v1/admin/status` and reads the new
`is_demo` boolean (`true` when the daemon is bound to the `mxr-demo`
instance). Run `mxr demo stop` to exit demo mode; the next refresh hides the
pill and routes return to your real profile.

## How auto-authentication works

The bridge exposes `GET /api/v1/auth/local-token`, which returns the
bridge bearer token **only** to callers whose TCP peer is a loopback
IP. On first load the SPA calls that endpoint, stores the token in
`localStorage`, and proceeds. If the token in `localStorage` becomes
stale the SPA repeats the handshake automatically.

The endpoint returns `404` (not 401) when:

- `[bridge].auto_local_token = false` in the file printed by `mxr config path`, or
- the caller is not on the same machine (the bridge is bound to a
  non-loopback address and the request originates from a different host).

That means cross-network scanners can't even tell the endpoint exists.

To disable the same-machine handshake (strict bearer auth even on
loopback, useful on multi-user machines), set:

```toml
[bridge]
auto_local_token = false
```

In that mode the SPA falls back to a paste-token panel at
`/settings/token`.

## Port behavior

The bridge uses port **42829** for the stable local URL. On `EADDRINUSE`
it fails by default and prints best-effort process details for the
listener using that port. Pass `--auto-port` to try the next free port
(up to 32 attempts). The actual bound port is written
to `<config_dir>/bridge-port` for the active runtime identity so:

- The Vite dev proxy (`apps/web/`) reads it to know where to send `/api`.
- Scripts can read it instead of hardcoding `42829`.
- `mxr web --print-url` prints the URL with the port it actually bound.

Detached `mxr web` also records `<data_dir>/web.pid`,
`<data_dir>/web.port`, and `<data_dir>/web.host` so later `mxr web`
runs reopen the same process and `mxr web stop` can terminate it.

Port conflicts fail fast unless you pass `mxr web --auto-port`.

## Remote access

First-class public bridge hosting is reserved for a later TLS/auth design.
For now, keep the bridge bound to loopback and reach it through a private
tunnel.

SSH tunnel example:

```bash
ssh -L 42829:127.0.0.1:42829 user@vps.example.com
```

Then open `http://mxr.localhost:42829` locally. Tailscale/WireGuard work
too, but keep the bridge itself loopback-bound on the host running mxr.

### Manual remote-host mode

When the daemon runs on a VPS, open the browser pointed at it:

```bash
mxr web --remote-host mxr.example.com
```

This **does not bind a local bridge**. It reads the per-host token from
`bridge-tokens/<host>.token` next to the active config file (mode 0600;
place it there yourself) and opens the browser to
`https://<host>/#token=<token>&remote=<host>`.

A `#remote=` link that points the app at a bridge other than the one serving
the page asks you to confirm before it switches. The app never sends its
local token to the new bridge: the link carries its own `#token=`, or you
paste one at `/settings/token`.

This mode is for manually configured remote bridges only; public/LAN bridge
serving remains future product work. Requirements on the remote side:

- TLS termination (Caddy / nginx / Cloudflare). The bridge itself does
  not terminate TLS today.
- `[bridge].cors_allowlist` includes your browser's origin.
- `[bridge].host_allowlist` includes the public hostname (defends
  against DNS rebinding).
- `[bridge].auto_local_token = false`. The handshake only checks that the
  TCP peer is loopback, and a reverse proxy on the same host connects from
  loopback, so with the handshake on, the bridge would hand its token to
  anyone who can reach the proxy.

Typed times follow the browser, not the server. The web app sends its
time zone with every time it resolves, so "tomorrow 9am" typed in London
means 09:00 in London even when the daemon runs on a UTC server. The
snooze and send-later presets use the browser's zone too. The CLI and TUI
run on the daemon's machine and use its local zone.

## Development against a running daemon

Inside `apps/web/`:

```bash
npm run dev
```

Vite serves the SPA at `http://localhost:5173`, proxying `/api/*` and
the WebSocket to the bridge. It discovers the bridge port via the active
runtime identity's `<config_dir>/bridge-port`. By default a dev Vite
server looks at `mxr-dev`; set `MXR_INSTANCE=mxr` only when you
intentionally want it to talk to the installed runtime.

Set `MXR_BRIDGE_URL=http://127.0.0.1:9000` to override.

## The page header

Every page opens with the same header: a title, one line saying what the
page is for, and at most one main button on the right. Anything else the page
can do is in the **⋯** menu next to it, and a page with sections (Reading's
Edition and Later, every Settings section on a wide screen, Screener's Queue
and Decisions) shows
them as tabs under the title. The header keeps the same height from page to
page at a given width, so moving between pages never shifts what is under it.
Filters and pickers, such as the Screener's account picker, sit at the top of
the page's content instead.

Settings is one page with one navigation. On a wide screen its sections
(Appearance, Reader, Marginalia, Keyboard and the rest) are the tabs under the
title, the current one underlined; the app's rail is the only left-hand menu.
On a phone, **More > Settings** opens a list of sections, each of which opens
on its own page with a **Settings** link back to the list.

## Now and the rail

The web app opens on [Now](/guides/now/): the few things that need you now,
from every mode, at most ten in four sections. Acting on a row does it in
that row's own mode, and `e` is done here: other modes keep the
conversation, and the toast says whether the email was archived. To answer
everyone you owe in one sitting, use [Focus & reply](/guides/focus-and-reply/).
[Work through your day](/guides/your-day/) puts them in order.

To open on the inbox instead, set **Settings > Appearance > Home** to
**Inbox**. The arrival-order inbox shows everything.

In **Settings > Appearance > Contrast**, **Match the system** follows your
OS contrast preference and is selected initially. Choose **Default** for
standard contrast regardless of that preference, or **Increased** for stronger
text and control borders. mxr remembers this choice in this browser,
separately from your mail account.

The sidebar is the rail, set dimmer than the page so the page stays the
brightest thing on screen. It lists Now, Messages, To do, Updates, Reading,
Archive, then Inbox, each with its `g` key. Reading is the
[edition](/guides/reading/), with no count on the rail. Only Now carries a
badge (people whose turn it is and things to act on); To do and Messages show
a quiet count, and the Screener shows how many senders wait while any do.

Below Inbox are four groups, each folded until you open it:

- **Mail**: Starred, Sent, Drafts, Snoozed, Reply later, All Mail, Spam, Trash.
- **Views**: Waiting on, Owed replies, Screener, Subscriptions, Invites,
  Deliveries.
- **Tools**: Search, Analytics, Rules, Accounts, Activity, Jobs, Diagnostics,
  Settings.
- **Labels**: your labels, each with a small square in its colour.

From the keyboard (`h` from the list), `j` and `k` stop on each group's
heading too: `Enter` there opens or folds the group, and `{` and `}` jump
between headings.

Saved searches and categories follow when you have any. The rail keeps these
items when the daemon is unreachable: they stay in place and the page shows
the offline banner. Collapse the rail with the arrows at its foot and it shows
only the modes and Inbox, each with its key in a tooltip; the **More places**
button there opens the same list as the phone's More sheet. Unread mail still
shows as bold rows, but no list carries an unread badge.

Updates (`g u`) is the briefing: the latest digest in Needs a look,
Changed and Routine, one line per source, with what arrived since the cut
below it. `E` lets go of the digest after a preview, `e` lets go of one
source, `t` makes a to-do from a line, `P` tunes the source, `L` opens the
line's link and `o` the email. **Let go of all** sits in the page header at
every width; on a phone the sections stack under it. See
[Read Updates as a briefing](/guides/updates/).

Rows stay quiet until you are on them. On every list (Now, Messages, To do,
Updates, Reading, Archive and the Inbox) one row is lit, a lighter surface
with a thin cyan line on its leading edge. The first row is lit when a page
opens, and a lit row opens nothing. `j` and `k` move it at once, a click moves
it with a short fade. The lit row's keys ("r reply", "e done") take the
place of its marks on the same line, and its time stays after them, so no row
changes height as the lamp moves, and pointing at a row shows its keys
without lighting it. The
status bar names the page's three to five main verbs with their keys, and the
whole key list is one `?` away; whatever doesn't fit drops off the end a whole
verb at a time, and the reason shortens with an ellipsis first (hover it for
the full text). Touch layouts show no key chips. Why the lit row is there shows in the status
bar: "4m ago · Here because: Maya wrote to you 22h ago". On a phone, which
has no status bar, the reason is on the person's page. Unread rows have a thin
bar in the margin and a heavier name, and the bar steps aside while the row
is lit. A to-do's due date turns amber on the day it is due and after.

The address bar keeps what you have selected: the row in To do, Updates,
Reading and Now, Updates' open peek, Archive's filter, question, record and
subscription, the person and topic in Messages, the conversation on screen in
Focus & reply, and Issue or Article in the Reading reader. Copy the URL to
send someone straight to it, or reload without losing your place. Moving the
cursor replaces the address rather than adding to the back button's history.
A link to an email Messages doesn't hold opens it in All Mail.

The reader names the other modes holding a conversation ("Also in To do:
sign by Mon 5 Oct"). A row on Now or Messages shows a glyph per other mode
instead, and a first-time person's row carries a `new` mark: light it and
press `1` to `4` (Messages, Updates, Reading, Block) to say where their mail
belongs.

On a phone the sidebar gives way to five tabs at the bottom: Now, Messages,
To do, Reading and More. **More** opens a sheet with Updates, Archive, Inbox
and the Mail, Views, Tools and Labels groups, so Settings, the Screener,
Drafts, Snoozed and every label are a tap away. The lit tab always matches the
page: Updates, Archive, the Inbox, a conversation in a label and every tool
page light More. Escape or any tap on a link closes the sheet. Search is the
field in the top bar; there is no second one.

## Archive

**Archive** (`g e`) opens on its ledger. Press `/` for the answer box and
type what you remember, like "lisbon booking ref", and the answer card shows the field, where it came
from and whether it is checked, with `y` to copy it, `Enter` for the PDF and
`o` for the email. When no record matches, the card says so and shows what
`mxr ask` found in all your mail. Below are the "Coming up" strip, kind
segmented control (Receipts, Orders, Trips, Bills, Documents), filters (`g f`) and the
ledger by month with totals. Select a record for its card on the right: every
field with its source on hover, the word "unchecked" beside a field mxr isn't sure of,
its documents and the emails it came from. `,` fixes a field or marks the
card checked, `X` says it isn't a record, `p` opens the issuer's page, `{` and
`}` step through years, and **Export…** in the palette writes a CSV after its
preview. On any
conversation, `T` files it in Archive after showing the card. On a phone,
Archive is in **More**, and a record's card opens full screen.
See [Archive](/guides/archive/).

### Recover a mailbox Archive action

The mailbox Archive action removes messages from the provider Inbox. It is
separate from filing a record in Archive or using **Done** on the desk. Press
`e`, or use the Archive button, swipe or hover action, and the conversation
leaves the list at once. There is no confirmation dialog: the toast offers
**Undo**, and a second Archive of a message that is still being archived does
nothing. Behind the scenes the web app still reviews the selection with the
daemon, keeps a record of it in the browser, submits exactly what was reviewed
and polls the outcome.

If the action loses its response, open the **Archive recovery** panel (it shows
a count of retained operations) and choose **Query outcome** on the retained
operation. **Submit reviewed selection** or **Resume undispatched items** sends
that same reviewed operation again, only to continue `pending` items. An
`unknown` item may have reached the provider; query again and do not start a
second Archive action for it.

## Reading and triage

The web app works like the TUI: a list of conversations, a reader beside it,
and the same keys. Press `?` anywhere for the keys that apply to the view
you are in. This section names actions; the
[keybindings reference](/reference/keybindings/#web-app) gives the key for
each.

- **Conversations or messages.** The list groups mail into conversations by
  default. An action on a conversation covers every message in it, as in the
  TUI's thread mode. The **⋯** menu in the list header switches to single
  messages.
- **Open and move.** Move through the list, open a conversation beside it,
  step to the next or previous conversation without acting, or archive and
  open the next in one step. Closing the reader puts you back on the same row
  of the list you came from, including search results.
- **Act.** Archive, mark read and archive, trash, spam, star, mark read or
  unread, label, move, snooze, unsubscribe and reply later. Snooze and send
  later take a time in words ("fri 3", "in 2d") and show the exact time
  before you commit; see [time phrases](/reference/time-phrases/). Changes
  show at once. Undo reverses the last archive, trash, spam, read change,
  star, snooze, move or label change for about a minute, putting back
  exactly what was there before. Unstarring a conversation clears every
  star in it, including a starred reply in Sent.
- **Select.** Select rows one at a time, a range, everything, or all read,
  unread or starred mail. Trashing or marking several conversations as spam,
  or changing more than 20 messages, asks first and lists what will change.
  Very large batches run in the background with a progress toast.
- **Filter.** Filter the loaded list by sender, subject or snippet;
  `⌘Enter` in the filter searches all mail instead.

### The reader

A conversation opens with its context before the messages:

- **The gist and the ask.** When a language model is configured, one
  sentence says what is currently true in the conversation, and a second line
  says what they are asking of you, or that nothing is. **Show in message**
  scrolls to the sentence that makes the ask, which is marked in the message.
  mxr checks the quote really is in the message before it marks anything. A
  small line above the gist says which model wrote it, local or cloud, and
  what it read: "Local model qwen2.5 · from this thread". Your history with
  the person only goes to a cloud model when you allow it with
  `llm.allow_cloud_relationship_data`. The gist has a fixed place, so the
  messages below never move when it arrives. It shows only for long
  conversations (four or more messages, or over 400 words): for a short one
  the list row already said it, and the ask is still marked in the message.
- **When it arrived.** Every message, open or folded, shows its arrival in
  your local time and your locale's format: "Today, 14:02 · 2 hours ago" for
  today's mail, "Tue 21 Oct, 14:02" for earlier mail this year, and with the
  year after that. Hover the time for the full date. The "2 hours ago" moves
  on while the email stays open. On a phone the arrival has its own line.
- **How you know them.** "You and Maya: 41 emails · you usually reply within
  4h · last spoke 12 Sep", or "your first conversation".
- **Whether you owe a reply**, and since when. Newsletters never count.
- **Open promises** in this conversation, yours and theirs, with due dates.
  Each promise names its own owner, so in a group thread Alice's promise
  never reads as Bob's. The check mark marks one done. A date a model wrote
  shows as "(date unverified)" and is not acted on; see
  [dates mxr does not act on](/guides/forgotten-work/#dates-mxr-shows-but-does-not-act-on).

With no model configured you get the facts only, and nothing asks you to set
one up. The same context is in the TUI and in `mxr briefing context`
([briefings](/guides/briefings-and-loop-in/#thread-context-mxr-briefing-context)).

Older messages you have read fold to one line, so a long conversation opens
on the newest or first unread message. You can move between messages, open
or fold one, or open them all. Quoted history and signatures fold behind a
control that says how much is hidden.

The view switch, and a key for each view, choose between the formatted
HTML, a cleaned reading view, and plain text; as in the TUI, pressing the
HTML or reader key again goes back to plain text.

In the formatted view, **Appearance** lets you choose **Auto for this message**,
**Original for this message**, or **Dark for this message**. Auto follows the
app’s current light or dark scheme: in light scheme it shows the original,
while in dark scheme it uses a warm paper canvas for designed mail and a dark
canvas for simple mail. The app’s **Match the system** theme follows your OS
appearance.
To keep an Original or Dark choice for a sender, use **Always show Original**
or **Always use Dark**; **Reset sender preference** removes that saved choice.

Tracking pixels are always removed, and remote images stay blocked until you
choose **Show images** (or press its key), or choose **Always for** a sender, so a
sender cannot tell when you read their mail. A quiet line under the message
header says what was blocked and who serves it: "Blocked 2 trackers and 5
remote images from Mailchimp." Images embedded in the message itself always
show.

From **More** or the palette you can list every link in the message to open
or copy, show the raw headers, open the original in a new tab, save the
conversation as Markdown, and summarize it.

The reply field sits where reading ends, after the last message. Click it, or
use the reply, reply-all or forward key, to open the reply in place. The
toolbar above the conversation keeps only close, previous and next, archive,
snooze and the view switch. Everything else is in **More**, with its key, and
in the command palette.

**Draft in your voice** in the reply field (shown when a model is configured)
opens the reply with the **Draft for me** line already open. **Draft a reply**
in the reader's **More** menu opens the same reply. Say what it should say,
or leave the line empty to answer what they asked; anything only you can decide
comes back as a `[[?: ...]]` gap. [How drafts are written](/guides/llm-features/).

### Triage queues

Reply later (the reply queue; reply later adds to it) and the other queues the TUI shows as
lenses live in the rail's Views and Mail groups: owed replies (the desk's
You owe lane in full: people you have written to waiting on you, most
overdue first; the others who wrote to you are in Messages and the page says how
many), invites and subscriptions. On the reply queue each row has a
**Done** button that takes it out of the queue. [Focus & reply](/guides/focus-and-reply/)
which works through owed replies and the reply queue one conversation at a
time.

**Snoozed** lists snoozed mail soonest to wake first, one row per
conversation. **Wake now** wakes every snoozed message in
that conversation. On both lists, the row-action key runs the row's button
for the row under the cursor.

## When mxr is unavailable

If mxr becomes unreachable while the web app is open, one banner under the top
bar says so within a second or two: "mxr isn't running. Showing what was loaded
at 21:04." Mail already loaded stays on screen and the rail keeps every item;
new pages and changes need mxr to answer. `j`, `k`, `Enter`, `Esc` and the `g` jumps keep
working on loaded views. Views you haven't opened yet wait to load.

Changes don't queue. Archive, trash, labels, snooze, Done, undo, send and
opening the composer are refused until mxr can be reached again, and nothing
moves on screen. A reply you were writing keeps its text; the composer shows
that it isn't saved and saves it once mxr is reachable again.

The banner's **Copy start command** button copies `mxr restart`. Run it on the
computer running mxr; for a remote bridge, check the tunnel or network first.
The app checks again while disconnected. When mxr is reachable, the banner
clears, lists refresh and live updates reconnect without a page reload.

## When a page is empty, loading or failed

Every page shows one of three states instead of its own variety. An empty page
sits at the top of the content, says what isn't there and, where there is
somewhere better to go, names it ("Next: 3 in To do"). A loading page draws rows
in the shape of its own list, but only after 150 ms, so a page that answers
from cache never flashes one. A failed page says what happened in plain words,
what still works, and offers the one action that can help: **Try again** for a
failure a retry can fix, **Copy start command** when mxr isn't running, **Open
token settings** when this browser isn't recognised, and **Go to Inbox** for a
conversation or item that can't be found. The raw error is under **Details**.
When mxr isn't running, the banner under the top bar carries **Copy start
command**, and the page doesn't repeat it.

An empty Messages with nobody listed uses the whole page, and Archive's empty
page links to the Inbox and says how to file an email by hand.

## Finishing a page, and what changes in front of you

A done row folds away and the light is already on the next one. Press `e` once
and you see the fold; press it repeatedly and the rows drop at once. When you
put Now, Messages, To do, Updates or Reading to bed, a small shore settles where the list
was, once per mode per day, followed by where the work is next ("Next: 3 in
To do"). A change the assistant makes, or mail that arrives while the page is
open, shows on its rows as a thin underline that fades in under a second.

A toast says what happened in a line, with **Undo** when the change can be
undone. There is one at a time, in the bottom right corner (top right in
Focus), clear of the row you are on. It goes after 5 seconds, or 8 with
Undo. With **Settings > Appearance > Motion** on **Reduced** (or your system
set to reduce motion) rows leave at once, panels only fade and the shore is
skipped.

If live updates disconnect while other requests still work, the app refreshes
loaded mail when the event connection returns. Your open draft keeps its text;
you do not need to reload the page.

## Command palette

`⌘K` (`Ctrl+K` off macOS, or `:`) runs any action by name. With a
conversation open or rows selected, the mail actions for it come first.
**Go to label** opens the palette as a jumper to any mailbox, label or saved
search. You can type straight after opening the palette or quick search:
keys pressed before it has focus land in its field.

## Talk to your mail

`@`, `⌘.` (`Ctrl+.`), or the assistant button beside Search, opens the
[assistant](/guides/assistant/) on the right. It stays as you move between
pages, and opens as a drawer over the page below 1024px wide. Ask it questions or tell
it what to do; replies appear as they are written, and every change waits
on a card until you approve it.

## Compose

Replies and forwards open under the message they answer, with one audience
line such as “To Pat · Reply all”. Select that line to edit To, Cc and Bcc.
New messages open in a fresh sheet, leaving earlier drafts in Drafts. A reply
whose conversation closes stays open in that sheet, keeping the same draft
and typed text. You can move a reply to
the sheet or go fullscreen. A sheet is modal:
Tab stays inside it, `Esc` saves the draft and leaves (inside the Markdown
editor `Esc` stays with vim, so use `:q`), and focus goes back to where you
were. On a phone it covers the whole screen. New messages start in the To
field and replies in the body. The editor is Markdown with vim keys by
default. **Writing options** changes Rich text or Markdown for this draft only;
**Settings → Compose** remains the default for new drafts. Snippets are in
Writing options; signatures fold under the message body. Send timing and reply
actions are in More send options.

**Draft for me** is one line above the page. Click it, or press ⌘J, and say
what the email should do or what to change. The placeholder names what will
happen: an empty draft asks for a reply, a draft with text is finished or
changed, and in the Markdown editor a selection is the only part that
changes. The rich-text editor has no selection editing yet: it always sends
the whole draft and never offers "Edit the selection". **Shorter**,
**Warmer**, **More formal** and **Proofread** steer the answer. **Polish**
is the one action that may change your words. A new email needs to know what
it's for (the subject counts as context, not as the instruction).

The answer comes back as a proof that replaces the page. Your words stay in
plain ink, additions are underlined, cuts are struck through and gaps are
highlighted. Nothing is written over your text until you accept:

- **Enter** accepts. **⌘Z** takes the accept back in one step.
- **Esc** keeps your text as it was. On the help line, Esc closes the line.
  Neither closes the sheet.
- **⌘J** revises the proof with a note. **← →** step through earlier answers
  for the same draft, and **Tab** moves from gap to gap.

In the Markdown editor, words the model wrote keep a dotted underline until
you edit them.

Under the proof, a line says which model wrote the draft (local or
cloud), how many of your emails shaped the voice and whether your history
was used. **Sources** lists those emails and the messages it read from the
conversation; each opens in a new tab on that message
([where a draft came from](/guides/llm-features/#see-where-a-draft-came-from)).

When you reopen a saved reply or forward from Drafts, Draft for me keeps that
stored intent. A new message uses its current recipients and subject.

### Recipients

To, Cc and Bcc hold each address as a chip. Type, then press `Enter`, `,` or
`;` to make a chip, or paste a list separated by commas, semicolons or new
lines. `Backspace` on an empty field removes the last chip. An address that does
not parse stays in the field as an invalid chip until you fix or remove it.

As you type, mxr suggests from the people who have written to you. A
suggestion matches part of the address or the display name, and each address
appears once, with its name, even when it writes to more than one of your
accounts. Addresses already in the field are left out. `↓` and `↑` move through
the list, `Enter` takes the highlighted suggestion, and `Esc` closes it. If a
suggestion does not appear, type the whole address.

Suggestions come from your most frequent inbound senders, so someone you only
write to, or who has written to you rarely, may not be suggested.

Drafts save as you type and when you close the composer. Sending asks for
confirmation with the recipients, the From address and the pre-send safety
check, and then waits 10 seconds so you can undo
([undo a send](/guides/focus-and-reply/#undo-a-send)). When the message
promises something with a date, mxr offers to remind you then
([keep the promises you make](/guides/focus-and-reply/#keep-the-promises-you-make)). **More send options** has send
later, send and archive, and send with a reminder if nobody replies.
Scheduled messages appear at the top of **Drafts** with a **Cancel send**
button.

The sidebar's **Drafts** entry is backed by mxr's local draft store. For
accounts with provider drafts (currently Gmail), **Save to server draft**
keeps one linked provider draft in step with the local one.

## Search

`/` opens quick search with live results. `Enter` opens the full results,
which behave like a mailbox, with the same keys and actions. Operators such as
`from:` and `has:attachment` show as chips you can remove, and the Sender,
List and Category facets under the query narrow it in one click. **Exact** is the default mode;
**Hybrid** and **Meaning** add semantic matches when semantic search is on.

**Save** keeps the query as a saved search in the sidebar, and the first
nine each have a go-to key. The bookmark button renames,
re-queries, pins, recolours or deletes them.

## Wrapped: story mode + copy

**Analytics → Wrapped** (`/analytics/wrapped`) sums up the chosen date range.
It has two modes:

- **Standard**: sections for volume, when you get mail, people who wrote most,
  replies, storage and newsletters, and superlatives. Senders and threads in
  them open the matching search or conversation.
- **Story mode**: one fact at a time (messages, busiest day, who wrote to you
  most, median reply time, storage, longest thread). The arrow keys or the
  previous and next buttons move between them. **Show everything** returns to
  the standard view.

**Copy summary** copies a plain-text version of those facts to the clipboard.

## Accounts and screener

The account switcher at the top of the sidebar shows mail from one account or
all of them. Unlike the TUI it does not change your default account; it only
filters what the web shows.

**Accounts** lists each account with **Make default** and **Disable**; open
one to manage it. The account page has **Test connection**, **Sign in again**
(for OAuth accounts), **Make default**, send-as addresses, and **Disable** or
**Remove account** (optionally deleting its local mail). **Re-save
password** writes the account's IMAP and SMTP passwords back to mxr's local
secrets file; accounts that sign in with OAuth report that there is nothing
to re-save.

The **Screener** page has a **Queue** tab of first-time senders and a
**Decisions** tab where you can **Clear** an earlier call. Each sender row has
**Allow**, **Deny**, **Feed** and **Paper trail**, shown on the lit row with
`1` to `4` (the TUI's `a`, `d`, `f`, `p` work too) to decide for that sender
([screener keys](/reference/keybindings/#screener)). With more than one account, a
picker above the list chooses which account's queue you are screening. The
queue lists the senders the Screener count counts: first seen in the last 14
days, mail in your inbox. **Show all senders** adds everyone without a
decision, however old.

## Sender standalone route

`/sender/<email>` is a deep-linkable sender page. It uses the first account
from `/platform/accounts`, shows the relationship profile
(`/mail/relationship`) and the sender profile (`/mail/sender`), and has a
**Recipient briefing** button. The right-rail sender panel still opens inside
a thread.

## Deliveries page

The **Deliveries** entry in the sidebar's Views group opens `/deliveries`,
the same tracked-packages list the [CLI and TUI](/guides/deliveries/) show.
Each row has the merchant or carrier, status, expected or delivered date,
items, order and tracking numbers, and:

- **On the way / Delivered / All** filter tabs.
- A **Mark delivered** button (for undelivered rows) and a **Dismiss** button
  for a false positive.
- **Open email**, which opens the source conversation in All Mail, plus a
  **Track** link when the carrier provides a tracking URL.

**Scan for deliveries** re-runs detection over recent mail and reports how
many deliveries it found or updated. The daemon also scans after each sync.

It reads `GET /mail/deliveries?filter=...`, posts to
`/mail/deliveries/{id}/resolve` and `/dismiss`, and scans with
`POST /mail/deliveries/scan`. See the [bridge reference](/reference/bridge/).

## Calendar invites page

The **Invites** entry in the sidebar opens `/invites`, the same
detected-invites list the [CLI and TUI](/guides/calendar-invites/) show,
across all accounts. Each row shows the event summary, when, location and
organizer, with:

- **Accept** / **Tentative** / **Decline** buttons for invites that still
  need a response, and a row menu with **Accept with comment**, **Maybe with
  comment** and **Decline with comment**, which open the composer on a reply
  that carries the RSVP.
- A short undo window before the RSVP is sent, as in the TUI and the
  in-thread invite card.
- Your response (Accepted, Tentative, Declined) once you have answered.
- Cancelled invites struck through and updated invites flagged.

It reads `GET /mail/invites?limit=...` and reuses the invite-reply action
(`POST /mail/actions/invite/reply`). See the
[bridge reference](/reference/bridge/).

## Comparing surfaces

| Surface | When you'd use it |
|---|---|
| **CLI** (`mxr ...`) | Scripts, automation, agents, one-off ops. |
| **TUI** (`mxr` no args) | Daily keyboard-driven mail triage in the terminal. |
| **Web app** (`mxr web`) | Multi-account mail in the browser, installable as a PWA; same daemon, vim-compatible compose editor, registry-backed keyboard model. |

The CLI is the canonical surface. If a feature exists only in the web app it
is incomplete by mxr's product rules; see the [why-mxr guide](/guides/why-mxr/).

## See also

- [HTTP bridge reference](/reference/bridge/): auth, endpoint table, OpenAPI spec.
- [Keybindings reference](/reference/keybindings/#web-app): every web key, generated from the app's action registry.
- [`mxr web` CLI reference](/reference/cli/web/): every flag and what it does.
- [Config reference](/reference/config/#bridge): `[bridge]` keys including `auto_local_token` and `port`.
- [Desktop app](/guides/desktop-app/): the macOS window around this app, with a global new-message shortcut.
- [Sound, key hints and touch](/guides/sound-hints-and-touch/): low tide, the optional sounds, key hints and swiping rows.
- [Desk and places reference](/reference/desk-and-places/): lanes, Done, placement rules and JSON output.
