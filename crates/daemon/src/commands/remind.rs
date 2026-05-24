//! `mxr remind` — set or cancel an auto-reminder on an outbound
//! message. Reuses the conversational time parser from `mxr-core` so
//! `--when` accepts the same forms as `mxr snooze --until`.

use crate::ipc_client::IpcClient;
use mxr_core::id::MessageId;
use mxr_protocol::*;

pub async fn run(message_id: String, when: Option<String>, cancel: bool) -> anyhow::Result<()> {
    let id: MessageId = message_id
        .parse()
        .map_err(|e| anyhow::anyhow!("invalid message id `{message_id}`: {e}"))?;
    let mut client = IpcClient::connect().await?;

    if cancel {
        let resp = client
            .request(Request::CancelAutoReminder {
                sent_message_id: id,
            })
            .await?;
        ack_or_bail(resp, "Reminder cancelled")?;
        return Ok(());
    }

    let when =
        when.ok_or_else(|| anyhow::anyhow!("either --when <time> or --cancel must be supplied"))?;
    let remind_at = mxr_core::parse_relative_time(&when, chrono::Utc::now()).map_err(|e| {
        anyhow::anyhow!(
            "Cannot parse '{when}': {e}. Try: `in 2h`, `tomorrow 9am`, `monday 17:00`, or ISO 8601."
        )
    })?;

    let resp = client
        .request(Request::SetAutoReminder {
            sent_message_id: id,
            remind_at,
        })
        .await?;
    let pretty = remind_at
        .with_timezone(&chrono::Local)
        .format("%a %b %e %H:%M");
    ack_or_bail(resp, &format!("Reminder set for {pretty}"))
}

fn ack_or_bail(resp: Response, success_message: &str) -> anyhow::Result<()> {
    match resp {
        Response::Ok {
            data: ResponseData::Ack,
        } => {
            println!("{success_message}");
            Ok(())
        }
        Response::Error { message, .. } => anyhow::bail!("{message}"),
        _ => anyhow::bail!("Unexpected response"),
    }
}
