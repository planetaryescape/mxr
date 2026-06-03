use super::*;

fn admin_router() -> Router<AppState> {
    Router::new()
        .route("/status", get(status))
        .route("/diagnostics", get(diagnostics))
        .route("/diagnostics/bug-report", get(generate_bug_report))
}

/// Liveness endpoint — unauthenticated. Surfaces just enough for clients
/// and orchestrators to verify the bridge is up and the protocol version
/// they expect, before they go acquire the bridge token.
async fn health() -> Json<serde_json::Value> {
    Json(json!({
        "status": "ok",
        "service": "mxr-bridge",
        "protocol_version": mxr_protocol::IPC_PROTOCOL_VERSION,
    }))
}

/// Returns the SPA-relevant locale bundle for the daemon's active locale.
/// The SPA fetches this once at startup and caches in TanStack Query, so the
/// `InviteCard` and other components can render localized strings without
/// shipping translations in the JS bundle. To add a language, append a new
/// `Locale` to `mxr_core::i18n::AVAILABLE_LOCALES` and configure
/// `MXR_LOCALE`. The structure mirrors `InviteStrings`/`StatusStrings` from
/// `mxr_core::i18n`.
async fn i18n_bundle() -> Json<serde_json::Value> {
    let locale = mxr_core::i18n::DEFAULT_LOCALE;
    let i = locale.invite;
    let s = locale.status;
    Json(json!({
        "code": locale.code,
        "invite": {
            "card_title": i.card_title,
            "chip_label_accept": i.chip_label_accept,
            "chip_label_tentative": i.chip_label_tentative,
            "chip_label_decline": i.chip_label_decline,
            "state_label_accepted": i.state_label_accepted,
            "state_label_tentative": i.state_label_tentative,
            "state_label_declined": i.state_label_declined,
            "hint_change_response": i.hint_change_response,
            "hint_comment": i.hint_comment,
            "banner_cancelled": i.banner_cancelled,
            "banner_publish": i.banner_publish,
            "banner_parse_warning": i.banner_parse_warning,
            "banner_updated": i.banner_updated,
            "banner_counter": i.banner_counter,
        },
        "status": {
            "invite_pending_accept": s.invite_pending_accept,
            "invite_pending_tentative": s.invite_pending_tentative,
            "invite_pending_decline": s.invite_pending_decline,
            "invite_cancelled": s.invite_cancelled,
        }
    }))
}

/// Same-machine handshake. Returns the bridge token to callers whose
/// TCP peer is a loopback address, gated by `[bridge].auto_local_token`.
///
/// This is *not* a way around bearer auth in general. The endpoint
/// refuses if either:
///   - the operator has disabled it via `auto_local_token = false`, or
///   - the connecting peer's IP is not a loopback address.
///
/// In both refusal cases it returns 404, not 401/403, so cross-network
/// scanners cannot tell the endpoint exists.
async fn local_token_handshake(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
) -> Response {
    if !state.config.auto_local_token {
        return (StatusCode::NOT_FOUND, Json(json!({"error": "not found"}))).into_response();
    }
    if !peer.ip().is_loopback() {
        tracing::debug!(?peer, "local-token handshake refused: non-loopback peer");
        return (StatusCode::NOT_FOUND, Json(json!({"error": "not found"}))).into_response();
    }
    Json(json!({
        "token": state.config.auth_token,
        "source": "local-handshake",
    }))
    .into_response()
}

/// Routes under `/api/v1/mail/*` — read, search, mutate, sync, compose.
fn mail_router() -> Router<AppState> {
    Router::new()
        .route("/mailbox", get(mailbox))
        .route("/search", get(search))
        .route("/threads/{thread_id}", get(thread))
        .route("/threads/{thread_id}/export", get(export_thread))
        .route("/drafts", get(list_drafts))
        .route("/snoozed", get(list_snoozed))
        .route("/invites", get(list_invites))
        .route("/deliveries", get(list_deliveries))
        .route("/deliveries/scan", post(scan_deliveries))
        .route("/deliveries/{delivery_id}", get(get_delivery))
        .route("/deliveries/{delivery_id}/resolve", post(resolve_delivery))
        .route("/deliveries/{delivery_id}/dismiss", post(dismiss_delivery))
        .route("/sync", post(trigger_sync))
        .route("/mutations/archive", post(archive))
        .route("/mutations/trash", post(trash))
        .route("/mutations/spam", post(spam))
        .route("/mutations/star", post(star))
        .route("/mutations/read", post(mark_read))
        .route("/mutations/read-and-archive", post(mark_read_and_archive))
        .route("/mutations/labels", post(modify_labels))
        .route("/mutations/move", post(move_messages))
        .route("/actions/snooze/presets", get(snooze_presets))
        .route("/actions/snooze", post(snooze))
        .route("/actions/unsubscribe", post(unsubscribe))
        .route("/actions/unsubscribe-purge", post(unsubscribe_purge))
        .route("/actions/invite/reply", post(reply_to_invite))
        .route("/attachments/open", post(open_attachment))
        .route("/attachments/download", post(download_attachment))
        .route("/labels/create", post(create_label))
        .route("/labels/rename", post(rename_label))
        .route("/labels/delete", post(delete_label))
        .route("/compose/session", post(start_compose_session))
        .route("/compose/session/refresh", post(refresh_compose_session))
        .route("/compose/session/restore", post(restore_compose_session))
        .route("/compose/session/update", post(update_compose_session))
        .route("/compose/session/send", post(send_compose_session))
        .route("/compose/session/save", post(save_compose_session))
        .route(
            "/compose/session/attachment",
            post(upload_compose_attachment),
        )
        .route("/compose/session/discard", post(discard_compose_session))
}

/// Routes under `/api/v1/platform/*` — accounts, rules, saved searches,
/// subscriptions, semantic.
fn platform_router() -> Router<AppState> {
    Router::new()
        .route("/rules", get(rules))
        .route("/rules/detail", get(rule_detail))
        .route("/rules/form", get(rule_form))
        .route("/rules/history", get(rule_history))
        .route("/rules/dry-run", get(rule_dry_run))
        .route("/rules/upsert", post(upsert_rule))
        .route("/rules/upsert-form", post(upsert_rule_form))
        .route("/rules/delete", post(delete_rule))
        .route("/accounts", get(accounts))
        .route("/accounts/test", post(test_account))
        .route("/accounts/upsert", post(upsert_account))
        .route("/accounts/default", post(set_default_account))
        .route("/auth/sessions/start", post(start_auth_session))
        .route("/auth/sessions/{session_id}", get(get_auth_session))
        .route(
            "/auth/sessions/{session_id}/cancel",
            post(cancel_auth_session),
        )
        .route(
            "/auth/sessions/{session_id}/complete",
            post(complete_auth_session),
        )
        .route("/saved-searches/create", post(create_saved_search))
        .route("/saved-searches/update", post(update_saved_search))
        .route("/saved-searches/delete", post(delete_saved_search))
        .route("/subscriptions", get(list_subscriptions))
        .route("/llm/status", get(get_llm_status))
        .route("/llm/config", get(get_llm_config).post(update_llm_config))
        .route("/semantic/status", get(get_semantic_status))
        .route("/semantic/reindex", post(trigger_semantic_reindex))
}

/// Client-specific UI shaping. Per AGENTS.md the `client-specific` IPC
/// bucket is not part of the core mail surface, so this lives under its
/// own prefix.
fn client_router() -> Router<AppState> {
    Router::new().route("/shell", get(shell))
}

pub fn app(config: WebServerConfig) -> Router {
    let cors = middleware::cors_layer(&config.cors_allowlist);
    let host_allowlist = std::sync::Arc::new(config.host_allowlist.clone());
    let state = AppState::new(config);

    let v1 = Router::new()
        .route("/health", get(health))
        .route("/auth/local-token", get(local_token_handshake))
        .route("/i18n", get(i18n_bundle))
        .nest("/admin", routes_v6::extend_admin(admin_router()))
        .nest("/mail", routes_v6::extend_mail(mail_router()))
        .nest("/platform", routes_v6::extend_platform(platform_router()))
        .nest("/client", client_router())
        .route("/events", get(events))
        .with_state(state.clone());

    let docs_router = Router::new()
        .merge(
            SwaggerUi::new("/api/v1/docs").url("/api/v1/openapi.json", openapi::ApiDoc::openapi()),
        )
        .route_layer(axum::middleware::from_fn_with_state(
            state.clone(),
            middleware::require_bridge_auth,
        ));

    let router = Router::new().nest("/api/v1", v1).merge(docs_router);

    #[cfg(feature = "web-ui")]
    let router = router.merge(spa::router());

    router
        .layer(axum::middleware::from_fn(legacy::redirect_legacy_paths))
        .layer(axum::middleware::from_fn_with_state(
            host_allowlist,
            middleware::host_allowlist,
        ))
        .layer(cors)
        .layer(axum::middleware::from_fn(middleware::security_headers))
}

pub async fn serve(listener: TcpListener, config: WebServerConfig) -> std::io::Result<()> {
    axum::serve(
        listener,
        app(config).into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await
}

pub async fn bind_and_serve(
    host: std::net::IpAddr,
    port: u16,
    config: WebServerConfig,
) -> std::io::Result<SocketAddr> {
    let listener = TcpListener::bind((host, port)).await?;
    let addr = listener.local_addr()?;
    tokio::spawn(async move {
        let _ = serve(listener, config).await;
    });
    Ok(addr)
}

/// Default bridge port. Chosen as a high unprivileged port that doesn't
/// clash with the common dev-server set (3000/5173/8000/8080/7777/4200).
/// Backwards-compat note: pre-launch the bridge defaulted to 7777; if
/// you're upgrading existing setups, your `[bridge].port` config wins.
pub const DEFAULT_BRIDGE_PORT: u16 = 42829;

/// How many ports to walk through when the configured one is in use.
/// Capped so a totally broken host (no free ports in a wide range) still
/// fails in finite time with a clear error.
pub const PORT_RETRY_ATTEMPTS: u16 = 32;

/// Attempt to bind a `TcpListener` to `host:port`. If the port is taken
/// and `retry` is true, increment by one and try again up to
/// `PORT_RETRY_ATTEMPTS` times.
///
/// Returns the bound listener (caller is responsible for serving it).
pub async fn bind_listener(
    host: std::net::IpAddr,
    port: u16,
    retry: bool,
) -> std::io::Result<TcpListener> {
    let mut candidate = port;
    let max = if retry {
        port.saturating_add(PORT_RETRY_ATTEMPTS)
    } else {
        port
    };
    loop {
        match TcpListener::bind((host, candidate)).await {
            Ok(listener) => return Ok(listener),
            Err(error) if retry && is_addr_in_use(&error) && candidate < max => {
                tracing::debug!(
                    "bridge port {candidate} in use, trying {next}",
                    next = candidate + 1
                );
                candidate += 1;
                continue;
            }
            Err(error) => return Err(error),
        }
    }
}

fn is_addr_in_use(error: &std::io::Error) -> bool {
    matches!(error.kind(), std::io::ErrorKind::AddrInUse)
}
