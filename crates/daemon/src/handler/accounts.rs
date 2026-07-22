use super::account_config::{
    account_operation_result, account_step, authorize_account_config, disable_account_config,
    list_account_configs, list_runtime_accounts, remove_account_config, repair_account_config,
    set_default_account, test_account_config, upsert_account_config,
};
use super::HandlerResult;
use crate::state::AppState;
use mxr_protocol::{AccountConfigData, ResponseData};
use std::sync::Arc;

pub(super) async fn list_accounts(state: &AppState) -> HandlerResult {
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

pub(super) async fn remove_account(
    state: &Arc<AppState>,
    key: &str,
    purge_local_data: bool,
    dry_run: bool,
) -> HandlerResult {
    Ok(ResponseData::AccountOperation {
        result: remove_account_config(state, key, purge_local_data, dry_run).await,
    })
}

pub(super) async fn disable_account(state: &Arc<AppState>, key: &str) -> HandlerResult {
    Ok(ResponseData::AccountOperation {
        result: disable_account_config(state, key).await,
    })
}

pub(super) async fn repair_account(
    state: &Arc<AppState>,
    account: AccountConfigData,
) -> HandlerResult {
    let mut result = repair_account_config(account);
    if result.ok {
        // Reload providers so the repaired password takes effect immediately
        // (rebuilds provider handles and signals the long-lived IDLE loop to
        // reconnect). Without this, a running daemon keeps the stale cached
        // credential until a restart. The credential is already on disk, so a
        // reload failure does not undo the repair — but we must NOT hide it:
        // surface that the daemon needs a restart to pick up the new password.
        if let Err(error) = state.reload_accounts_from_disk().await {
            result.summary = format!(
                "{} The credential was saved to disk, but the daemon could not reload it live — restart the daemon to apply the new password.",
                result.summary
            );
            result.sync = Some(account_step(
                false,
                format!("daemon reload failed (restart to apply): {error}"),
            ));
        }
    }
    Ok(ResponseData::AccountOperation { result })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::handler::handle_request;
    use mxr_protocol::{
        AccountSendConfigData, AccountSyncConfigData, GmailCredentialSourceData, IpcMessage,
        IpcPayload, Request, Response,
    };

    #[test]
    fn remove_account_dry_run_reports_cached_messages_without_changing_config() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let config_dir = temp_dir.path().join("config");
        let data_dir = temp_dir.path().join("data");
        let socket_path = temp_dir.path().join("mxr.sock");
        std::fs::create_dir_all(&config_dir).expect("config dir");

        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime");

        temp_env::with_vars(
            [
                ("MXR_CONFIG_DIR", Some(config_dir)),
                ("MXR_DATA_DIR", Some(data_dir)),
                ("MXR_SOCKET_PATH", Some(socket_path)),
            ],
            || {
                runtime.block_on(async {
                let mut config = mxr_config::MxrConfig::default();
                config.general.default_account = Some("work".into());
                config.accounts.insert(
                    "work".into(),
                    account_config("Work", "work@example.com", true),
                );
                config.accounts.insert(
                    "personal".into(),
                    account_config("Personal", "me@example.com", true),
                );
                mxr_config::save_config(&config).expect("save config");

                let state = Arc::new(AppState::in_memory_without_accounts().await.expect("state"));
                let msg = IpcMessage {
                    id: 1,
                    source: ::mxr_protocol::ClientKind::default(),
                    payload: IpcPayload::Request(Request::RemoveAccountConfig {
                        key: "work".into(),
                        purge_local_data: false,
                        dry_run: true,
                    }),
                };

                let resp = handle_request(&state, &msg).await;
                match resp.payload {
                    IpcPayload::Response(Response::Ok {
                        data: ResponseData::AccountOperation { result },
                    }) => {
                        assert!(result.ok);
                        assert_eq!(
                            result.summary,
                            "Would remove account 'work' from config and detach 0 cached message(s)."
                        );
                    }
                    other => panic!("Expected AccountOperation, got {other:?}"),
                }

                let after = mxr_config::load_config().expect("load config");
                assert!(after.accounts.contains_key("work"));
                assert_eq!(after.general.default_account.as_deref(), Some("work"));
                });
            },
        );
    }

    #[test]
    fn disable_account_updates_config_and_default_through_ipc() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let config_dir = temp_dir.path().join("config");
        let data_dir = temp_dir.path().join("data");
        let socket_path = temp_dir.path().join("mxr.sock");
        std::fs::create_dir_all(&config_dir).expect("config dir");

        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime");

        temp_env::with_vars(
            [
                ("MXR_CONFIG_DIR", Some(config_dir)),
                ("MXR_DATA_DIR", Some(data_dir)),
                ("MXR_SOCKET_PATH", Some(socket_path)),
            ],
            || {
                runtime.block_on(async {
                    let mut config = mxr_config::MxrConfig::default();
                    config.general.default_account = Some("work".into());
                    config.accounts.insert(
                        "work".into(),
                        account_config("Work", "work@example.com", true),
                    );
                    config.accounts.insert(
                        "personal".into(),
                        account_config("Personal", "me@example.com", true),
                    );
                    mxr_config::save_config(&config).expect("save config");

                    let state =
                        Arc::new(AppState::in_memory_without_accounts().await.expect("state"));
                    let msg = IpcMessage {
                        id: 1,
                        source: ::mxr_protocol::ClientKind::default(),
                        payload: IpcPayload::Request(Request::DisableAccountConfig {
                            key: "work".into(),
                        }),
                    };

                    let resp = handle_request(&state, &msg).await;
                    match resp.payload {
                        IpcPayload::Response(Response::Ok {
                            data: ResponseData::AccountOperation { result },
                        }) => {
                            assert!(result.ok);
                            assert_eq!(result.summary, "Disabled account 'work'.");
                        }
                        other => panic!("Expected AccountOperation, got {other:?}"),
                    }

                    let after = mxr_config::load_config().expect("load config");
                    assert!(!after.accounts["work"].enabled);
                    assert_eq!(after.general.default_account.as_deref(), Some("personal"));
                });
            },
        );
    }

    #[test]
    fn repair_account_without_password_backed_credentials_is_rejected_by_ipc() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime");

        runtime.block_on(async {
            let state = Arc::new(AppState::in_memory_without_accounts().await.expect("state"));
            let msg = IpcMessage {
                id: 1,
                source: ::mxr_protocol::ClientKind::default(),
                payload: IpcPayload::Request(Request::RepairAccountConfig {
                    account: AccountConfigData {
                        key: "gmail".into(),
                        name: "Gmail".into(),
                        email: "me@example.com".into(),
                        enabled: true,
                        sync: Some(AccountSyncConfigData::Gmail {
                            credential_source: GmailCredentialSourceData::Custom,
                            client_id: "client".into(),
                            client_secret: Some("secret".into()),
                            token_ref: "mxr/gmail-token".into(),
                        }),
                        send: Some(AccountSendConfigData::Gmail),
                        is_default: false,
                    },
                }),
            };

            let resp = handle_request(&state, &msg).await;
            match resp.payload {
                IpcPayload::Response(Response::Ok {
                    data: ResponseData::AccountOperation { result },
                }) => {
                    assert!(!result.ok);
                    assert_eq!(
                        result.summary,
                        "Account 'gmail' has no password-backed credentials to repair."
                    );
                }
                other => panic!("Expected AccountOperation, got {other:?}"),
            }
        });
    }

    #[test]
    fn repair_account_through_ipc_persists_password_to_disk() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let config_dir = temp_dir.path().join("config");
        let data_dir = temp_dir.path().join("data");
        let socket_path = temp_dir.path().join("mxr.sock");
        let secrets_path = config_dir.join("secrets.toml");
        std::fs::create_dir_all(&config_dir).expect("config dir");

        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime");

        temp_env::with_vars(
            [
                ("MXR_CONFIG_DIR", Some(config_dir.as_os_str())),
                ("MXR_DATA_DIR", Some(data_dir.as_os_str())),
                ("MXR_SOCKET_PATH", Some(socket_path.as_os_str())),
                ("MXR_SECRETS_PATH", Some(secrets_path.as_os_str())),
                // Prod instance keeps the credential service unscoped; disable
                // the keychain mirror so the test never touches the real OS
                // keychain / Secret Service.
                ("MXR_INSTANCE", Some("mxr".as_ref())),
                ("MXR_KEYCHAIN", Some("off".as_ref())),
            ],
            || {
                runtime.block_on(async {
                    let state =
                        Arc::new(AppState::in_memory_without_accounts().await.expect("state"));
                    let msg = IpcMessage {
                        id: 1,
                        source: ::mxr_protocol::ClientKind::default(),
                        payload: IpcPayload::Request(Request::RepairAccountConfig {
                            account: AccountConfigData {
                                key: "consulting".into(),
                                name: "Consulting".into(),
                                email: "consulting@example.com".into(),
                                enabled: true,
                                sync: Some(AccountSyncConfigData::Imap {
                                    host: "imap.example.com".into(),
                                    port: 993,
                                    username: "consulting@example.com".into(),
                                    password_ref: "mxr/consulting-imap".into(),
                                    password: Some("s3cret-app-pw".into()),
                                    auth_required: true,
                                    use_tls: true,
                                }),
                                send: None,
                                is_default: false,
                            },
                        }),
                    };

                    let resp = handle_request(&state, &msg).await;
                    match resp.payload {
                        IpcPayload::Response(Response::Ok {
                            data: ResponseData::AccountOperation { result },
                        }) => {
                            assert!(result.ok, "repair should succeed, got {result:?}");
                        }
                        other => panic!("Expected AccountOperation, got {other:?}"),
                    }

                    // The daemon (this handler) is the writer: the credential is
                    // authoritatively on disk in the secrets store.
                    let store = mxr_config::SecretStore::new(secrets_path.clone());
                    assert_eq!(
                        store
                            .get("mxr/consulting-imap", "consulting@example.com")
                            .expect("read secret")
                            .as_deref(),
                        Some("s3cret-app-pw"),
                    );
                });
            },
        );
    }

    fn account_config(name: &str, email: &str, enabled: bool) -> mxr_config::AccountConfig {
        mxr_config::AccountConfig {
            name: name.into(),
            email: email.into(),
            enabled,
            sync: None,
            send: None,
        }
    }
}
