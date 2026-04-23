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
                enabled: true,
                is_default: false,
                source: AccountSourceData::Config,
                editable: AccountEditModeData::Full,
                sync: None,
                send: None,
            });

        summary.account_id = account_id;
        summary.key = Some(key.clone());
        summary.name.clone_from(&account.name);
        summary.email.clone_from(&account.email);
        summary.provider_kind = account_primary_provider_kind(&account);
        summary.sync_kind = account.sync.as_ref().map(config_sync_kind_label);
        summary.send_kind = account.send.as_ref().map(config_send_kind_label);
        summary.sync = account.sync.clone().map(sync_config_to_data);
        summary.send = account.send.clone().map(send_config_to_data);
        summary.is_default = default_config_key.as_deref() == Some(key.as_str());
        summary.source = match summary.source {
            AccountSourceData::Runtime => AccountSourceData::Both,
            _ => AccountSourceData::Config,
        };
        summary.editable = AccountEditModeData::Full;
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

pub(super) async fn authorize_account_config(
    account: AccountConfigData,
    reauthorize: bool,
) -> AccountOperationResult {
    let Some(AccountSyncConfigData::Gmail {
        credential_source,
        client_id,
        client_secret,
        token_ref,
    }) = account.sync
    else {
        return account_operation_result(
            false,
            "Authorization is only available for Gmail accounts.".into(),
            None,
            Some(account_step(
                false,
                "Selected account does not use Gmail sync.".into(),
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

    let mut auth = mxr_provider_gmail::auth::GmailAuth::new(client_id, client_secret, token_ref);
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
            Some(account_step(false, error.to_string())),
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
                        let mut gmail_auth = mxr_provider_gmail::auth::GmailAuth::new(
                            client_id,
                            client_secret,
                            token_ref,
                        );
                        let auth_result = match gmail_auth.load_existing().await {
                            Ok(()) => Ok("Existing OAuth token loaded.".to_string()),
                            Err(_) => gmail_auth.interactive_auth().await.map(|()| {
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
                                auth = Some(account_step(false, error.to_string()));
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
                let provider = mxr_provider_imap::ImapProvider::new(
                    mxr_core::AccountId::from_provider_id("imap", &account.email),
                    mxr_provider_imap::config::ImapConfig::new(
                        host,
                        port,
                        username,
                        password_ref,
                        auth_required,
                        use_tls,
                    ),
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
        }
    }

    match account.send {
        Some(AccountSendConfigData::Gmail) => {
            send = Some(account_step(true, "Gmail send configured.".into()));
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
            let provider = mxr_provider_smtp::SmtpSendProvider::new(
                mxr_provider_smtp::config::SmtpConfig::new(
                    host,
                    port,
                    username,
                    password_ref,
                    auth_required,
                    use_tls,
                ),
            );
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
    }
}

fn resolve_gmail_credentials(
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
                        Err("Bundled Gmail OAuth credentials are unavailable. Switch Credential source to Custom and enter your client ID/client secret.".into())
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

pub(super) fn sync_config_to_data(sync: mxr_config::SyncProviderConfig) -> AccountSyncConfigData {
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
    }
}

pub(super) fn config_account_id(
    key: &str,
    account: &mxr_config::AccountConfig,
) -> mxr_core::AccountId {
    let kind = account
        .sync
        .as_ref()
        .map(config_sync_kind_label)
        .or_else(|| account.send.as_ref().map(config_send_kind_label))
        .unwrap_or_else(|| key.to_string());
    mxr_core::AccountId::from_provider_id(&kind, &account.email)
}

pub(super) fn config_sync_kind_label(sync: &mxr_config::SyncProviderConfig) -> String {
    match sync {
        mxr_config::SyncProviderConfig::Gmail { .. } => "gmail".into(),
        mxr_config::SyncProviderConfig::Imap { .. } => "imap".into(),
    }
}

pub(super) fn config_send_kind_label(send: &mxr_config::SendProviderConfig) -> String {
    match send {
        mxr_config::SendProviderConfig::Gmail => "gmail".into(),
        mxr_config::SendProviderConfig::Smtp { .. } => "smtp".into(),
    }
}

pub(super) fn account_primary_provider_kind(account: &mxr_config::AccountConfig) -> String {
    account
        .sync
        .as_ref()
        .map(config_sync_kind_label)
        .or_else(|| account.send.as_ref().map(config_send_kind_label))
        .unwrap_or_else(|| "unknown".into())
}

pub(super) fn provider_kind_label(kind: &mxr_core::ProviderKind) -> &'static str {
    match kind {
        mxr_core::ProviderKind::Gmail => "gmail",
        mxr_core::ProviderKind::Imap => "imap",
        mxr_core::ProviderKind::Smtp => "smtp",
        mxr_core::ProviderKind::Fake => "fake",
    }
}

pub(super) fn send_config_to_data(send: mxr_config::SendProviderConfig) -> AccountSendConfigData {
    match send {
        mxr_config::SendProviderConfig::Gmail => AccountSendConfigData::Gmail,
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
    }
}

pub(super) fn sync_data_to_config(
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
    }
}

pub(super) fn send_data_to_config(
    data: AccountSendConfigData,
) -> Result<mxr_config::SendProviderConfig, String> {
    match data {
        AccountSendConfigData::Gmail => Ok(mxr_config::SendProviderConfig::Gmail),
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

pub(super) fn persist_account_passwords(account: &AccountConfigData) -> anyhow::Result<()> {
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

fn persist_account_password(
    service: &str,
    auth_required: bool,
    username: &str,
    password_ref: &str,
    password: &str,
) -> anyhow::Result<()> {
    if !auth_required || password.is_empty() {
        return Ok(());
    }
    if username.trim().is_empty() {
        anyhow::bail!("{service} user is required to store the password.");
    }
    if password_ref.trim().is_empty() {
        anyhow::bail!("{service} pass ref is required to store the password.");
    }
    mxr_keychain::set_password(password_ref, username, password)?;
    Ok(())
}
