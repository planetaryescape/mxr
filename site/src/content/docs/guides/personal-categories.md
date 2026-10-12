---
title: Personal categories
description: Group mail locally, review category rules, and preserve manual corrections.
---

```sh
mxr categories create Travel
mxr categories --format json
mxr categories --format json show <category-id>
```

Categories group mail across accounts while keeping provider labels, folders
and workflow placement unchanged. A category ID stays the same after rename.

In the web mailbox, open **Labels**, then **Categories**. Select categories,
review the assignment preview, then apply. **Manage** opens rename and deletion
controls. Undo is available from the mailbox.

In the TUI, press `Ctrl-P` and choose `Categories` or `Assign Category`. Press
`Space` to stage assignments, `Enter` to preview, then `Enter` to apply. Press
`w` in a category view or assignment picker for Why. With multiple messages
selected, the TUI explains the first selected message.

## Category rules keep manual corrections

In web Rules, create a rule with a sender condition and choose Travel in the
local category selector. Review its complete preview, then save and apply.
To remove a category automatically, use `remove-category:Travel` in the action
field. TUI Rules accepts the same action text; `Ctrl-S` or `Ctrl-D` opens a
preview and `A` applies it.

Manual inclusion or exclusion wins over every rule. Lower numeric priority
wins conflicting rules for the same category; equal priorities use the stable
rule ID. Why shows the manual decision, winning rule and other sources.
Disabling, editing or deleting a rule reconciles only its own contributions,
so another rule or manual inclusion can keep mail in a category. Deleting a
category is blocked by every referencing rule, including disabled rules.

## Preview and apply the same complete form

Save this form as `airline-rule.json`, replacing the account ID:

```json
{
  "id": null,
  "account_id": "<account-id>",
  "name": "Airline mail",
  "condition": "from:airline@example.com",
  "action": "add-category:Travel",
  "priority": 10,
  "enabled": true
}
```

```sh
mxr rules --format json category-preview --form airline-rule.json
mxr rules --format json category-apply --form airline-rule.json --preview-token <token>
mxr categories --format json why <message-id> <category-id>
```

In TUI Rules, use `add-category:Travel` or `remove-category:Travel` in the action
field. `Ctrl-S` or `Ctrl-D` opens the preview. Use `PageUp` and `PageDown` to review
each effect, then `A` to apply. The account field includes **All accounts** for
category rules.

The preview includes old contributions and current matches without a hidden
selection cap. Changes to mail, rules, categories, manual decisions or sources
require a new preview. Undo restores the previous manual decision while keeping
rule sources. MCP exposes `mxr_category_rule` and `mxr_category_why`.

Use `account_id: null` only to select All accounts explicitly. A restricted
agent profile must keep an allowed selected account; saving global behavior
requires access to every stored and configured account.
