# A bill's to-do changes identity when its due date crosses a month

Status: fixed 2026-10-29 (fix/bill-todo-keeps-email). Found 2026-10-29 while fixing the clock dependence in `handler::tests::todo::html_evidence`. BK's decision: same email, same to-do.

## What happened

`mxr_todo::pass::stored_record` keys a bill's to-do by its due month: `asks|payment|<fingerprint>|due:%Y-%m`. Every other asks kind ends its key with the same period, `due:%Y-%m` or `sent:%Y-%m`. If a rescan of the same email found a due date in a different month, for example a correction that moves the due date from 31 October to 1 November, the rescan made a second row with the new date. An untouched old row dropped out of the list because its stored answer was stale. A snoozed or edited one stayed open beside the new row with the old date, so the user saw a duplicate and the new row had none of their snooze or edits.

The month in the key is deliberate: it keeps next month's bill from the same sender, which arrives in a new email, a separate to-do.

## Fix

The key format is unchanged. Before the upsert, `classify` calls `Store::adopt_asks_same_source`. When no row holds the new key, and the same source email already has exactly one row whose key differs only in its period, that row takes the new key. It keeps its id, state, snooze and edits. The upsert then writes the new due date, unless the user edited the row: edited rows keep the user's fields, as before.

The move is skipped when:

- another item in the same email holds that period now, so one email with two bills a month apart keeps two rows;
- the stored answer is no longer current;
- the email has two candidate rows, which is already ambiguous.

A row made from another email never moves, so next month's bill is still a new to-do.

No migration is needed. Rows written before this change already carry the current key format, and the adoption reads them as they are.

## Tests

- `handler::tests::todo::same_email`: a re-dated bill keeps its id and snooze and shows 1 November, an edited bill stays one row with the edit, next month's bill in a new email is a second row, and one email with two bills a month apart keeps two rows across a rescan.
- `handler::tests::todo::html_evidence::structured_fields_retain_exact_authored_quote_and_revoke_with_account_trust` runs at 26 and 29 October. On 29 October the rescan moves the due date into November.
- `mxr-store` `todos::tests::a_redated_item_keeps_its_emails_row_and_never_takes_a_sibling_or_another_emails` covers the sibling, stale-answer, other-email and taken-key guards.

With the adoption call removed, the `same_email` re-date tests and the 29 October `html_evidence` case fail with two rows or a new id.

## What's left

Moving one email's bill from 31 October to 1 November puts it in the same period as a November bill from a new email with the same reference. The two still share one key and one row, as before this fix.
