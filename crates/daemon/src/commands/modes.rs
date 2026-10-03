//! `mxr modes`: how each mode explains itself, from the daemon's one copy
//! table, and the first-encounter card's seen state.

use crate::cli::{ModesAction, OutputFormat};
use crate::commands::expect_response;
use crate::ipc_client::IpcClient;
use crate::output::{jsonl, resolve_format};
use mxr_protocol::{ModeGuideData, Request, Response, ResponseData};
use std::fmt::Write as _;

pub async fn run(action: ModesAction, format: Option<OutputFormat>) -> anyhow::Result<()> {
    let mut client = IpcClient::connect().await?;
    let format = resolve_format(format);
    let request = match action {
        ModesAction::Explain { mode } => Request::GetModeGuide { mode },
        ModesAction::Card { mode, show } => Request::SetModeGuideSeen { mode, seen: !show },
    };
    let guides = expect_response(client.request(request).await?, |response| match response {
        Response::Ok {
            data: ResponseData::ModeGuides { guides },
        } => Some(guides),
        _ => None,
    })?;
    match format {
        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&guides)?),
        OutputFormat::Jsonl => println!("{}", jsonl(&guides)?),
        _ => print!(
            "{}",
            guides.iter().map(guide_text).collect::<Vec<_>>().join("\n")
        ),
    }
    Ok(())
}

fn keys_line(keys: &[mxr_protocol::ModeKeyData]) -> String {
    keys.iter()
        .map(|key| format!("{} {}", key.key, key.verb))
        .collect::<Vec<_>>()
        .join(" · ")
}

fn guide_text(guide: &ModeGuideData) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "{}: {}", guide.name, guide.header);
    let _ = writeln!(out, "{}", guide.lands_here);
    let _ = writeln!(out, "\n{}", guide.card);
    let _ = writeln!(out, "{}", keys_line(&guide.card_keys));
    let _ = writeln!(out, "\nKeys: {}", keys_line(&guide.keys));
    let _ = writeln!(
        out,
        "First-encounter card: {}",
        if guide.card_seen {
            "retired (mxr modes card MODE --show brings it back)"
        } else {
            "shows the first time the mode has items"
        }
    );
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_leads_with_the_job_and_prints_keys_with_their_verbs() {
        let text = guide_text(&mxr_protocol::TODO_GUIDE.to_data(None));
        assert!(text.starts_with("To do: Things email asked you to do, ordered by when to act."));
        assert!(text.contains("Enter do it · e tick off · Z schedule · X not a to-do"));
        assert!(text.contains("shows the first time"));
    }
}
