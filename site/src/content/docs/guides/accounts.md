---
title: Accounts
description: Manage runtime accounts and config-backed account definitions across Gmail, Outlook, IMAP, and SMTP.
---

## Account model

mxr distinguishes between:

- Runtime accounts: what the daemon is actually running now
- Config-backed accounts: editable account definitions used to produce runtime accounts

The TUI Accounts page is built from runtime inventory, not from config file entries alone.

## TUI accounts page

Open it with:

- `g` then `C`
- `Ctrl-p` then `Open Accounts Page`

Actions:

- `j` / `k`: move account selection
- `n`: new IMAP/SMTP account
- `Enter` / `o`: edit selected account
- `t`: test selected account
- `d`: set default account
- `c`: edit config
- `r`: refresh runtime account inventory

The page shows:

- Details on the left
- Account list on the right
- Runtime Gmail, Outlook, IMAP, and SMTP-backed accounts
- Editable config-backed accounts where supported
- Default-account state
- Provider kind and enabled state
- Last test/status messaging

## CLI account actions

```bash
mxr accounts
mxr accounts add gmail
mxr accounts add outlook       # Outlook.com, Hotmail, Live
mxr accounts add outlook-work  # Microsoft 365 work or school
mxr accounts add imap
mxr accounts add smtp
mxr accounts show ACCOUNT
mxr accounts test ACCOUNT
mxr accounts identity ACCOUNT --color amber --tag W --dry-run
```

## Colour and tag

Every account has a colour and a tag, so mixed lists can say which account a
row came from. Today the tag appears in CLI mail tables (see the `ACCT` column
below); the web app and the TUI do not draw it yet, and the Raycast extension
labels rows with the account's config key. The daemon gives an account its
colour and its tag when the account is created, and keeps both.

The tag comes from the account's name: "Work" is `W`, "Planetary Escape" is
`PE`. If that tag is taken, the name gives more letters or a digit, so
`Wedding` becomes `WE`. Renaming an account does not change its tag.

`mxr accounts` lists each account's tag and colour. The JSON output carries
the same values under `identity`.

To change them, pass `--color` (one of `sky`, `amber`, `green`, `yellow`,
`blue`, `vermilion`, `purple`, `slate`), `--tag` (1 to 3 letters or digits), or
both. Add `--dry-run` to see the change without writing it:

```bash
mxr accounts identity work --color amber --tag W --dry-run
mxr accounts identity work --tag OFF
```

```text
$ mxr accounts identity work --color amber --tag W --dry-run
Would set identity for account 'work' (before: tag DW, colour amber; after: tag W, colour amber).
  ok Dry run only; no changes made.
```

A tag already used by another account is refused, and the error names that
account:

```text
$ mxr accounts identity work --tag DP --dry-run
Failed to set identity for account 'work'.
  error Tag 'DP' is already used by account 'Demo Personal'.
```

An unknown colour is refused before anything is written, with the list of
valid slots. The same values can be set in config; see
[`accounts` in the config reference](/reference/config/#accounts).

Mail tables from `mxr search`, `mxr threads`, `mxr messages`, `mxr now`,
`mxr todo`, `mxr updates` and `mxr reading` show the tag, in an `ACCT` column
where the table has columns. The tag appears only when you have more than one
enabled account and the command was not given `--account`. Scripts that read
JSON or JSONL get `account_id` on each row, as before.

```text
$ mxr threads --limit 3 --format table
DP   2026-10-22 22:25  1 msgs (1 unread)  Boiler service at the flat
DP   2026-10-22 22:05  1 msgs (1 unread)  Okafor Lettings: tenant news for October
DW   2026-10-22 21:58  2 msgs (0 unread)  Launch checklist for Project Aurora
```

## Owned addresses and aliases

An account owns its configured account email plus any aliases you register.
mxr uses that set, together with the provider's sent mark, to classify mail as
inbound vs outbound, and to let you send from any of them. Mail is outbound when
its From is one of your owned addresses, or when Gmail or IMAP filed it as
sent. Gmail's SENT label and the IMAP Sent folder both count, so mail you send
from a Gmail send-as address counts as sent even if you have not added that
address as an alias. The current primary address cannot be removed; `remove` fails
with `cannot remove the primary address; set another address as primary first`,
so promote another address with `set-primary` first.

```bash
mxr accounts addresses list --account work
mxr accounts addresses add --account work accounts@planetaryescape.xyz
mxr accounts addresses add --account work support@planetaryescape.xyz
mxr accounts addresses set-primary --account work hello@planetaryescape.xyz
mxr accounts addresses remove --account work support@planetaryescape.xyz
```

Adding or removing an address reclassifies the account's stored mail against
the new set in the same command. Mail from an address you now own reads as
sent, and mail from an address you no longer own reads as received, unless the
provider filed it as sent. Reply
pairs follow in the same command. The contacts table is rebuilt in the
background, starting about ten seconds after the last change; a contacts or
analytics view already open shows the new counts after you reload it. A sync pass that was already running when you made the change
is reconciled before it releases its place in line, so the messages it
wrote take the new direction and their reply pairs are rebuilt with them. The
catch is timing: until that pass finishes, a message it wrote can still show
the old direction, for as long as the pass runs. If the reconciliation fails,
it is retried after the account's next successful sync pass. Relationship
profiles and resolution suggestions are not refreshed by an address change
yet.

Arrival history is not rewritten. An arrival records what arrived when it
arrived, and a correction you made stays as you made it. So mail you sent from
a new alias may still appear in an old arrival count, and mail from an alias you
removed is not added to arrival counts after the fact. The command reports how
many messages changed:

```bash
mxr accounts addresses add --account work support@planetaryescape.xyz
# Added support@planetaryescape.xyz. Stored mail reclassified: 14 now sent, 0 now received.
mxr accounts addresses add --account work support@planetaryescape.xyz --format json
# {"email":"support@planetaryescape.xyz","dry_run":false,"to_outbound":14,"to_inbound":0}
```

Add and remove take `--dry-run`. It prints the same counts the real change
would apply and changes nothing: the address, the stored directions and the
arrivals stay as they are. Run it first when you want to see the effect:

```bash
mxr accounts addresses add --account work support@planetaryescape.xyz --dry-run
# Dry run: nothing changed. Added support@planetaryescape.xyz. Would reclassify stored mail: 14 now sent, 0 now received.
mxr accounts addresses remove --account work support@planetaryescape.xyz --dry-run --format json
```

`set-primary` has no dry run. It changes only the primary marker, so it never
changes which addresses are owned. It still runs the account-wide
reconciliation, so its counts can be non-zero when rows from an earlier change
were stale.

Once an address is registered it becomes a valid sender inside mxr for that
account. A reply defaults its From to whichever owned address the original was
delivered to, and `--from` / the `from:` frontmatter field can send as any
owned address. See [Sending from an alias](/guides/compose/#sending-from-an-alias-per-message-from).
Only the account's own registered addresses are accepted; an unowned From is
rejected before the message is sent.

Register the alias with your mail provider first. Adding it to mxr does not
create the address at Zoho, Gmail, or another SMTP provider, and it does not
grant provider-side send-as permission. Confirm mxr's local sender resolution
before sending:

```bash
mxr compose --from accounts@planetaryescape.xyz --to alice@example.com \
  --subject "Invoice 42" --dry-run --format json
```

What you get: a preview whose `from` field names the exact address mxr will
hand to the provider. The provider can still reject an alias it has not
authorised.

The address registry also has an `is_primary` marker. In mxr 0.6.12,
`set-primary` and `add --primary` change that marker; they do not rewrite the
configured account `email` or the default From used by an account-key selector.
Existing aliases stay registered and remain valid per-message senders:

```bash
mxr accounts addresses add --account work hello@planetaryescape.xyz --primary
mxr accounts addresses list --account work --format table
```

What you get: `hello@planetaryescape.xyz` marked as primary in the owned-address
inventory, with the previous primary retained as a non-primary owned address.
Use an exact owned address with `mxr compose --from <address>` when you need
that address as the visible From.

## After you change an address

Adding, removing or promoting an address updates the daemon's set of owned
addresses at once. Reply and forward recipients, thread summaries and
triage verdicts all read from that set, so they follow the change: a cached
summary or triage verdict made under the old set is rebuilt the next time it
is read, and the TUI drops the reply recipients it had prepared. The daemon
announces the change as an event. Watch for it while you make one:

```bash
mxr events
mxr accounts addresses add --account work support@example.com
```

What you get: `account addresses changed account=<account-id>` in the
`mxr events` stream. A config reload that changes an account's addresses
sends the same event.

Sync decides each new message's direction (inbound or outbound) from the owned
set and the provider's sent mark. If the daemon cannot rebuild that set, it stores new mail with direction
`Unknown` instead of guessing, and `mxr doctor` warns:

```text
Account address cache is stale — new sync messages get Unknown direction until it rebuilds
```

The JSON report has the same signal as `address_cache_stale`. Run `mxr sync`, as
the finding suggests. Once the set rebuilds, the next sync
pass reclassifies the `Unknown` rows.

Registering an alias reclassifies mail that is already stored. Messages you
sent from an alias you never registered still count as sent when the provider
filed them as sent, which is the case for Gmail send-as addresses.

## Selecting an account

```bash
mxr accounts --format table
mxr search "is:unread" --account work --format json
mxr archive --account work --search "from:noreply@example.com older_than:30d" --dry-run
```

What you get: the account selectors available locally, account-limited
search results, and a preview of the matching work-account messages
before any mutation runs.

Most mail-facing CLI commands accept `--account <selector>`.

The selector can be:

- account key
- email address
- account id
- display name, when it matches exactly one account

Use the same selector on reads, lists, replies, and drafts:

```bash
mxr search "is:unread" --account work --format json
mxr count "from:notifications@github.com" --account you@example.com
mxr archive --account work --search "from:noreply@example.com older_than:30d" --dry-run
mxr reply MESSAGE_ID --account work --body "Thanks, will do." --dry-run
mxr drafts --account personal --format json
```

If you omit `--account`, mxr keeps the command's normal behavior. Search,
counts, lists, reads, and batch mutations operate across all enabled
accounts unless the command is already inherently tied to one account
(for example `mxr sync --account`, `mxr accounts show`, or compose sender
selection).

Unknown or ambiguous selectors fail before the command is sent to the
daemon. Direct-ID commands also check that the message, draft, delivery,
or invite belongs to the selected account before reading or mutating it.

List the selectors currently available:

```bash
mxr accounts --format json
```

## Compose behavior

Compose, reply, and forward resolve the sender from the selected/default runtime account. That is the same source the TUI Accounts page shows. To send from a specific [owned address](#owned-addresses-and-aliases) rather than the account primary, use `--from <address>` or edit the `from:` frontmatter field — see [Sending from an alias](/guides/compose/#sending-from-an-alias-per-message-from).

## Multi-account notes

- Sync can run per account.
- Mail reads, searches, saved searches, reply queues, deliveries, invites, drafts, and core mutations can run per account with `--account`.
- The daemon tracks account health in status and diagnostics.
- Changing editable accounts in the TUI triggers daemon reload so the runtime view updates without a restart.
- Runtime-only accounts are inspectable in the TUI even when they are not editable there.
