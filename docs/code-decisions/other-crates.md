# Other crates

Paths are relative to the repository root.

### Humanizer catalogue is the single source of positive instructions
**Chosen**: `PATTERNS` in `crates/humanizer/src/catalogue.rs` (Humanizer 3.1) holds the positive instructions once; generation, checking and repair all read it.
**Rejected**: separate per-path instruction lists, because the three paths would drift apart.
**Where**: crates/humanizer/src/catalogue.rs, `PATTERNS`.

### Writer-rate comparison uses cross-products, not rounded rates
**Chosen**: `score_with_rates` compares cross-products against the unrounded historical rate.
**Rejected**: comparing rounded per-thousand rates, which grants a whole occurrence to a short draft for a tiny rate.
**Where**: crates/humanizer/src/lib.rs, `score_with_rates`.

### daemon_lock recovers from mutex poisoning
**Chosen**: `daemon_lock` ignores poisoning. The guarded value is `()`, a serialisation token with no invariants a panicking test could break. The lock also guards the cargo build cache (parallel `mxr` builds thrash) and the macOS `Application Support/<instance>` cleanup.
**Rejected**: propagating poison. One genuine failure poisoned the mutex and every later test died on `daemon lock poisoned`, hiding the real cause.
**Where**: crates/test-support/src/daemon.rs, `daemon_lock`.

### Tests run a uniquely named mxr binary
**Chosen**: `link_mxr_binary` places a hard link (or copy) of `mxr` under a distinct file name per test. The daemon's pid-file fallback (`server::fallback_live_daemon_pid_without_pid_file`) scans `ps` and only adopts a process running the same executable file in the same profile. `mxr demo` uses the fixed `mxr-demo` instance, and tests in one checkout share `target/debug/mxr`, so the binary name is the one discriminator a test controls.
**Rejected**: running the shared binary, which lets a test reach another checkout's daemon.
**Where**: crates/test-support/src/daemon.rs, `link_mxr_binary`; `run_with_env` callers isolate `mxr demo` through `HOME`/`XDG_*`, a pinned socket and this binary.

### Timed runner splits output on carriage returns
**Chosen**: `read_timed_lines` splits on `\r` as well as `\n`.
**Rejected**: newline-only splitting, because progress printers rewrite one line with `\r` and a whole run would collapse into one entry with the wrong timestamp.
**Where**: crates/test-support/src/daemon.rs, `read_timed_lines`.

### Gate treats only a literal false confirm as a preview
**Chosen**: for `Rule::Confirm` tools, only a literal `false` or a missing flag is a preview. The string "true" or any other value counts as the real change.
**Rejected**: loose truthiness parsing, which would let a malformed flag read as a preview of a real mutation.
**Where**: crates/assistant/src/gate.rs, `classify`.

### Token-bound approvals show no preview
**Chosen**: when a call carries a `preview_token`, `classify` returns `Gate::Approve { preview: None }`.
**Rejected**: re-running a preview, because the token binds the selection an earlier preview showed and a fresh preview could differ.
**Where**: crates/assistant/src/gate.rs, `classify`.

### Config keeps what the user typed; the daemon resolver validates
**Chosen**: `load_config_from_str` keeps values as typed, including ones the daemon will ignore; validation lives in the daemon's resolver.
**Rejected**: validating in the config crate, which would drop what the user wrote.
**Where**: crates/config/src/lib.rs, test around `load_config_from_str`.

### Secrets live in a 0600 plaintext file, not the OS keychain
**Chosen**: `secrets.toml` (`config_dir()/secrets.toml`, mode 0600) holds password-backed IMAP/SMTP secrets, the same model as `~/.aws/credentials`, `~/.config/gh/hosts.yml`, `~/.npmrc`. Plaintext at rest is deliberate; only filesystem permissions protect it. Opened once with `O_NOFOLLOW`, verified and `fchmod`ed through the descriptor (no TOCTOU). Writes go temp file (0600) -> fsync (`F_FULLFSYNC` on macOS) -> rename -> fsync the directory, with a drop guard that removes the temp file. Empty reads as absent, so an empty value never suppresses the keychain fallback, and `set` refuses an empty secret. In-process writes share a global lock; there is no cross-process write lock because the daemon is the single writer and the CLI only reads. The keychain stays an optional backend; the disk-to-keychain fallback and mirror policy lives in the daemon credential resolver.
**Rejected**: OS keychain as the only store. For an ad-hoc-signed release binary macOS revokes the item's ACL on every upgrade, a non-interactive read returned `errSecAuthFailed`, and that hard-failed daemon startup for password accounts. Encrypting at rest was rejected as out of scope.
**Where**: crates/config/src/secret_store.rs, module and `SecretStore`.

### Secret-store errors never carry TOML error text
**Chosen**: parse and serialize failures in `SecretStore` return sanitized errors. The `toml` error quotes the offending source line, which can hold a real `secret = "..."` value.
**Rejected**: forwarding the `toml` error.
**Where**: crates/config/src/secret_store.rs, `SecretStore::set` serialize and `read` parse paths.

### Deliveries: a tracking-number string alone never creates a delivery
**Chosen**: `detect` creates only when the signal is corroborated by a shipping context (carrier or merchant sender, shipping subject, schema.org, or another corroborating signal). Amazon TBA is the exception: the checksum library validates it with high precision, so it is trusted regardless of sender. A post-delivery review or survey email never creates or resurrects a delivery, and a Create needs a dedup key; otherwise the LLM tries to recover identifiers. The decision ladder favours precision over recall.
**Rejected**: creating from any checksum-valid number, because loose carrier formats (for example DHL eCommerce) match unrelated strings.
**Where**: crates/deliveries/src/lib.rs, `detect`, `CORROBORATING_SIGNALS`.

### Deliveries: deterministic and schema fields win over the LLM
**Chosen**: `merge` in `extract.rs` lets schema.org and deterministic fields stand and the LLM only fills gaps. A tracking number from the LLM is re-validated by checksum before it is adopted.
**Rejected**: trusting model output over structured data.
**Where**: crates/deliveries/src/extract.rs, `merge`.

### Deliveries: the LLM sees the email as untrusted content
**Chosen**: from, subject and body are all attacker-controllable, so `enrich` wraps the whole block in untrusted-content delimiters and relies on strict-JSON parsing plus the checksum re-validation.
**Rejected**: passing the raw email text into the prompt.
**Where**: crates/deliveries/src/extract.rs, `enrich`.

### mailmerge never leaks personalised content and never double-drafts
**Chosen**: render every record before creating any draft (a template error on record 40 must not leave 39 drafts). Build the `Redactor` from every record, not the one being drafted, because mxr's stderr can quote back a body rendered for someone else; failure reasons come from a closed set, so subprocess output cannot steer a property value into the manifest, and stderr is length-capped. Rendered bodies live in a 0700 temp dir removed on drop and each body is deleted as soon as mxr has it. The manifest stores a hash of properties, not values, saved after every record, and a record that already produced a draft is not drafted again (rerun upserts, never appends). A manifest that exists but cannot be read stops the run, because a fresh one would re-draft every record and overwrite the only record. Property hashing separates fields with unit separators so {"ab","c"} and {"a","bc"} do not collide. Duplicate recipients and comma-smuggled addresses (`mxr compose --to` takes a comma list) are rejected before any draft exists. A short row is an error, not a silently dropped column. Send writes its durable checkpoint before the subprocess runs, and a missing scheduler outcome is never turned into an immediate send.
**Rejected**: partial campaigns, appending manifest rows, trusting mxr output.
**Where**: crates/mailmerge/src (main.rs `run`, manifest.rs, mxr.rs, records.rs `Redactor`, send.rs).

### mailmerge templates fail loudly and carry nothing between records
**Chosen**: undefined properties are errors (also in `{% if %}`), the error names the missing properties, no loader is set so includes cannot reach the filesystem, HTML templates auto-escape while subjects do not, and one shared environment keeps no context from the previous record.
**Rejected**: minijinja's lenient undefined behaviour (mails "Hi ,"), and its generic "undefined value" message.
**Where**: crates/mailmerge/src/template.rs, `Templates`.

### Reader: recognise quotes only within budget, and never present someone else's words as the sender's
**Chosen**: HTML quote/signature detection runs only when the markers are present and the document is within the html_limits budget; a lost helper (timeout, crash) leaves the body whole. Marked inline questions and forwarded sections are kept as attributed context. When nothing of the sender's own remains, the reader says so. Reply-header detection locates "wrote:" as an ASCII byte offset in the original header, because `to_lowercase()` can change byte length (U+0130).
**Rejected**: lowercasing the header and reusing offsets; dropping the whole quote.
**Where**: crates/reader/src/html_quote.rs, new_text.rs, quotes.rs, pipeline.rs.

### Reading fetch: every answer public, no loopback allowance, HTML only
**Chosen**: `fetch` refuses a name when any resolved address is non-public, whichever one a retry would pick; the first hop is refused before a socket opens, so an unrelated local server never sees the request. Only declared `text/html` or `application/xhtml+xml` is read; with no declared type the body must look like HTML. Local-use NAT64 (RFC 8215) and Teredo addresses are never trusted.
**Rejected**: allowing loopback, or checking only the first resolved address.
**Where**: crates/reading/src/fetch.rs, address policy and content-type check.

### Records: only schema.org or the user checks a value
**Chosen**: a value from a rule parse is unchecked; only schema.org markup or the user marks a date or value checked. Every rule-read value must be quotable from the email, or it is dropped. A kind word with nothing to file is a notification, not a record. A named kind filters answers. Reviewing an inferred renewal is not proof that a contract renewed, and an estimate never silently closes a review the user has yet to do. Only a finished e-signature files a contract. Several unlabelled amounts yield no total; among labelled totals the largest wins.
**Rejected**: showing parses that drifted from the email's words, and treating inference as confirmation.
**Where**: crates/records/src/rules.rs, fields.rs, answer.rs, pass.rs, renewal_tasks.rs, detect.rs.

### Records: subscriptions are statements-series, with order-independent results
**Chosen**: receipts and invoices recur as subscriptions only after a cadence and amount check; a series is statements only; a subscription needs two or more charges; a price change is measured against the run's first charge, while a band's drift is measured against the previous point. Buckets are ordered by day then id and ties go to the larger record id, so the result never depends on the order the store returned records.
**Rejected**: grouping by issuer alone (it merged one-off orders and two plans).
**Where**: crates/records/src/subscriptions.rs, group.rs, answer.rs.

### Relationship LLM prompts: guard first, mail untrusted, cut on boundaries
**Chosen**: user-only prompts lead with the injection guard, then the task, then excerpts wrapped as untrusted content; strict-JSON parsing plus validation is the boundary. The fixed overhead is reserved before the byte budget cut, and truncation is boundary-safe so multi-byte mail cannot panic the worker. An open circuit breaker skips the contact quietly (it already warned once). A summary write is refused when mail it read was deleted while the model ran; the delete's re-enqueue rebuilds it. A commitment needs you as the speaker, so marketing mail never yields one. Forced rebuilds bypass the background gate. Only lengths and finish_reason are logged, since replies can quote mail.
**Rejected**: warning per contact, logging model output, trusting list mail.
**Where**: crates/relationship/src/commitments.rs, summary.rs, service.rs.

### Rules: poisoned regex cache is recovered; shell hooks are bounded
**Chosen**: a poisoned lock in condition evaluation is recovered, since a panic cannot leave a half-written map entry. String patterns match any body part (a `^...$` rule is about one part's whole text). Shell hooks drain stderr while writing stdin, wait on the shell itself rather than pipe EOF, abandon an unfinished stdin write, and bound reaping separately, with `kill_on_drop` as the fallback.
**Rejected**: propagating poison; waiting for pipe EOF (a backgrounded descendant keeps it open).
**Where**: crates/rules/src/condition.rs, engine.rs, shell_hook.rs.

### Search: the index is derived data; any open failure rebuilds it
**Chosen**: SQLite is canonical and the Tantivy index is rebuildable, so any failure to open an existing index (schema change, tantivy upgrade, corruption) leads to a rebuild instead of an error. Re-indexing is upsert by message id (`delete_term` first), or `mxr search` returns duplicates. Keywords are indexed lowercased as separate untokenised STRING terms so `is:$Forwarded` and `is:$forwarded` match the same set; the case-preserved form lives in the message keyword store. Owed-reply is computed dynamically, not indexed, so its query emits `AllQuery`. Unknown filters or query variants degrade to `AllQuery`/today so mxr never panics.
**Rejected**: failing on index open errors; indexing keywords tokenised or case-preserved.
**Where**: crates/search/src/index.rs, schema.rs, query_builder.rs.

### Search service: one atomic word for rebuild count and epoch, heavy work off the async workers
**Chosen**: rebuilds in progress (low bits) and an epoch (high bits) share one word, so a reader sees both in one load; the incomplete flag is set before the rebuild count so a reader that sees the count also sees the index as incomplete. A query that straddled a rebuild must not pass as clean. Indexing, committing and full-match scans run on the blocking pool, because they are CPU-bound and take seconds to minutes on large indexes. A stopped worker outranks a rebuild. A chunk commit still runs when the batch only removed documents, since those deletes are uncommitted.
**Rejected**: running index work inline (parks a tokio worker for the whole run); separate counters.
**Where**: crates/search/src/service.rs.

### Safety checks are conservative and never expose raw secrets
**Chosen**: any explicit negation suppresses the missing-attachment warning (real wording like "I did not attach the deck"); the HTML the recipient reads is what gets gap-checked, and when the HTML cannot be read the result says so and a retry is the fix. The greeting check reads only the first non-empty paragraph; a greeting naming several people does not fire reply-all. A typed recipient with a prior-history sum of 3 or more counts as strong and suppresses the typo warning, sum 2 allows it. PII issues carry redacted previews only (first 4 characters of a token, never raw digits). Tone results are at most a warning, never a blocker.
**Rejected**: warning on every negation; previews with raw key bytes (an Info-severity PII issue would slip past the daemon's enforce gate).
**Where**: crates/safety/src/attachment.rs, gaps.rs, reply_all.rs, recipients.rs, pii.rs, tone.rs.

### Semantic: the ANN index is rebuilt in the background, with a capped retry budget
**Chosen**: ingest defers the ANN rebuild (`dirty_indexes`) so a backlog drains in linear time; the rebuild runs on a blocking thread from the worker loop, which wakes itself for retry backoff and for swapping in a finished index. A profile is dequeued only once its build is running, stays dirty on failure (the record that the index is stale), and the attempt cap holds even for callers that skip backoff. Only a finished pass hands the attempt budget back; ingest must not. An index older than the last finished pass makes a query wait for the rebuild or fail rather than answer stale. Only a completed pass claims the current model revision; a revision change invalidates every stored vector, so nothing counts as current. Only vectors whose dimensions match the index are loaded, because mixed widths make the cosine distance panic. Rows are paged in so peak memory is the index plus one page, with one row upsert per page so `GetSemanticStatus` shows live progress. A synchronous ingest during a reindex is refused so it cannot overwrite the running pass's Indexing status. Embedding batches keep a message's chunks in one call, and short header chunks are not mixed with full-window body chunks (the ONNX tokenizer pads every row to the longest). Index text is real text only: no OCR of images or PDFs.
**Rejected**: rebuilding per ingest (quadratic), unbounded retries, serving stale indexes silently, OCR.
**Where**: crates/semantic/src/lib.rs (`SemanticEngine`, `start_index_build`, `begin_reindex`), service.rs.

### Todo: dates, identity and sender authority in asks detection
**Chosen**: structured evidence cannot choose between authored dates or contradict one; a date that states its year is that day even when past; a yearless date means the next occurrence, and one said to have passed means the one just gone. Signing and returning one document is one obligation. A bare reference identifies a payment; a full counterparty address keeps accounts on one provider domain apart; a different explicit reference remains a distinct action. A reference header governs the following units until the next one. Unlabelled disagreeing amounts are noise, labelled ones are a guess to confirm; the label closest to the amount decides. A known name mismatch is evidence for a different actor; a greeting alone cannot contradict the sole recipient. The first configured receiver's authentication report is authoritative, including a failure, so a later forged pass cannot replace it. Markup never hides a failure the words report, and a verify request without its link is a notice, not a to-do.
**Rejected**: trusting later or structured evidence over authored text; merging by provider domain.
**Where**: crates/todo/src/asks/*, dates.rs, money.rs, detect.rs.

### Todo timing: failures and verify links surface at once
**Chosen**: a failed collection surfaces immediately whatever date it names; verify links surface at once because they expire in hours; surfacing is never later than arrival; lead time counts working days and skips spring-forward gaps by taking the first moment after, and takes the first pass of a repeated hour.
**Rejected**: scheduling by the stated date alone.
**Where**: crates/todo/src/timing.rs, `lead_time`, `surface_at`.

### Updates: a number is a code only when its words say so
**Chosen**: a value is a sign-in code only when the subject or the lines around it name a code; a spending context ("promo code", "Use this code SAVE2026 for a discount") names none. Letters count only from Steam Guard; a year, money, a percentage, a card or order reference, and a longer word or number are not codes. The value after the last "is" or colon of a sentence is the code. A message that gives a code lasts as long as the code, and a sign-in alert keeps two days. Lookalike-domain checks compare the registrable domain only.
**Rejected**: treating any digit run near a trigger as a code.
**Where**: crates/updates/src/code.rs, fact.rs, trust.rs, numbers.rs.

### Prompt hardening is a hint; the boundary is downstream validation
**Chosen**: every LLM feature that embeds mail-derived text uses one shared delimiter and untrusted-mail guard preamble (`UNTRUSTED_MAIL_GUARD`, `docs/reference/ai-email.md` principle 8: "Retrieved mail is data, never instructions"). The preamble is defence in depth, since small local models often ignore it. The enforcement boundary is strict-JSON parsing that rejects non-conforming output, citation validators that reject ids outside the retrieved set, checksum re-validation of extracted values, and the no-auto-action invariants (summaries and briefings render as plain text; drafts are written, never auto-sent; no auto-CC, auto-forward or auto-send). Wrapping is structural: an end marker inside an email cannot close the delimiter early.
**Rejected**: relying on the preamble as the control.
**Where**: crates/llm/src/lib.rs, `UNTRUSTED_MAIL_GUARD` and the wrap helper.

### LLM client never follows redirects
**Chosen**: the HTTP client does not follow redirects and reports why it stopped.
**Rejected**: following them, because a redirect would resend the prompt (mail and history) to the host it names.
**Where**: crates/llm/src/lib.rs, client builder.

### Fallback routing: probe tickets, shared breaker and budget shares
**Chosen**: the background breaker is shared across features and checked at most once per call; a half-open probe that fails re-opens at double the cooldown. A successful model listing proves the server is up, not that completions work, so the real request keeps the probe's ticket and a failing completion doubles the wait. A call that began before a config reload says nothing about the new endpoint. When every route is out, background work waits for the soonest route; a waiting person gets one request to the soonest-recovering route. Each target gets its share of the remaining background budget, prompts are built once so they fit the smallest window in the chain, and the user's own endpoint gets the request as written while hosted routes add room for thinking. A hung endpoint must not pin a background worker for the full foreground budget. The breaker state is saved across restarts.
**Rejected**: one global timeout; marking a route bad on a request-level failure.
**Where**: crates/llm/src/background.rs, health.rs, routed.rs, lib.rs.

### MCP client fails fast on a non-correlating frame
**Chosen**: a frame whose id does not match the request fails the call.
**Rejected**: the old loop that skipped frames with id other than 1 and kept reading, which risks hanging. The case is unreachable today (one request per connection, id pinned).
**Where**: crates/mcp/src/lib.rs, request/response loop.

### Workspace crates are seams, not library APIs
**Chosen**: mxr ships via Homebrew, GitHub Releases and `cargo install --git`, not crates.io. The internal `mxr-*` crates are organisational seams, not library APIs.
**Rejected**: publishing each crate with its own semver contract.
**Where**: tests/workspace_boundaries.rs (the publish-flag check). See AGENTS.md.

### Subscription amounts within a small fraction are the same price
**Chosen**: charges whose amounts are within a fixed fraction count as one price (card FX and rounding), and a price rise or cut counts as plausible within 60%.
**Rejected**: exact amount equality, which splits one subscription into many on FX drift.
**Where**: crates/records/src/subscriptions.rs, the price tolerance constants.

### Todo detection reads only the top of a message body
**Chosen**: body text past a fixed length is not read, because what a to-do asks is near the top.
**Rejected**: scanning the whole body, which finds quoted and footer text.
**Where**: crates/todo/src/detect.rs, the body length cap.

### An LLM refusal or empty answer is about the request, not the route
**Chosen**: a refusal or an empty answer from Anthropic is classified as a request-level failure, so the route is not marked unhealthy.
**Rejected**: counting it against route health, which would cool down a working API.
**Where**: crates/llm/src/anthropic.rs, failure classification.

### Statements form a series only across several different months
**Chosen**: a series needs a minimum number of statements in that many different months.
**Rejected**: grouping receipts and invoices by issuer alone (covered by the subscriptions entry above).
**Where**: crates/records/src/group.rs, the series thresholds.
