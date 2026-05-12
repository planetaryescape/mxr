use crate::cli::OutputFormat;
use crate::commands::resolve_account;
use crate::ipc_client::IpcClient;
use crate::output::resolve_format;
use mxr_protocol::*;

pub async fn run(
    email: String,
    account: Option<String>,
    rebuild: bool,
    format: Option<OutputFormat>,
) -> anyhow::Result<()> {
    let mut client = IpcClient::connect().await?;
    let account_id = resolve_account(&mut client, account.as_deref()).await?;
    let request = if rebuild {
        Request::RebuildRelationshipProfile {
            account_id,
            email: email.clone(),
        }
    } else {
        Request::GetRelationshipProfile {
            account_id,
            email: email.clone(),
        }
    };
    let resp = client.request(request).await?;
    let fmt = resolve_format(format);
    match resp {
        Response::Ok {
            data: ResponseData::RelationshipProfile { profile },
        } => match fmt {
            OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&profile)?),
            OutputFormat::Jsonl => println!("{}", serde_json::to_string(&profile)?),
            _ => print_profile(&email, profile),
        },
        Response::Error { message, .. } => anyhow::bail!(message),
        _ => anyhow::bail!("Unexpected response"),
    }
    Ok(())
}

fn print_profile(email: &str, profile: Option<RelationshipProfileData>) {
    let Some(profile) = profile else {
        println!("No relationship profile for {email}");
        return;
    };
    println!("{}", profile.email);
    if let Some(style) = profile.style {
        println!(
            "  voice: yours formality {:.2}, theirs {:.2}; avg sentence {:.1}/{:.1}",
            style.formality_score,
            style.formality_score_theirs,
            style.avg_sentence_len,
            style.avg_sentence_len_theirs
        );
    }
    if let Some(summary) = profile.summary {
        println!("  relationship: {}", summary.text);
        if !summary.known_topics.is_empty() {
            println!("  topics: {}", summary.known_topics.join(", "));
        }
    }
    if !profile.open_commitments.is_empty() {
        println!("  commitments:");
        for commitment in profile.open_commitments {
            println!(
                "    {} {:?}: {}",
                commitment.id, commitment.direction, commitment.what
            );
        }
    }
}
