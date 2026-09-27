//! Natural-language time preview for every web time field.
//!
//! A thin passthrough to the daemon's `ResolveTime`, so the web shows the
//! same resolution the CLI and TUI compute. Clients send the chosen
//! `choices[n].at` back to the real mutation unchanged.

use super::routes_v6::passthrough;
use super::*;

#[derive(Debug, Deserialize)]
struct ResolveTimeQuery {
    input: String,
    #[serde(default)]
    now: Option<DateTime<Utc>>,
    #[serde(default)]
    time_zone: Option<String>,
}

async fn resolve_time(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<ResolveTimeQuery>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    ensure_authorized(&headers, None, &state.config.auth_token)?;
    let response = ipc_request(
        &state.config.socket_path,
        Request::ResolveTime {
            input: query.input,
            now: query.now,
            time_zone: query.time_zone,
        },
    )
    .await?;
    passthrough(response)
}

pub(crate) fn extend_mail(router: Router<AppState>) -> Router<AppState> {
    router.route("/time/resolve", get(resolve_time))
}
