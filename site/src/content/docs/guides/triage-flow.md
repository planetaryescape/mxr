---
title: Triage flow
description: Reply-later, screener, and custom snooze — the keyboard-first triage loop in mxr.
---

## The shape of triage in mxr

Email triage is a two-pass loop:

1. **First pass**: classify each message — does it deserve attention now,
   later, or never?
2. **Second pass**: actually deal with the "later" pile.

mxr ships three primitives that together make the loop fast.

## Reply-later (first pass)

The fastest decision: "I want to reply, just not now." Press `b` on a
message and it's flagged as reply-later. The flag is local-only — never
roundtrips to the provider.

```bash
mxr replies                        # see the queue
mxr replies add MESSAGE_ID         # add via CLI / agent
mxr replies remove MESSAGE_ID      # clear when done
```

Replying via any path automatically clears the flag (CLI `mxr reply`,
TUI `r`).

## Screener (first pass, by sender)

The decision you make once per *sender* rather than per message:

- `allow` — you want their mail in the inbox
- `deny` — auto-trash + mark-read
- `feed` — newsletters / non-urgent: skip inbox, route to a feed view
- `paper-trail` — receipts / records: archive on ingest

```bash
mxr screener queue                 # senders waiting for a decision
mxr screener allow alice@example.com
mxr screener deny spammer@example.com
mxr screener feed newsletter@example.com
mxr screener paper-trail receipts@example.com
```

The screener is **local-only by default**. Mobile / web Gmail won't see
your decisions unless you opt in per-sender:

```bash
mxr screener feed newsletter@example.com --label "Newsletters"
```

When `--label` is set, the daemon mirrors the disposition as a real
provider label so the categorisation rolls out to all your devices.

## Custom snooze (first pass, defer)

When "reply later" is too vague — you know exactly when this should
come back. The `--until` parser accepts conversational forms:

```bash
mxr snooze --until "in 2h" MESSAGE_ID
mxr snooze --until "tomorrow 9am" MESSAGE_ID
mxr snooze --until "monday 17:00" MESSAGE_ID
mxr snooze --until "friday 5pm" MESSAGE_ID
mxr snooze --until "2026-06-01T15:00:00Z" MESSAGE_ID
```

Or the configured presets (`tomorrow`, `weekend`, `tonight`, `monday`)
that resolve via your `[snooze]` config block.

## The sender view (second pass)

When you're working through the reply-later queue or following up with
a specific person, `mxr sender <addr>` is the unfair advantage:

```bash
mxr sender alice@example.com
```

You see their volume in/out, your replied-to-them count, p50 cadence,
when you last heard from them, and how many threads are open and waiting
on you. No other email tool reasons over senders this way because the
data is normally locked behind a provider API; mxr's local SQLite makes
it a single read.

## Putting it together

A typical mxr triage session:

1. Open inbox.
2. For each unknown sender, press the relevant disposition: `mxr screener allow|deny|feed|paper-trail`.
3. For each message you'll engage with: press `b` (reply-later) or
   `Z` (snooze with a specific time).
4. Run `mxr replies` later to walk the reply-later queue.
5. Run `mxr sender alice@example.com` before replying to a specific
   person to get instant context.

The whole loop stays in the keyboard. No mouse, no context switches.

## In real life

- **Monday morning:** open mxr, hit `Ctrl-p → Reply Queue` to see what
  you bookmarked over the weekend; walk it with `j/k` and `r`.
- **Inbox bombing after vacation:** `mxr screener queue --format ids
  | xargs -n1 mxr cat | less` — skim every unknown sender at once,
  decide dispositions in one pass.
- **Tax season:** `mxr screener feed billing@*.example.com --label "Receipts"`
  and the disposition mirrors to Gmail so your accountant's mobile app
  sees the categorisation too.
- **Newsletter overload:** `mxr subscriptions --rank --format json |
  jq '.[:10]'` shows the worst ROI lists; pipe to `xargs -n1 mxr
  unsubscribe`.

## Agent prompts that work

```text
"Show me the senders waiting in my screener queue. For each, summarise
the latest message in one sentence. I'll tell you the disposition; you
run `mxr screener allow|deny|feed|paper-trail`."
```

```text
"It's Friday at 3pm. Snooze every newsletter in the inbox until Monday
morning. Use `mxr search 'label:newsletters is:unread' --format ids |
mxr snooze --until 'monday 9am' --yes`. Show the dry-run first."
```

```text
"Walk my reply-later queue. For each one, show me the thread context,
suggest a 2-line reply, and wait for me to approve. Use `mxr replies
--format ids` to start."
```

## See also

- [Recipes — fzf / jq / xargs](/guides/recipes/)
- [Automated follow-ups](/guides/automated-followups/)
- [CLI reference — Snooze](/reference/cli/#snooze)
