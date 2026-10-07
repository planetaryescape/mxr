//! Freshness over the bridge: the newest arrival, each account's sync
//! health and the last few arrivals with their modes. A thin passthrough
//! to `GetFreshness`, which owns the cap and the health rules.

use super::place_routes::parse_optional_account;
use super::routes_v6::{dispatch, passthrough};
use super::*;

#[derive(Debug, Deserialize)]
struct FreshnessQuery {
    #[serde(default)]
    token: Option<String>,
    /// Omitted: every account.
    #[serde(default, alias = "account_id")]
    account: Option<String>,
    /// Arrivals to return; the daemon defaults and caps it.
    #[serde(default)]
    limit: Option<u32>,
}

async fn get_freshness(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<FreshnessQuery>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    let account_id = parse_optional_account(query.account.as_deref())?;
    let response = dispatch(
        &state,
        &headers,
        query.token.as_deref(),
        Request::GetFreshness {
            account_id,
            limit: query.limit,
        },
    )
    .await?;
    passthrough(response)
}

pub(crate) fn extend_mail(router: Router<AppState>) -> Router<AppState> {
    router.route("/freshness", get(get_freshness))
}
