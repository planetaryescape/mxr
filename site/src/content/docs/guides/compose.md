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
mxr send DRAFT_ID --dry-run --format json
mxr send DRAFT_ID --expected-revision REVISION_FROM_PREVIEW \
  --expected-workspace '<OWNER_JSON_FROM_PREVIEW>' \
  --operation-id OPERATION_ID_FROM_PREVIEW
mxr send-outcome OPERATION_ID_FROM_PREVIEW --account ACCOUNT_ID \
  --expected-workspace '<OWNER_JSON_FROM_PREVIEW>' --format json
```

Copy the exact `expected_workspace` JSON from the original preview and pass it
unchanged in single quotes to `--expected-workspace` for send and outcome queries.

## Replies through a paired remote host

Request explicit host approval with `mxr connect HTTPS_ORIGIN --allow-send`; default registrations stay read-only. Reply text and `$EDITOR` remain on the client. Attachments upload as managed bytes, and previews do not stage resources. Save the draft, preview the returned revision, and keep its account, operation ID and original workspace for sending and recovery. Path-backed legacy drafts are refused remotely. Follow [Read and reply from another computer](/guides/remote-mailbox/) for the complete CLI, TUI and MCP journey.

## Local and linked provider drafts

mxr's local draft store is canonical. The CLI, TUI, web app, and MCP all read
the same rows. List or edit them with:

```bash
mxr drafts --format json
mxr drafts edit DRAFT_ID
```

Delete uses the same draft selection for preview and commit. For a linked
draft, commit deletes the provider draft first and then the local row;
provider failures preserve the local row. `discard` remains an alias for older
scripts:

```bash
mxr drafts --format json delete DRAFT_ID --dry-run
mxr drafts delete DRAFT_ID
```

Gmail accounts can link a local draft to Gmail Drafts:

```bash
mxr drafts --format json push DRAFT_ID --dry-run
mxr drafts push DRAFT_ID
```

The first push creates one Gmail draft and records its stable draft id.
Repeating `push`, or editing the linked draft from any mxr client, updates that
same Gmail draft in place. Normal `mxr sync` uses Gmail's changing message id
as a revision: Gmail-side edits replace the same local draft, and Gmail-side
deletion removes it locally. No MIME diff is required. Unsupported providers
are refused without changing the local draft.

In `mxr web`, **Drafts** in the sidebar opens this same local list. Open a row
to edit it, use the confirmed trash action to delete both linked copies, or
choose **Save to server draft** to create/update the provider link.

In the TUI, open the command palette and choose **Drafts**, or press `gE`.
The draft browser uses `e` to edit, `d` to preview and confirm linked deletion,
and `p` to preview and confirm provider sync. Unsupported accounts are
refused without changing the local draft. A draft scheduled to send later
shows when it sends; `c` previews and cancels the scheduled send and keeps
the draft.

[Follow the linked Gmail draft workflow](/guides/linked-drafts/) for CLI and
MCP examples, sync behavior, and failure handling.

## Save as a draft (don't send)

`compose`, `reply`, `reply-all`, and `forward` each save a draft with
`--draft` and send with `--yes`. The two flags are mutually exclusive, so
no added flag can turn a save into a send — pass `--draft` and the command
cannot transmit.

```bash
# Save a reply-all as a draft. Threading is preserved; nothing is sent.
mxr reply-all MESSAGE_ID --body "Thanks all." --draft
# Review the saved draft before sending it.
mxr send DRAFT_ID --dry-run --format json

# Same for a new message, a reply, or a forward.
mxr compose --to alice@example.com --subject "Friday" --body "Notes below." --draft
mxr reply MESSAGE_ID --body "On it." --draft
mxr forward MESSAGE_ID --to team@example.com --body "FYI" --draft

# Preview first — the dry-run reports "save draft", matching what runs.
mxr reply-all MESSAGE_ID --body "Thanks all." --draft --dry-run

# --draft and --yes cannot be combined:
mxr reply-all MESSAGE_ID --draft --yes
# → error: the argument '--draft' cannot be used with '--yes'
```

Without `--draft` and without `--yes`, an interactive `mxr reply`
(no `--body`) opens `$EDITOR` and then saves a draft; adding `--yes` is
what sends. Reach for `--draft` when you want the save to be explicit and
unsendable — especially from scripts or agents, where an accidental
`--yes` would otherwise transmit. One-shot `compose`, `reply`, `reply-all`, or
`forward` with `--yes` prints its operation ID and query command to stderr
before submission. If its response is lost, query that operation; rerunning
the one-shot command creates a new draft and can send a second message. For a
saved draft, first run `mxr send DRAFT_ID --dry-run --format json`, then submit
the returned revision, exact `expected_workspace` JSON and operation ID with
`--expected-revision`, `--expected-workspace` and `--operation-id`. See the
[crash-safe drafts guide](/guides/crash-safe-drafts/) for recovery rules.

The send preview returns `expected_workspace` with the workspace ID and
recovery generation. Keep that JSON with the draft revision and operation ID;
pass it unchanged when sending, scheduling, or querying the send outcome. The
CLI captures the selected workspace before opening its editor. If the selected
daemon changes before private work is saved or submitted, mxr refuses to
rebind that work. Return to its original workspace and explicitly reopen or
review it before continuing. See [crash-safe drafts](/guides/crash-safe-drafts/)
for recovery after a workspace conflict.

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
from: hello@planetaryescape.xyz
---

Hello from mxr.
```

The `from:` field is the address the message is sent from. Leave it as the
account's primary address, or set it to any [registered alias](#sending-from-an-alias-per-message-from)
to send as that address. Editing `from:` in `$EDITOR` works too.

## HTML email

Some mail is designed, not written: a branded template with tables, inline CSS,
media queries, Outlook conditional comments and a logo. Markdown cannot express
that, so `--html-file` takes the document as-is.

```bash
mxr compose \
  --account notto \
  --to person@example.com \
  --subject "Product Digest" \
  --html-file message.html \
  --text-file message.txt \
  --inline notto-logo=assets/notto-logo.png \
  --draft
```

`--html-stdin` reads the same thing from a pipe. HTML mode is explicit — mxr
never guesses that a `--body` string is HTML — and it is mutually exclusive with
`--body` and `--body-stdin`.

### Your HTML is not rewritten

The document mxr builds contains the bytes you supplied. It is not reformatted,
re-wrapped, minified, prettified, or sanitised. Tables, inline `style`
attributes, `<style>` blocks, `@media` queries, Outlook conditional comments,
and Unicode like ® all survive unchanged.

The `text/html` part is base64-encoded rather than quoted-printable
specifically to guarantee this: quoted-printable would rewrite your line endings
to CRLF, so an LF-only file would not decode back to the file you wrote.

### Dangerous content is refused, not stripped

mxr parses the document to check it, then throws the parse away. If it finds
active content it reports the problem and refuses to create the draft:

- `<script>`, `<object>`, `<embed>`, `<applet>`, `<iframe>`, `<form>` and other
  executing or submitting elements
- inline event handlers (`onclick=` and friends)
- URLs using a scheme outside `http`, `https`, `mailto`, `cid`, `tel` and
  `data:image/*` — `javascript:` and `vbscript:` are rejected
- `<style>` blocks containing `expression()` or `javascript:`
- any of the above hidden inside a conditional comment

It will not quietly delete the offending tag and send the rest. You get the tag,
the line number, and an unmodified file to fix.

This check runs in the daemon, not just the CLI, so a client speaking IPC
directly gets the same treatment.

### The plain-text alternative

Every HTML message goes out as `multipart/alternative` with a `text/plain` half.
Supply it with `--text-file`, or let mxr generate one from the HTML — the same
renderer the reader uses. Generating it does not alter the HTML.

### Inline images

`--inline NAME=PATH` attaches an image as a CID-referenced part so the HTML can
reference it:

```html
<img src="cid:notto-logo" alt="Notto">
```

The parts nest as `multipart/related` wrapping the alternative, which is the
shape Gmail's own composer produces and the one clients reliably render. Using
`multipart/mixed` instead is the common mistake that makes inline images show up
as attachments. If the HTML references a `cid:` no `--inline` provides, mxr
warns but does not block — the author may know something mxr does not.

### Signatures

A signature is **never** injected into supplied HTML. Splicing markdown into a
designed document is exactly the kind of silent edit this feature exists to
avoid. Opt in explicitly with `--signature-html <path>`, which appends before
the closing `</body>`. Markdown composition is unchanged: it still appends the
account signature as before.

### Editing an HTML draft

`mxr drafts edit` refuses an HTML draft. The compose file is markdown, and
round-tripping a designed document through it would destroy the markup. Edit the
source file and create a new draft.

### Upgrading from an earlier version

Schema migration 049 adds four columns to `drafts` (`body_html`, `body_text`,
`inline_assets`, `content_kind`). It is additive and forward-only: existing
drafts keep their `body_markdown`, default to `content_kind = 'markdown'`, and
are not rewritten. Nothing needs to be exported or re-created.

The JSON a draft serialises to is unchanged for markdown drafts — still
`{"body_markdown": "..."}` — so existing scripts keep working. HTML drafts
carry `body_html` and `body_text` instead, which older clients simply will not
recognise as a body they can render.

### A caveat about Gmail

mxr guarantees the MIME **it produces**. What Gmail's API then does on send is
outside its control, and Gmail is known to re-serialise outgoing messages: it
rewrites MIME boundaries and replaces a supplied `text/plain` part with one it
generates from the HTML. Stored drafts (`--draft`) are not affected, and the
SMTP path transmits mxr's bytes unchanged. If byte-level fidelity at the
recipient matters, prefer an SMTP account.

## Reply context

Reply and forward drafts include message context. If the original message only had HTML, mxr uses the rendered reader output, not raw HTML tags.

## Reply recipient

A reply targets the original message's `Reply-To:` header when the sender set one, falling back to `From:` otherwise. This is what mailing lists and `no-reply@` senders rely on — a reply to a list digest goes to the list, not the unmonitored sender address. `reply-all` adds the other original recipients as Cc on top of that target.

The reply's own From defaults to the owned address the original was delivered to — see [Sending from an alias](#sending-from-an-alias-per-message-from).

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

# Preview first and retain the reviewed revision and operation ID.
mxr send DRAFT_ID --dry-run --format json

# If --check turned up a Blocker you accept, use its single-use token
# with the same reviewed identity.
mxr send DRAFT_ID --expected-revision REVISION_FROM_PREVIEW \
  --expected-workspace '<OWNER_JSON_FROM_PREVIEW>' \
  --operation-id OPERATION_ID_FROM_PREVIEW \
  --override-safety OVERRIDE_TOKEN_FROM_CHECK
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

Those are two related choices, not one identity. An account key such as
`--from work` selects that account and uses its configured account email. An
owned address such as `--from accounts@example.com` selects the account that
owns the address and uses that exact address as the message From. If more than
one account owns the address, add `--account` to disambiguate it.

The sender address comes from the selected/default runtime account, not
from a static status snapshot. This matters for multi-account setups.

```bash
mxr drafts --account work --format json
```

## Sending from an alias (per-message From)

An account can own several addresses: its primary plus any aliases you
[register](/guides/accounts/#owned-addresses-and-aliases) with `mxr accounts
addresses add`. Any owned address can be the From on a per-message basis —
handy for a shared mailbox with `hello@`, `accounts@`, and `support@` on one
account.

Pick the sender three ways, all landing on the same daemon-side path:

```bash
# --from accepts a registered alias on compose, reply, reply-all, and forward.
mxr compose --from accounts@planetaryescape.xyz --to client@example.com \
  --subject "Invoice 42" --dry-run

mxr reply MESSAGE_ID --from support@planetaryescape.xyz --body "On it."
```

```md
# ...or edit the `from:` frontmatter field in $EDITOR:
---
to: client@example.com
subject: Invoice 42
from: accounts@planetaryescape.xyz
---
```

Rules:

- **Owned addresses only.** The chosen From must be the account's primary or a
  registered alias (matched case-insensitively). An unowned address is rejected
  before anything is sent, with the list of registered addresses in the error —
  the same check on a real send, a `--dry-run`, and the send-confirm modal.
- **Replies default to the delivered-to address.** A reply, reply-all, or
  forward starts from whichever owned address the original was delivered to
  (its `To`/`Cc`/`Delivered-To`), falling back to the primary. So a message
  that arrived at `support@` is answered from `support@` unless you override it.
- **`--from` / `from:` win.** An explicit value overrides the reply default.
- The From flows into both the message `From:` header and the SMTP envelope
  sender; `--dry-run --format json` reports the effective `from`.
- **Provider permission is separate.** Registering an owned address in mxr
  does not create the alias or grant send-as permission at Zoho, Gmail, or an
  SMTP provider. Configure the alias there first. A dry-run proves mxr's local
  account and From resolution; the provider still makes the final acceptance
  decision when you send.

## Attachments retain the uploaded bytes

Preview a compose request, then save its attachments as immutable daemon resources:

```bash
mxr compose --to alice@example.com --subject 'Notes' --body 'Please read the attachment.' \
  --attach ./notes.txt --draft --dry-run --format json
mxr compose --to alice@example.com --subject 'Notes' --body 'Please read the attachment.' \
  --attach ./notes.txt --draft --format json
```

`--attach` reads the file on your client and uploads bounded chunks through its
connection. The saved draft keeps the uploaded bytes after you edit or delete
the client file or restart the daemon. Each managed resource is limited to
25 MiB; provider limits may differ. Dry runs create no draft or upload.

To add files to a saved draft, preview and use its current revision:

```bash
mxr drafts --format json attach DRAFT_ID ./another.txt --dry-run
mxr drafts --format json attach DRAFT_ID ./another.txt --expected-revision REVISION
mxr send DRAFT_ID --dry-run --format json
```

The save response contains the canonical draft revision and
`managed_attachments` metadata: opaque ID, owner draft/account, original
filename, MIME type, byte length and SHA-256. Review that returned revision
before sending or scheduling.

The TUI uses the same upload path when you add client file paths in `$EDITOR`.
Reopening a saved draft renders each managed resource as an
`mxr-resource:RESOURCE_ID/filename` entry in `attach:`. Keep the entry to retain
it; remove it to detach the resource. Browser compose uploads files in bounded
chunks and retains the same managed resources after save/reload.

```bash
mxr drafts edit DRAFT_ID
```

Older saved drafts may still contain legacy daemon-local paths. Those drafts
remain readable and sendable, and their bytes remain path-backed. A client that
cannot display managed resources is refused when it tries to edit, preview,
send or schedule a draft containing them; upgrade the client.

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
the saved draft and its durable outcome remain available for inspection.
An `unknown` result is not evidence that sending failed. Do not reset or
resend it; query the retained operation first. A `not_accepted` result
allows a fresh review and an explicit new operation.

```bash
mxr drafts recover
mxr send-outcome OPERATION_ID --account ACCOUNT_ID \
  --expected-workspace '<OWNER_JSON_FROM_ORIGINAL_PREVIEW>' --format json
mxr drafts discard DRAFT_ID  # permanently delete
```

See the [crash-safe drafts guide](/guides/crash-safe-drafts/) for the
full state machine.

## In real life

- **You write better at night, send better in the morning:** compose
  the draft, save it (`mxr drafts`), preview it with `mxr send DRAFT_ID
  --dry-run --format json`, then schedule the reviewed revision with
  `mxr send DRAFT_ID --at 'tomorrow 9am' --expected-revision
  REVISION_FROM_PREVIEW --expected-workspace
  '<OWNER_JSON_FROM_PREVIEW>'`.
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
- [CLI — `mxr compose`](/reference/cli/compose/) and [`mxr drafts`](/reference/cli/drafts/)

## The compose sheet in the web app

Popout and fullscreen open the editor as a sheet over a scrim, so the rows
behind stop answering until you close it (the draft is saved). A reply started
inline stays in the thread; if you leave the thread it docks in the corner
without a scrim. **Rich text** and **Markdown** are one switch. The Markdown
editor is the vim one and shows its mode line; Rich text has none.
