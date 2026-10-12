# Personal categories

Personal categories are local labels for organizing mail across accounts. They
do not create provider labels, move mail between folders, or change an email's
workflow section. Category IDs stay the same when a name changes. Removing an
account removes its message memberships while leaving the profile's category
catalog intact.

Create and rename categories with `mxr categories create <name>` and
`mxr categories rename <category-id> <new-name>`. Names are trimmed, normalized
with Unicode NFKC and full case folding for uniqueness, and may contain ordinary
Unicode text. Blank names and control characters are rejected.

List categories with `mxr categories --format json`. To see a category's mail,
use `mxr categories --format jsonl show <category-id>`. Both commands accept
`--account <account>` to limit results to one account. A scoped agent profile
must name an allowed account; its category list includes only categories with
memberships there, and counts are computed within that account. A profile that
covers every stored and configured account can read across accounts and retains
the full catalog, including categories with no mail in a selected account.

Use `mxr categories create <name> --dry-run` and
`mxr categories rename <category-id> <new-name> --dry-run` to preview normalized
name validity and uniqueness before changing the catalog.

Assignment changes require a preview token. For example:

```sh
mxr categories --format json assign --message-ids <message-id> --add <category-id>
mxr categories --format json assign --preview-token <preview-token>
mxr categories undo <mutation-id>
```

The preview does not write. Applying it checks that the selected membership
revisions still match; the returned mutation ID can undo only assignments that
have not changed since. Category deletion follows the same preview-token
pattern with `mxr categories delete <category-id>` followed by
`mxr categories delete --preview-token <preview-token>`. Deletion removes the
category and its memberships, never the mail itself.

The category catalog is profile-global. MCP and agent profiles can list or
change it only when their existing `allowed_accounts` explicitly cover every
stored and configured account, including disabled accounts. An empty allowlist
never grants catalog access. A profile restricted to a subset may list and
assign categories through a named allowed account, but cannot create, rename,
or delete the global catalog. Catalog writes still follow the profile safety
and destructive-action gates.

The bridge and daemon must use the same IPC protocol version. Category-rule IPC
variants require the matching daemon and client revision, so a new bridge refuses an old daemon
and an old bridge refuses the new daemon through the existing health check.


## Find categories in every client

In the web mailbox, open the **Labels** picker, then **Categories**. Select
categories and review the assignment preview before applying. **Manage** opens
rename and deletion controls. In the TUI, press `Ctrl-P` and choose `Categories`
or `Assign Category`. Press `Space` to stage an assignment, `Enter` to preview,
and `Enter` again to apply. Undo is available from the mailbox. Press `w` in a
category view or assignment picker for Why; an assignment selection explains
the first selected message.

## Rules preserve your manual choices

Create Travel, then prepare a rule form:

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

Actions accept category IDs or names in a form; saved rules retain stable IDs,
so renaming Travel preserves the rule. `remove-category:Travel` contributes a
negative decision. Lower numeric priorities win for each category; equal
priorities use the stable rule ID. A rule cannot add and remove the same category.
Manual inclusion or exclusion wins over all rule sources. Undo restores only
the prior manual decision, leaving current rule contributions intact.

The preview covers the complete union of this rule's old contributions and
current matches. Apply saves the rule and reconciles its sources atomically.
Changes to mail, rules, category identity, manual decisions or sources reject a
stale preview. Conditions requiring an unfetched body do not match until the
body is available; the preview reports unavailable conditions.

For an existing rule, export `mxr rules --format json form <rule> > rule.json`,
or read the editable form through MCP `mxr_rule_form`. Change `enabled`, conditions, priority, scope or actions in
the complete form, preview it, then apply. Deleting a rule uses
`category-preview <rule> --delete` and
`category-apply <rule> --delete --preview-token <token>`. Disable and deletion
remove only that rule's sources; another rule or manual decision may still
keep a message in the category. A category cannot be deleted while any rule,
including a disabled rule, references it. The error names those rules.

In web Rules, choose a local category in the editor and review the complete
preview before **Save** or **Apply**. The enabled switch and deletion are
reviewed in that editor. In TUI Rules, use `add-category:Travel` or
`remove-category:Travel` in the action field. `Ctrl-S` or `Ctrl-D` opens the
preview; `PageUp` and `PageDown` page through every effect, with the current effect and total
in the title. `A` applies. The account field cycles through enabled accounts and
**All accounts** for category rules. Toggling or deleting a saved category rule opens its
reconciliation preview. MCP exposes `mxr_category_rule` for the same
preview/apply lifecycle and `mxr_category_why` for provenance.

`account_id: null` explicitly selects All accounts. Persisting that form
requires access to every stored and configured account, including disabled
accounts. A restricted profile must keep an allowed selected account. Category
actions remain local and do not change provider labels, folders or workflow
placement. Other actions in the same rule retain their existing sync behavior.
