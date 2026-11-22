use crate::cli::OutputFormat;
use crate::commands::resolve_optional_account;
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
        println!(
            "\n{} senders ranked by open-rate (low first)",
            subscriptions.len()
        );
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

pub async fn run(
    limit: u32,
    account: Option<String>,
    rank: bool,
    format: Option<OutputFormat>,
) -> anyhow::Result<()> {
    let mut client = IpcClient::connect().await?;
    let account_id = resolve_optional_account(&mut client, account.as_deref()).await?;
    let resp = client
        .request(Request::ListSubscriptions {
            account_id,
            limit,
            all_accounts: false,
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
