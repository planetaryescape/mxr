---
title: Unsubscribe
description: Understand and use List-Unsubscribe safely from mxr.
---

`List-Unsubscribe` is an email header, not a visible link in the message body.
Mailing-list software adds it so mail clients can show a real unsubscribe
action without scraping the HTML.

```text
List-Unsubscribe: <mailto:list@example.com?subject=unsubscribe>, <https://example.com/u/abc>
List-Unsubscribe-Post: List-Unsubscribe=One-Click
```

mxr parses those headers during sync and stores the best available unsubscribe
method on the envelope. Check what mxr found before doing anything:

```bash
mxr subscriptions --rank --format json \
  | jq '.[0] | {sender_email, message_count, opened_count, archived_unread_count, unsubscribe}'
```

What you get: the sender, local engagement counts, and method mxr can use
(`OneClick`, `Mailto`, `HttpLink`, `BodyLink`, or `None`).

## What mxr will do

mxr treats unsubscribe as a normal mutation: preview first, then confirm.

```bash
mxr unsubscribe newsletter@example.com --dry-run
mxr unsubscribe newsletter@example.com --yes
```

What happens after confirmation depends on the stored method:

| Method | Source | Action |
|---|---|---|
| `OneClick` | `List-Unsubscribe` + `List-Unsubscribe-Post` | POSTs `List-Unsubscribe=One-Click` to the HTTPS URL |
| `Mailto` | `List-Unsubscribe` | Sends a short unsubscribe email through the account's send provider |
| `HttpLink` | `List-Unsubscribe` | Opens the unsubscribe page in your browser |
| `BodyLink` | HTML body fallback | Opens the unsubscribe page in your browser |
| `None` | No usable method | Fails without mutating anything |

mxr records a successful unsubscribe event. Re-running the same unsubscribe is
idempotent, so scripts do not accidentally fire the side effect twice.

```bash
mxr activity --category mutation --format json \
  | jq '.[] | select(.action == "mail.unsubscribe")'
```

## Header method vs body link

The public `list-unsubscribe` crate only parses the standard headers. mxr adds
one local fallback: if the headers have no usable method, Gmail and IMAP parsing
look for an unsubscribe-ish link in the HTML body and store it as `BodyLink`.

```bash
mxr cat --search 'from:newsletter@example.com' --first --format json \
  | jq '{subject, unsubscribe}'
```

Use the distinction when debugging:

- `OneClick`, `Mailto`, and `HttpLink` came from the header.
- `BodyLink` came from the message body and may be less reliable.
- `None` means mxr has no safe unsubscribe action for that message.

## Bulk cleanup

Rank noisy senders, preview the unsubscribe, then archive the residue.

```bash
mxr subscriptions --rank --format json \
  | jq -r '.[]
      | select(.message_count >= 4 and .opened_count == 0)
      | .sender_email'
```

What you get: subscription senders with at least four messages and zero local
`READ` flags. `opened_count` is a read-state count, not a tracking-pixel or
distinct-open metric; `opened_count == message_count` means every message in
that sender bucket is already read locally, whether from the `mxr read` command,
another client, provider-side state sync, or a bulk mark-read action.

For each sender you approve:

```bash
mxr unsubscribe sender@example.com --dry-run
mxr unsubscribe sender@example.com --yes
mxr archive --search 'from:sender@example.com' --dry-run
mxr archive --search 'from:sender@example.com' --yes
```

What you get: one unsubscribe side effect, then a normal archived-mail mutation
you can see in `mxr history`.

## See also

- [CLI reference — unsubscribe](/reference/cli/unsubscribe/)
- [Mailbox workflow](/guides/mailbox/)
- [For agents](/guides/for-agents/)
- [Automation contract](/guides/automation-contract/)
