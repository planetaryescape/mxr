use super::{
    account_operation_result, account_step, authorize_account_config, list_account_configs,
    list_runtime_accounts, set_default_account, test_account_config, upsert_account_config,
    HandlerResult,
};
use crate::mxr_protocol::{AccountConfigData, ResponseData};
use crate::state::AppState;
use std::sync::Arc;

pub(super) async fn list_accounts(state: &Arc<AppState>) -> HandlerResult {
    let accounts = list_runtime_accounts(state).await?;
    Ok(ResponseData::Accounts { accounts })
}

pub(super) fn list_accounts_config() -> HandlerResult {
    let accounts = list_account_configs()?;
    Ok(ResponseData::AccountsConfig { accounts })
}

pub(super) async fn authorize_account(
    account: AccountConfigData,
    reauthorize: bool,
) -> HandlerResult {
    Ok(ResponseData::AccountOperation {
        result: authorize_account_config(account, reauthorize).await,
    })
}

pub(super) async fn upsert_account(
    state: &Arc<AppState>,
    account: AccountConfigData,
) -> HandlerResult {
    Ok(ResponseData::AccountOperation {
        result: upsert_account_config(state, account).await,
    })
}

pub(super) async fn set_default_account_key(state: &Arc<AppState>, key: &str) -> HandlerResult {
    set_default_account(state, key).await?;
    Ok(ResponseData::AccountOperation {
        result: account_operation_result(
            true,
            format!("Default account set to '{key}'."),
            Some(account_step(
                true,
                format!("Default account set to '{key}'."),
            )),
            None,
            None,
            None,
        ),
    })
}

pub(super) async fn test_account(
    state: &Arc<AppState>,
    account: AccountConfigData,
) -> HandlerResult {
    let result = test_account_config(account).await;
    if result.ok {
        // Reload providers so a newly-authorized token gets picked up immediately
        let _ = state.reload_accounts_from_disk().await;
    }
    Ok(ResponseData::AccountOperation { result })
}
