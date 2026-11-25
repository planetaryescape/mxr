use crate::cli::{LlmAction, LlmKeyAction, OutputFormat};
use crate::ipc_client::IpcClient;
use crate::output::resolve_format;
use mxr_protocol::{
    LlmBackfillHeldData, LlmEndpointStatusData, LlmFeatureStatusData, LlmKeyChangeData,
    LlmKeySourceData, LlmKeyStatusData, LlmProviderData, LlmRouteStateData, LlmRouteStatusData,
    LlmStatusSnapshot, Request, Response, ResponseData,
};
use std::io::{IsTerminal, Read};

pub async fn run(action: Option<LlmAction>, format: Option<OutputFormat>) -> anyhow::Result<()> {
    let action = action.unwrap_or(LlmAction::Status);
    let key = match &action {
        LlmAction::Key {
            action: LlmKeyAction::Set { provider, .. },
        } => Some(read_key_from_stdin((*provider).into())?),
        _ => None,
    };
    let mut client = IpcClient::connect().await?;
    match action {
        LlmAction::Status => {
            let snapshot = fetch_status(&mut client).await?;
            render_status(&snapshot, resolve_format(format))
        }
        LlmAction::Key { action } => match action {
            LlmKeyAction::Status => {
                let snapshot = fetch_status(&mut client).await?;
                render_key_status(&snapshot.keys, resolve_format(format))
            }
            LlmKeyAction::Set { provider, dry_run } => {
                let Some(key) = key else {
                    anyhow::bail!("no key was read");
                };
                let response = client
                    .request(Request::SetLlmKey {
                        provider: provider.into(),
                        key,
                        dry_run,
                    })
                    .await?;
                render_key_change(
                    expect_change(response)?,
                    resolve_format(format),
                    Verb::Store,
                )
            }
            LlmKeyAction::Remove { provider, dry_run } => {
                let response = client
                    .request(Request::RemoveLlmKey {
                        provider: provider.into(),
                        dry_run,
                    })
                    .await?;
                render_key_change(
                    expect_change(response)?,
                    resolve_format(format),
                    Verb::Remove,
                )
            }
        },
    }
}

async fn fetch_status(client: &mut IpcClient) -> anyhow::Result<LlmStatusSnapshot> {
    let response = client.request(Request::GetLlmStatus).await?;
    crate::commands::expect_response(response, |r| match r {
        Response::Ok {
            data: ResponseData::LlmStatus { snapshot },
        } => Some(snapshot),
        _ => None,
    })
}

fn expect_change(response: Response) -> anyhow::Result<LlmKeyChangeData> {
    crate::commands::expect_response(response, |r| match r {
        Response::Ok {
            data: ResponseData::LlmKeyChange { change },
        } => Some(change),
        _ => None,
    })
}

/// The key comes from a pipe, so it never appears in argv, shell history or
/// the terminal. Refusing a terminal also means a forgotten pipe doesn't
/// leave the command waiting for typed (and echoed) input.
fn read_key_from_stdin(provider: LlmProviderData) -> anyhow::Result<mxr_protocol::LlmKeySecret> {
    let stdin = std::io::stdin();
    if stdin.is_terminal() {
        anyhow::bail!(
            "pipe the key in so it isn't echoed or kept in shell history, for example:\n  \
             op read \"op://<vault>/<item>/credential\" | mxr llm key set {}",
            provider.key()
        );
    }
    let mut raw = String::new();
    stdin
        .lock()
        .take(MAX_STDIN_BYTES + 1)
        .read_to_string(&mut raw)
        .map_err(|_| anyhow::anyhow!("couldn't read a key from stdin"))?;
    if raw.len() as u64 > MAX_STDIN_BYTES {
        anyhow::bail!("stdin is longer than any API key");
    }
    let key = crate::llm_keys::clean_key(&raw).map_err(|reason| anyhow::anyhow!(reason))?;
    Ok(mxr_protocol::LlmKeySecret::new(key))
}

const MAX_STDIN_BYTES: u64 = 4096;

fn render_status(snapshot: &LlmStatusSnapshot, format: OutputFormat) -> anyhow::Result<()> {
    match format {
        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(snapshot)?),
        OutputFormat::Jsonl => println!("{}", serde_json::to_string(snapshot)?),
        _ => {
            println!(
                "enabled={} provider={} model={}",
                snapshot.enabled, snapshot.provider, snapshot.model
            );
            if let Some(base_url) = &snapshot.base_url {
                println!("base_url={base_url}");
            }
            if let Some(api_key_env) = &snapshot.api_key_env {
                println!(
                    "api_key_env={} present={}",
                    api_key_env, snapshot.api_key_present
                );
            }
            println!(
                "context_window={} supports_streaming={} timeout_secs={}",
                snapshot.context_window, snapshot.supports_streaming, snapshot.request_timeout_secs
            );
            if let Some(model) = &snapshot.fallback_model {
                println!("fallback={model}");
            }
            if let Some(error) = &snapshot.fallback_error {
                println!("fallback not used: {error}");
            }
            if let Some(until) = snapshot.background_paused_until {
                println!(
                    "background calls paused after repeated endpoint failures until {}",
                    until.with_timezone(&chrono::Local).format("%H:%M")
                );
            }
            if snapshot.enabled && snapshot.background_live_only {
                println!("{}", backfill_line(&snapshot.backfill_held));
            }
            if snapshot.enabled {
                print!("{}", features_table(&snapshot.features));
                if snapshot
                    .features
                    .iter()
                    .any(|row| row.routed || !row.fallback.is_empty())
                {
                    print!("{}", keys_table(&snapshot.keys));
                }
            }
        }
    }
    Ok(())
}

/// Background jobs skip backfilled mail; say so, and how much they held back.
fn backfill_line(held: &[LlmBackfillHeldData]) -> String {
    let counts = held
        .iter()
        .map(|row| format!("{} {}", row.job, row.inputs))
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "background AI: live mail only; backfilled mail excluded (held back since daemon start: {})",
        if counts.is_empty() { "none" } else { &counts }
    )
}

fn features_table(features: &[LlmFeatureStatusData]) -> String {
    let name_width = features.iter().map(|f| f.feature.len()).max().unwrap_or(0);
    let model_width = features.iter().map(|f| f.model.len()).max().unwrap_or(0);
    let mut out = String::from("\nfeatures:\n");
    for feature in features {
        let route = if !feature.enabled {
            "off".to_string()
        } else if let Some(endpoint) = &feature.endpoint {
            chain_label(endpoint, feature)
        } else if feature.routed {
            feature
                .routes
                .iter()
                .map(|route| format!("{}: {}", route.provider.key(), route_label(route)))
                .collect::<Vec<_>>()
                .join(" → ")
                + &feature.routes.iter().find(|route| route.next).map_or_else(
                    || {
                        none_available(
                            feature
                                .routes
                                .iter()
                                .map(|route| (route.provider.key(), route.state, route.retry_at)),
                        )
                    },
                    |route| format!("  [now: {}]", route.provider.key()),
                )
        } else {
            let place = if feature.cloud { "cloud" } else { "local" };
            format!(
                "{} ({place})",
                feature.base_url.as_deref().unwrap_or("this machine")
            )
        };
        let blocked = feature
            .blocked
            .as_ref()
            .map_or_else(String::new, |reason| format!("  [blocked: {reason}]"));
        out.push_str(&format!(
            "  {:name_width$}  {:model_width$}  {route}{blocked}\n",
            feature.feature, feature.model
        ));
    }
    out
}

fn chain_label(endpoint: &LlmEndpointStatusData, feature: &LlmFeatureStatusData) -> String {
    let mut parts = vec![format!(
        "{}: {}",
        endpoint.base_url,
        state_label(
            endpoint.state,
            endpoint.reason.as_deref(),
            endpoint.retry_at
        )
    )];
    parts.extend(
        feature
            .fallback
            .iter()
            .map(|route| format!("{}: {}", route.provider.key(), route_label(route))),
    );
    let now = if endpoint.next {
        "  [now: base_url]".to_string()
    } else {
        feature
            .fallback
            .iter()
            .find(|route| route.next)
            .map_or_else(
                || {
                    none_available(
                        std::iter::once(("base_url", endpoint.state, endpoint.retry_at)).chain(
                            feature
                                .fallback
                                .iter()
                                .map(|route| (route.provider.key(), route.state, route.retry_at)),
                        ),
                    )
                },
                |route| format!("  [now: {}]", route.provider.key()),
            )
    };
    let withheld = feature
        .fallback_withheld
        .as_ref()
        .map_or_else(String::new, |reason| format!("  [no fallback: {reason}]"));
    parts.join(" → ") + &now + &withheld
}

/// Every route is out. Background work waits, but a command you run still
/// tries the one that comes back soonest, once; name it. A route already due
/// or being checked comes first; a route with no key is never tried.
fn none_available<'a>(
    routes: impl Iterator<
        Item = (
            &'a str,
            LlmRouteStateData,
            Option<chrono::DateTime<chrono::Utc>>,
        ),
    >,
) -> String {
    routes
        .filter(|(_, state, _)| *state != LlmRouteStateData::NoKey)
        .filter_map(|(name, state, at)| match state {
            LlmRouteStateData::Recovering | LlmRouteStateData::Probing => {
                Some((name, at.unwrap_or(chrono::DateTime::<chrono::Utc>::MIN_UTC)))
            }
            _ => Some((name, at?)),
        })
        .min_by_key(|(_, at)| *at)
        .map_or_else(
            || "  (no route available)".to_string(),
            |(name, _)| format!("  (no route available; a command you run tries {name} once)"),
        )
}

fn route_label(route: &LlmRouteStatusData) -> String {
    state_label(route.state, route.reason.as_deref(), route.retry_at)
}

fn state_label(
    state: LlmRouteStateData,
    reason_text: Option<&str>,
    retry_at: Option<chrono::DateTime<chrono::Utc>>,
) -> String {
    let reason = reason_text.map_or_else(String::new, |reason| format!(" ({reason})"));
    let at = retry_at.map(|at| at.with_timezone(&chrono::Local).format("%H:%M").to_string());
    match state {
        LlmRouteStateData::Ok => "ok".to_string(),
        LlmRouteStateData::NoKey => "no key".to_string(),
        LlmRouteStateData::Cooling => format!(
            "cooling until {}{reason}",
            at.unwrap_or_else(|| "soon".to_string())
        ),
        LlmRouteStateData::Broken => format!(
            "broken: {}{}",
            reason_text.unwrap_or("needs attention"),
            at.map_or_else(String::new, |at| format!(" (rechecked {at})"))
        ),
        LlmRouteStateData::Recovering => format!("retry due{reason}"),
        LlmRouteStateData::Probing => format!("being retried{reason}"),
    }
}

fn keys_table(keys: &[LlmKeyStatusData]) -> String {
    let mut out = String::from("\nkeys:\n");
    for key in keys {
        out.push_str(&format!("  {}\n", key_line(key)));
    }
    out
}

fn key_line(key: &LlmKeyStatusData) -> String {
    let provider = provider_name(key.provider);
    match (key.source, &key.location) {
        (Some(LlmKeySourceData::Stored), Some(location)) => {
            format!("{provider:10}  stored in {location}")
        }
        (Some(LlmKeySourceData::Env), Some(location)) => {
            format!("{provider:10}  from the environment ({location})")
        }
        _ => format!(
            "{provider:10}  none; run `mxr llm key set {provider}` or set {}",
            key.env_var
        ),
    }
}

fn provider_name(provider: LlmProviderData) -> &'static str {
    provider.key()
}

fn render_key_status(keys: &[LlmKeyStatusData], format: OutputFormat) -> anyhow::Result<()> {
    match format {
        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(keys)?),
        OutputFormat::Jsonl => {
            for key in keys {
                println!("{}", serde_json::to_string(key)?);
            }
        }
        _ => {
            for key in keys {
                println!("{}", key_line(key));
            }
        }
    }
    Ok(())
}

#[derive(Clone, Copy)]
enum Verb {
    Store,
    Remove,
}

fn render_key_change(
    change: LlmKeyChangeData,
    format: OutputFormat,
    verb: Verb,
) -> anyhow::Result<()> {
    match format {
        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&change)?),
        OutputFormat::Jsonl => println!("{}", serde_json::to_string(&change)?),
        _ => println!("{}", key_change_message(&change, verb)),
    }
    Ok(())
}

fn key_change_message(change: &LlmKeyChangeData, verb: Verb) -> String {
    let provider = provider_name(change.provider);
    let after = key_line(&change.after);
    match (verb, change.dry_run, change.changed) {
        (Verb::Store, true, _) => {
            format!("Dry run, nothing stored. Would store the {provider} key.\n{after}")
        }
        (Verb::Store, false, _) => format!("Stored the {provider} key.\n{after}"),
        (Verb::Remove, true, true) => {
            format!("Dry run, nothing removed. Would remove the stored {provider} key.\nAfterwards: {after}")
        }
        (Verb::Remove, false, true) => format!("Removed the stored {provider} key.\n{after}"),
        (Verb::Remove, _, false) => {
            format!("No stored {provider} key to remove.\n{after}")
        }
    }
}
