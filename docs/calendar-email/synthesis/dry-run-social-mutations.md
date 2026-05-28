---
title: Dry-run social mutations
kind: concept
tags:
  - safety
  - product-design
  - email
  - automation
---

# Dry-Run Social Mutations

A social mutation is an action that changes what another person believes you
did: sending a reply, accepting a meeting, declining an invitation, forwarding a
message, or marking a thread in a shared system.

The core design rule is simple: preview before commitment. The preview must
show the exact action, target, identity, and payload shape. A vague
confirmation dialog is not enough.

## Good Preview Shape

A useful dry-run answers:

- Who am I acting as?
- Who receives the action?
- What exact content or protocol payload will be sent?
- What local state will change after success?
- What warnings or refusal reasons exist?

```bash
mxr invite reply MESSAGE_ID decline --dry-run --format json \
  | jq '{attendee_email, organizer_email, subject, warnings, ics}'
```

What you get: the sender identity, organizer target, human subject, warnings,
and raw iMIP reply before any email is sent.

## Why This Generalizes

Dry-run is useful anywhere a local-first tool becomes agent-operable. It lets a
human or agent compose workflows without guessing hidden side effects. It also
makes tests and docs better because the preview is a stable, inspectable
contract.

## Local Application

`mxr` refuses unsafe calendar replies, previews `METHOD:REPLY`, and only sends
when the user omits `--dry-run` or an undo-before-send window elapses in the
interactive clients.

See:

- [RSVP and sending](../blueprint/06-rsvp-and-sending.md)
- [Security](../blueprint/07-security.md)
- [[Email invites are mail state before calendar state]]
