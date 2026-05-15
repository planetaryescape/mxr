# RSVP And Sending

Accept/tentative/decline is not a normal email reply. It is an iMIP `METHOD:REPLY` message.

## Actions

Initial actions:

- accept -> `PARTSTAT=ACCEPTED`
- tentative -> `PARTSTAT=TENTATIVE`
- decline -> `PARTSTAT=DECLINED`

Do not implement delegate/counter/propose-new-time in the first slice.

## Preconditions

Enable RSVP only when:

- invite method is `REQUEST`
- component is `VEVENT`
- `UID` is present
- organizer is present
- one attendee matches a configured account identity
- target attendee can be selected unambiguously
- invite is not stale relative to local known sequence
- parser produced no fatal warnings

If the user is not listed as attendee, show why action is disabled.

## Generated Reply

The outbound iCalendar object should include:

- `BEGIN:VCALENDAR`
- `PRODID`
- `VERSION:2.0`
- `METHOD:REPLY`
- `BEGIN:VEVENT`
- `UID`
- `SEQUENCE` if present
- `RECURRENCE-ID` if replying to one instance
- `DTSTAMP`
- `ORGANIZER`
- target `ATTENDEE` with updated `PARTSTAT`
- optional `SUMMARY`
- optional `REQUEST-STATUS`
- `END:VEVENT`
- `END:VCALENDAR`

The email should contain:

- recipient: organizer mailto address
- subject: response-oriented subject, e.g. `Accepted: <summary>`
- MIME `text/calendar; method=REPLY; charset=UTF-8; component=vevent`
- optionally a plain text body explaining the response

## Dry Run

`--dry-run` must show:

- action
- attendee identity used
- organizer recipient
- event summary/time
- UID/sequence/recurrence id
- generated ICS
- generated email headers/body summary
- warnings

Dry-run must exercise the same selection and generation path as real send.

## Local State

After successful send:

- record local response action/status
- store generated reply metadata if useful
- activity log via daemon recorder
- do not pretend remote organizer accepted or processed the reply

## Provider APIs

Do not use Gmail Calendar API or Outlook Calendar API in initial work. Email RSVP over iMIP is provider-agnostic and fits `mxr` architecture.

If provider-specific scheduling APIs are ever added, they belong behind provider traits and must not leak into daemon handler logic.

