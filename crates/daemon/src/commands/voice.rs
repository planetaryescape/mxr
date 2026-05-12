use crate::cli::{OutputFormat, VoiceAction};
use crate::commands::resolve_account;
use crate::ipc_client::IpcClient;
use crate::output::resolve_format;
use mxr_protocol::*;

pub async fn run(
    action: Option<VoiceAction>,
    account: Option<String>,
    format: Option<OutputFormat>,
) -> anyhow::Result<()> {
    let mut client = IpcClient::connect().await?;
    let account_id = resolve_account(&mut client, account.as_deref()).await?;
    let request = match action.unwrap_or(VoiceAction::Show) {
        VoiceAction::Show => Request::GetUserVoice { account_id },
        VoiceAction::Rebuild => Request::RebuildUserVoice { account_id },
    };
    let resp = client.request(request).await?;
    match resp {
        Response::Ok {
            data: ResponseData::UserVoice { profile },
        } => print_voice(profile, resolve_format(format))?,
        Response::Ok {
            data: ResponseData::Ack,
        } => println!("rebuild queued"),
        Response::Error { message, .. } => anyhow::bail!(message),
        _ => anyhow::bail!("Unexpected response"),
    }
    Ok(())
}

fn print_voice(profile: Option<UserVoiceProfileData>, fmt: OutputFormat) -> anyhow::Result<()> {
    match fmt {
        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&profile)?),
        OutputFormat::Jsonl => println!("{}", serde_json::to_string(&profile)?),
        _ => {
            let Some(profile) = profile else {
                println!("No user voice profile yet");
                return Ok(());
            };
            println!(
                "voice: formality {:.2}, avg sentence {:.1}, messages {}",
                profile.formality_score, profile.avg_sentence_len, profile.msg_count_used
            );
            for mode in profile.register_modes {
                println!(
                    "  {:?}: formality {:.2}, avg sentence {:.1}",
                    mode.register, mode.formality_score, mode.avg_sentence_len
                );
            }
        }
    }
    Ok(())
}
