---
title: Review later evidence
description: Review quoted email evidence before completing a task or changing its due date.
---

Review the source before accepting a task change. When a later email appears to finish a to-do or change its deadline, mxr shows the exact source and a short quote as an offer. The task stays unchanged until you review and accept it. You can dismiss the offer or undo an accepted change while its saved task snapshot is still current.

Local rules catch clear completion confirmations. A later email that proposes a new date needs the optional Commitments model, which runs only when an LLM is enabled and configured. Neither path completes or retimes a task automatically; each match remains an offer for you to review. Model checks run in small background batches, so offers may appear after sync finishes. Pending checks survive a daemon restart; turning the model off pauses them until it is enabled again.

## Review a quoted message

```bash
mxr todo --format json
mxr todo resolve TODO_ID SUGGESTION_ID preview
```

Use the `resolution.id` from the task's JSON as `SUGGESTION_ID`. Read the quoted sentence and open the source email before deciding. A completion offer proposes to tick off the task; a date offer proposes the new deadline and words from the later email.

## Accept or dismiss

```bash
# Preview the selected completion or date change without applying it.
mxr todo resolve TODO_ID SUGGESTION_ID accept --dry-run

# Apply only after reviewing the source and preview.
mxr todo resolve TODO_ID SUGGESTION_ID accept

# Keep the task unchanged and dismiss this offer.
mxr todo resolve TODO_ID SUGGESTION_ID dismiss
```

Accept updates only the named task. Dismissal leaves its fields alone and retains the decision so the same evidence does not keep returning.

## Use the To do screen

```bash
mxr
```

Open To do with `g x`, select the row marked **Looks resolved**, and press `v` to preview it. While the preview is open, press `o` to open that exact source email. In the preview, Enter accepts, `x` dismisses, and Esc cancels. After an accepted change, `u` undoes it if the task has not changed since acceptance.

## Review it on the web

```bash
mxr web
```

In To do, use **Source email** to inspect the cited message, then **Review** to see the proposed change. Choose **Accept and complete** or **Accept new date**, or dismiss the offer. The accepted row has an **Undo** action while the saved task snapshot remains current.

## Use the MCP tool

```bash
mxr todo --format json
```

Call `mxr_todo_resolve` with the task and suggestion IDs. `action` is `accept`, `dismiss` or `undo`; omit `confirm` or set it to `false` to request that action's preview, and set `confirm` to `true` only after reviewing it. The [`MCP reference`](/reference/mcp/#todo-resolution) lists the tool alongside the other To do calls.

## When an offer has not appeared

```bash
mxr llm status --format json
mxr todo why TODO_ID --format json
```

Check that the task is still open and that the later email concerns the same
task. A quote must support the proposed completion or replacement date;
receiving another email is not enough.

Clear completion rules can work without a model. Semantic matches and new
dates need an enabled, configured Commitments model. Model checks share a
background batch limit of four checks and an eight-second deadline, so a
finished sync does not mean every candidate has been checked. A provider's
empty-response fallback may retry within a check; the limit is not four HTTP
requests across all model features.

Failed checks retry with increasing delays, up to five attempts for unchanged
input. Turning off the model pauses pending work. Pending checks and attempt
history survive restarts and [backups](/guides/backup-recovery/); a restored
workspace keeps model work paused until you activate recovery and reconfigure
its processors. You can always complete or edit the task yourself.

## When a review becomes stale

```bash
mxr todo --format json
mxr todo resolve TODO_ID CURRENT_SUGGESTION_ID preview
```

Refresh the task and use its current `resolution.id`. Preview, acceptance and
Undo check that the task still matches the saved offer or accepted state. If
you edited the task in another client, an older offer cannot overwrite that
edit. The cited email and quote must also remain available. If no current
offer appears, inspect the source and complete or edit the task yourself.

## See also

- [Act on To dos](/guides/todo/)
- [To do commands](/reference/cli/todo/)
- [MCP reference](/reference/mcp/#todo-resolution)
