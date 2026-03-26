use super::super::*;

pub(crate) async fn rules(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(auth): Query<AuthQuery>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    ensure_authorized(&headers, auth.token.as_deref(), &state.config.auth_token)?;
    match ipc_request(&state.config.socket_path, Request::ListRules).await? {
        ResponseData::Rules { rules } => Ok(Json(json!({ "rules": rules }))),
        _ => Err(BridgeError::UnexpectedResponse),
    }
}

pub(crate) async fn rule_detail(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<RuleQuery>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    ensure_authorized(&headers, query.token.as_deref(), &state.config.auth_token)?;
    match ipc_request(
        &state.config.socket_path,
        Request::GetRule {
            rule: query.rule.clone(),
        },
    )
    .await?
    {
        ResponseData::RuleData { rule } => Ok(Json(json!({ "rule": rule }))),
        _ => Err(BridgeError::UnexpectedResponse),
    }
}

pub(crate) async fn rule_form(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<RuleQuery>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    ensure_authorized(&headers, query.token.as_deref(), &state.config.auth_token)?;
    match ipc_request(
        &state.config.socket_path,
        Request::GetRuleForm {
            rule: query.rule.clone(),
        },
    )
    .await?
    {
        ResponseData::RuleFormData { form } => Ok(Json(json!({ "form": form }))),
        _ => Err(BridgeError::UnexpectedResponse),
    }
}

pub(crate) async fn rule_history(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<RuleQuery>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    ensure_authorized(&headers, query.token.as_deref(), &state.config.auth_token)?;
    match ipc_request(
        &state.config.socket_path,
        Request::ListRuleHistory {
            rule: Some(query.rule.clone()),
            limit: 20,
        },
    )
    .await?
    {
        ResponseData::RuleHistory { entries } => Ok(Json(json!({ "entries": entries }))),
        _ => Err(BridgeError::UnexpectedResponse),
    }
}

pub(crate) async fn rule_dry_run(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<RuleQuery>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    ensure_authorized(&headers, query.token.as_deref(), &state.config.auth_token)?;
    match ipc_request(
        &state.config.socket_path,
        Request::DryRunRules {
            rule: Some(query.rule.clone()),
            all: false,
            after: None,
        },
    )
    .await?
    {
        ResponseData::RuleDryRun { results } => Ok(Json(json!({ "results": results }))),
        _ => Err(BridgeError::UnexpectedResponse),
    }
}

pub(crate) async fn upsert_rule(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(auth): Query<AuthQuery>,
    Json(request): Json<UpsertRuleRequest>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    ensure_authorized(&headers, auth.token.as_deref(), &state.config.auth_token)?;
    match ipc_request(
        &state.config.socket_path,
        Request::UpsertRule { rule: request.rule },
    )
    .await?
    {
        ResponseData::RuleData { rule } => Ok(Json(json!({ "rule": rule }))),
        _ => Err(BridgeError::UnexpectedResponse),
    }
}

pub(crate) async fn upsert_rule_form(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(auth): Query<AuthQuery>,
    Json(request): Json<UpsertRuleFormRequest>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    ensure_authorized(&headers, auth.token.as_deref(), &state.config.auth_token)?;
    match ipc_request(
        &state.config.socket_path,
        Request::UpsertRuleForm {
            existing_rule: request.existing_rule,
            name: request.name,
            condition: request.condition,
            action: request.action,
            priority: request.priority,
            enabled: request.enabled,
        },
    )
    .await?
    {
        ResponseData::RuleData { rule } => Ok(Json(json!({ "rule": rule }))),
        _ => Err(BridgeError::UnexpectedResponse),
    }
}

pub(crate) async fn delete_rule(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(auth): Query<AuthQuery>,
    Json(request): Json<DeleteRuleRequest>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    ensure_authorized(&headers, auth.token.as_deref(), &state.config.auth_token)?;
    let _ = ack_request(
        &state.config.socket_path,
        Request::DeleteRule { rule: request.rule },
    )
    .await?;
    Ok(Json(json!({ "ok": true })))
}
