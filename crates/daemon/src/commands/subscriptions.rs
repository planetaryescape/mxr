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

fn render_table(subscriptions: &[SubscriptionSummary], rank: bool) {
    if subscriptions.is_empty() {
        println!("No subscriptions found.");
        return;
    }

    if rank {
        println!(
            "{:<32} {:<34} {:>6} {:>6} {:>10} {:<10}",
            "SENDER", "EMAIL", "COUNT", "OPENED", "ARCH/UNRD", "METHOD"
        );
        println!("{}", "-".repeat(102));
        for subscription in subscriptions {
            let method = method_label(&subscription.unsubscribe);
            println!(
                "{:<32} {:<34} {:>6} {:>6} {:>10} {:<10}",
                display_name(subscription),
                subscription.sender_email,
                subscription.message_count,
                subscription.opened_count,
                subscription.archived_unread_count,
                method,
            );
        }
        println!("\n{} senders ranked by open-rate (low first)", subscriptions.len());
        return;
    }

    println!(
        "{:<32} {:<34} {:>6} {:<10} {:<32}",
        "SENDER", "EMAIL", "COUNT", "METHOD", "LATEST SUBJECT"
    );
    println!("{}", "-".repeat(122));
    for subscription in subscriptions {
        let subject: String = subscription.latest_subject.chars().take(32).collect();
        let method = method_label(&subscription.unsubscribe);
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

fn method_label(method: &mxr_core::types::UnsubscribeMethod) -> &'static str {
    match method {
        mxr_core::types::UnsubscribeMethod::OneClick { .. } => "one-click",
        mxr_core::types::UnsubscribeMethod::HttpLink { .. } => "link",
        mxr_core::types::UnsubscribeMethod::Mailto { .. } => "mailto",
        mxr_core::types::UnsubscribeMethod::BodyLink { .. } => "body-link",
        mxr_core::types::UnsubscribeMethod::None => "-",
    }
}

/// Open-rate as a fraction in `[0, 1]`. Senders with no messages are pinned at 1.0
/// so they fall to the bottom of the rank rather than dominating it.
fn open_rate(s: &SubscriptionSummary) -> f64 {
    if s.message_count == 0 {
        1.0
    } else {
        f64::from(s.opened_count) / f64::from(s.message_count)
    }
}

pub async fn run(limit: u32, rank: bool, format: Option<OutputFormat>) -> anyhow::Result<()> {
    let mut client = IpcClient::connect().await?;
    let resp = client
        .request(Request::ListSubscriptions {
            account_id: None,
            limit,
        })
        .await?;

    let fmt = resolve_format(format);
    let mut subscriptions = crate::commands::expect_response(resp, |r| match r {
        Response::Ok {
            data: ResponseData::Subscriptions { subscriptions },
        } => Some(subscriptions),
        _ => None,
    })?;

    if rank {
        // Lowest open-rate first; ties broken by larger archived_unread first.
        subscriptions.sort_by(|a, b| {
            open_rate(a)
                .partial_cmp(&open_rate(b))
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(b.archived_unread_count.cmp(&a.archived_unread_count))
        });
    }

    match fmt {
        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&subscriptions)?),
        OutputFormat::Jsonl => println!("{}", jsonl(&subscriptions)?),
        _ => render_table(&subscriptions, rank),
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
            opened_count: 8,
            replied_count: 0,
            archived_unread_count: 2,
        }
    }

    #[test]
    fn table_render_handles_empty() {
        render_table(&[], false);
        render_table(&[], true);
    }

    #[test]
    fn rank_ordering_puts_low_open_rate_first() {
        // Senders ordered by open-rate ASC; tie-break by archived_unread DESC.
        let high_rate = SubscriptionSummary {
            opened_count: 9,
            archived_unread_count: 0,
            ..sample_subscription()
        };
        let mut low_rate = sample_subscription();
        low_rate.sender_email = "low@list.example.com".into();
        low_rate.opened_count = 1;
        low_rate.archived_unread_count = 5;
        let mut very_low = sample_subscription();
        very_low.sender_email = "verylow@list.example.com".into();
        very_low.opened_count = 0;
        very_low.archived_unread_count = 3;

        let mut subs = vec![high_rate.clone(), low_rate.clone(), very_low.clone()];
        subs.sort_by(|a, b| {
            open_rate(a)
                .partial_cmp(&open_rate(b))
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(b.archived_unread_count.cmp(&a.archived_unread_count))
        });
        assert_eq!(subs[0].sender_email, very_low.sender_email);
        assert_eq!(subs[1].sender_email, low_rate.sender_email);
        assert_eq!(subs[2].sender_email, high_rate.sender_email);
    }

    #[test]
    fn json_render_round_trips() {
        let rendered = serde_json::to_string(&vec![sample_subscription()]).unwrap();
        assert!(rendered.contains("readwise.io"));
        assert!(rendered.contains("weekly highlights"));
    }
}
