//! OpenAPI 3.1 surface for the mxr HTTP bridge.
//!
//! The Axum routes live in `lib.rs` and `routes_v6.rs`. The path inventory is
//! declared here so Swagger UI, the docs site, and generated clients see actual
//! endpoints instead of a schema-only OpenAPI document.

use mxr_protocol::{DaemonEvent, MutationCommand, Request, Response, ResponseData};
use utoipa::{
    openapi::security::{HttpAuthScheme, HttpBuilder, SecurityScheme},
    Modify, OpenApi,
};

/// Top-level OpenAPI document. Per-route `#[utoipa::path]` annotations get
/// folded in by `OpenApiRouter` when the bridge crate's router is constructed.
#[derive(OpenApi)]
#[openapi(
    info(
        title = "mxr HTTP Bridge",
        description = "Local-first email daemon HTTP/WebSocket surface. \
                       This document is a route inventory plus the protocol \
                       schemas: it lists paths and methods, not per-route \
                       parameters or response bodies, and some live bridge \
                       routes are not declared here. \
                       All routes declared in this document except \
                       /api/v1/health require a bearer \
                       token, read from the `bridge-token` file in the active \
                       profile's config directory (`~/Library/Application \
                       Support/mxr` on macOS, `$XDG_CONFIG_HOME/mxr` or \
                       `~/.config/mxr` on Linux). `[bridge].token_path` wins \
                       over `MXR_BRIDGE_TOKEN_PATH`, which wins over \
                       `MXR_CONFIG_DIR`.",
        license(name = "MIT OR Apache-2.0"),
        contact(name = "mxr", url = "https://mxr.sh")
    ),
    paths(
        health, openapi_json, swagger_ui, events_ws, client_shell,
        admin_status, admin_diagnostics, admin_bug_report, admin_events,
        admin_logs, admin_ping, admin_shutdown,
        mail_mailbox, mail_search, mail_search_groups, mail_thread, mail_thread_export,
        mail_drafts, mail_snoozed, mail_count, mail_jobs, mail_job_detail, mail_sync_status, mail_sync,
        mutation_archive, mutation_trash, mutation_spam, mutation_star,
        mutation_read, mutation_read_archive, mutation_labels, mutation_move,
        mutation_route, mutation_undo, action_snooze_presets, action_snooze,
        action_unsubscribe, action_unsubscribe_purge, action_invite_reply, attachment_open, attachment_download,
        label_create, label_rename, label_delete, mail_unsnooze_one,
        reply_later_list, reply_later_set, reminders_set, reminders_cancel,
        scheduled_sends_list, scheduled_sends_create, scheduled_sends_cancel, snippets_list,
        snippets_set, snippets_delete, sender_profile, contacts_autocomplete, screener_queue,
        screener_decisions_list, screener_decisions_set,
        screener_decisions_clear, thread_summarize, mail_draft_compose,
        mail_draft_refine, mail_humanizer_score, mail_humanizer_rewrite,
        mail_relationship_profile, mail_relationship_rebuild, mail_commitments_list,
        mail_commitments_resolve, mail_thread_briefing, mail_contacts_briefing, mail_contacts_expert,
        compose_session_start, compose_session_refresh, compose_session_restore,
        compose_session_update, compose_session_send, compose_session_safety_check,
        compose_session_collaborators, compose_session_save, compose_session_schedule,
        compose_session_attachment, compose_session_discard, rules_list, rule_detail, rule_form,
        rule_history, rule_dry_run, rule_upsert, rule_upsert_form, rule_delete,
        saved_searches_list, saved_searches_create, saved_searches_delete, saved_searches_update,
        saved_searches_run, accounts_list, accounts_config, account_test,
        account_upsert, account_set_default, account_remove, account_disable,
        account_addresses_list, account_addresses_add, account_addresses_remove,
        account_addresses_primary, auth_session_start, auth_session_get,
        auth_session_cancel, auth_session_complete, subscriptions_list,
        llm_status, llm_config_get, llm_config_update, notification_chimes_get,
        notification_chimes_update, semantic_status, semantic_reindex, semantic_enable,
        semantic_profile_install, semantic_profile_use, semantic_backfill, analytics_wrapped,
        analytics_storage_breakdown, analytics_largest_messages,
        analytics_stale_threads, analytics_contact_asymmetry,
        analytics_contact_decay, analytics_response_time,
        analytics_refresh_contacts, analytics_rebuild,
        mail_message_body, mail_message_html_images, mail_message_inline_image, mail_message_raw_headers,
        mail_message_set_flags, mail_export_search, mail_drafts_orphaned_list,
        mail_drafts_save_local, mail_drafts_reset_orphan, mail_drafts_send_stored,
        mail_drafts_delete_stored, mail_signatures_list, mail_signatures_upsert,
        mail_signature_defaults_list, mail_signature_default_set,
        mail_signature_default_clear, mail_signature_resolve, mail_signatures_delete,
        platform_accounts_authorize, platform_accounts_repair, platform_voice_get,
        platform_voice_rebuild, mail_mutation_jobs, mail_owed, mail_desk, mail_desk_dismiss, mail_desk_restore, mail_desk_done, mail_desk_later, mail_whois,
        mail_send_time, mail_archive_ask, saved_searches_unread_counts,
        analytics_cadence_drift, cadence_watch_list, cadence_watch, cadence_unwatch,
        mail_time_resolve, mail_thread_context, mail_thread_gist, mail_thread_gists,
        compose_session_promises, mail_commitments_record,
        mail_place_list, mail_place_sweep, mail_message_kind, mail_messages_pin,
        mail_sender_kind, mail_todos_runway, mail_todos_create, mail_todos_in_state,
        mail_todo_get, mail_todos_state, mail_todo_schedule, mail_todo_edit,
        mail_todos_catchup_get, mail_todos_catchup_set, mail_mode_guide, mail_hint_seen,
        mail_now, mail_rail, mail_freshness, mail_mode_membership_get, mail_mode_membership_post, mail_mode_done,
        mail_people, mail_people_page, mail_people_ack, mail_people_merge, mail_people_split,
        mail_people_merge_suggestions,
        mail_records_ledger, mail_records_answer, mail_records_subscriptions, mail_record_get, mail_record_field,
        mail_records_dismiss, mail_records_file, mail_records_sender, mail_records_export,
        mail_reading_edition, mail_reading_item, mail_reading_later, mail_reading_engagement,
        mail_reading_article, mail_reading_highlights_get, mail_reading_highlights_post,
        mail_reading_sources,
        mail_updates_digest, mail_updates_let_go, mail_updates_source,
        mail_arrivals, mail_arrivals_list, mail_arrivals_modes, mail_message_move,
        mail_move_undo, mail_corrections
    ),
    components(schemas(
        Request,
        Response,
        ResponseData,
        DaemonEvent,
        MutationCommand,
        SweepPlaceBody,
        PinMessagesBody,
        DeskDoneBody,
        DeskLaterBody,
        ThreadGistsBody,
        SenderKindBody,
        crate::todo_routes::TodoStateBody,
        crate::todo_routes::TodoScheduleBody,
        crate::todo_routes::TodoEditBody,
        crate::todo_routes::TodoCreateBody,
        crate::todo_routes::TodoCatchupBody,
        crate::mode_routes::HintSeenBody,
        crate::mode_routes::ModeMembershipBody,
        crate::mode_routes::ModeDoneBody,
        crate::messages_routes::AckBody,
        crate::messages_routes::MergeBody,
        crate::messages_routes::SplitBody,
        crate::record_routes::RecordFieldBody,
        crate::record_routes::RecordDismissBody,
        crate::record_routes::RecordFileBody,
        crate::record_routes::RecordSenderBody,
        crate::record_routes::RecordExportBody,
        crate::reading_routes::ReadingLaterBody,
        crate::reading_routes::ReadingEngagementBody,
        crate::reading_routes::ReadingArticleBody,
        crate::reading_routes::ReadingHighlightBody,
        crate::reading_routes::ReadingSourceBody,
        crate::updates_routes::UpdatesLetGoBody,
        crate::updates_routes::UpdateSourceBody,
        crate::arrival_routes::ArrivalModesBody,
        crate::arrival_routes::MoveBody,
    )),
    modifiers(&BearerSecurity),
    security(("bearer" = []))
)]
pub struct ApiDoc;

macro_rules! endpoint {
    ($method:ident $name:ident $path:literal, $summary:literal) => {
        #[utoipa::path(
                            $method,
                            path = $path,
                            summary = $summary,
                            responses(
                                (status = 200, description = "OK"),
                                (status = 401, description = "Missing or invalid bridge token")
                            )
                        )]
        #[allow(dead_code)]
        fn $name() {}
    };
}

/// The one route `router::app` serves outside the bearer-auth layer, so its
/// operation overrides the document-level security requirement.
#[utoipa::path(
    get,
    path = "/api/v1/health",
    summary = "Unauthenticated bridge liveness probe",
    security(),
    responses((
        status = 200,
        description = "Bridge is up. Returns status, service, and protocol_version"
    ))
)]
#[allow(dead_code)]
fn health() {}

endpoint!(get openapi_json "/api/v1/openapi.json", "OpenAPI 3.1 document");
endpoint!(get swagger_ui "/api/v1/docs", "Swagger UI");
endpoint!(get events_ws "/api/v1/events", "WebSocket daemon event stream");
endpoint!(get client_shell "/api/v1/client/shell", "Client shell manifest");

endpoint!(get admin_status "/api/v1/admin/status", "Daemon status snapshot");
endpoint!(get admin_diagnostics "/api/v1/admin/diagnostics", "Diagnostics report");
endpoint!(get admin_bug_report "/api/v1/admin/diagnostics/bug-report", "Sanitized bug report");
endpoint!(get admin_events "/api/v1/admin/events", "Persisted daemon events");
endpoint!(get admin_logs "/api/v1/admin/logs", "Recent daemon logs");
endpoint!(post admin_ping "/api/v1/admin/ping", "Bridge round-trip ping");
endpoint!(post admin_shutdown "/api/v1/admin/shutdown", "Request daemon shutdown");

endpoint!(get mail_mailbox "/api/v1/mail/mailbox", "Mailbox view");
endpoint!(get mail_search "/api/v1/mail/search", "Run a mail search");
endpoint!(get mail_search_groups "/api/v1/mail/search/groups", "Group a mail search result set");
endpoint!(get mail_thread "/api/v1/mail/threads/{thread_id}", "Read a thread");
endpoint!(get mail_thread_export "/api/v1/mail/threads/{thread_id}/export", "Export a thread");
endpoint!(get mail_drafts "/api/v1/mail/drafts", "List drafts");
endpoint!(get mail_snoozed "/api/v1/mail/snoozed", "List snoozed messages");
endpoint!(get mail_count "/api/v1/mail/count", "Count matching messages");
endpoint!(post mail_mutation_jobs "/api/v1/mail/mutation-jobs", "Start a mutation as a background job");
endpoint!(get mail_jobs "/api/v1/mail/jobs", "List background jobs");
endpoint!(get mail_job_detail "/api/v1/mail/jobs/{job_id}", "Inspect a background job");
endpoint!(get mail_sync_status "/api/v1/mail/sync/status", "Sync status");
endpoint!(post mail_sync "/api/v1/mail/sync", "Trigger sync");

endpoint!(post mutation_archive "/api/v1/mail/mutations/archive", "Archive messages");
endpoint!(post mutation_trash "/api/v1/mail/mutations/trash", "Trash messages");
endpoint!(post mutation_spam "/api/v1/mail/mutations/spam", "Mark messages as spam");
endpoint!(post mutation_star "/api/v1/mail/mutations/star", "Star or unstar messages");
endpoint!(post mutation_read "/api/v1/mail/mutations/read", "Mark messages read or unread");
endpoint!(post mutation_read_archive "/api/v1/mail/mutations/read-and-archive", "Read and archive messages");
endpoint!(post mutation_labels "/api/v1/mail/mutations/labels", "Add or remove labels");
endpoint!(post mutation_move "/api/v1/mail/mutations/move", "Move messages to a label or folder");
endpoint!(post mutation_route "/api/v1/mail/mutations/route", "Route messages from a queue to a label");
endpoint!(post mutation_undo "/api/v1/mail/mutations/undo", "Undo a recent mutation");

endpoint!(get action_snooze_presets "/api/v1/mail/actions/snooze/presets", "List snooze presets");
endpoint!(post action_snooze "/api/v1/mail/actions/snooze", "Snooze messages");
endpoint!(post action_unsubscribe "/api/v1/mail/actions/unsubscribe", "Unsubscribe from list mail");
endpoint!(post action_unsubscribe_purge "/api/v1/mail/actions/unsubscribe-purge", "Unsubscribe and clear sender footprint");
endpoint!(post action_invite_reply "/api/v1/mail/actions/invite/reply", "Reply to a calendar invite");
endpoint!(post attachment_open "/api/v1/mail/attachments/open", "Open an attachment");
endpoint!(post attachment_download "/api/v1/mail/attachments/download", "Download an attachment");
endpoint!(post label_create "/api/v1/mail/labels/create", "Create a label");
endpoint!(post label_rename "/api/v1/mail/labels/rename", "Rename a label");
endpoint!(post label_delete "/api/v1/mail/labels/delete", "Delete a label");
endpoint!(post mail_unsnooze_one "/api/v1/mail/snoozed/{message_id}/wake", "Wake one snoozed message");

endpoint!(get reply_later_list "/api/v1/mail/reply-later", "List reply-later messages");
endpoint!(post reply_later_set "/api/v1/mail/reply-later/{message_id}", "Set or clear reply-later");
endpoint!(post reminders_set "/api/v1/mail/reminders", "Schedule an auto-reminder");
endpoint!(delete reminders_cancel "/api/v1/mail/reminders/{message_id}", "Cancel an auto-reminder");
endpoint!(get scheduled_sends_list "/api/v1/mail/scheduled-sends", "List pending scheduled sends");
endpoint!(post scheduled_sends_create "/api/v1/mail/scheduled-sends", "Schedule a draft send");
endpoint!(delete scheduled_sends_cancel "/api/v1/mail/scheduled-sends/{draft_id}", "Cancel a scheduled send");
endpoint!(get snippets_list "/api/v1/mail/snippets", "List snippets");
endpoint!(post snippets_set "/api/v1/mail/snippets", "Create or update a snippet");
endpoint!(delete snippets_delete "/api/v1/mail/snippets/{name}", "Delete a snippet");
endpoint!(get sender_profile "/api/v1/mail/sender", "Sender profile");
endpoint!(get contacts_autocomplete "/api/v1/mail/contacts/autocomplete", "Prefix-search known senders");
endpoint!(get screener_queue "/api/v1/mail/screener/queue", "List screener queue");
endpoint!(get screener_decisions_list "/api/v1/mail/screener/decisions", "List screener decisions");
endpoint!(post screener_decisions_set "/api/v1/mail/screener/decisions", "Set screener decision");
endpoint!(delete screener_decisions_clear "/api/v1/mail/screener/decisions", "Clear screener decision");
endpoint!(post thread_summarize "/api/v1/mail/threads/{thread_id}/summarize", "Summarize a thread");
endpoint!(post mail_draft_compose "/api/v1/mail/drafts/compose", "Generate an LLM draft (new message or reply)");
endpoint!(post mail_draft_refine "/api/v1/mail/drafts/refine", "Refine draft text with LLM");
endpoint!(post mail_humanizer_score "/api/v1/mail/humanizer/score", "Score draft for human-like voice");
endpoint!(post mail_humanizer_rewrite "/api/v1/mail/humanizer/rewrite", "Rewrite draft toward target voice");
endpoint!(get mail_relationship_profile "/api/v1/mail/relationship", "Relationship profile for a contact");
endpoint!(post mail_relationship_rebuild "/api/v1/mail/relationship/rebuild", "Rebuild relationship analytics");
endpoint!(get mail_commitments_list "/api/v1/mail/commitments", "List detected commitments");
endpoint!(post mail_commitments_resolve "/api/v1/mail/commitments/{commitment_id}/resolve", "Resolve a commitment");
endpoint!(get mail_thread_briefing "/api/v1/mail/threads/{thread_id}/briefing", "Briefing for a dormant thread");
endpoint!(get mail_contacts_briefing "/api/v1/mail/contacts/briefing", "Recipient briefing for compose context");
endpoint!(get mail_contacts_expert "/api/v1/mail/contacts/expert", "Find experts who answered similar questions");
endpoint!(get mail_owed "/api/v1/mail/owed", "List threads that owe a reply");
endpoint!(get mail_desk "/api/v1/mail/desk", "The desk: owed replies, due promises, waiting threads and new mail from people");
endpoint!(post mail_desk_dismiss "/api/v1/mail/desk/dismiss", "Done waiting: take threads off the desk's Waiting lane (dry_run previews)");
endpoint!(post mail_desk_restore "/api/v1/mail/desk/restore", "Undo done waiting");

/// Body of `POST /api/v1/mail/desk/done`.
#[derive(utoipa::ToSchema)]
#[allow(dead_code)]
struct DeskDoneBody {
    items: Vec<mxr_protocol::DeskDoneItemData>,
    /// Preview only; nothing changes.
    dry_run: Option<bool>,
}

#[utoipa::path(
    post,
    path = "/api/v1/mail/desk/done",
    summary = "Done: put desk items away (archive, mark read, keep off the desk; resolve a promise). dry_run previews",
    request_body = DeskDoneBody,
    responses(
        (
            status = 200,
            description = "The `DeskItemsResolved` variant: one outcome per item and the undo id",
            body = ResponseData
        ),
        (status = 400, description = "No items"),
        (status = 401, description = "Missing or invalid bridge token")
    )
)]
#[allow(dead_code)]
fn mail_desk_done() {}

/// Body of `POST /api/v1/mail/desk/later`.
#[derive(utoipa::ToSchema)]
#[allow(dead_code)]
struct DeskLaterBody {
    thread_ids: Vec<String>,
    /// The instant the client previewed, as RFC3339. Never a phrase: resolve
    /// it with `/api/v1/mail/time/resolve` and send the chosen `at`.
    until: String,
    /// Preview only; nothing changes.
    dry_run: Option<bool>,
}

#[utoipa::path(
    post,
    path = "/api/v1/mail/desk/later",
    summary = "Reply later, or bring back if nobody replies, until a time (by who wrote last). dry_run previews",
    request_body = DeskLaterBody,
    responses(
        (
            status = 200,
            description = "The `ThreadsDeferred` variant: one item per conversation and the undo id",
            body = ResponseData
        ),
        (status = 400, description = "No conversations"),
        (status = 401, description = "Missing or invalid bridge token")
    )
)]
#[allow(dead_code)]
fn mail_desk_later() {}
endpoint!(get mail_whois "/api/v1/mail/whois", "Explain a person or term from local evidence");
endpoint!(get mail_send_time "/api/v1/mail/send-time", "Recommend a send time for recipients");
endpoint!(post mail_archive_ask "/api/v1/mail/archive-ask", "Ask the archive a question with citations");
/// Typed so generated clients get the query and the answer: the web app's
/// time fields call this on every debounced keystroke.
#[utoipa::path(
    get,
    path = "/api/v1/mail/time/resolve",
    summary = "Resolve a natural-language time phrase",
    params(
        ("input" = String, Query, description = "The phrase, such as \"fri 3\" or \"in 2d\""),
        (
            "now" = Option<chrono::DateTime<chrono::Utc>>,
            Query,
            description = "RFC3339 anchor for relative phrases; defaults to the daemon's clock"
        ),
        (
            "time_zone" = Option<String>,
            Query,
            description = "IANA zone to resolve in, such as \"Europe/London\"; defaults to the daemon's zone"
        ),
    ),
    responses(
        (
            status = 200,
            description = "The `ResolvedTime` variant: exactly one of `resolution` and `error` is set",
            body = ResponseData
        ),
        (status = 401, description = "Missing or invalid bridge token")
    )
)]
#[allow(dead_code)]
fn mail_time_resolve() {}
#[utoipa::path(
    post,
    path = "/api/v1/mail/compose/session/promises",
    summary = "Find promises in an outgoing compose session",
    request_body = crate::promise_routes::DetectComposePromisesRequest,
    responses(
        (
            status = 200,
            description = "The `Promises` variant. A missing, blocked or slow model is a status, never an error",
            body = ResponseData
        ),
        (status = 401, description = "Missing or invalid bridge token")
    )
)]
#[allow(dead_code)]
fn compose_session_promises() {}
#[utoipa::path(
    post,
    path = "/api/v1/mail/commitments",
    summary = "Keep a promise from a sent message as a dated commitment",
    request_body = crate::promise_routes::RecordPromiseRequest,
    responses(
        (
            status = 200,
            description = "The `RecordedPromise` variant; with `dry_run` nothing is stored and the id is empty",
            body = ResponseData
        ),
        (status = 401, description = "Missing or invalid bridge token")
    )
)]
#[allow(dead_code)]
fn mail_commitments_record() {}
endpoint!(get mail_thread_context "/api/v1/mail/threads/{thread_id}/context", "Store facts for the reader's context block");
endpoint!(get mail_thread_gist "/api/v1/mail/threads/{thread_id}/context/gist", "Model-written gist and ask for a conversation, cached per newest message");

/// Body of `POST /api/v1/mail/gists`.
#[derive(utoipa::ToSchema)]
#[allow(dead_code)]
struct ThreadGistsBody {
    /// At most 100, in the order to write missing gists (visible rows first).
    thread_ids: Vec<String>,
    /// Queue missing gists for people's conversations; each arrives as a
    /// `ThreadGistReady` event.
    generate: Option<bool>,
}

#[utoipa::path(
    post,
    path = "/api/v1/mail/gists",
    summary = "List-row gists for many conversations: cached ones at once, missing ones queued with generate",
    request_body = ThreadGistsBody,
    responses(
        (
            status = 200,
            description = "The `ThreadGists` variant: model state, cached gists, queued and skipped ids",
            body = ResponseData
        ),
        (status = 400, description = "A malformed thread id or more than 100 ids"),
        (status = 401, description = "Missing or invalid bridge token")
    )
)]
#[allow(dead_code)]
fn mail_thread_gists() {}

#[utoipa::path(
    get,
    path = "/api/v1/mail/todos",
    summary = "The To do runway: Now, Coming up by week, Later, Whenever and Done this week",
    params(
        ("account" = Option<String>, Query, description = "Account id; omitted covers every account"),
        ("mark_seen" = Option<bool>, Query, description = "Record that To do was opened, so the expired count starts again"),
    ),
    responses(
        (status = 200, description = "The `TodoRunway` variant", body = ResponseData),
        (status = 401, description = "Missing or invalid bridge token")
    )
)]
#[allow(dead_code)]
fn mail_todos_runway() {}

#[utoipa::path(
    post,
    path = "/api/v1/mail/todos",
    summary = "Make a to-do from a message yourself (dry_run previews)",
    request_body = crate::todo_routes::TodoCreateBody,
    responses(
        (status = 200, description = "The `TodoChange` variant", body = ResponseData),
        (status = 401, description = "Missing or invalid bridge token")
    )
)]
#[allow(dead_code)]
fn mail_todos_create() {}

#[utoipa::path(
    get,
    path = "/api/v1/mail/todos/in/{state}",
    summary = "Every to-do in one state, newest change first; `expired` is the Expired list",
    params(
        ("state" = String, Path, description = "`open`, `done`, `dismissed` or `expired`"),
        ("account" = Option<String>, Query, description = "Account id; omitted covers every account"),
        ("limit" = Option<u32>, Query, description = "Rows (default 200)"),
    ),
    responses(
        (status = 200, description = "The `Todos` variant", body = ResponseData),
        (status = 400, description = "Unknown state"),
        (status = 401, description = "Missing or invalid bridge token")
    )
)]
#[allow(dead_code)]
fn mail_todos_in_state() {}

#[utoipa::path(
    get,
    path = "/api/v1/mail/todos/{todo_id}",
    summary = "One to-do with where each of its fields came from",
    params(("todo_id" = String, Path, description = "A full id or a unique prefix")),
    responses(
        (status = 200, description = "The `Todo` variant", body = ResponseData),
        (status = 401, description = "Missing or invalid bridge token")
    )
)]
#[allow(dead_code)]
fn mail_todo_get() {}

#[utoipa::path(
    post,
    path = "/api/v1/mail/todos/state",
    summary = "Tick off, reopen or mark to-dos not a to-do (dry_run previews)",
    request_body = crate::todo_routes::TodoStateBody,
    responses(
        (status = 200, description = "The `TodoChange` variant", body = ResponseData),
        (status = 401, description = "Missing or invalid bridge token")
    )
)]
#[allow(dead_code)]
fn mail_todos_state() {}

#[utoipa::path(
    post,
    path = "/api/v1/mail/todos/{todo_id}/schedule",
    summary = "Show a to-do on your own date, or clear it (dry_run previews)",
    params(("todo_id" = String, Path, description = "A full id or a unique prefix")),
    request_body = crate::todo_routes::TodoScheduleBody,
    responses(
        (status = 200, description = "The `TodoChange` variant", body = ResponseData),
        (status = 401, description = "Missing or invalid bridge token")
    )
)]
#[allow(dead_code)]
fn mail_todo_schedule() {}

#[utoipa::path(
    post,
    path = "/api/v1/mail/todos/{todo_id}/edit",
    summary = "Correct a to-do's fields; it is yours from then on (dry_run previews)",
    params(("todo_id" = String, Path, description = "A full id or a unique prefix")),
    request_body = crate::todo_routes::TodoEditBody,
    responses(
        (status = 200, description = "The `TodoChange` variant", body = ResponseData),
        (status = 401, description = "Missing or invalid bridge token")
    )
)]
#[allow(dead_code)]
fn mail_todo_edit() {}

#[utoipa::path(
    get,
    path = "/api/v1/mail/todos/catchup",
    summary = "The first run's one-time catch-up, with what was already over",
    params(("account" = Option<String>, Query, description = "Account id; omitted covers every account")),
    responses(
        (status = 200, description = "The `TodoCatchup` variant", body = ResponseData),
        (status = 401, description = "Missing or invalid bridge token")
    )
)]
#[allow(dead_code)]
fn mail_todos_catchup_get() {}

#[utoipa::path(
    get,
    path = "/api/v1/mail/modes/guide",
    summary = "How a mode explains itself: header, empty states, why template, keys and hints",
    params(("mode" = Option<String>, Query, description = "`todo`; omitted returns every shipped mode")),
    responses(
        (status = 200, description = "The `ModeGuides` variant", body = ResponseData),
        (status = 401, description = "Missing or invalid bridge token")
    )
)]
#[allow(dead_code)]
fn mail_mode_guide() {}

#[utoipa::path(
    post,
    path = "/api/v1/mail/hints/{hint}",
    summary = "Dismiss a hint in every client, or show it again",
    params(("hint" = String, Path, description = "`todo.runway`")),
    request_body = crate::mode_routes::HintSeenBody,
    responses(
        (status = 200, description = "The `ModeGuides` variant", body = ResponseData),
        (status = 401, description = "Missing or invalid bridge token")
    )
)]
#[allow(dead_code)]
fn mail_hint_seen() {}

#[utoipa::path(
    get,
    path = "/api/v1/mail/arrivals",
    summary = "Now's arrivals line: every email first seen since Now was last opened, counted once by where it went, with Not-sure questions and the track record",
    params(
        ("account" = Option<String>, Query, description = "Account id; omitted covers every account"),
        ("mark_seen" = Option<bool>, Query, description = "True when Now opens: starts a visit"),
        ("since" = Option<String>, Query, description = "The window start the open visit was answered; a refetch that doesn't mark keeps it")
    ),
    responses(
        (status = 200, description = "The `Arrivals` variant", body = ResponseData),
        (status = 401, description = "Missing or invalid bridge token")
    )
)]
#[allow(dead_code)]
fn mail_arrivals() {}

#[utoipa::path(
    get,
    path = "/api/v1/mail/arrivals/list",
    summary = "The emails behind one count of the arrivals line, newest first, exactly as many as the count",
    params(
        ("account" = Option<String>, Query, description = "Account id; omitted covers every account"),
        ("bucket" = Option<String>, Query, description = "messages, todo, updates, reading, archive, screened_out, spam or sorting; omitted lists every arrival"),
        ("since" = Option<String>, Query, description = "RFC 3339; defaults to the line's window"),
        ("until" = Option<String>, Query, description = "RFC 3339; defaults to now"),
        ("limit" = Option<u32>, Query, description = "Default 100")
    ),
    responses(
        (status = 200, description = "The `ArrivalList` variant", body = ResponseData),
        (status = 400, description = "Unknown bucket, or until not after since"),
        (status = 401, description = "Missing or invalid bridge token")
    )
)]
#[allow(dead_code)]
fn mail_arrivals_list() {}

#[utoipa::path(
    post,
    path = "/api/v1/mail/arrivals/modes",
    summary = "Where each of up to 200 emails went and where it is now, for Inbox's mode chips",
    request_body = crate::arrival_routes::ArrivalModesBody,
    responses(
        (status = 200, description = "The `ArrivalModes` variant", body = ResponseData),
        (status = 400, description = "No ids, more than 200, or a bad id"),
        (status = 401, description = "Missing or invalid bridge token")
    )
)]
#[allow(dead_code)]
fn mail_arrivals_modes() {}

#[utoipa::path(
    post,
    path = "/api/v1/mail/messages/{message_id}/move",
    summary = "Move one email to a mode (X), or with sender set the sender's mode (K); To do and Archive add it there (dry_run previews)",
    params(("message_id" = String, Path, description = "The email")),
    request_body = crate::arrival_routes::MoveBody,
    responses(
        (status = 200, description = "The `MessageMoved` variant, with the correction id undo takes", body = ResponseData),
        (status = 400, description = "Unknown mode or bad id"),
        (status = 401, description = "Missing or invalid bridge token")
    )
)]
#[allow(dead_code)]
fn mail_message_move() {}

#[utoipa::path(
    post,
    path = "/api/v1/mail/moves/{correction_id}/undo",
    summary = "Put a move back exactly as it was; undoing twice changes nothing",
    params(("correction_id" = i64, Path, description = "From the move's outcome")),
    responses(
        (status = 200, description = "The `MoveUndone` variant", body = ResponseData),
        (status = 401, description = "Missing or invalid bridge token")
    )
)]
#[allow(dead_code)]
fn mail_move_undo() {}

#[utoipa::path(
    get,
    path = "/api/v1/mail/corrections",
    summary = "Every move, sender mode and Not-sure answer, newest first",
    params(
        ("account" = Option<String>, Query, description = "Account id; omitted covers every account"),
        ("limit" = Option<u32>, Query, description = "Default 50")
    ),
    responses(
        (status = 200, description = "The `Corrections` variant", body = ResponseData),
        (status = 401, description = "Missing or invalid bridge token")
    )
)]
#[allow(dead_code)]
fn mail_corrections() {}

#[utoipa::path(
    get,
    path = "/api/v1/mail/now",
    summary = "Now: People, Due soon, one Updates card and an evening Reading pick, at most three items each",
    params(("account" = Option<String>, Query, description = "Account id; omitted covers every account")),
    responses(
        (status = 200, description = "The `Now` variant", body = ResponseData),
        (status = 401, description = "Missing or invalid bridge token")
    )
)]
#[allow(dead_code)]
fn mail_now() {}

#[utoipa::path(
    get,
    path = "/api/v1/mail/rail",
    summary = "The rail: Now, the five modes and Inbox with keys, counts and early-version notes, plus More",
    params(("account" = Option<String>, Query, description = "Account id; omitted covers every account")),
    responses(
        (status = 200, description = "The `Rail` variant", body = ResponseData),
        (status = 401, description = "Missing or invalid bridge token")
    )
)]
#[allow(dead_code)]
fn mail_rail() {}

#[utoipa::path(
    get,
    path = "/api/v1/mail/freshness",
    summary = "Freshness: the newest mail received, each account's sync health and the last arrivals with their modes",
    params(
        ("account" = Option<String>, Query, description = "Account id; omitted covers every account: the freshest mail and the worst sync state"),
        ("limit" = Option<u32>, Query, description = "Arrivals to return; default 5, at most 50"),
    ),
    responses(
        (status = 200, description = "The `Freshness` variant", body = ResponseData),
        (status = 401, description = "Missing or invalid bridge token")
    )
)]
#[allow(dead_code)]
fn mail_freshness() {}

#[utoipa::path(
    get,
    path = "/api/v1/mail/modes/membership",
    summary = "Which modes hold one thread and why, named by the thread or one of its messages",
    params(
        ("thread_id" = Option<String>, Query, description = "The thread"),
        ("message_id" = Option<String>, Query, description = "Or one of its messages"),
    ),
    responses(
        (status = 200, description = "The `ModeMembership` variant", body = ResponseData),
        (status = 400, description = "Neither id given"),
        (status = 401, description = "Missing or invalid bridge token")
    )
)]
#[allow(dead_code)]
fn mail_mode_membership_get() {}

#[utoipa::path(
    post,
    path = "/api/v1/mail/modes/membership",
    summary = "Which modes hold each of up to 100 threads, in request order",
    request_body = crate::mode_routes::ModeMembershipBody,
    responses(
        (status = 200, description = "The `ModeMembership` variant", body = ResponseData),
        (status = 400, description = "No threads, or more than 100"),
        (status = 401, description = "Missing or invalid bridge token")
    )
)]
#[allow(dead_code)]
fn mail_mode_membership_post() {}

#[utoipa::path(
    post,
    path = "/api/v1/mail/modes/{mode}/done",
    summary = "Done here: threads, a sender's threads or named to-dos leave one mode, archived only when no other mode holds them (dry_run previews)",
    params(("mode" = String, Path, description = "`messages`, `todo`, `updates` or `reading`")),
    request_body = crate::mode_routes::ModeDoneBody,
    responses(
        (status = 200, description = "The `ModeDone` variant: one outcome per thread with its handoff copy, and the undo id", body = ResponseData),
        (status = 400, description = "Unknown mode or no threads"),
        (status = 401, description = "Missing or invalid bridge token")
    )
)]
#[allow(dead_code)]
fn mail_mode_done() {}

#[utoipa::path(
    get,
    path = "/api/v1/mail/people",
    summary = "Messages: people you talk with in four bands (Your turn, Pinned, Recent, Quiet), each row with its topics and what they asked",
    params(
        ("account" = Option<String>, Query, description = "Account id; omitted covers every account"),
        ("turn" = Option<String>, Query, description = "`mine` or `theirs`: only rows where it is that side's turn"),
        ("limit" = Option<u32>, Query, description = "Rows in Recent and Quiet (default 50); totals count all"),
    ),
    responses(
        (status = 200, description = "The `Messages` variant", body = ResponseData),
        (status = 401, description = "Missing or invalid bridge token")
    )
)]
#[allow(dead_code)]
fn mail_people() {}

#[utoipa::path(
    get,
    path = "/api/v1/mail/people/page",
    summary = "A person's page: relationship line, every topic, and the selected topic as new text with trimmed markers",
    params(
        ("person" = String, Query, description = "A row id (`person:<email>`, `group:<thread>`) or an address"),
        ("topic" = Option<String>, Query, description = "Thread id of the conversation to show"),
        ("account" = Option<String>, Query, description = "Account id; omitted searches every account"),
    ),
    responses(
        (status = 200, description = "The `PersonPage` variant", body = ResponseData),
        (status = 401, description = "Missing or invalid bridge token")
    )
)]
#[allow(dead_code)]
fn mail_people_page() {}

#[utoipa::path(
    post,
    path = "/api/v1/mail/people/ack",
    summary = "Got it: a short acknowledgement in your usual greeting and sign-off (dry_run previews the exact text)",
    request_body = crate::messages_routes::AckBody,
    responses(
        (status = 200, description = "The `MessagesAck` variant", body = ResponseData),
        (status = 401, description = "Missing or invalid bridge token")
    )
)]
#[allow(dead_code)]
fn mail_people_ack() {}

#[utoipa::path(
    post,
    path = "/api/v1/mail/people/merge",
    summary = "Merge addresses into one person, by hand (dry_run previews)",
    request_body = crate::messages_routes::MergeBody,
    responses(
        (status = 200, description = "The `PersonMerge` variant", body = ResponseData),
        (status = 401, description = "Missing or invalid bridge token")
    )
)]
#[allow(dead_code)]
fn mail_people_merge() {}

#[utoipa::path(
    post,
    path = "/api/v1/mail/people/split",
    summary = "Take an address back out of its person (dry_run previews)",
    request_body = crate::messages_routes::SplitBody,
    responses(
        (status = 200, description = "The `PersonMerge` variant", body = ResponseData),
        (status = 401, description = "Missing or invalid bridge token")
    )
)]
#[allow(dead_code)]
fn mail_people_split() {}

#[utoipa::path(
    get,
    path = "/api/v1/mail/people/merge-suggestions",
    summary = "Merges mxr suggests: the same name on addresses you have written to. Never applied on its own",
    params(("account" = Option<String>, Query, description = "Account id; omitted covers every account")),
    responses(
        (status = 200, description = "The `MergeSuggestions` variant", body = ResponseData),
        (status = 401, description = "Missing or invalid bridge token")
    )
)]
#[allow(dead_code)]
fn mail_people_merge_suggestions() {}

#[utoipa::path(
    get,
    path = "/api/v1/mail/records",
    summary = "Archive's ledger: records by month with counts, totals, facets and what is coming up",
    params(
        ("account" = Option<String>, Query, description = "Account id; omitted covers every account"),
        ("kind" = Option<String>, Query, description = "Comma-separated kinds: receipt, order, booking, invoice, statement, ticket, contract, warranty, account"),
        ("issuer" = Option<String>, Query, description = "One issuer: the issuer page"),
        ("year" = Option<i32>, Query, description = "By the transaction's date"),
        ("min_amount_minor" = Option<i64>, Query, description = "At least this, in minor units"),
        ("max_amount_minor" = Option<i64>, Query, description = "At most this, in minor units"),
        ("has_pdf" = Option<bool>, Query, description = "With or without a PDF"),
        ("checked" = Option<bool>, Query, description = "Every amount and date confirmed, or not"),
        ("group" = Option<String>, Query, description = "One trip or series"),
        ("limit" = Option<u32>, Query, description = "Rows on this page (default 200)"),
        ("offset" = Option<u32>, Query, description = "Rows to skip"),
    ),
    responses(
        (status = 200, description = "The `RecordLedger` variant", body = ResponseData),
        (status = 400, description = "Unknown kind"),
        (status = 401, description = "Missing or invalid bridge token")
    )
)]
#[allow(dead_code)]
fn mail_records_ledger() {}

#[utoipa::path(
    get,
    path = "/api/v1/mail/records/answer",
    summary = "The answer box: the field a query asks for from record fields, with no model; falls back to mxr ask only when no record matches",
    params(
        ("q" = String, Query, description = "What you remember: \"lisbon booking ref\""),
        ("account" = Option<String>, Query, description = "Account id; omitted covers every account"),
        ("fallback" = Option<bool>, Query, description = "Fall back to mxr ask over all mail (default true)"),
        ("limit" = Option<u32>, Query, description = "Records in \"also matching\" (default 4)"),
        ("list" = Option<bool>, Query, description = "Every match as a list, whatever the query asks for (default false)"),
        ("offset" = Option<u32>, Query, description = "Where a list's page starts (default 0)"),
        ("list_limit" = Option<u32>, Query, description = "Records in a list's page (default 200)"),
    ),
    responses(
        (status = 200, description = "The `RecordAnswer` variant", body = ResponseData),
        (status = 400, description = "Empty query"),
        (status = 401, description = "Missing or invalid bridge token")
    )
)]
#[allow(dead_code)]
fn mail_records_answer() {}

#[utoipa::path(
    get,
    path = "/api/v1/mail/records/subscriptions",
    summary = "Subscriptions: receipts and invoices at a steady cadence, with next charge, yearly cost, price changes, status and totals per currency",
    params(("account" = Option<String>, Query, description = "Account id; omitted covers every account")),
    responses(
        (status = 200, description = "The `RecordSubscriptions` variant", body = ResponseData),
        (status = 400, description = "Bad account id"),
        (status = 401, description = "Missing or invalid bridge token")
    )
)]
#[allow(dead_code)]
fn mail_records_subscriptions() {}

#[utoipa::path(
    get,
    path = "/api/v1/mail/records/{record_id}",
    summary = "One record's card: every field with where it came from, its documents and source emails",
    params(("record_id" = String, Path, description = "A full id or a unique prefix")),
    responses(
        (status = 200, description = "The `Record` variant", body = ResponseData),
        (status = 401, description = "Missing or invalid bridge token")
    )
)]
#[allow(dead_code)]
fn mail_record_get() {}

#[utoipa::path(
    post,
    path = "/api/v1/mail/records/{record_id}/field",
    summary = "Fix, confirm or clear a record field, or mark the card checked (dry_run previews)",
    params(("record_id" = String, Path, description = "A full id or a unique prefix")),
    request_body = crate::record_routes::RecordFieldBody,
    responses(
        (status = 200, description = "The `RecordChange` variant", body = ResponseData),
        (status = 401, description = "Missing or invalid bridge token")
    )
)]
#[allow(dead_code)]
fn mail_record_field() {}

#[utoipa::path(
    post,
    path = "/api/v1/mail/records/dismiss",
    summary = "Not a record, or back again; the email is never touched (dry_run previews)",
    request_body = crate::record_routes::RecordDismissBody,
    responses(
        (status = 200, description = "The `RecordChange` variant", body = ResponseData),
        (status = 400, description = "No record ids"),
        (status = 401, description = "Missing or invalid bridge token")
    )
)]
#[allow(dead_code)]
fn mail_records_dismiss() {}

#[utoipa::path(
    post,
    path = "/api/v1/mail/records/file",
    summary = "File an email as a record yourself (dry_run returns the card it would file)",
    request_body = crate::record_routes::RecordFileBody,
    responses(
        (status = 200, description = "The `RecordChange` variant", body = ResponseData),
        (status = 400, description = "Invalid message id"),
        (status = 401, description = "Missing or invalid bridge token")
    )
)]
#[allow(dead_code)]
fn mail_records_file() {}

#[utoipa::path(
    post,
    path = "/api/v1/mail/records/sender",
    summary = "Always or never file a sender's mail (dry_run previews)",
    request_body = crate::record_routes::RecordSenderBody,
    responses(
        (status = 200, description = "The `RecordChange` variant", body = ResponseData),
        (status = 400, description = "Invalid message id or verdict"),
        (status = 401, description = "Missing or invalid bridge token")
    )
)]
#[allow(dead_code)]
fn mail_records_sender() {}

#[utoipa::path(
    post,
    path = "/api/v1/mail/records/export",
    summary = "Export records as CSV, and PDFs to a folder; dry_run returns the row count, totals, unchecked rows and missing PDFs for the same rows",
    request_body = crate::record_routes::RecordExportBody,
    responses(
        (status = 200, description = "The `RecordExport` variant", body = ResponseData),
        (status = 401, description = "Missing or invalid bridge token")
    )
)]
#[allow(dead_code)]
fn mail_records_export() {}

#[utoipa::path(
    get,
    path = "/api/v1/mail/reading",
    summary = "Reading's edition: bands since your last visit, earlier and fading, ranked by what you read, with Later and the sources",
    params(
        ("account" = Option<String>, Query, description = "Account id; omitted covers every account"),
        ("mark_visit" = Option<bool>, Query, description = "Record that Reading was opened"),
    ),
    responses(
        (status = 200, description = "The `ReadingEdition` variant", body = ResponseData),
        (status = 401, description = "Missing or invalid bridge token")
    )
)]
#[allow(dead_code)]
fn mail_reading_edition() {}

#[utoipa::path(
    get,
    path = "/api/v1/mail/reading/items/{item_key}",
    summary = "One Reading item for the reader: text, saved article, highlights and minutes left. Never fetches",
    params(("item_key" = String, Path, description = "`<message id>:<index>`, URL-encoded")),
    responses(
        (status = 200, description = "The `ReadingItem` variant", body = ResponseData),
        (status = 401, description = "Missing or invalid bridge token")
    )
)]
#[allow(dead_code)]
fn mail_reading_item() {}

#[utoipa::path(
    post,
    path = "/api/v1/mail/reading/later",
    summary = "Put Reading items on Later or take them off (dry_run previews)",
    request_body = crate::reading_routes::ReadingLaterBody,
    responses(
        (status = 200, description = "The `ReadingLater` variant", body = ResponseData),
        (status = 401, description = "Missing or invalid bridge token")
    )
)]
#[allow(dead_code)]
fn mail_reading_later() {}

#[utoipa::path(
    post,
    path = "/api/v1/mail/reading/engagement",
    summary = "Local reading engagement (opened, time, progress); not written with MXR_ACTIVITY=off",
    request_body = crate::reading_routes::ReadingEngagementBody,
    responses(
        (status = 200, description = "The `ReadingEngagement` variant", body = ResponseData),
        (status = 401, description = "Missing or invalid bridge token")
    )
)]
#[allow(dead_code)]
fn mail_reading_engagement() {}

#[utoipa::path(
    post,
    path = "/api/v1/mail/reading/items/{item_key}/article",
    summary = "Fetch and save the article a Reading item links to, contacting its site; private addresses are refused",
    params(("item_key" = String, Path, description = "`<message id>:<index>`, URL-encoded")),
    request_body = crate::reading_routes::ReadingArticleBody,
    responses(
        (status = 200, description = "The `ReadingArticle` variant", body = ResponseData),
        (status = 401, description = "Missing or invalid bridge token")
    )
)]
#[allow(dead_code)]
fn mail_reading_article() {}

#[utoipa::path(
    get,
    path = "/api/v1/mail/reading/highlights",
    summary = "Every Reading highlight, with the same as one Markdown document",
    params(("account" = Option<String>, Query, description = "Account id; omitted covers every account")),
    responses(
        (status = 200, description = "The `ReadingHighlights` variant", body = ResponseData),
        (status = 401, description = "Missing or invalid bridge token")
    )
)]
#[allow(dead_code)]
fn mail_reading_highlights_get() {}

#[utoipa::path(
    post,
    path = "/api/v1/mail/reading/highlights",
    summary = "Save a passage from a Reading item, with an optional note",
    request_body = crate::reading_routes::ReadingHighlightBody,
    responses(
        (status = 200, description = "The `ReadingHighlight` variant", body = ResponseData),
        (status = 401, description = "Missing or invalid bridge token")
    )
)]
#[allow(dead_code)]
fn mail_reading_highlights_post() {}

#[utoipa::path(
    post,
    path = "/api/v1/mail/reading/sources",
    summary = "Per-source Reading settings: the sender's own layout, or no more unsubscribe offers",
    request_body = crate::reading_routes::ReadingSourceBody,
    responses(
        (status = 200, description = "The `ReadingSource` variant", body = ResponseData),
        (status = 401, description = "Missing or invalid bridge token")
    )
)]
#[allow(dead_code)]
fn mail_reading_sources() {}

#[utoipa::path(
    get,
    path = "/api/v1/mail/updates",
    summary = "The Updates digest: one line per source in Needs a look, Changed and Routine, at the latest cut or a past one, plus what arrived since",
    params(
        ("account" = Option<String>, Query, description = "Account id; omitted covers every account"),
        ("cut" = Option<String>, Query, description = "A past cut, RFC3339; the latest when omitted"),
        ("mark_seen" = Option<bool>, Query, description = "Record that Updates was opened"),
        ("expired" = Option<bool>, Query, description = "List every update past its window"),
    ),
    responses(
        (status = 200, description = "The `UpdatesDigest` variant", body = ResponseData),
        (status = 401, description = "Missing or invalid bridge token")
    )
)]
#[allow(dead_code)]
fn mail_updates_digest() {}

#[utoipa::path(
    post,
    path = "/api/v1/mail/updates/let-go",
    summary = "Let go of a digest or one of its sources: exactly the cut's updates leave Updates, archived only when no other mode holds them (dry_run previews; selection_token pins the run to the preview)",
    request_body = crate::updates_routes::UpdatesLetGoBody,
    responses(
        (status = 200, description = "The `UpdatesLetGo` variant, with the undo id", body = ResponseData),
        (status = 401, description = "Missing or invalid bridge token")
    )
)]
#[allow(dead_code)]
fn mail_updates_let_go() {}

#[utoipa::path(
    post,
    path = "/api/v1/mail/updates/sources",
    summary = "Tune an Updates source: every digest, changes only, muted or breakthrough (dry_run previews)",
    request_body = crate::updates_routes::UpdateSourceBody,
    responses(
        (status = 200, description = "The `UpdateSource` variant, with the prior setting for undo", body = ResponseData),
        (status = 400, description = "No source named"),
        (status = 401, description = "Missing or invalid bridge token")
    )
)]
#[allow(dead_code)]
fn mail_updates_source() {}

#[utoipa::path(
    post,
    path = "/api/v1/mail/todos/catchup",
    summary = "Keep or let go in the catch-up; let_go_all takes every row still waiting (dry_run previews)",
    request_body = crate::todo_routes::TodoCatchupBody,
    responses(
        (status = 200, description = "The `TodoChange` variant", body = ResponseData),
        (status = 401, description = "Missing or invalid bridge token")
    )
)]
#[allow(dead_code)]
fn mail_todos_catchup_set() {}

/// Body of `POST /api/v1/mail/places/{place}/sweep`.
#[derive(utoipa::ToSchema)]
#[allow(dead_code)]
struct SweepPlaceBody {
    /// Omitted: every account.
    account_id: Option<String>,
    /// Only this sender's bundle.
    sender_email: Option<String>,
    /// Preview only; nothing is archived.
    dry_run: Option<bool>,
    /// From the dry run's preview; required unless `dry_run`. The sweep
    /// archives only what that preview listed.
    preview_token: Option<String>,
}

/// Body of `POST /api/v1/mail/messages/pin`.
#[derive(utoipa::ToSchema)]
#[allow(dead_code)]
struct PinMessagesBody {
    message_ids: Vec<String>,
    pinned: bool,
}

/// Body of `POST /api/v1/mail/senders/kind`.
#[derive(utoipa::ToSchema)]
#[allow(dead_code)]
struct SenderKindBody {
    account_id: String,
    sender_email: String,
    /// `null` or omitted: back to automatic.
    kind: Option<mxr_protocol::SenderKindData>,
}

/// Reading or Paper trail, bundled by sender, with the reason for each.
#[utoipa::path(
    get,
    path = "/api/v1/mail/places/{place}",
    summary = "List a place (Reading or Paper trail) as bundles by sender",
    params(
        ("place" = String, Path, description = "`reading` or `paper-trail`"),
        ("account" = Option<String>, Query, description = "Account id; omitted covers every account"),
        ("sender" = Option<String>, Query, description = "Only this sender's bundle"),
        ("limit" = Option<u32>, Query, description = "Bundles per page (default 50)"),
        ("offset" = Option<u32>, Query, description = "Bundles to skip"),
        ("messages_per_bundle" = Option<u32>, Query, description = "Messages listed per bundle (default 20)"),
        ("message_offset" = Option<u32>, Query, description = "Messages to skip in each bundle, for paging one sender"),
    ),
    responses(
        (status = 200, description = "The `Place` variant", body = ResponseData),
        (status = 400, description = "Unknown place or bad account id"),
        (status = 401, description = "Missing or invalid bridge token")
    )
)]
#[allow(dead_code)]
fn mail_place_list() {}

#[utoipa::path(
    post,
    path = "/api/v1/mail/places/{place}/sweep",
    summary = "Archive everything unpinned in a place or one sender's bundle (dry_run previews)",
    params(("place" = String, Path, description = "`reading` or `paper-trail`")),
    request_body = SweepPlaceBody,
    responses(
        (
            status = 200,
            description = "The `PlaceSwept` variant: the preview, and the archive job unless dry_run",
            body = ResponseData
        ),
        (status = 401, description = "Missing or invalid bridge token")
    )
)]
#[allow(dead_code)]
fn mail_place_sweep() {}

#[utoipa::path(
    get,
    path = "/api/v1/mail/messages/{message_id}/kind",
    summary = "Why a message is where it is: its kind, rule and reason",
    params(("message_id" = String, Path, description = "Message id")),
    responses(
        (status = 200, description = "The `MessageKind` variant", body = ResponseData),
        (status = 401, description = "Missing or invalid bridge token")
    )
)]
#[allow(dead_code)]
fn mail_message_kind() {}

#[utoipa::path(
    post,
    path = "/api/v1/mail/messages/pin",
    summary = "Pin or unpin messages so a sweep leaves them where they are",
    request_body = PinMessagesBody,
    responses(
        (status = 200, description = "The `MessagesPinned` variant", body = ResponseData),
        (status = 401, description = "Missing or invalid bridge token")
    )
)]
#[allow(dead_code)]
fn mail_messages_pin() {}

#[utoipa::path(
    post,
    path = "/api/v1/mail/senders/kind",
    summary = "Move a sender to people, Reading, Paper trail or screened out, or back to automatic",
    request_body = SenderKindBody,
    responses(
        (
            status = 200,
            description = "The `SenderKindSet` variant, with the previous kind for undo",
            body = ResponseData
        ),
        (status = 401, description = "Missing or invalid bridge token")
    )
)]
#[allow(dead_code)]
fn mail_sender_kind() {}

endpoint!(post compose_session_start "/api/v1/mail/compose/session", "Start compose session");
endpoint!(post compose_session_refresh "/api/v1/mail/compose/session/refresh", "Refresh compose session");
#[utoipa::path(
    post,
    path = "/api/v1/mail/compose/session/restore",
    summary = "Restore compose session",
    responses(
        (status = 200, description = "OK"),
        (status = 401, description = "Missing or invalid bridge token"),
        (
            status = 409,
            description = "The draft's body is a supplied HTML document, which the markdown \
                           compose editor cannot represent. Body carries `code: \
                           \"html_draft_not_editable\"` and `previewHtml` (the document \
                           verbatim, for read-only display). Retrying cannot succeed."
        )
    )
)]
#[allow(dead_code)]
fn compose_session_restore() {}
endpoint!(post compose_session_update "/api/v1/mail/compose/session/update", "Update compose session");
endpoint!(post compose_session_send "/api/v1/mail/compose/session/send", "Send compose session");
endpoint!(post compose_session_safety_check "/api/v1/mail/compose/session/safety-check", "Run the pre-send safety report for a compose session");
endpoint!(post compose_session_collaborators "/api/v1/mail/compose/session/collaborators", "Suggest maybe-include recipients for a compose session");
endpoint!(post compose_session_save "/api/v1/mail/compose/session/save", "Save compose session");
endpoint!(post compose_session_schedule "/api/v1/mail/compose/session/schedule", "Store a compose session as a local draft and schedule it");
endpoint!(post compose_session_attachment "/api/v1/mail/compose/session/attachment", "Upload compose attachment");
endpoint!(post compose_session_discard "/api/v1/mail/compose/session/discard", "Discard compose session");

endpoint!(get rules_list "/api/v1/platform/rules", "List rules");
endpoint!(get rule_detail "/api/v1/platform/rules/detail", "Rule detail");
endpoint!(get rule_form "/api/v1/platform/rules/form", "Rule form payload");
endpoint!(get rule_history "/api/v1/platform/rules/history", "Rule history");
endpoint!(get rule_dry_run "/api/v1/platform/rules/dry-run", "Dry-run rules");
endpoint!(post rule_upsert "/api/v1/platform/rules/upsert", "Create or update rule");
endpoint!(post rule_upsert_form "/api/v1/platform/rules/upsert-form", "Create or update rule from form");
endpoint!(post rule_delete "/api/v1/platform/rules/delete", "Delete rule");

endpoint!(get saved_searches_list "/api/v1/platform/saved-searches", "List saved searches");
endpoint!(post saved_searches_create "/api/v1/platform/saved-searches/create", "Create saved search");
endpoint!(post saved_searches_update "/api/v1/platform/saved-searches/update", "Patch a saved search by name");
endpoint!(post saved_searches_delete "/api/v1/platform/saved-searches/delete", "Delete saved search");
endpoint!(post saved_searches_run "/api/v1/platform/saved-searches/run", "Run saved search");
endpoint!(get saved_searches_unread_counts "/api/v1/platform/saved-searches/unread-counts", "Unread match counts per saved search");

endpoint!(get accounts_list "/api/v1/platform/accounts", "List runtime accounts");
endpoint!(get accounts_config "/api/v1/platform/accounts/config", "List configured accounts");
endpoint!(post account_test "/api/v1/platform/accounts/test", "Test account connectivity");
endpoint!(post account_upsert "/api/v1/platform/accounts/upsert", "Create or update account");
endpoint!(post account_set_default "/api/v1/platform/accounts/default", "Set default account");
endpoint!(delete account_remove "/api/v1/platform/accounts/{key}", "Remove account");
endpoint!(post account_disable "/api/v1/platform/accounts/{key}/disable", "Disable account");
endpoint!(get account_addresses_list "/api/v1/platform/accounts/{account_id}/addresses", "List account addresses");
endpoint!(post account_addresses_add "/api/v1/platform/accounts/{account_id}/addresses", "Add account address");
endpoint!(post account_addresses_remove "/api/v1/platform/accounts/{account_id}/addresses/remove", "Remove account address");
endpoint!(post account_addresses_primary "/api/v1/platform/accounts/{account_id}/addresses/primary", "Set primary account address");

endpoint!(post auth_session_start "/api/v1/platform/auth/sessions/start", "Start OAuth session");
endpoint!(get auth_session_get "/api/v1/platform/auth/sessions/{session_id}", "Get OAuth session");
endpoint!(post auth_session_cancel "/api/v1/platform/auth/sessions/{session_id}/cancel", "Cancel OAuth session");
endpoint!(post auth_session_complete "/api/v1/platform/auth/sessions/{session_id}/complete", "Complete OAuth session");

endpoint!(get subscriptions_list "/api/v1/platform/subscriptions", "List subscriptions");
endpoint!(get llm_status "/api/v1/platform/llm/status", "LLM provider status");
endpoint!(get llm_config_get "/api/v1/platform/llm/config", "Get LLM configuration");
endpoint!(post llm_config_update "/api/v1/platform/llm/config", "Update LLM configuration");
endpoint!(get notification_chimes_get "/api/v1/platform/notifications/chimes", "Get the notification chime setting");
endpoint!(post notification_chimes_update "/api/v1/platform/notifications/chimes", "Change fields of the notification chime setting (the body is a NotificationChimesPatchData)");
endpoint!(get semantic_status "/api/v1/platform/semantic/status", "Semantic index status");
endpoint!(post semantic_reindex "/api/v1/platform/semantic/reindex", "Reindex semantic search");
endpoint!(post semantic_enable "/api/v1/platform/semantic/enable", "Enable semantic search");
endpoint!(post semantic_profile_install "/api/v1/platform/semantic/profiles/install", "Install semantic profile");
endpoint!(post semantic_profile_use "/api/v1/platform/semantic/profiles/use", "Use semantic profile");
endpoint!(post semantic_backfill "/api/v1/platform/semantic/backfill", "Backfill semantic chunks");

endpoint!(get analytics_wrapped "/api/v1/platform/analytics/wrapped", "Wrapped analytics");
endpoint!(get analytics_storage_breakdown "/api/v1/platform/analytics/storage-breakdown", "Storage breakdown");
endpoint!(get analytics_largest_messages "/api/v1/platform/analytics/largest-messages", "Largest messages");
endpoint!(get analytics_stale_threads "/api/v1/platform/analytics/stale-threads", "Stale threads");
endpoint!(get analytics_contact_asymmetry "/api/v1/platform/analytics/contact-asymmetry", "Contact asymmetry");
endpoint!(get analytics_contact_decay "/api/v1/platform/analytics/contact-decay", "Contact decay");
endpoint!(get analytics_response_time "/api/v1/platform/analytics/response-time", "Response-time analytics");
endpoint!(post analytics_refresh_contacts "/api/v1/platform/analytics/refresh-contacts", "Refresh contacts");
endpoint!(post analytics_rebuild "/api/v1/platform/analytics/rebuild", "Rebuild analytics");
endpoint!(get analytics_cadence_drift "/api/v1/platform/analytics/cadence-drift", "Watched contacts past their usual cadence");
endpoint!(get cadence_watch_list "/api/v1/platform/cadence/watch", "List the cadence watchlist");
endpoint!(post cadence_watch "/api/v1/platform/cadence/watch", "Add a contact to the cadence watchlist");
endpoint!(post cadence_unwatch "/api/v1/platform/cadence/unwatch", "Remove a contact from the cadence watchlist");

endpoint!(get mail_message_body "/api/v1/mail/messages/{message_id}/body", "Get message body (IPC GetBody)");
endpoint!(get mail_message_html_images "/api/v1/mail/messages/{message_id}/html-images", "List HTML-linked image assets");
endpoint!(get mail_message_inline_image "/api/v1/mail/messages/{message_id}/inline-image", "Bytes of one inline (cid:) image, by its HTML src");
endpoint!(get mail_message_raw_headers "/api/v1/mail/messages/{message_id}/headers", "Raw RFC headers");
endpoint!(post mail_message_set_flags "/api/v1/mail/messages/{message_id}/flags", "Set message flags bitmask");
endpoint!(post mail_export_search "/api/v1/mail/export-search", "Export all threads matching a search");
endpoint!(get mail_drafts_orphaned_list "/api/v1/mail/drafts/orphaned", "List orphaned mid-send drafts");
endpoint!(post mail_drafts_save_local "/api/v1/mail/drafts/save-local", "Persist draft locally (SaveDraft)");
endpoint!(post mail_drafts_reset_orphan "/api/v1/mail/drafts/{draft_id}/reset-orphan", "Reset orphaned sending draft");
endpoint!(post mail_drafts_send_stored "/api/v1/mail/drafts/{draft_id}/send-stored", "Send stored draft by id");
endpoint!(delete mail_drafts_delete_stored "/api/v1/mail/drafts/{draft_id}/stored", "Delete stored draft");
endpoint!(get mail_signatures_list "/api/v1/mail/signatures", "List signatures");
endpoint!(post mail_signatures_upsert "/api/v1/mail/signatures", "Create or update signature");
endpoint!(get mail_signature_defaults_list "/api/v1/mail/signature-defaults", "List signature defaults");
endpoint!(post mail_signature_default_set "/api/v1/mail/signatures/default", "Set default signature");
endpoint!(post mail_signature_default_clear "/api/v1/mail/signatures/default/clear", "Clear default signature");
endpoint!(post mail_signature_resolve "/api/v1/mail/signatures/resolve", "Resolve signature for compose");
endpoint!(delete mail_signatures_delete "/api/v1/mail/signatures/{name}", "Delete signature");
endpoint!(post platform_accounts_authorize "/api/v1/platform/accounts/authorize", "Authorize or re-authorize account config");
endpoint!(post platform_accounts_repair "/api/v1/platform/accounts/repair", "Repair account credentials in local stores");
endpoint!(get platform_voice_get "/api/v1/platform/voice", "User voice profile for drafting");
endpoint!(post platform_voice_rebuild "/api/v1/platform/voice/rebuild", "Rebuild user voice profile from sent mail");

/// Registers the bearer-token security scheme so the Swagger UI "Authorize"
/// button works and so generated SDKs know the wire format.
struct BearerSecurity;

impl Modify for BearerSecurity {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        let components = openapi
            .components
            .get_or_insert_with(utoipa::openapi::Components::default);
        components.add_security_scheme(
            "bearer",
            SecurityScheme::Http(
                HttpBuilder::new()
                    .scheme(HttpAuthScheme::Bearer)
                    .bearer_format("opaque")
                    .description(Some(
                        "Token from the `bridge-token` file in the active \
                         profile's config directory, or from \
                         `[bridge].token_path` / `MXR_BRIDGE_TOKEN_PATH` when \
                         either is set. Send via \
                         `Authorization: Bearer <token>` header. \
                         Browser WebSocket clients can use the \
                         `Sec-WebSocket-Protocol: bearer, <token>` subprotocol. \
                         The `?token=<token>` query string is accepted only by \
                         `/api/v1/events` for command-line WebSocket fallback.",
                    ))
                    .build(),
            ),
        );
    }
}

/// The bridge's OpenAPI document, built once. utoipa's generated schema
/// code for the large `Request`/`ResponseData` enums needs a stack frame of
/// several megabytes in debug builds, more than a Tokio worker's 2 MiB: the
/// daemon aborted with a stack overflow on startup when the enum grew. It is
/// built on a thread with room to spare and shared from then on.
pub fn cached_spec() -> utoipa::openapi::OpenApi {
    use std::sync::OnceLock;
    static SPEC: OnceLock<utoipa::openapi::OpenApi> = OnceLock::new();
    SPEC.get_or_init(|| {
        std::thread::Builder::new()
            .name("openapi-spec".into())
            .stack_size(64 * 1024 * 1024)
            .spawn(ApiDoc::openapi)
            .expect("spawn the OpenAPI spec builder thread")
            .join()
            .expect("build the OpenAPI spec")
    })
    .clone()
}
