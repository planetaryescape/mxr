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
                       All routes (except /api/v1/health) require a bearer \
                       token from ~/.config/mxr/bridge-token.",
        license(name = "MIT OR Apache-2.0"),
        contact(name = "mxr", url = "https://mxr.sh")
    ),
    paths(
        health, openapi_json, swagger_ui, events_ws, desktop_shell,
        admin_status, admin_diagnostics, admin_bug_report, admin_events,
        admin_logs, admin_ping, admin_shutdown,
        mail_mailbox, mail_search, mail_thread, mail_thread_export,
        mail_drafts, mail_snoozed, mail_count, mail_sync_status, mail_sync,
        mutation_archive, mutation_trash, mutation_spam, mutation_star,
        mutation_read, mutation_read_archive, mutation_labels, mutation_move,
        mutation_undo, action_snooze_presets, action_snooze,
        action_unsubscribe, attachment_open, attachment_download,
        label_create, label_rename, label_delete, mail_unsnooze_one,
        reply_later_list, reply_later_set, reminders_set, reminders_cancel,
        scheduled_sends_create, scheduled_sends_cancel, snippets_list,
        snippets_set, snippets_delete, sender_profile, screener_queue,
        screener_decisions_list, screener_decisions_set,
        screener_decisions_clear, thread_summarize, draft_assist, mail_draft_new,
        mail_draft_refine, mail_humanizer_score, mail_humanizer_rewrite,
        mail_relationship_profile, mail_relationship_rebuild, mail_commitments_list,
        mail_commitments_resolve, compose_session_start, compose_session_refresh, compose_session_restore,
        compose_session_update, compose_session_send, compose_session_save,
        compose_session_attachment, compose_session_discard, rules_list, rule_detail, rule_form,
        rule_history, rule_dry_run, rule_upsert, rule_upsert_form, rule_delete,
        saved_searches_list, saved_searches_create, saved_searches_delete,
        saved_searches_run, accounts_list, accounts_config, account_test,
        account_upsert, account_set_default, account_remove, account_disable,
        account_addresses_list, account_addresses_add, account_addresses_remove,
        account_addresses_primary, auth_session_start, auth_session_get,
        auth_session_cancel, auth_session_complete, subscriptions_list,
        llm_status, llm_config_get, llm_config_update, semantic_status, semantic_reindex, semantic_enable,
        semantic_profile_install, semantic_profile_use, semantic_backfill, analytics_wrapped,
        analytics_storage_breakdown, analytics_largest_messages,
        analytics_stale_threads, analytics_contact_asymmetry,
        analytics_contact_decay, analytics_response_time,
        analytics_refresh_contacts, analytics_rebuild,
        mail_message_body, mail_message_html_images, mail_message_raw_headers,
        mail_message_set_flags, mail_export_search, mail_drafts_orphaned_list,
        mail_drafts_save_local, mail_drafts_reset_orphan, mail_drafts_send_stored,
        mail_drafts_delete_stored, mail_signatures_list, mail_signatures_upsert,
        mail_signature_defaults_list, mail_signature_default_set,
        mail_signature_default_clear, mail_signature_resolve, mail_signatures_delete,
        platform_accounts_authorize, platform_accounts_repair, platform_voice_get,
        platform_voice_rebuild
    ),
    components(schemas(
        Request,
        Response,
        ResponseData,
        DaemonEvent,
        MutationCommand,
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

endpoint!(get health "/api/v1/health", "Unauthenticated bridge liveness probe");
endpoint!(get openapi_json "/api/v1/openapi.json", "OpenAPI 3.1 document");
endpoint!(get swagger_ui "/api/v1/docs", "Swagger UI");
endpoint!(get events_ws "/api/v1/events", "WebSocket daemon event stream");
endpoint!(get desktop_shell "/api/v1/desktop/shell", "Desktop shell manifest");

endpoint!(get admin_status "/api/v1/admin/status", "Daemon status snapshot");
endpoint!(get admin_diagnostics "/api/v1/admin/diagnostics", "Diagnostics report");
endpoint!(get admin_bug_report "/api/v1/admin/diagnostics/bug-report", "Sanitized bug report");
endpoint!(get admin_events "/api/v1/admin/events", "Persisted daemon events");
endpoint!(get admin_logs "/api/v1/admin/logs", "Recent daemon logs");
endpoint!(post admin_ping "/api/v1/admin/ping", "Bridge round-trip ping");
endpoint!(post admin_shutdown "/api/v1/admin/shutdown", "Request daemon shutdown");

endpoint!(get mail_mailbox "/api/v1/mail/mailbox", "Mailbox view");
endpoint!(get mail_search "/api/v1/mail/search", "Run a mail search");
endpoint!(get mail_thread "/api/v1/mail/threads/{thread_id}", "Read a thread");
endpoint!(get mail_thread_export "/api/v1/mail/threads/{thread_id}/export", "Export a thread");
endpoint!(get mail_drafts "/api/v1/mail/drafts", "List drafts");
endpoint!(get mail_snoozed "/api/v1/mail/snoozed", "List snoozed messages");
endpoint!(get mail_count "/api/v1/mail/count", "Count matching messages");
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
endpoint!(post mutation_undo "/api/v1/mail/mutations/undo", "Undo a recent mutation");

endpoint!(get action_snooze_presets "/api/v1/mail/actions/snooze/presets", "List snooze presets");
endpoint!(post action_snooze "/api/v1/mail/actions/snooze", "Snooze messages");
endpoint!(post action_unsubscribe "/api/v1/mail/actions/unsubscribe", "Unsubscribe from list mail");
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
endpoint!(post scheduled_sends_create "/api/v1/mail/scheduled-sends", "Schedule a draft send");
endpoint!(delete scheduled_sends_cancel "/api/v1/mail/scheduled-sends/{draft_id}", "Cancel a scheduled send");
endpoint!(get snippets_list "/api/v1/mail/snippets", "List snippets");
endpoint!(post snippets_set "/api/v1/mail/snippets", "Create or update a snippet");
endpoint!(delete snippets_delete "/api/v1/mail/snippets/{name}", "Delete a snippet");
endpoint!(get sender_profile "/api/v1/mail/sender", "Sender profile");
endpoint!(get screener_queue "/api/v1/mail/screener/queue", "List screener queue");
endpoint!(get screener_decisions_list "/api/v1/mail/screener/decisions", "List screener decisions");
endpoint!(post screener_decisions_set "/api/v1/mail/screener/decisions", "Set screener decision");
endpoint!(delete screener_decisions_clear "/api/v1/mail/screener/decisions", "Clear screener decision");
endpoint!(post thread_summarize "/api/v1/mail/threads/{thread_id}/summarize", "Summarize a thread");
endpoint!(post draft_assist "/api/v1/mail/threads/draft-assist", "Generate a draft body");
endpoint!(post mail_draft_new "/api/v1/mail/drafts/new", "Start a new LLM-backed draft");
endpoint!(post mail_draft_refine "/api/v1/mail/drafts/refine", "Refine draft text with LLM");
endpoint!(post mail_humanizer_score "/api/v1/mail/humanizer/score", "Score draft for human-like voice");
endpoint!(post mail_humanizer_rewrite "/api/v1/mail/humanizer/rewrite", "Rewrite draft toward target voice");
endpoint!(get mail_relationship_profile "/api/v1/mail/relationship", "Relationship profile for a contact");
endpoint!(post mail_relationship_rebuild "/api/v1/mail/relationship/rebuild", "Rebuild relationship analytics");
endpoint!(get mail_commitments_list "/api/v1/mail/commitments", "List detected commitments");
endpoint!(post mail_commitments_resolve "/api/v1/mail/commitments/{commitment_id}/resolve", "Resolve a commitment");

endpoint!(post compose_session_start "/api/v1/mail/compose/session", "Start compose session");
endpoint!(post compose_session_refresh "/api/v1/mail/compose/session/refresh", "Refresh compose session");
endpoint!(post compose_session_restore "/api/v1/mail/compose/session/restore", "Restore compose session");
endpoint!(post compose_session_update "/api/v1/mail/compose/session/update", "Update compose session");
endpoint!(post compose_session_send "/api/v1/mail/compose/session/send", "Send compose session");
endpoint!(post compose_session_save "/api/v1/mail/compose/session/save", "Save compose session");
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
endpoint!(post saved_searches_delete "/api/v1/platform/saved-searches/delete", "Delete saved search");
endpoint!(post saved_searches_run "/api/v1/platform/saved-searches/run", "Run saved search");

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

endpoint!(get mail_message_body "/api/v1/mail/messages/{message_id}/body", "Get message body (IPC GetBody)");
endpoint!(get mail_message_html_images "/api/v1/mail/messages/{message_id}/html-images", "List HTML-linked image assets");
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
endpoint!(post platform_accounts_repair "/api/v1/platform/accounts/repair", "Repair account credentials in keychain");
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
                        "Token from `~/.config/mxr/bridge-token`. Send via \
                         `Authorization: Bearer <token>` header. \
                         WebSocket clients can also pass it via the \
                         `?token=<token>` query string or the \
                         `Sec-WebSocket-Protocol: bearer, <token>` subprotocol.",
                    ))
                    .build(),
            ),
        );
    }
}
