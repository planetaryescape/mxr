---
title: Calendar email implementation synthesis
kind: synthesis
tags:
  - mxr
  - calendar-email
  - code-truth
  - synthesis
---

# Calendar Email Implementation Synthesis

The useful insight from the calendar-email work is not "parse ICS." It is that
email clients need to make embedded protocol objects actionable without
swallowing the whole adjacent product.

`mxr` now handles the email-shaped part of scheduling: receive, parse, show,
search, preview, send, and record local RSVP state. The implementation remains
local-first and provider-agnostic.

## What Changed

The original blueprint was directionally right but stale in its current-state
section. Code now confirms:

- `icalendar` is used for parsing instead of line-only scanning.
- `CalendarMetadata` contains rich event, attendee, organizer, raw ICS, warning,
  viewer status, and update fields.
- `calendar_invites` persists first-class invite rows.
- CLI, IPC, TUI, web, and bridge surfaces exist.
- `has:calendar` search exists.
- RSVP send exists for accept/tentative/decline.
- Backfill can recover already-stored messages and attachment-only invites.

```bash
mxr invites backfill --format json
mxr invites list --limit 20
```

What you get: restored invite rows and a current list from the local database.

## What Stayed True

The durable parts of the original research still hold:

- Email invites should be handled before full calendar sync.
- Raw ICS matters for auditability.
- RSVP must be dry-runnable.
- Attendee identity matching is safety-critical.
- Stale update and organizer replacement checks are not polish; they are trust
  boundaries.

```bash
mxr invite reply MESSAGE_ID accept --dry-run --format json \
  | jq '{action, attendee_email, organizer_email, warnings}'
```

What you get: the safety-critical facts before the reply is sent.

## New Learnings

Attachment-only invites are common enough to deserve a backfill path. Inline
`text/calendar` is not the only real-world shape.

Viewer status should be derived with account identity in mind. A stored
`PARTSTAT` column can be convenient, but it is not automatically "my RSVP"
unless the viewer identity is part of the key.

Reply generation and parsing have different risk profiles. Parsing broad input
belongs to a library. Generating one narrow `METHOD:REPLY` shape can be a small
audited builder, as long as tests stay close and docs do not overclaim.

Rules automation should lag behind search. It is fine to let users find
calendar-bearing mail with `has:calendar`; it is a separate decision to let
rules automate around invites.

## Current Boundary

This is still valid: no CalDAV, no provider calendar APIs, no calendar grid, no
automatic external event mutation.

That boundary is a feature. It lets `mxr` improve the email experience without
accidentally owning a user's calendar.

See:

- [Current state](../blueprint/02-current-state.md)
- [Decisions](../blueprint/09-decisions.md)
- [[Email invites are mail state before calendar state]]
- [[Dry-run social mutations]]
- [[Capability slices before platform expansion]]
