# Standards

Calendar email handling is governed by three layers.

## RFC 5545: iCalendar

Source: <https://www.rfc-editor.org/rfc/rfc5545>

RFC 5545 defines the data format. It is independent of transport and calendar service.

Core objects and fields `mxr` must care about:

| Field | Meaning |
|---|---|
| `VCALENDAR` | Container for calendar components. |
| `VEVENT` | Event component. Initial scope should focus here. |
| `VTODO` | Task component. Read-only later. |
| `VTIMEZONE` | Time-zone definition used by `TZID`. Preserve even if not fully interpreted. |
| `METHOD` | Calendar-level scheduling method when used with iTIP/iMIP. |
| `UID` | Stable event or series identity. Correlation key. |
| `SEQUENCE` | Organizer-controlled revision number. Used to reject stale updates. |
| `DTSTAMP` | Timestamp for sequencing/tie-breaks and generated replies. |
| `DTSTART` / `DTEND` / `DURATION` | Event time. |
| `SUMMARY` | Event title. |
| `DESCRIPTION` | Event description/body. |
| `LOCATION` | Location string. |
| `ORGANIZER` | Calendar user address of organizer, usually `mailto:`. |
| `ATTENDEE` | Calendar user address plus params such as `PARTSTAT`, `RSVP`, `ROLE`, `CN`. |
| `PARTSTAT` | Attendee participation status. |
| `RRULE`, `RDATE`, `EXDATE` | Recurrence set. |
| `RECURRENCE-ID` | Specific instance identity in a recurring series. |
| `STATUS` | Event status such as confirmed/cancelled. |

Parsing must support content-line unfolding. A folded line begins with space or tab and continues the prior logical line. The current `mxr` line scan does not handle this.

## RFC 5546: iTIP

Source: <https://www.rfc-editor.org/rfc/rfc5546>

iTIP defines scheduling behavior. Key methods:

| Method | Meaning | Initial handling |
|---|---|---|
| `REQUEST` | Organizer invites or updates attendees. | Display + RSVP. |
| `REPLY` | Attendee response to organizer. | Display; generate outbound. |
| `CANCEL` | Organizer cancels event or instance. | Display with warning; later local state. |
| `PUBLISH` | Non-interactive publication. | Display only. |
| `ADD` | Add instance to existing event. | Display only initially. |
| `REFRESH` | Attendee asks organizer for latest copy. | Display only initially. |
| `COUNTER` | Attendee proposes a change. | Display only initially. |
| `DECLINECOUNTER` | Organizer declines counter. | Display only initially. |

Important semantics:

- Organizer owns the event.
- Attendees do not mutate the master event directly.
- Accept/tentative/decline is a `REPLY` with the attendee `PARTSTAT` changed.
- Replies must preserve `UID`.
- Replies should refer to the relevant `SEQUENCE`.
- Replies for one recurrence instance must include `RECURRENCE-ID`.
- A `CANCEL` without `RECURRENCE-ID` cancels the whole event or series.
- A `CANCEL` with `RECURRENCE-ID` cancels that instance.

Common `PARTSTAT` values for events:

- `NEEDS-ACTION`
- `ACCEPTED`
- `DECLINED`
- `TENTATIVE`
- `DELEGATED`

## RFC 6047: iMIP

Source: <https://www.rfc-editor.org/rfc/rfc6047>

iMIP maps iTIP to email. It uses MIME `text/calendar`.

The key MIME shape:

```text
Content-Type: text/calendar; method=REQUEST; charset=UTF-8; component=vevent
```

Rules and implications:

- A calendar invite is not just an `.ics` filename. It can be an inline `text/calendar` MIME body part.
- The MIME `method` parameter must match the iCalendar `METHOD` property.
- Multiple calendar objects with different methods should be separate MIME entities.
- `ORGANIZER` and `ATTENDEE` must be discovered from the calendar body, not from email `From`, `Sender`, or `Reply-To`.
- `ORGANIZER` and `ATTENDEE` in iMIP should be `mailto:` URIs.
- Email forwarding means the RFC 5322 sender may not be the organizer.

## Related Standards

| Standard | Relevance |
|---|---|
| RFC 2045-2049 | MIME structure, content transfer encoding, content type params. Already handled mostly by mail-parser/Gmail JSON. |
| RFC 5322 | Email headers. Not enough to determine calendar roles. |
| RFC 6638 | CalDAV scheduling. Out of initial scope unless calendar sync is added. |
| RFC 6868 | Parameter value encoding updates for iCalendar. Library should handle if possible. |
| RFC 7986 | New iCalendar properties. Nice to preserve, not first-slice critical. |

## Sources

- RFC 5545: <https://www.rfc-editor.org/rfc/rfc5545>
- RFC 5546: <https://www.rfc-editor.org/rfc/rfc5546>
- RFC 6047: <https://www.rfc-editor.org/rfc/rfc6047>
- CalConnect iMIP guide: <https://devguide.calconnect.org/iMIP/iMIP-Introduction/>

