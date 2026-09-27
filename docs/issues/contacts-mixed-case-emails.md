# Contacts stored with mixed-case emails would stop matching

Status: logged, not fixed. No known occurrence.

## What

`owed_replies`, the cadence watchlist and the desk now join `contacts` on
the bare `contacts.email` column (`contacts.email = LOWER(x)`) so the
`(account_id, email)` key serves the lookup. Wrapping the column in
`LOWER()` scanned every contact per candidate thread and made `mxr owed`
time out on a 110k-message mailbox.

That join only matches rows whose `email` is stored lowercase.

## Why it is safe today

- The only production writer, the contacts refresh (`refresh_contacts`),
  stores `LOWER(...)` addresses.
- `upsert_contact` now lowercases on write too; before, only tests called it.
- A read-only check of a large real store found 0 rows where
  `email != LOWER(email)`.

## If it ever appears

Symptoms: a known contact treated as a stranger (owed replies ranked with
the default cadence, list senders not excluded, desk rows under "New from
people").

Fix: a migration that normalises existing rows, merging duplicates that
differ only by case:

```sql
UPDATE OR IGNORE contacts SET email = LOWER(email) WHERE email != LOWER(email);
DELETE FROM contacts WHERE email != LOWER(email);
```

The next contacts refresh rebuilds the merged counts.
