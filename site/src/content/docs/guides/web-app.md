---
title: Web app
description: How the mxr web SPA is served, how it auto-authenticates on the local machine, and how to point it at a remote bridge.
---

The mxr web app is a React SPA that talks to the daemon through its
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

## The desk

The web app opens on the [desk](/guides/desk/): replies you owe, promises
coming due, threads waiting on someone and new mail from people, each row
with the reason it is there. Mail that is not from people goes to
[Reading and Paper trail](/guides/reading-and-paper-trail/). To answer
everyone you owe in one sitting, use [Focus & reply](/guides/focus-and-reply/).
[Work through your day](/guides/your-day/) puts the three in order.

To open on the inbox instead, set **Settings > Appearance > Home** to
**Inbox**. The arrival-order inbox shows everything, Reading and Paper trail
mail included; the separation lives on the desk and in the places.

The sidebar holds places rather than folders: Desk, Inbox, Reply queue,
Waiting on, Snoozed, Reading, Paper trail and, when someone new is waiting
for a decision, Screener. Folders (Starred, Sent, Drafts, All Mail, Spam,
Trash) and the rarer lists (Owed replies, Invites, Subscriptions and
Deliveries) sit under **More**. **Labels**, **Saved searches** and **Tools**
have their own sections; More, Labels and Tools start folded. Only work
carries a count: the desk (owed and due), the reply queue and the screener.
Unread mail still shows as bold rows, but the inbox, labels and saved searches
carry no unread badge.

## Reading and triage

The web app works like the TUI: a list of conversations, a reader beside it,
and the same keys. Press `?` anywhere for the keys that apply to the view
you are in. This section names actions; the
[keybindings reference](/reference/keybindings/#web-app) gives the key for
each.

- **Conversations or messages.** The list groups mail into conversations by
  default. An action on a conversation covers every message in it, as in the
  TUI's thread mode. The button in the list header switches to single
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
  snooze, move or label change for about a minute. Star has no undo; star
  again to remove it.
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
- **How you know them.** "You and Maya: 41 emails · you usually reply within
  4h · last spoke 12 Sep", or "your first conversation".
- **Whether you owe a reply**, and since when. Newsletters never count.
- **Open promises** in this conversation, yours and theirs, with due dates.
  Each promise names its own owner, so in a group thread Alice's promise
  never reads as Bob's. The check mark marks one done.

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
opens the reply with **Draft for me** ready. **Draft a reply with AI** (in the
reader's **More** menu) writes a reply in
your voice, from real emails you sent this person. Say what it should say,
or leave it empty to answer what they asked; anything only you can decide
comes back as a `[[?: ...]]` gap. **Reply with this** opens the reply with the
draft above the quoted message. [How drafts are written](/guides/llm-features/).

### Triage queues

The reply queue (reply later adds to it) is one of the sidebar's places. The other
queues the TUI shows as lenses live under **More**: owed replies (every
conversation where someone is waiting on you, archived or not, most overdue
first), invites and subscriptions. On the reply queue each row has a
**Done** button that takes it out of the queue. [Focus & reply](/guides/focus-and-reply/)
which works through owed replies and the reply queue one conversation at a
time.

**Snoozed** lists snoozed mail soonest to wake first, one row per
conversation. **Wake now** wakes every snoozed message in
that conversation. On both lists, the row-action key runs the row's button
for the row under the cursor.

## When the daemon stops

If the daemon stops or restarts while the web app is open, a banner says
so within a second or two. Everything already loaded stays on screen: lists,
the desk and conversations you opened. `j`, `k`, `Enter`, `Esc` and the
`g` jumps keep working on them. Views you hadn't opened yet wait to load.

Changes don't queue. Archive, trash, labels, snooze, Done, undo, send and
opening the composer are refused with a toast such as "Can't archive while
mxr's daemon is stopped", and nothing moves on screen. A reply you were
writing keeps its text; the composer shows that it isn't saved and saves
it once the daemon is back.

While it's down the app checks again every few seconds (backing off to
about every 8 seconds). When the daemon answers, the banner clears, the
lists refresh and live updates reconnect, with no page reload. Start the
daemon with `mxr daemon` or `mxr restart`.

## Command palette

`⌘K` (`Ctrl+K` off macOS, or `:`) runs any action by name. With a
conversation open or rows selected, the mail actions for it come first.
**Go to label** opens the palette as a jumper to any mailbox, label or saved
search. You can type straight after opening the palette or quick search:
keys pressed before it has focus land in its field.

## Compose

Compose opens in place, for a new message, a reply, reply all or a forward.
A reply opens under the conversation; you can pop it
out or go fullscreen. New messages start in the To field and replies in the
body. The editor is Markdown with vim keys by default; **Settings →
Compose** switches to rich text.

**Draft for me** in compose writes the body in your voice. A reply can leave
the instruction empty; a new email needs to know what it's for (the subject
counts as context, not as the instruction). A recipient outside the
conversation drafts a forwarding note. Refine works on what's in the editor.

Under **Draft for me**, a line says which model wrote the draft (local or
cloud), how many of your emails shaped the voice and whether your history
was used. **Sources** lists those emails and the messages it read from the
conversation; each opens in a new tab on that message
([where a draft came from](/guides/llm-features/#see-where-a-draft-came-from)).

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
**Allow**, **Deny**, **Feed** and **Paper trail**, each with a key that
decides for the highlighted sender
([screener keys](/reference/keybindings/#screener)). With more than one account, a
picker in the header chooses which account's queue you are screening.

## Sender standalone route

`/sender/<email>` is a deep-linkable sender page. It uses the first account
from `/platform/accounts`, shows the relationship profile
(`/mail/relationship`) and the sender profile (`/mail/sender`), and has a
**Recipient briefing** button. The right-rail sender panel still opens inside
a thread.

## Deliveries page

The **Deliveries** entry in the sidebar's Tools section opens `/deliveries`,
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
- [No native desktop app](/guides/no-native-desktop-app/): why the web app is installable without an Electron shell.
- [Sound, key hints and touch](/guides/sound-hints-and-touch/): low tide, the optional sounds, key hints and swiping rows.
- [Desk and places reference](/reference/desk-and-places/): lanes, Done, placement rules and JSON output.
