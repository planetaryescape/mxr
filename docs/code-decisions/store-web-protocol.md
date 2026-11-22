# Store, web bridge, protocol, core and client

Paths are relative to the repository root.

### A retained archive review is reloaded from the journal, never rebuilt from the caller's buffer
**Chosen**: Reload the retained review by operation id.
**Rejected**: Trusting the caller buffer: a lost buffer would create a new identity for the same operation.
**Where**: crates/client/src/archive.rs `wait_archive`

### Draft attachments attach with the reviewed owner's revision, before any send or provider effect
**Chosen**: Attach resources under the revision the owner reviewed, ahead of any send.
**Rejected**: Attaching after effects start, which could act on an unreviewed revision.
**Where**: crates/client/src/draft_attachment_upload.rs `tests`

### Byte reviews follow the session: kept on the same connection, dropped on a different one
**Chosen**: Carry draft attachment previews over to a fresh connection only when it is the same daemon connection.
**Rejected**: Always carrying them: reviews for a different daemon would be trusted without being reviewed again.
**Where**: crates/client/src/lib.rs `is_paired_remote`

### Connection setup is sequential and hidden; ordinary request ids stay unchanged
**Chosen**: Authenticate, then declare the cooperative profile, with setup requests on id 0; neither releases events ahead of its ack.
**Rejected**: Consuming ordinary correlation ids in setup, which would shift bridge-supplied start ids.
**Where**: crates/client/src/lib.rs `setup_request`; crates/client/src/lib.rs `with_default_timeout`

### Pairing and proof handshakes against a saved remote origin use a 30 second timeout
**Chosen**: SETUP_TIMEOUT and the remote connection timeout are 30s.
**Rejected**: Five seconds: it was not enough for a loopback gateway reached through a proxy and tailnet relay.
**Where**: crates/client/src/remote_connection.rs `authority`; crates/client/src/selected_daemon.rs `SETUP_TIMEOUT:`

### Proof cleanup recreates only an unapproved proof record
**Chosen**: When start was lost or expired, rebuild just the unapproved record.
**Rejected**: Touching approved records during cleanup.
**Where**: crates/client/src/remote_connection.rs `retire_pairing`

### mxr-client re-exports TransportAddr
**Chosen**: Consumers select a daemon through mxr-client.
**Rejected**: A direct mxr-transport dependency in each consumer. The web bridge keeps its Unix-only resolver.
**Where**: crates/client/src/lib.rs `daemon_autostart_enabled`

### Owned-address matching folds ASCII case only
**Chosen**: ASCII-only folding for From validation and reply membership.
**Rejected**: Unicode folding: one spelling would be dropped and the other would stop validating as a sender.
**Where**: crates/core/src/lib.rs `owned_addresses_match_ignoring_case_and_whitespace_and_keep_primary_first`

### The owned-address set fails closed: a failed rebuild keeps the old sets and the readiness gate reads false
**Chosen**: Sync writes Unknown direction when the gate is closed; a pass that began while current keeps answering from its set.
**Rejected**: Writing Inbound or a guess from a stale set.
**Where**: crates/core/src/lib.rs `stale_account_address_lookup_reads_as_not_loaded_until_replaced`

### Natural-time input is rewritten with ASCII-only normalisation so byte offsets stay identical
**Chosen**: Spans computed on the normalised text index straight into what the user typed.
**Rejected**: Unicode-changing normalisation, which would shift offsets.
**Where**: crates/core/src/natural_time/grammar.rs `tokenize`

### A skipped DST wall time is read with the offset in force just before the gap
**Chosen**: The result falls after the change, at the requested time plus the real gap, whatever its length.
**Rejected**: Assuming a one-hour gap.
**Where**: crates/core/src/natural_time/resolve.rs `offset<Tz:`

### Days, weeks and months keep wall-clock time; hours and minutes are elapsed time; arithmetic is checked
**Chosen**: "in 1d" across a clock change is the same time tomorrow. Every step returns an error on overflow.
**Rejected**: Unchecked arithmetic: a phrase with huge parts such as "999999w" would panic.
**Where**: crates/core/src/natural_time/resolve.rs `offset`

### Reply-email strings are plain templates with no Fluent or ICU dependency
**Chosen**: Placeholder syntax stays simple.
**Rejected**: Adding a localisation library for a handful of strings.
**Where**: crates/core/src/i18n.rs `str`

### A draft body is HTML or markdown, never both, and a text alternative needs an HTML document
**Chosen**: A payload carrying both bodies is rejected, even when body_markdown is empty; body_text without body_html is rejected; markdown_source is None for HTML drafts; markdown drafts always emit body_markdown, even empty.
**Rejected**: Precedence rules (ignore an empty markdown key, fall through to empty markdown, empty-string fallback): each silently drops a body or makes safety checks analyse nothing, which is how the wrong message gets sent. Omitting the key on an empty markdown body would break legacy clients that read it unconditionally.
**Where**: crates/core/src/types.rs `deserialize<D:`; crates/core/src/types.rs `itself`; crates/core/src/types.rs `legacy_payload_without_html_keys_deserialises_as_markdown`; crates/core/src/types.rs `carrying_both_bodies_is_rejected_rather_than_resolved_by_precedence`; crates/core/src/types.rs `markdown_draft_carries_body_markdown_at_the_top_level`; crates/core/src/types.rs `draft_fixture`; crates/core/src/types.rs `SendIdentity`

### SyncCursor has a custom Debug that hides the payload
**Chosen**: Logs stay bounded; use provider.describe_cursor for a richer view.
**Rejected**: Derived Debug: it could leak account-scoped tokens that adapters embed in cursors, such as Gmail page tokens.
**Where**: crates/core/src/types.rs `fmt::Debug`

### Link density uses integer arithmetic
**Chosen**: Heavy when link_count * WORDS_PER_LINK >= max(1, words).
**Rejected**: Floating point in the hot path.
**Where**: crates/core/src/types.rs `LinkDensity`

### New wire fields are optional and skipped when empty, so older peers keep decoding
**Chosen**: Additive fields default when missing and are omitted at their default; an older daemon ignores unknown keys and reads the fallback field it always read.
**Rejected**: Required fields or always-emitted fields: they would break older clients and daemons across a protocol bump.
**Where**: crates/protocol/src/lib.rs `variant`; crates/protocol/src/lib.rs `PreLabelRoleRequest`; crates/protocol/src/lib.rs `with`; crates/protocol/src/lib.rs `legacy_status_response_defaults_new_fields`; crates/protocol/src/lib.rs `account_scoped_requests_deserialize_legacy_payloads`; crates/protocol/src/lib.rs `BeforeRevise`; crates/protocol/src/search_degraded.rs `a_daemon_notice_replaces_the_generic_one_and_older_clients_ignore_it`; crates/protocol/src/types/assistant.rs `an_older_daemon_fails_a_step_rows_request_it_does_not_know`

### An unknown account colour slot degrades one account, not the whole accounts list
**Chosen**: A daemon that knows a slot this build does not yields an unknown colour for that account only.
**Rejected**: Failing the list decode.
**Where**: crates/protocol/src/lib.rs `replay_policy_is_not_silently_ignored_by_older_daemons`

### Every ClientKind tag the recorder writes must parse back
**Chosen**: The bridge turns Activity filter text into a ClientKind, so a test walks every tag.
**Rejected**: A missing arm silently drops that client's rows from a filtered view.
**Where**: crates/protocol/src/lib.rs `request_categories_cover_every_case`

### Activity types are duplicated between mxr-protocol and mxr-store
**Chosen**: Each crate has its own Tier and ActivityFilter, with converters at the boundary.
**Rejected**: Sharing one definition: protocol is a leaf crate and would pull in a heavier dependency graph. Filed as an issue because the duplication is a cost.
**Where**: crates/protocol/src/types.rs `ActivityTier`

### Request variants are grouped in core mail, app/platform and admin buckets
**Chosen**: Core mail is the stable contract; admin and maintenance are legitimate daemon features fenced off from it.
**Rejected**: One flat list of variants.
**Where**: crates/protocol/src/types.rs `from_str`

### Daemon Auth errors map to a non-401 HTTP status
**Chosen**: Auth and Provider errors map to a gateway-style status.
**Rejected**: 401: the daemon's Auth kind is the mail provider rejecting its credentials, and a 401 would make the SPA drop a valid bridge token.
**Where**: crates/web/src/auth.rs `code`

### Invalid drafts use the daemon's InvalidRequest code
**Chosen**: The bridge reuses the daemon's draft-validation code.
**Rejected**: A bridge-only code: clients would handle "fix your draft" twice.
**Where**: crates/web/src/auth.rs `IntoResponse`

### The ?token= query path authenticates only the WebSocket upgrade
**Chosen**: Handlers still read `token` for older query shapes, but regular HTTP routes authenticate from the header only.
**Rejected**: Query tokens on every route.
**Where**: crates/web/src/auth.rs `ensure_authorized_with_query_token`; crates/web/src/router.rs `app_with_state`

### All protected /api/v1 routes sit behind one auth layer; only health and the handshake are public
**Chosen**: Handlers keep their own checks as defence in depth.
**Rejected**: Per-handler auth only: a new route could be added without auth by forgetting a call.
**Where**: crates/web/src/router.rs `app_with_state`

### The compression layer must not buffer the event stream
**Chosen**: Only JSON, the SPA and its assets compress; images and text/event-stream are left alone.
**Rejected**: Compressing everything.
**Where**: crates/web/src/router.rs `openapi_json`

### HTTP route limit defaults mirror the protocol's serde defaults
**Chosen**: Constants in the insight, place, record and todo routes repeat the protocol defaults.
**Rejected**: Importing them: they are private to mxr-protocol. An omitted limit must behave the same over HTTP and IPC.
**Where**: crates/web/src/insight_routes.rs `DEFAULT_OWED_REPLY_LIMIT:`; crates/web/src/place_routes.rs `DEFAULT_PLACE_LIMIT:`; crates/web/src/record_routes.rs `DEFAULT_RECORD_LIMIT:`; crates/web/src/todo_routes.rs `DEFAULT_TODO_LIMIT:`

### Sidebar navigation is built across every in-scope account, with the starred flag standing in for a missing Starred label
**Chosen**: System labels and counts come from all in-scope accounts; Starred lists by flag, so an IMAP account without a Starred label still gets an item; an older daemon that ignores all_accounts or lacks the counts request falls back to the default account's data.
**Rejected**: Using only the default account's labels: another account's mailbox would be missing or miscounted.
**Where**: crates/web/src/chrome.rs `build_bridge_chrome`; crates/web/src/chrome.rs `build_sidebar_sections`; crates/web/src/lib.rs `thread`; crates/web/src/mailbox_threads.rs `ThreadLens<`; crates/web/src/mailbox_threads.rs `load_thread_page`

### Only lenses whose IPC request takes an offset paginate
**Chosen**: A subscription drilldown runs through Request::Search and paginates; saved-search and subscription-overview lenses do not.
**Rejected**: Faking pagination over requests with no offset.
**Where**: crates/web/src/chrome.rs `load_mailbox_selection`; crates/web/src/lib.rs `open_commitment_counts`

### Compose autosave and explicit retry write revisions under separate rules
**Chosen**: Automatic saves keep the editor's private destination; only an explicit retry submits its original observed revision; a lost create response reuses the identity written before submission; two unsaved views cannot both act once one saves.
**Rejected**: Letting any save submit a stale revision: a second view would perform effects with text a revised snapshot now owns.
**Where**: crates/web/src/lib.rs `update_compose_session`; crates/web/src/lib.rs `persist_compose_draft`; crates/web/src/lib.rs `submit_compose_draft`; crates/web/src/lib.rs `send_compose_session`; crates/web/src/lib.rs `RestoredComposeSession`

### Compose session files are written token-first, with sidecar metadata, and restored on partial failure
**Chosen**: The token is written before metadata so a crash cannot detach bytes; tokens without metadata block later saves; if the second replace fails the previous coherent file is restored and the accepted snapshot kept; recovery ownership is bridge-written metadata.
**Rejected**: A filename convention for ownership, and metadata-first writes that could leave bytes detached.
**Where**: crates/web/src/lib.rs `update_compose_session`; crates/web/src/lib.rs `send_compose_session`; crates/web/src/lib.rs `discard_compose_session`; crates/web/src/lib.rs `compose_recovery_source`; crates/web/src/lib.rs `write_draft_session_metadata`; crates/web/src/lib.rs `preserve_compose_snapshot`; crates/web/src/lib.rs `refresh_compose_session`

### A restored compose session owns a fresh copy; the stored draft owns its reply mode
**Chosen**: Explicit reopen copies the recovery file; autosave never enters that path; opening from another message or mode must not flip the stored intent.
**Rejected**: Sharing the remembered recovery file with another tab that may still edit or discard it.
**Where**: crates/web/src/lib.rs `set_compose_audience`; crates/web/src/lib.rs `restore_compose_session`

### An HTML draft that the markdown composer cannot edit returns 409 and is refused before any scratch file exists
**Chosen**: 409, not 502: the daemon is healthy and the draft intact, so the client must not retry. The check runs first and the body match is exhaustive. The draft list exposes content_kind so a client knows before opening.
**Rejected**: 502 or a late refusal: clients would retry, and an orphaned compose file would remain on every attempt.
**Where**: crates/web/src/lib.rs `update_compose_session`; crates/web/src/lib.rs `restore_saved_draft_session`; crates/web/src/lib.rs `subscription_summary_view`

### A committed send never fails because of leftover session files
**Chosen**: Cleanup problems become warnings. After a send the session file and invite sidecar go; the attachment directory stays because the stored draft reads those files when the send fires.
**Rejected**: Failing the request: the client would put the reply back and a retry would send it again under a new draft id.
**Where**: crates/web/src/lib.rs `SessionCleanup`; crates/web/src/lib.rs `upload_compose_attachment`

### Safety checks and promise lookups store nothing under the draft id
**Chosen**: The draft built from the file is read-only for these routes; override tokens are keyed by blocker kind.
**Rejected**: Persisting a draft just to check it.
**Where**: crates/web/src/lib.rs `suggest_compose_collaborators`; crates/web/src/lib.rs `save_compose_session`; crates/web/src/promise_routes.rs `record_promise`

### Invite-with-comment sessions persist a trusted triple and rebuild the iTIP reply at send time
**Chosen**: Only (source, attendee, partstat) is stored in the sidecar; the ICS is rebuilt server-side at the send choke point; From is the matched ATTENDEE alias.
**Rejected**: Persisting ICS bytes: they would be trusted from the sidecar.
**Where**: crates/web/src/lib.rs `create_compose_session`; crates/web/src/lib.rs `RestoredComposeSession`

### Reply, forward and invite sessions seed From with the message's own account address
**Chosen**: The daemon-computed default From for that account; New uses the default account's address.
**Rejected**: The global default account's address: a reply on another account would seed an address it does not own.
**Where**: crates/web/src/lib.rs `create_compose_session`

### A draft's kind stays `new` and its intent rides beside it
**Chosen**: `draftIntent` carries reply or forward.
**Rejected**: Changing `kind`: flows that read it would break, and a client could not ask for help with the stored draft.
**Where**: crates/web/src/lib.rs `saved_draft`

### Attachment copies are deduplicated by source, with UUID-named destinations
**Chosen**: Repeated references reuse one copy; equal basenames from different directories get distinct names.
**Rejected**: Name-based destinations that collide.
**Where**: crates/web/src/lib.rs `copy_recovery_attachments`

### The send response carries draft_id and message_id so a client can chain POST /mail/reminders
**Chosen**: The raw receipt is kept for existing readers; a bare Ack leaves the ids null.
**Rejected**: Making clients re-find the sent message.
**Where**: crates/web/src/lib.rs `send_compose_session`; crates/web/src/routes_v6.rs `save_draft_local`

### Omitted search sort defers to the daemon's default_sort
**Chosen**: The bridge sends nothing, the response reports "default".
**Rejected**: Hard-coding a sort in the bridge.
**Where**: crates/web/src/lib.rs `search`; crates/web/src/lib.rs `triage`

### Global sync has its own endpoint; the existing sync route stays scoped to the default account
**Chosen**: Additive endpoint.
**Rejected**: Widening the old route: older SPAs would change behaviour against a newer bridge.
**Where**: crates/web/src/lib.rs `trigger_sync_all`

### The event WebSocket also reads the browser side
**Chosen**: The bridge selects over daemon events and client frames.
**Rejected**: Reading only daemon events: a closed tab would keep the daemon socket open until a send fails.
**Where**: crates/web/src/lib.rs `bridge_events`

### Contact suggestions keep one entry per address, scanning past the limit
**Chosen**: Case-insensitive dedupe; the first row with a name supplies it; the scan reads 20x the limit.
**Rejected**: Cutting at the limit: duplicates would use up slots and names could be missed.
**Where**: crates/web/src/routes_v6.rs `contacts_autocomplete`; crates/web/src/routes_v6.rs `RelationshipProfileQuery`

### Remote browser access never derives trust from forwarded headers or URLs
**Chosen**: The proxy must preserve Host; the bridge credential comes from a private middleware header, never returned; the single-use code is typed into a clean form, never carried in URL or query; the editor shell, filesystem helpers and host admin have no remote authority; native gateway clients never send Origin; form POST Origin `null` is tolerated.
**Rejected**: Trusting forwarded headers, query codes or URL tokens.
**Where**: crates/web/src/remote_browser.rs `browser_security_headers`; crates/web/src/remote_browser.rs `cookie`; crates/web/src/remote_browser.rs `authorize`; crates/web/src/remote_browser.rs `authenticate`; crates/web/src/remote_gateway.rs `session`

### The gateway never drops a stopping notice because the provider also finished
**Chosen**: Both a ready response and an event can arrive in one poll; the notice is sent first.
**Rejected**: Treating the finished provider as the end.
**Where**: crates/web/src/remote_gateway.rs `tests`

### The workspace owner is checked before the body is read
**Chosen**: A stale request cannot create files or upload bytes.
**Rejected**: Reading the body first.
**Where**: crates/web/src/workspace_owner.rs `sidecar_owner`

### Account colour and identity tag are picked and written in one transaction, then stay put
**Chosen**: First-free choice among held values; ON CONFLICT leaves them alone; a rename keeps the stored tag.
**Rejected**: Absolute ranking and re-deriving the tag from the name: renaming Work to Office moved other accounts' tags.
**Where**: crates/store/src/account.rs `insert_account_reserving`; crates/store/src/account.rs `list_accounts`; crates/store/src/account.rs `tag_of`

### Restore compares legacy bundles against their immutable schema, not today's migrated schema
**Chosen**: Backup restore stages the bundle in the schema it was written with, rebuilds data introduced later (thread pages, receipts, labels' roles) and stamps the migrated staging version.
**Rejected**: Comparing against the current schema: older bundles would fail or lose protections, such as an unsubscribe receipt.
**Where**: crates/store/src/backup.rs `inventory`; crates/store/src/backup.rs `populate`; crates/store/src/backup.rs `retire_restored`

### Derived state is published in the same writer transaction that reads its inputs
**Chosen**: Take the writer transaction (BEGIN IMMEDIATE where needed) before reading; settled reads use one SELECT on one snapshot.
**Rejected**: Read-then-write across transactions: a late builder could publish facts from before a read, label, facet or input edit.
**Where**: crates/store/src/analytics.rs `super::Store`; crates/store/src/analytics.rs `backfill_message_list_ids`; crates/store/src/category_rules.rs `snapshot`; crates/store/src/conversation_summaries.rs `conversation_summaries<T:`; crates/store/src/conversation_summaries.rs `tests`; crates/store/src/assessments.rs `save_assessment_facets`; crates/store/src/todos.rs `upsert_accepted_promise_todo`; crates/store/src/todo_resolution_queue.rs `input_is_current`; crates/store/src/message_arrivals.rs `message_reports_delivery_problem`

### Bogus Date headers are floored or cut off, not trusted
**Chosen**: Epoch-0 dates are floored at year 2000; future dates are cut off or treated as undated; arrival time (stored_at) drives freshness.
**Rejected**: Trusting Date: a 1970 message would rank as the stalest thread forever, and a future one would pin a thread to the top of every lane.
**Where**: crates/store/src/analytics.rs `list_stale_threads`; crates/store/src/contacts.rs `list_contact_decay_at`; crates/store/src/desk.rs `desk_conversation_messages`; crates/store/src/thread_preview.rs `thread_previews`; crates/store/src/arrivals.rs `a_date_header_ahead_of_the_clock_arrives_when_mxr_stored_it`; crates/store/src/pool.rs `reader`

### To-do and promise upgrades never merge histories the user has acted on
**Chosen**: Aliases rather than moves; automatic rows only move when nothing took the key; undo revokes suppression; a user row copies no text from mail; the user's latest answer wins.
**Rejected**: Merging: the user's exact Undo snapshots and decisions would be lost.
**Where**: crates/store/src/asks_todos.rs `adopt_asks_payment_identities`; crates/store/src/asks_todos.rs `adopt_asks_same_source`; crates/store/src/asks_todos.rs `fresh_asks_todos`; crates/store/src/asks_todos.rs `upgrade_asks_todo`; crates/store/src/asks_commitments.rs `crate::Store`; crates/store/src/asks_commitments.rs `unique_commitment_todo`; crates/store/src/contact_commitments.rs `resolve_contact_commitment`; crates/store/src/records.rs `edit_in_tx`

### Read before write so common scans take no writer lock
**Chosen**: A read first; the write transaction re-checks.
**Rejected**: Writing unconditionally: it would queue behind the writer.
**Where**: crates/store/src/asks_todos.rs `adopt_asks_reference_identity`; crates/store/src/asks_todos.rs `adopt_asks_same_source`; crates/store/src/mutation_jobs.rs `read_archive_workspace`

### Hot queries are shaped to keep SQLite on the intended index
**Chosen**: Requested ids outermost, UNION ALL for disjoint sets, account prefix enumerating the small table, open rows found by state index.
**Rejected**: Natural SQL: an account index scan makes projection quadratic or scans the mailbox.
**Where**: crates/store/src/asks.rs `ASKS_FOR_MESSAGES:`; crates/store/src/asks.rs `FACETS_FOR_MESSAGES:`; crates/store/src/places.rs `mode_candidates_sql`; crates/store/src/record_subscriptions.rs `tests`; crates/store/src/todos.rs `find_todo_ids_by_prefix`; crates/store/src/conversation_summaries.rs `save_conversation_summaries`

### Deleting a message clears everything derived from it, inside the deletion transaction
**Chosen**: Trigger-cleared projections, receipts, briefings, relationship summaries, detected to-dos, records and decision evidence are cleared or recomputed; the user's own to-dos stay without text copied from the mail; counterparties are read before the transaction and resolved again inside.
**Rejected**: Leaving derived text behind: model text and evidence would outlive the mail.
**Where**: crates/store/src/message_deletion.rs `module`; crates/store/src/message_deletion.rs `delete_messages_and_derived`; crates/store/src/message_deletion.rs `super::Store`

### File and index cleanup owed to a deleted message survives a crash and is cancelled if the id returns
**Chosen**: The cleanup is recorded with the delete; storing a message whose id was deleted before (an IMAP UID reused) removes the pending entry under the cleanup lock.
**Rejected**: Cleanup in memory only: a failed pass or crash loses it. Without the cancel, cleanup would hit a live message's files.
**Where**: crates/store/src/message_deletion.rs `delete_messages_and_derived`; crates/store/src/sync_upsert.rs `apply_sync_upserts`; crates/store/src/sync_upsert.rs `tests`

### Message upsert keeps one row per (account, provider id) and does not clobber prior values
**Chosen**: Version 0.4.52 changed MessageId derivation, so a fresh sync can compute a different id for an existing message; the natural key decides. body_word_count is preserved; first seeing INBOX mail is not an unarchive.
**Rejected**: Trusting the derived id alone: duplicates after the upgrade.
**Where**: crates/store/src/message.rs `StoredEnvelope`; crates/store/src/message.rs `upsert_envelope_tx`

### Label upserts write only on a real change, and counts belong to recalculation
**Chosen**: Sync upserts every label every pass, so identical rewrites are skipped; ON CONFLICT never overwrites counts.
**Rejected**: Rewriting each pass: cached mode views would count changes and the WAL would grow.
**Where**: crates/store/src/label.rs `upsert_label`; crates/store/src/label.rs `list_labels_by_account`; crates/store/src/label.rs `find_labels_by_provider_ids`

### Contacts refresh aggregates on a reader, writes in chunks, and prunes against the messages table
**Chosen**: Phase one aggregates on the reader pool; phase two upserts in chunks with yield_now between them; the aggregate only nominates prune candidates; the refresh trigger debounces on the trailing edge.
**Rejected**: One giant write and sleeping inside the command handler: it starved other writers and backpressured the queue.
**Where**: crates/store/src/contacts.rs `refresh_contacts`; crates/store/src/contacts.rs `zero_direction_counters`; crates/store/src/contacts_refresh_handle.rs `start_with_debounce`

### The sqlite pool uses full synchronous commits and a fixed reader count
**Chosen**: synchronous=Full; six readers; planner statistics after migrations; a named shared-cache memory database with an anchor connection; VACUUM INTO via a scratch connection.
**Rejected**: Weaker durability, or relying on sqlx connection recycling: authority commits must survive power loss and a replaced connection would open an empty memory database.
**Where**: crates/store/src/pool.rs `new`; crates/store/src/pool.rs `optimize_all_tables`; crates/store/src/pool.rs `empty_in_memory`; crates/store/src/pool.rs `run_migrations`

### Open-time repairs run on every open
**Chosen**: Account identity backfill and the unsubscribe receipt foreign key repair run each time.
**Rejected**: Once-only repair: a restored bundle or a dev database edited in place would stay broken.
**Where**: crates/store/src/pool.rs `apply_migration`

### Migrations are additive and forward-only
**Chosen**: New columns default so old rows decode unchanged; the hints migration drops the old per-mode rows.
**Rejected**: Rewriting rows or carrying over rows for cards that no longer exist.
**Where**: crates/store/src/pool.rs `reader`

### One autocommit statement checks the credential and owner at the same fresh snapshot
**Chosen**: Remote authority reads credential and owner in one autocommit statement, and browser launches match it. Malformed proofs consume the retry budget, lost-ACK retries bind to both proofs, one outstanding launch per registration, child sessions cannot delegate.
**Rejected**: Separate reads or the writer connection: a revocation could be missed, or reads would queue behind unrelated background work.
**Where**: crates/store/src/remote_authority.rs `revoke_remote_client`; crates/store/src/remote_authority.rs `claim_remote_pairing`; crates/store/src/remote_authority.rs `cleanup_remote_client`; crates/store/src/remote_authority.rs `tests`; crates/store/src/remote_authority/browser.rs `claim_browser_code`; crates/store/src/remote_authority/browser.rs `tests`

### Safety override tokens are single-use through one atomic UPDATE ... RETURNING
**Chosen**: used_at IS NULL in the UPDATE.
**Rejected**: Select then update: concurrent sends could both use one token.
**Where**: crates/store/src/draft_safety.rs `tests`

### Draft rows update in place, and an uncertain Sending row stays blocked
**Chosen**: Plain UPDATE with created_at absent from SET; legacy Sending rows are not auto-reset; NULL html body falls back to markdown.
**Rejected**: INSERT OR REPLACE would arm the account_id cascade. Age cannot prove a provider rejected the message.
**Where**: crates/store/src/draft.rs `draft_insert`; crates/store/src/draft.rs `list_drafts`; crates/store/src/draft_recovery.rs `tests`

### Mutation receipts count for their own account across the key upgrade
**Chosen**: A receipt stored under the bare provider id still dedupes for the row's account_id.
**Rejected**: Ignoring pre-upgrade receipts: a replay across an upgrade would repeat.
**Where**: crates/store/src/mutation_dedup.rs `record_mutation_applied`

### Unsubscribe claims fail safe
**Chosen**: A row released between two statements reads as a live claim; the caller gets a retryable error.
**Rejected**: Treating it as free: two unsubscribes could run.
**Where**: crates/store/src/unsubscribe_receipts.rs `confirm_unsubscribe`

### The Microsoft promise mirror commits with the local task
**Chosen**: Mirror and local task in one transaction; the linked task is protected before any external effect.
**Rejected**: Separate commits: a failed mirror could undo a Microsoft reopening on the next To do tick.
**Where**: crates/store/src/todo_microsoft.rs `todo_microsoft_export`; crates/store/src/todo_microsoft.rs `mark_todo_microsoft_export_unknown`

### Undated model dates are mirrored undated, not as due dates
**Chosen**: A date from before due words existed is not a due date.
**Rejected**: Nagging on an unverified date.
**Where**: crates/store/src/todo_first_run.rs `promises_for_todos`

### FTS fallback lowercases ASCII on both sides and quotes user text
**Chosen**: Addresses lowercase as the index does; hidden accounts stay hidden; both flag and label exclude hidden mail.
**Rejected**: Passing user text as FTS syntax, or trusting the flag alone.
**Where**: crates/store/src/search_fts_fallback.rs `push_filters`; crates/store/src/search_fts_fallback.rs `super::Store`; crates/store/src/search_fts_fallback.rs `search_fts_fallback`

### Sync cursors are opaque UTF-8 JSON, and an idle sync writes nothing
**Chosen**: The column is TEXT; the adapter parses; an unchanged cursor leaves the row alone.
**Rejected**: Decoding in the store, or rewriting on every idle sync.
**Where**: crates/store/src/sync_cursor.rs `super::Store`; crates/store/src/sync_cursor.rs `list_sync_cursors`

### Wrapped aggregates run concurrently on the reader pool
**Chosen**: Independent aggregates fan out; asymmetry is recomputed per window, not read from the all-time contacts table.
**Rejected**: A serial chain: a large mailbox would block instead of saturating the pool.
**Where**: crates/store/src/wrapped.rs `wrapped_volume`; crates/store/src/wrapped.rs `wrapped_time_patterns`; crates/store/src/wrapped.rs `wrapped_top_contacts`

### The desk uses one selection for preview and write, and settled homes decide ordinary lanes
**Chosen**: Same SELECTION constant; raw response evidence only for pending mail and explicit timers; unknown-direction mail filed as sent counts as yours.
**Rejected**: Separate queries: preview and write could disagree.
**Where**: crates/store/src/desk.rs `SELECTION:`; crates/store/src/desk.rs `desk_messages_sql`

### Thread listing falls back to newest-first for Relevance and restores SQL order after a batch fetch
**Chosen**: Relevance has no meaning for threads; get_threads_batch order is re-sorted.
**Rejected**: Passing Relevance to SQL, or trusting batch order.
**Where**: crates/store/src/thread.rs `list_threads`; crates/store/src/thread.rs `get_thread_envelopes_for_model`

### History reads keep legacy evidence; only the active view depends on current asks
**Chosen**: Retiring an extractor does not hide old records; thread context retains history.
**Rejected**: Filtering history by current extractors.
**Where**: crates/store/src/contact_commitments.rs `list_open_thread_commitments`; crates/store/src/contact_commitments.rs `record_own_promise`

### Derived columns that a query did not select read as unknown, never as a wrong marker
**Chosen**: source_filed_at and source_unread stay unknown and the row keeps the neutral tier.
**Rejected**: Defaulting to a concrete value.
**Where**: crates/store/src/todos.rs `row_to_todo`

### Category override decoding distinguishes missing from explicit null
**Chosen**: Missing is a C1 snapshot; null is a C2 undo of an absent override.
**Rejected**: Collapsing both.
**Where**: crates/store/src/categories.rs `deserialize_prior_manual_decision<`

### The owned-address rule for sender history follows the daemon's
**Chosen**: The accounts row email is always owned, even after another address becomes primary and its row is removed.
**Rejected**: A separate rule: samples would diverge from the daemon's.
**Where**: crates/store/src/contact_style.rs `relationship_contacts_for_messages`; crates/store/src/contact_style.rs `row_to_contact_style`

### Activity counts bucket by UTC day
**Chosen**: Clients format to local time.
**Rejected**: Server-side local time.
**Where**: crates/store/src/user_activity.rs `activity_stats_by_source`
