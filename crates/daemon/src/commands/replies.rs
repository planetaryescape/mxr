//! `mxr replies` — manage the reply-later queue.
//!
//! The default invocation lists currently-flagged messages. Subcommands
//! mark and unmark individual messages by ID. Local-only state — flags
//! never roundtrip to the provider.

use crate::cli::{OutputFormat, RepliesAction};
use crate::ipc_client::IpcClient;
use crate::output::{jsonl, resolve_format};
use mxr_core::id::MessageId;
use mxr_core::Envelope;
use mxr_protocol::*;
use std::io::Write;

pub async fn run(
    action: Option<RepliesAction>,
    format: Option<OutputFormat>,
) -> anyhow::Result<()> {
    let action = action.unwrap_or(RepliesAction::List);
    let mut client = IpcClient::connect().await?;

    match action {
        RepliesAction::List => {
            let messages = list_reply_queue(&mut client).await?;
            let fmt = resolve_format(format);
            match fmt {
                OutputFormat::Json => {
                    println!("{}", serde_json::to_string_pretty(&messages)?);
                }
                OutputFormat::Jsonl => {
                    println!("{}", jsonl(&messages)?);
                }
                _ => {
                    if messages.is_empty() {
                        println!("Reply-later queue is empty");
                    } else {
                        for env in &messages {
                            print_reply_queue_row(env);
                        }
                    }
                }
            }
        }
        RepliesAction::Walk => walk_reply_queue(&mut client).await?,
        RepliesAction::Add { message_id } => {
            let id = parse_message_id(&message_id)?;
            let resp = client
                .request(Request::SetReplyLater {
                    message_id: id,
                    flag: true,
                })
                .await?;
            ack_or_bail(resp, "Marked for reply later")?;
        }
        RepliesAction::Remove { message_id } => {
            let id = parse_message_id(&message_id)?;
            let resp = client
                .request(Request::SetReplyLater {
                    message_id: id,
                    flag: false,
                })
                .await?;
            ack_or_bail(resp, "Cleared reply-later flag")?;
        }
    }

    Ok(())
}

async fn list_reply_queue(client: &mut IpcClient) -> anyhow::Result<Vec<Envelope>> {
    let resp = client.request(Request::ListReplyQueue).await?;
    match resp {
        Response::Ok {
            data: ResponseData::ReplyQueue { messages },
        } => Ok(messages),
        Response::Error { message, .. } => anyhow::bail!("{}", message),
        _ => anyhow::bail!("Unexpected response"),
    }
}

async fn walk_reply_queue(client: &mut IpcClient) -> anyhow::Result<()> {
    let mut messages = list_reply_queue(client).await?;
    if messages.is_empty() {
        println!("Reply-later queue is empty");
        return Ok(());
    }

    let mut index = 0;
    while index < messages.len() {
        let env = messages[index].clone();
        println!();
        println!("[{}/{}] {}", index + 1, messages.len(), env.subject.trim());
        print_reply_queue_row(&env);
        if !env.snippet.trim().is_empty() {
            println!("  {}", env.snippet.trim());
        }
        println!("Actions: [r]eply  [c]lear  [s]kip  [q]uit");
        print!("reply> ");
        std::io::stdout().flush()?;

        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;
        match parse_walk_action(input.trim()) {
            WalkAction::Reply => {
                crate::commands::mutations::reply(crate::commands::mutations::ReplyCommand {
                    message_id: env.id.to_string(),
                    body: None,
                    body_stdin: false,
                    signature: None,
                    no_signature: false,
                    yes: false,
                    dry_run: false,
                    remind_after: None,
                    format: None,
                })
                .await?;
                clear_reply_later(client, &env.id).await?;
                apply_walk_action(&mut messages, &mut index, WalkAction::Reply);
            }
            WalkAction::Clear => {
                clear_reply_later(client, &env.id).await?;
                println!("Cleared reply-later flag");
                apply_walk_action(&mut messages, &mut index, WalkAction::Clear);
            }
            WalkAction::Skip => apply_walk_action(&mut messages, &mut index, WalkAction::Skip),
            WalkAction::Quit => break,
            WalkAction::Unknown(other) => {
                println!("Unknown action `{other}`");
            }
        }
    }

    Ok(())
}

/// Decoded user input for an iteration of `mxr replies walk`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum WalkAction {
    Reply,
    Clear,
    Skip,
    Quit,
    Unknown(String),
}

pub(crate) fn parse_walk_action(input: &str) -> WalkAction {
    match input.to_ascii_lowercase().as_str() {
        "r" | "reply" => WalkAction::Reply,
        "c" | "clear" => WalkAction::Clear,
        "" | "s" | "skip" => WalkAction::Skip,
        "q" | "quit" => WalkAction::Quit,
        other => WalkAction::Unknown(other.to_string()),
    }
}

/// Apply one walk-mode action to the local queue snapshot.
///
/// `Reply` and `Clear` remove the current entry — the daemon-side
/// flag clear is the source of truth for the queue, but the local
/// snapshot must reflect it so the user keeps walking forward instead
/// of re-prompting on the same message. `Skip` leaves the entry but
/// advances the cursor so the user can come back to it later (the
/// flag stays set in the store). `Quit` and `Unknown` are no-ops on
/// the queue.
pub(crate) fn apply_walk_action(queue: &mut Vec<Envelope>, index: &mut usize, action: WalkAction) {
    if *index >= queue.len() {
        return;
    }
    match action {
        WalkAction::Reply | WalkAction::Clear => {
            queue.remove(*index);
        }
        WalkAction::Skip => {
            *index += 1;
        }
        WalkAction::Quit | WalkAction::Unknown(_) => {}
    }
}

async fn clear_reply_later(client: &mut IpcClient, message_id: &MessageId) -> anyhow::Result<()> {
    let resp = client
        .request(Request::SetReplyLater {
            message_id: message_id.clone(),
            flag: false,
        })
        .await?;
    ack_or_bail(resp, "")
}

fn print_reply_queue_row(env: &Envelope) {
    let from = env.from.name.as_deref().unwrap_or(&env.from.email);
    println!("  {}  {}  {}", env.id.as_str(), from, env.subject);
}

fn parse_message_id(input: &str) -> anyhow::Result<MessageId> {
    input
        .parse::<MessageId>()
        .map_err(|e| anyhow::anyhow!("invalid message id `{input}`: {e}"))
}

fn ack_or_bail(resp: Response, success_message: &str) -> anyhow::Result<()> {
    match resp {
        Response::Ok {
            data: ResponseData::Ack,
        } => {
            if !success_message.is_empty() {
                println!("{success_message}");
            }
            Ok(())
        }
        Response::Error { message, .. } => anyhow::bail!("{}", message),
        _ => anyhow::bail!("Unexpected response"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};
    use mxr_core::id::{AccountId, ThreadId};
    use mxr_core::types::{Address, MessageFlags, UnsubscribeMethod};

    fn walk_envelope(slug: &str) -> Envelope {
        Envelope {
            id: MessageId::new(),
            account_id: AccountId::new(),
            provider_id: slug.into(),
            thread_id: ThreadId::new(),
            message_id_header: Some(format!("<{slug}@example.com>")),
            in_reply_to: None,
            references: vec![],
            from: Address {
                name: Some(format!("Sender {slug}")),
                email: format!("{slug}@example.com"),
            },
            to: vec![Address {
                name: Some("Me".into()),
                email: "me@example.com".into(),
            }],
            cc: vec![],
            bcc: vec![],
            subject: format!("subject {slug}"),
            date: Utc.with_ymd_and_hms(2024, 3, 15, 9, 0, 0).unwrap(),
            flags: MessageFlags::READ,
            snippet: format!("snippet {slug}"),
            has_attachments: false,
            size_bytes: 100,
            unsubscribe: UnsubscribeMethod::None,
            link_count: 0,
            body_word_count: 0,
            label_provider_ids: vec!["INBOX".into()],
            keywords: std::collections::BTreeSet::new(),
        }
    }

    /// Phase 2.1: after a send, walk mode drops the entry from the
    /// local queue snapshot so the next prompt targets the NEXT
    /// message, not the one we just replied to. The store-side flag
    /// clear is handled separately; this tests the local-state
    /// invariant.
    #[test]
    fn walk_mode_advances_after_send() {
        let mut queue = vec![
            walk_envelope("first"),
            walk_envelope("second"),
            walk_envelope("third"),
        ];
        let mut index = 0;

        apply_walk_action(&mut queue, &mut index, WalkAction::Reply);

        assert_eq!(queue.len(), 2, "the replied-to message is removed");
        assert_eq!(
            queue[0].provider_id, "second",
            "walking forward lands on the next queued message"
        );
        assert_eq!(
            index, 0,
            "index stays at 0 — the removed slot is now occupied by what was next"
        );
    }

    /// Phase 2.1: skipping leaves the message in the queue (so the
    /// user can return) but advances the cursor so the same message
    /// isn't re-prompted on the next loop iteration.
    #[test]
    fn walk_mode_advances_after_skip() {
        let mut queue = vec![walk_envelope("first"), walk_envelope("second")];
        let mut index = 0;

        apply_walk_action(&mut queue, &mut index, WalkAction::Skip);

        assert_eq!(queue.len(), 2, "skip preserves the message in the queue");
        assert_eq!(index, 1, "cursor advanced to the next message");
        assert_eq!(
            queue[index].provider_id, "second",
            "after skip, the next prompt is for the next message"
        );
    }

    /// Phase 2.1: a Clear action (user said "actually, never mind on
    /// this one") drops the entry the same as a Reply. The daemon-
    /// side clear is logged separately; local state must reflect
    /// reality.
    #[test]
    fn walk_mode_advances_after_clear() {
        let mut queue = vec![walk_envelope("first"), walk_envelope("second")];
        let mut index = 0;

        apply_walk_action(&mut queue, &mut index, WalkAction::Clear);

        assert_eq!(queue.len(), 1);
        assert_eq!(queue[0].provider_id, "second");
        assert_eq!(index, 0);
    }

    /// Out-of-range index is a safe no-op — the walk loop exits
    /// naturally on the next iteration when `index >= queue.len()`.
    #[test]
    fn walk_mode_action_at_out_of_range_index_is_noop() {
        let mut queue = vec![walk_envelope("only")];
        let mut index = 5;

        apply_walk_action(&mut queue, &mut index, WalkAction::Reply);

        assert_eq!(queue.len(), 1);
        assert_eq!(index, 5);
    }

    #[test]
    fn parse_walk_action_recognizes_each_form() {
        assert_eq!(parse_walk_action("r"), WalkAction::Reply);
        assert_eq!(parse_walk_action("reply"), WalkAction::Reply);
        assert_eq!(parse_walk_action("c"), WalkAction::Clear);
        assert_eq!(parse_walk_action("clear"), WalkAction::Clear);
        assert_eq!(parse_walk_action(""), WalkAction::Skip);
        assert_eq!(parse_walk_action("s"), WalkAction::Skip);
        assert_eq!(parse_walk_action("q"), WalkAction::Quit);
        assert_eq!(parse_walk_action("?"), WalkAction::Unknown("?".to_string()));
    }
}
