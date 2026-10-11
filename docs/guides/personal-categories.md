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
`--account <account>` to limit results to one account. Scoped agent profiles
must name the account; their category list includes only categories with
memberships in that account, and counts are computed within that account.

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

The category catalog is profile-global, so creating, renaming, and deleting a
category require unrestricted account access. Scoped agent profiles may read
and assign categories visible in a named account, but cannot make global
catalog changes. Categories are manual in this release; rules do not create or
remove memberships.
