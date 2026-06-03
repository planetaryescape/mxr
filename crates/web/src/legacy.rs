//! 301-redirect shim that translates v0.4.x flat paths to the v0.5
//! bucketed `/api/v1/<bucket>/...` paths. Will be removed in v0.6.
//!
//! Mapping is intentionally exhaustive (rather than rule-based) so a typo
//! in a single redirect can be caught by the slice 3 integration test.

use axum::{
    extract::Request,
    http::{header, StatusCode, Uri},
    response::{IntoResponse, Redirect, Response},
};

/// Returns the v0.5 path for a v0.4.x flat path, or `None` if there is no
/// known mapping (in which case the request falls through to the rest of
/// the router and gets the usual 404).
fn translate(path: &str) -> Option<&'static str> {
    Some(match path {
        // admin
        "/status" => "/api/v1/admin/status",
        "/diagnostics" => "/api/v1/admin/diagnostics",
        "/diagnostics/bug-report" => "/api/v1/admin/diagnostics/bug-report",

        // mail — content
        "/mailbox" => "/api/v1/mail/mailbox",
        "/search" => "/api/v1/mail/search",
        "/drafts" => "/api/v1/mail/drafts",
        "/snoozed" => "/api/v1/mail/snoozed",
        "/sync" => "/api/v1/mail/sync",

        // mail — mutations
        "/mutations/archive" => "/api/v1/mail/mutations/archive",
        "/mutations/trash" => "/api/v1/mail/mutations/trash",
        "/mutations/spam" => "/api/v1/mail/mutations/spam",
        "/mutations/star" => "/api/v1/mail/mutations/star",
        "/mutations/read" => "/api/v1/mail/mutations/read",
        "/mutations/read-and-archive" => "/api/v1/mail/mutations/read-and-archive",
        "/mutations/labels" => "/api/v1/mail/mutations/labels",
        "/mutations/move" => "/api/v1/mail/mutations/move",

        // mail — actions
        "/actions/snooze" => "/api/v1/mail/actions/snooze",
        "/actions/snooze/presets" => "/api/v1/mail/actions/snooze/presets",
        "/actions/unsubscribe" => "/api/v1/mail/actions/unsubscribe",
        "/actions/unsubscribe-purge" => "/api/v1/mail/actions/unsubscribe-purge",
        "/actions/invite/reply" => "/api/v1/mail/actions/invite/reply",

        // mail — attachments
        "/attachments/open" => "/api/v1/mail/attachments/open",
        "/attachments/download" => "/api/v1/mail/attachments/download",

        // mail — labels
        "/labels/create" => "/api/v1/mail/labels/create",
        "/labels/rename" => "/api/v1/mail/labels/rename",
        "/labels/delete" => "/api/v1/mail/labels/delete",

        // mail — compose
        "/compose/session" => "/api/v1/mail/compose/session",
        "/compose/session/refresh" => "/api/v1/mail/compose/session/refresh",
        "/compose/session/restore" => "/api/v1/mail/compose/session/restore",
        "/compose/session/update" => "/api/v1/mail/compose/session/update",
        "/compose/session/send" => "/api/v1/mail/compose/session/send",
        "/compose/session/save" => "/api/v1/mail/compose/session/save",
        "/compose/session/discard" => "/api/v1/mail/compose/session/discard",

        // platform — rules
        "/rules" => "/api/v1/platform/rules",
        "/rules/detail" => "/api/v1/platform/rules/detail",
        "/rules/form" => "/api/v1/platform/rules/form",
        "/rules/history" => "/api/v1/platform/rules/history",
        "/rules/dry-run" => "/api/v1/platform/rules/dry-run",
        "/rules/upsert" => "/api/v1/platform/rules/upsert",
        "/rules/upsert-form" => "/api/v1/platform/rules/upsert-form",
        "/rules/delete" => "/api/v1/platform/rules/delete",

        // platform — accounts
        "/accounts" => "/api/v1/platform/accounts",
        "/accounts/test" => "/api/v1/platform/accounts/test",
        "/accounts/upsert" => "/api/v1/platform/accounts/upsert",
        "/accounts/default" => "/api/v1/platform/accounts/default",

        // platform — auth sessions
        "/auth/sessions/start" => "/api/v1/platform/auth/sessions/start",

        // platform — saved searches
        "/saved-searches/create" => "/api/v1/platform/saved-searches/create",
        "/saved-searches/delete" => "/api/v1/platform/saved-searches/delete",

        // platform — subscriptions / semantic
        "/subscriptions" => "/api/v1/platform/subscriptions",
        "/semantic/status" => "/api/v1/platform/semantic/status",
        "/semantic/reindex" => "/api/v1/platform/semantic/reindex",

        // events — top-level under v1
        "/events" => "/api/v1/events",

        _ => return None,
    })
}

/// Translate dynamic paths that include path parameters (e.g.
/// `/thread/{id}` and `/auth/sessions/{id}/...`).
fn translate_dynamic(path: &str) -> Option<String> {
    if let Some(rest) = path.strip_prefix("/thread/") {
        let new = if let Some(suffix) = rest.strip_suffix("/export") {
            format!("/api/v1/mail/threads/{suffix}/export")
        } else {
            format!("/api/v1/mail/threads/{rest}")
        };
        return Some(new);
    }
    if let Some(rest) = path.strip_prefix("/auth/sessions/") {
        let new = if let Some(id) = rest.strip_suffix("/cancel") {
            format!("/api/v1/platform/auth/sessions/{id}/cancel")
        } else if let Some(id) = rest.strip_suffix("/complete") {
            format!("/api/v1/platform/auth/sessions/{id}/complete")
        } else {
            // bare /auth/sessions/{id}
            format!("/api/v1/platform/auth/sessions/{rest}")
        };
        return Some(new);
    }
    None
}

/// Build a target URI that preserves the original query string.
fn target_uri(target_path: &str, original: &Uri) -> String {
    match original.query() {
        Some(q) => format!("{target_path}?{q}"),
        None => target_path.to_owned(),
    }
}

/// Axum middleware: 301-redirect known v0.4.x paths to their v0.5
/// equivalent. Pass through everything else to the next layer.
pub async fn redirect_legacy_paths(request: Request, next: axum::middleware::Next) -> Response {
    let uri = request.uri().clone();
    let path = uri.path();

    if let Some(target) = translate(path) {
        return Redirect::permanent(&target_uri(target, &uri)).into_response();
    }
    if let Some(target) = translate_dynamic(path) {
        return Redirect::permanent(&target_uri(&target, &uri)).into_response();
    }

    next.run(request).await
}

/// Returns 410 Gone for `/api/v1/...` paths that don't match a known route,
/// keeping unauthenticated probes from leaking into the auth layer's 401.
/// (Used by slice 4 — exposed here to avoid circular imports.)
#[allow(dead_code)]
pub async fn unknown_v1_path() -> Response {
    (
        StatusCode::NOT_FOUND,
        [(header::CONTENT_TYPE, "application/json")],
        r#"{"error":"unknown api path"}"#,
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn translates_admin_paths() {
        assert_eq!(translate("/status"), Some("/api/v1/admin/status"));
        assert_eq!(translate("/diagnostics"), Some("/api/v1/admin/diagnostics"));
    }

    #[test]
    fn translates_mail_paths() {
        assert_eq!(translate("/mailbox"), Some("/api/v1/mail/mailbox"));
        assert_eq!(translate("/sync"), Some("/api/v1/mail/sync"));
        assert_eq!(
            translate("/mutations/star"),
            Some("/api/v1/mail/mutations/star")
        );
    }

    #[test]
    fn translates_platform_paths() {
        assert_eq!(translate("/rules"), Some("/api/v1/platform/rules"));
        assert_eq!(translate("/accounts"), Some("/api/v1/platform/accounts"));
    }

    #[test]
    fn translates_dynamic_thread_paths() {
        let id = "01992f7e-0000-7000-8000-000000000000";
        assert_eq!(
            translate_dynamic(&format!("/thread/{id}")),
            Some(format!("/api/v1/mail/threads/{id}"))
        );
        assert_eq!(
            translate_dynamic(&format!("/thread/{id}/export")),
            Some(format!("/api/v1/mail/threads/{id}/export"))
        );
    }

    #[test]
    fn translates_dynamic_auth_session_paths() {
        let id = "session-abc";
        assert_eq!(
            translate_dynamic(&format!("/auth/sessions/{id}")),
            Some(format!("/api/v1/platform/auth/sessions/{id}"))
        );
        assert_eq!(
            translate_dynamic(&format!("/auth/sessions/{id}/cancel")),
            Some(format!("/api/v1/platform/auth/sessions/{id}/cancel"))
        );
        assert_eq!(
            translate_dynamic(&format!("/auth/sessions/{id}/complete")),
            Some(format!("/api/v1/platform/auth/sessions/{id}/complete"))
        );
    }

    #[test]
    fn unknown_paths_pass_through() {
        assert!(translate("/api/v1/admin/status").is_none());
        assert!(translate("/").is_none());
        assert!(translate_dynamic("/api/v1/mail/threads/foo").is_none());
    }

    #[test]
    fn target_uri_preserves_query_string() {
        let uri: Uri = "/mailbox?lens=inbox&limit=5".parse().unwrap();
        assert_eq!(
            target_uri("/api/v1/mail/mailbox", &uri),
            "/api/v1/mail/mailbox?lens=inbox&limit=5"
        );
    }
}
