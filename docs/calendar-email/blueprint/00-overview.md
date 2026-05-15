# Overview

`mxr` should handle calendar invitations that arrive through email. That does not mean `mxr` should immediately become a full calendar client.

The first product surface is mail-derived scheduling:

- identify invite/update/cancel emails
- show the event in a compact, trustworthy way
- explain whether the current user is an attendee
- allow accept/tentative/decline when safe
- send a standards-shaped iMIP reply through the existing outbound mail path

## Product Discipline

This solves a real email-client gap. Users already receive `.ics` or `text/calendar` invites. Other email clients make those actionable. `mxr` currently reduces them to generic body text or attachments.

Do not expand this into "everything calendars" without a separate validated problem. Full calendar sync has a much larger maintenance cost: CalDAV auth, calendars, server scheduling, conflict resolution, recurring instances, reminders, time-zone history, and user preference surfaces.

## Topic Scoring

Scores: 5 high, 1 low.

| Topic | User value | Complexity | Risk | First-slice priority | Notes |
|---|---:|---:|---:|---:|---|
| Detect `text/calendar` invite | 5 | 2 | 2 | 5 | Required foundation. |
| Parse core VEVENT fields | 5 | 3 | 3 | 5 | Use a library; no more line scanning. |
| Display invite summary | 5 | 2 | 2 | 5 | CLI first, then TUI/web. |
| Preserve raw ICS | 5 | 3 | 3 | 5 | Needed for correct replies/debugging. |
| Accept/tentative/decline reply | 5 | 4 | 4 | 4 | Must be dry-runnable. |
| Identity/trust checks | 5 | 4 | 5 | 5 | Prevent spoofed or wrong-attendee replies. |
| Cancellation/update handling | 4 | 4 | 4 | 3 | Display first; mutation later. |
| Recurrence expansion | 3 | 5 | 4 | 2 | Need recognition before expansion. |
| CalDAV sync | 2 | 5 | 5 | 1 | Not needed for email RSVP. |
| Calendar grid UI | 2 | 5 | 3 | 1 | Separate product. |

## Definitions

- iCalendar: the `.ics` data format.
- iTIP: scheduling semantics layered on iCalendar.
- iMIP: iTIP transported over email using MIME `text/calendar`.
- Invite: a mail-derived calendar scheduling object shown to the user.
- RSVP: a response sent by the attendee, usually `METHOD:REPLY`.

## First Useful Product

The smallest complete user journey:

1. User opens a message containing an invite.
2. `mxr` shows a calendar card with title, organizer, time, location, attendee status, and method.
3. User runs or selects accept/tentative/decline.
4. `mxr` previews the exact reply with `--dry-run`.
5. User sends.
6. `mxr` records local response state and logs activity locally.

## Non-Goals For Initial Work

- No automatic event insertion into an external calendar.
- No silent acceptance.
- No remote `TZURL` fetching.
- No provider-specific Gmail Calendar or Outlook Calendar APIs.
- No daemon import of provider-specific clients.
- No custom iCalendar parser beyond glue around a library.

