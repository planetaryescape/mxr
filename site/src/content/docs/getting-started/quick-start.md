---
title: Quick start
description: Try mxr safely, then connect your own inbox.
---

After [installing](./install/), you have one binary: `mxr`. The TUI is what you'll use most; the CLI handles scripting and one-off operations. Both talk to the same daemon.

## 1. Try the demo inbox

Start with a realistic local demo before connecting your own mail:

```bash
mxr demo
```

This creates a separate `mxr-demo` config, database, socket, and daemon. It seeds a 50k-message, two-account synthetic inbox with repeat senders, threads, attachments, links, images, newsletters, promos, spam, suspicious inbox mail, receipts, alerts, and demo rules. Demo setup also prewarms analytics, Wrapped, and semantic vectors for the active profile so you can try search, labels, summaries, sender profiles, analytics, and keyboard triage without granting access to your real inbox.

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

The wizard prompts for an account name and your Gmail address, then opens a browser to authorize. If you're SSH'd into a remote box, see the [Gmail setup notes on SSH-friendly flows](./gmail-setup#working-over-ssh-or-in-a-container).

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

```bash
mxr sync --wait
```

`--wait` blocks until the initial sync completes. Subsequent syncs are incremental and run automatically in the background once the daemon is up.

## 4. Open the TUI

```bash
mxr
```

`j`/`k` to navigate, `<Enter>` to open, `R` for reader mode, `Ctrl-p` for the command palette, `/` for search, `?` for help.

## 5. Or do it from the CLI

```bash
# Search
mxr search "from:alice is:unread" --format json | jq .

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

- [Configure rules](../guides/rules/) for declarative filing.
- [Write a saved search](../guides/search/) for your daily inbox lens.
- [Hand mxr to an LLM](../guides/agent-skill/) — the same CLI is the agent surface.

Run into something? See [Troubleshooting](../troubleshooting/).
