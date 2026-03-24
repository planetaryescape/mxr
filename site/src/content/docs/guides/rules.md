---
title: Rules
description: Deterministic inbox automation with dry runs, history, and shell hooks.
---

## Philosophy

Rules are data first. mxr lets you inspect, dry-run, enable, disable, and audit them before trusting them on live sync traffic.

## CLI

```bash
mxr rules
mxr rules show RULE_ID
mxr rules
mxr rules add "Archive newsletters" --when "label:newsletters unread" --then archive
mxr rules edit RULE_ID --then mark-read --disable
mxr rules validate --when "from:billing@example.com" --then "add-label:finance"
mxr rules enable RULE_ID
mxr rules disable RULE_ID
mxr rules delete RULE_ID
mxr rules dry-run RULE_ID
mxr rules dry-run --all
mxr rules history
```

## TUI

The Rules page gives you:

- Rule list on the left
- Guided workspace on the right
- Overview, History, Dry Run, and Edit states
- Multiline condition/action editing
- Starter examples and validation help in the form flow

Open it from:

- `3`
- `Ctrl-p` then `Open Rules Page`

Common actions:

- `n`: new rule
- `E`: edit rule
- `e`: enable or disable
- `D`: dry-run
- `H`: history
- `Enter`: refresh overview for the selected rule
- `Ctrl-s`: save the current form
- `#`: delete

## Supported actions

- `archive`
- `trash`
- `star`
- `mark-read`
- `mark-unread`
- `add-label:NAME`
- `remove-label:NAME`
- `shell:COMMAND`

## Runtime behavior

- Rules run after sync writes messages locally.
- Matching is deterministic and priority ordered.
- Execution history is stored in SQLite.
- Shell hooks are opt-in escape hatches, not the foundation.
- Sync-time execution and dry-run share the same rule model.

## Recommended workflow

1. Create a rule with `mxr rules add` or `n` in the TUI.
2. Validate it with `mxr rules validate`.
3. Dry-run it before save or enable.
4. Let sync execute it automatically.
5. Inspect `mxr rules history` or the Rules history pane if anything looks off.
