use crate::cli::OutputFormat;
use crate::ipc_client::IpcClient;
use crate::output::{jsonl, resolve_format};
use mxr_protocol::{Request, Response, ResponseData, SenderSummaryData};

fn display_name(sender: &SenderSummaryData) -> &str {
    sender
        .display_name
        .as_deref()
        .filter(|name| !name.trim().is_empty())
        .unwrap_or(sender.sender_email.as_str())
}

fn render_table(senders: &[SenderSummaryData]) {
    if senders.is_empty() {
        println!("No senders found.");
        return;
    }

    println!(
        "{:<32} {:<34} {:>6} {:>6} {:<32}",
        "SENDER", "EMAIL", "COUNT", "UNREAD", "LATEST SUBJECT"
    );
    println!("{}", "-".repeat(116));
    for sender in senders {
        let subject: String = sender.latest_subject.chars().take(32).collect();
        println!(
            "{:<32} {:<34} {:>6} {:>6} {:<32}",
            display_name(sender),
            sender.sender_email,
            sender.message_count,
            sender.unread_count,
            subject
        );
    }
}

pub async fn run(top: u32, format: Option<OutputFormat>) -> anyhow::Result<()> {
    let mut client = IpcClient::connect().await?;
    let resp = client.request(Request::ListSenders { limit: top }).await?;
    let senders = crate::commands::expect_response(resp, |r| match r {
        Response::Ok {
            data: ResponseData::Senders { senders },
        } => Some(senders),
        _ => None,
    })?;

    match resolve_format(format) {
        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&senders)?),
        OutputFormat::Jsonl => println!("{}", jsonl(&senders)?),
        _ => render_table(&senders),
    }

    Ok(())
}
