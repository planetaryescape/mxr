# Stored-send recovery can erase uncertainty

Read-only audit on freshly fetched `origin/main` at `80606f4833b651bf9b07975ddd49083d7cb2568a`, 2026-10-10. Worker `/root/portability_send_evidence`, GPT-6-Luna/high. No runtime reproduction or implementation in this audit.

`send_stored_draft` changes `sending` back to `draft` on every provider error (`crates/daemon/src/handler/mutations.rs:3361-3369`). The fake provider already supports acceptance followed by an error (`crates/provider-fake/src/lib.rs:136-138,256-260,584-599`). The existing Got-it handler test exercises that failure while retaining its separate claim (`crates/daemon/src/handler/tests/messages.rs:953-980`); stored-send tests do not exercise it. S03 should reproduce this path and ensure a retry cannot repeat the effect.

`MailSendProvider::send` returns `Result<SendReceipt>` (`crates/core/src/provider.rs:130-155`). `MxrError` has provider strings and rate limiting but no acceptance disposition (`crates/core/src/error.rs:3-44`). SMTP treats both transient responses and timeouts as retryable rate limiting (`crates/provider-smtp/src/lib.rs:212-237`). Outlook wraps transport errors as Provider (`crates/provider-outlook/src/smtp.rs:94-105`). Gmail request/response-parse failures become broad provider errors (`crates/provider-gmail/src/client.rs:143-165,364-385`, `error.rs:31-37`). Preserve evidence of rejection versus unknown acceptance in adapters; do not infer exactly-once behavior from a Message-ID.

Known repeated-send paths to include:

- CLI/IPC/web retries after the unconditional provider-error reset (`crates/daemon/src/commands/mutations/compose.rs:1157-1200`, `crates/web/src/routes_v6.rs:2090-2100`).
- Startup resets stale `sending` drafts after one hour without provider evidence (`crates/daemon/src/server.rs:1772-1796`, `crates/store/src/draft_recovery.rs:48-87`).
- Explicit `drafts resume` and web reset use `ResetOrphanedDraft` to restore editable state (`crates/daemon/src/commands/mutations/compose.rs:872-894`, `crates/web/src/routes_v6.rs:2072-2087`, `crates/daemon/src/handler/mutations.rs:2351-2360`).

Receipts already replay before sending (`crates/daemon/src/handler/mutations.rs:3182-3184,3256-3258`; tests `crates/daemon/src/handler/tests/platform_and_export.rs:436-574`). After provider success, however, sent-message ingestion precedes status and receipt persistence (`mutations.rs:3373-3420`). A crash can leave `sending` without a receipt. Receipt write failure after a successful status write leaves `Sent` without a queryable receipt; failure of both can leave a retryable draft. Add fault injection around these boundaries.

Scheduled sends clear `send_at` and persist an attempt marker before the effect (`crates/daemon/src/loops.rs:2041-2058`, `crates/store/src/scheduled_sends.rs:74-100`). Startup marks unresolved attempts interrupted instead of resending (`server.rs:1811-1839`). Provider errors nevertheless become generic failed attempts (`loops.rs:2058-2089`), and the shared handler already reset the draft. Retain explicit uncertainty across manual send/reschedule as well. Existing tests cover successful no-refire and cancellation, not accepted-then-error (`handler/tests/body_and_invites.rs:1225-1340`).

Build on existing draft statuses, sent receipts and scheduled markers. Useful test seams include `fail_sends_after_accepting`, handler provider injection, store fixtures and scheduled marker injection. Do not implement a generic job system. S03 implementation starts only after the durable-draft stage is released.

Parent refresh at origin/main `2c55a735e2b013910cb7222211fa8d6156d6204c`: all 13 audited production source files remain byte-identical to the initial audit, verified by Git blob identity in [recheck evidence](s03-send-audit-recheck-2c55a735.json). Parent also reread the current stored-send path, receipt store and provider trait. No runtime reproduction is claimed. Recheck again against the eventual deployed S02 contract before implementation.
