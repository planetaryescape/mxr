//! Archive: the ledger, a record's card, the answer box, corrections,
//! filing and the export. Thin passthroughs to the daemon, which owns
//! detection, the ledger's months and totals and every preview's
//! selection.

use super::place_routes::parse_optional_account;
use super::routes_v6::{dispatch, passthrough};
use super::*;
use mxr_protocol::{RecordEditData, RecordFilterData, RecordKindData};

// Mirrors the protocol's serde defaults so an omitted limit behaves the
// same over HTTP as over IPC.
const DEFAULT_RECORD_LIMIT: u32 = 200;
const DEFAULT_ANSWER_LIMIT: u32 = 4;
/// A list's page: the ledger's page size.
const DEFAULT_ANSWER_LIST_LIMIT: u32 = 200;

fn parse_kinds(raw: Option<&str>) -> Result<Vec<RecordKindData>, BridgeError> {
    raw.unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|kind| !kind.is_empty())
        .map(|kind| {
            RecordKindData::parse(kind).ok_or_else(|| {
                BridgeError::BadRequest(format!(
                    "unknown record kind `{kind}`; use receipt, order, booking, invoice, statement, ticket, contract, warranty or account"
                ))
            })
        })
        .collect()
}

#[derive(Debug, Deserialize)]
struct LedgerQuery {
    #[serde(default)]
    token: Option<String>,
    /// Omitted: every account.
    #[serde(default, alias = "account_id")]
    account: Option<String>,
    /// Comma-separated kinds.
    #[serde(default)]
    kind: Option<String>,
    #[serde(default)]
    issuer: Option<String>,
    #[serde(default)]
    year: Option<i32>,
    #[serde(default)]
    min_amount_minor: Option<i64>,
    #[serde(default)]
    max_amount_minor: Option<i64>,
    #[serde(default)]
    has_pdf: Option<bool>,
    #[serde(default)]
    checked: Option<bool>,
    #[serde(default)]
    group: Option<String>,
    #[serde(default)]
    limit: Option<u32>,
    #[serde(default)]
    offset: Option<u32>,
}

impl LedgerQuery {
    fn filter(&self) -> Result<RecordFilterData, BridgeError> {
        Ok(RecordFilterData {
            kinds: parse_kinds(self.kind.as_deref())?,
            issuer: self
                .issuer
                .clone()
                .filter(|issuer| !issuer.trim().is_empty()),
            year: self.year,
            min_amount_minor: self.min_amount_minor,
            max_amount_minor: self.max_amount_minor,
            has_pdf: self.has_pdf,
            checked: self.checked,
            group_id: self.group.clone().filter(|group| !group.is_empty()),
        })
    }
}

async fn ledger(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<LedgerQuery>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    let account_id = parse_optional_account(query.account.as_deref())?;
    let response = dispatch(
        &state,
        &headers,
        query.token.as_deref(),
        Request::ListRecords {
            account_id,
            filter: query.filter()?,
            limit: query.limit.unwrap_or(DEFAULT_RECORD_LIMIT),
            offset: query.offset.unwrap_or(0),
        },
    )
    .await?;
    passthrough(response)
}

#[derive(Debug, Deserialize)]
struct TokenQuery {
    #[serde(default)]
    token: Option<String>,
}

async fn get_record(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath(record_id): AxumPath<String>,
    Query(query): Query<TokenQuery>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    let response = dispatch(
        &state,
        &headers,
        query.token.as_deref(),
        Request::GetRecord { record_id },
    )
    .await?;
    passthrough(response)
}

#[derive(Debug, Deserialize)]
struct AnswerQuery {
    #[serde(default)]
    token: Option<String>,
    q: String,
    #[serde(default, alias = "account_id")]
    account: Option<String>,
    /// Fall back to `mxr ask` when no record matches. Defaults to true.
    #[serde(default)]
    fallback: Option<bool>,
    #[serde(default)]
    limit: Option<u32>,
    /// Every match as a list: "Show all".
    #[serde(default)]
    list: Option<bool>,
    #[serde(default)]
    offset: Option<u32>,
    #[serde(default)]
    list_limit: Option<u32>,
}

async fn answer(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<AnswerQuery>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    if query.q.trim().is_empty() {
        return Err(BridgeError::BadRequest("q must not be empty".into()));
    }
    let account_id = parse_optional_account(query.account.as_deref())?;
    let response = dispatch(
        &state,
        &headers,
        query.token.as_deref(),
        Request::AnswerFromRecords {
            query: query.q,
            account_id,
            fallback: query.fallback.unwrap_or(true),
            limit: query.limit.unwrap_or(DEFAULT_ANSWER_LIMIT),
            list: query.list.unwrap_or(false),
            offset: query.offset.unwrap_or(0),
            list_limit: query.list_limit.unwrap_or(DEFAULT_ANSWER_LIST_LIMIT),
        },
    )
    .await?;
    passthrough(response)
}

/// Body of `POST /api/v1/mail/records/{record_id}/field`.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub(crate) struct RecordFieldBody {
    edit: RecordEditData,
    /// With an issuer name: rename this sender's issuer on every record.
    #[serde(default)]
    apply_to_sender: bool,
    #[serde(default)]
    dry_run: bool,
}

async fn set_field(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath(record_id): AxumPath<String>,
    Json(body): Json<RecordFieldBody>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    let response = dispatch(
        &state,
        &headers,
        None,
        Request::SetRecordField {
            record_id,
            edit: body.edit,
            apply_to_sender: body.apply_to_sender,
            dry_run: body.dry_run,
        },
    )
    .await?;
    passthrough(response)
}

/// Body of `POST /api/v1/mail/records/dismiss`.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub(crate) struct RecordDismissBody {
    record_ids: Vec<String>,
    /// Bring dismissed records back.
    #[serde(default)]
    restore: bool,
    #[serde(default)]
    dry_run: bool,
}

async fn dismiss(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<RecordDismissBody>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    if body.record_ids.is_empty() {
        return Err(BridgeError::BadRequest(
            "record_ids must not be empty".into(),
        ));
    }
    let response = dispatch(
        &state,
        &headers,
        None,
        Request::DismissRecord {
            record_ids: body.record_ids,
            restore: body.restore,
            dry_run: body.dry_run,
        },
    )
    .await?;
    passthrough(response)
}

/// Body of `POST /api/v1/mail/records/file`.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub(crate) struct RecordFileBody {
    message_id: String,
    #[serde(default)]
    kind: Option<RecordKindData>,
    #[serde(default)]
    dry_run: bool,
}

async fn file(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<RecordFileBody>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    let message_id = parse_message_id(&body.message_id)?;
    let response = dispatch(
        &state,
        &headers,
        None,
        Request::FileRecord {
            message_id,
            kind: body.kind,
            dry_run: body.dry_run,
        },
    )
    .await?;
    passthrough(response)
}

/// Body of `POST /api/v1/mail/records/sender`.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub(crate) struct RecordSenderBody {
    message_id: String,
    /// `always` or `never`; omitted lets the rules decide again.
    #[serde(default)]
    verdict: Option<String>,
    #[serde(default)]
    kind: Option<RecordKindData>,
    #[serde(default)]
    dry_run: bool,
}

async fn sender(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<RecordSenderBody>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    let message_id = parse_message_id(&body.message_id)?;
    if let Some(verdict) = body.verdict.as_deref() {
        if !matches!(verdict, "always" | "never") {
            return Err(BridgeError::BadRequest(format!(
                "unknown verdict `{verdict}`; use always or never"
            )));
        }
    }
    let response = dispatch(
        &state,
        &headers,
        None,
        Request::SetRecordSender {
            message_id,
            verdict: body.verdict,
            kind: body.kind,
            dry_run: body.dry_run,
        },
    )
    .await?;
    passthrough(response)
}

/// Body of `POST /api/v1/mail/records/export`.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub(crate) struct RecordExportBody {
    #[serde(default)]
    account_id: Option<String>,
    #[serde(default)]
    filter: RecordFilterData,
    /// An absolute folder to copy each record's PDF into.
    #[serde(default)]
    attachments_dir: Option<String>,
    #[serde(default)]
    dry_run: bool,
}

async fn export(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<RecordExportBody>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    let account_id = parse_optional_account(body.account_id.as_deref())?;
    let response = dispatch(
        &state,
        &headers,
        None,
        Request::ExportRecords {
            account_id,
            filter: body.filter,
            attachments_dir: body.attachments_dir,
            dry_run: body.dry_run,
        },
    )
    .await?;
    passthrough(response)
}

pub(crate) fn extend_mail(router: Router<AppState>) -> Router<AppState> {
    router
        .route("/records", get(ledger))
        .route("/records/answer", get(answer))
        .route("/records/dismiss", post(dismiss))
        .route("/records/file", post(file))
        .route("/records/sender", post(sender))
        .route("/records/export", post(export))
        .route("/records/{record_id}", get(get_record))
        .route("/records/{record_id}/field", post(set_field))
}
