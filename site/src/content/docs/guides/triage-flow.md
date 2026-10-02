---
title: Triage flow
description: Reply-later, screener, and custom snooze, the keyboard-first triage loop in mxr.
---

## The shape of triage in mxr

Email triage is a two-pass loop:

1. **First pass**: classify each message: does it deserve attention now,
   later, or never?
2. **Second pass**: actually deal with the "later" pile.

mxr ships three primitives that together make the loop fast.

## Reply-later (first pass)

The fastest decision: "I want to reply, just not now." Press `b` on a
message and it's flagged as reply-later. The flag is local-only, never
roundtrips to the provider.

```bash
mxr replies                        # see the queue
mxr replies add MESSAGE_ID         # add via CLI / agent
mxr replies remove MESSAGE_ID      # clear when done
```

Replying via any path automatically clears the flag (CLI `mxr reply`,
TUI `r`).

## Screener (first pass, by sender)

The decision you make once per *sender* rather than per message. The
queue is for strangers: anyone you have written to is never in it.

- `allow`: you want their mail in the inbox, as mail from a person. Until
  you write to them, it shows on the desk under New from people, not You
  owe.
- `deny`: auto-trash + mark-read
- `feed`: newsletters and lists. Their mail lives in [Reading](/guides/reading-and-paper-trail/), off the desk.
- `paper-trail`: receipts and notifications. Their mail lives in [Paper trail](/guides/reading-and-paper-trail/), off the desk.

```bash
mxr screener queue                 # senders waiting for a decision
mxr screener allow alice@example.com
mxr screener deny spammer@example.com
mxr screener feed newsletter@example.com
mxr screener paper-trail receipts@example.com
```

Screener decisions are **local**: they live in mxr's store, not in your
provider. To also label a sender's mail:

```bash
mxr screener feed newsletter@example.com --label "Newsletters"
```

With `--label`, mxr adds that label to the sender's new mail as it syncs
in. Decisions match the exact sender address; wildcards are not supported.

## Custom snooze (first pass, defer)

When "reply later" is too vague: you know exactly when this should
come back. `--until` takes a time in words, resolved in your local time:

```bash
mxr time "fri 3"                         # preview first
mxr snooze --until "fri 3" MESSAGE_ID
mxr snooze --until "in 2h" MESSAGE_ID
mxr snooze --until "tomorrow 9am" MESSAGE_ID
mxr snooze --until "next week" MESSAGE_ID
mxr snooze --until "2026-06-01T15:00:00Z" MESSAGE_ID
```

Phrases without a time (`tomorrow`, `weekend`, `tonight`, `monday`) use
the hours in your `[snooze]` config block. See
[Time phrases](/reference/time-phrases/) for everything mxr accepts.

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
  | xargs -n1 mxr cat | less` to skim every unknown sender at once,
  decide dispositions in one pass.
- **Tax season:** `mxr screener paper-trail billing@shop.example --label "Receipts"`
  files that sender's new receipts under one label as they arrive.
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

- [Unsubscribe](/guides/unsubscribe/)
- [Recipes: fzf / jq / xargs](/guides/recipes/)
- [Automated follow-ups](/guides/automated-followups/)
- [CLI: `mxr snooze`](/reference/cli/snooze/)
