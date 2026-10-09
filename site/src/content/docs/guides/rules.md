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

## Sort mail into Messages, Updates or Reading

```bash
mxr accounts --format json
cat > sorting-rule.json <<'JSON'
{
  "id": null,
  "account_id": "ACCOUNT_UUID",
  "name": "Weekly reading",
  "condition": "from:editor@example.com subject:weekly",
  "action": "treatment:reading",
  "priority": 100,
  "enabled": true
}
JSON
mxr rules --format json treatment-preview --form sorting-rule.json
mxr rules --format json treatment-apply --form sorting-rule.json --preview-token PREVIEW_TOKEN
```

Replace `ACCOUNT_UUID` with the account ID returned by `mxr accounts` and
`PREVIEW_TOKEN` with the preview token. `--form -` reads JSON from stdin.
To preview a saved rule, pass its name or ID instead of `--form`; apply it with
`mxr rules treatment-apply RULE PREVIEW_TOKEN`. The preview returns a token and
up to 200 recent messages, each with its current place, proposed place and
reason. `complete: false` means older mail was excluded. Apply requires the
unchanged token; if mail, rules or personal choices changed, preview again.
Tokens remain valid for up to ten minutes and can be used once.

Sorting accepts header conditions such as sender, recipient, subject, labels,
date and flags. Body and link-density conditions are rejected. The preview
includes inbound inbox mail; archived, trashed and outbound mail is excluded.
Lower numeric priority wins. Equal priorities use rule ID order, so the winner
stays stable. Personal sender preferences and message corrections keep their
existing precedence and appear in the preview when they prevent a change.

Saving a rule sorts future arrivals. Editing, disabling or deleting it leaves
historical placements in place; apply a new preview to change the selected
existing mail. Ordinary sync replay preserves those placements. The explanation
names the source rule and identifies deleted, disabled or earlier versions.
Account scope covers every action in the rule. Apply changes only sorting;
label, archive and other existing actions retain their ordinary sync behavior.

In the TUI, choose `n` or `E`, set the account, condition and
`treatment:messages`, `treatment:updates` or `treatment:reading`. `Space` cycles
accounts in the Account field. `Ctrl-d` previews the draft; `D` previews a saved
rule. Press `A` on the preview to save and apply it. `Ctrl-s` saves for future
arrivals.

In the web Rules editor, select an account and **Sort into Messages**,
**Sort into Updates** or **Sort into Reading**. Review the before/after places
and reasons, then choose **Apply to…** and confirm. **Save rule** affects future
arrivals. The MCP tool `mxr_sorting_rule` accepts the same draft fields and a
`messages`, `updates` or `reading` treatment. Omit `preview_token` to preview;
pass the returned token to apply. `mxr_rule_form` reads an existing rule before
editing. The sorting tool preserves its other actions. Scoped MCP profiles can
read and edit only rules within their allowed accounts.

The bridge exposes `POST /api/v1/platform/rules/treatment` with a `form` object
and optional `preview_token`. It uses the bridge's bearer-token authorization.
The response contains `token`, `applied`, `rule_id` and `result`, with the same
selection and explanations as the CLI and TUI.

Prefer a forward fix if sorting needs repair. Before downgrading to a binary
without sorting support, remove or migrate every account-scoped rule and
remove all `set_treatment` actions using the current binary. Disabling rules
is insufficient: older binaries deserialize every stored rule before filtering
by enabled state, and cannot read the new action. They also ignore account
scope, so remaining label, archive or shell actions could run across accounts.
The additive storage migration can remain; historical sorting is displayed
only by binaries supporting it.

## Supported actions

- `archive`
- `trash`
- `star`
- `mark-read`
- `mark-unread`
- `add-label:NAME`
- `remove-label:NAME`
- `shell:COMMAND`
- `treatment:messages`, `treatment:updates`, `treatment:reading`

## Body-derived conditions

Rules can match on classifications computed from the message body during sync:

- `has:link` — the body contains at least one external link (excluding
  trackers / unsubscribe / list-management URLs).
- `has:link-heavy` — link-dense newsletter-shaped mail. Useful for "auto-archive
  link-heavy mail from unknown senders".
- `has:link-none` — body has no external links at all.

To populate the link classification on messages synced before mxr 0.6, run
`mxr doctor --recompute-link-counts` once. New mail is classified automatically
during sync.

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
