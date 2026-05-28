# Surfaces

Calendar invite support must be CLI-first and daemon-backed.

Current code truth lives in [Current State](02-current-state.md). This file
preserves the design surface; the shipped protocol names are
`GetInvite`, `ListInvites`, `BackfillCalendarInvites`, `RespondInvite`,
`PrepareInviteResponse`, and `MarkInviteAnswered`.

## IPC

Likely requests:

```rust
Request::ListInvites { filter, limit }
Request::GetInvite { message_id }
Request::RespondInvite { message_id, action, dry_run }
```

Likely responses:

```rust
ResponseData::Invite(CalendarInvite)
ResponseData::InviteList(Vec<CalendarInviteSummary>)
ResponseData::InviteResponsePreview(CalendarInviteResponsePreview)
ResponseData::InviteResponseSent(CalendarInviteResponseResult)
```

Classification:

- mail-derived invite display/respond: `core-mail`
- future local calendar/product scheduling: `mxr-platform`
- diagnostics/repair: `admin-maintenance`

## CLI

Initial CLI:

```text
mxr invite show <MESSAGE_ID> [--json]
mxr invite reply <MESSAGE_ID> accept|tentative|decline --dry-run
mxr invite reply <MESSAGE_ID> accept|tentative|decline
mxr invites list [--json|--jsonl] [--account ACCOUNT] [--from DATE] [--to DATE]
```

Output must be pipeable.

Human `show` should include:

- summary
- organizer
- time
- location
- current user attendee
- current status
- method
- UID / sequence
- warnings
- available actions

## TUI

Message view should show an invite block before attachments:

- title
- date/time
- organizer
- location
- status/action row
- warning line if stale/untrusted/not-an-attendee

Actions:

- accept
- tentative
- decline
- open invite details
- copy/export raw ICS

Do not create a full calendar screen in initial work.

## Web

Thread view should render the same invite card. The web client must preserve body metadata or fetch invite data from an invite endpoint.

Bridge routes should remain thin IPC wrappers.

## Search

Add:

- `has:calendar`
- maybe later `calendar:request`, `calendar:cancel`, `organizer:`, `attendee:`

First implementation can post-filter SQLite data. Later indexing can promote calendar fields into search.

## Activity

Record user action locally:

- viewed invite details
- sent invite response
- dry-run response previewed if useful

Use daemon activity recorder only. Never direct SQL writes outside `crates/daemon/src/activity/`.
