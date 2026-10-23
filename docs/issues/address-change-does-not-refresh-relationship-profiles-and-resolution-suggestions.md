# An address change does not refresh relationship profiles or resolution suggestions

**Status:** open · **Found:** cross-model review of PR #665 (round 1) · **Severity:** should-fix · **Deferred**

## What happens

An alias add, remove or set-primary reclassifies stored mail (see
`docs/issues/alias-registration-does-not-reclassify-stored-mail.md`). Contacts
are refreshed after it, but the other tables that cache a view of a person or a
thread from direction are not. They keep the view they were built from until
something else rebuilds them.

- `contact_style`, `contact_relationship_summary` and `user_voice_profile` are
  written from a contact's sent mail. Nothing queues a rebuild after a
  direction change, so a person whose sent mail changed direction keeps the
  style and summary built from the old split.
- `context_briefings` keys recipient briefings on contact totals, so they go
  stale with the contacts table. Gist keys omit ownership and direction, so a
  cached gist can outlive an ownership change (`thread_gist.rs:483`).
- Relationship profile reads return the persisted profile directly
  (`crates/daemon/src/handler/relationship_profile.rs:47`). They do not check
  direction, so a profile built before the change is served as current.
- Accepting a resolution suggestion checks that its source still exists, not
  that the direction it was offered under still holds
  (`crates/store/src/todo_resolution.rs:537`). Existing offers and their
  evidence are not reconciled.

## Why it is deferred

The contacts refresh is the one cache with a queued refresh today. Adding
rebuilds for the four tables above means choosing, for each, between a rebuild
on address change and a check at read time. That is a design decision about how
much a profile may lag, so it is not part of the alias fix.

## What to decide

- For each table: rebuild on address change, or invalidate and rebuild on the
  next read.
- Whether accepting a suggestion should recheck the direction it was offered
  under, and refuse when it no longer holds.
