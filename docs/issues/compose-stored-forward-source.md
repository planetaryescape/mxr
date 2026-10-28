# A stored forward draft forgets the message it forwards

**Status:** open · **Found:** cross-model review of writing help (S4a round 1, finding 8) · **Severity:** should-fix

## What happens

A draft stores its recipients, subject and body, and a reply's `reply_headers`.
A forward has no `reply_headers`. Forward preparation now retains the original
content and attachments in the saved draft, and its compose file carries a
`forward_source` descriptor. The canonical `Draft` row still does not record
the source message id, so a reopened stored draft cannot resolve that message's
conversation or relationship context.

`ComposeAssist` with a `Stored` target resolves a draft's conversation from
`reply_headers`. For a forward there are none, so the daemon writes it as a new
message to the first recipient: no resolved conversation reaches the model, and the
subject (which came from the forwarded mail) is shown as data, not as a task.

## Effect

- `mxr compose assist --draft <id>` on a forward does not resolve the source
  conversation, even though the forwarded text is present in the draft.
- A forward reopened from Drafts and helped in the web app takes the same path.
- A forward being composed right now is fine in the web app: it knows its source
  and sends the `forward` target.

## Workaround

Ask with the source: `mxr compose assist --forward <message-id> --to <address>
--text <file>`, or the `forward` target over the bridge.

## Proposed fix

Store the source message id on the draft when a forward is created (a nullable
`source_message_id` column, filled by `PrepareForward` and the compose paths),
expose it on `Draft`, and let `gather` resolve a stored forward through
`in_conversation` with the recipient, as it does for a reply. Older drafts keep
the current behaviour. Needs a migration and a `Draft` field with a serde default,
so it is its own change.
