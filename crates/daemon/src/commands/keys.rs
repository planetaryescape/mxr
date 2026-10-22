//! `mxr keys`: every keyboard shortcut in every client, from the daemon's
//! one key definition (blueprint chapter 35).

use crate::cli::{KeyClientArg, KeysAction, OutputFormat};
use crate::commands::expect_response;
use crate::ipc_client::IpcClient;
use crate::output::{jsonl, resolve_format};
use mxr_protocol::{KeymapData, Request, Response, ResponseData};
use serde::Serialize;

/// One key in one place in one client.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct KeyRow {
    pub client: &'static str,
    /// A web scope or a TUI context.
    pub context: String,
    pub key: String,
    pub action: String,
    pub label: String,
    /// The TUI action its dispatcher returns.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tui_action: Option<String>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub mac_only: bool,
}

pub async fn run(
    action: Option<KeysAction>,
    client: Option<KeyClientArg>,
    context: Option<String>,
    format: Option<OutputFormat>,
) -> anyhow::Result<()> {
    let mut ipc = IpcClient::connect().await?;
    let keymap = expect_response(
        ipc.request(Request::GetKeymap).await?,
        |response| match response {
            Response::Ok {
                data: ResponseData::Keymap { keymap },
            } => Some(keymap),
            _ => None,
        },
    )?;
    let rows = match action {
        None => list(&keymap, client, context.as_deref()),
        Some(KeysAction::Explain { key }) => explain(&keymap, &key, client, context.as_deref()),
        Some(KeysAction::Check { file }) => return check(file, format),
    };
    match resolve_format(format) {
        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&rows)?),
        OutputFormat::Jsonl => println!("{}", jsonl(&rows)?),
        _ if rows.is_empty() => println!("No keys match."),
        _ => print!("{}", table(&rows)),
    }
    Ok(())
}

#[derive(Serialize)]
struct CheckReport {
    file: std::path::PathBuf,
    exists: bool,
    problems: Vec<String>,
}

/// `mxr keys check`: everything in keys.toml mxr would ignore. Reads the
/// file directly, so it works without a daemon.
pub fn check(file: Option<std::path::PathBuf>, format: Option<OutputFormat>) -> anyhow::Result<()> {
    let file = file.unwrap_or_else(|| {
        mxr_config::config_file_path()
            .parent()
            .map_or_else(|| "keys.toml".into(), |dir| dir.join("keys.toml"))
    });
    let text = match std::fs::read_to_string(&file) {
        Ok(text) => Some(text),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => return Err(anyhow::anyhow!("can't read {}: {error}", file.display())),
    };
    let report = CheckReport {
        exists: text.is_some(),
        problems: text
            .as_deref()
            .map(mxr_tui::keybindings::check_keys_toml)
            .unwrap_or_default(),
        file,
    };
    match resolve_format(format) {
        OutputFormat::Json | OutputFormat::Jsonl => {
            println!("{}", serde_json::to_string_pretty(&report)?);
        }
        _ if !report.exists => println!("No {}; every key is the default.", report.file.display()),
        _ if report.problems.is_empty() => println!("{} is fine.", report.file.display()),
        _ => {
            println!("{}:", report.file.display());
            for problem in &report.problems {
                println!("  {problem}");
            }
        }
    }
    if !report.problems.is_empty() {
        anyhow::bail!(
            "{} {} in {}",
            report.problems.len(),
            if report.problems.len() == 1 {
                "problem"
            } else {
                "problems"
            },
            report.file.display()
        );
    }
    Ok(())
}

fn all_rows(keymap: &KeymapData) -> Vec<KeyRow> {
    let mut rows = Vec::new();
    for action in &keymap.actions {
        for binding in &action.tui {
            for context in &binding.contexts {
                for key in &binding.keys {
                    rows.push(KeyRow {
                        client: "tui",
                        context: context.clone(),
                        key: key.clone(),
                        action: action.id.clone(),
                        label: action.label.clone(),
                        tui_action: Some(binding.name.clone()),
                        mac_only: false,
                    });
                }
            }
        }
        for scope in &action.scopes {
            let keys = action.keys.iter().map(|k| (k, false));
            for (key, mac_only) in keys.chain(action.mac_keys.iter().map(|k| (k, true))) {
                rows.push(KeyRow {
                    client: "web",
                    context: scope.clone(),
                    key: key.clone(),
                    action: action.id.clone(),
                    label: action.label.clone(),
                    tui_action: None,
                    mac_only,
                });
            }
        }
    }
    rows.sort_by(|a, b| (a.client, &a.context, &a.key).cmp(&(b.client, &b.context, &b.key)));
    rows
}

fn client_name(client: KeyClientArg) -> &'static str {
    match client {
        KeyClientArg::Tui => "tui",
        KeyClientArg::Web => "web",
    }
}

pub(crate) fn list(
    keymap: &KeymapData,
    client: Option<KeyClientArg>,
    context: Option<&str>,
) -> Vec<KeyRow> {
    all_rows(keymap)
        .into_iter()
        .filter(|row| client.is_none_or(|c| row.client == client_name(c)))
        .filter(|row| context.is_none_or(|c| row.context == c))
        .collect()
}

/// What `key` does. With a context, the web answer is the binding that
/// wins there: the context's own scope first, then global (the screener
/// swallows global keys, as the web dispatcher's scope stack does).
pub(crate) fn explain(
    keymap: &KeymapData,
    key: &str,
    client: Option<KeyClientArg>,
    context: Option<&str>,
) -> Vec<KeyRow> {
    let matching: Vec<KeyRow> = list(keymap, client, None)
        .into_iter()
        .filter(|row| row.key == key)
        .collect();
    let Some(context) = context else {
        return matching;
    };
    let mut rows: Vec<KeyRow> = matching
        .iter()
        .filter(|row| row.client == "tui" && row.context == context)
        .cloned()
        .collect();
    let web_here = matching
        .iter()
        .filter(|row| row.client == "web" && row.context == context);
    let web_global = matching
        .iter()
        .filter(|row| row.client == "web" && row.context == "global" && context != "screener");
    let mut web: Vec<KeyRow> = web_here.cloned().collect();
    if web.is_empty() {
        web = web_global.cloned().collect();
    }
    rows.extend(web);
    rows
}

fn table(rows: &[KeyRow]) -> String {
    let width = |f: fn(&KeyRow) -> usize| rows.iter().map(f).max().unwrap_or(0);
    let (cw, kw, aw) = (
        width(|r| r.context.len()),
        width(|r| r.key.chars().count()),
        width(|r| r.action.len()),
    );
    rows.iter()
        .map(|row| {
            let mut note = String::new();
            if let Some(name) = &row.tui_action {
                note = format!("  ({name})");
            }
            if row.mac_only {
                note.push_str("  (macOS)");
            }
            format!(
                "{:<3}  {:<cw$}  {:<kw$}  {:<aw$}  {}{note}\n",
                row.client, row.context, row.key, row.action, row.label
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use mxr_protocol::{KeyActionData, TuiKeysData};

    fn action(
        id: &str,
        scopes: &[&str],
        keys: &[&str],
        tui: &[(&str, &str, &[&str])],
    ) -> KeyActionData {
        let strings = |items: &[&str]| items.iter().map(|s| (*s).to_string()).collect();
        KeyActionData {
            id: id.into(),
            label: id.into(),
            description: None,
            group: "Mail".into(),
            clients: vec!["tui".into(), "web".into()],
            scopes: strings(scopes),
            keys: strings(keys),
            mac_keys: Vec::new(),
            palette_only: false,
            note: None,
            tui: tui
                .iter()
                .map(|(name, key, contexts)| TuiKeysData {
                    name: (*name).into(),
                    keys: vec![(*key).into()],
                    contexts: strings(contexts),
                })
                .collect(),
        }
    }

    fn fixture() -> KeymapData {
        KeymapData {
            actions: vec![
                action(
                    "mail.archive",
                    &["list"],
                    &["e"],
                    &[("Archive", "e", &["list"])],
                ),
                action(
                    "todo.done",
                    &["todo"],
                    &["e"],
                    &[("TodoDone", "e", &["todo"])],
                ),
                action(
                    "shell.help",
                    &["global"],
                    &["?"],
                    &[("Help", "?", &["list", "todo"])],
                ),
            ],
        }
    }

    #[test]
    fn explain_answers_for_the_context_in_both_clients() {
        let rows = explain(&fixture(), "e", None, Some("todo"));
        let got: Vec<_> = rows.iter().map(|r| (r.client, r.action.as_str())).collect();
        assert_eq!(got, vec![("tui", "todo.done"), ("web", "todo.done")]);
    }

    #[test]
    fn a_global_web_key_answers_in_a_context_without_its_own() {
        let rows = explain(&fixture(), "?", Some(KeyClientArg::Web), Some("todo"));
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].action, "shell.help");
        assert!(explain(&fixture(), "?", Some(KeyClientArg::Web), Some("screener")).is_empty());
    }

    #[test]
    fn list_filters_by_client_and_context() {
        let rows = list(&fixture(), Some(KeyClientArg::Tui), Some("list"));
        let got: Vec<_> = rows.iter().map(|r| r.key.as_str()).collect();
        assert_eq!(got, vec!["?", "e"]);
    }
}
