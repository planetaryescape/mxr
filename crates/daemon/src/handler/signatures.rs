//! Outgoing signatures: daemon-side validation and IPC/store translation.

use super::HandlerResult;
use crate::state::AppState;
use chrono::Utc;
use mxr_core::{AccountId, SignatureId};
use mxr_protocol::{ResponseData, SignatureContextData, SignatureData, SignatureDefaultData};
use mxr_store::{Signature, SignatureDefault, SignatureKind, SignatureScope};

fn to_data(signature: Signature) -> SignatureData {
    SignatureData {
        id: signature.id,
        name: signature.name,
        body: signature.body,
        created_at: signature.created_at,
        updated_at: signature.updated_at,
    }
}

fn kind_from_data(kind: SignatureContextData) -> SignatureKind {
    match kind {
        SignatureContextData::New => SignatureKind::New,
        SignatureContextData::Reply => SignatureKind::Reply,
    }
}

fn kind_to_data(kind: SignatureKind) -> SignatureContextData {
    match kind {
        SignatureKind::New => SignatureContextData::New,
        SignatureKind::Reply => SignatureContextData::Reply,
    }
}

fn default_to_data(default: SignatureDefault) -> SignatureDefaultData {
    let (account_id, from_email) = match default.scope {
        SignatureScope::Global => (None, None),
        SignatureScope::Account(account_id) => (Some(account_id), None),
        SignatureScope::Address { account_id, email } => (Some(account_id), Some(email)),
    };
    SignatureDefaultData {
        kind: kind_to_data(default.kind),
        account_id,
        from_email,
        signature: to_data(default.signature),
        created_at: default.created_at,
        updated_at: default.updated_at,
    }
}

fn scope_from(
    account_id: Option<&AccountId>,
    from_email: Option<&str>,
) -> Result<SignatureScope, String> {
    let from_email = from_email.map(str::trim).filter(|email| !email.is_empty());
    match (account_id, from_email) {
        (None, None) => Ok(SignatureScope::Global),
        (Some(account_id), None) => Ok(SignatureScope::Account(account_id.clone())),
        (Some(account_id), Some(email)) => {
            Ok(SignatureScope::address(account_id.clone(), email.into()))
        }
        (None, Some(_)) => Err("signature from-email defaults require an account".to_string()),
    }
}

pub(super) async fn list_signatures(state: &AppState) -> HandlerResult {
    let signatures = state
        .store
        .list_signatures()
        .await
        .map_err(|e| e.to_string())?
        .into_iter()
        .map(to_data)
        .collect();
    Ok(ResponseData::Signatures { signatures })
}

pub(super) async fn list_signature_defaults(state: &AppState) -> HandlerResult {
    let defaults = state
        .store
        .list_signature_defaults()
        .await
        .map_err(|e| e.to_string())?
        .into_iter()
        .map(default_to_data)
        .collect();
    Ok(ResponseData::SignatureDefaults { defaults })
}

pub(super) async fn set_signature(state: &AppState, name: String, body: String) -> HandlerResult {
    let name = name.trim().to_string();
    if name.is_empty() {
        return Err("signature name cannot be empty".to_string());
    }
    if body.trim().is_empty() {
        return Err("signature body cannot be empty".to_string());
    }

    let now = Utc::now();
    let existing = state
        .store
        .get_signature_by_name(&name)
        .await
        .map_err(|e| e.to_string())?;
    let signature = Signature {
        id: existing
            .as_ref()
            .map(|signature| signature.id.clone())
            .unwrap_or_else(SignatureId::new),
        name,
        body,
        created_at: existing.map_or(now, |signature| signature.created_at),
        updated_at: now,
    };
    state
        .store
        .upsert_signature(&signature)
        .await
        .map_err(|e| e.to_string())?;
    Ok(ResponseData::SignatureData {
        signature: to_data(signature),
    })
}

pub(super) async fn delete_signature(state: &AppState, name: &str) -> HandlerResult {
    state
        .store
        .delete_signature_by_name(name)
        .await
        .map_err(|e| e.to_string())?;
    Ok(ResponseData::Ack)
}

pub(super) async fn set_signature_default(
    state: &AppState,
    name: &str,
    kind: SignatureContextData,
    account_id: Option<&AccountId>,
    from_email: Option<&str>,
) -> HandlerResult {
    let signature = state
        .store
        .get_signature_by_name(name)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("signature not found: {name}"))?;
    let scope = scope_from(account_id, from_email)?;
    state
        .store
        .set_signature_default(&scope, kind_from_data(kind), &signature.id)
        .await
        .map_err(|e| e.to_string())?;
    Ok(ResponseData::SignatureData {
        signature: to_data(signature),
    })
}

pub(super) async fn clear_signature_default(
    state: &AppState,
    kind: SignatureContextData,
    account_id: Option<&AccountId>,
    from_email: Option<&str>,
) -> HandlerResult {
    let scope = scope_from(account_id, from_email)?;
    state
        .store
        .clear_signature_default(&scope, kind_from_data(kind))
        .await
        .map_err(|e| e.to_string())?;
    Ok(ResponseData::Ack)
}

pub(super) async fn resolve_signature(
    state: &AppState,
    name: Option<&str>,
    kind: SignatureContextData,
    account_id: Option<&AccountId>,
    from_email: Option<&str>,
) -> HandlerResult {
    if let Some(name) = name.map(str::trim).filter(|name| !name.is_empty()) {
        let signature = state
            .store
            .get_signature_by_name(name)
            .await
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("signature not found: {name}"))?;
        return Ok(ResponseData::ResolvedSignature {
            signature: Some(to_data(signature)),
        });
    }

    if account_id.is_none()
        && from_email
            .map(str::trim)
            .is_some_and(|email| !email.is_empty())
    {
        return Err("signature resolution with from-email requires an account".to_string());
    }

    let signature = state
        .store
        .resolve_signature(account_id, from_email, kind_from_data(kind))
        .await
        .map_err(|e| e.to_string())?
        .map(to_data);
    Ok(ResponseData::ResolvedSignature { signature })
}
