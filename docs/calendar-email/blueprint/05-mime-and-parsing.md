# MIME And Parsing

Calendar invite detection starts in provider parsing but should normalize into a shared mail/calendar parser.

## Input Shapes

Common real-world shapes:

```text
multipart/alternative
  text/plain
  text/html
  text/calendar; method=REQUEST
```

```text
multipart/mixed
  text/plain
  application/ics or text/calendar attachment filename=invite.ics
```

```text
text/calendar; method=CANCEL
```

`mxr` must handle inline `text/calendar` and `.ics` attachments. Filename alone is not enough; MIME type matters more.

## Detection Rules

Detect calendar content when:

- MIME type is `text/calendar`.
- Filename ends with `.ics` and content can be fetched or decoded.
- MIME type is known ICS-ish from providers, if encountered, but prefer standards.

For iMIP actionability:

- MIME `method` param should exist.
- Calendar `METHOD` property should exist.
- MIME method and calendar method should match case-insensitively.
- Component should be `VEVENT` for first implementation.

If those checks fail, show read-only calendar attachment/import information, not RSVP actions.

## Provider Responsibilities

Gmail and IMAP adapters should not implement scheduling logic. They should:

- decode MIME/body part bytes
- identify candidate calendar parts
- pass text to shared parsing helpers
- preserve raw ICS text
- populate provider-agnostic `MessageBody` / invite data

Daemon must not import provider-specific Gmail/IMAP clients for calendar behavior.

## Parser Responsibilities

Shared parser should:

- unfold lines
- parse `VCALENDAR`
- select supported components
- extract method
- extract event identity fields
- extract title/time/location/description
- extract organizer
- extract attendees and params
- preserve raw ICS
- emit warnings, not fatal errors, for unsupported features

Unsupported or dangerous features should degrade to read-only display.

## Multiple Events

A single `VCALENDAR` can contain more than one component. First implementation should support:

- one `VEVENT` as actionable
- multiple `VEVENT`s as read-only or show "multiple events; action unavailable"
- `VTIMEZONE` as supporting data, not an event

## Time Zones

Time display must be honest:

- UTC `...Z`: display converted to user/local configured zone.
- `TZID`: use matching `VTIMEZONE` or library resolution.
- floating local time: mark as floating/local, do not pretend UTC.
- date-only: display as all-day.

No remote `TZURL` fetch during parsing.

## Attachments

Calendar parts may also be exposed as attachments. The same raw ICS should not create duplicate invite rows. Dedupe by:

- message id
- calendar `UID`
- recurrence id
- method
- sequence

