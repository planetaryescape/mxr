---
title: Compose
description: Compose, reply, reply-all, and forward through $EDITOR.
---

## Core model

mxr writes drafts in your editor. The daemon handles parsing, validation, send, save-draft, and provider delivery.

This applies to:

- New compose
- Reply
- Reply-all
- Forward
- Draft editing

## CLI

```bash
mxr compose
mxr compose --to alice@example.com --subject "hello"
mxr reply MESSAGE_ID
mxr reply-all MESSAGE_ID
mxr forward MESSAGE_ID --to team@example.com
mxr drafts
mxr send DRAFT_ID
```

## TUI

- `c`: compose
- `r`: reply
- `a`: reply all
- `f`: forward

If you start from a thread view, reply actions target the focused message, not the latest message in the thread.

## Draft format

Drafts use YAML frontmatter plus body text:

```md
---
to:
  - alice@example.com
cc: []
bcc: []
subject: Example
---

Hello from mxr.
```

## Reply context

Reply and forward drafts include message context. If the original message only had HTML, mxr uses the rendered reader output, not raw HTML tags.

## Reply recipient

A reply targets the original message's `Reply-To:` header when the sender set one, falling back to `From:` otherwise. This is what mailing lists and `no-reply@` senders rely on — a reply to a list digest goes to the list, not the unmonitored sender address. `reply-all` adds the other original recipients as Cc on top of that target.

## Send confirmation

After the editor closes, mxr shows a confirmation modal:

- Changed draft: send, save draft, edit again, discard
- Unchanged draft: edit again or discard

The modal also shows the [pre-send safety verdict](/guides/pre-send-safety/)
(SAFE / WARN / BLOCKED) and any issues found. Blocker issues require a
fix or override token before the `s` (send) key is enabled.

## Pre-send safety

Every send runs through a [six-check safety pipeline](/guides/pre-send-safety/):
wrong recipient, missing attachment, reply-all sanity, PII/secrets, tone
mismatch, and answer-coverage. The pipeline runs in three places:

- The TUI send-confirm modal after `Ctrl-x` (changed draft).
- The CLI on `mxr send DRAFT_ID` (gates the send) and `mxr send
  DRAFT_ID --check` (dry-run only).
- The scheduled-send flusher when a scheduled send fires.

```bash
# Dry-run a stored draft (no provider call). Exit 2 on Blocker.
mxr send DRAFT_ID --check --format json

# Same idea, but for a transient draft built from CLI args — no daemon
# row created.
mxr compose --to alice@example.com --body 'see attached' --check

# If --check turned up a Blocker you accept (e.g. you really do mean
# to email competitor.com), it minted a single-use override token.
mxr send DRAFT_ID --override-safety OVERRIDE_TOKEN_FROM_CHECK
```

## Account selection

```bash
mxr compose --from work --to alice@example.com --subject "Follow-up"
mxr reply MESSAGE_ID --account work --body "Thanks." --dry-run
mxr forward MESSAGE_ID --account personal --to bob@example.com --dry-run
```

What you get: a new draft from the selected sender, plus reply/forward
previews that first confirm the original message belongs to that account.

New compose uses `--from <account-or-address>` to choose the sender.
Replies and forwards can also take `--account <selector>` to assert which
account owns the original message before drafting.

The sender address comes from the selected/default runtime account, not
from a static status snapshot. This matters for multi-account setups.

```bash
mxr drafts --account work --format json
```

## Attachments

CLI compose supports:

```bash
mxr compose --attach ./invoice.pdf --attach ./notes.txt
```

Every compose flow stores attachment paths in the draft's `attach:` frontmatter.
That gives replies, reply-all drafts, and forwards the same editor escape hatch
even when a client does not expose a dedicated attachment control:

```md
---
to:
  - team@example.com
subject: Re: Investor letter
attach:
  - /Users/alice/Documents/investor-letter.docx
---

Please find my rewrite attached.
```

Add or remove those paths while the draft is open in `$EDITOR`, or reopen a
saved draft first:

```bash
mxr drafts edit DRAFT_ID
```

TUI message viewing supports attachment open/download. Compose-side attachment management is through the editor and CLI.

## Snippets

mxr can store short stock replies for reuse in compose flows. Use
`mxr snippets set thanks "Thanks for reaching out, will follow up shortly."`
once, then browse or copy it when drafting. Built-in variables:
`{first_name}`, `{date}`, `{thread_subject}`.

```bash
mxr snippets list
mxr snippets set decline "Thanks for the offer; can't take this on right now." --vars ""
mxr snippets remove decline
```

The TUI exposes a read-only snippets browser via `Ctrl-p → Snippets`.

## Recipes

```bash
# Send a quick reply from the command line, body via heredoc
mxr reply MESSAGE_ID --body-stdin <<'EOF'
Confirmed for Friday at 14:00 GMT.
EOF

# Compose with multiple attachments and dry-run before sending
mxr compose --to team@example.com --subject "Q1 numbers" \
  --attach ~/work/q1-summary.pdf \
  --attach ~/work/q1-charts.png \
  --dry-run

# Reply to every flagged "reply later" item in a row, interactively
mxr replies --format ids | while read id; do
  mxr cat "$id" --view reader
  read -p "Reply? [y/N/q] " a < /dev/tty
  case "$a" in y) mxr reply "$id" ;; q) break ;; esac
done
```

## Crash safety

A draft you're editing lives in SQLite. If the daemon dies mid-send,
the row sits in `'sending'` state — and the next daemon startup auto-
resets anything older than an hour back to `'draft'`. To act sooner:

```bash
mxr drafts recover           # show orphans
mxr drafts resume DRAFT_ID   # back to 'draft'; retry with mxr send
mxr drafts discard DRAFT_ID  # permanently delete
```

See the [crash-safe drafts guide](/guides/crash-safe-drafts/) for the
full state machine.

## In real life

- **You write better at night, send better in the morning:** compose
  the draft, save it (`mxr drafts`), then send tomorrow with `mxr send
  DRAFT_ID --at 'tomorrow 9am'`.
- **Repetitive client onboarding emails:** keep a `;welcome` snippet
  with `{first_name}` placeholders.
- **Composing from a script:** `--body-stdin` lets you pipe rendered
  Markdown / templated output straight into a reply without ever
  opening an editor.

## Agent prompts that work

```text
"Draft a polite decline to the latest message from acme.com using my
prior tone. Don't send — write to a draft so I can review with `mxr
drafts`. Use `mxr draft-assist` for the body."
```

```text
"Schedule a Friday-afternoon nudge to anyone I haven't replied to in 7
days. Use `mxr stale --mine --older-than-days 7 --format ids | xargs
-I{} mxr remind {} --when 'friday 16:00'`. Show me what would fire."
```

## See also

- [Pre-send safety](/guides/pre-send-safety/) — the six checks every
  draft passes through, plus the override-token flow
- [Crash-safe drafts](/guides/crash-safe-drafts/)
- [LLM features — draft assist](/guides/llm-features/)
- [Recipes — compose loops](/guides/recipes/#with-editor--compose-loops)
- [CLI — Compose and drafts](/reference/cli/#compose-and-drafts)
