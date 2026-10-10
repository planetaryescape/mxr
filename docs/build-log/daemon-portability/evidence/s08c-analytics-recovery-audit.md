# Restart does not reconstruct the relationship queue

Source-only audit of fresh `origin/main` at `2c55a735e2b013910cb7222211fa8d6156d6204c`, with S08a candidate `62421fe6d87692e2d2c780713b3bb49c98eb7d09` checked separately. Fast worker `gpt-6-luna`/high performed the inventory; the parent checked the named paths and added the draft/deletion callers. No daemon, mail or data was changed. Blob identities are in [the source manifest](s08c-analytics-recovery-audit-2c55a735.json).

At main, `crates/relationship/src/service.rs:50` holds pending contacts and deduplication keys in memory. Enqueue acknowledgement means admission, not completion. The worker drains pending style, summary and commitment work before accepting another command. S08a `crates/daemon/src/state.rs:1585` bounds service shutdown and can abort that worker. Its passing CI lifecycle test (`crates/daemon/tests/daemon_lifecycle.rs:611`) compares search IDs after restart; it does not inspect relationship profiles.

Existing recovery paths have different scopes:

- Sync enqueues contacts from upserted message IDs (`crates/daemon/src/loops.rs:1140`). During initial backfill it accumulates contacts until the last page (`:637`, `:690`); that deferred set is also process-local (`crates/daemon/src/state.rs:490`). A no-change sync does not perform a full relationship sweep.
- Provider deletion enqueues affected counterparties (`crates/daemon/src/loops.rs:1219`). This repairs the affected contacts, not every missed contact.
- `mxr profile <email> --rebuild` selects one contact (`crates/daemon/src/commands/profile.rs:7`; `handler/relationship_profile.rs:20`). Style determines the returned result; summary and commitment failures are logged (`crates/relationship/src/service.rs:106`). Verify the fields, not just exit status.
- An AI draft request without `before` calls `ensure_contact_fresh` (`crates/daemon/src/handler/draft_compose.rs:233`). Its predicate checks missing style, missing summary, or style computed before the newest contact message (`handler/draft_context.rs:227`). It does not independently check whether commitment extraction completed.
- `doctor --rebuild-analytics` repairs directions, list IDs, reply pairs, business-hours latency and the contacts table (`crates/daemon/src/handler/diagnostics/mod.rs:615`). Those SQL passes do not rebuild relationship profiles.

Startup clears interrupted sync status (`crates/daemon/src/loops.rs:794`). Its maintenance routine handles drafts, decision evidence, interrupted scheduled sends and lexical search (`crates/daemon/src/server.rs:1772`); it does not reconstruct relationship work lost from earlier backfill pages or the pending worker queue.

S08c should use an isolated fake-provider profile, hold the relationship worker behind the existing zero-permit semaphore test seam, enqueue a contact, stop, and restart against the same data. Check acknowledged mail/search separately from that contact's style, summary and commitments. Run a no-new-mail sync, then exercise the explicit per-contact rebuild and inspect each result. Recheck the source after S03/S04 deploy; this audit does not freeze a recovery design.
