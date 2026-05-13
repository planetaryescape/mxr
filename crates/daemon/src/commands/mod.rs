pub mod accounts;
pub mod ask;
pub mod briefing;
pub mod bug_report;
pub mod cadence;
pub mod cat;
pub mod commitments;
pub mod completions;
pub mod config;
pub mod contacts;
pub mod count;
pub mod decisions;
pub mod demo;
pub mod doctor;
pub mod draft;
pub mod draft_assist;
pub mod events;
pub mod export;
pub mod headers;
pub mod history;
pub mod humanize;
pub mod labels;
pub mod llm;
pub mod logs;
pub mod mutations;
pub mod notify;
pub mod owed;
pub mod profile;
pub mod remind;
pub mod replies;
pub mod reset;
pub mod response_time;
pub mod rules;
pub mod saved;
pub mod screener;
pub mod send_time;
pub mod search;
pub mod selection;
pub mod semantic;
pub mod sender;
pub mod senders;
pub mod setup;
pub mod signatures;
pub mod snippets;
pub mod stale;
pub mod status;
pub mod storage;
pub mod subscriptions;
pub mod summarize;
pub mod sync_cmd;
pub mod thread;
pub mod version;
pub mod voice;
pub mod web;
pub mod wrapped;

use crate::ipc_client::IpcClient;
use mxr_protocol::{Request, Response, ResponseData};

/// Extract a typed value from a daemon `Response`, converting `Response::Error`
/// into an `anyhow` error and rejecting unexpected variants.
pub(crate) fn expect_response<F, T>(resp: Response, extract: F) -> anyhow::Result<T>
where
    F: FnOnce(Response) -> Option<T>,
{
    match resp {
        Response::Error { message, .. } => anyhow::bail!("{message}"),
        other => extract(other).ok_or_else(|| anyhow::anyhow!("unexpected response from daemon")),
    }
}

pub(crate) async fn resolve_account(
    client: &mut IpcClient,
    explicit: Option<&str>,
) -> anyhow::Result<mxr_core::AccountId> {
    let resp = client.request(Request::ListAccounts).await?;
    let accounts = match resp {
        Response::Ok {
            data: ResponseData::Accounts { accounts },
        } => accounts,
        Response::Error { message, .. } => anyhow::bail!(message),
        _ => anyhow::bail!("Unexpected response"),
    };
    if let Some(key) = explicit {
        return accounts
            .into_iter()
            .find(|account| {
                account.key.as_deref() == Some(key)
                    || account.email == key
                    || account.account_id.to_string() == key
            })
            .map(|account| account.account_id)
            .ok_or_else(|| anyhow::anyhow!("No account matching '{key}'"));
    }
    if accounts.len() == 1 {
        return Ok(accounts.into_iter().next().unwrap().account_id);
    }
    if let Some(default) = accounts.iter().find(|account| account.is_default) {
        return Ok(default.account_id.clone());
    }
    anyhow::bail!("Multiple accounts configured; pass --account <key>")
}
