pub mod accounts;
pub mod activity;
pub mod ask;
pub mod briefing;
pub mod bug_report;
pub mod cadence;
pub mod cat;
pub mod chimes;
pub mod commitments;
pub mod completions;
pub mod config;
pub mod contacts;
pub mod count;
pub mod decisions;
pub mod deliveries;
pub mod demo;
pub mod doctor;
pub mod draft;
pub mod draft_assist;
pub mod draft_output;
pub mod events;
pub mod expert;
pub mod export;
pub mod headers;
pub mod history;
pub mod humanize;
pub mod invites;
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
pub mod search;
pub mod selection;
pub mod semantic;
pub mod send_time;
pub mod sender;
pub mod senders;
pub mod setup;
pub mod signatures;
pub mod snippets;
pub mod stale;
pub mod status;
pub mod storage;
pub mod subscriptions;
pub mod suggest_recipients;
pub mod summarize;
pub mod sync_cmd;
pub mod thread;
pub mod threads;
pub mod triage;
pub mod version;
pub mod voice;
pub mod web;
pub mod whois;
pub mod wrapped;

use crate::ipc_client::IpcClient;
use mxr_core::{AccountId, DraftId, MessageId};
use mxr_protocol::{AccountSummaryData, Request, Response, ResponseData};

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
) -> anyhow::Result<AccountId> {
    let resp = client.request(Request::ListAccounts).await?;
    let accounts = match resp {
        Response::Ok {
            data: ResponseData::Accounts { accounts },
        } => accounts,
        Response::Error { message, .. } => anyhow::bail!(message),
        _ => anyhow::bail!("Unexpected response"),
    };
    if let Some(key) = explicit {
        return resolve_account_from_list(&accounts, key);
    }
    if let [account] = accounts.as_slice() {
        return Ok(account.account_id.clone());
    }
    if let Some(default) = accounts.iter().find(|account| account.is_default) {
        return Ok(default.account_id.clone());
    }
    anyhow::bail!("Multiple accounts configured; pass --account <key>")
}

pub(crate) async fn resolve_optional_account(
    client: &mut IpcClient,
    explicit: Option<&str>,
) -> anyhow::Result<Option<AccountId>> {
    match explicit {
        Some(selector) => resolve_account(client, Some(selector)).await.map(Some),
        None => Ok(None),
    }
}

pub(crate) async fn ensure_message_account(
    client: &mut IpcClient,
    message_id: &MessageId,
    account_id: Option<&AccountId>,
) -> anyhow::Result<()> {
    let Some(account_id) = account_id else {
        return Ok(());
    };
    let resp = client
        .request(Request::GetEnvelope {
            message_id: message_id.clone(),
        })
        .await?;
    let envelope = expect_response(resp, |response| match response {
        Response::Ok {
            data: ResponseData::Envelope { envelope },
        } => Some(envelope),
        _ => None,
    })?;
    if &envelope.account_id != account_id {
        anyhow::bail!(
            "Message {} belongs to a different account",
            message_id.as_str()
        );
    }
    Ok(())
}

pub(crate) async fn get_draft_for_account(
    client: &mut IpcClient,
    draft_id: &DraftId,
    account_id: Option<&AccountId>,
) -> anyhow::Result<mxr_core::Draft> {
    let resp = client.request(Request::ListDrafts).await?;
    let drafts = expect_response(resp, |response| match response {
        Response::Ok {
            data: ResponseData::Drafts { drafts },
        } => Some(drafts),
        _ => None,
    })?;
    let draft = drafts
        .into_iter()
        .find(|draft| &draft.id == draft_id)
        .ok_or_else(|| anyhow::anyhow!("Draft not found: {draft_id}"))?;
    if account_id.is_some_and(|account_id| draft.account_id != *account_id) {
        anyhow::bail!("Draft {draft_id} belongs to a different account");
    }
    Ok(draft)
}

fn resolve_account_from_list(
    accounts: &[AccountSummaryData],
    selector: &str,
) -> anyhow::Result<AccountId> {
    let matches: Vec<_> = accounts
        .iter()
        .filter(|account| account_matches(account, selector))
        .collect();
    match matches.as_slice() {
        [account] => Ok(account.account_id.clone()),
        [] => anyhow::bail!("No account matching '{selector}'"),
        _ => anyhow::bail!("Account selector '{selector}' is ambiguous"),
    }
}

fn account_matches(account: &AccountSummaryData, selector: &str) -> bool {
    account.key.as_deref() == Some(selector)
        || account.email == selector
        || account.name == selector
        || account.account_id.to_string() == selector
}
