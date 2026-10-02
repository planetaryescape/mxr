//! To do: the runway, one row with its provenance, the catch-up, and the
//! mutations. Thin passthroughs to the daemon, which owns detection, the
//! bands, the labels and every preview's selection.

use super::routes_v6::{dispatch, passthrough};
use super::*;
use mxr_protocol::{TodoCatchupDecisionData, TodoEditData, TodoStateActionData, TodoStateData};

// Mirrors the protocol's serde default so an omitted limit behaves the same
// over HTTP as over IPC.
const DEFAULT_TODO_LIMIT: u32 = 200;

fn parse_optional_account(raw: Option<&str>) -> Result<Option<AccountId>, BridgeError> {
    raw.filter(|value| !value.is_empty())
        .map(parse_account_id)
        .transpose()
}

fn parse_state(raw: &str) -> Result<TodoStateData, BridgeError> {
    match raw {
        "open" => Ok(TodoStateData::Open),
        "done" => Ok(TodoStateData::Done),
        "dismissed" => Ok(TodoStateData::Dismissed),
        "expired" => Ok(TodoStateData::Expired),
        other => Err(BridgeError::BadRequest(format!(
            "unknown to-do state `{other}`; use open, done, dismissed or expired"
        ))),
    }
}

#[derive(Debug, Deserialize)]
struct RunwayQuery {
    #[serde(default)]
    token: Option<String>,
    /// Omitted: every account.
    #[serde(default, alias = "account_id")]
    account: Option<String>,
    /// Record that To do was opened, so "expired since you last looked"
    /// counts from now.
    #[serde(default)]
    mark_seen: bool,
    #[serde(default)]
    limit: Option<u32>,
}

async fn runway(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<RunwayQuery>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    let account_id = parse_optional_account(query.account.as_deref())?;
    let response = dispatch(
        &state,
        &headers,
        query.token.as_deref(),
        Request::GetTodoRunway {
            account_id,
            mark_seen: query.mark_seen,
        },
    )
    .await?;
    passthrough(response)
}

async fn list_in_state(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath(todo_state): AxumPath<String>,
    Query(query): Query<RunwayQuery>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    let account_id = parse_optional_account(query.account.as_deref())?;
    let response = dispatch(
        &state,
        &headers,
        query.token.as_deref(),
        Request::ListTodos {
            account_id,
            state: parse_state(&todo_state)?,
            limit: query.limit.unwrap_or(DEFAULT_TODO_LIMIT),
        },
    )
    .await?;
    passthrough(response)
}

async fn get_todo(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath(todo_id): AxumPath<String>,
    Query(query): Query<RunwayQuery>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    let response = dispatch(
        &state,
        &headers,
        query.token.as_deref(),
        Request::GetTodo { todo_id },
    )
    .await?;
    passthrough(response)
}

async fn get_catchup(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<RunwayQuery>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    let account_id = parse_optional_account(query.account.as_deref())?;
    let response = dispatch(
        &state,
        &headers,
        query.token.as_deref(),
        Request::GetTodoCatchup { account_id },
    )
    .await?;
    passthrough(response)
}

/// Body of `POST /api/v1/mail/todos/state`.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub(crate) struct TodoStateBody {
    /// Full ids or unique prefixes.
    todo_ids: Vec<String>,
    action: TodoStateActionData,
    /// Preview only: the rows that would change, nothing written.
    #[serde(default)]
    dry_run: bool,
}

async fn set_state(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<TodoStateBody>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    if body.todo_ids.is_empty() {
        return Err(BridgeError::BadRequest("todo_ids must not be empty".into()));
    }
    let response = dispatch(
        &state,
        &headers,
        None,
        Request::SetTodoState {
            todo_ids: body.todo_ids,
            action: body.action,
            dry_run: body.dry_run,
        },
    )
    .await?;
    passthrough(response)
}

/// Body of `POST /api/v1/mail/todos/{todo_id}/schedule`.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub(crate) struct TodoScheduleBody {
    /// "mon 9am", "in 3d" or RFC3339; omitted clears your date.
    #[serde(default)]
    when: Option<String>,
    /// The browser's IANA zone, so "mon 9am" means the user's Monday.
    #[serde(default)]
    time_zone: Option<String>,
    #[serde(default)]
    dry_run: bool,
}

async fn schedule(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath(todo_id): AxumPath<String>,
    Json(body): Json<TodoScheduleBody>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    let response = dispatch(
        &state,
        &headers,
        None,
        Request::ScheduleTodo {
            todo_id,
            when: body.when,
            time_zone: body.time_zone,
            dry_run: body.dry_run,
        },
    )
    .await?;
    passthrough(response)
}

/// Body of `POST /api/v1/mail/todos/{todo_id}/edit`.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub(crate) struct TodoEditBody {
    /// title, due, amount, counterparty or kind; an empty value clears due,
    /// amount and counterparty.
    edits: Vec<TodoEditData>,
    #[serde(default)]
    time_zone: Option<String>,
    #[serde(default)]
    dry_run: bool,
}

async fn edit(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath(todo_id): AxumPath<String>,
    Json(body): Json<TodoEditBody>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    let response = dispatch(
        &state,
        &headers,
        None,
        Request::UpdateTodo {
            todo_id,
            edits: body.edits,
            time_zone: body.time_zone,
            dry_run: body.dry_run,
        },
    )
    .await?;
    passthrough(response)
}

/// Body of `POST /api/v1/mail/todos`.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub(crate) struct TodoCreateBody {
    /// The message it's about.
    message_id: String,
    /// What to do, starting with the verb.
    title: String,
    /// Defaults to `other`.
    #[serde(default)]
    kind: Option<String>,
    /// A phrase ("fri", "9 oct") or RFC3339.
    #[serde(default)]
    due: Option<String>,
    #[serde(default)]
    time_zone: Option<String>,
    #[serde(default)]
    dry_run: bool,
}

async fn create(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<TodoCreateBody>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    let message_id = parse_message_id(&body.message_id)?;
    let response = dispatch(
        &state,
        &headers,
        None,
        Request::CreateTodo {
            message_id,
            title: body.title,
            kind: body.kind,
            due: body.due,
            time_zone: body.time_zone,
            dry_run: body.dry_run,
        },
    )
    .await?;
    passthrough(response)
}

/// Body of `POST /api/v1/mail/todos/catchup`.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub(crate) struct TodoCatchupBody {
    /// Omitted: every account.
    #[serde(default)]
    account_id: Option<String>,
    decision: TodoCatchupDecisionData,
    #[serde(default)]
    dry_run: bool,
}

async fn set_catchup(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<TodoCatchupBody>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    let account_id = parse_optional_account(body.account_id.as_deref())?;
    let response = dispatch(
        &state,
        &headers,
        None,
        Request::SetTodoCatchup {
            account_id,
            decision: body.decision,
            dry_run: body.dry_run,
        },
    )
    .await?;
    passthrough(response)
}

pub(crate) fn extend_mail(router: Router<AppState>) -> Router<AppState> {
    router
        .route("/todos", get(runway).post(create))
        .route("/todos/catchup", get(get_catchup).post(set_catchup))
        .route("/todos/state", post(set_state))
        .route("/todos/in/{state}", get(list_in_state))
        .route("/todos/{todo_id}", get(get_todo))
        .route("/todos/{todo_id}/schedule", post(schedule))
        .route("/todos/{todo_id}/edit", post(edit))
}
