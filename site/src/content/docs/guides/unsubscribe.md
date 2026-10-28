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

## Unsubscribe from Reading with evidence

In [Reading](/guides/reading/#unsubscribe-with-evidence), `D` on an item
opens this same unsubscribe for its source, with the evidence first: "You
opened 0 of the last 11 issues", how the sender is told (one click, a page
or an email) and that it can't be undone from mxr. It runs the dry run
below before anything is sent. A source that sent eight or more issues you
never opened gets a quiet "Unsubscribe?" offer on its items.
`mxr reading sources` prints each source's evidence. Choose **Just unsubscribe**
(Enter or `u`) to keep existing mail, or **Unsubscribe and clear** (`a`) to mark
read and archive the previewed issues. Both use the method shown in the preview.

## What mxr will do

mxr treats unsubscribe as a normal mutation: preview first, then confirm.

```bash
mxr unsubscribe newsletter@example.com --dry-run
mxr unsubscribe newsletter@example.com --yes
```

An address sends one request per sender. mxr finds the messages from that
sender (narrowed by `--search`, if you pass one) and uses the newest one with a
usable List-Unsubscribe method. It prefers a one-click URL, then an http(s)
link, then a mailto address. A link taken from the HTML body is the last
resort. The other messages from that sender are not used.

A sender that runs several lists, each with its own unsubscribe, needs one
request per list. Unsubscribe from each list by its message id:
`mxr unsubscribe MESSAGE_ID --dry-run`, then `--yes`.

The dry run shows the request before anything is sent:

```bash
mxr unsubscribe newsletter@example.com --dry-run --format json \
  | jq '{selected_messages, request_count, requests}'
```

What you get: how many messages matched, how many requests will be sent (0 or
1), and the message and method the request uses. The method is one of
`one-click`, `http-link`, `body-link` or `mailto:<address>`. If that newest
message already has a receipt, `already_unsubscribed` is `1` and nothing is
sent.

In the demo mailbox (`mxr demo --messages 2000`), `offers@promo.demo.mxr.local`
has 15 messages, and its dry run gives `"request_count": 1`. The request uses
the newest of them, a one-click URL. Its campaigns put a different token in each
unsubscribe URL, so one request per distinct URL would still have sent 8.

What happens after confirmation depends on the stored method:

| Method | Source | Action |
|---|---|---|
| `OneClick` | `List-Unsubscribe` + `List-Unsubscribe-Post` | POSTs `List-Unsubscribe=One-Click` to the HTTPS URL |
| `Mailto` | `List-Unsubscribe` | Sends a short unsubscribe email through the account's send provider |
| `HttpLink` | `List-Unsubscribe` | Opens the unsubscribe page in your browser |
| `BodyLink` | HTML body fallback | Opens the unsubscribe page in your browser |
| `None` | No usable method | Fails without mutating anything |

mxr records a successful unsubscribe event.

```bash
mxr activity list --action mail.unsubscribe --format json \
  | jq '.entries'
```

## Why a second unsubscribe is never sent

mxr keeps one receipt per message for each unsubscribe it sent. The receipt
holds the method and the time, with no subject, sender or URL. Once a message
has a receipt, running `mxr unsubscribe` on it again is acknowledged without
sending anything, so a script can retry without firing the side effect twice.

The receipt lives in its own table, so purging history (`mxr logs --purge`, or
the daily [history retention](/guides/observability/#automatic-history-retention))
cannot make a second request go out. Deleting the message deletes its receipt.

mxr claims the message before it sends, so two requests for the same message
cannot both send. A request that finds another one still running gets the error
code `unsubscribe_in_progress`, marked retryable. A claim that is more than ten
minutes old, left by a crash, is taken over.

If the send fails or is not confirmed, mxr releases the claim and answers with
the error code `unsubscribe_not_confirmed`, marked retryable. The first attempt
may have reached the sender, so a retry can send again. The CLI prints one line
to stderr for each message it skipped:

```text
Skipped 4912a411-7dad-5a95-89c8-c3049e918bd6 (The unsubscribe was not confirmed: could not complete the request to the sender's server (could not connect). A retry may send it again if the first attempt actually reached the sender.)
```

Failure text names neither the unsubscribe URL nor your address, because those
carry tokens and the error is logged. If the send went out but mxr could not
write the receipt, the error code is `unsubscribe_sent_not_recorded`, not
retryable. Check with the sender before trying again.

If the claim cannot be released after a failure, the error code is
`unsubscribe_release_failed`, also not retryable: mxr keeps the message marked
and will not resend it.

Unsubscribes sent before v0.6.136 are recovered from the event log when you
upgrade, and when you restore a backup made before that version.

## Keep mail with the sender preview

```bash
mxr unsubscribe sender@example.com --purge --keep-mail --dry-run --format json
mxr unsubscribe sender@example.com --purge --keep-mail --preview-token '<preview_token>' --yes --format json
```

Copy `preview_token` from the dry-run JSON into the second command. The token
expires after ten minutes and can be committed once. The daemon refuses it if
the sender's unsubscribe method changed. `--keep-mail` preserves read flags and
labels; omit it to read-archive exactly the previewed messages. Mail arriving
after the preview stays in place.

`--keep-mail` requires `--purge` and cannot be combined with
`--archive-on-no-method`. `--preview-token` requires `--purge --yes` and cannot
be combined with `--dry-run`. Without `--preview-token`, `--purge` previews and
commits within the same command.

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
