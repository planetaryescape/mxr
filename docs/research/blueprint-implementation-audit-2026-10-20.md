# Audit the blueprints against the implementation

Use this report to choose implementation work separately from documentation repairs. All five email modes and Now exist in the daemon, CLI, web app and TUI. Their remaining model, evaluation and indexing work does not make the modes themselves unimplemented.

## Scope and evidence

- Completed: 2026-10-20.
- Source inspected in the checkout: `900d41d82d554cb302126fb7394ecc0156fcf98b` (workspace version `0.6.106`).
- Fetched upstream and checked the six subsequent commits through `origin/main` at `404c223d4cb66714fa9750cdd8eefc5c935ab1ee` (version `0.6.108`). In particular, this includes Microsoft To Do linking and further web fixes. The website build and generated-reference checks below use that upstream snapshot.
- Scope: all 39 Markdown files under `docs/blueprint/`, including both chapter-18 files, the index and five supplementary audits; the docs-site sources, navigation, generators and workflows; relevant Rust and web implementation paths and tests.
- The existing `target/debug/mxr` is an old `0.5.8` build. Its help is not evidence that a current command is missing. Current clap definitions and checked-in help snapshots were used instead.
- This is a static implementation audit plus docs generation/build/link checks. Rust tests and real-provider operations were not run. Test code establishes existing coverage, not a new passing test result. No daemon, mailbox, credentials or remote host was changed.
- File/line references below refer to the checkout SHA unless marked upstream. Symbols are included where line numbers may move.

The inventory has three different kinds of open work:

| Classification | Meaning | Action |
| --- | --- | --- |
| Missing or partial implementation | A specified operation is absent, disconnected or incomplete | Implement it, or explicitly amend the requirement |
| Documentation drift | The implementation already supplies the capability, or deliberately uses a different contract | Update the blueprint or website; do not rebuild the feature |
| Deferred or undecided | The blueprint explicitly parks the capability or leaves a product choice open | Keep it out of the current bug backlog unless selected |

There is no useful single completion percentage: these documents mix historical designs, alternatives, current requirements and explicitly future work.

## Fix these first

| Priority | Finding | Evidence and impact | Completion check |
| --- | --- | --- | --- |
| High | C01. Activity privacy switches are disconnected | `crates/config/src/types.rs::ActivityConfig` promises `enabled` and four `track_*` controls. `crates/daemon/src/activity/mod.rs:67` reads only `MXR_ACTIVITY`; `Recorder::spawn` takes no config. The tracking fields have no runtime readers. Setting `activity.enabled = false` does not disable recording. | Exercise config disable and each redaction toggle through the daemon; query persisted activity and Reading engagement. Keep `MXR_ACTIVITY=off` working. |
| High | C02. Custom TUI keybindings are disconnected | `crates/tui/src/keybindings.rs:419`, `load_keybindings`, has no callers. Actual dispatch is in `crates/tui/src/app/input.rs`; help uses the default shown bindings. Chapters 08, 12 and roadmap Phase 2 advertise customization. | Start an isolated TUI with a remap; prove execution, help and palette all use it, including multi-key and conflicting bindings. |
| High | C03. Shell hooks receive empty message fields | `crates/daemon/src/loops.rs:1697-1721` constructs `ShellHookPayload` with an empty sender and subject, no body or attachments, and the current time rather than the message date. The hook process itself works. Chapters 10 and 14 promise the structured message. | Capture stdin from an isolated sync-time hook and compare its fields with the stored fixture message. Preserve timeouts and mail-as-data handling. |
| High | C04. Rule previews and live evaluation differ | `handler/helpers.rs:295-382`, `DryRunMessage`, omits `link_density_inputs`; live `RuleMessage` supplies it in `loops.rs`. Link-density previews therefore use the default empty inputs. `RuleEngine::dry_run` always uses `evaluate`, while treatment execution uses `evaluate_known` (`crates/rules/src/engine.rs:81-89,117-140`). | Feed identical fixtures through preview and execution, including linked bodies, missing bodies and negated conditions; assert identical selections. |
| Medium | C05. Log size is unbounded | `crates/daemon/src/lib.rs::init_tracing` opens one append-only `logs/mxr.log`. There is no rotation and no runtime use of `logging.max_size_mb` or `max_files`. `commands/logs.rs:123-126` incorrectly assumes the file is bounded while buffering matches. A006/D039 and chapter 18 specify rotation. | Force rollover at a small size, check the retained-file cap, and exercise follow/read/purge across rollover. |

These are supported by concrete call paths, not just missing names in a search. C01 and C04 affect privacy and the preview/execution safety contract respectively.

## Other missing or partial runtime work

| ID | Requirement | What exists and what is missing | Evidence |
| --- | --- | --- | --- |
| C06 | Automatic operational-history retention (A006/A009, chapter 18, D074) | Manual `mxr logs --purge` reads `logging.event_retention_days` and prunes event rows plus old log files. No startup/daily operational event/sync-history retention loop was found. Activity retention is a separate, implemented loop. | `crates/daemon/src/commands/logs.rs:40-84`; loop inventory in `crates/daemon/src/loops.rs`, including `activity_prune_loop` |
| C07 | Honor documented settings (chapter 12) | Several parsed settings have no runtime consumers; see the explicit table below. This is not true of every config field. | `crates/config/src/types.rs`; searches for each field outside config serialization and `commands/config.rs` |
| C08 | Search/rules reuse reader-cleaned text (05, 07, D011) | Dense body chunks use the reader. Lexical indexing writes plain text and raw HTML, and live rule bodies use plain text or HTML directly. The shared cleaned-text guarantee is incomplete. | `crates/search/src/index.rs:334-338`; `crates/semantic/src/lib.rs:1779-1786`; `crates/daemon/src/loops.rs::RuleMessage` |
| C09 | FTS5 fallback when Tantivy is unavailable (02, D004) | `messages_fts` and its maintenance triggers exist, but no application query reads that table. The fallback is not implemented. | `crates/store/migrations/001_initial.sql:195-216`; no `messages_fts` reader in Rust sources |
| C10 | Coalesce provider batch mutations (03) | Gmail advertises batch operations and implements `batch_modify_messages`, but that helper has no callers. The daemon applies provider mutations one message at a time. | `crates/provider-gmail/src/client.rs:313`; `provider-gmail/src/provider.rs`; `crates/daemon/src/handler/mutations.rs:736-784` |
| C11 | Repair missing label associations per account (04 and sync/index audit) | Repair compares a per-account message count with the global junction-table count. Labels belonging to another account suppress the advertised repair for an empty account. | `crates/sync/src/engine.rs:618-638`; `crates/store/src/message.rs:995-999`, `count_message_labels` |
| C12 | Sync-history diagnostics (04) | Doctor supplies current failure status, last success and recent events, but not the promised sync-duration trends or recurring-error analysis of `sync_log`. | `crates/daemon/src/commands/doctor.rs`; `crates/store/src/sync_log.rs::get_last_sync` has only test use |
| C13 | Config-file rule definitions (10) | `mxr rules` and SQLite rule persistence work. `MxrConfig` has no `rules` collection, so the documented `[[rules]]` definition path is absent. The sample `is_read` condition also disagrees with the serde `is_unread` variant. | `crates/config/src/types.rs::MxrConfig`; `crates/rules/src/condition.rs`; `crates/daemon/src/commands/rules.rs` |
| C14 | Idempotent rule execution (10) | Flag/label actions can converge, but repeated Snooze execution inserts a fresh row and ShellHook executes again. A new mutation UUID per execution is not a stable deduplication identity. The blanket idempotency claim needs implementation or narrowing. | `crates/daemon/src/loops.rs::execute_rule_action`; `crates/store/src/mutation_dedup.rs` |
| C15 | Per-account polling interval (04) | Polling is driven by global `general.sync_interval` and clamped to at least 30 seconds. There is no account interval setting. | `crates/config/src/types.rs::GeneralConfig`; `crates/daemon/src/state.rs::sync_interval_secs`; `crates/daemon/src/loops.rs` |
| C16 | Distinct CLI daemon/auth exit codes (09) | The documented codes 3 and 4 have no corresponding mapping. The process has generic error handling and particular code-2 cases, not the promised daemon/auth taxonomy. | `crates/daemon/src/main.rs`; `crates/daemon/src/commands/mutations/compose.rs`; clap definitions |
| C17 | Subscribe to log streams (A006) | Logs can be read/followed, but there is no IPC log-subscription contract. Do not confuse following a file with a daemon subscription. | `crates/protocol/src/types.rs`; `crates/daemon/src/commands/logs.rs` |
| C18 | Explicit warning for unsanitized reports (18) | `--no-sanitize` bypasses sanitization without the specified risk warning. Sanitized reporting itself is implemented. | `crates/daemon/src/commands/bug_report.rs`, `no_sanitize` branch |
| C19 | Complete portable API contracts (23/P04) | Draft conflict and resource schemas exist, generated compose types are consumed, and conformance tests exist. Many route entries still provide only metadata: for example, the committed mailbox and search GET responses describe 200/401 without response content schemas. Route reachability is not complete request/response-schema coverage. | `crates/web/src/openapi.rs:19-38`; `site/public/openapi.json`; `crates/web/src/tests.rs::draft_revision_routes_describe_requests_successes_and_conflicts`; `.github/workflows/openapi-conformance.yml` |

Rule dry-run also reads only the first 10,000 envelopes per account (`handler/helpers.rs:244`). If `--all` is meant to cover the whole mailbox rather than all rules, pagination and completeness reporting need explicit treatment. This is a scope ambiguity, separate from the confirmed C04 evaluation mismatch.

### Parsed settings without runtime effect

| Setting | Observation |
| --- | --- |
| `activity.enabled` | Recorder enablement comes only from `MXR_ACTIVITY` (C01) |
| `activity.track_link_clicks` | No runtime reader; this audit does not infer that URL recording occurs merely because the option exists |
| `activity.track_subjects` | No runtime reader to enforce the advertised subject opt-out |
| `activity.track_recipient_handles` | No runtime reader to enforce the advertised recipient opt-out |
| `activity.track_search_queries` | No runtime reader to enforce the advertised query opt-out |
| `search.default_sort` | Read/write config surface exists; search does not use it as the default |
| `search.max_results` | Read/write config surface exists; search does not use it as the limit default |
| `humanizer.apply_to_summaries` | No runtime reader; `apply_to_drafts` is used |
| `logging.max_size_mb`, `logging.max_files` | No runtime rotation consumer (C05) |
| `logging.stderr` | Not used by tracing initialization |
| `appearance.sidebar` | Not used to control TUI sidebar rendering |
| `appearance.date_format`, `appearance.date_format_full` | TUI formatting does not read these values |
| `appearance.subject_max_width` | Not used for TUI subject sizing |

Do not include `logging.event_retention_days` in this list: manual purge reads it. `activity.retention.*` is also used. `search.semantic.max_pending_jobs` and `query_timeout_ms` are reserved/inactive, and blueprint 12 already says so.

Not every parsed key is exposed through `mxr config set`; C07 concerns the config contract as well as the CLI's get/set subset.

## Email modes: what remains versus what already ships

### Shared work still missing

| ID | Missing slice | Evidence and boundary |
| --- | --- | --- |
| M01 | Rules-only classification/extraction evaluation: `mxr modes eval`, `--sample`, `--extract`, precision/recall reporting and the badge accuracy gate | `crates/daemon/src/cli/mod.rs::ModesAction` and `commands/modes.rs` implement Explain, Hint, Rail, Why and Done only. This is promised in chapter 22 phases 1/2 as well as phase 7; the website already names the command as a future badge gate. |
| M02 | Fast/smart tier routing and mode-specific extraction | No `llm.tiers.fast`, `llm.tiers.smart`, escalation table or mode-extraction tier config. `crates/llm/src/lib.rs::LlmFeature` has existing features such as ArchiveAsk and Commitments, not the new mode classifier/extractor contract. Existing per-feature LLM overrides are not the same feature. |
| M03 | Phase-7 model aspects/classification, hash cache and model-history horizon | No `ModeAspects`, `mode_aspects` table, `modes.model_history_days` or `allow_cloud_background_classification` implementation. This is deliberately future work, not evidence that deterministic modes do not ship. |
| M04 | Remaining per-mode index recipes and mail-mode search filter | `crates/semantic/src/recipes.rs` defines only the Messages prototype, intentionally not activated before retrieval evaluation. To do instructions, Updates facts, Reading sections/lazy embedding, mode tags and recipe/classification versioning are not wired. Archive is an implemented exception outside that table: `load_chunk_preparation_input` reads record fields, `build_chunk_records` inserts their header chunk, and `handler/records.rs::reindex_record_sources` enqueues them after filing. `SearchMode` means lexical/hybrid/semantic, not Messages/To do/etc. |
| M05 | N2 security-alert never-bury override (D119) | Sign-in and failed-payment detection exists in `crates/updates/src/fact.rs`, but `handler/now.rs` reads the bounded digest rather than enforcing a separate security override. The published never-bury copy only promises the implemented N1 people rule (`crates/protocol/src/types/arrivals.rs::NEVER_BURY`). |

The evaluation and recipe switches have explicit acceptance gates. Do not turn on a prototype recipe just to make a checkbox green; establish retrieval results first.

### Shipped mode inventory

| Mode | Confirmed implementation | Remaining slices |
| --- | --- | --- |
| Now | Four fixed capped sections, shared people-turn counting, relevancy filtering, overload/next-item copy, first-run data, coming-up records, arrivals accounting, per-mode handoff and done | Shared M01-M05 work where applicable; first-run history processing is not the future model queue |
| Messages | People and group-thread identities, closeness/pace, Your turn/Pinned/Recent/Quiet, person pages, merge/split with suggestions, new-text trimming, cached ask and Got it with preview | M02/M04; incoming Gmail reactions remain planned in the site status table |
| To do | Deterministic and schema.org extraction, act-by/lead times, relevancy windows, promises, bounded catch-up, manual add/edit, scheduling, undo and later-evidence resolution review | M01-M04; the rail accuracy badge is intentionally gated |
| Updates | Two configurable cuts (08:00/16:30 defaults), source facts and numeric changes, needs-you suggestions, trackers, source tuning, token-bound let-go preview/commit, expiry | M02-M05; suggestions are not automatically created tasks |
| Reading | Editions, fading, Later, per-source original layout, guarded article fetch, reading pace/headlines, highlights/notes/export, Marginalia send and highlight read-back | M04 section-aware/lazy indexing; finishing-scene gap U06 |
| Archive | Record detection, provenance and corrections, field answers with archive-ask fallback, ledger/facets, CSV/PDF export, recurring subscriptions, renewal reviews, coming-up moments and semantic record-field chunks | M01/M02 for smart fields, learned issuer extraction and evaluation; shared M04 versioning/filter infrastructure |

Evidence owners: `crates/todo`, `updates`, `reading`, `records`; daemon `handler/{now,modes,messages,todos,todo_resolution,updates,records,arrivals}*`; corresponding CLI command modules, web feature directories and TUI lenses.

Upstream addition: `9ca8f08a6` implements Microsoft To Do linking and bidirectional synchronization, with a fingerprint-bound preview, durable link state and CLI/web/TUI/MCP surfaces. It adds migration `091_todo_microsoft.sql`, `crates/todo/src/linked.rs`, daemon `handler/microsoft_todo*` and `todo microsoft status|link|sync`. The upstream To do guide already documents it. This is shipped work absent from the older blueprint narrative, not a new task for this audit.

### Phase status, without misleading checkboxes

| Chapter 22 phase | Verdict |
| --- | --- |
| 0: philosophy and docs | Substantially delivered; website drift remains |
| 1: To do | Core workflow delivered; tiers, extraction evaluation and index recipe remain |
| 2: Now/membership | Delivered except evaluation/indexing slices and the stronger never-bury contract |
| 3: Messages | Delivered; recipe activation/retrieval evaluation remains |
| 4: Updates | Delivered; model facts/fact indexing remain |
| 5: Reading | Delivered; section-aware/lazy indexing remains |
| 6: Archive | Delivered, including the field-chunk/PDF indexing path; smart-tier fields/issuer learning/evaluation remain |
| 7: model aspects | Not implemented |

## TUI, web and assistant details still incomplete

| ID | Specification | Implementation gap or mismatch |
| --- | --- | --- |
| U01 | TUI pattern selection, counts and batch-confirm config (08/A007) | `Action::PatternSelect` exists but is not keyboard-bound; no `*a/*n/...` prefix dispatch, vim count accumulator or `behavior.batch_confirm` config. Toggle selection and visual mode do work. Evidence: `crates/tui/src/action.rs`, `keybindings.rs`, `input.rs`, `app/mutation_actions.rs`. |
| U02 | True full-width fullscreen (08) | `LayoutMode::FullScreen` retains a 15/85 sidebar split in `crates/tui/src/app/draw.rs`. The actual key is `F`, not old `f`. |
| U03 | TUI diagnostics/status indicators (05/08) | Search-mode controls and semantic diagnostics exist, but the status bar has no dedicated search-mode, reader-mode or snooze count; semantic active-profile/progress/error presentation is not the sketched inline display. `crates/tui/src/ui/status_bar.rs::StatusBarState` shows the actual fields. |
| U04 | TUI export format/destination selection (11) | `E` requests Markdown and saves a temp file. No four-format picker, clipboard action or `~/mxr/exports/` destination. CLI exports support all four formats. Evidence: `crates/tui/src/runner.rs:3266-3299`. |
| U05 | Shared web activity status (27) | No `StatusActivity` component/contract for sweeps and assistant turns in the main status bar. Sync/semantic status and the assistant's own `LiveLine` exist. Evidence: `apps/web/src/components/StatusBar.tsx`, `features/assistant/LiveLine.tsx`. |
| U06 | Finishing scenes once per section per day (27/D134) | Reading never calls `useLowTide` or renders a finishing scene. Now's `NowEmpty` directly mounts `LowTideScene sound` (`NowRoute.tsx:418-427`) without the per-day memory used by Messages/To do/Updates; its sound guard is only component-lifetime state. The legacy desk's hook also lacks `daily: true`. |
| U07 | Named spacing/shadow tokens and heading-role lint (27/D126) | No `--space-row-x`, `--space-gutter`, `--space-page-x`, `--space-section`, `--shadow-drawer` or `--shadow-overlay` implementation. Drawer shadows remain literal CSS. `apps/web/scripts/check-type-scale.mjs` checks literal sizes, not the specified role-class restriction on feature headings. |
| U08 | Provenance and reader reasons (27/D136) | `ModelMark.tsx` gives model/location text and a tooltip, not the specified provenance link. `ReasonLine` is used for person context but not consistently under the generic mailbox/thread reader header. |
| U09 | One mode icon everywhere (27) | `ModeMark.tsx` uses `ListChecks`/`Archive`; `sidebarRail.ts` and `MobileTabs.tsx` use `ListTodo`/`FolderArchive`. The 1.75 icon stroke is not a global default either. |
| U10 | Assistant readouts can fully expand (29) | Web `Proof.tsx` expands all preview rows supplied to it, but `ReadoutRows.tsx:39` renders non-interactive "and N more" for a tool readout. The TUI caps approval rows at six and displays another summary, not a full expandable item list. Do not call all folded readouts fully inspectable from these presentations. |
| U11 | Assistant TUI presentation parity (29) | Daemon titles, streaming, step folding, links and approvals exist. The TUI still uses "auto-approve on"/"YOUR CALL" rather than the proposed trusted/proof presentation; its running line lacks the web's slow-model step note; the proof card has no equivalent irreversible-warning renderer. See `crates/tui/src/ui/assistant_pane.rs::{title_line,card_lines,footer_lines}` versus web `Proof.tsx`/`wire.ts`. |
| U12 | Design acceptance evidence (21/26/30/31, D102) | `docs/dogfooding-log.md` remains an empty five-day template. Spike S2 has no recorded result, unlike S1/S3/S4. No recorded real-model gist benchmark or generated-arrival property test required by chapter 22/X14 was found; deterministic arrival tests exist. Fixture/browser tests do not establish model accuracy on real mail. Independent/manual gates are not proven by the existence of design-gate tests. |

Other lower-impact differences: the palette uses `match_score`, not nucleo (`ui/command_palette.rs`); viewport-bottom is an unbound action because `L` opens links; no `:` command mode; reader cleanup has no explicit ASCII-divider pass, and `QuotedBlock.date` stays `None`. These should be amended or implemented only if still desired, rather than treated as missing core email workflows.

W1-W8 all have implementation and tests in the tree. The remaining items above are incomplete contracts or acceptance evidence, not eight unstarted design rungs. The surviving `/desk` route and old `Page` wrappers are migration leftovers; replacing a wrapper name alone is not a missing user capability.

## Explicit future work and optional requirements

These items are not currently implemented, but the documents explicitly defer or qualify them. They should not be represented as regressions.

| Area | Still absent or unaccepted | Blueprint/source evidence |
| --- | --- | --- |
| Remote browser access (R3) | Child/browser grants, remote-origin `mxr web`, browser-cookie/CSRF lifecycle | Chapter 24; `crates/web/src/remote_gateway.rs` rejects browser Origin requests and mounts only pairing/deregistration/IPC routes |
| Remote presence and lifecycle (R4) | Session leases/heartbeats/last-seen, ready/stopping lifecycle revisions, shutdown preview/drain and reconnect semantics | Chapter 24; registration tables exist but not a presence/session table; `serve status` requires a live local daemon and reports narrower authority state |
| Managed remote hosting (R5) | `serve --tailscale`, `serve stop`, launchd/systemd/Tailscale reconciliation and installed reboot acceptance | Chapter 24; `ServeAction` is Status/Pair/Clients, with foreground/listen options |
| Remote-send acceptance (R2) | Draft/send/attachment implementation exists; complete installed fault/reconnect acceptance is not established by this audit | Chapter 24 and `docs/implementation/remote-hosting.md`; `crates/protocol/src/remote.rs` allowlist includes portable sends/attachments |
| Backups | Coordinated online backup | Chapter 23 explicitly starts with stopped-daemon snapshots; `crates/daemon/src/backup.rs` implements that supported path |
| Additional transports | Windows named pipes, systemd socket activation, vsock, a general browser WS-binary server adapter, direct TLS/mTLS transport policy | Chapter 20 section 6. The implemented WSS connector and authenticated remote gateway do not imply these separate adapters ship. |
| In-process bridge | Replacing the bridge's local socket round-trip with an in-memory connector | Chapter 20 known follow-up 5d; explicitly latency-only and deferred |
| Transport test robustness | Bound the post-stop UDS assertion to tolerate a connection immediately reaching EOF | Chapter 20 known follow-up; `crates/transport/src/uds.rs::stop_accepting_refuses_new_connections_but_defers_unlink` still uses the immediate assertion |
| Gmail authorization | Device-code fallback mentioned as optional in the OAuth addendum | Gmail uses installed-app loopback authorization, not device code. Outlook's device-code implementation is separate. |
| Packaging | AUR/Nix, Intel macOS and Linux aarch64 binaries | Chapter 14 explicitly keeps these in the demand-driven backlog; current release targets are intentional |
| Extensions | Embedded Lua/Rhai runtime, cloud embedding backend, richer future retrieval experiments | Chapter 14 future section. Do not conflate the proposed office extraction work with all office extraction being absent: `undoc` and spreadsheet extraction already exist. |
| Mail capabilities | PGP/age mail encryption, conditional snooze-until-reply, full calendar/CalDAV state and grid, newsletter TTS | Chapters 14/15 explicitly defer these. Email invites and RSVP already ship. |
| Deletion after an offline gap | Gmail offline-deletion recovery remains deferred in chapter 22's final decisions | This is distinct from the implemented derived-data cleanup after a deletion is actually observed. |
| Optional integrations | Sign in with ChatGPT SDK, Obsidian-specific highlight integration | Chapter 22 parks the SDK and says Obsidian waits until requested. Markdown highlight export and Marginalia already exist. |
| Desktop notification delivery | OS notifications via notify-rust and terminal bell | Chapter 14 future section; no active notify-rust/bell path was found. Local chimes are a separate, shipped feature. |
| Native/mobile hosting architecture | SwiftUI client, Rust-on-iOS extraction, replication/CRDTs, APNs, Bonjour pairing, multi-tenancy, production VPS migration | Chapter 23 explicitly excludes these; the Tauri macOS wrapper is a different, shipped client. |
| Product choices | Per-item removal from an assistant proof, editor-style themes, aesthetic decisions Q1/Q2/Q4/Q6 | Chapter 31 leaves these open; whole-call approval is deliberate, not a broken implementation. |
| Site versioning | Versioned documentation | Chapter 19 mentions versioning, but the Starlight config has no versioning integration. It is not an out-of-the-box feature established by the current config. |
| Changelog customization | Checked-in `cliff.toml` | Chapter 17 prescribes one; release uses `git cliff` without a checked-in config. Changelog/release generation itself exists. |

The transport conformance corpus and provider adapter kit are implemented. OCR is explicitly outside current semantic extraction scope; its absence is not a broken feature. ProviderMeta and remote-search capability flags are explicitly dormant/optional and should not be counted as missing product features merely because they have no runtime consumers.

## Website assessment

### Incorrect or incomplete user-facing content

| ID | Source page | Verified issue | Correct update |
| --- | --- | --- | --- |
| D01 | `site/src/content/docs/guides/email-modes.md`, deletion section | Still says derived-data deletion is planned and summaries/gists/promises/opened attachment files remain. The same text was read from `https://mxr.sh/guides/email-modes/`. | Describe the implemented `delete_messages_and_derived` transaction and pending file/index cleanup. Preserve the distinction between derived mail data, deliberate user-created state and retained no-text receipts. |
| D02 | `guides/security-and-privacy.md`, deletion table | Its "Left behind today" list describes pre-fix derived-data retention. | Align with `crates/store/src/message_deletion.rs`, migration 058 and pending forget cleanup. Do not promise instant filesystem erasure when cleanup is queued. |
| D03 | `reference/config.md`, `guides/activity-log.md` | Advertises effective activity config/privacy controls that C01 shows are disconnected; config reference also presents inactive appearance/log settings without qualification. | Document the effective env control and reserve/reject unsupported options until implemented; restore positive claims with tests. |
| D04 | `reference/config.md` | Missing dedicated `[modes]`, `[messages]` and `[records]` sections. | Document `archive_on_last_done`, Messages window/recipient settings, and record detection/PDF budget settings from the Rust types/defaults. |
| D05 | `guides/todo.md` | No task-oriented guidance for shipped `todo add`, `todo edit` and `todo catchup`, including their previews. Current upstream adds Microsoft linking but not those workflows. | Add creation, correction and first-run catch-up examples from the actual CLI definitions. These commands do have generated reference entries. |
| D06 | `guides/email-modes.md` and mode guides | The mode overview omits explanation/hint/correction/arrivals commands from its shipped command inventory. Its planned first-run wording is too broad: first-run data and To do catch-up already exist, while the model queue remains future. | Separate shipped deterministic first-run behavior from future cross-mode/model processing; add `modes explain`, `modes hint`, arrivals and correction links where useful. |
| D07 | Generated CLI reference inputs | Ten commands generate stub pages because their own help snapshots are absent; nested command coverage also has holes. | Add snapshot cases; details below. Do not hand-edit the ignored generated pages. |
| D08 | `site/scripts/generate-cli-reference.mjs::COMMAND_EXAMPLES` | 27 generated command pages lack curated use/examples, including Now and most mode controls. | Add goal-oriented examples, with preview first where applicable. |
| D09 | `site/public/openapi.json` and its publishing path | At upstream `0.6.108`, the committed artifact still reports `0.6.105` and 254 paths. It omits the three Microsoft To Do routes and schemas now in Rust. | Regenerate the committed artifact and add an explicit artifact-drift assertion; source-schema snapshots do not compare the committed website export. |
| D10 | `reference/keybindings.md` | Calls `g h` the desk while current navigation calls it Now. | Use current names and distinguish the legacy `/desk` route. |
| D11 | `guides/reading-and-paper-trail.md` | Link to `/reference/keybindings/#reading-and-paper-trail-1` has no target heading. | Link to the actual heading ID. This was the only broken local heading target found in built HTML. |
| D12 | Site navigation | `guides/headless-linux.md` builds but is not in the sidebar. | Put headless operation under the appropriate setup/operator group, or deliberately link it from a canonical guide. |

Deletion evidence is stronger than the test-only `MESSAGE_DELETION_RULES` registry: the production `Store::delete_messages_and_derived` method clears derived caches in the transaction, cascades per-message rows and leaves durable cleanup IDs. See `crates/store/src/message_deletion.rs` and the daemon forget drain. The report does not claim user-authored drafts or receipt identities are all deleted.

### What the generated reference actually does

The build generated **119 command pages**, plus the CLI index and handwritten concepts page. There are **no missing top-level command pages after generation**. Live `/reference/cli/todo/` contains `add`, `edit`, `catchup` and the new Microsoft subcommands. Live `/reference/cli/assistant/` exists but is a stub.

The 10 commands without their own help snapshot are:

```text
assistant  commitments  draft  humanize  mcp
profile    signatures   threads  time    voice
```

These pages explicitly render "No detailed --help snapshot is captured". The cause is the hand-maintained cases in `crates/daemon/tests/cli_help.rs`, not the fact that output Markdown is gitignored. Additional concrete omissions include `llm key set|status|remove` and `records subscriptions`: they exist in clap but have no dedicated nested help snapshots. Root help generation alone cannot establish complete nested flag coverage.

The 27 command pages without a `COMMAND_EXAMPLES` entry are:

```text
arrivals  assistant  backup  categories  corrections  count  draft
headers   humanize   jobs    mcp         modes        now    profile
reclassify records   recovery restart    route        signatures
snoozed   threads    todo    triage      undo         updates voice
```

Keep generated pages ignored if that remains the intended architecture. The useful gates are automatic command-tree snapshot coverage, example validation and a clean build. `git diff --exit-code` on ignored CLI pages would not detect their absence. That command *is* useful for tracked artifacts such as `site/public/openapi.json` after regeneration.

### Build and publishing checks

| Check | Result | What it proves |
| --- | --- | --- |
| `npm test` in the checkout's `site/` | Passed: 3 tests | Search-envelope documentation rules, not all examples |
| `npm run validate` | Passed: 254 OpenAPI paths | Selected banned patterns, IDs and API metadata; it is not a general link checker |
| Regenerate CLI docs from upstream snapshots in an isolated copy | Passed: 119 command pages | Generated page coverage of the captured root command list |
| Clean `npm ci --ignore-scripts --no-audit --no-fund`, then queued `npm exec -- astro build` in that copy | Passed: 202 pages; Pagefind processed 204 HTML files | Current locked site dependencies and static generation work without modifying the working checkout |
| `node scripts/generate-llms-txt.mjs` in the isolated copy | Passed: 198 Markdown siblings and full-text entries | Agent-facing Markdown generation works |
| Parse all 204 built HTML files and resolve same-origin anchor links | No missing page paths; one missing heading target (D11) | Generated mode references are not broken page links |
| `bash scripts/check_architecture_boundaries.sh` | Passed | Current enforced Cargo dependency boundaries, not every historical dependency sentence |
| Live text checks | Confirmed D01; To do page exists; assistant page is a stub | The source drift is not solely an old local generated-file artifact |

The first isolated build attempt reused a symlink to older checkout dependencies and failed on cross-root Astro metadata. A clean install of the lockfile dependencies removed that harness problem; the clean build passed. No product build failure is asserted from that first attempt.

The build emitted a large-chunk warning and an empty-i18n-collection warning. Neither failed generation. The audit did not run Rust/OpenAPI export compilation, so D09 is based on direct comparison of the tracked JSON with the new upstream route/schema declarations, not a new Rust exporter run.

Five ignored/untracked `.bak` files exist in the local site tree (`guides/compose.md.bak`, `for-agents.md.bak`, `linked-drafts.md.bak`, `recipes.md.bak`, and `reference/mcp.md.bak`). They were not deleted or counted as shipped pages; they could contain another person's work.

## Blueprint statements to correct instead of implementing again

| Documents | Drift |
| --- | --- |
| 00, 13 | The Gmail/SMTP-only first-party story and IMAP/Outlook community-only labels are historical. Both provider crates exist. "Not an AI product" needs qualification against the shipped assistant and optional model workflows. |
| 01 | Crate inventory and migration path are old. Migrations are under `crates/store/migrations`. Client runtime dependencies include core/config; the current boundary audit and boundary checker already permit them. Use `yup-oauth2`, not the old `oauth2` dependency claim. |
| 02 | `SyncCursor` is opaque bytes, not the sketched tagged core enum. Drafts use `DraftContent` with revisions, managed attachments and HTML support; provider kinds include Outlook variants. IPC envelopes include source/workspace information. |
| 03 | Gmail sync fetches Full bodies. Adapter documentation exists at `site/src/content/docs/guides/adapter-development.md`; claiming no adapter guide was a false positive. |
| 05 and semantic audit | Highlight chunks are an additional source kind. Office and spreadsheet text extraction exist. OCR is intentionally excluded, not an unfinished current promise. |
| 06 | Editor scratch files are client editing artifacts in a private directory, not a daemon `CreateDraft` request returning `/tmp/mxr-draft-*.md`. Actual sending uses explicit flags, not the old Send? prompt. Frontmatter has additional intent/reference/thread/signature fields, and designed HTML drafts exist. Bare relative attachment paths are not implicitly relative to HOME; only `~/` expands. |
| 07, D010 | Terminal image rendering exists via `ratatui-image` and remote-content controls, contrary to the absolute no-images policy. `O`, not `o`, opens in the browser. Reader attachments come from `MessageBody`, not the sketched `ReaderOutput.attachments`. |
| 08, 09, 16 | Older key tables conflict with the Gmail-native defaults and mode keys. CLI and TUI dispatch are separate layers; a shared Rust Action enum is an old architecture sketch, not a missing public operation. Old command spellings below are not currently supported. |
| 11 | CLI export is complete; TUI picker/clipboard is not (U04). Do not mark all export formats unimplemented. |
| 14, 21, 30 | Entire mode phases and W1-W8 are still unchecked despite delivered workflows/rungs. Mark delivered sub-slices and leave the specific M/U findings open; do not mark a whole mixed phase done either. |
| 15 | D004's fallback, D011's shared cleaning and D114's tiers remain incomplete. Historical D054 version/client limitations must not be read as the latest contract. D118's first-encounter card was explicitly superseded by anchored hints. |
| 17 | Releases now include `mxr-mailmerge` and a desktop cask; signing/notarization workflow exists. Secret availability and installed distribution success are operational facts not newly verified by source inspection. |
| 18/19 | OAuth/runtime identity and the docs hierarchy have expanded. Starlight does not establish versioning just by being installed. |
| 20, 24 | IPC protocol is **18**, not the quoted 11/12. `mxr-transport` remains internally independent but has WSS/TLS/URL dependencies. TUI/MCP use `SelectedDaemonTarget`, not the old Unix-only resolver assumption. The local HTTP bridge still has its own local-only boundary. |
| 22 | Request names evolved: `ListModeItems` became per-mode reads; `SetModeCorrection` became move/sender correction operations; `SetCatchUp` became `SetTodoCatchup`; record answer/file requests are `AnswerFromRecords`/`FileRecord`. `mode_corrections.scope` uses `email`, not `message`. `todos.action_trusted` was superseded by the no-one-click decision. Resolution review, subscriptions, renewals, Marginalia and Microsoft linking are ahead of the narrative. |
| 23 | Signal-driven shutdown, atomic Gmail token replacement, draft revisions/outcomes, attachment resources, target-aware diagnostics, backup/restore and recovery gating exist. The pinned baseline explains the old problem descriptions; don't reopen them without a failing current path. API completeness still needs C19/D09. |
| 24 | WSS/read pairing and portable send/attachment support exist. The document's "R2 is next" wording should distinguish implementation from installed acceptance. It must not imply R3-R5 are already supported. |
| 25, 29 | Assistant CLI/web/TUI, code-enforced approval, questions, durable history, trust, streaming and compaction exist. The 25 follow-up issue is resolved; the TUI and full-readout presentation still have U10/U11 gaps. |
| 26-31 | PageFrame, Row, key hints, state components, quiet navigation, reading/compose layouts, copy-desk UI and theme generation exist. Header breakpoint is `sm`/640 rather than the prose's 1024; rail default is 248px rather than 232; list bounds are 320-640 rather than 420-600. The spec's `MailChanged` landing event name is not the actual `NewMessages` event. These are documentation/contract reconciliations, not absent screens. |
| `ipc-audit.md` | Not a complete current protocol inventory. `NewMessages` is emitted/consumed now and is no longer dormant; the app uses it for invalidation/landing. Many new mode/assistant/remote requests are absent from its old table. Absolute `/Users/...` links also limit portability. |
| Other supplementary audits | Internal-model honesty and crate separation remain valid. The transport dependency list and semantic source-kind inventory have drifted; the junction-repair claim needs C11. |

Documented command spellings with no exact current implementation include:

```text
mxr daemon stop                 mxr daemon status
mxr accounts default NAME       mxr sync --watch
mxr logs --purge --before DATE  mxr logs --purge --all
mxr modes eval                  mxr modes first-run --status
mxr modes catch-up              mxr modes arrivals
```

Some have supported equivalents (`mxr status`, `general.default_account`, `mxr todo catchup`, `mxr arrivals list`, first-run data in Now/To do). Reconcile the wording before adding aliases. `modes eval` has no equivalent accuracy harness.

## Coverage register

Every file under `docs/blueprint/` is accounted for here. "No additional gap" means no additional evidenced gap beyond the linked findings, not a fresh runtime certification.

| File | Assessment |
| --- | --- |
| `00-overview.md` | Core architecture exists; identity/provider/AI wording has drifted |
| `01-architecture.md` | Real crates and enforcement exist; inventory/dependency/IPC sketches are stale |
| `02-data-model.md` | Canonical model/store exist; C09 and evolved cursor/draft/provider shapes |
| `03-providers.md` | Gmail/IMAP/SMTP/Outlook/fake exist; C10; optional flags are not all routed |
| `04-sync.md` | Eager sync and recovery exist; C11/C12/C15 |
| `05-search.md` | Lexical/dense/hybrid/profiles/saved searches exist; C08/M04; source kinds have expanded |
| `06-compose.md` | Markdown/HTML/send/drafts exist; lifecycle/path/prompt sketches differ |
| `07-rendering.md` | Reader/unsubscribe exist; C08, limited heuristics and stale image policy |
| `08-tui.md` | Client/modes/navigation exist; C02 and U01-U04 |
| `09-cli.md` | Much larger current command surface; C16 and stale exact spellings |
| `10-rules-engine.md` | Engine/actions/history exist; C03/C04/C13/C14 |
| `11-export.md` | Four CLI formats exist; U04 |
| `12-config.md` | Parsing/credentials/model cache exist; C01/C02/C05/C07 and reserved settings |
| `13-open-source.md` | Licenses/contribution/kit/CI exist; provider and out-of-tree publication story is historical |
| `14-roadmap.md` | Historical baseline shipped; checked/unchecked status does not accurately represent current slices |
| `15-decision-log.md` | Historical/overridden decisions separated from active ones; C08/C09/M01-M05 and U findings |
| `16-addendum.md` | Most amendments implemented; C02/C03/C05/C06/C17 and U01 plus command-name drift |
| `17-release-pipeline.md` | Release/signing/cask automation exists; missing cliff config is low-impact; no live release certification |
| `18-addendum-oauth.md` | BYOC/keychain/fallback/loopback flows exist; optional Gmail device flow absent |
| `18-bug-reporting.md` | Reporting/sanitization exists; C05/C06/C18 and purge-flag drift |
| `19-addendum-docs-site.md` | Site/build/search/Markdown publishing exist; versioning absent, D findings |
| `20-transports.md` | Carrier/conformance/token/WSS foundations exist; explicit backlog plus version/dependency drift |
| `21-web-experience.md` | Most UI implementation superseded by later modes/design work; U12 acceptance remains |
| `22-email-modes.md` | Five modes plus Now delivered; M01-M05 and per-mode matrix above |
| `23-daemon-portability.md` | Preparation implemented; C19 and deferred online/mobile work |
| `24-remote-hosting-and-clients.md` | Pairing/read/send foundations exist; R3-R5 absent, installed R2 acceptance unverified here |
| `25-assistant.md` | Behavior broadly implemented; U10/U11 presentation gaps and resolved-follow-up wording |
| `26-web-design-concept.md` | Concept/rungs represented in code; U05/U06/U12 |
| `27-web-design-system.md` | Shared parts largely present; U05-U09 and literal/breakpoint drift |
| `28-web-screens.md` | Main screens exist; copy/layout specifics need reconciliation, not rebuilding |
| `29-assistant-design.md` | Copy-desk web/daemon/TUI work present; U10/U11 |
| `30-design-roadmap.md` | W1-W8 implementations present; remaining U items and acceptance evidence |
| `31-design-open-questions.md` | S1/S3/S4 results present; S2 unrecorded; open product choices remain choices |
| `README.md` | Index exists; needs a current audit/status pointer |
| `crate-boundary-audit.md` | Real Cargo seams exist and checker passes; external transport-dependency sentence is stale |
| `internal-model-audit.md` | Explicit folder/capability/identity seams remain implemented; no additional missing feature established |
| `ipc-audit.md` | Historical classification, not current inventory; emitted NewMessages and many newer requests missing |
| `semantic-search-audit.md` | Implemented local lifecycle; Highlight added, OCR intentionally excluded |
| `sync-index-lifecycle-audit.md` | Core lifecycle implemented; C11 repair scope and historical cursor naming need correction |

## Recommended work order and acceptance

1. **Privacy and safety:** C01, C02 and C04. Start with tests that fail for ignored config/remaps and different preview selections. Keep validation isolated from real mail.
2. **Operational correctness:** C03, C05, C06 and C11. Add daemon integration tests, not just unit tests of a helper that the daemon bypasses.
3. **Correct the website:** D01-D06, D10-D12. Update generator inputs for D07/D08; never patch generated pages alone. Regenerate the tracked API artifact for D09 and assert drift in CI.
4. **Reconcile the blueprint status:** mark delivered mode/design slices and current request names, preserving historical decisions and deliberate non-goals. Leave specific unresolved slices visible.
5. **Build the measured mode work:** M01 first, then M02/M03 and recipe activation under M04. N2 needs an explicit tested breakthrough rule, not merely a sign-in detector.
6. **Select the remaining product polish:** U findings and current CLI/config gaps, with tests for each promised behavior. Keep remote R3-R5 as a separately authorized project.

Before closing any implementation finding, prove its daemon path and applicable CLI/web/TUI/MCP surfaces, preview selection, persistence and failure behavior. A type, parser or helper with no production caller is not completion. Likewise, an unchecked historical box is not proof of missing code.

No source fixes, historical blueprint rewrites, GitHub issues or commits were made as part of this assessment.
