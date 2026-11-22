# Providers, sync and transport

Paths are relative to the repository root.

### IMAP network waits are bounded per response, with a slow bound for STATUS and QRESYNC
**Chosen**: every IMAP command has a deadline. Streaming commands (`UID FETCH 1:*`) use an inactivity bound that each arriving response resets, so large healthy transfers are not cut off. `STATUS` and `SELECT (QRESYNC)` get a slower bound. IDLE init and DONE are bounded too.
**Rejected**: one total deadline per command (breaks large initial syncs); no bound on the raw IDLE session (a half-open connection wedges the account's idle task with no event, no error, no reconnect); an aggressive STATUS deadline (a total sync outage is worse than a slow listing).
**Where**: `crates/provider-imap/src/session.rs`, `collect_bounded`, `list_folders`, the IDLE watcher init/DONE.

### An IMAP timeout fails the sync; only a tagged NO/BAD falls back to plain SELECT
**Chosen**: a QRESYNC timeout aborts that sync and the next tick retries on a fresh session. A tagged NO/BAD leaves the connection consistent, so a plain SELECT on the same session is safe. A STATUS failure on one folder yields that folder with unknown counts; a STATUS timeout aborts the whole listing.
**Rejected**: falling back to SELECT after a timeout (late untagged QRESYNC lines parse as SELECT's reply and corrupt the folder cursor); aborting LIST on any STATUS error.
**Where**: `crates/provider-imap/src/lib.rs` QRESYNC branch of the delta sync; `session.rs` `list_folders`.

### IMAP deletion detection diffs `UID SEARCH ALL` on every non-QRESYNC path
**Chosen**: CONDSTORE-only servers and the first delta after a legacy cursor run `UID SEARCH ALL` and diff against the UIDs believed present at the last sync (1..old.uid_next-1).
**Rejected**: trusting CHANGEDSINCE alone, which never reports expunges.
**Where**: `crates/provider-imap/src/lib.rs`, `delta_sync_folder`.

### IMAP treats an omitted UIDNEXT as unknown and rescans `1:*`
**Chosen**: a SELECT that omits UIDNEXT maps to 0; the folder rescans `1:*` and relies on `seen_uids` plus the store's upsert for dedup.
**Rejected**: trusting 0 (it can never satisfy `new > old`, so the folder would never fetch new mail).
**Where**: `crates/provider-imap/src/lib.rs`, `delta_sync_folder`.

### IMAP refuses a move or delete without MOVE or UIDPLUS before touching the server
**Chosen**: without MOVE a move is COPY plus delete, and the delete needs UIDPLUS to target one UID. A server with neither is refused before COPY, so no partial move leaves a duplicate that grows on each daemon retry. Bare EXPUNGE is never used for the same reason (it removes every `\Deleted` message).
**Rejected**: COPY first and fail on the delete.
**Where**: `crates/provider-imap/src/lib.rs`, `move_selected_message` and the delete helper.

### IMAP rejects a mutation whole if any keyword atom is unsafe
**Chosen**: before any `UID STORE`, every custom keyword must be a safe RFC 3501 flag atom; otherwise the entire mutation is refused.
**Rejected**: passing keywords through (`uid_store` does not validate; a crafted keyword injects command tokens such as `\Deleted`); dropping only the bad atom.
**Where**: `crates/provider-imap/src/lib.rs`, `apply_set_keywords`.

### IMAP does not advertise server-side search
**Chosen**: the adapter reports `search_remote` false; all search uses the local index.
**Rejected**: advertising it (the core default `search_remote` errors, so the daemon would route searches into an error).
**Where**: `crates/provider-imap/src/lib.rs`, capabilities.

### Generic IMAP backfill reports no exact remaining count and never passes the frozen horizon
**Chosen**: the backfill walks a UID range downward from a horizon frozen at SELECT time; mail above it is left to the first delta. Progress reports no count, because UIDs are sparse.
**Rejected**: `EXISTS` as the denominator (it counts the whole mailbox, not what is left); a `SEARCH` per page for the true count (cost); advancing past the horizon (mail arriving mid-backfill would be skipped).
**Where**: `crates/provider-imap/src/generic_backfill.rs`, `discover_uids`; `crates/provider-imap/src/lib.rs`, `gmail_all_mail_page`.

### A parse failure floors the folder's `uid_next` instead of skipping the message
**Chosen**: after a batch, `uid_next` is floored to the lowest UID that failed to parse, so the next delta re-fetches it.
**Rejected**: advancing the cursor past it (the message is lost silently).
**Where**: `crates/provider-imap/src/lib.rs`, `floor_uid_next_to_failed`.

### IMAP falls back to INTERNALDATE when a message has no parseable Date
**Chosen**: use the server's INTERNALDATE, so spam, bounces and drafts get a stable time.
**Rejected**: `Utc::now()` (re-sorts to the top and changes on every resync).
**Where**: `crates/provider-imap/src/parse.rs`, envelope conversion.

### IMAP skips a bare `text/calendar` part when filling `text_plain`
**Chosen**: the calendar is captured into `metadata.calendar`; the part is not also written as body text.
**Rejected**: mail_parser's `PartType::Text` default (raw `BEGIN:VCALENDAR` lands in `text_plain`).
**Where**: `crates/provider-imap/src/parse.rs`, body extraction.

### IMAP keeps keyword atoms exactly as the server wrote them
**Chosen**: no case folding; system flags go to the bitfield and everything without a `\` prefix stays a custom keyword.
**Rejected**: normalising (round trips back to the server would change the on-the-wire form).
**Where**: `crates/provider-imap/src/parse.rs`, `flags_and_keywords_from_imap`.

### Gmail uses its internalDate as the received time and drops out-of-range values
**Chosen**: `internalDate` is the canonical received time. An out-of-range or ambiguous value returns `None` and is logged, and the caller falls back to the parsed `Date:` header.
**Rejected**: stamping `Utc::now()` (corrupts sort order and threading with no trail); trusting sender-controlled `Date:` headers first.
**Where**: `crates/provider-gmail/src/parse.rs`, `parse_internal_date`, `gmail_message_to_envelope`.

### Gmail never retries a send that may have been dispatched
**Chosen**: once HTTP dispatch may have begun, a timeout, 5xx or unreadable response is ambiguous. A retry is never authorised from error text. A 429 produced by mxr's own held gate (before auth and HTTP) is the only retryable case, and it is typed. A deferred reply send re-resolves its parent thread and fails rather than send into the wrong conversation. Absence from Gmail's index never proves non-acceptance.
**Rejected**: retrying on error text; sending a reply without a thread lookup.
**Where**: `crates/provider-gmail/src/provider.rs`, `classify_send_error`, `send_classified`; `client.rs` rate-limit gate.

### Gmail rate limits are a typed error, not an auth failure
**Chosen**: a 429 from the token endpoint or Gmail becomes a typed rate-limit error carrying the wait. It stays out of the "Auth error" label so sync health does not tell the user to sign in again. It is reported before any mail request exists, so a send is never retried because of it.
**Rejected**: mapping 429 to auth failure.
**Where**: `crates/provider-gmail/src/client.rs`; `auth_refresh.rs`; `error_response.rs`.

### Gmail token refresh is serialised, never opens a browser, and treats persistence failure as non-fatal
**Chosen**: cache lookup, refresh and persistence run under one lock so simultaneous API calls reuse the new token. Background acquisition never starts an interactive flow. If the new token is live but persisting it failed, the grant is intact and no sign-in is requested. The refresh-token client is built once with a 30s total timeout inside `access_token()`'s 60s bound; device-flow polls get 60s because yup-oauth2 treats a poll transport error as flow-fatal.
**Rejected**: parallel refreshes of one grant; a per-call client (loses its connection pool); equal bounds (non-deterministic error).
**Where**: `crates/provider-gmail/src/auth_refresh.rs`; `auth.rs` `with_refresh_token_at`.

### Receiver verdicts need one aligned signature and the earlier of two signed clocks
**Chosen**: a sender signature must be exactly one aligned signature, failed ones included, so adding bogus signatures cannot pick a new identity for an intact replay. Both signed clocks must be non-future and the earlier is used, so old evidence is never freshened. The pin is checked before DNS and again against the validated chain.
**Rejected**: first-match signatures; using the later clock.
**Where**: `crates/mail-parse/src/receiver_arc.rs`, `verify_inner`.

### Delivered-To is read only from a column-0 header scan that we delimit ourselves
**Chosen**: `Delivered-To:` counts only when the block is well formed. We normalise line endings, split at the first blank line, skip folded continuations, count only the first physical line, and return empty on anything ambiguous. The residual risk is bounded: the caller picks only among the account's owned addresses, and the effective From shows at dry-run and send-confirm.
**Rejected**: a permissive parser (it strips leading CRLF/WSP and recovers a missing separator, which promotes a sender-controlled body line); rewriting an invalid header name when serialising (it would synthesise a real header, so the pair is dropped).
**Where**: `crates/mail-parse/src/lib.rs`, `delivered_to_addresses` and the header serialiser.

### Sync batches commit once per chunk, and the lexical index commits before counts and cursor
**Chosen**: after a batch SQLite has the envelope and body and Tantivy the lexical corpus. Semantic chunk prep is left to the daemon's post-sync step. Writes use the store's batched path (one transaction per chunk). The page is consumed so two copies of each body are never alive. Reply pairs resolve after the batch so same-batch parents pair directly. Labels are skipped during backfill; counts are recalculated every batch.
**Rejected**: about eight commits per message through the single writer; semantic prep inside sync.
**Where**: `crates/sync/src/engine.rs`, `sync_account_reporting`.

### Sync reads stored ids back and tracks vacated threads
**Chosen**: the store keeps the id an existing row already has, so the engine reads ids back off the page and the index, reply pairs and daemon list use those. A message that moves threads adds its old thread to the touched set, tombstoned if emptied.
**Rejected**: using provider-supplied ids (rows and index would disagree).
**Where**: `crates/sync/src/engine.rs`, `sync_account_reporting`.

### Reading and Paper trail mail keeps INBOX at screening
**Chosen**: they are views over the inbox, so their mail keeps INBOX like allowed mail.
**Rejected**: dropping INBOX (the sender's new mail leaves the place the user just moved them to).
**Where**: `crates/sync/src/engine.rs`, `apply_screener_decision`.

### Outbound HTML from a caller goes out as base64 bytes; mxr-generated HTML stays quoted-printable
**Chosen**: a caller's designed HTML is base64 and passed as bytes so it round-trips exactly. Inline assets wrap the alternative in multipart/related. Invite replies omit the text/html half because calendar servers match only the calendar part. Inline assets are loaded up front so each future owns its inputs.
**Rejected**: quoted-printable and string bodies (lettre canonicalises line endings to CRLF); multipart/mixed (inline images render as attachments); borrowing `&InlineAsset` across the await (rustc rejects it under the boxed `#[async_trait]` return type).
**Where**: `crates/outbound/src/email.rs`, `build_message_with_id_and_parts`; `attachments.rs`, `load_inline_assets_async`.

### Outbound HTML validation refuses what it cannot check
**Chosen**: over the parser budget, or if the HTML helper died or timed out, the document is refused. The nested parses run on parts of the same document. `<animate>`, `<foreignObject>` and SVG animation are refused outright. A content-ID that could end a header line is refused. The plain-text alternative always has a tag-strip fallback so a sent message always has text.
**Rejected**: a weaker linear scan (it cannot see attribute counts or entities); sanitising (the caller holds their exact source and can fix it).
**Where**: `crates/outbound/src/html.rs`, `validate_html`, `generate_text_alternative`; `attachments.rs`, `validate_cid`.

### A non-empty From must parse in every compose mode, and clearing intent keeps the original
**Chosen**: a bad From is a hard error and never falls through to the account primary. Clearing `intent:` keeps the draft's intent. An HTML draft is guarded on the way back in so conversion to markdown cannot orphan its inline assets.
**Rejected**: silent fallback to the primary; downgrading a reply to a new message.
**Where**: `crates/compose/src/lib.rs`, `validate_draft_with_mode`; `draft_codec.rs`, `apply_edited_compose_file`.

### Demo attachment ids are derived and scoped to the account
**Chosen**: ids derive from the message so regeneration is stable, and include the account because both demo accounts number messages from 1.
**Rejected**: unscoped ids (`attachments.id` is the primary key, so the second account to sync steals the first's row).
**Where**: `crates/provider-fake/src/fixtures.rs`, `demo_attachments`.

### TCP transport is loopback-only on both ends
**Chosen**: a bind or connect to a non-loopback address is refused before a socket opens, because the token travels in plaintext. The connector never advertises the token for a non-loopback target. Remote access goes through `dial-stdio` over SSH.
**Rejected**: remote TCP without TLS.
**Where**: `crates/transport/src/tcp.rs`, `bind`, `connect`.

### UDS bind records its socket identity and cleanup never unlinks a successor's socket
**Chosen**: after bind the transport captures the socket file's identity and fails the bind if `stat` fails; a failed bind unlinks its own file. Cleanup unlinks only when the identity matches, because during an upgrade restart a successor can re-bind the path while the old daemon drains. A missing peer credential drops that one connection instead of fabricating a peer or ending the accept loop.
**Rejected**: unconditional unlink; falling back without identity.
**Where**: `crates/transport/src/uds.rs`, `bind`, `accept`, `cleanup`.

### The command transport kills its child on drop and a stopped memory transport refuses connects
**Chosen**: the child gets a kill signal when the stream drops, reaping is best effort, and normal completion does not rely on it. A stopped in-memory listener fails `connect` atomically with enqueue.
**Rejected**: leaking the child; handing back a client whose server end is never accepted.
**Where**: `crates/transport/src/cmd.rs`, `connect`; `memory.rs`, `connect`.

### html-worker keeps the inherited hard limit, runs with a clean environment, and retries once on a fresh helper
**Chosen**: the helper never changes the hard rlimit; the soft limit's SIGXCPU ends it. On memory exhaustion it aborts. The helper gets a cleared environment and its own process group. One retry on a fresh helper tells a bad input from an OS kill; a second death marks the input bad. Slow inputs are not retried.
**Rejected**: raising or lowering the hard limit; retrying slow inputs; sharing the terminal's process group.
**Where**: `crates/html-worker/src/limits.rs`; `supervisor.rs`, `spawn`, `run_blocking`.
