# Current State

This file is a code-truth audit as of 2026-05-28. It supersedes the
original pre-implementation audit that described calendar email as only
partial metadata extraction.

## What Shipped

`mxr` now handles email-derived calendar invites as a product feature. It
does not implement full calendar sync.

Implemented:

- Parse inline `text/calendar` MIME parts and `.ics` attachment payloads.
- Persist parsed invite metadata and raw ICS in a first-class SQLite table.
- Display invite cards in message views.
- List invites in CLI, TUI, and web.
- Search invite-bearing mail with `has:calendar` and aliases
  `has:invite` / `has:invites`.
- Preview and send accept/tentative/decline iMIP replies.
- Support "with comment" replies by seeding compose with an inline calendar
  reply payload.
- Backfill old stored messages after upgrading.

Still intentionally not implemented:

- CalDAV sync.
- Google Calendar or Microsoft Graph Calendar APIs.
- Standalone calendar grid views.
- Automatic event insertion into an external calendar.
- Reminder alarms or free/busy scheduling.

```bash
mxr invites list --limit 20
mxr search 'has:calendar newer_than:30d' --format ids
mxr invite reply MESSAGE_ID accept --dry-run --format json
```

## Standards Boundary

Calendar email support sits on three standards:

- RFC 5545: iCalendar object syntax.
- RFC 5546: iTIP scheduling methods such as `REQUEST`, `REPLY`, `CANCEL`,
  `PUBLISH`, and `COUNTER`.
- RFC 6047: iMIP, the email/MIME binding for iTIP.

Current `mxr` scope is iMIP over email. Provider calendar APIs are outside
the slice.

```bash
mxr invite show MESSAGE_ID --format json \
  | jq '{method: .metadata.method, uid: .metadata.uid, sequence: .metadata.sequence}'
```

## Core Types

`CalendarMetadata` is now rich enough to drive display and reply safety.
It includes:

- `method`
- `summary`
- `component_kind`
- `uid`
- `sequence`
- `recurrence_id`
- `dtstamp`
- `starts_at`
- `ends_at`
- `description`
- `location`
- `status`
- `rrule`
- `organizer`
- `attendees`
- `rsvp_requested`
- `raw_ics`
- `warnings`
- derived `viewer_partstat`
- derived `viewer_attendee_email`
- derived `is_update`

Code truth:

- `crates/core/src/types.rs`
- `crates/mail-parse/src/lib.rs`

The derived viewer fields are computed for read/list surfaces. They are
not raw wire data from the ICS payload.

```bash
mxr invites list --format json \
  | jq '.[0].metadata | {summary, viewer_partstat, viewer_attendee_email, warnings}'
```

## Parsing

`mail-parse` uses the Rust `icalendar` parser for production parsing. It
unfolds lines, reads `VEVENT`, extracts common event fields, organizer and
attendee parameters, stores raw ICS, and emits parser warnings for unsafe
or incomplete invites.

There is still a legacy fallback for minimal `METHOD:` / `SUMMARY:` line
recognition when full RFC 5545 parsing fails. Those rows carry the warning
`calendar invite could not be parsed as RFC 5545` and the send path refuses
to answer them.

Code truth:

- `crates/mail-parse/src/lib.rs`
- `crates/test-support/fixtures/standards/multipart-calendar.eml`
- `crates/mail-parse/src/snapshots/`

```bash
mxr invite show MESSAGE_ID --format json \
  | jq -r '.metadata.raw_ics'
```

## Provider Extraction

Gmail and IMAP both normalize calendar payloads into `MessageMetadata`.

Gmail:

- Walks Gmail API MIME parts.
- Detects inline `text/calendar`.
- Treats calendar-like attachments as recoverable invite sources.
- Fetches attachment bytes for attachment-only `.ics` cases so the parser
  can populate metadata.

IMAP:

- Parses raw RFC/MIME bytes through `mail-parser`.
- Detects `text/calendar` parts.
- Preserves attachment metadata from MIME name/disposition/binary fields.

Central attachment detection lives on `AttachmentMeta::is_calendar`.

```bash
mxr attachments list MESSAGE_ID
mxr invites backfill --format json
```

## Store

Calendar invites are persisted in `calendar_invites`.

The table stores:

- invite id
- account id
- source message id
- method
- UID
- recurrence id
- sequence
- summary
- start/end values
- organizer email
- current partstat cache
- RSVP requested flag
- full metadata JSON
- raw ICS
- created/updated timestamps

Indexes cover message lookup, `(account_id, uid)` correlation, and
`starts_at` ordering.

Code truth:

- `crates/store/migrations/038_calendar_invites.sql`
- `crates/store/src/calendar.rs`
- `crates/store/src/body.rs`

Important caveat: `calendar_invites.current_partstat` is a cache derived
from stored attendee data and can be misleading for future analytics if
read as "the viewer's current RSVP" in isolation. List/body surfaces
derive the viewer-specific status from account addresses at read time.

```bash
mxr invites list --format jsonl \
  | jq '{message_id, current: .metadata.viewer_partstat, attendee: .metadata.viewer_attendee_email}'
```

## IPC And CLI

Calendar invite IPC exists in `Request`:

- `GetInvite`
- `ListInvites`
- `BackfillCalendarInvites`
- `RespondInvite`
- `PrepareInviteResponse`
- `MarkInviteAnswered`

CLI commands exist:

- `mxr invites list`
- `mxr invites backfill`
- `mxr invite show <MESSAGE_ID>`
- `mxr invite reply <MESSAGE_ID> accept|tentative|decline`

Dry-run is part of the RSVP command contract.

```bash
mxr invite show MESSAGE_ID
mxr invite reply MESSAGE_ID tentative --dry-run
```

## RSVP Sending

`RespondInvite` builds a preview, refuses unsafe cases, and sends only
after dry-run is omitted. The send path:

- requires a parsed `METHOD:REQUEST`;
- requires organizer email;
- requires UID;
- rejects stale invites when a newer sequence exists;
- warns on organizer changes for the same UID;
- strictly matches exactly one current account address to an attendee;
- sends `METHOD:REPLY` through the configured send provider;
- updates local attendee `PARTSTAT` after successful send.

Parsing uses a real library. Reply ICS generation is currently a small,
constrained manual builder in `crates/daemon/src/handler/mailbox.rs`,
using escaping helpers and a fixed `METHOD:REPLY` shape. That is the code
truth; do not describe reply generation as delegated to `icalendar`
unless the implementation changes.

Send provider coverage:

- Gmail
- SMTP
- Outlook SMTP
- fake provider for tests

```bash
mxr invite reply MESSAGE_ID decline --dry-run --format json \
  | jq '{subject, attendee_email, organizer_email, ics}'
```

## TUI And Web

TUI:

- Message view renders an invite card.
- Sidebar has a Calendar invites lens.
- Lens actions accept, tentative, decline, undo-before-send, and
  "with comment" compose.

Web:

- `/invites` lists detected invites across accounts.
- Thread view renders invite cards.
- Inline actions use a short undo window before POSTing the reply.
- The "with comment" menu opens an invite-reply compose session.

Bridge routes include:

- `GET /api/v1/mail/invites?limit=...`
- `POST /api/v1/mail/actions/invite/reply`
- `POST /api/v1/mail/compose/session` with `kind: "invite_reply"`

```bash
mxr web --no-open
curl -G -H "Authorization: Bearer $MXR_TOKEN" \
  "$MXR_BASE/api/v1/mail/invites" \
  --data-urlencode 'limit=50'
```

## Search And Rules

Search supports `has:calendar`, plus aliases `has:invite` and
`has:invites`.

The search index stores calendar presence as a boolean field. It does not
yet index organizer, attendee, UID, or method as first-class searchable
calendar fields.

Rules intentionally reject `has:calendar` today. That is a safety and
implementation boundary: search can inspect invite-bearing mail, but
automated rules should not silently mutate around social scheduling
objects until that behavior is deliberately designed.

```bash
mxr search 'has:calendar newer_than:30d' --format json \
  | jq -r '.[] | "\(.date)\t\(.from)\t\(.subject)"'
```

## What Stayed Valid From The Original Plan

Still valid:

- Mail-derived invites are the right first slice.
- Full calendar sync is out of scope.
- CLI/daemon first remains the right architecture.
- Raw ICS retention matters.
- Dry-run before RSVP is mandatory.
- Identity and stale-sequence checks are product safety requirements.

No longer valid:

- "No full iCalendar parser."
- "No raw ICS retention guarantee."
- "No invite schema."
- "No RSVP action."
- "No invite-specific display in CLI/TUI/web."
- "No `has:calendar` search."

```bash
mxr invites backfill --format json
mxr invites list --format jsonl | head
```

## Remaining Risks

- Time values are stored as strings from iCalendar properties. This is
  good enough for display and correlation, but not enough for robust
  calendar math, time-zone history, or free/busy scheduling.
- Recurrence is recognized through fields like `RRULE` and
  `RECURRENCE-ID`, not expanded into instances.
- `CANCEL`, `PUBLISH`, and `COUNTER` are displayed, but only
  `METHOD:REQUEST` is answerable.
- The manual reply builder must stay tightly tested because escaping,
  folded-line shape, and attendee parameters are easy to regress.
- The local `current_partstat` column should not become an analytics
  source without first making viewer identity explicit.

```bash
mxr invite show MESSAGE_ID --format json \
  | jq '.metadata | {method, rrule, recurrence_id, warnings}'
```
