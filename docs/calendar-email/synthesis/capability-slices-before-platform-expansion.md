---
title: Capability slices before platform expansion
kind: concept
tags:
  - product-strategy
  - architecture
  - scope-control
---

# Capability Slices Before Platform Expansion

A capability slice solves one real user job across the existing architecture.
A platform expansion adds a new domain, new ownership model, and new lifetime
of obligations.

Calendar email is a capability slice. A full calendar is a platform expansion.

## Decision Test

Ask four questions before expanding:

- Can the user job be solved inside the current source of truth?
- Can the action use an existing transport?
- Can failure be explained without inventing a new mental model?
- Does the feature still work if the larger platform never arrives?

If the answer is yes, ship the slice and document the boundary.

```bash
mxr invites list --format jsonl \
  | jq -r 'select(.metadata.viewer_partstat == "needs_action") | .message_id'
```

What you get: a useful scheduling workflow using only synced mail and local
state.

## Local Application

The `mxr` slice stays inside the daemon, store, protocol, CLI, TUI, and web
surfaces that already exist. It adds one mail-derived table and one action
family. It does not require a calendar account model.

See:

- [Current state](../blueprint/02-current-state.md)
- [Data model](../blueprint/04-data-model.md)
- [[Dry-run social mutations]]
