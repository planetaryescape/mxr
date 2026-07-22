use crate::state::AppState;
use mxr_core::provider::MailSyncProvider;
use mxr_protocol::*;
use std::sync::Arc;

pub(super) async fn list_runtime_accounts(
    state: &AppState,
) -> Result<Vec<AccountSummaryData>, String> {
    use std::collections::BTreeMap;

    let config = state.config_snapshot();
    let default_config_key = config.general.default_account.clone();
    let runtime_ids = state.runtime_account_ids();
    let default_account_id = state.default_account_id_opt();
    let runtime_accounts = state
        .store
        .list_accounts()
        .await
        .map_err(|e| e.to_string())?;

    let mut accounts: BTreeMap<String, AccountSummaryData> = BTreeMap::new();

    for account in runtime_accounts
        .into_iter()
        .filter(|account| runtime_ids.iter().any(|id| id == &account.id))
    {
        let key = account
            .sync_backend
            .as_ref()
            .map(|backend| backend.config_key.clone())
            .or_else(|| {
                account
                    .send_backend
                    .as_ref()
                    .map(|backend| backend.config_key.clone())
            });
        let sync_kind = account
            .sync_backend
            .as_ref()
            .map(|backend| provider_kind_label(&backend.provider_kind).to_string());
        let send_kind = account
            .send_backend
            .as_ref()
            .map(|backend| provider_kind_label(&backend.provider_kind).to_string());
        let provider_kind = sync_kind
            .clone()
            .or_else(|| send_kind.clone())
            .unwrap_or_else(|| "unknown".to_string());
        let mut capabilities = state
            .get_provider(Some(&account.id))
            .map(|provider| AccountCapabilitiesData::from(provider.capabilities()))
            .unwrap_or_default();
        capabilities.supports_send = send_kind.is_some();
        let map_key = key.clone().unwrap_or_else(|| account.id.to_string());

        accounts.insert(
            map_key,
            AccountSummaryData {
                account_id: account.id.clone(),
                key,
                name: account.name,
                email: account.email,
                provider_kind,
                sync_kind,
                send_kind,
                enabled: account.enabled,
                is_default: default_account_id.as_ref() == Some(&account.id),
                source: AccountSourceData::Runtime,
                editable: AccountEditModeData::RuntimeOnly,
                sync: None,
                send: None,
                capabilities,
            },
        );
    }

    for (key, account) in config.accounts {
        let account_id = config_account_id(&key, &account);
        let summary = accounts
            .entry(key.clone())
            .or_insert_with(|| AccountSummaryData {
                account_id: account_id.clone(),
                key: Some(key.clone()),
                name: account.name.clone(),
                email: account.email.clone(),
                provider_kind: account_primary_provider_kind(&account),
                sync_kind: account.sync.as_ref().map(config_sync_kind_label),
                send_kind: account.send.as_ref().map(config_send_kind_label),
                enabled: account.enabled,
                is_default: false,
                source: AccountSourceData::Config,
                editable: AccountEditModeData::Full,
                sync: None,
                send: None,
                capabilities: config_account_capabilities(&account),
            });

        summary.account_id = account_id;
        summary.key = Some(key.clone());
        summary.name = account.name.clone();
        summary.email = account.email.clone();
        summary.provider_kind = account_primary_provider_kind(&account);
        summary.sync_kind = account.sync.as_ref().map(config_sync_kind_label);
        summary.send_kind = account.send.as_ref().map(config_send_kind_label);
        summary.enabled = account.enabled;
        summary.sync = account.sync.clone().map(sync_config_to_data);
        summary.send = account.send.clone().map(send_config_to_data);
        summary.is_default = default_config_key.as_deref() == Some(key.as_str());
        summary.source = match summary.source {
            AccountSourceData::Runtime => AccountSourceData::Both,
            _ => AccountSourceData::Config,
        };
        summary.editable = AccountEditModeData::Full;
        summary.capabilities = state.get_provider(Some(&summary.account_id)).map_or_else(
            |_| config_account_capabilities(&account),
            |provider| AccountCapabilitiesData::from(provider.capabilities()),
        );
        summary.capabilities.supports_send = summary.send_kind.is_some();
    }

    let mut accounts = accounts.into_values().collect::<Vec<_>>();
    accounts.sort_by(|left, right| {
        right
            .is_default
            .cmp(&left.is_default)
            .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
            .then_with(|| left.email.to_lowercase().cmp(&right.email.to_lowercase()))
    });
    Ok(accounts)
}

pub(super) fn list_account_configs() -> Result<Vec<AccountConfigData>, String> {
    let config = mxr_config::load_config().map_err(|e| e.to_string())?;
    let default_account = config.general.default_account.clone();
    let mut accounts = config
        .accounts
        .into_iter()
        .map(|(key, account)| AccountConfigData {
            is_default: default_account.as_deref() == Some(key.as_str()),
            key,
            name: account.name,
            email: account.email,
            enabled: account.enabled,
            sync: account.sync.map(sync_config_to_data),
            send: account.send.map(send_config_to_data),
        })
        .collect::<Vec<_>>();
    accounts.sort_by(|left, right| left.key.cmp(&right.key));
    Ok(accounts)
}

pub(super) async fn upsert_account_config(
    state: &Arc<AppState>,
    account: AccountConfigData,
) -> AccountOperationResult {
    let save_result = (|| -> Result<String, String> {
        let mut config = mxr_config::load_config().map_err(|e| e.to_string())?;
        persist_account_passwords(&account).map_err(|e| e.to_string())?;

        config.accounts.insert(
            account.key.clone(),
            mxr_config::AccountConfig {
                name: account.name.clone(),
                email: account.email.clone(),
                enabled: account.enabled,
                sync: account.sync.clone().map(sync_data_to_config).transpose()?,
                send: account.send.clone().map(send_data_to_config).transpose()?,
            },
        );
        if account.is_default || config.general.default_account.is_none() {
            config.general.default_account = Some(account.key.clone());
        }
        mxr_config::save_config(&config).map_err(|e| e.to_string())?;
        Ok(format!("Saved account '{}' to config.", account.key))
    })();

    match save_result {
        Ok(save_detail) => match state.reload_accounts_from_disk().await {
            Ok(()) => account_operation_result(
                true,
                format!("Saved account '{}' and reloaded runtime.", account.key),
                Some(account_step(
                    true,
                    format!("{save_detail} Runtime reloaded."),
                )),
                None,
                None,
                None,
            ),
            Err(error) => account_operation_result(
                false,
                format!(
                    "Saved account '{}' but failed to reload runtime.",
                    account.key
                ),
                Some(account_step(
                    false,
                    format!("{save_detail} Reload failed: {error}"),
                )),
                None,
                None,
                None,
            ),
        },
        Err(error) => account_operation_result(
            false,
            format!("Failed to save account '{}'.", account.key),
            Some(account_step(false, error)),
            None,
            None,
            None,
        ),
    }
}

pub(super) async fn set_default_account(
    state: &Arc<AppState>,
    key: &str,
) -> Result<String, String> {
    let mut config = mxr_config::load_config().map_err(|e| e.to_string())?;
    if !config.accounts.contains_key(key) {
        return Err(format!("Account '{key}' cannot be set as default"));
    }
    config.general.default_account = Some(key.to_string());
    mxr_config::save_config(&config).map_err(|e| e.to_string())?;
    state.reload_accounts_from_disk().await?;
    Ok(format!("Default account set to '{key}'."))
}

pub(super) async fn remove_account_config(
    state: &Arc<AppState>,
    key: &str,
    purge_local_data: bool,
    dry_run: bool,
) -> AccountOperationResult {
    let config = match mxr_config::load_config() {
        Ok(config) => config,
        Err(error) => {
            return account_operation_result(
                false,
                format!("Failed to remove account '{key}'."),
                Some(account_step(false, error.to_string())),
                None,
                None,
                None,
            )
        }
    };
    let Some(account) = config.accounts.get(key).cloned() else {
        return account_operation_result(
            false,
            format!("Account '{key}' not found."),
            Some(account_step(false, format!("Account '{key}' not found."))),
            None,
            None,
            None,
        );
    };

    let account_id = config_account_id(key, &account);
    let message_ids = match state.store.list_message_ids_by_account(&account_id).await {
        Ok(message_ids) => message_ids,
        Err(error) => {
            return account_operation_result(
                false,
                format!("Failed to inspect cached mail for account '{key}'."),
                Some(account_step(false, error.to_string())),
                None,
                None,
                None,
            )
        }
    };
    let cached_message_count = message_ids.len();
    let cache_action = if purge_local_data { "purge" } else { "detach" };

    if dry_run {
        return account_operation_result(
            true,
            format!(
                "Would remove account '{key}' from config and {cache_action} {cached_message_count} cached message(s)."
            ),
            Some(account_step(true, "Dry run only; no changes made.".into())),
            None,
            None,
            None,
        );
    }

    let save_result = (|| -> Result<(), String> {
        let mut config = config;
        config.accounts.remove(key);
        refresh_default_account(&mut config);
        mxr_config::save_config(&config).map_err(|e| e.to_string())?;
        Ok(())
    })();
    if let Err(error) = save_result {
        return account_operation_result(
            false,
            format!("Failed to remove account '{key}'."),
            Some(account_step(false, error)),
            None,
            None,
            None,
        );
    }

    let local_result = if purge_local_data {
        match state
            .search
            .apply_batch(mxr_search::SearchUpdateBatch {
                entries: Vec::new(),
                removed_message_ids: message_ids,
            })
            .await
            .map_err(|e| e.to_string())
        {
            Ok(()) => state
                .store
                .delete_account(&account_id)
                .await
                .map(|_| ())
                .map_err(|e| e.to_string()),
            Err(error) => Err(error),
        }
    } else {
        state
            .store
            .set_account_enabled(&account_id, false)
            .await
            .map_err(|e| e.to_string())
    };
    if let Err(error) = local_result {
        return account_operation_result(
            false,
            format!("Removed account '{key}' from config but failed to update cached mail."),
            Some(account_step(false, error)),
            None,
            None,
            None,
        );
    }

    match state.reload_accounts_from_disk().await {
        Ok(()) => account_operation_result(
            true,
            if purge_local_data {
                format!(
                    "Removed account '{key}' and purged {cached_message_count} cached message(s)."
                )
            } else {
                format!("Removed account '{key}' from config; cached mail detached.")
            },
            Some(account_step(
                true,
                "Config saved and daemon runtime reloaded.".into(),
            )),
            None,
            None,
            None,
        ),
        Err(error) => account_operation_result(
            false,
            format!("Removed account '{key}' but failed to reload runtime."),
            Some(account_step(false, error)),
            None,
            None,
            None,
        ),
    }
}

pub(super) async fn disable_account_config(
    state: &Arc<AppState>,
    key: &str,
) -> AccountOperationResult {
    let mut config = match mxr_config::load_config() {
        Ok(config) => config,
        Err(error) => {
            return account_operation_result(
                false,
                format!("Failed to disable account '{key}'."),
                Some(account_step(false, error.to_string())),
                None,
                None,
                None,
            )
        }
    };
    let Some(account) = config.accounts.get_mut(key) else {
        return account_operation_result(
            false,
            format!("Account '{key}' not found."),
            Some(account_step(false, format!("Account '{key}' not found."))),
            None,
            None,
            None,
        );
    };

    account.enabled = false;
    let account_id = config_account_id(key, account);
    refresh_default_account(&mut config);
    if let Err(error) = mxr_config::save_config(&config) {
        return account_operation_result(
            false,
            format!("Failed to disable account '{key}'."),
            Some(account_step(false, error.to_string())),
            None,
            None,
            None,
        );
    }
    if let Err(error) = state.store.set_account_enabled(&account_id, false).await {
        return account_operation_result(
            false,
            format!("Disabled account '{key}' in config but failed to update cached mail."),
            Some(account_step(false, error.to_string())),
            None,
            None,
            None,
        );
    }

    match state.reload_accounts_from_disk().await {
        Ok(()) => account_operation_result(
            true,
            format!("Disabled account '{key}'."),
            Some(account_step(
                true,
                "Config saved and daemon runtime reloaded.".into(),
            )),
            None,
            None,
            None,
        ),
        Err(error) => account_operation_result(
            false,
            format!("Disabled account '{key}' but failed to reload runtime."),
            Some(account_step(false, error)),
            None,
            None,
            None,
        ),
    }
}

pub(super) fn repair_account_config(account: AccountConfigData) -> AccountOperationResult {
    match repair_account_passwords(&account) {
        Ok(count) => account_operation_result(
            true,
            format!("Repaired credentials for '{}'.", account.key),
            Some(account_step(
                true,
                format!("Stored {count} password-backed credential(s) on disk (secrets.toml, mode 0600)."),
            )),
            None,
            None,
            None,
        ),
        Err(error) => {
            let detail = error.to_string();
            let summary = if detail.contains("no password-backed") {
                format!(
                    "Account '{}' has no password-backed credentials to repair.",
                    account.key
                )
            } else {
                format!("Failed to repair credentials for '{}'.", account.key)
            };
            account_operation_result(
                false,
                summary,
                Some(account_step(false, detail)),
                None,
                None,
                None,
            )
        }
    }
}

fn refresh_default_account(config: &mut mxr_config::MxrConfig) {
    let current_default_is_enabled = config
        .general
        .default_account
        .as_ref()
        .and_then(|key| config.accounts.get(key))
        .is_some_and(|account| account.enabled);
    if current_default_is_enabled {
        return;
    }

    config.general.default_account = config
        .accounts
        .iter()
        .filter(|(_, account)| account.enabled)
        .map(|(key, _)| key.clone())
        .min();
}

pub(super) async fn authorize_account_config(
    account: AccountConfigData,
    reauthorize: bool,
) -> AccountOperationResult {
    // Outlook device-code flow — check sync first, fall back to send for send-only accounts
    let outlook_tenant = match &account.sync {
        Some(AccountSyncConfigData::OutlookPersonal { .. }) => {
            Some(mxr_provider_outlook::OutlookTenant::Personal)
        }
        Some(AccountSyncConfigData::OutlookWork { .. }) => {
            Some(mxr_provider_outlook::OutlookTenant::Work)
        }
        _ => match &account.send {
            Some(AccountSendConfigData::OutlookPersonal { .. }) => {
                Some(mxr_provider_outlook::OutlookTenant::Personal)
            }
            Some(AccountSendConfigData::OutlookWork { .. }) => {
                Some(mxr_provider_outlook::OutlookTenant::Work)
            }
            _ => None,
        },
    };
    if let Some(tenant) = outlook_tenant {
        let (client_id, token_ref) = match &account.sync {
            Some(
                AccountSyncConfigData::OutlookPersonal {
                    client_id,
                    token_ref,
                }
                | AccountSyncConfigData::OutlookWork {
                    client_id,
                    token_ref,
                },
            ) => (client_id.clone(), token_ref.clone()),
            _ => match &account.send {
                Some(
                    AccountSendConfigData::OutlookPersonal {
                        client_id,
                        token_ref,
                    }
                    | AccountSendConfigData::OutlookWork {
                        client_id,
                        token_ref,
                    },
                ) => (client_id.clone(), token_ref.clone()),
                _ => unreachable!(),
            },
        };
        let cid = client_id
            .or_else(|| mxr_provider_outlook::OutlookAuth::bundled_client_id().map(String::from))
            .unwrap_or_default();
        if cid.is_empty() {
            return account_operation_result(
                false,
                "Outlook authorization requires a client ID.".into(),
                None,
                Some(account_step(
                    false,
                    "No bundled client ID and none provided. Add client_id to account config."
                        .into(),
                )),
                None,
                None,
            );
        }
        let auth = crate::provider_credentials::outlook_auth(cid, token_ref, tenant);
        if !reauthorize && auth.get_valid_access_token().await.is_ok() {
            return account_operation_result(
                true,
                "Outlook authorization ready.".into(),
                None,
                Some(account_step(true, "Existing OAuth token valid.".into())),
                None,
                None,
            );
        }
        let device_resp = match auth.start_device_flow().await {
            Ok(r) => r,
            Err(e) => {
                return account_operation_result(
                    false,
                    "Outlook authorization failed.".into(),
                    None,
                    Some(account_step(false, e.to_string())),
                    None,
                    None,
                );
            }
        };
        let device_code_url = device_resp
            .verification_uri_complete
            .clone()
            .unwrap_or_else(|| device_resp.verification_uri.clone());
        let device_code_user_code = device_resp.user_code.clone();
        let _ = open::that(&device_code_url);
        tracing::info!(
            user_code = %device_resp.user_code,
            url = %device_code_url,
            "Outlook device code flow started — user must enter code in browser"
        );
        return match auth
            .poll_for_token(&device_resp.device_code, device_resp.interval)
            .await
        {
            Ok(tokens) => {
                if let Err(e) = auth.save_tokens(&tokens) {
                    account_operation_result(
                        false,
                        "Outlook authorization failed.".into(),
                        None,
                        Some(account_step(false, format!("Token save failed: {e}"))),
                        None,
                        None,
                    )
                } else {
                    AccountOperationResult {
                        ok: true,
                        summary: "Outlook authorization complete.".into(),
                        save: None,
                        auth: Some(account_step(true, "Token stored successfully.".into())),
                        sync: None,
                        send: None,
                        device_code_url: Some(device_code_url),
                        device_code_user_code: Some(device_code_user_code),
                    }
                }
            }
            Err(e) => account_operation_result(
                false,
                "Outlook authorization failed.".into(),
                None,
                Some(account_step(false, e.to_string())),
                None,
                None,
            ),
        };
    }

    let Some(AccountSyncConfigData::Gmail {
        credential_source,
        client_id,
        client_secret,
        token_ref,
    }) = account.sync
    else {
        return account_operation_result(
            false,
            "Authorization is only available for Gmail and Outlook accounts.".into(),
            None,
            Some(account_step(
                false,
                "Selected account does not use Gmail or Outlook sync.".into(),
            )),
            None,
            None,
        );
    };

    let (client_id, client_secret) =
        match resolve_gmail_credentials(credential_source, client_id, client_secret) {
            Ok(creds) => creds,
            Err(error) => {
                return account_operation_result(
                    false,
                    "Gmail authorization unavailable.".into(),
                    None,
                    Some(account_step(false, error)),
                    None,
                    None,
                )
            }
        };

    let mut auth = crate::provider_credentials::gmail_auth(client_id, client_secret, token_ref);
    let auth_result = if reauthorize {
        auth.interactive_auth().await
    } else {
        match auth.load_existing().await {
            Ok(()) => Ok(()),
            Err(_) => auth.interactive_auth().await,
        }
    };

    match auth_result {
        Ok(()) => account_operation_result(
            true,
            if reauthorize {
                "Gmail authorization refreshed.".into()
            } else {
                "Gmail authorization ready.".into()
            },
            None,
            Some(account_step(
                true,
                if reauthorize {
                    "Browser authorization completed and token stored.".into()
                } else {
                    "OAuth token is available for this Gmail account.".into()
                },
            )),
            None,
            None,
        ),
        Err(error) => account_operation_result(
            false,
            "Gmail authorization failed.".into(),
            None,
            Some(account_step(
                false,
                friendly_gmail_auth_error(&error.to_string()),
            )),
            None,
            None,
        ),
    }
}

pub(super) async fn test_account_config(account: AccountConfigData) -> AccountOperationResult {
    if let Err(error) = persist_account_passwords(&account) {
        return account_operation_result(
            false,
            "Failed to persist account secrets before testing.".into(),
            None,
            Some(account_step(false, error.to_string())),
            None,
            None,
        );
    }

    let mut auth = None;
    let mut sync = None;
    let mut send = None;
    let mut ok = true;

    if let Some(sync_config) = account.sync.clone() {
        match sync_config {
            AccountSyncConfigData::Gmail {
                credential_source,
                client_id,
                client_secret,
                token_ref,
            } => {
                let creds = resolve_gmail_credentials(credential_source, client_id, client_secret);
                match creds {
                    Ok((client_id, client_secret)) => {
                        let mut gmail_auth = crate::provider_credentials::gmail_auth(
                            client_id,
                            client_secret,
                            token_ref,
                        );
                        let auth_result = match gmail_auth.load_existing().await {
                            Ok(()) => Ok("Existing OAuth token loaded.".to_string()),
                            Err(_) => gmail_auth.interactive_auth().await.map(|_| {
                                "Browser authorization completed and token stored.".to_string()
                            }),
                        };
                        match auth_result {
                            Ok(detail) => {
                                auth = Some(account_step(true, detail));
                                let client =
                                    mxr_provider_gmail::client::GmailClient::new(gmail_auth);
                                match client.list_labels().await {
                                    Ok(response) => {
                                        let count =
                                            response.labels.map_or(0, |labels| labels.len());
                                        sync = Some(account_step(
                                            true,
                                            format!("Gmail sync ok: {count} labels"),
                                        ));
                                    }
                                    Err(error) => {
                                        ok = false;
                                        sync = Some(account_step(false, error.to_string()));
                                    }
                                }
                            }
                            Err(error) => {
                                ok = false;
                                auth = Some(account_step(
                                    false,
                                    friendly_gmail_auth_error(&error.to_string()),
                                ));
                                sync = Some(account_step(
                                    false,
                                    "Skipped Gmail sync because authorization failed.".into(),
                                ));
                            }
                        }
                    }
                    Err(error) => {
                        ok = false;
                        auth = Some(account_step(false, error));
                        sync = Some(account_step(
                            false,
                            "Skipped Gmail sync because OAuth credentials are unavailable.".into(),
                        ));
                    }
                }
            }
            AccountSyncConfigData::Imap {
                host,
                port,
                username,
                password_ref,
                auth_required,
                use_tls,
                ..
            } => {
                match crate::provider_credentials::imap_config_with_credentials(
                    host,
                    port,
                    username,
                    password_ref,
                    auth_required,
                    use_tls,
                ) {
                    Ok(config) => {
                        let provider = mxr_provider_imap::ImapProvider::new(
                            mxr_core::AccountId::from_provider_id("imap", &account.email),
                            config,
                        );
                        match provider.sync_labels().await {
                            Ok(folders) => {
                                sync = Some(account_step(
                                    true,
                                    format!("IMAP sync ok: {} folders", folders.len()),
                                ));
                            }
                            Err(error) => {
                                ok = false;
                                sync = Some(account_step(false, error.to_string()));
                            }
                        }
                    }
                    Err(error) => {
                        ok = false;
                        sync = Some(account_step(false, error.to_string()));
                    }
                }
            }
            AccountSyncConfigData::OutlookPersonal {
                client_id,
                token_ref,
            }
            | AccountSyncConfigData::OutlookWork {
                client_id,
                token_ref,
            } => {
                let tenant = match &account.sync {
                    Some(AccountSyncConfigData::OutlookWork { .. }) => {
                        mxr_provider_outlook::OutlookTenant::Work
                    }
                    _ => mxr_provider_outlook::OutlookTenant::Personal,
                };
                let cid =
                    client_id.or_else(|| mxr_provider_outlook::BUNDLED_CLIENT_ID.map(String::from));
                match cid {
                    None => {
                        ok = false;
                        sync = Some(account_step(
                            false,
                            "No client_id and no bundled OUTLOOK_CLIENT_ID".into(),
                        ));
                    }
                    Some(cid) => {
                        let auth_inst = std::sync::Arc::new(
                            crate::provider_credentials::outlook_auth(cid, token_ref, tenant),
                        );
                        let email = account.email.clone();
                        let token_fn: std::sync::Arc<
                            dyn Fn() -> futures::future::BoxFuture<'static, anyhow::Result<String>>
                                + Send
                                + Sync,
                        > = std::sync::Arc::new(move || {
                            let a = auth_inst.clone();
                            Box::pin(async move {
                                a.get_valid_access_token()
                                    .await
                                    .map_err(|e| anyhow::anyhow!(e))
                            })
                        });
                        let factory = mxr_provider_imap::XOAuth2ImapSessionFactory::new(
                            "outlook.office365.com".to_string(),
                            993,
                            email.clone(),
                            token_fn,
                        );
                        let provider = mxr_provider_imap::ImapProvider::with_session_factory(
                            mxr_core::AccountId::from_provider_id("outlook", &email),
                            mxr_provider_imap::config::ImapConfig::new(
                                "outlook.office365.com".to_string(),
                                993,
                                email,
                                String::new(),
                                true,
                                true,
                            ),
                            Box::new(factory),
                        );
                        match provider.sync_labels().await {
                            Ok(folders) => {
                                sync = Some(account_step(
                                    true,
                                    format!("Outlook IMAP ok: {} folders", folders.len()),
                                ));
                            }
                            Err(error) => {
                                ok = false;
                                sync = Some(account_step(false, error.to_string()));
                            }
                        }
                    }
                }
            }
            AccountSyncConfigData::Fake => {
                sync = Some(account_step(true, "Fake sync provider (test-only)".into()));
            }
        }
    }

    match account.send {
        Some(AccountSendConfigData::Gmail) => {
            // Gmail send uses the same OAuth token as Gmail sync. Only claim
            // send is reachable if Gmail sync auth actually succeeded — the
            // old behavior reported "configured" unconditionally and masked
            // real auth failures.
            let gmail_sync_auth_ok = matches!(
                (&account.sync, auth.as_ref()),
                (Some(AccountSyncConfigData::Gmail { .. }), Some(step)) if step.ok
            );
            let has_gmail_sync = matches!(account.sync, Some(AccountSyncConfigData::Gmail { .. }));
            if gmail_sync_auth_ok {
                send = Some(account_step(
                    true,
                    "Gmail send reachable (shares OAuth credentials with sync).".into(),
                ));
            } else if has_gmail_sync {
                ok = false;
                send = Some(account_step(
                    false,
                    "Gmail send unavailable because Gmail authorization failed.".into(),
                ));
            } else {
                ok = false;
                send = Some(account_step(
                    false,
                    "Gmail send requires a Gmail sync configuration to provide OAuth credentials."
                        .into(),
                ));
            }
        }
        Some(
            send_cfg @ (AccountSendConfigData::OutlookPersonal { .. }
            | AccountSendConfigData::OutlookWork { .. }),
        ) => {
            let (token_ref, send_client_id, tenant) = match send_cfg {
                AccountSendConfigData::OutlookPersonal {
                    token_ref,
                    client_id,
                } => (
                    token_ref,
                    client_id,
                    mxr_provider_outlook::OutlookTenant::Personal,
                ),
                AccountSendConfigData::OutlookWork {
                    token_ref,
                    client_id,
                } => (
                    token_ref,
                    client_id,
                    mxr_provider_outlook::OutlookTenant::Work,
                ),
                _ => unreachable!(),
            };
            let cid = send_client_id
                .or_else(|| match &account.sync {
                    Some(
                        AccountSyncConfigData::OutlookPersonal {
                            client_id: Some(id),
                            ..
                        }
                        | AccountSyncConfigData::OutlookWork {
                            client_id: Some(id),
                            ..
                        },
                    ) => Some(id.clone()),
                    _ => None,
                })
                .or_else(|| mxr_provider_outlook::BUNDLED_CLIENT_ID.map(String::from));
            match cid {
                None => {
                    ok = false;
                    send = Some(account_step(
                        false,
                        "No client_id and no bundled OUTLOOK_CLIENT_ID for Outlook send".into(),
                    ));
                }
                Some(cid) => {
                    let auth_inst = std::sync::Arc::new(crate::provider_credentials::outlook_auth(
                        cid, token_ref, tenant,
                    ));
                    let email = account.email.clone();
                    let token_fn: std::sync::Arc<
                        dyn Fn() -> futures::future::BoxFuture<'static, anyhow::Result<String>>
                            + Send
                            + Sync,
                    > = std::sync::Arc::new(move || {
                        let a = auth_inst.clone();
                        Box::pin(async move {
                            a.get_valid_access_token()
                                .await
                                .map_err(|e| anyhow::anyhow!(e))
                        })
                    });
                    let smtp_host = match tenant {
                        mxr_provider_outlook::OutlookTenant::Personal => "smtp-mail.outlook.com",
                        mxr_provider_outlook::OutlookTenant::Work => "smtp.office365.com",
                    };
                    let provider = mxr_provider_outlook::OutlookSmtpSendProvider::new(
                        smtp_host.to_string(),
                        587,
                        email,
                        token_fn,
                    );
                    match provider.test_connection().await {
                        Ok(()) => {
                            send = Some(account_step(true, "Outlook SMTP ok".into()));
                        }
                        Err(error) => {
                            ok = false;
                            send = Some(account_step(false, error));
                        }
                    }
                }
            }
        }
        Some(AccountSendConfigData::Fake) => {
            send = Some(account_step(true, "Fake send provider (test-only)".into()));
        }
        Some(AccountSendConfigData::Smtp {
            host,
            port,
            username,
            password_ref,
            auth_required,
            use_tls,
            ..
        }) => {
            let config = match crate::provider_credentials::smtp_config_with_credentials(
                host,
                port,
                username,
                password_ref,
                auth_required,
                use_tls,
            ) {
                Ok(config) => config,
                Err(error) => {
                    ok = false;
                    send = Some(account_step(false, error.to_string()));
                    return account_operation_result(
                        ok,
                        format!("Account '{}' test failed.", account.key),
                        None,
                        auth,
                        sync,
                        send,
                    );
                }
            };
            let provider = mxr_provider_smtp::SmtpSendProvider::new(config);
            match provider.test_connection().await {
                Ok(()) => {
                    send = Some(account_step(true, "SMTP send ok".into()));
                }
                Err(error) => {
                    ok = false;
                    send = Some(account_step(false, error.to_string()));
                }
            }
        }
        None if account.sync.is_none() => {
            ok = false;
            send = Some(account_step(
                false,
                "No sync or send configuration provided.".into(),
            ));
        }
        None => {}
    }

    account_operation_result(
        ok,
        if ok {
            format!("Account '{}' test passed.", account.key)
        } else {
            format!("Account '{}' test failed.", account.key)
        },
        None,
        auth,
        sync,
        send,
    )
}

pub(super) fn account_step(ok: bool, detail: String) -> AccountOperationStep {
    AccountOperationStep { ok, detail }
}

pub(super) fn account_operation_result(
    ok: bool,
    summary: String,
    save: Option<AccountOperationStep>,
    auth: Option<AccountOperationStep>,
    sync: Option<AccountOperationStep>,
    send: Option<AccountOperationStep>,
) -> AccountOperationResult {
    AccountOperationResult {
        ok,
        summary,
        save,
        auth,
        sync,
        send,
        device_code_url: None,
        device_code_user_code: None,
    }
}

pub(super) fn resolve_gmail_credentials(
    credential_source: GmailCredentialSourceData,
    client_id: String,
    client_secret: Option<String>,
) -> Result<(String, String), String> {
    match credential_source {
        GmailCredentialSourceData::Bundled => {
            match (
                mxr_provider_gmail::auth::BUNDLED_CLIENT_ID,
                mxr_provider_gmail::auth::BUNDLED_CLIENT_SECRET,
            ) {
                (Some(id), Some(secret)) => Ok((id.to_string(), secret.to_string())),
                _ => {
                    if client_id.trim().is_empty()
                        || client_secret.as_deref().unwrap_or("").trim().is_empty()
                    {
                        Err("This mxr build does not include one-click Gmail OAuth credentials. Install an official release build, run `mxr demo`, or switch Gmail Credential source to Custom and enter your own Google OAuth client ID/client secret.".into())
                    } else {
                        Ok((client_id, client_secret.unwrap_or_default()))
                    }
                }
            }
        }
        GmailCredentialSourceData::Custom => {
            if client_id.trim().is_empty()
                || client_secret.as_deref().unwrap_or("").trim().is_empty()
            {
                Err("Custom Gmail OAuth requires both client ID and client secret.".into())
            } else {
                Ok((client_id, client_secret.unwrap_or_default()))
            }
        }
    }
}

/// Rewrite raw Gmail OAuth errors into actionable guidance for onboarding UI.
/// Google's response strings are precise but cryptic; users need to know what
/// to change in Google Cloud Console.
pub(super) fn friendly_gmail_auth_error(raw: &str) -> String {
    let lower = raw.to_lowercase();
    if lower.contains("invalid_client") {
        return format!(
            "{raw}\n\nGoogle rejected the OAuth client. Most likely the client in Google Cloud Console is not type \"Desktop app\". mxr uses the installed-application flow with an http://localhost redirect, which only works with Desktop app credentials.\n\nFix:\n  1. Open https://console.cloud.google.com/apis/credentials\n  2. Find the OAuth 2.0 Client ID being used (check the client_id reported in this build).\n  3. If its Application type is not \"Desktop app\", create a new one of type \"Desktop app\".\n  4. Re-run Gmail onboarding with the new credentials, or rebuild mxr with updated GMAIL_CLIENT_ID/GMAIL_CLIENT_SECRET.\n\nSee https://mxr-mail.vercel.app/getting-started/gmail-setup/ for full instructions."
        );
    }
    if lower.contains("access_denied") {
        return format!(
            "{raw}\n\nYou (or Google) denied the consent screen. If the OAuth app is in \"Testing\" mode, make sure your Google account is added under \"Test users\" in the OAuth consent screen settings."
        );
    }
    if lower.contains("invalid_scope") {
        return format!(
            "{raw}\n\nGoogle rejected the requested scopes. Make sure the Gmail API is enabled for the project in https://console.cloud.google.com/apis/library/gmail.googleapis.com and the OAuth consent screen lists the gmail.readonly, gmail.modify, and gmail.labels scopes."
        );
    }
    raw.to_string()
}

fn sync_config_to_data(sync: mxr_config::SyncProviderConfig) -> AccountSyncConfigData {
    match sync {
        mxr_config::SyncProviderConfig::Gmail {
            credential_source,
            client_id,
            client_secret,
            token_ref,
        } => AccountSyncConfigData::Gmail {
            credential_source: match credential_source {
                mxr_config::GmailCredentialSource::Bundled => GmailCredentialSourceData::Bundled,
                mxr_config::GmailCredentialSource::Custom => GmailCredentialSourceData::Custom,
            },
            client_id,
            client_secret,
            token_ref,
        },
        mxr_config::SyncProviderConfig::Imap {
            host,
            port,
            username,
            password_ref,
            auth_required,
            use_tls,
        } => AccountSyncConfigData::Imap {
            host,
            port,
            username,
            password_ref,
            password: None,
            auth_required,
            use_tls,
        },
        mxr_config::SyncProviderConfig::OutlookPersonal {
            client_id,
            token_ref,
        } => AccountSyncConfigData::OutlookPersonal {
            client_id,
            token_ref,
        },
        mxr_config::SyncProviderConfig::OutlookWork {
            client_id,
            token_ref,
        } => AccountSyncConfigData::OutlookWork {
            client_id,
            token_ref,
        },
        mxr_config::SyncProviderConfig::Fake => AccountSyncConfigData::Fake,
    }
}

fn config_account_id(key: &str, account: &mxr_config::AccountConfig) -> mxr_core::AccountId {
    let kind = account
        .sync
        .as_ref()
        .map(config_sync_kind_label)
        .or_else(|| account.send.as_ref().map(config_send_kind_label))
        .unwrap_or_else(|| key.to_string());
    mxr_core::AccountId::from_provider_id(&kind, &account.email)
}

fn config_sync_kind_label(sync: &mxr_config::SyncProviderConfig) -> String {
    match sync {
        mxr_config::SyncProviderConfig::Gmail { .. } => "gmail".into(),
        mxr_config::SyncProviderConfig::Imap { .. } => "imap".into(),
        mxr_config::SyncProviderConfig::OutlookPersonal { .. } => "outlook".into(),
        mxr_config::SyncProviderConfig::OutlookWork { .. } => "outlook-work".into(),
        mxr_config::SyncProviderConfig::Fake => "fake".into(),
    }
}

fn config_send_kind_label(send: &mxr_config::SendProviderConfig) -> String {
    match send {
        mxr_config::SendProviderConfig::Gmail => "gmail".into(),
        mxr_config::SendProviderConfig::Smtp { .. } => "smtp".into(),
        mxr_config::SendProviderConfig::OutlookPersonal { .. } => "outlook".into(),
        mxr_config::SendProviderConfig::OutlookWork { .. } => "outlook-work".into(),
        mxr_config::SendProviderConfig::Fake => "fake".into(),
    }
}

fn account_primary_provider_kind(account: &mxr_config::AccountConfig) -> String {
    account
        .sync
        .as_ref()
        .map(config_sync_kind_label)
        .or_else(|| account.send.as_ref().map(config_send_kind_label))
        .unwrap_or_else(|| "unknown".into())
}

fn config_account_capabilities(account: &mxr_config::AccountConfig) -> AccountCapabilitiesData {
    let mut capabilities = account
        .sync
        .as_ref()
        .map(config_sync_capabilities)
        .unwrap_or_default();
    capabilities.supports_send = account.send.is_some();
    capabilities
}

fn config_sync_capabilities(sync: &mxr_config::SyncProviderConfig) -> AccountCapabilitiesData {
    match sync {
        mxr_config::SyncProviderConfig::Gmail { .. } => AccountCapabilitiesData {
            labels: true,
            server_search: true,
            delta_sync: true,
            batch_operations: true,
            native_thread_ids: true,
            ..AccountCapabilitiesData::default()
        },
        mxr_config::SyncProviderConfig::Imap { .. } => AccountCapabilitiesData {
            server_search: true,
            delta_sync: true,
            push: true,
            ..AccountCapabilitiesData::default()
        },
        mxr_config::SyncProviderConfig::Fake => AccountCapabilitiesData {
            labels: true,
            native_thread_ids: true,
            ..AccountCapabilitiesData::default()
        },
        mxr_config::SyncProviderConfig::OutlookPersonal { .. }
        | mxr_config::SyncProviderConfig::OutlookWork { .. } => AccountCapabilitiesData::default(),
    }
}

fn provider_kind_label(kind: &mxr_core::ProviderKind) -> &'static str {
    match kind {
        mxr_core::ProviderKind::Gmail => "gmail",
        mxr_core::ProviderKind::Imap => "imap",
        mxr_core::ProviderKind::Smtp => "smtp",
        mxr_core::ProviderKind::OutlookPersonal => "outlook-personal",
        mxr_core::ProviderKind::OutlookWork => "outlook-work",
        mxr_core::ProviderKind::Fake => "fake",
    }
}

fn send_config_to_data(send: mxr_config::SendProviderConfig) -> AccountSendConfigData {
    match send {
        mxr_config::SendProviderConfig::Gmail => AccountSendConfigData::Gmail,
        mxr_config::SendProviderConfig::OutlookPersonal {
            client_id,
            token_ref,
        } => AccountSendConfigData::OutlookPersonal {
            client_id,
            token_ref,
        },
        mxr_config::SendProviderConfig::OutlookWork {
            client_id,
            token_ref,
        } => AccountSendConfigData::OutlookWork {
            client_id,
            token_ref,
        },
        mxr_config::SendProviderConfig::Smtp {
            host,
            port,
            username,
            password_ref,
            auth_required,
            use_tls,
        } => AccountSendConfigData::Smtp {
            host,
            port,
            username,
            password_ref,
            password: None,
            auth_required,
            use_tls,
        },
        mxr_config::SendProviderConfig::Fake => AccountSendConfigData::Fake,
    }
}

fn sync_data_to_config(
    data: AccountSyncConfigData,
) -> Result<mxr_config::SyncProviderConfig, String> {
    match data {
        AccountSyncConfigData::Gmail {
            credential_source,
            client_id,
            client_secret,
            token_ref,
        } => Ok(mxr_config::SyncProviderConfig::Gmail {
            credential_source: match credential_source {
                GmailCredentialSourceData::Bundled => mxr_config::GmailCredentialSource::Bundled,
                GmailCredentialSourceData::Custom => mxr_config::GmailCredentialSource::Custom,
            },
            client_id,
            client_secret,
            token_ref,
        }),
        AccountSyncConfigData::Imap {
            host,
            port,
            username,
            password_ref,
            auth_required,
            use_tls,
            ..
        } => Ok(mxr_config::SyncProviderConfig::Imap {
            host,
            port,
            username,
            password_ref,
            auth_required,
            use_tls,
        }),
        AccountSyncConfigData::OutlookPersonal {
            client_id,
            token_ref,
        } => Ok(mxr_config::SyncProviderConfig::OutlookPersonal {
            client_id,
            token_ref,
        }),
        AccountSyncConfigData::OutlookWork {
            client_id,
            token_ref,
        } => Ok(mxr_config::SyncProviderConfig::OutlookWork {
            client_id,
            token_ref,
        }),
        AccountSyncConfigData::Fake => Ok(mxr_config::SyncProviderConfig::Fake),
    }
}

fn send_data_to_config(
    data: AccountSendConfigData,
) -> Result<mxr_config::SendProviderConfig, String> {
    match data {
        AccountSendConfigData::Gmail => Ok(mxr_config::SendProviderConfig::Gmail),
        AccountSendConfigData::OutlookPersonal {
            client_id,
            token_ref,
        } => Ok(mxr_config::SendProviderConfig::OutlookPersonal {
            client_id,
            token_ref,
        }),
        AccountSendConfigData::OutlookWork {
            client_id,
            token_ref,
        } => Ok(mxr_config::SendProviderConfig::OutlookWork {
            client_id,
            token_ref,
        }),
        AccountSendConfigData::Fake => Ok(mxr_config::SendProviderConfig::Fake),
        AccountSendConfigData::Smtp {
            host,
            port,
            username,
            password_ref,
            auth_required,
            use_tls,
            ..
        } => Ok(mxr_config::SendProviderConfig::Smtp {
            host,
            port,
            username,
            password_ref,
            auth_required,
            use_tls,
        }),
    }
}

fn persist_account_passwords(account: &AccountConfigData) -> anyhow::Result<()> {
    tracing::debug!(
        account_key = %account.key,
        sync_kind = %match account.sync {
            Some(AccountSyncConfigData::Gmail { .. }) => "gmail",
            Some(AccountSyncConfigData::Imap { .. }) => "imap",
            Some(AccountSyncConfigData::OutlookPersonal { .. }) => "outlook",
            Some(AccountSyncConfigData::OutlookWork { .. }) => "outlook-work",
            Some(AccountSyncConfigData::Fake) => "fake",
            None => "none",
        },
        send_kind = %match account.send {
            Some(AccountSendConfigData::Gmail) => "gmail",
            Some(AccountSendConfigData::Smtp { .. }) => "smtp",
            Some(AccountSendConfigData::OutlookPersonal { .. }) => "outlook",
            Some(AccountSendConfigData::OutlookWork { .. }) => "outlook-work",
            Some(AccountSendConfigData::Fake) => "fake",
            None => "none",
        },
        has_inline_imap_password = matches!(
            account.sync,
            Some(AccountSyncConfigData::Imap {
                password: Some(ref password),
                ..
            }) if !password.is_empty()
        ),
        has_inline_smtp_password = matches!(
            account.send,
            Some(AccountSendConfigData::Smtp {
                password: Some(ref password),
                ..
            }) if !password.is_empty()
        ),
        "persisting inline account credentials if supplied"
    );

    if let Some(AccountSyncConfigData::Imap {
        auth_required,
        username,
        password_ref,
        password: Some(password),
        ..
    }) = &account.sync
    {
        persist_account_password("IMAP", *auth_required, username, password_ref, password)?;
    }

    if let Some(AccountSendConfigData::Smtp {
        auth_required,
        username,
        password_ref,
        password: Some(password),
        ..
    }) = &account.send
    {
        persist_account_password("SMTP", *auth_required, username, password_ref, password)?;
    }

    Ok(())
}

fn repair_account_passwords(account: &AccountConfigData) -> anyhow::Result<usize> {
    let mut repaired = 0usize;
    let mut repairable = 0usize;

    if let Some(AccountSyncConfigData::Imap {
        auth_required,
        username,
        password_ref,
        password,
        ..
    }) = &account.sync
    {
        if *auth_required {
            repairable += 1;
            let password = password
                .as_deref()
                .filter(|password| !password.is_empty())
                .ok_or_else(|| {
                    anyhow::anyhow!("IMAP password is required to repair this account.")
                })?;
            persist_account_password("IMAP", true, username, password_ref, password)?;
            repaired += 1;
        }
    }

    if let Some(AccountSendConfigData::Smtp {
        auth_required,
        username,
        password_ref,
        password,
        ..
    }) = &account.send
    {
        if *auth_required {
            repairable += 1;
            let password = password
                .as_deref()
                .filter(|password| !password.is_empty())
                .ok_or_else(|| {
                    anyhow::anyhow!("SMTP password is required to repair this account.")
                })?;
            persist_account_password("SMTP", true, username, password_ref, password)?;
            repaired += 1;
        }
    }

    if repairable == 0 {
        anyhow::bail!(
            "Account '{}' has no password-backed IMAP/SMTP credentials to repair",
            account.key
        );
    }

    Ok(repaired)
}

fn persist_account_password(
    service: &str,
    auth_required: bool,
    username: &str,
    password_ref: &str,
    password: &str,
) -> anyhow::Result<()> {
    if !auth_required || password.is_empty() {
        tracing::debug!(
            credential_service = service,
            password_ref,
            auth_required,
            password_supplied = !password.is_empty(),
            "skipping credential persist"
        );
        return Ok(());
    }
    if username.trim().is_empty() {
        anyhow::bail!("{service} user is required to store the password.");
    }
    if password_ref.trim().is_empty() {
        anyhow::bail!("{service} pass ref is required to store the password.");
    }
    tracing::info!(
        credential_service = service,
        password_ref,
        "persisting credential to disk"
    );
    let scoped_ref = crate::provider_credentials::scoped_password_ref(password_ref);

    // Disk is authoritative: a 0600 file survives binary upgrades, so this can
    // never be blocked by a lost keychain ACL again.
    mxr_config::SecretStore::at_default_path()
        .set(&scoped_ref, username, password)
        .map_err(|error| {
            anyhow::anyhow!("failed to persist {service} credential to disk: {error}")
        })?;

    // Best-effort keychain mirror: keep the keychain in sync for users who rely
    // on it, but a keychain failure must never fail the operation.
    if let Err(error) = mxr_keychain::set_password(&scoped_ref, username, password) {
        tracing::warn!(
            credential_service = service,
            password_ref,
            error = %error,
            "keychain mirror write failed (non-fatal); credential is stored on disk"
        );
    }

    tracing::info!(
        credential_service = service,
        password_ref,
        "credential persisted to disk"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn imap_config_capabilities_advertise_idle_push_support() {
        let capabilities = config_sync_capabilities(&mxr_config::SyncProviderConfig::Imap {
            host: "imap.example.com".to_string(),
            port: 993,
            username: "me@example.com".to_string(),
            password_ref: "mxr/test".to_string(),
            auth_required: true,
            use_tls: true,
        });

        assert!(capabilities.push);
    }
}
