//! OpenAPI 3.1 surface for the mxr HTTP bridge.
//!
//! The Axum routes live in `lib.rs` and `routes_v6.rs`. The path inventory is
//! declared here so Swagger UI, the docs site, and generated clients see actual
//! endpoints instead of a schema-only OpenAPI document.

use crate::request_types::{
    ComposeSessionAudienceRequest, ComposeSessionPathRequest, ComposeSessionResponse,
    ComposeSessionRestoreRequest, ComposeSessionScheduleRequest, ComposeSessionSendRequest,
    ComposeSessionStartRequest, ComposeSessionUpdateRequest, DiscardedComposeResponse,
    DraftConflictResponse, SavedDraftResponse, ScheduledComposeResponse,
};
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
                       This document is a route inventory plus protocol \
                       schemas. Some routes include typed parameters and \
                       request and response schemas; others list only path \
                       and method metadata. Some live bridge routes are not \
                       declared here. \
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
        archive_workspace, archive_preview, archive_submit, archive_query,
        recovery_status, recovery_activate, recovery_prepare,
        mail_drafts, mail_snoozed, mail_count, mail_jobs, mail_job_detail, mail_sync_status, mail_sync, mail_sync_all, mail_sync_preview,
        mutation_archive, mutation_trash, mutation_spam, mutation_star,
        mutation_read, mutation_read_archive, mutation_labels, mutation_move,
        mutation_route, mutation_undo, action_snooze_presets, action_snooze,
        action_unsubscribe, action_unsubscribe_purge, action_commit_unsubscribe_preview, action_invite_reply, attachment_open, attachment_download, attachment_transfer_metadata, attachment_transfer_chunk,
        label_create, label_rename, label_delete, mail_unsnooze_one,
        categories_list, categories_create, categories_messages, categories_memberships, categories_name_preview,
        categories_rename, categories_assignment_preview, categories_assignment_apply,
        categories_delete, categories_undo,
        reply_later_list, reply_later_set, reminders_set, reminders_cancel,
        scheduled_sends_list, scheduled_sends_create, scheduled_sends_cancel, snippets_list,
        snippets_set, snippets_delete, sender_profile, contacts_autocomplete, screener_queue,
        screener_decisions_list, screener_decisions_set,
        screener_decisions_clear, thread_summarize, mail_draft_compose,
        mail_draft_refine, mail_compose_models, mail_compose_assist, mail_compose_apply, mail_humanizer_score, mail_humanizer_rewrite,
        mail_relationship_profile, mail_relationship_rebuild, mail_commitments_list,
        mail_commitments_resolve, mail_thread_briefing, mail_contacts_briefing, mail_contacts_expert,
        compose_session_start, compose_session_refresh, compose_session_restore, compose_session_audience,
        compose_session_update, compose_session_send, compose_session_safety_check,
        compose_session_collaborators, compose_session_save, compose_session_schedule,
        compose_session_attachment, compose_session_discard, rules_list, rule_detail, rule_form,
        rule_history, rule_dry_run, rule_upsert, rule_upsert_form, rule_delete, rule_treatment, rule_categories, category_why,
        saved_searches_list, saved_searches_create, saved_searches_delete, saved_searches_update,
        saved_searches_run, accounts_list, accounts_config, account_test,
        account_upsert, account_set_default, account_remove, account_disable, account_identity,
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
        mail_drafts_save_local, mail_drafts_reset_orphan, mail_drafts_send_stored, mail_send_outcome, mail_recover_send_outcome,
        mail_drafts_delete_stored, mail_signatures_list, mail_signatures_upsert,
        mail_signature_defaults_list, mail_signature_default_set,
        mail_signature_default_clear, mail_signature_resolve, mail_signatures_delete,
        platform_accounts_authorize, platform_accounts_repair, platform_voice_get,
        platform_voice_rebuild, mail_mutation_jobs, mail_owed, mail_desk, mail_desk_dismiss, mail_desk_restore, mail_desk_done, mail_desk_later, mail_whois,
        mail_send_time, mail_archive_ask, saved_searches_unread_counts,
        analytics_cadence_drift, cadence_watch_list, cadence_watch, cadence_unwatch,
        mail_time_resolve, mail_thread_context, mail_thread_gist, mail_thread_gists, mail_thread_previews,
        compose_session_promises, mail_commitments_record,
        mail_place_list, mail_place_sweep, mail_message_kind, mail_messages_pin,
        mail_sender_kind, mail_todos_runway, mail_todos_create, mail_todos_in_state,
        mail_todo_get, mail_todos_state, mail_todo_correct, mail_todo_undo_correction, mail_todo_resolve, mail_todo_schedule, mail_todo_edit,
        mail_microsoft_todo_status, mail_microsoft_todo_link, mail_microsoft_todo_sync,
        mail_todos_catchup_get, mail_todos_catchup_set, mail_mode_guide, mail_hint_seen,
        mail_classify_report, mail_now, mail_rail, mail_codes, mail_freshness, mail_mode_membership_get, mail_mode_membership_post, mail_mode_done,
        mail_people, mail_people_page, mail_people_ack, mail_people_merge, mail_people_split,
        mail_people_merge_suggestions,
        mail_records_ledger, mail_records_answer, mail_records_subscriptions, mail_record_get, mail_record_field,
        mail_records_dismiss, mail_records_file, mail_records_sender, mail_records_export,
        mail_reading_edition, mail_reading_item, mail_reading_later, mail_reading_engagement,
        mail_reading_article, mail_reading_highlights_get, mail_reading_highlights_post,
        mail_reading_highlight_note, mail_reading_highlight_remove,
        mail_reading_sources, mail_reading_marginalia_get, mail_reading_marginalia_post,
        mail_reading_marginalia_send, mail_reading_marginalia_sync,
        mail_assistant_availability, mail_assistant_list, mail_assistant_send,
        mail_assistant_get, mail_assistant_step_rows, mail_assistant_decide, mail_assistant_cancel, mail_assistant_delete,
        mail_assistant_auto_approve,
        mail_assistant_models,
        mail_assistant_set_model,
        mail_updates_digest, mail_updates_let_go, mail_updates_source,
        mail_arrivals, mail_arrivals_list, mail_arrivals_modes, mail_message_move,
        mail_move_undo, mail_corrections
    ),
    components(schemas(
        crate::request_types::AcceptedComposeDraft,
        crate::request_types::RecoveredComposeSession,
        crate::routes_v6::ContactSuggestion,
        crate::routes_v6::ContactsAutocompleteResponse,
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
        crate::todo_routes::TodoCorrectionBody,
        crate::todo_routes::TodoCorrectionUndoBody,
        crate::todo_routes::TodoResolutionBody,
        crate::todo_routes::TodoScheduleBody,
        crate::todo_routes::TodoEditBody,
        crate::todo_routes::TodoCreateBody,
        crate::todo_routes::TodoCatchupBody,
        crate::mode_routes::HintSeenBody,
        crate::mode_routes::ModeMembershipBody,
        crate::thread_context_routes::ThreadPreviewsBody,
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
        crate::reading_routes::ReadingHighlightNoteBody,
        crate::reading_routes::ReadingHighlightRemoveBody,
        crate::reading_routes::ReadingSourceBody,
        crate::reading_routes::MarginaliaConnectionBody,
        crate::reading_routes::MarginaliaSendBody,
        crate::assistant_routes::AssistantSendBody,
        crate::assistant_routes::AssistantDecideBody,
        crate::assistant_routes::AssistantDeleteBody,
        crate::assistant_routes::AssistantAutoApproveBody,
        crate::assistant_routes::AssistantModelBody,
        crate::reading_routes::MarginaliaSyncBody,
        crate::updates_routes::UpdatesLetGoBody,
        crate::updates_routes::UpdateSourceBody,
        crate::routes_v6::SetAccountIdentityBody,
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
endpoint!(post mail_sync "/api/v1/mail/sync", "Trigger default-account sync");
endpoint!(post mail_sync_all "/api/v1/mail/sync/all", "Trigger global sync");
endpoint!(post mail_sync_preview "/api/v1/mail/sync/preview", "Preview accounts selected for global sync");

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
#[utoipa::path(
    post, path = "/api/v1/mail/actions/unsubscribe-preview/commit",
    summary = "Commit previewed unsubscribe and optionally clear mail",
    request_body = crate::request_types::CommitUnsubscribePreviewRequest,
    responses((status = 200, description = "Unsubscribe result"),
        (status = 401, description = "Missing or invalid bridge token"),
        (status = 422, description = "Invalid or unsupported preview commit"))
)]
#[allow(dead_code)]
fn action_commit_unsubscribe_preview() {}
endpoint!(post action_invite_reply "/api/v1/mail/actions/invite/reply", "Reply to a calendar invite");
endpoint!(post attachment_open "/api/v1/mail/attachments/open", "Open an attachment");
endpoint!(post attachment_download "/api/v1/mail/attachments/download", "Download an attachment");
#[utoipa::path(
    post, path = "/api/v1/mail/attachments/transfer/metadata",
    summary = "Get attachment transfer metadata",
    request_body = crate::request_types::AttachmentRequest,
    responses((status = 200, description = "Attachment identifiers, size, checksum, and maximum chunk size for the transfer", body = mxr_protocol::AttachmentTransferMetadata),
        (status = 401, description = "Missing or invalid bridge token"))
)]
#[allow(dead_code)]
fn attachment_transfer_metadata() {}
#[utoipa::path(
    post, path = "/api/v1/mail/attachments/transfer/chunk",
    summary = "Read a bounded attachment transfer chunk",
    request_body = crate::request_types::AttachmentChunkRequest,
    responses((status = 200, description = "Base64-encoded attachment bytes at the requested offset", body = mxr_protocol::AttachmentChunk),
        (status = 401, description = "Missing or invalid bridge token"))
)]
#[allow(dead_code)]
fn attachment_transfer_chunk() {}
endpoint!(post label_create "/api/v1/mail/labels/create", "Create a label");
endpoint!(post label_rename "/api/v1/mail/labels/rename", "Rename a label");
endpoint!(post label_delete "/api/v1/mail/labels/delete", "Delete a label");
#[derive(utoipa::ToSchema)]
#[allow(dead_code)]
struct CategoriesResponse {
    categories: Vec<mxr_core::PersonalCategory>,
}

#[derive(utoipa::ToSchema)]
#[allow(dead_code)]
struct CategoryMessagesResponse {
    envelopes: Vec<mxr_core::Envelope>,
    total: u32,
    has_more: bool,
}

#[derive(utoipa::ToSchema)]
#[allow(dead_code)]
struct CategoryMembershipStatesResponse {
    states: Vec<mxr_core::CategoryMembershipState>,
}

#[derive(utoipa::ToSchema)]
#[allow(dead_code)]
struct CategoryNamePreviewResponse {
    normalized_name: String,
    available: bool,
}

#[derive(utoipa::ToSchema)]
#[allow(dead_code)]
struct CategoryMembershipAppliedResponse {
    mutation_id: String,
    added: u32,
    removed: u32,
}

#[derive(Debug, serde::Serialize, serde::Deserialize, utoipa::ToSchema)]
#[serde(untagged)]
#[allow(dead_code)]
enum CategoryDeleteResponse {
    Preview(mxr_protocol::CategoryDeletePreview),
    Deleted(CategoryDeletedResponse),
}

#[derive(Debug, serde::Serialize, serde::Deserialize, utoipa::ToSchema)]
#[allow(dead_code)]
struct CategoryDeletedResponse {
    category_id: mxr_core::CategoryId,
    name: String,
}

#[derive(utoipa::ToSchema)]
#[allow(dead_code)]
struct CategoryMembershipUndoneResponse {
    restored: u32,
    skipped: u32,
}

#[utoipa::path(
    get,
    path = "/api/v1/mail/categories",
    summary = "List personal categories",
    params(("account_id" = Option<String>, Query, description = "Filter membership counts to this account")),
    responses(
        (status = 200, description = "Personal categories and account-scoped counts", body = CategoriesResponse),
        (status = 400, description = "Malformed account id"),
        (status = 401, description = "Missing or invalid bridge token")
    )
)]
#[allow(dead_code)]
fn categories_list() {}

#[utoipa::path(
    get,
    path = "/api/v1/mail/categories/messages",
    summary = "List messages in a personal category",
    params(
        ("category_id" = String, Query, description = "Stable category id"),
        ("account_id" = Option<String>, Query, description = "Restrict messages to this account"),
        ("limit" = Option<u32>, Query, description = "Page size; defaults to 50"),
        ("offset" = Option<u32>, Query, description = "Zero-based page offset")
    ),
    responses(
        (status = 200, description = "Page of category messages", body = CategoryMessagesResponse),
        (status = 400, description = "Missing or invalid category id"),
        (status = 404, description = "Category was not found"),
        (status = 401, description = "Missing or invalid bridge token")
    )
)]
#[allow(dead_code)]
fn categories_messages() {}

#[utoipa::path(
    get,
    path = "/api/v1/mail/compose/models",
    summary = "List models available to compose under the compose route and privacy policy",
    responses(
        (status = 200, description = "Compose model choices, locality and availability", body = crate::compose_assist_routes::ComposeModelsResponse),
        (status = 401, description = "Missing or invalid bridge token")
    )
)]
#[allow(dead_code)]
fn mail_compose_models() {}
#[utoipa::path(
    post,
    path = "/api/v1/mail/categories/memberships",
    summary = "Read category assignments for selected messages",
    request_body = crate::CategoryMembershipStateBody,
    responses(
        (status = 200, description = "Membership counts for each selected category", body = CategoryMembershipStatesResponse),
        (status = 400, description = "Malformed account, message, or category id"),
        (status = 422, description = "The daemon rejected this account-scoped lookup"),
        (status = 401, description = "Missing or invalid bridge token")
    )
)]
#[allow(dead_code)]
fn categories_memberships() {}

#[utoipa::path(
    post,
    path = "/api/v1/mail/categories",
    summary = "Create a personal category",
    request_body = crate::CategoryNameBody,
    responses(
        (status = 200, description = "Updated personal category catalog", body = CategoriesResponse),
        (status = 422, description = "The daemon rejected an invalid or duplicate category name"),
        (status = 401, description = "Missing or invalid bridge token")
    )
)]
#[allow(dead_code)]
fn categories_create() {}

#[utoipa::path(
    post,
    path = "/api/v1/mail/categories/name/preview",
    summary = "Validate a personal category name without changing the catalog",
    request_body = crate::CategoryNamePreviewBody,
    responses(
        (status = 200, description = "Normalized name and whether it is available", body = CategoryNamePreviewResponse),
        (status = 400, description = "Malformed category id"),
        (status = 422, description = "The daemon rejected an invalid category name"),
        (status = 401, description = "Missing or invalid bridge token")
    )
)]
#[allow(dead_code)]
fn categories_name_preview() {}

#[utoipa::path(
    post,
    path = "/api/v1/mail/categories/rename",
    summary = "Rename a personal category while preserving its id and memberships",
    request_body = crate::RenameCategoryBody,
    responses(
        (status = 200, description = "Updated personal category catalog", body = CategoriesResponse),
        (status = 400, description = "Malformed category id"),
        (status = 422, description = "The daemon rejected an invalid or duplicate category name"),
        (status = 401, description = "Missing or invalid bridge token")
    )
)]
#[allow(dead_code)]
fn categories_rename() {}

#[utoipa::path(
    post,
    path = "/api/v1/mail/categories/assignment/preview",
    summary = "Preview category assignments for selected messages",
    request_body = crate::CategoryMembershipBody,
    responses(
        (status = 200, description = "Stale-safe assignment preview and confirmation token", body = mxr_protocol::CategoryMembershipPreview),
        (status = 400, description = "Malformed account, message, or category id"),
        (status = 422, description = "The daemon rejected this account-scoped selection or category"),
        (status = 401, description = "Missing or invalid bridge token")
    )
)]
#[allow(dead_code)]
fn categories_assignment_preview() {}

#[utoipa::path(
    post,
    path = "/api/v1/mail/categories/assignment/apply",
    summary = "Apply previewed category assignments",
    request_body = crate::CategoryTokenBody,
    responses(
        (status = 200, description = "Applied assignment counts and Undo mutation id", body = CategoryMembershipAppliedResponse),
        (status = 422, description = "The daemon rejected an invalid, stale, or expired preview token"),
        (status = 401, description = "Missing or invalid bridge token")
    )
)]
#[allow(dead_code)]
fn categories_assignment_apply() {}

#[utoipa::path(
    post,
    path = "/api/v1/mail/categories/delete",
    summary = "Preview category deletion or apply its stale-safe preview",
    request_body = crate::DeleteCategoryBody,
    responses(
        (status = 200, description = "Deletion preview, or deleted category id and name", body = CategoryDeleteResponse),
        (status = 400, description = "Malformed category id"),
        (status = 404, description = "Category was not found"),
        (status = 422, description = "The daemon rejected a stale, missing, or expired preview token"),
        (status = 401, description = "Missing or invalid bridge token")
    )
)]
#[allow(dead_code)]
fn categories_delete() {}

#[utoipa::path(
    post,
    path = "/api/v1/mail/categories/undo",
    summary = "Undo a category assignment mutation",
    request_body = crate::CategoryUndoBody,
    responses(
        (status = 200, description = "Restored and skipped membership counts", body = CategoryMembershipUndoneResponse),
        (status = 404, description = "Mutation was not found"),
        (status = 422, description = "The daemon rejected an invalid, stale, or expired mutation id"),
        (status = 401, description = "Missing or invalid bridge token")
    )
)]
#[allow(dead_code)]
fn categories_undo() {}
endpoint!(post mail_unsnooze_one "/api/v1/mail/snoozed/{message_id}/wake", "Wake one snoozed message");

endpoint!(get reply_later_list "/api/v1/mail/reply-later", "List reply-later messages");
endpoint!(post reply_later_set "/api/v1/mail/reply-later/{message_id}", "Set or clear reply-later");
endpoint!(post reminders_set "/api/v1/mail/reminders", "Schedule an auto-reminder");
endpoint!(delete reminders_cancel "/api/v1/mail/reminders/{message_id}", "Cancel an auto-reminder");
endpoint!(get scheduled_sends_list "/api/v1/mail/scheduled-sends", "List pending scheduled sends");
#[utoipa::path(post, path = "/api/v1/mail/scheduled-sends", request_body = crate::request_types::ScheduleStoredDraftRequest, responses((status = 200, description = "Scheduled draft", body = ResponseData), (status = 422, description = "Managed attachments require explicit review")))]
#[allow(dead_code)]
fn scheduled_sends_create() {}
endpoint!(delete scheduled_sends_cancel "/api/v1/mail/scheduled-sends/{draft_id}", "Cancel a scheduled send");
endpoint!(get snippets_list "/api/v1/mail/snippets", "List snippets");
endpoint!(post snippets_set "/api/v1/mail/snippets", "Create or update a snippet");
endpoint!(delete snippets_delete "/api/v1/mail/snippets/{name}", "Delete a snippet");
endpoint!(get sender_profile "/api/v1/mail/sender", "Sender profile");
#[utoipa::path(get, path = "/api/v1/mail/contacts/autocomplete", summary = "Prefix-search known senders", responses((status = 200, description = "Known senders matching the query", body = crate::routes_v6::ContactsAutocompleteResponse), (status = 401, description = "Missing or invalid bridge token")))]
#[allow(dead_code)]
fn contacts_autocomplete() {}
endpoint!(get screener_queue "/api/v1/mail/screener/queue", "List screener queue");
endpoint!(get screener_decisions_list "/api/v1/mail/screener/decisions", "List screener decisions");
endpoint!(post screener_decisions_set "/api/v1/mail/screener/decisions", "Set screener decision");
endpoint!(delete screener_decisions_clear "/api/v1/mail/screener/decisions", "Clear screener decision");
endpoint!(post thread_summarize "/api/v1/mail/threads/{thread_id}/summarize", "Summarize a thread");
endpoint!(post mail_draft_compose "/api/v1/mail/drafts/compose", "Generate an LLM draft (new message or reply)");
endpoint!(post mail_draft_refine "/api/v1/mail/drafts/refine", "Refine draft text with LLM");
#[utoipa::path(
    post,
    path = "/api/v1/mail/compose/assist",
    summary = "Writing help for the text in a compose box: write it, finish it, polish it or edit a selection. Returns a proposal and changes nothing",
    request_body = crate::compose_assist_routes::ComposeAssistBody,
    responses(
        (status = 200, description = "A proposal: the whole proposed text, the word diff from yours, the gaps left, and which of your sentences changed", body = crate::compose_assist_routes::ComposeAssistResponse),
        (status = 401, description = "Missing or invalid bridge token"),
        (status = 422, description = "Nothing to finish, a selection off a character boundary, text too long, or a privacy setting that stops the model")
    )
)]
#[allow(dead_code)]
fn mail_compose_assist() {}
#[utoipa::path(
    post,
    path = "/api/v1/mail/compose/apply",
    summary = "Write an accepted proposal into a stored draft's text region; refused if that text changed since the proposal",
    request_body = crate::compose_assist_routes::ComposeApplyBody,
    responses(
        (status = 200, description = "The draft now holds the proposal", body = crate::compose_assist_routes::ComposeApplyResponse),
        (status = 401, description = "Missing or invalid bridge token"),
        (status = 422, description = "The proposal expired, belongs to another draft, or the draft's text changed")
    )
)]
#[allow(dead_code)]
fn mail_compose_apply() {}
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
    post,
    path = "/api/v1/mail/threads/previews",
    summary = "Headline facts for up to 100 threads with no bodies: subject, latest sender, date, snippet, counts, attachments",
    request_body = crate::thread_context_routes::ThreadPreviewsBody,
    responses(
        (
            status = 200,
            description = "The `ThreadPreviews` variant, in request order; unknown ids are left out",
            body = ResponseData
        ),
        (status = 400, description = "No threads, a malformed thread id, or more than 100"),
        (status = 401, description = "Missing or invalid bridge token")
    )
)]
#[allow(dead_code)]
fn mail_thread_previews() {}

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

#[utoipa::path(get, path = "/api/v1/mail/todos/microsoft", summary = "Whether the optional ms-todo CLI is installed on the daemon", responses((status = 200, description = "The `MicrosoftTodoStatus` variant", body = ResponseData)))]
#[allow(dead_code)]
fn mail_microsoft_todo_status() {}

#[utoipa::path(post, path = "/api/v1/mail/todos/{todo_id}/microsoft", summary = "Preview or create a two-way Microsoft To Do task link", params(("todo_id" = String, Path)), request_body = crate::todo_routes::MicrosoftTodoLinkBody, responses((status = 200, description = "The `MicrosoftTodoSend` variant", body = ResponseData), (status = 400, description = "Preview is stale or ms-todo is unavailable")))]
#[allow(dead_code)]
fn mail_microsoft_todo_link() {}

#[utoipa::path(post, path = "/api/v1/mail/todos/microsoft/sync", summary = "Reconcile linked tasks without discarding conflicting edits", request_body = crate::todo_routes::MicrosoftTodoSyncBody, responses((status = 200, description = "The `MicrosoftTodoSync` variant", body = ResponseData)))]
#[allow(dead_code)]
fn mail_microsoft_todo_sync() {}

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
    path = "/api/v1/mail/todos/{todo_id}/resolution",
    summary = "Review, accept, dismiss or undo a later-email resolution offer",
    params(("todo_id" = String, Path, description = "A full id or a unique prefix")),
    request_body = crate::todo_routes::TodoResolutionBody,
    responses(
        (status = 200, description = "The `TodoChange` variant", body = ResponseData),
        (status = 400, description = "The offer is stale or the requested action is unavailable"),
        (status = 401, description = "Missing or invalid bridge token")
    )
)]
#[allow(dead_code)]
fn mail_todo_resolve() {}

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
    summary = "Move one email to a mode (X), or with sender set the sender's mode (P); To do and Archive add it there (dry_run previews)",
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
    path = "/api/v1/mail/codes",
    summary = "One-time codes from the last day's mail, newest first, each current or expired",
    params(
        ("account" = Option<String>, Query, description = "Account id; omitted covers every account"),
        ("limit" = Option<u32>, Query, description = "Most codes returned; default 10, at most 50")
    ),
    responses(
        (status = 200, description = "The `OneTimeCodes` variant", body = ResponseData),
        (status = 401, description = "Missing or invalid bridge token")
    )
)]
#[allow(dead_code)]
fn mail_codes() {}

#[utoipa::path(
    get, path = "/api/v1/mail/classify/report",
    summary = "Stored fallback clusters and pending assessment counts; local and read-only",
    params(("account" = Option<String>, Query, description = "Account id; omitted covers every account"),
        ("limit" = Option<u32>, Query, description = "Emails examined; default 1000, maximum 10000")),
    responses((status = 200, description = "The ClassifyReport variant", body = ResponseData),
        (status = 401, description = "Missing or invalid bridge token"))
)]
#[allow(dead_code)]
fn mail_classify_report() {}

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
    summary = "Messages: conversations needing attention by default; explicit All conversations includes history",
    params(
        ("account" = Option<String>, Query, description = "Account id; omitted covers every account"),
        ("turn" = Option<String>, Query, description = "Omit for Needs attention; `all` includes conversation history; `mine` or `theirs` selects a side's turn"),
        ("unread" = Option<bool>, Query, description = "`true`: only rows with unread mail from them. Counts in the response are unchanged"),
        ("pace" = Option<bool>, Query, description = "`true`: only rows where your turn has run past your usual pace. Counts in the response are unchanged"),
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
        ("conversation" = Option<bool>, Query, description = "`false` returns the profile only, with no conversation or message text; default true"),
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
    path = "/api/v1/mail/reading/highlights/{id}/note",
    summary = "Change a highlight's note; a blank note clears it. Highlights made in Marginalia are refused. The run returns a mutation id for undo",
    params(("id" = String, Path, description = "The highlight's id")),
    request_body = crate::reading_routes::ReadingHighlightNoteBody,
    responses(
        (status = 200, description = "The `ReadingHighlightChanged` variant", body = ResponseData),
        (status = 401, description = "Missing or invalid bridge token")
    )
)]
#[allow(dead_code)]
fn mail_reading_highlight_note() {}

#[utoipa::path(
    post,
    path = "/api/v1/mail/reading/highlights/{id}/remove",
    summary = "Remove a highlight, or with `dry_run` name what it would remove. Highlights made in Marginalia are refused. The run returns a mutation id for undo",
    params(("id" = String, Path, description = "The highlight's id")),
    request_body(content = crate::reading_routes::ReadingHighlightRemoveBody, description = "Optional; omitted removes"),
    responses(
        (status = 200, description = "The `ReadingHighlightChanged` variant", body = ResponseData),
        (status = 401, description = "Missing or invalid bridge token")
    )
)]
#[allow(dead_code)]
fn mail_reading_highlight_remove() {}

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
    path = "/api/v1/mail/reading/marginalia",
    summary = "Whether Marginalia is connected, where, and how many items were sent",
    responses(
        (status = 200, description = "The `MarginaliaStatus` variant", body = ResponseData),
        (status = 401, description = "Missing or invalid bridge token")
    )
)]
#[allow(dead_code)]
fn mail_reading_marginalia_get() {}

#[utoipa::path(
    post,
    path = "/api/v1/mail/reading/marginalia",
    summary = "Connect Marginalia with its site and a personal token (checked first), or disconnect with null",
    request_body = crate::reading_routes::MarginaliaConnectionBody,
    responses(
        (status = 200, description = "The `MarginaliaStatus` variant", body = ResponseData),
        (status = 401, description = "Missing or invalid bridge token")
    )
)]
#[allow(dead_code)]
fn mail_reading_marginalia_post() {}

#[utoipa::path(
    post,
    path = "/api/v1/mail/reading/items/{item_key}/marginalia",
    summary = "Send a Reading item to Marginalia, or find it there if sent before; dry_run contacts nothing",
    params(("item_key" = String, Path, description = "`<message id>:<index>`")),
    request_body = crate::reading_routes::MarginaliaSendBody,
    responses(
        (status = 200, description = "The `MarginaliaSend` variant", body = ResponseData),
        (status = 401, description = "Missing or invalid bridge token")
    )
)]
#[allow(dead_code)]
fn mail_reading_marginalia_send() {}

#[utoipa::path(
    post,
    path = "/api/v1/mail/reading/marginalia/highlights",
    summary = "Read back highlights made in Marginalia on one sent item, or every sent item",
    request_body = crate::reading_routes::MarginaliaSyncBody,
    responses(
        (status = 200, description = "The `MarginaliaSync` variant", body = ResponseData),
        (status = 401, description = "Missing or invalid bridge token")
    )
)]
#[allow(dead_code)]
fn mail_reading_marginalia_sync() {}

#[utoipa::path(
    get,
    path = "/api/v1/mail/assistant/availability",
    summary = "Whether the assistant can run, on which model, and whether conversations leave this machine",
    responses(
        (status = 200, description = "The `AssistantAvailability` variant", body = ResponseData),
        (status = 401, description = "Missing or invalid bridge token")
    )
)]
#[allow(dead_code)]
fn mail_assistant_availability() {}

#[utoipa::path(
    get,
    path = "/api/v1/mail/assistant/models",
    summary = "The models the assistant can be set to, which can be picked now and why not, and the one in use",
    responses(
        (status = 200, description = "The `AssistantModels` variant", body = ResponseData),
        (status = 401, description = "Missing or invalid bridge token")
    )
)]
#[allow(dead_code)]
fn mail_assistant_models() {}

#[utoipa::path(
    post,
    path = "/api/v1/mail/assistant/model",
    summary = "Set the model the assistant uses from its next message, for every conversation; refused for a model outside the list, without its key, or off this machine while the cloud setting is off",
    request_body = crate::assistant_routes::AssistantModelBody,
    responses(
        (status = 200, description = "The `AssistantModelChange` variant", body = ResponseData),
        (status = 401, description = "Missing or invalid bridge token")
    )
)]
#[allow(dead_code)]
fn mail_assistant_set_model() {}

#[utoipa::path(
    get,
    path = "/api/v1/mail/assistant/conversations",
    summary = "Assistant conversations, most recently active first",
    params(("limit" = Option<u32>, Query, description = "At most this many (default 50)")),
    responses(
        (status = 200, description = "The `AssistantConversations` variant", body = ResponseData),
        (status = 401, description = "Missing or invalid bridge token")
    )
)]
#[allow(dead_code)]
fn mail_assistant_list() {}

#[utoipa::path(
    post,
    path = "/api/v1/mail/assistant/conversations",
    summary = "Send a message to the assistant; returns once the turn starts. Follow AssistantUpdated events",
    request_body = crate::assistant_routes::AssistantSendBody,
    responses(
        (status = 200, description = "The `AssistantConversation` variant", body = ResponseData),
        (status = 401, description = "Missing or invalid bridge token")
    )
)]
#[allow(dead_code)]
fn mail_assistant_send() {}

#[utoipa::path(
    get,
    path = "/api/v1/mail/assistant/conversations/{id}",
    summary = "One assistant conversation, with what waits for the user",
    params(("id" = String, Path, description = "Conversation id")),
    responses(
        (status = 200, description = "The `AssistantConversation` variant", body = ResponseData),
        (status = 401, description = "Missing or invalid bridge token")
    )
)]
#[allow(dead_code)]
fn mail_assistant_get() {}

#[utoipa::path(
    get,
    path = "/api/v1/mail/assistant/conversations/{id}/steps/{call_id}/rows",
    summary = "Every row one assistant step's stored result lists, up to 500",
    params(
        ("id" = String, Path, description = "Conversation id"),
        ("call_id" = String, Path, description = "The step's tool call id"),
        ("seq" = i64, Query, description = "The seq of the assistant message that declared the call")
    ),
    responses(
        (status = 200, description = "The `AssistantStepRows` variant", body = ResponseData),
        (status = 401, description = "Missing or invalid bridge token")
    )
)]
#[allow(dead_code)]
fn mail_assistant_step_rows() {}

#[utoipa::path(
    post,
    path = "/api/v1/mail/assistant/conversations/{id}/decide",
    summary = "Approve, reject or answer calls the assistant is waiting on; approving runs exactly the call shown",
    params(("id" = String, Path, description = "Conversation id")),
    request_body = crate::assistant_routes::AssistantDecideBody,
    responses(
        (status = 200, description = "The `AssistantConversation` variant", body = ResponseData),
        (status = 401, description = "Missing or invalid bridge token")
    )
)]
#[allow(dead_code)]
fn mail_assistant_decide() {}

#[utoipa::path(
    post,
    path = "/api/v1/mail/assistant/conversations/{id}/cancel",
    summary = "Stop a running turn, or close what is waiting without running it",
    params(("id" = String, Path, description = "Conversation id")),
    responses(
        (status = 200, description = "The `AssistantConversation` variant", body = ResponseData),
        (status = 401, description = "Missing or invalid bridge token")
    )
)]
#[allow(dead_code)]
fn mail_assistant_cancel() {}

#[utoipa::path(
    post,
    path = "/api/v1/mail/assistant/conversations/{id}/delete",
    summary = "Delete a conversation and its transcript; dry_run says what would go",
    params(("id" = String, Path, description = "Conversation id")),
    request_body = crate::assistant_routes::AssistantDeleteBody,
    responses(
        (status = 200, description = "The `AssistantDelete` variant", body = ResponseData),
        (status = 401, description = "Missing or invalid bridge token")
    )
)]
#[allow(dead_code)]
fn mail_assistant_delete() {}

#[utoipa::path(
    post,
    path = "/api/v1/mail/assistant/conversations/{id}/auto-approve",
    summary = "Let reversible changes run without asking in this conversation; sending, deleting, trash, spam and rules still ask",
    params(("id" = String, Path, description = "Conversation id")),
    request_body = crate::assistant_routes::AssistantAutoApproveBody,
    responses(
        (status = 200, description = "The `AssistantConversation` variant", body = ResponseData),
        (status = 401, description = "Missing or invalid bridge token")
    )
)]
#[allow(dead_code)]
fn mail_assistant_auto_approve() {}

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

#[utoipa::path(post, path = "/api/v1/mail/compose/session", request_body = ComposeSessionStartRequest, responses((status = 200, description = "Compose session result", body = ComposeSessionResponse), (status = 409, description = "Stale draft revision; preserve unsent text", body = DraftConflictResponse), (status = 422, description = "Invalid compose session", body = DraftConflictResponse)))]
#[allow(dead_code)]
fn compose_session_start() {}
#[utoipa::path(post, path = "/api/v1/mail/compose/session/audience", request_body = ComposeSessionAudienceRequest, responses((status = 200, description = "Reply audience preview or updated compose session", body = ComposeSessionResponse), (status = 400, description = "Invalid draft identity or unsupported compose content", body = DraftConflictResponse), (status = 401, description = "Missing or invalid bridge token"), (status = 409, description = "Stale draft revision; preserve unsent text", body = DraftConflictResponse), (status = 422, description = "Invalid compose session", body = DraftConflictResponse)))]
#[allow(dead_code)]
fn compose_session_audience() {}
#[utoipa::path(post, path = "/api/v1/mail/compose/session/refresh", request_body = ComposeSessionPathRequest, responses((status = 200, description = "Compose session result", body = ComposeSessionResponse), (status = 409, description = "Stale draft revision; preserve unsent text", body = DraftConflictResponse), (status = 422, description = "Invalid compose session", body = DraftConflictResponse)))]
#[allow(dead_code)]
fn compose_session_refresh() {}
#[utoipa::path(
    post,
    path = "/api/v1/mail/compose/session/restore",
    summary = "Restore compose session",
    request_body = ComposeSessionRestoreRequest,
    responses(
        (status = 200, description = "OK", body = ComposeSessionResponse),
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
#[utoipa::path(post, path = "/api/v1/mail/compose/session/update", request_body = ComposeSessionUpdateRequest, responses((status = 200, description = "Saved draft session", body = ComposeSessionResponse), (status = 409, description = "Stale draft revision; preserve unsent text", body = DraftConflictResponse), (status = 422, description = "Invalid draft or unsupported daemon", body = DraftConflictResponse), (status = 500, description = "Daemon operation failed; may carry an accepted draft revision", body = DraftConflictResponse), (status = 502, description = "Bridge IPC failed; may carry an accepted draft revision", body = DraftConflictResponse), (status = 503, description = "Daemon connection failed; may carry an accepted draft revision", body = DraftConflictResponse), (status = 504, description = "Daemon request timed out; may carry an accepted draft revision", body = DraftConflictResponse), (status = 401, description = "Missing bridge token")))]
#[allow(dead_code)]
fn compose_session_update() {}
#[utoipa::path(post, path = "/api/v1/mail/compose/session/send", summary = "Send compose session", request_body = ComposeSessionSendRequest, responses((status = 400, description = "Local recovery requires explicit canonical reopen", body = DraftConflictResponse), (status = 200, description = "Sent compose session"), (status = 409, description = "Stale draft revision; preserve unsent text", body = DraftConflictResponse), (status = 401, description = "Missing bridge token")))]
#[allow(dead_code)]
fn compose_session_send() {}
#[utoipa::path(post, path = "/api/v1/mail/compose/session/safety-check", summary = "Run the pre-send safety report for a compose session", request_body = ComposeSessionSendRequest, responses((status = 400, description = "Local recovery requires explicit canonical reopen", body = DraftConflictResponse), (status = 200, description = "Compose safety report"), (status = 409, description = "Stale draft revision; preserve unsent text", body = DraftConflictResponse), (status = 401, description = "Missing bridge token")))]
#[allow(dead_code)]
fn compose_session_safety_check() {}
endpoint!(post compose_session_collaborators "/api/v1/mail/compose/session/collaborators", "Suggest maybe-include recipients for a compose session");
#[utoipa::path(post, path = "/api/v1/mail/compose/session/save", request_body = ComposeSessionSendRequest, responses((status = 400, description = "Local recovery requires explicit canonical reopen", body = DraftConflictResponse), (status = 200, description = "Saved draft session", body = SavedDraftResponse), (status = 409, description = "Stale draft revision; preserve unsent text", body = DraftConflictResponse), (status = 422, description = "Invalid draft or unsupported daemon", body = DraftConflictResponse), (status = 500, description = "Daemon operation failed; may carry an accepted draft revision", body = DraftConflictResponse), (status = 502, description = "Bridge IPC failed; may carry an accepted draft revision", body = DraftConflictResponse), (status = 503, description = "Daemon connection failed; may carry an accepted draft revision", body = DraftConflictResponse), (status = 504, description = "Daemon request timed out; may carry an accepted draft revision", body = DraftConflictResponse), (status = 401, description = "Missing bridge token")))]
#[allow(dead_code)]
fn compose_session_save() {}
#[utoipa::path(post, path = "/api/v1/mail/compose/session/schedule", request_body = ComposeSessionScheduleRequest, responses((status = 400, description = "Local recovery requires explicit canonical reopen", body = DraftConflictResponse), (status = 200, description = "Saved draft session", body = ScheduledComposeResponse), (status = 409, description = "Stale draft revision; preserve unsent text", body = DraftConflictResponse), (status = 422, description = "Invalid draft or unsupported daemon", body = DraftConflictResponse), (status = 500, description = "Daemon operation failed; may carry an accepted draft revision", body = DraftConflictResponse), (status = 502, description = "Bridge IPC failed; may carry an accepted draft revision", body = DraftConflictResponse), (status = 503, description = "Daemon connection failed; may carry an accepted draft revision", body = DraftConflictResponse), (status = 504, description = "Daemon request timed out; may carry an accepted draft revision", body = DraftConflictResponse), (status = 401, description = "Missing bridge token")))]
#[allow(dead_code)]
fn compose_session_schedule() {}
#[utoipa::path(post, path = "/api/v1/mail/compose/session/attachment", request_body = crate::request_types::ComposeSessionAttachmentRequest, responses((status = 200, description = "Bounded upload declaration, chunk acknowledgement, or finalized owner revision", body = crate::request_types::ComposeAttachmentResponse), (status = 409, description = "Stale owner revision"), (status = 422, description = "Invalid declaration, chunk, integrity or unsupported daemon")))]
#[allow(dead_code)]
fn compose_session_attachment() {}
#[utoipa::path(post, path = "/api/v1/mail/compose/session/discard", request_body = ComposeSessionPathRequest, responses((status = 200, description = "Compose session result", body = DiscardedComposeResponse), (status = 409, description = "Stale draft revision; preserve unsent text", body = DraftConflictResponse), (status = 422, description = "Invalid compose session", body = DraftConflictResponse)))]
#[allow(dead_code)]
fn compose_session_discard() {}

endpoint!(get rules_list "/api/v1/platform/rules", "List rules");
endpoint!(get rule_detail "/api/v1/platform/rules/detail", "Rule detail");
endpoint!(get rule_form "/api/v1/platform/rules/form", "Rule form payload");
endpoint!(get rule_history "/api/v1/platform/rules/history", "Rule history");
endpoint!(get rule_dry_run "/api/v1/platform/rules/dry-run", "Dry-run rules");
endpoint!(post rule_upsert "/api/v1/platform/rules/upsert", "Create or update rule");
endpoint!(post rule_upsert_form "/api/v1/platform/rules/upsert-form", "Create or update rule from form");
endpoint!(post rule_treatment "/api/v1/platform/rules/treatment", "Preview or apply account-scoped sorting");
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
#[utoipa::path(
    post,
    path = "/api/v1/platform/accounts/{key}/identity",
    summary = "Set an account's colour and tag",
    params(("key" = String, Path, description = "The account's config key, such as `work`")),
    request_body = crate::routes_v6::SetAccountIdentityBody,
    responses(
        (status = 200, description = "The `AccountOperation` variant. When the daemon refuses a tag (malformed, or held by another account), `result.ok` is false and `result.save.detail` says why; nothing is saved", body = ResponseData),
        (status = 422, description = "The body doesn't match the shape: a colour outside the eight slots, or a field of the wrong type"),
        (status = 401, description = "Missing or invalid bridge token")
    )
)]
#[allow(dead_code)]
fn account_identity() {}
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
#[utoipa::path(post, path = "/api/v1/mail/drafts/save-local", request_body = mxr_core::Draft, responses((status = 200, description = "Canonical saved draft", body = ResponseData), (status = 409, description = "Stale revision", body = DraftConflictResponse), (status = 422, description = "Invalid draft or legacy daemon", body = DraftConflictResponse)))]
#[allow(dead_code)]
fn mail_drafts_save_local() {}
endpoint!(post mail_drafts_reset_orphan "/api/v1/mail/drafts/{draft_id}/reset-orphan", "Reset orphaned sending draft");
#[utoipa::path(get, path = "/api/v1/mail/send-outcomes/{operation_id}", params(("operation_id" = String, Path), ("account_id" = String, Query, description = "Owning account ID")), responses((status = 200, description = "Durable send outcome", body = ResponseData), (status = 401, description = "Missing bridge token")))]
#[allow(dead_code)]
fn mail_send_outcome() {}

#[utoipa::path(post, path = "/api/v1/mail/send-outcomes/{operation_id}/recover", params(("operation_id" = String, Path), ("account_id" = String, Query)), request_body = crate::routes_v6::RecoverSendBody, responses((status = 200, description = "Evidence-bound recovery preview or resumed original operation", body = ResponseData)))]
#[allow(dead_code)]
fn mail_recover_send_outcome() {}
#[utoipa::path(post, path = "/api/v1/mail/drafts/{draft_id}/send-stored", params(("draft_id" = String, Path), ("operation_id" = String, Query, description = "Retained reviewed send operation"), ("expected_revision" = i64, Query, description = "Reviewed draft revision"), ("managed_attachments_reviewed" = Option<bool>, Query, description = "True only after reviewing current managed attachment metadata; defaults false")), responses((status = 200, description = "Send receipt or outcome", body = ResponseData), (status = 401, description = "Missing bridge token")))]
#[allow(dead_code)]
fn mail_drafts_send_stored() {}
#[utoipa::path(delete, path = "/api/v1/mail/drafts/{draft_id}/stored", params(("draft_id" = String, Path), ("expected_revision" = i64, Query, description = "Revision from the deletion preview")), responses((status = 200, description = "Draft deleted", body = ResponseData), (status = 409, description = "Stale revision", body = DraftConflictResponse), (status = 422, description = "Legacy daemon", body = DraftConflictResponse)))]
#[allow(dead_code)]
fn mail_drafts_delete_stored() {}
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

#[derive(serde::Serialize, utoipa::ToSchema)]
struct ArchiveErrorResponse {
    error: String,
    code: String,
    details: Option<serde_json::Value>,
}
#[utoipa::path(get, path = "/api/v1/mail/archive-operations/workspace", responses((status = 200, description = "Selected workspace and recovery generation", body = ResponseData), (status = 401, description = "Bridge authorization required", body = ArchiveErrorResponse), (status = 422, description = "Unsupported daemon", body = ArchiveErrorResponse), (status = 503, description = "Selected daemon unavailable", body = ArchiveErrorResponse), (status = 400, description = "Malformed Archive request", body = ArchiveErrorResponse), (status = 500, description = "Receipt persistence unavailable", body = ArchiveErrorResponse), (status = 502, description = "IPC response unavailable", body = ArchiveErrorResponse), (status = 504, description = "IPC response timed out", body = ArchiveErrorResponse)))]
#[allow(dead_code)]
fn archive_workspace() {}
#[utoipa::path(get, path = "/api/v1/mail/recovery/status", responses((status = 200, description = "Persistent restored-profile pause, held drafts and retired intentions", body = ResponseData), (status = 401, description = "Bridge authorization required", body = ArchiveErrorResponse), (status = 422, description = "Daemon does not support protocol 12 recovery", body = ArchiveErrorResponse)))]
#[allow(dead_code)]
fn recovery_status() {}
#[utoipa::path(post, path = "/api/v1/mail/recovery/activate", request_body = crate::routes_v6::RecoveryActivateBody, responses((status = 200, description = "Activation preview or result; old intentions are never replayed", body = ResponseData), (status = 409, description = "Workspace or generation changed", body = ArchiveErrorResponse), (status = 401, description = "Bridge authorization required", body = ArchiveErrorResponse)))]
#[allow(dead_code)]
fn recovery_activate() {}
#[utoipa::path(post, path = "/api/v1/mail/recovery/prepare-draft", request_body = crate::routes_v6::RecoveryPrepareBody, responses((status = 200, description = "Preview or creation of a new draft while the original remains held", body = ResponseData), (status = 409, description = "Workspace, generation, or draft revision changed", body = ArchiveErrorResponse), (status = 401, description = "Bridge authorization required", body = ArchiveErrorResponse)))]
#[allow(dead_code)]
fn recovery_prepare() {}
#[utoipa::path(post, path = "/api/v1/mail/archive-operations/preview", request_body = crate::routes_v6::ArchivePreviewBody, responses((status = 200, description = "Immutable Archive review", body = ResponseData), (status = 409, description = "Identity, workspace or selection conflict", body = ArchiveErrorResponse), (status = 401, description = "Bridge authorization required", body = ArchiveErrorResponse), (status = 422, description = "Unsupported daemon or policy", body = ArchiveErrorResponse), (status = 500, description = "Review persistence failed", body = ArchiveErrorResponse), (status = 503, description = "Selected daemon unavailable", body = ArchiveErrorResponse), (status = 400, description = "Malformed Archive request", body = ArchiveErrorResponse), (status = 502, description = "IPC response unavailable", body = ArchiveErrorResponse), (status = 504, description = "IPC response timed out", body = ArchiveErrorResponse)))]
#[allow(dead_code)]
fn archive_preview() {}
#[utoipa::path(post, path = "/api/v1/mail/archive-operations/submit", request_body = crate::routes_v6::ArchiveSubmitBody, responses((status = 200, description = "Durable acceptance or retained per-item outcome", body = ResponseData), (status = 409, description = "Identity, workspace or review conflict", body = ArchiveErrorResponse), (status = 401, description = "Bridge authorization required", body = ArchiveErrorResponse), (status = 422, description = "Unsupported daemon or policy", body = ArchiveErrorResponse), (status = 500, description = "Acceptance persistence failed", body = ArchiveErrorResponse), (status = 502, description = "Reply lost; query retained identity", body = ArchiveErrorResponse), (status = 503, description = "Selected daemon unavailable", body = ArchiveErrorResponse), (status = 504, description = "Reply timed out; query retained identity", body = ArchiveErrorResponse), (status = 400, description = "Malformed Archive request", body = ArchiveErrorResponse)))]
#[allow(dead_code)]
fn archive_submit() {}
#[utoipa::path(post, path = "/api/v1/mail/archive-operations/query", request_body = crate::routes_v6::ArchiveQueryBody, responses((status = 200, description = "Side-effect-free retained receipt", body = ResponseData), (status = 409, description = "Identity or workspace conflict", body = ArchiveErrorResponse), (status = 401, description = "Bridge authorization required", body = ArchiveErrorResponse), (status = 422, description = "Unsupported daemon or policy", body = ArchiveErrorResponse), (status = 503, description = "Selected daemon unavailable", body = ArchiveErrorResponse), (status = 400, description = "Malformed Archive request", body = ArchiveErrorResponse), (status = 500, description = "Receipt persistence unavailable", body = ArchiveErrorResponse), (status = 502, description = "IPC response unavailable", body = ArchiveErrorResponse), (status = 504, description = "IPC response timed out", body = ArchiveErrorResponse)))]
#[allow(dead_code)]
fn archive_query() {}

endpoint!(post rule_categories "/api/v1/platform/rules/categories", "Preview or apply complete category-rule reconciliation");
endpoint!(get category_why "/api/v1/mail/categories/why", "Explain category membership and rule provenance");

#[utoipa::path(post, path = "/api/v1/mail/todos/{todo_id}/correct", summary = "Correct ownership, task status or details with safe undo", params(("todo_id" = String, Path)), request_body = crate::todo_routes::TodoCorrectionBody, responses((status = 200, description = "The `TodoChange` variant", body = ResponseData)))]
#[allow(dead_code)]
fn mail_todo_correct() {}
#[utoipa::path(post, path = "/api/v1/mail/todos/{todo_id}/correction/undo", summary = "Undo a correction unless the row has changed", params(("todo_id" = String, Path)), request_body = crate::todo_routes::TodoCorrectionUndoBody, responses((status = 200, description = "The `TodoChange` variant", body = ResponseData)))]
#[allow(dead_code)]
fn mail_todo_undo_correction() {}
