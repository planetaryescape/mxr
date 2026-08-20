---
title: Quick start
description: Try mxr safely, then connect your own inbox.
---

After [installing](/getting-started/install/), you have one binary: `mxr`. The TUI is what you'll use most; the CLI handles scripting and one-off operations. Both talk to the same daemon.

## 1. Try the demo inbox

Start with a realistic local demo before connecting your own mail:

```bash
mxr demo
```

This creates a separate `mxr-demo` config, database, socket, and daemon. It seeds a 50k-message, two-account synthetic inbox with repeat senders, threads, attachments, links, images, newsletters, promos, spam, suspicious inbox mail, receipts, alerts, and demo rules. You get search, labels, summaries, sender profiles, analytics, and keyboard triage without granting access to your real inbox.

`mxr demo` prints progress and waits for the daemon however long each step needs — none of its long-running calls (seed, sync, analytics, semantic) is capped by a timeout. Expect a couple of minutes end to end on a laptop, longer on a small cloud VM; the mailbox is seeded and the demo is announced as active around the 30–45 second mark. After that it prewarms analytics and Wrapped so the first click on each surface is instant, and watches semantic indexing briefly before handing over — embedding 50k messages takes far longer than anyone should wait, so it reports how far it got and continues in the background (`mxr semantic status`). Search works throughout; semantic results sharpen as vectors land. Press Ctrl-C any time during the prewarm phase and start using the demo. Pass `--messages` for a smaller mailbox (`mxr demo --messages 5000`).

The demo also pre-seeds every "empty queue" surface so the first click on any feature shows something useful: snippets, signatures, custom labels, saved searches, screener decisions, snoozed messages, reply-later flags, and a couple of in-progress drafts. LLM-backed features (summarize, briefing, ask, draft-assist, voice, decisions, commitments) are answered by an in-process **canned provider**, so the demo works fully offline — no `OPENAI_API_KEY` needed, and your real LLM credentials are never invoked even if `[llm]` is configured.

**Demo mode is sticky.** Once `mxr demo` finishes, every subsequent `mxr` command (`mxr search`, `mxr cat`, `mxr archive`, `mxr web`, ...) automatically targets the demo profile. The TUI status bar and the web app's topbar both show a `DEMO` chip so you always know which profile you're on. Exit with:

```bash
mxr demo stop
```

Other demo subcommands:

```bash
mxr demo status        # is demo active? where are its files?
mxr demo reset         # wipe demo data so the next `mxr demo` re-seeds from scratch
mxr demo --reset       # equivalent: reset + restart in one step
mxr demo --no-tui      # seed + sync without launching the TUI
```

## 2. Add your account

For Gmail on your local machine:

```bash
mxr accounts add gmail
```

The wizard prompts for an account name and your Gmail address, then opens a browser to authorize. If you're SSH'd into a remote box, see the [Gmail setup notes on SSH-friendly flows](/getting-started/gmail-setup/#working-over-ssh-or-in-a-container).

For Outlook.com, Hotmail, or Live:

```bash
mxr accounts add outlook
```

For a Microsoft 365 work or school account:

```bash
mxr accounts add outlook-work
```

For IMAP+SMTP non-interactively (e.g. Gmail with an app password):

```bash
MXR_IMAP_PASSWORD="$APP_PASSWORD" MXR_SMTP_PASSWORD="$APP_PASSWORD" \
  mxr accounts add imap-smtp \
    --account-name personal \
    --email you@gmail.com \
    --imap-host imap.gmail.com \
    --imap-username you@gmail.com \
    --smtp-host smtp.gmail.com \
    --smtp-username you@gmail.com
```

`MXR_IMAP_PASSWORD` / `MXR_SMTP_PASSWORD` env vars resolve when stdin is not a TTY — handy for CI. You can also pass `--imap-password` / `--smtp-password` literal values if you don't mind shell history.

## 3. Sync

`mxr sync` starts a sync in the daemon and returns as soon as it has started. The account keeps syncing afterwards, so you can start reading mail right away. Add `--wait` to stay until that account reports idle:

```bash
mxr sync                                  # trigger, return immediately
mxr sync --wait                           # trigger, then wait for idle
mxr sync --wait --wait-timeout-secs 900   # a first backfill needs a longer wait
```

What you get: with `--wait`, a live progress line on stderr each time the count moves, then exit 0 once the account is idle.

```text
personal: 3,000/50,000 — Stored 3000 messages
```

The denominator appears only when the provider can say how much is left. Gmail and IMAP cannot count the remainder without paying for it, so there you get a bare running count.

:::caution[`--wait-timeout-secs` bounds the wait, not the sync]
It defaults to 60 seconds. When it expires, `mxr sync` exits non-zero with `timed out after 60s waiting for sync to quiesce` and the sync carries on in the daemon. Give a first backfill a generous value.
:::

`--wait` also exits non-zero if the sync it started failed — an error the account was already carrying before the trigger does not count.

Check on it any time without waiting:

```bash
mxr sync --status
mxr sync --status --format json | jq '.[] | {account_name, sync_in_progress, progress}'
```

Subsequent syncs are incremental and run automatically in the background once the daemon is up.

## 4. Open the TUI

```bash
mxr
```

`j`/`k` to navigate, `<Enter>` to open, `R` for reader mode, `Ctrl-p` for the command palette, `/` for search, `?` for help.

## 5. Or do it from the CLI

```bash
# Search
mxr search "from:alice@example.com is:unread" --format json | jq .

# Read the first match
mxr cat <message-id>

# Reply and send
mxr reply <message-id> --body "On it." --yes

# Archive a query
mxr archive --search "label:newsletters older_than:30d" --dry-run
mxr archive --search "label:newsletters older_than:30d" --yes
```

Most read/list/status/mutation surfaces accept `--format json`; the generated [CLI reference](/reference/cli/) lists the exact formats per command.

## What's next

- [Configure rules](/guides/rules/) for declarative filing.
- [Write a saved search](/guides/search/) for your daily inbox lens.
- [Hand mxr to an LLM](/guides/agent-skill/) — the same CLI is the agent surface.

Run into something? See [Troubleshooting](/troubleshooting/).
