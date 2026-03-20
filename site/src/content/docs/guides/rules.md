---
title: Rules
description: Deterministic inbox automation with dry runs, history, and shell hooks.
---

## Philosophy

Rules are data first. `mxr` lets you inspect, dry-run, enable, disable, and audit them before trusting them on live sync traffic.

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

- rule list
- details panel
- history panel
- dry-run panel
- form-driven create and edit flows

Open it from:

- `Ctrl-p` then `Open Rules Page`

Common actions:

- `n`: new rule
- `E`: edit rule
- `e`: enable or disable
- `D`: dry-run
- `H`: history
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

1. Create a rule with `mxr rules add`.
2. Validate it with `mxr rules validate`.
3. Check impact with `mxr rules dry-run`.
4. Let sync execute it automatically.
5. Inspect `mxr rules history` if anything looks off.
