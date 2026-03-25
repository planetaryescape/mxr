use crate::ipc::{ipc_call, IpcRequest};
use mxr_core::{AccountId, MxrError};
use mxr_protocol::{
    AccountConfigData, AccountEditModeData, AccountSendConfigData, AccountSourceData,
    AccountSummaryData, AccountSyncConfigData, Request, Response, ResponseData,
};
use tokio::sync::mpsc;

pub(crate) async fn load_accounts_page_accounts(
    bg: &mpsc::UnboundedSender<IpcRequest>,
) -> Result<Vec<AccountSummaryData>, MxrError> {
    match ipc_call(bg, Request::ListAccounts).await {
        Ok(Response::Ok {
            data: ResponseData::Accounts { accounts },
        }) if !accounts.is_empty() => Ok(accounts),
        Ok(Response::Ok {
            data: ResponseData::Accounts { .. },
        })
        | Ok(Response::Error { .. })
        | Err(_) => load_config_account_summaries(bg).await,
        Ok(_) => Err(MxrError::Ipc("unexpected response".into())),
    }
}

async fn load_config_account_summaries(
    bg: &mpsc::UnboundedSender<IpcRequest>,
) -> Result<Vec<AccountSummaryData>, MxrError> {
    let resp = ipc_call(bg, Request::ListAccountsConfig).await?;
    match resp {
        Response::Ok {
            data: ResponseData::AccountsConfig { accounts },
        } => Ok(accounts
            .into_iter()
            .map(account_config_to_summary)
            .collect()),
        Response::Error { message } => Err(MxrError::Ipc(message)),
        _ => Err(MxrError::Ipc("unexpected response".into())),
    }
}

fn account_config_to_summary(account: AccountConfigData) -> AccountSummaryData {
    let provider_kind = account
        .sync
        .as_ref()
        .map(account_sync_kind_label)
        .or_else(|| account.send.as_ref().map(account_send_kind_label))
        .unwrap_or_else(|| "unknown".to_string());
    let account_id = AccountId::from_provider_id(&provider_kind, &account.email);

    AccountSummaryData {
        account_id,
        key: Some(account.key),
        name: account.name,
        email: account.email,
        provider_kind,
        sync_kind: account.sync.as_ref().map(account_sync_kind_label),
        send_kind: account.send.as_ref().map(account_send_kind_label),
        enabled: true,
        is_default: account.is_default,
        source: AccountSourceData::Config,
        editable: AccountEditModeData::Full,
        sync: account.sync,
        send: account.send,
    }
}

fn account_sync_kind_label(sync: &AccountSyncConfigData) -> String {
    match sync {
        AccountSyncConfigData::Gmail { .. } => "gmail".to_string(),
        AccountSyncConfigData::Imap { .. } => "imap".to_string(),
        AccountSyncConfigData::OutlookPersonal { .. } => "outlook".to_string(),
        AccountSyncConfigData::OutlookWork { .. } => "outlook-work".to_string(),
    }
}

fn account_send_kind_label(send: &AccountSendConfigData) -> String {
    match send {
        AccountSendConfigData::Gmail => "gmail".to_string(),
        AccountSendConfigData::Smtp { .. } => "smtp".to_string(),
        AccountSendConfigData::OutlookPersonal { .. } => "outlook".to_string(),
        AccountSendConfigData::OutlookWork { .. } => "outlook-work".to_string(),
    }
}
