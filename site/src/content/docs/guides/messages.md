---
title: Messages
description: People you talk with, one row each, with your conversations inside as topics. Reply, say got it, or mark done here.
---

Messages shows people, not emails. Each row is a person you're in
conversation with, merged across their addresses, and your conversations
with them sit inside the row as topics. A group conversation, where two or
more other people took part, is its own row. The quoted line under a name
is what they asked you.

```bash
mxr messages            # the bands
mxr messages list --turn mine
mxr messages list --unread --past-pace
mxr messages person samir@launchpad.example
mxr messages ack THREAD_ID --dry-run
```

In the web app and the TUI, `g m` opens Messages.

The web app starts with the conversation list and no conversation open.
Choose a person or group to read it. A link to a specific conversation opens
that conversation directly. Opening Messages from navigation returns to the list.

Possible personal mail with no deciding rule stays in Messages, marked **Not
sure**, with the reason “No rule decided; Messages for now”. Inspect the stored
answer with `mxr why MESSAGE_ID --format json`, or measure fallback clusters
with `mxr classify report --format json`. A stored Messages home does not undo
a snooze or a done mark. The stored-assessment details require changes made after
`v0.6.158`; that release does not include them.

## Four bands, closest people first

| Band | Who is in it | Order |
|---|---|---|
| Your turn | A directed request to you is unanswered, or your reply-later timer has returned | Closest first, then whoever has waited longest past your usual pace with them |
| Pinned | Up to nine people you pinned with `s` | Most recent first, with a dot when it's your turn |
| Recent | Everyone else you've talked with in the last 30 days | Most recent first; "You: ..." when you wrote last |
| Quiet | Done here, or a turn nobody took | Collapsed; a new message brings them back |

Closeness comes from your own history with them, counted on your machine:
how often you've written to each other, how recently, and for how long.
Your turn uses the same rule as Now's People, so the two always agree: Now's
headline, the Messages count in the rail and the Your turn rows are the same
number, which `your_turn_split` in `mxr messages --format json` divides into
people you write with and others who wrote to you. The Waiting on view
(`turn=theirs`) is empty with "Nobody you're waiting to hear from."

A turn nobody took goes quiet after three times your usual interval with
that person, or a week when there's no history, and never stays longer
than 30 days. Nothing in Messages expires; it only goes quiet.

## Filter the list

The list can show only some rows. A turn choice (All, Your turn, Waiting on
them) sits beside two toggles: Unread keeps people with a message from them
you haven't read, and Past your pace keeps the people whose turn has run
longer than your usual reply time with them. The toggles and the turn
choice combine, so Your turn with Unread shows only the unread people you
owe a reply.

The filter changes which rows show, never the counts. The header, the rail
and Now still count everyone, and the filter line says how many rows it
kept ("Showing 2 of 9") with Clear filters beside it while a filter is on.
An empty filtered list says why: "No unread messages from people." and what
is still waiting in Your turn.

| Where | How |
|---|---|
| Web app | The filter line above the list. `g f` moves focus to it |
| TUI | `Ctrl-f` opens a picker; `Enter` picks or toggles, `Esc` closes. The status bar names the filters on |
| Raycast | `⌘F` opens a Filter menu. The choice is remembered for that command |
| CLI | `mxr messages list --turn mine --unread --past-pace`; `--format json` echoes the filter and reports `matched` |

In the CLI, `--turn mine` and `--turn theirs` pick Your turn and Waiting on
them. The table says what it kept:

```text
$ mxr messages list --unread --past-pace
People you talk with, one row each. Reply or mark done.
Filtered: Unread · Past your pace · 1 of 782 rows

YOUR TURN
  DP   Samir Patel                 16h  close · usually 47m
```

## Which threads land here

Sorting follows this email's evidence, not a permanent sender identity.
List headers and no-reply or automatically generated messages keep their
Reading or Updates destination even after you have written to the sender.
Your per-email move, sender choice or custom sorting rule still wins.

Cached document fields can also place an email in Updates without list
headers: a document reference and date, an order reference or amount, plus
purchase or document-delivery framing. An invoice subject alone is not enough.
Team FYIs and report deliveries without a directed request go to Updates;
a supported request naming you can stay in Messages. Human billing and support
answers can settle Waiting on without creating another reply obligation.
An owned directed request can remain conversational alongside document fields
and purchase framing; an actual list, no-reply or auto-generated declaration
still takes precedence. Reply headers or positive local thread correspondence
can establish that a human answered, even when its body is uncached or HTML-only.
Missing plain text cannot create automatic
reply debt, but it does not erase your explicit Reply later choice.
Bare greetings such as "Hi," and "Good morning," do not name another
recipient, including inline "Hi, could you confirm ...". A greeting naming
someone mxr cannot match to an owned recipient remains uncertain: it does
not silently register that name as yours.

Possible personal mail stays accessible in Recent or Quiet without claiming
someone is waiting for you. Automatic Your turn requires a directed request:
currently English requests such as "Could you confirm ..." in cached plain
text, with you in To. A group
request also needs your recipient name or address local part. FYIs,
acknowledgements, quoted requests and unknown aliases do not create a turn.
Missing text, other languages, indirect questions and HTML-only requests remain
uncertain. Set Reply later when you want an explicit reminder; no model or
network request is needed for this sorting.

Request detection shares the reader's Outlook/header, quote, signature and
footer rules, with one bounded earlier cached message for unmarked history.
An explicit reference selects that context first; otherwise message date and
stable ID select an earlier message, regardless of import order.
Unrecognized quote formats and history beyond that context remain limitations.
Use `mxr why MESSAGE_ID` and `mxr modes why MESSAGE_ID` to inspect the evidence
and destination when a result looks wrong.

mxr judges each thread's shape from who took part:

- One other person wrote in it or you wrote to them: a topic in their row.
- Two or more other people took part: its own row, keyed by the thread,
  so it stays one row however the CC list changes.
- You were only copied and never wrote in it, or it went to more than 10
  people: it goes to Updates, not Messages, and Updates lists it with the
  reason "copied". A crowd thread stays in Messages while someone has
  replied to you in it, with you in To, and you haven't answered yet. A
  positively directed request can also keep a crowd thread in Messages.

The 10-recipient line is `messages.large_thread_recipients` in
[config](/reference/config/). Automated mail and lists never land here.

## What a conversation shows

Opening a person shows how you know them ("You've written 48 times since
2023. Last: Tuesday.") and opens the selected topic as a conversation. The
compact conversation control names the selected subject and shows whether it
is direct or a group; open it to switch topics. Group topics name their other
participants, and the `{` and `}` keys step through topics.

In the web app, choose a person's name to inspect and copy their linked email
addresses. Choose a message's sender name to see the original From address;
it can differ from the addresses linked to that person. Choose `Message info`
on a message to inspect its recorded From, To, Cc and available Bcc addresses.
An omitted or empty To or Cc list says that none were recorded. An omitted or
empty Bcc list says it was not recorded in this copy, which cannot confirm that
nobody was hidden. If the message details cannot be loaded, the app reports that
or offers a retry. Long address lists scroll within the dialog. Escape closes
the address panel or message info and returns focus to its trigger.
Choose `People` below the topic selector to inspect and copy addresses
that appear as senders or recorded To, Cc or available Bcc recipients in this
thread. It expands across the reading area and scrolls internally for a long
list. This is historical context: message audiences can differ. `Message info`
remains the source for the recorded roles on an individual message. People
stays closed until you open it and reports an error with a retry if the thread
cannot be loaded.

Read the conversation directly. mxr removes repeated trailing history,
signatures and footers when it can identify them. Quoted questions beside
inline answers stay together, and forwarded messages keep their attribution
and contents. A message with retained context stays expanded.

When something was removed, the message says `trimmed: quote, sig` (or
`footer`). Select that marker, or press `o`, to see the message as
sent. A reply containing only repeated quoted text says `(only quoted text)`.
The CLI, TUI, web app and MCP use the same extracted text.

Short messages read like chat. The latest message opens in full at its start.
Older long letters lead with the paragraph containing the request when one is identified;
otherwise they show the opening paragraph, including the next paragraph if
the first is only a greeting. Choose `Read all N paragraphs` to expand an
older letter, then `Show less` to collapse it. Retained inline or forwarded
context is shown in full.

If mxr has only a provider preview, it labels that text as incomplete.
Choose `Check the original` to try loading the full message. If it is still
unavailable, mxr says so. HTML shown as sent follows your global or saved
sender [mail appearance choice](/guides/web-app/#the-reader).

Choose `Reply` or `Reply all` above the footer to set the next reply mode. When
writing opens, the compact header shows the actual To recipients, populated Cc
and Bcc fields, From address and sending account from the draft. An empty To
field offers an add-recipient action. Choose `Edit` to open the existing fields
and `Done` to fold them again; edits stay in the draft and appear in the compact
header immediately. The reply uses the selected message's prepared recipients,
including its Reply-To when present. If you switch topics, the reply docks with
its source subject in the title and returns to that topic.

Topic labels show the state and recency for your turn, waiting and done topics.
Quiet topics show only recency; the separate Quiet band for people remains.

## Got it

`.` replies with a short acknowledgement in your own words for that
person, such as "Hi Samir,\n\nGot it, thanks.\n\nCheers,\nAlex". The
greeting and sign-off are the ones you actually use with them, read from
your sent mail; no model writes it. It shows the exact text, To address and
From address from its preview plan, then counts down for five seconds before
sending. Press `u` to cancel it.

```bash
mxr messages ack THREAD_ID --dry-run   # the exact text, nothing sent
mxr messages ack THREAD_ID             # shows it, counts down, then sends
```

Got it is a real reply, so it reads fine in every mail client, unlike an
emoji reaction. Because it sends mail on one key, mxr guards it:

- It goes only to one person, never to all. It refuses mailing lists,
  automated and no-reply addresses, and a Reply-To on another domain you
  have never written to; reply by hand in those cases.
- What goes out is exactly what you saw. If someone writes again, or
  anything about the reply changes during the countdown, nothing is sent
  and you are asked to look again. A preview is good for one minute.
- It never sends twice for the same message, even when a send fails half
  way: if the provider might have taken it, mxr says so and won't try
  again, so check Sent. Once you've written after their message, there is
  nothing left to acknowledge.
- It works only on a conversation in Messages. A thread you were only
  copied on, or one sent to a crowd, is in Updates, and Got it refuses it.

## Keys

| Key | Does |
|---|---|
| `Enter` | Open the person |
| `r` / `a` | Reply / reply all |
| `.` | Got it |
| `e` | Done here, until they write again |
| `t` | Make a to-do from this topic |
| `b` | Reply later |
| `s` | Pin or unpin the person |
| `c` | New topic with this person |
| `{` / `}` | Previous / next topic |
| `p` | The person's page |
| `o` | The message as sent |
| `g f` (web) or `Ctrl-f` (TUI) | Filter the list |
| `u` | Undo |
| `?` | What this mode is for, and its keys |

## One person, several addresses

People write from work and personal addresses. mxr never merges them on
its own, because a wrong merge shows one person's mail under another.
When two addresses you've written to share a name, it suggests the merge
on the person's page and in `mxr messages merge --suggestions`. You merge
by hand, with a preview first:

```bash
mxr messages merge samir@launchpad.example samir.p@home.example --dry-run
mxr messages merge samir@launchpad.example samir.p@home.example
mxr messages split samir.p@home.example
```

## For scripts and agents

`mxr messages --format json` returns every band with each row's topics,
preview, closeness and why line. `mxr messages person ID --format json`
returns the page with each message's text, `trimmed` flags, `source`
(`stored_body` or `provider_snippet`; omitted by older daemons), and
`has_preserved_context`.
The bridge serves the same data under `/api/v1/mail/people`, and the MCP
server has `mxr_messages`, `mxr_person` and `mxr_got_it` (which previews
unless confirmed with the previewed text).
