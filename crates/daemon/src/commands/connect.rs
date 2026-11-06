//! Explicit client selection commits; network responses never overwrite a newer generation.
use crate::cli::OutputFormat;
use mxr_client::remote_connection::{authority, retry_cleanup};
use mxr_config::connection_state::{
    self as persistence, ConnectionRegistration, ConnectionState, PendingConnection,
};
use mxr_core::{RemoteCapability, RemoteSecret};
use mxr_protocol::{ClientKind, RemoteRequest, RemoteResponse, Request, ResponseData};
use sha2::{Digest, Sha256};

async fn update<T: Send + 'static>(
    generation: Option<u64>,
    mutate: impl FnOnce(&mut ConnectionState) -> std::io::Result<T> + Send + 'static,
) -> anyhow::Result<T> {
    Ok(tokio::task::spawn_blocking(move || {
        persistence::update(&persistence::path(), generation, mutate)
    })
    .await??)
}
async fn read() -> anyhow::Result<ConnectionState> {
    Ok(tokio::task::spawn_blocking(|| persistence::read(&persistence::path())).await??)
}
fn secret() -> std::io::Result<RemoteSecret> {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).map_err(std::io::Error::other)?;
    Ok(RemoteSecret::new(base16ct::lower::encode_string(&bytes)))
}
fn verifier(secret: &RemoteSecret) -> RemoteSecret {
    RemoteSecret::new(base16ct::lower::encode_string(&Sha256::digest(
        secret.expose().as_bytes(),
    )))
}

fn validate_client_name(name: &str) -> anyhow::Result<()> {
    if name.trim().is_empty() || name.len() > 80 || name.chars().any(char::is_control) {
        anyhow::bail!("Client name must be non-empty, at most 80 UTF-8 bytes, and contain no control characters");
    }
    Ok(())
}

pub struct ConnectCommand {
    pub url: Option<String>,
    pub local: bool,
    pub status: bool,
    pub name: String,
    pub dry_run: bool,
    pub confirm: bool,
    pub allow_send: bool,
    pub allow_triage: bool,
    pub allow_ai: bool,
    pub cancel_pairing: bool,
    pub restart_pairing: bool,
    pub forget_cleanup: bool,
    pub format: Option<OutputFormat>,
}

pub async fn run(command: ConnectCommand) -> anyhow::Result<()> {
    let ConnectCommand {
        url,
        local,
        status,
        name,
        dry_run,
        confirm,
        allow_send,
        allow_triage,
        allow_ai,
        cancel_pairing,
        restart_pairing,
        forget_cleanup,
        format,
    } = command;
    let format = format.unwrap_or(OutputFormat::Json);
    let snapshot = read().await?;
    let mut capabilities = if allow_send {
        vec![RemoteCapability::MailRead, RemoteCapability::MailSend]
    } else {
        vec![RemoteCapability::MailRead]
    };
    if allow_triage {
        capabilities.push(RemoteCapability::MailTriage);
    }
    if allow_ai {
        capabilities.push(RemoteCapability::MailAi);
    }
    if cancel_pairing || forget_cleanup {
        return recover_pending(snapshot, cancel_pairing, dry_run || !confirm, format).await;
    }
    if status {
        if !dry_run {
            retry_cleanup().await?;
        }
        let snapshot = read().await?;
        let override_address = std::env::var("MXR_DAEMON_ADDR")
            .ok()
            .filter(|value| !value.trim().is_empty());
        let override_target = override_address
            .as_deref()
            .map(mxr_transport::TransportAddr::parse)
            .transpose()?
            .map(|address| mxr_client::SelectedDaemonTarget::new(address, None).description());
        return crate::output::print_json(
            &serde_json::json!({"effective_target":override_target.as_deref().or_else(|| snapshot.selected.as_ref().map(|entry| entry.origin.as_str())).unwrap_or("local"),"selection_source":if override_address.is_some(){"environment"}else if snapshot.selected.is_some(){"saved"}else{"local"},"selection_generation":snapshot.generation,"registration_state":if snapshot.selected.is_some(){"paired"}else{"local"},"session_state":"not_inspected","capabilities":snapshot.selected.as_ref().map(|entry| &entry.capabilities),"pending_deregistration_count":snapshot.cleanup.len()+snapshot.retired_pairings.len(),"cleanup_retry_at":snapshot.cleanup_retry_at,"pending_pairing":pending_view(&snapshot)}),
            format,
        );
    }
    if std::env::var("MXR_DAEMON_ADDR")
        .ok()
        .is_some_and(|value| !value.trim().is_empty())
    {
        anyhow::bail!("MXR_DAEMON_ADDR masks saved selection; unset it before mxr connect");
    }
    let origin = url
        .as_deref()
        .map(mxr_transport::https_origin)
        .transpose()?;
    if !local && origin.is_none() {
        anyhow::bail!("Supply an HTTPS origin, --local, or --status");
    }
    if !local {
        validate_client_name(&name)?;
    }
    if dry_run || !confirm {
        return crate::output::print_json(
            &serde_json::json!({"dry_run":true,"old_target":snapshot.selected.as_ref().map(|entry| &entry.origin),"new_target":origin.as_deref().unwrap_or("local"),"selection_applied":false,"requested_capabilities":capabilities,"old_capabilities":snapshot.selected.as_ref().map(|entry| &entry.capabilities),"restart_pairing":restart_pairing,"pending_pairing":pending_view(&snapshot)}),
            format,
        );
    }
    if local {
        update(Some(snapshot.generation), |state| {
            if let Some(old) = state.selected.take() {
                state.cleanup.push(old.into());
                state.reset_cleanup_retry();
            }
            if let Some(pending) = state.pending_pairing.take() {
                state.retired_pairings.push(pending.into());
                state.reset_cleanup_retry();
            }
            state.generation = state
                .generation
                .checked_add(1)
                .ok_or_else(|| std::io::Error::other("Selection generation exhausted"))?;
            Ok(())
        })
        .await?;
        let pending = retry_cleanup().await?;
        return crate::output::print_json(
            &serde_json::json!({"selection_applied":true,"effective_target":"local","deregistration":if pending == 0 {"complete"}else{"pending"},"pending_deregistration_count":pending}),
            format,
        );
    }
    let origin = origin.ok_or_else(|| anyhow::anyhow!("HTTPS origin required"))?;
    if let Some(saved) = &snapshot.selected {
        if !restart_pairing
            && snapshot.pending_pairing.is_none()
            && saved.origin == origin
            && capabilities
                .iter()
                .all(|grant| saved.capabilities.contains(grant))
        {
            mxr_client::selected_daemon::connect_registration(saved, ClientKind::Cli).await?;
            return crate::output::print_json(
                &serde_json::json!({"selection_applied":true,"effective_target":origin,"client_id":saved.client_id,"reused":true,"capabilities":saved.capabilities}),
                format,
            );
        }
    }
    let pending = update(Some(snapshot.generation), move |state| {
        if restart_pairing {
            if let Some(pending) = state.pending_pairing.take() {
                state.retired_pairings.push(pending.into());
                state.reset_cleanup_retry();
            }
        }
        if let Some(pending) = &state.pending_pairing {
            if pending.origin != origin || pending.capabilities != capabilities {
                return Err(std::io::Error::other(
                    "A pairing with a different origin or grant is pending; complete it or cancel it before changing the request",
                ));
            }
            return Ok(pending.clone());
        }
        let pending = PendingConnection {
            capabilities,
            origin,
            name,
            mail_secret: secret()?,
            cleanup_secret: secret()?,
            request_id: None,
        };
        state.pending_pairing = Some(pending.clone());
        Ok(pending)
    })
    .await?;
    let request_id = match pending.request_id.clone() {
        Some(id) => id,
        None => {
            let response = authority(
                &pending.origin,
                "pairings",
                RemoteRequest::StartPairing {
                    name: pending.name.clone(),
                    capabilities: pending.capabilities.clone(),
                    mail_verifier: verifier(&pending.mail_secret),
                    cleanup_verifier: verifier(&pending.cleanup_secret),
                },
            )
            .await?;
            let ResponseData::RemoteAuthority {
                response: RemoteResponse::Pairing { pairing, .. },
            } = response
            else {
                anyhow::bail!("Unexpected pairing response");
            };
            let id = pairing.request_id.clone();
            let persisted_id = id.clone();
            let expected_cleanup = pending.cleanup_secret.clone();
            update(Some(snapshot.generation), move |state| {
                ensure_pending_matches(state, &expected_cleanup)?;
                let entry = state
                    .pending_pairing
                    .as_mut()
                    .ok_or_else(|| std::io::Error::other("Pairing was cancelled"))?;
                entry.request_id = Some(persisted_id);
                Ok(())
            })
            .await?;
            crate::output::print_json(
                &serde_json::json!({"selection_applied":false,"pairing":pairing,"next_action":"Compare the full challenge on the host, approve this request locally, then repeat mxr connect with the same origin"}),
                format,
            )?;
            return Ok(());
        }
    };
    let response = authority(
        &pending.origin,
        "pairings",
        RemoteRequest::ClaimPairing {
            request_id,
            mail_secret: pending.mail_secret.clone(),
            cleanup_secret: pending.cleanup_secret.clone(),
        },
    )
    .await.map_err(|error| anyhow::anyhow!("{error}. The selected registration is unchanged. Approve the pending challenge on the host and repeat, or use --restart-pairing with this URL and the same grant flags; --cancel-pairing preserves the selected mailbox."))?;
    let ResponseData::RemoteAuthority {
        response: RemoteResponse::Client { client, .. },
    } = response
    else {
        anyhow::bail!("Unexpected claim response");
    };
    if client.capabilities != pending.capabilities {
        anyhow::bail!("Host approved different grants; saved selection was preserved");
    }
    let mut registration = ConnectionRegistration {
        capabilities: client.capabilities,
        origin: pending.origin,
        client_id: client.client_id,
        host_id: String::new(),
        workspace_id: String::new(),
        recovery_generation: String::new(),
        mail_secret: pending.mail_secret,
        cleanup_secret: pending.cleanup_secret,
    };
    let mut connection =
        mxr_client::selected_daemon::connect_registration(&registration, ClientKind::Cli).await?;
    let ResponseData::RemoteReady {
        host_id,
        workspace,
        protocol_version,
        ..
    } = connection.request(Request::GetRemoteReady).await?
    else {
        anyhow::bail!("Remote readiness missing");
    };
    if protocol_version != mxr_protocol::IPC_PROTOCOL_VERSION {
        anyhow::bail!("Remote protocol mismatch");
    }
    registration.host_id = host_id;
    registration.workspace_id = workspace.workspace_id;
    registration.recovery_generation = workspace.recovery_generation;
    let displayed_id = registration.client_id.clone();
    let displayed_origin = registration.origin.clone();
    let displayed_capabilities = registration.capabilities.clone();
    update(Some(snapshot.generation), move |state| {
        ensure_pending_matches(state, &registration.cleanup_secret)?;
        if let Some(old) = state.selected.take() {
            state.cleanup.push(old.into());
            state.reset_cleanup_retry();
        }
        state.selected = Some(registration);
        state.pending_pairing = None;
        state.generation = state
            .generation
            .checked_add(1)
            .ok_or_else(|| std::io::Error::other("Selection generation exhausted"))?;
        Ok(())
    })
    .await?;
    let pending_count = retry_cleanup().await?;
    crate::output::print_json(
        &serde_json::json!({"selection_applied":true,"effective_target":displayed_origin,"client_id":displayed_id,"capabilities":displayed_capabilities,"pending_deregistration_count":pending_count}),
        format,
    )
}

fn ensure_pending_matches(state: &ConnectionState, cleanup: &RemoteSecret) -> std::io::Result<()> {
    if state
        .pending_pairing
        .as_ref()
        .is_none_or(|entry| entry.cleanup_secret.expose() != cleanup.expose())
    {
        return Err(std::io::Error::other("Pending pairing changed while the host was responding; the newer challenge and selected mailbox were preserved"));
    }
    Ok(())
}

fn pending_view(state: &ConnectionState) -> Option<serde_json::Value> {
    state.pending_pairing.as_ref().map(|pending| serde_json::json!({"origin":pending.origin,"name":pending.name,"capabilities":pending.capabilities,"request_id":pending.request_id}))
}

async fn recover_pending(
    snapshot: ConnectionState,
    cancel: bool,
    dry_run: bool,
    format: OutputFormat,
) -> anyhow::Result<()> {
    if dry_run {
        return crate::output::print_json(
            &serde_json::json!({"dry_run":true,"action":if cancel{"cancel_pairing"}else{"forget_cleanup"},"selection_preserved":true,
            "pending_pairing":pending_view(&snapshot),"registrations":snapshot.cleanup.iter().map(|entry| serde_json::json!({"origin":entry.origin,"client_id":entry.client_id})).collect::<Vec<_>>(),
            "pairing_intents":snapshot.retired_pairings.iter().map(|entry| serde_json::json!({"origin":entry.origin,"name":entry.name})).collect::<Vec<_>>(),
            "effect":if cancel{"Retire only the pending challenge; the selected mailbox stays connected"}else{"Drop only these local retry intents. This does not prove the old host revoked access; revoke it there if reachable"}}),
            format,
        );
    }
    let remaining = update(Some(snapshot.generation), move |state| {
        if cancel {
            cancel_pending(state, snapshot.pending_pairing.as_ref())?;
        } else {
            forget_previewed(state, &snapshot);
        }
        Ok(state.cleanup.len() + state.retired_pairings.len())
    })
    .await?;
    let remaining = if cancel {
        retry_cleanup().await?
    } else {
        remaining
    };
    crate::output::print_json(
        &serde_json::json!({"dry_run":false,"selection_preserved":true,"pending_deregistration_count":remaining,"remote_revocation_verified":false}),
        format,
    )
}

fn cancel_pending(
    state: &mut ConnectionState,
    expected: Option<&PendingConnection>,
) -> std::io::Result<()> {
    if state
        .pending_pairing
        .as_ref()
        .map(|p| p.cleanup_secret.expose())
        != expected.map(|p| p.cleanup_secret.expose())
    {
        return Err(std::io::Error::other(
            "Pending pairing changed; preview again before cancelling",
        ));
    }
    if let Some(pending) = state.pending_pairing.take() {
        state.retired_pairings.push(pending.into());
        state.reset_cleanup_retry();
    }
    Ok(())
}
fn forget_previewed(state: &mut ConnectionState, snapshot: &ConnectionState) {
    state.cleanup.retain(|entry| {
        !snapshot.cleanup.iter().any(|old| {
            old.origin == entry.origin
                && old.client_id == entry.client_id
                && old.cleanup_secret.expose() == entry.cleanup_secret.expose()
        })
    });
    state.retired_pairings.retain(|entry| {
        !snapshot.retired_pairings.iter().any(|old| {
            old.origin == entry.origin
                && old.cleanup_secret.expose() == entry.cleanup_secret.expose()
        })
    });
    if state.cleanup.is_empty() && state.retired_pairings.is_empty() {
        state.reset_cleanup_retry();
    }
}

#[cfg(test)]
mod tests;
