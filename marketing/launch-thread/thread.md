# mxr launch thread

Ten posts. Post 02 uses the native video; every post also has a dark and light
card. Every post is at or below 280 characters.

---

## 01

**Post** (254 literal characters; 262 after X link weighting)

> I built mxr so you can talk to your coding agent about your email.
>
> It brings all your accounts into one local mailbox. Your agent can search years of mail locally, tell you what needs attention, and help you handle it.
>
> It's open source: https://mxr.sh/

- Image: `cards/dark/01.png`
- Alternate: `cards/light/01.png`

**Alt text**

A dark card headed "Talk to your coding agent about your email", with "email" in
blue. Below it: "I built mxr. It brings all your accounts into one local mailbox,
so your agent can search years of mail locally, tell you what needs attention,
and help you handle it." A footer line reads mxr, "Mixer", mxr.sh. On the right,
a framed screenshot of the mxr terminal mailbox lists about thirty threads from a
demo account with senders, subjects, and ages. The orange mxr diamond mark sits
top left, and a marker in the top right reads 01 of 10.

---

## 02

**Post** (210 characters)

> This is a normal mxr question for me: "Which emails should I be careful with?"
>
> Here it is on the synthetic demo mailbox. The agent searches locally, opens matching threads, and reports what deserves attention.

- Video: `videos/mxr-agent-demo.mp4`
- Fallback image: `cards/dark/02.png`
- Light fallback: `cards/light/02.png`

**Alt text**

A card titled "Which emails should I be careful with?" with the line "The agent
uses mxr to search my local mailbox, open the relevant threads, and report what
deserves attention." Beneath it, a wide screenshot of a coding agent's terminal,
labelled synthetic demo mailbox. The agent ran an mxr search across the inbox and
piped the JSON through jq, and the output lists five demo senders with message
counts, from 55 messages for a payroll notice down to 42 for cloud deals,
finishing with "Took 0.6s". The caption reads: mxr search across the local
mailbox, grouped by sender. Marker: 02 of 10.

---

## 03

**Post** (186 characters)

> mxr brings Gmail, Outlook, Microsoft 365, and any IMAP account into one local mailbox. Tantivy indexes it for fast local search. Sending works through Gmail, Outlook, or any SMTP server.

- Image: `cards/dark/03.png`
- Alternate: `cards/light/03.png`

**Alt text**

A card titled "All my accounts, one local mailbox", explaining that Gmail,
Outlook, Microsoft 365, and any IMAP account sync into SQLite on the author's
machine, that Tantivy indexes it for fast local search, and that he sends through
Gmail, Outlook, or any SMTP server. A diagram below shows green lines running
from three boxes on the left - Gmail, Outlook / Microsoft 365, and any IMAP -
into a box called provider adapters, then on to SQLite, labelled "one mail
model", which in turn feeds Tantivy, labelled "local search index". A separate
blue dashed line drops from provider adapters to a box reading Gmail, Outlook,
any SMTP. A key reads: solid green is inbound sync, dashed blue is outbound send.
Marker: 03 of 10.

---

## 04

**Post** (182 characters)

> Because the mailbox is already synced and indexed, retrieval is local. I don't notice the search. The only thing I wait for is the model that reads the results and answers or drafts.

- Image: `cards/dark/04.png`
- Alternate: `cards/light/04.png`

**Alt text**

A card titled "Search stays on the machine". The text says that because the
mailbox is already synced and indexed, a question starts with a fast local
lookup, and then, in white: "The only thing I wait for is the model." Below, a
row of boxes traces the path: agent, then mxr, then Tantivy and SQLite, then
results, all outlined in green, followed by model and "answer or draft" outlined
in blue. Under that, a single wide bar is split in two: a short green segment
labelled "on the machine" under the heading local retrieval, and a much longer
blue segment labelled "the part I wait for", marked variable, under the heading
model inference. Marker: 04 of 10.

---

## 05

**Post** (234 characters)

> Once the mailbox is local, I can ask questions that are awkward one email at a time: Who am I slow to reply to? Which relationships have gone quiet? Which newsletters do I rarely open? What is taking up space? mxr answers from SQLite.

- Image: `cards/dark/05.png`
- Alternate: `cards/light/05.png`

**Alt text**

A card titled "I can analyse the whole mailbox at once". The text says every
synced message body, sender, thread, timestamp, and attachment metadata is
already in SQLite, and that mxr can answer questions across years of email
without fetching messages one by one. A panel labelled "mxr, year in review",
noted as local SQLite queries, shows the command mxr wrapped --ytd, and under it
five green tags naming what the summary covers: volume, top contacts, reply time,
storage, newsletters. Below the panel, under the line "or ask something
narrower", four question and command pairs are ruled off in two columns: "Who am
I slow to reply to?" with mxr response-time; "Which relationships have gone
quiet?" with mxr contacts decay; "Which newsletters do I rarely open?" with mxr
subscriptions --rank; and "What is taking up space?" with mxr storage --by
message. Marker: 05 of 10.

---

## 06

**Post** (208 characters)

> A shell-capable agent uses the same CLI I do. It discovers commands with `--help`, asks for JSON, and passes IDs from one command to the next. mxr also ships an MCP server for clients that prefer typed tools.

- Image: `cards/dark/06.png`
- Alternate: `cards/light/06.png`

**Alt text**

A card titled "The agent uses the same CLI I do". The text says a shell-capable
agent discovers commands with --help, asks for JSON, and passes IDs from one
command to the next, and that mxr also ships an MCP server for clients that
prefer typed tools. A terminal panel labelled "agent, shell" shows three
commands: mxr search "from:maya" --format ids, which
returns a single message ID; mxr thread followed by that same ID with --format
json; and mxr profile maya@example.com --format json. Marker: 06 of 10.

---

## 07

**Post** (236 characters)

> I often ask: "Find the latest thread with Maya, look at how I usually write to her, and draft a reply. Don't send it." mxr retrieves the thread and adds local context from our previous emails so the draft sounds like me writing to Maya.

- Image: `cards/dark/07.png`
- Alternate: `cards/light/07.png`

**Alt text**

A card titled "Drafting for one person", explaining that mxr retrieves the thread
and adds local context from previous emails, so the draft sounds like the author
writing to Maya. On the left, under the label "what I ask", a typed prompt reads:
"Find the latest thread with Maya, look at how I usually write to her, and draft
a reply. Don't send it." On the right, two rows outlined in blue sit under the
heading "weighs most": the current thread, and my instruction. Two fainter rows
sit under "background context": recent messages I sent to her, and contact style
profile. Marker: 07 of 10.

---

## 08

**Post** (221 characters)

> Drafting stops at text. Sending is a separate operation that requires a command, a daemon policy check, and confirmation. mxr treats email content as data, so a message cannot grant itself permission or redirect the task.

- Image: `cards/dark/08.png`
- Alternate: `cards/light/08.png`

**Alt text**

A card titled "A draft is still a draft". The text says drafting stops at text,
and that sending is a separate operation needing a command, a daemon policy
check, and confirmation. The diagram shows two separate rows. The top row,
labelled drafting, runs from mxr draft-assist to a box reading "draft text,
returned to me", and then a red line ends at three vertical red bars captioned
"drafting stops here". The bottom row, labelled sending, runs from "my send
command" through "daemon policy check" and "confirmation" to provider. Below
both, a red-outlined strip pairs a box reading "email content" with the line
"Data, never an instruction. It cannot authorise a send or redirect the task."
Marker: 08 of 10.

---

## 09

**Post** (190 characters)

> The TUI can close, the agent can come and go, and sync keeps running. They all talk to the same mxr daemon, so every client sees the same mailbox, search index, rules, and permission checks.

- Image: `cards/dark/09.png`
- Alternate: `cards/light/09.png`

**Alt text**

A card titled "Close the TUI. Sync keeps going.", explaining that the agent comes
and goes too, and that every client talks to one mxr daemon and sees the same
mailbox, search index, rules, and permission checks. The diagram shows six boxes
in a row across the top: TUI, CLI, web, scripts, agent skill, and MCP. Lines from
all six curve down and meet at a single blue dot, and one blue arrow continues
from that dot into a wide box labelled mxr daemon. Inside that box, five green
tags read: sync, SQLite, Tantivy, provider adapters, permission checks. Marker:
09 of 10.

---

## 10

**Post** (273 characters)

> Try mxr without connecting a real account:
>
> `brew install planetaryescape/mxr/mxr`
> `mxr demo`
>
> The demo is synthetic and isolated from your real mail.
>
> Explore: https://mxr.sh/
> Thread deck: https://nimble-ripple-t8dy.here.now/
> Source: https://github.com/planetaryescape/mxr

- Image: `cards/dark/10.png`
- Alternate: `cards/light/10.png`

**Alt text**

A card titled "Try mxr with a synthetic mailbox", noting that the demo creates an
isolated two-account mailbox with 50,000 messages and does not touch your real
mxr data. A terminal panel labelled "install and run" shows two commands: brew
install planetaryescape/mxr/mxr, and mxr demo. Below the panel, two buttons link
to mxr.sh and to github.com/planetaryescape/mxr. On the right, a screenshot
labelled "mxr demo, isolated" shows the mxr sidebar with two demo accounts,
alex@demo.mxr.local and alex@work.demo.mxr.local, along with folder counts and
labels, next to the start of the thread list. Marker: 10 of 10.
