# Daemon request handlers

Paths are relative to `crates/daemon/src/handler/`.

### Mutations: the undo entry and the dedup key are one key, and undo is written for failed-part-way batches too
**Chosen**: persist an undo entry whenever a message may have changed (succeeded kinds and the one that failed part way), keyed by the same `mutation_id` as dedup. A failed undo write is non-fatal but flagged to clients. Snapshot every envelope before `apply_mutation_to_envelope` runs. Two provider calls under one mutation id suffix the dedup key. A batch that fails outright still lets the undo of an earlier batch be recorded. Desk Done and its sibling command share one id, so dedup keys never collide and one undo entry covers both.
**Rejected**: separate undo and dedup keys; skipping undo on partial failure (a retry is a new mutation with its own snapshot, so this entry is the only record of the original); failing the mutation when only the undo write fails (the mutation already succeeded).
**Where**: mutations.rs (mutation apply and undo snapshot code), desk_done.rs (Done plan run).

### Mutation provider errors after success are logged, not raised
**Chosen**: when the provider already applied the mutation and a later local step fails, log it and continue. A retry of the same `mutation_id` is safe because Gmail and IMAP operations are set-semantics. Left-out items count as skipped without stopping a job. An unfilled draft gap, or a claim whose desk rows will not restore, hands the claim back so the same id retries.
**Rejected**: surfacing the error as a mutation failure, which would invite a duplicate retry of work already done.
**Where**: mutations.rs (mutation apply, job loop, desk restore).

### Mutation and job ids are idempotency keys: a repeated job id returns the existing job
**Chosen**: a job with the same id means the first request got through and its reply was lost, so hand the job back. Undo stamps first, so a double undo (two clients, a double press) is a no-op. Arrivals take a lock across the check and insert so overlapping moves see the first task.
**Rejected**: starting a second job; reverting twice; check-then-insert without a lock.
**Where**: selection_jobs.rs (job start), arrivals.rs (undo stamp, move task insert).

### Sent copies for IMAP/SMTP are appended best-effort and never turn a sent mail into a failure
**Chosen**: Gmail send files Sent itself. For IMAP/SMTP the transport does not, so the daemon appends a copy to the server Sent folder (parity with Gmail). A failed APPEND never fails the send. An acceptance repair is local-only and never repeats a provider write. An HTML draft files a Sent copy with both text halves. The sent-thread summary refresh runs fire-and-forget because sends are rare and the user's confirmation must not wait on the LLM.
**Rejected**: failing the send on APPEND failure (the mail already left); repeating provider writes while recovering; awaiting the summary.
**Where**: mutations.rs (send path, Sent copy filing, post-send summary refresh).

### A draft is safety-checked against a materialised text body, never an empty one
**Chosen**: `--check` on a transient draft, and rows written before the alternative was materialised, get the text alternative built first. An unfilled gap (`[[?: ...]]`) is never overridden by a reviewed token. A custom Bcc is never made visible by the reply policy picking the same address for To or Cc. Per-message From is validated as an owned address like the send path.
**Rejected**: running checks on an HTML-only draft, which would always report SAFE against an empty body; letting override tokens cover unfinished text.
**Where**: mutations.rs (draft check, send validation, reply recipient building).

### Replying ties to a thread through In-Reply-To when the client gives no ThreadId; send flusher refuses early
**Chosen**: fall back to `In-Reply-To` for the thread. Refuse a send now when its provider cannot be found, because the flusher only looks the provider up when it fires. The draft mutex key is namespaced and never stored in the drafts table.
**Rejected**: running thread-scoped checks against nothing; accepting a send bound to fail later.
**Where**: mutations.rs (reply thread lookup, send enqueue, draft mutex key).

### Invalid input is reported with an explicit wire error kind, not by string sniffing
**Chosen**: oversized instructions are rejected up front and carry `IpcErrorKind::InvalidRequest`; a rejected account credential is an auth failure whatever its text says and skips the substring classifier. Provider Auth/OAuth wrappers do not imply a rejected grant: token-endpoint transport failures are retryable network failures and an unsaved refreshed token is a local storage problem.
**Rejected**: classifying by error text; truncating an oversized instruction (it would crowd out the conversation).
**Where**: mutations.rs (kinded errors), draft_compose.rs (instruction limit), error.rs (classifier), status_helpers.rs (auth classification).

### Compose and draft calls never leak across accounts or to a non-opted-in cloud model
**Chosen**: a conversation from another account is refused (by thread or source message) before any model call; a cloud model without opt-in sees only the conversation being drafted, not other emails or the relationship summary, and a draft written from history skips cloud rewrite and says so. A proposal revision uses the same account's text. Mail-derived blocks (thread, draft, voice context, humanizer inputs) are wrapped in untrusted-content markers so only the task sits outside as authoritative; output is a suggestion and never auto-sent. Truncated model output is retried with more room, then reported, not delivered half written. Source-backed figures only: a guessed amount in a gap must not become evidence.
**Rejected**: one account's voice dressing another account's reply; trusting mail text in prompts; handing over cut-off drafts.
**Where**: draft_compose.rs, compose_assist.rs, humanizer.rs, safety_llm.rs (prompt building and account checks).

### LLM-cited evidence ids are validated against the retrieved set; one made-up id rejects the whole result
**Chosen**: reject any ask or decision whose `evidence_msg_id` is outside the retrieved messages; do not write half-credible rows.
**Rejected**: keeping the valid citations of a partly invented answer.
**Where**: safety_llm.rs, decisions_extract.rs (citation validation).

### Recipient suggestions ignore Bcc entirely
**Chosen**: a candidate who appears on a thread only through a Bcc on a message the user did not send is excluded; ignoring Bcc fields when iterating candidates is the simplest enforcement.
**Rejected**: tracking Bcc per candidate and filtering later.
**Where**: suggest_recipients.rs (candidate aggregation).

### Account scope: selections and no-account reads are checked by account, not by message list
**Chosen**: a selection names its account and its id list is empty, so classifying by messages would let it past every scoped profile; no account means `AllAccounts`, denied to scoped profiles. Reads with no account (Reading) are narrowed by dispatch with `allowed_accounts`. Disabled accounts' messages never reach a reader. Remote clients: one legacy draft must not hide the rest of a paired client's drafts (omit it so host paths stay on the host). A local continuation never puts privileged tool output into a transcript a remote registration can read later. A legacy thread id may span accounts. `account_id` on the archive filter struct is accepted but not required, since dispatch fills it in.
**Rejected**: message-based classification for selections; denying Reading to a scoped agent.
**Where**: account_scope.rs (scope classification), mailbox.rs (thread read), remote.rs (draft listing), assistant.rs (transcript), archive_ask.rs (filter), mod.rs.

### Request classes: dry-run RSVP and egress fetches are not read-only
**Chosen**: a dry-run RSVP is the same verb as a real one so it stays out of `Read`; `GetHtmlImageAssets` and attachment fetches do remote egress (tracking-pixel risk) and are deliberately not read-only.
**Rejected**: treating them as read-only.
**Where**: mod.rs (request class table), tests.rs.

### Presence leases do not invalidate mode caches; stale views are rebuilt, never served
**Chosen**: a browser touches its lease before every API read, so presence leases do not invalidate caches (that would defeat every remote client's mode cache). A view that may have missed a commit is rebuilt, at most every `MIN_REBUILD` while a long write holds the writer. A build that began after a read arrived is shared by readers queued behind it. A source change during a read means the build is not reusable and is not cached.
**Rejected**: invalidating on lease touches; serving a view that may have missed a commit; each reader building separately.
**Where**: mod.rs (activity and invalidation), mode_cache.rs (rebuild and wait), messages/history.rs.

### Mode membership and Now ordering rules
**Chosen**: the Now band puts late rows first so its cap of three never hides one; membership exposes bounded evidence quotes but loads full sources only for the requested threads and never mixes generations; "mailing a thread you were only copied on, or a crowd you never wrote in" belongs to Updates; unknown human-response semantics stay in Messages' Recent/Quiet views, never the attention rail; "copied" needs positive evidence (you in Cc and not To), so mail to an unknown alias counts as addressed and never vanishes; desk counts are issues this week, never unread counts; To do holds a thread while any to-do is open; archiving needs every inbox message in the preview, so a message put back in the inbox is not archived behind the user; a failed `put_away` notifies every plan account, a dry run notifies none; a record that carries a current ask keeps that ask; current codes sort before expired ones so a limit never hides a working code; a record's price change or missed charge leads over a moment.
**Rejected**: oldest-first caps, hiding late rows, unread counts, silent vanishing of mail.
**Where**: now.rs, modes.rs, desk_lanes.rs, conversation_shape.rs, mode_done.rs, desk_done.rs, mail_kind.rs, codes.rs, records.rs, updates_digest.rs, mode_rules.rs.

### Places and mode reads use stored homes, not fresh scans
**Chosen**: stored facets already validate policy, context and generation; opening Now, Reading or Updates does not re-read and fingerprint settled threads. Raw signals are only a fallback for unresolved detail reads and normal lists never classify unresolved mail. Pins are written one account at a time and a gate is held only for that account's writes. Move logging is bookkeeping whose failure must not report the move as failed. Assessment readers expose pending, never a stale stored or newly computed answer, while inputs keep moving; projection yields between pages and keeps its cursor.
**Rejected**: rescanning all settled threads on every list read.
**Where**: places.rs, assessments.rs, row_seen.rs (mutation replies and list reads must describe the same revision; evidence queries are not repeated on cached reads).

### Account config saves re-plan under the lock and keep omitted fields
**Chosen**: omitted identity fields keep config's values, since the TUI and web forms do not send them; the plan is made again under the in-process lock against the current file and store, so a tag another account claimed in between is refused. The keychain mirror is best-effort and never fails the operation. Probes read the TLS certificate before syncing so a self-signed bridge's fingerprint is reported even when sync fails; credentials are never sent. A repaired password reloads providers (and signals IDLE to reconnect); if reload fails the repair stays and the error is surfaced saying a restart is needed. Tests disable the keychain mirror so they never touch the real keychain.
**Rejected**: planning once before the lock; failing the operation on keychain errors; hiding the reload failure.
**Where**: account_config.rs (save, probe, keychain mirror), accounts.rs (password repair).

### Microsoft To Do: missing is not cleared; links are explicit
**Chosen**: omitted fields from older schema-2 CLIs never erase a linked schedule; reclassification and ageing never erase the local half of a live link (linking is an explicit user decision); one failing link never stops the rest, and the store's attempt time sets the retry interval.
**Rejected**: treating a missing field as an explicit clear; stopping the run on one failure.
**Where**: microsoft_todo_cli.rs, microsoft_todo.rs.

### Promises, assessments and arrivals keep identity with the stored unit
**Chosen**: a model-supplied fragment does not carry identity, the stored authored unit does, so accepting its date never creates another promise; names come from this account's envelopes, never an address or UI label; arrivals show the current reason from the read projection while `arrived_in` retains history, and an automatic policy change is never labelled a user move; stored-under-unknown-name counts still add to the total so parts never fall short; deferral re-reads state at write time so undo holds what the write replaces.
**Rejected**: new identities from model text; deriving names from addresses; labelling automatic moves as user moves.
**Where**: promises.rs, ask_assessments.rs, arrivals.rs, deferral.rs.

### Gists and digests are not cached over newer state
**Chosen**: a gist built while a message arrived is returned with the message it answers for but not cached over the newer state. Numbers the figure pattern cannot read (full-width, superscript, fractions, Roman numerals) fail the check rather than pass.
**Rejected**: caching the stale gist; passing unverifiable figures.
**Where**: thread_gist.rs, gist_numbers.rs.

### Sync and rebuild: claims release on drop, progress is per step, subscribe before trigger
**Chosen**: the sync claim is moved into the task and dropped when it ends however it ends, so a panic or shutdown releases the account (a leaked claim leaves `sync_in_progress` stuck); analytics rebuild emits an `OperationProgress` per each of six SQL passes; event subscription happens before triggering so the first `OperationStarted` is not lost; a search-index scan that overlapped a rebuild is discarded; live-id checks are chunked so 500k ids never become one statement; semantic chunk persistence needs the `semantic-local` worker.
**Rejected**: explicit release calls; subscribing after trigger; acting on a half-filled index.
**Where**: diagnostics/mod.rs (sync task, analytics rebuild, event wait), diagnostics/search_resolve.rs.

### Search: label names resolve to provider ids; unknown custom filters are no-ops
**Chosen**: user-typed label names (`label:Notto`) resolve to provider ids (`Label_101`) before query construction, since the index only holds provider ids. Owed-reply is computed per thread, so the per-envelope test defaults true. An unknown custom filter defaults to true so new Gmail additions do not break searches. A name or term typed as words must not become a field filter because of a colon. `has:calendar` stays an explicit unsupported error in rules, never silently matching everything. Triage is not fed the narrower SQLite fallback set. Saved-search counting records a zero and logs on failure rather than poisoning the response.
**Rejected**: querying the index by display name; dropping candidates in per-envelope tests.
**Where**: diagnostics/search_execute.rs, diagnostics/search_filter.rs, whois.rs, tests.rs (rules conditions), triage.rs, platform.rs.

### Gmail spoof defence: spam and trash never count as deliveries
**Chosen**: skip spam and trashed mail when detecting deliveries, since phishing that spoofs carriers lands in Spam; a cheap high-precision guard.
**Rejected**: scoring every message.
**Where**: deliveries.rs.

### Acknowledgements and send recovery never repeat provider writes
**Chosen**: the acknowledgement claim is stored before the provider call and never removed after, one per message whatever goes wrong; only the attempt holding the claim sends, failures release it, a confirmed unsubscribe is never sent again; retry is never earlier than the provider's deadline and jittered per job to avoid a stampede; archive recovery does not restore an Undo entry the user already consumed.
**Rejected**: removing the claim on failure for acknowledgements; unjittered retries.
**Where**: messages_ack.rs, mutations.rs (unsubscribe send), send_recovery.rs, archive_recovery/runner.rs.

### Messages thread text uses CRLF-aware paragraph counting
**Chosen**: `new_text` keeps original line endings, so a break can be CRLF; count paragraphs accepting `\r\n\r\n` because a naive `split("\n\n")` undercounts to one. Reading data for many messages uses two queries for the set, not two per message; missing bodies stay per-id failures. With no topic named, never open one just finished.
**Rejected**: splitting on `\n\n`; per-message queries.
**Where**: messages.rs, mailbox.rs.

### Activity capture is fire-and-forget
**Chosen**: capture at the seam never propagates errors (see `docs/activity-log.md`).
**Rejected**: failing a request because activity capture failed.
**Where**: mod.rs (activity capture hook).

### RSVP replies go out from the alias the invite named
**Chosen**: reply From equals the ATTENDEE identity we RSVP as (RFC 6047).
**Where**: mailbox.rs (RSVP send).

### Test-only choices
**Chosen**: remote profile test uses a real allowed preview to prove account filtering rather than blanket rejection; diagnostics timeout limits are generous in tests because a fake-provider sync can exceed a tight limit under parallel load, with the detach path tested in `loops::tests` using explicit limits; a made-up citation id in `decisions_extract` tests proves whole-decision rejection.
**Where**: remote_tests/profile.rs, diagnostics/mod.rs, decisions_extract.rs.

### Config and keymap writes are one locked synchronous section
**Chosen**: account config, notification settings, LLM and keymap writes take the file lock, read the file as it is then, apply the change and save atomically (keymap writes via a sibling file and rename). Nothing awaits while the file lock is held, so the blocking lock never parks a runtime worker. Callers hold `AppState::config_write` for in-process writers. Notification settings apply only the given fields on top of the current setting, so two clients changing different fields never lose one. Saved-search names must be unique because search runs, renames and deletes by name. `[llm]` timeouts not in the IPC DTO keep the file value.
**Rejected**: reading config before awaiting and writing it after (a stale copy); full-struct overwrites.
**Where**: account_config.rs (config write section), notifications.rs, keymap.rs (write), diagnostics/mod.rs (config updates).

### A providerless account keeps kind "account"
**Chosen**: an account with no provider gets the kind `"account"`, the id startup has always created, because stored rows already use it.
**Rejected**: renaming it.
**Where**: account_config.rs (account kind for a provider-less account).

### Scope is an exhaustive match, and responses are scoped again
**Chosen**: `request_scope` has no catch-all arm, so a new `Request` variant does not compile until someone decides what it touches (the old hand list ended in `_ => None` and let every forgotten request through). `scope_response` is a second net: responses that carry other accounts' rows (status, signatures, threads whose id another account shares) are cut to allowed accounts, and thread aggregates are rebuilt from the kept messages (the store takes `MIN(subject)`, so the rebuild does the same). Events with no resolvable account are dropped. An `--all` job keeps no undo (D192); a caller cleared for all its accounts gets the refusal that says why, any other caller gets the not-found an unknown id gets, so the job's existence does not leak. A job is attributable to accounts only once its result is in. A target naming something that does not exist is denied, not waved through. A new message with no account falls back to the default account, which a scoped agent did not name, so it is scoped.
**Rejected**: catch-all arms; scoping requests only.
**Where**: account_scope.rs (`request_scope`, `scope_response`, event scoping, undo job accounts).

### Agents drive the tools directly, not the assistant
**Chosen**: the assistant's changes wait for a person's approval and its tools run as `Assistant`, which no profile governs, so an agent that drove it could approve its own changes. Agents use the tools themselves. Scoped profiles get a not-found for tags they asked about, so they cannot learn which account holds a tag. Token transports authenticate before dispatch; a request reaching dispatch is on a trusted connection. The per-action destructive restriction applies when a profile lists actions even though `allow_destructive` is on. The destructive gate spans Mutate, Destructive and Admin so its behaviour is unchanged.
**Rejected**: letting agents call the assistant; telling scoped callers which account holds a tag.
**Where**: mod.rs (profile enforcement, request class table), account_scope.rs.

### A request cancels with its client only when it is a read
**Chosen**: a mutation, send or draft write runs to completion whether or not anyone waits. Slow work (LLM, scans, HTML prefetch) is demoted to the slow lane so a gallery cannot tie up every fast slot.
**Rejected**: cancelling on disconnect for everything.
**Where**: mod.rs (cancel-on-disconnect, concurrency lane).

### Reply and forward From is chosen by a fixed precedence, hardest to spoof first
**Chosen**: `Delivered-To` (topmost owned occurrence) wins over To/Cc, because a sender can put an owned alias in `To:` to steer our From while the MTA's `Delivered-To` names the real destination. Then a lone owned To/Cc address; with two distinct owned addresses and no `Delivered-To` it refuses to guess and uses the primary. Every send re-validates From against owned addresses at one choke point (rows can come from older binaries). A preview and the real send use the same resolver (`ResolveSendFrom`). Reply sources resolve by account-scoped RFC Message-ID, never provider thread ids, which are sending hints, not conversation identity. Reply recipients drop owned addresses from To first; the reply falls back to Cc, then the original To, only when nothing else is left.
**Rejected**: trusting To/Cc first; resolving by provider thread id.
**Where**: mutations.rs (`default_reply_from`, `resolve_send_from`, reply source lookup), reply_recipients.rs.

### Drafts validate at the IPC boundary and are re-validated at send
**Chosen**: the daemon is the authority, so content, safety and From checks run at the IPC boundary, not only in the CLI, and again at send because rows can come from an older binary. Only a supplied HTML document is validated for active content (mxr generated the HTML for markdown drafts). Provider drafts: only an explicit provider 404 removes a linked local draft; lookup errors keep every local row. A link made by an older mxr has no baseline, so record the current provider version without downloading. Detail: the stored-draft update touches only rows still in `draft` status.
**Rejected**: CLI-only validation; deleting local drafts on lookup errors.
**Where**: mutations.rs (draft gate, provider draft sync).

### Sends: one attempt holds the claim, acceptance never turns into a resend
**Chosen**: only the attempt holding the claim sends; a provider that already accepted cannot be turned into a resend; a retained approval retries only after positive non-submission evidence, ambiguous attempts are reconciled, never dispatched twice; a malformed hint cannot erase known non-submission or authorise an earlier retry; acceptance is persisted first and a missing attachment cache blocks only local repair. The reviewed hash, not freshly copied bytes, stays authoritative for tampered resources. Cleanup removes only the exact files this operation created and never follows a client-supplied attachment path; permission repair works through a no-follow directory handle so a replaced path cannot chmod a symlink target; older restore directories created with the process umask are tightened before frozen send bytes are published. A failed preparation owns only its unique temporary snapshot.
**Rejected**: recursive deletes of cache roots; following client paths; resending on ambiguity.
**Where**: send_recovery.rs, mutations.rs (send path), forward.rs.

### Background jobs: rate limits wait, missing mail is skipped, selection jobs keep no undo
**Chosen**: a selection (`--all`) job waits out provider rate limits (messages refused go first in the next chunk, retried once after the wait) and skips mail that left the store; an immediate request keeps strict rules (a missing message fails the batch). A selection job keeps no undo (D192), so undo of its id is refused with one wording shared by the daemon and `mxr undo --dry-run`. Start-job order makes a job id mean a real job: the token is taken first, and a same id with a different selection or action is an error. A job remembers its action without ids. Job rows bound free-text (a provider error can carry a whole HTTP body; Gmail does) and per-account name repeats. Daemon restart marks jobs left `running` as failed.
**Rejected**: unbounded job rows; one undo entry for an `--all` job (megabytes of ids).
**Where**: mutations.rs (job loop), selection_jobs.rs, job_records.rs, job_recovery.rs.

### Undo claims atomically and restores what the mutation might have changed
**Chosen**: claim the entry with one delete-and-return statement (`Ok(None)` means someone else took it) and put it back if undo cannot proceed. Undo runs under a fresh mutation id and suffixed dedup keys (a ReadAndArchive undo makes two calls). A failed mutation's undo reverses what it could have done at the provider too. One search commit covers the whole undo (a commit per message made a large let-go take seconds). The read flag is restored for READ and STARRED provider flags at provider and local copy. A move or label edit may have removed labels; a deferral or highlight change touches no message and has no snapshot.
**Rejected**: read-then-delete claims; per-message reindex commits.
**Where**: mutations.rs (undo), desk_done.rs, deferral.rs, reading/highlights.rs.

### Reindex is marked in the store and retried, never rebuilt per mutation
**Chosen**: messages whose reindex failed are marked in the store (memory fallback), retried with a bounded batch on the next mutation or startup so a long backlog cannot become a rebuild. A stale search index is not a failed mutation, because reporting it would invite a retry of work that is done.
**Rejected**: failing mutations on index errors.
**Where**: mutations.rs (reindex marks), mod.rs (label count refresh).

### Label counts refresh once per batch, not per message
**Chosen**: batch paths persist label changes without refreshing counts (the refresh scans every label's rows) and refresh once at the end; undo and selection paths name labels by provider id only, so the local copy gets exactly what the provider got; a snoozed-again email moves only its wake time and keeps the labels from before the first snooze. Provider ids repeat across accounts (two IMAP accounts both hold `INBOX:42`), so a batch across accounts reaches each provider.
**Rejected**: per-message refresh; reading labels again when snoozing again.
**Where**: mod.rs (label reconcile helpers), mutations.rs, selection_jobs.rs.

### A bulk label edit may only name user-created labels
**Chosen**: only labels of kind `User` with no mailbox role (some providers tag a role on a label) are accepted, and the preview pins each name to the label it means in each account; labels are checked in the accounts the selection reaches after resolution.
**Rejected**: names resolved again at run time; system labels.
**Where**: selection_jobs.rs (label validation).

### Sweeps archive exactly what a preview listed, rechecked per account
**Chosen**: a sweep preview token cannot be replayed and expires; the real sweep archives only what the preview listed that still qualifies, rechecked while the account's gate is held (a pin or sender move bumps the generation). Pins are written under the gate. The contact refresh can change who counts as a list sender across every account at once. Unsubscribe previews are token-bound; a confirmed unsubscribe is never sent again; opening a page in a browser records nothing since mxr cannot see the outcome; reasons shown to the user never carry a URL or address; an expired one-click URL (HTTP 410) falls back to the current mailto.
**Rejected**: recomputing the selection at run time; recording browser opens as sent.
**Where**: places.rs, mutations.rs (unsubscribe), unsubscribe_plan.rs.

### Search state: pages never mix index and SQLite
**Chosen**: availability is read before the query and checked again after, and a page that straddled a stop or rebuild is rerun (bounded) then answered from SQLite; a page is never part index, part SQLite. The wire reason stays `index_stopped` because older clients reject new reasons; the retry text is chosen server-side. The SQLite fallback runs only when every part of the query is something `messages_fts` can answer; bare words need every token as the index does; `from:` is an exact address match. The owed filter runs inside the page loop so a page fills with owed rows (before, the first page took the newest `limit` and returned empty), owed rows are capped per account, and owed lookups fail loudly (a caller that must not act on a partial answer treats any failure as fatal). Selections use the same compile path as the search that previewed them, with no per-account cap because a cap would drop owed threads silently. Trash and Spam are excluded unless asked, as Gmail search does, by one rewrite that all paths share; past the expansion cap a query keeps both exclusions. Dense retrieval only widens recall inside the source kinds the query implies. Semantic search errors in a read fail the request: an empty set would list you as an expert or suggest your own address.
**Rejected**: part-index part-SQLite pages; a new wire reason value; per-path exclusion logic.
**Where**: diagnostics/search_execute.rs, search_fallback.rs, search_filter.rs, search_resolve.rs, expert.rs, suggest_recipients.rs.

### Status never blocks on the resource it reports
**Chosen**: `get_status` races the DB snapshot against a budget and degrades instead of waiting under reader-pool saturation; only the DB budget sets the degraded flag (a dropped semantic runtime leaves other fields real). The snapshot future is passed in so tests decide whether it resolves. A degraded reading never triggers a search repair. Doctor waits for its snapshot. A sync in flight is not a health problem. Doctor and sync health agree on auth-class failures (a Gmail 401, a rejected IMAP login); a search worker that stopped in a clean shutdown is not a failure; `search_worker_failed` is passed in so one report cannot contradict itself. Freshness marks a sync stale after five missed intervals, never under fifteen minutes.
**Rejected**: blocking status on the DB; reading search repair from degraded data.
**Where**: diagnostics/mod.rs (get_status), status_helpers.rs, freshness.rs.

### Ownership changes reclassify under a gate and queue reconciliation
**Chosen**: the account gate covers the address write and the reclassify, so a sweep check cannot see the old set and archive after the change. The account owes a reconciliation after its next sync pass even when the immediate reclassify succeeds (a sync batch may have used the old set). Derived caches (Wrapped, contacts) are dropped unconditionally. Reclassifying against an address set that failed to rebuild would move mail the wrong way, so the step fails retryably. One-shot historical repairs flip their flag on first attempt regardless of outcome.
**Rejected**: reclassifying without the gate; trusting a stale registry.
**Where**: diagnostics/mod.rs (address change, reconcile, backfills).

### Model calls: untrusted wrapping, no caching of stale answers, numbers checked
**Chosen**: everything derived from mail (subjects, transcripts, relationship context, drafts, voice context, baselines, a single message in triage) is wrapped in untrusted-content markers; the task stays outside. Thread id and message count are trusted; subject is not. Summary and triage cache keys include an owned-address revision so a new alias misses; relationship summaries are other people's mail condensed, so they go to a cloud model only with opt-in and the key changes when the context is withheld. Gists: kind is part of the cache key; a gist with a figure the mail does not write is worse than none (subject shown instead); quotes must appear in the cited message; the model's JSON may sit inside prose or fences. Provider and privacy gate are pinned once per request, and what the prompt carries follows the endpoint actually called, not the config. A gist that went stale while written goes to the back of the queue (not the front, so a busy conversation cannot starve others), and backoffs are bounded. The queue is capped because more than the cap is minutes of work for rows the user left.
**Rejected**: trusting thread metadata or caching under the old key; config-based privacy decisions.
**Where**: summarize.rs, triage.rs, thread_gist.rs, thread_gists.rs, gist_kind.rs, draft_provenance.rs, briefing.rs.

### Draft generation details that readers would undo
**Chosen**: relationship tone weighs the contact's formality (0.6) over the user's (0.4), since theirs signals how they expect to be addressed (0.9 and 0.35 blend to 0.68, Formal, where an average would give Neutral). A reply to a message in Trash or Spam stays in, because the user chose it. Replies under a length say nothing about voice, longer ones are templates. Too little with this person falls back to how the user replies to anyone. Cleanup removes model chatter, fences, `Subject:` lines and `[Your Name]` placeholders; `[[?: ...]]` gaps stay for the user. The prompt leaves room for a reasoning model's thinking inside the context window. A stored forward keeps content and files but not source ids, so the client sends the source with the `forward` target. A draft prefilter keeps the model away from drafts with no commitment marker; the draft's own "by Friday" is read in the daemon's zone with one reading only.
**Rejected**: plain average formality; blocking replies to trashed mail.
**Where**: draft_context.rs, draft_voice.rs, draft_output.rs, draft_compose.rs, compose_assist.rs, commitments_extract.rs.

### Mode and lane rules readers would reverse
**Chosen**: a crowd thread stays in Messages only while it is your turn; owed means the desk's You owe lane computed by the desk itself so the two never disagree (D103); an auto-responder or notification after your message answers nothing, so a set wait time still hides or returns it; a time you set that brought a conversation back is due by definition and takes no pace; your turn decays after max(3 times their cadence, 7 days), capped at the window (blueprint 22; Kooti et al., WWW 2015); Allow gives Messages visibility, not proof of a human response; mail synced before its alias was registered counts as yours when the provider filed it as sent; promises stand whatever happens to their thread, so due rows skip inbox checks; a date a model wrote is not a deadline the desk acts on; archive does not take a watched contact's row off Waiting because the row is about the person; a snoozed thread hides until it wakes. Reading expires on source dates, not extracted text; Later items older than the cutoff are asked about once; opening Reading is a visit unless activity is off or paused (one privacy switch). Mark ids: with an id list a message the mark never saw is new even if dated earlier; without one the (date, id) watermark breaks ties by id.
**Rejected**: plain unread counts; staleness by extracted text.
**Where**: desk_lanes.rs, desk_timers.rs, desk.rs, conversation_shape.rs, mode_rules.rs, reading/mod.rs.

### Mail kind rules: positive evidence, bounded work
**Chosen**: kind is decided by positive evidence: the user's choice for a sender, a message-level directed request addressed to "you", and history can support a personal reading of a role address but cannot erase per-message evidence. List headers on notifying hosts (GitHub's notifications have List-Unsubscribe) do not make mail a newsletter; an ATS reply with an opt-out header loses to message-level response evidence. Banks' alerts match an amount plus a masked account, not money words alone. Field-label detection tries only a bounded set of splits (trying every split of a long paragraph re-parsed once per word: quadratic). The authored-text cache is bounded and keyed with the earlier message since the same body reads differently against it. A nameless salutation is not evidence of another addressee; Unicode-folded owned-address matching folds ASCII only (`user@BÜCHER.example` is not `user@bücher.example`).
**Rejected**: list headers as kind proof; unbounded split search.
**Where**: mail_kind.rs, suggest_recipients.rs.

### Messages and Now previews read bodies lazily and bound overlap
**Chosen**: preview text loads its own bodies and quote lookback (classifier evidence here would reread whole threads); overlap bounded to four without reordering; a quote-only message is skipped for the latest earlier real message; Attention rows have no history pagination; selecting topics before building rows (filtering people only leaks unrelated history); a person id is an address so the same person in two accounts is two rows. Mode reads are served from a cache rebuilt in the background, since a build reads and classifies every inbox message and takes about a second on a large mailbox. Now reads its independent branches concurrently on SQLite's reader pool. The same mutation reply and list read must describe the same revision.
**Rejected**: serial reads; per-row body rereads.
**Where**: messages.rs, messages_text.rs, now.rs, mode_cache.rs, row_seen.rs.

### Gists, digests and Updates behaviour readers would reverse
**Chosen**: digest letting go acts on exactly what a preview listed (a token hashes the cut and message ids, and a run without one is refused); each thread's Updates mark covers what its earlier mark saw, so another source's mail in the same thread or mail after the cut stays; facts derived before the body arrived are derived again once the body is there; failures counting a let-go toward a mute suggestion are logged only. The mute question returns no sooner than a set interval. Parcel lines: tracked things fold to one line with the latest state deciding the section; needs-a-look mail gets one line per subject template; the sending host shows beside a display name so a borrowed brand shows; carrier-only sources do not repeat the carrier. A breakthrough source leads Needs a look with a suggested to-do; mxr never adds the to-do itself for suggestions, but a verified alert (the receiving provider vouches) becomes a to-do on arrival (D193); the receiver gate runs before acknowledging asks projection, so an untrusted payment task is never made before the Updates scan.
**Rejected**: acting on a recomputed set; auto-creating suggestions.
**Where**: updates.rs, updates_digest.rs, alert_todos.rs, arrivals.rs.

### Arrivals corrections: sender moves win over earlier email moves, older moves cannot undo under newer
**Chosen**: sender-level corrections beat that sender's earlier email moves; a later email move still wins for that email; an older move cannot be undone while a newer one of the same email (or sender) stands, because restoring the older "before" would discard the newer. The Now line's window is the visit before this one, never more than 24 hours back; a second visit this soon after the first is the same visit so a double mount never empties the line. Reads re-evaluate only the requested cached mail and leave first placement, moves, tasks, marks, reminders and provider state untouched. The cap on "Not sure" questions counts asked ones, so an answer never pulls a fourth in.
**Rejected**: last-write-wins undo.
**Where**: arrivals.rs.

### Ask and archive retrieval
**Chosen**: `mxr ask` treats the question as prose searched as plain words; structured filters alone go through the parser; with a narrowing filter every word must match, so mail with one stray word cannot fill the pool; Trash and Spam never reach the model as evidence; `executed_mode` is reported honestly (Lexical when semantic returns nothing); candidate fusion uses reciprocal rank fusion; filters apply up front so the model only sees in-scope messages; the `to` filter is honoured (it was once ignored). Retrieved messages and the question are separated by untrusted-content delimiters; the citation validator is the boundary.
**Rejected**: parsing the question as query syntax; ignoring `to`.
**Where**: archive_ask.rs, expert.rs.

### Classifier evals are deterministic and clock-independent
**Chosen**: due dates compare as day differences against the email's own date, never a calendar day, because the seed clock's hour moves the day a mail was sent on; label date words are read with the to-do code's own reader and unreadable words count as present, never exact; items and rows pair by content order (never position) with exact assignment up to a limit and a greedy order after it, ties going to the first content-ordered pairing; output order is stable whatever order the store returned; scenario labels match only the scenario account's email from the scenario's sender, since demo accounts number mail from 1 and reuse people; a wrong who is not a to-do doubt and the reverse. Eval commands save, send and log nothing and the humanizer rewrite is skipped.
**Rejected**: calendar-day comparison; positional pairing.
**Where**: classify_eval.rs, classify_eval_asks.rs, compose_eval.rs, compose_corpus.rs.

### Microsoft To Do transport is minimal and never retries an uncertain write
**Chosen**: the optional CLI is the only transport; never invoke a shell, parse task text, prompt for credentials, or retry an uncertain write; patch only shared fields and never replace notes, recurrence, reminders, steps, categories, importance, list membership or linked resources; inspect the advertised contract before creating anything (schema 2 predates deferral); discard child output with bounded memory because a capped reader can close the pipe early or strand a verbose child on stderr; absent credentials never hold up email sync. An overdue date is set directly because the phrase parser refuses a past day.
**Rejected**: whole-list import, task deletion, email-body export.
**Where**: microsoft_todo.rs, microsoft_todo_cli.rs.

### Misc runtime choices
**Chosen**: `[search] max_results` of zero reads as one row (zero returns empty pages with `has_more` set and clients page forever); the reply quote text render is memoised because bodies are immutable after sync; unknown-mode keymap guides must match both clients so no client advertises a key that does something else; reply-later joins the queue when its time comes and is re-sorted because the join may reshuffle; remote admission validates the original envelope owner after authentication, never from the selected host; revocation closes the lease at the handler's own clock read; saved-search unread counts record a zero on parse failure so the tab strip does not go silent.
**Where**: mod.rs, reply_later.rs, remote.rs, keymap.rs, platform.rs.
