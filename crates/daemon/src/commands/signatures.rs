//! `mxr signatures` — manage outgoing compose signatures.

use crate::cli::{OutputFormat, SignatureDefaultKindArg, SignaturesAction};
use crate::ipc_client::IpcClient;
use crate::output::{jsonl, resolve_format};
use mxr_core::AccountId;
use mxr_protocol::*;

pub async fn run(
    action: Option<SignaturesAction>,
    format: Option<OutputFormat>,
) -> anyhow::Result<()> {
    let action = action.unwrap_or(SignaturesAction::List);
    let mut client = IpcClient::connect().await?;

    match action {
        SignaturesAction::List => list(&mut client, format).await,
        SignaturesAction::Set { name, body } => set(&mut client, name, body).await,
        SignaturesAction::Remove { name } => remove(&mut client, name).await,
        SignaturesAction::Defaults => defaults(&mut client, format).await,
        SignaturesAction::Default {
            name,
            kind,
            account,
            from_email,
        } => set_default(&mut client, name, kind, account, from_email).await,
        SignaturesAction::ClearDefault {
            kind,
            account,
            from_email,
        } => clear_default(&mut client, kind, account, from_email).await,
    }
}

async fn list(client: &mut IpcClient, format: Option<OutputFormat>) -> anyhow::Result<()> {
    let signatures = match client.request(Request::ListSignatures).await? {
        Response::Ok {
            data: ResponseData::Signatures { signatures },
        } => signatures,
        Response::Error { message, .. } => anyhow::bail!(message),
        _ => anyhow::bail!("Unexpected response"),
    };

    match resolve_format(format) {
        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&signatures)?),
        OutputFormat::Jsonl => println!("{}", jsonl(&signatures)?),
        OutputFormat::Ids => {
            for signature in &signatures {
                println!("{}", signature.id);
            }
        }
        OutputFormat::Csv => {
            let mut writer = csv::Writer::from_writer(Vec::new());
            writer.write_record(["id", "name", "body_bytes", "updated_at"])?;
            for signature in &signatures {
                writer.write_record(vec![
                    signature.id.as_str(),
                    signature.name.clone(),
                    signature.body.len().to_string(),
                    signature.updated_at.to_rfc3339(),
                ])?;
            }
            println!("{}", String::from_utf8(writer.into_inner()?)?.trim_end());
        }
        OutputFormat::Table => {
            if signatures.is_empty() {
                println!("No signatures defined");
            } else {
                for signature in &signatures {
                    let preview: String = signature.body.chars().take(60).collect();
                    let suffix = if signature.body.chars().count() > 60 {
                        "..."
                    } else {
                        ""
                    };
                    println!("  {}: {preview}{suffix}", signature.name);
                }
            }
        }
    }
    Ok(())
}

async fn set(client: &mut IpcClient, name: String, body: String) -> anyhow::Result<()> {
    match client.request(Request::SetSignature { name, body }).await? {
        Response::Ok {
            data: ResponseData::SignatureData { signature },
        } => {
            println!("Saved signature {}", signature.name);
            Ok(())
        }
        Response::Error { message, .. } => anyhow::bail!(message),
        _ => anyhow::bail!("Unexpected response"),
    }
}

async fn remove(client: &mut IpcClient, name: String) -> anyhow::Result<()> {
    match client
        .request(Request::DeleteSignature { name: name.clone() })
        .await?
    {
        Response::Ok {
            data: ResponseData::Ack,
        } => {
            println!("Deleted signature {name}");
            Ok(())
        }
        Response::Error { message, .. } => anyhow::bail!(message),
        _ => anyhow::bail!("Unexpected response"),
    }
}

async fn defaults(client: &mut IpcClient, format: Option<OutputFormat>) -> anyhow::Result<()> {
    let defaults = match client.request(Request::ListSignatureDefaults).await? {
        Response::Ok {
            data: ResponseData::SignatureDefaults { defaults },
        } => defaults,
        Response::Error { message, .. } => anyhow::bail!(message),
        _ => anyhow::bail!("Unexpected response"),
    };

    match resolve_format(format) {
        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&defaults)?),
        OutputFormat::Jsonl => println!("{}", jsonl(&defaults)?),
        OutputFormat::Ids => {
            for default in &defaults {
                println!("{}", default.signature.id);
            }
        }
        OutputFormat::Csv => {
            let mut writer = csv::Writer::from_writer(Vec::new());
            writer.write_record(["scope", "kind", "signature"])?;
            for default in &defaults {
                writer.write_record(vec![
                    scope_label(default),
                    context_label(default.kind).to_string(),
                    default.signature.name.clone(),
                ])?;
            }
            println!("{}", String::from_utf8(writer.into_inner()?)?.trim_end());
        }
        OutputFormat::Table => {
            if defaults.is_empty() {
                println!("No signature defaults defined");
            } else {
                for default in &defaults {
                    println!(
                        "  {} {}: {}",
                        scope_label(default),
                        context_label(default.kind),
                        default.signature.name
                    );
                }
            }
        }
    }
    Ok(())
}

async fn set_default(
    client: &mut IpcClient,
    name: String,
    kind: SignatureDefaultKindArg,
    account: Option<String>,
    from_email: Option<String>,
) -> anyhow::Result<()> {
    let account_id = resolve_account_id(client, account.as_deref()).await?;
    for context in contexts(kind) {
        match client
            .request(Request::SetSignatureDefault {
                name: name.clone(),
                kind: context,
                account_id: account_id.clone(),
                from_email: from_email.clone(),
            })
            .await?
        {
            Response::Ok { .. } => {}
            Response::Error { message, .. } => anyhow::bail!(message),
        }
    }
    println!("Set {} default signature to {name}", kind_label(kind));
    Ok(())
}

async fn clear_default(
    client: &mut IpcClient,
    kind: SignatureDefaultKindArg,
    account: Option<String>,
    from_email: Option<String>,
) -> anyhow::Result<()> {
    let account_id = resolve_account_id(client, account.as_deref()).await?;
    for context in contexts(kind) {
        match client
            .request(Request::ClearSignatureDefault {
                kind: context,
                account_id: account_id.clone(),
                from_email: from_email.clone(),
            })
            .await?
        {
            Response::Ok { .. } => {}
            Response::Error { message, .. } => anyhow::bail!(message),
        }
    }
    println!("Cleared {} default signature", kind_label(kind));
    Ok(())
}

async fn resolve_account_id(
    client: &mut IpcClient,
    selector: Option<&str>,
) -> anyhow::Result<Option<AccountId>> {
    let Some(selector) = selector.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    let accounts = match client.request(Request::ListAccounts).await? {
        Response::Ok {
            data: ResponseData::Accounts { accounts },
        } => accounts,
        Response::Error { message, .. } => anyhow::bail!(message),
        _ => anyhow::bail!("Unexpected response"),
    };
    let selector_lower = selector.to_ascii_lowercase();
    let matches = accounts
        .into_iter()
        .filter(|account| {
            account.key.as_deref().is_some_and(|key| key == selector)
                || account.name == selector
                || account.email.eq_ignore_ascii_case(selector)
                || account
                    .account_id
                    .to_string()
                    .eq_ignore_ascii_case(&selector_lower)
        })
        .map(|account| account.account_id)
        .collect::<Vec<_>>();

    match matches.as_slice() {
        [account_id] => Ok(Some(account_id.clone())),
        [] => anyhow::bail!("Account '{selector}' not found"),
        _ => anyhow::bail!("Account selector '{selector}' is ambiguous"),
    }
}

fn contexts(kind: SignatureDefaultKindArg) -> Vec<SignatureContextData> {
    match kind {
        SignatureDefaultKindArg::All => {
            vec![SignatureContextData::New, SignatureContextData::Reply]
        }
        SignatureDefaultKindArg::New => vec![SignatureContextData::New],
        SignatureDefaultKindArg::Reply => vec![SignatureContextData::Reply],
    }
}

fn context_label(kind: SignatureContextData) -> &'static str {
    match kind {
        SignatureContextData::New => "new",
        SignatureContextData::Reply => "reply",
    }
}

fn kind_label(kind: SignatureDefaultKindArg) -> &'static str {
    match kind {
        SignatureDefaultKindArg::All => "new/reply",
        SignatureDefaultKindArg::New => "new",
        SignatureDefaultKindArg::Reply => "reply",
    }
}

fn scope_label(default: &SignatureDefaultData) -> String {
    match (&default.account_id, &default.from_email) {
        (None, None) => "global".to_string(),
        (Some(account_id), None) => format!("account:{account_id}"),
        (Some(account_id), Some(email)) => format!("address:{account_id}:{email}"),
        (None, Some(email)) => format!("address:{email}"),
    }
}
