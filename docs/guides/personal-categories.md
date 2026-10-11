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
use `mxr categories show <category-id> --format jsonl`. Both commands accept
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
mxr categories assign --message-ids <message-id> --add <category-id> --format json
mxr categories assign --preview-token <preview-token> --format json
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
and destructive-action gates. Categories are manual in this release; rules do
not create or remove memberships.

The bridge and daemon must use the same IPC protocol version. Category IPC
variants raise that version from 6 to 7, so a new bridge refuses an old daemon
and an old bridge refuses the new daemon through the existing health check.
