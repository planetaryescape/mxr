#![cfg_attr(test, allow(clippy::panic, clippy::unwrap_used))]

use crate::cli::OutputFormat;
use crate::ipc_client::IpcClient;
use crate::output::{jsonl, resolve_format};
use mxr_core::types::SubscriptionSummary;
use mxr_protocol::{Request, Response, ResponseData};

fn display_name(subscription: &SubscriptionSummary) -> &str {
    subscription
        .sender_name
        .as_deref()
        .filter(|name| !name.trim().is_empty())
        .unwrap_or(subscription.sender_email.as_str())
}

fn render_table(subscriptions: &[SubscriptionSummary]) {
    if subscriptions.is_empty() {
        println!("No subscriptions found.");
        return;
    }

    println!(
        "{:<32} {:<34} {:>6} {:<10} {:<32}",
        "SENDER", "EMAIL", "COUNT", "METHOD", "LATEST SUBJECT"
    );
    println!("{}", "-".repeat(122));
    for subscription in subscriptions {
        let subject: String = subscription.latest_subject.chars().take(32).collect();
        let method = match &subscription.unsubscribe {
            mxr_core::types::UnsubscribeMethod::OneClick { .. } => "one-click",
            mxr_core::types::UnsubscribeMethod::HttpLink { .. } => "link",
            mxr_core::types::UnsubscribeMethod::Mailto { .. } => "mailto",
            mxr_core::types::UnsubscribeMethod::BodyLink { .. } => "body-link",
            mxr_core::types::UnsubscribeMethod::None => "-",
        };
        println!(
            "{:<32} {:<34} {:>6} {:<10} {:<32}",
            display_name(subscription),
            subscription.sender_email,
            subscription.message_count,
            method,
            subject
        );
    }
    println!("\n{} senders", subscriptions.len());
}

pub async fn run(limit: u32, format: Option<OutputFormat>) -> anyhow::Result<()> {
    let mut client = IpcClient::connect().await?;
    let resp = client
        .request(Request::ListSubscriptions {
            account_id: None,
            limit,
        })
        .await?;

    let fmt = resolve_format(format);
    let subscriptions = crate::commands::expect_response(resp, |r| match r {
        Response::Ok {
            data: ResponseData::Subscriptions { subscriptions },
        } => Some(subscriptions),
        _ => None,
    })?;
    match fmt {
        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&subscriptions)?),
        OutputFormat::Jsonl => println!("{}", jsonl(&subscriptions)?),
        _ => render_table(&subscriptions),
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};
    use mxr_core::id::{AccountId, MessageId, ThreadId};
    use mxr_core::types::{MessageFlags, UnsubscribeMethod};

    fn sample_subscription() -> SubscriptionSummary {
        SubscriptionSummary {
            account_id: AccountId::new(),
            sender_name: Some("Readwise".into()),
            sender_email: "hello@readwise.io".into(),
            message_count: 12,
            latest_message_id: MessageId::new(),
            latest_provider_id: "provider-1".into(),
            latest_thread_id: ThreadId::new(),
            latest_subject: "Your weekly highlights".into(),
            latest_snippet: "Snippet".into(),
            latest_date: Utc.with_ymd_and_hms(2026, 3, 21, 10, 0, 0).unwrap(),
            latest_flags: MessageFlags::READ,
            latest_has_attachments: false,
            latest_size_bytes: 1024,
            unsubscribe: UnsubscribeMethod::HttpLink {
                url: "https://example.com/unsub".into(),
            },
        }
    }

    #[test]
    fn table_render_handles_empty() {
        render_table(&[]);
    }

    #[test]
    fn json_render_round_trips() {
        let rendered = serde_json::to_string(&vec![sample_subscription()]).unwrap();
        assert!(rendered.contains("readwise.io"));
        assert!(rendered.contains("weekly highlights"));
    }
}
