# Security

Calendar invitations are executable-looking social objects delivered through email. Treat them as untrusted input.

## Threats

| Threat | Example | Required behavior |
|---|---|---|
| Spoofed organizer | Attacker sends `CANCEL` for a real meeting. | Do not auto-apply. Show trust warning. |
| Wrong attendee | Invite was forwarded; current account is not in `ATTENDEE`. | Disable RSVP by default. |
| Stale update | Old `SEQUENCE` arrives after newer invite. | Mark stale. Do not offer response without warning. |
| Organizer replacement | New invite has same UID but different organizer. | Warn; require confirmation if ever mutating state. |
| Calendar flooding | Spam sends many invites. | No auto-add. User action required. |
| Malicious URLs | Description/location contain URLs. | Render as text/link safely; no auto-fetch. |
| Remote time-zone URL | `TZURL` points remote. | Do not fetch automatically. |
| Private data leak | Reply reveals attendance. | Dry-run recipient and content. |

## Trust Signals

Use existing email auth metadata when available:

- Authentication-Results
- DKIM/SPF/DMARC summaries if parsed
- sender alignment with organizer domain/address

Do not require perfect auth for display. Require stronger checks for automatic actions.

## Identity Matching

Attendee matching should normalize:

- `mailto:` URI
- case-insensitive email local/domain comparison where appropriate
- configured account aliases

Ambiguous matches should block one-click RSVP and ask the user to choose.

## Failure Mode

Fail closed for mutation, open for reading:

- If parsing fails: show generic attachment/invite warning.
- If trust is unclear: display but disable action or require explicit confirmation.
- If send fails: do not update local response state.
- If activity logging fails: warn only, per activity-log invariant.

## Privacy

Calendar invite rows are local-only. Do not sync them. Do not add telemetry. Do not store credentials, tokens, or full unrelated mail bodies in calendar-specific context fields.

