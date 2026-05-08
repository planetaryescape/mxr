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

## Send confirmation

After the editor closes, mxr shows a confirmation modal:

- Changed draft: send, save draft, edit again, discard
- Unchanged draft: edit again or discard

## Account selection

The sender address comes from the selected/default runtime account, not from a static status snapshot. This matters for multi-account setups.

## Attachments

CLI compose supports:

```bash
mxr compose --attach ./invoice.pdf --attach ./notes.txt
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

- [Crash-safe drafts](/guides/crash-safe-drafts/)
- [LLM features — draft assist](/guides/llm-features/)
- [Recipes — compose loops](/guides/recipes/#with-editor--compose-loops)
- [CLI — Compose and drafts](/reference/cli/#compose-and-drafts)
