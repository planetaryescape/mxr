---
title: Email invites are mail state before calendar state
kind: concept
tags:
  - calendar-email
  - email
  - product-boundaries
  - interoperability
---

# Email Invites Are Mail State Before Calendar State

A received invite is first a message with a scheduling payload. Treating it as
mail state before calendar state keeps the first product slice small: parse the
payload, show the user what it means, and let the user answer safely.

The trap is to see `.ics` and immediately build a calendar product. That jumps
from "make this email understandable" to calendar sync, reminders, free/busy,
time-zone history, recurrence expansion, CalDAV, provider APIs, conflict
resolution, and UI for event creation. Those are real products, not incidental
features.

## Reusable Rule

When a protocol object arrives inside another workflow, solve the receiving
workflow first.

For calendar email, that means:

- detect the scheduling object in the message;
- preserve the original payload;
- display the object in terms the reader can act on;
- answer through the transport it arrived on;
- defer external calendar state until there is a validated calendar problem.

```bash
mxr search 'has:calendar newer_than:30d' --format ids
mxr invite show MESSAGE_ID
```

What you get: mail-derived scheduling context without taking ownership of the
user's whole calendar.

## Local Application

`mxr` implements email-derived invite handling over iMIP. It does not auto-add
events to a remote calendar and does not call Google Calendar, Microsoft Graph
Calendar, or CalDAV.

See:

- [Current state](../blueprint/02-current-state.md)
- [Overview](../blueprint/00-overview.md)
- [[Capability slices before platform expansion]]
