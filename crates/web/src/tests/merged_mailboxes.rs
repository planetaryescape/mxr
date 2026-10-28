//! "All accounts" with a Gmail-shaped default account and an IMAP-shaped
//! second account: every row is judged by its own account's labels, the
//! sidebar is built from mailbox roles, and counts come from the daemon.

use super::*;
use mxr_core::{LabelId, LabelKind};
use std::sync::Arc;

struct World {
    gmail: AccountId,
    imap: AccountId,
    gmail_labels: Vec<Label>,
    imap_labels: Vec<Label>,
}

fn system_label(account: &AccountId, name: &str, provider_id: &str) -> Label {
    Label {
        id: LabelId::from_scoped_provider_id(account, "test", provider_id),
        account_id: account.clone(),
        name: name.into(),
        kind: LabelKind::System,
        color: None,
        provider_id: provider_id.into(),
        unread_count: 1,
        total_count: 10,
        role: None,
    }
}

impl World {
    /// Gmail names its system labels `INBOX`/`SENT`/`DRAFT`/`STARRED`; the
    /// IMAP adapter maps special-use folders to `INBOX`/`SENT`/`SPAM` names
    /// with the folder's own name as provider id, and has no STARRED label.
    fn new() -> Self {
        let gmail = AccountId::new();
        let imap = AccountId::new();
        let gmail_labels = vec![
            system_label(&gmail, "INBOX", "INBOX"),
            system_label(&gmail, "SENT", "SENT"),
            system_label(&gmail, "DRAFT", "DRAFT"),
            system_label(&gmail, "STARRED", "STARRED"),
        ];
        let imap_labels = vec![
            system_label(&imap, "INBOX", "INBOX"),
            system_label(&imap, "SENT", "Sent Items"),
            system_label(&imap, "SPAM", "Junk"),
        ];
        Self {
            gmail,
            imap,
            gmail_labels,
            imap_labels,
        }
    }

    fn gmail_label(&self, name: &str) -> &Label {
        self.gmail_labels.iter().find(|l| l.name == name).unwrap()
    }
}

fn envelope(account: &AccountId, thread: &ThreadId, subject: &str, minutes_ago: i64) -> Envelope {
    let mut envelope = sample_envelope();
    envelope.account_id = account.clone();
    envelope.thread_id = thread.clone();
    envelope.subject = subject.into();
    envelope.date = Utc::now() - chrono::Duration::minutes(minutes_ago);
    envelope
}

fn thread_of(account: &AccountId, envelopes: &[&Envelope]) -> Thread {
    let mut thread = sample_thread(envelopes[0]);
    thread.account_id = account.clone();
    thread.message_count = envelopes.len() as u32;
    thread.message_ids = envelopes.iter().map(|e| e.id.clone()).collect();
    thread
}

async fn serve_world(
    socket_path: PathBuf,
    world: &World,
    envelopes: Vec<Envelope>,
    threads: Vec<Thread>,
    counts: Vec<mxr_protocol::SystemMailboxCount>,
) -> (tokio::task::JoinHandle<()>, std::net::SocketAddr) {
    let default_labels = world.gmail_labels.clone();
    let all_labels = world
        .gmail_labels
        .iter()
        .chain(world.imap_labels.iter())
        .cloned()
        .collect::<Vec<_>>();
    let ipc = spawn_fake_ipc_server(
        &socket_path,
        move |request| {
            let data = match request {
                Request::GetStatus => sample_status(),
                Request::ListLabels { all_accounts, .. } => ResponseData::Labels {
                    labels: if all_accounts {
                        all_labels.clone()
                    } else {
                        default_labels.clone()
                    },
                },
                Request::SystemMailboxCounts { .. } => ResponseData::SystemMailboxCounts {
                    mailboxes: counts.clone(),
                },
                Request::ListCategories { .. } => ResponseData::Categories {
                    categories: Vec::new(),
                },
                Request::ListSavedSearches => ResponseData::SavedSearches {
                    searches: Vec::new(),
                },
                Request::ListSubscriptions { .. } => ResponseData::Subscriptions {
                    subscriptions: Vec::new(),
                },
                Request::ListThreads { .. } => ResponseData::Threads {
                    threads: threads.clone(),
                },
                Request::ListEnvelopes { .. } => ResponseData::Envelopes {
                    envelopes: envelopes.clone(),
                },
                Request::ListEnvelopesByIds { message_ids } => ResponseData::Envelopes {
                    envelopes: envelopes
                        .iter()
                        .filter(|e| message_ids.contains(&e.id))
                        .cloned()
                        .collect(),
                },
                Request::ListCommitments { .. } => ResponseData::CommitmentList {
                    commitments: Vec::new(),
                },
                other => return Some(Response::error(format!("unexpected: {other:?}"))),
            };
            Some(Response::Ok { data })
        },
        None,
    )
    .await;
    let addr = bind_and_serve(
        std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
        0,
        WebServerConfig::new(socket_path, TEST_AUTH_TOKEN.into()),
    )
    .await
    .unwrap();
    (ipc, addr)
}

async fn get_json(addr: std::net::SocketAddr, query: &str) -> serde_json::Value {
    let response = test_http_client()
        .get(format!("http://{addr}/mailbox?{query}"))
        .header("x-mxr-bridge-token", TEST_AUTH_TOKEN)
        .send()
        .await
        .unwrap();
    let status = response.status();
    let body = response.text().await.unwrap();
    assert_eq!(status, reqwest::StatusCode::OK, "{query}: {body}");
    serde_json::from_str(&body).unwrap()
}

fn rows(json: &serde_json::Value) -> Vec<serde_json::Value> {
    json["mailbox"]["groups"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|group| group["rows"].as_array().unwrap().iter().cloned())
        .collect()
}

#[tokio::test]
async fn trashing_a_sent_row_targets_only_the_sent_message_on_another_provider() {
    let temp = TempDir::new().unwrap();
    let world = World::new();
    // The IMAP thread holds an older message in "Sent Items" and a newer
    // reply in the inbox. The default (Gmail) SENT label is `SENT`, which
    // matches neither.
    let thread = ThreadId::new();
    let mut sent = envelope(&world.imap, &thread, "Sent note", 30);
    sent.label_provider_ids = vec!["Sent Items".into()];
    let mut reply = envelope(&world.imap, &thread, "Re: Sent note", 5);
    reply.label_provider_ids = vec!["INBOX".into()];
    let gmail_thread = ThreadId::new();
    let mut gmail_sent = envelope(&world.gmail, &gmail_thread, "Gmail sent", 10);
    gmail_sent.label_provider_ids = vec!["SENT".into()];
    let threads = vec![
        thread_of(&world.gmail, &[&gmail_sent]),
        thread_of(&world.imap, &[&sent, &reply]),
    ];
    let (_ipc, addr) = serve_world(
        temp.path().join("mxr.sock"),
        &world,
        vec![sent.clone(), reply.clone(), gmail_sent.clone()],
        threads,
        Vec::new(),
    )
    .await;

    let sent_label = world.gmail_label("SENT").id.clone();
    let json = get_json(
        addr,
        &format!("lens_kind=label&label_id={sent_label}&view=threads"),
    )
    .await;
    let rows = rows(&json);
    assert_eq!(rows.len(), 2);
    let ids = |row: &serde_json::Value| -> Vec<String> {
        row["message_ids"]
            .as_array()
            .unwrap()
            .iter()
            .map(|id| id.as_str().unwrap().to_string())
            .collect()
    };
    let imap_row = rows
        .iter()
        .find(|row| row["account_id"] == world.imap.to_string())
        .unwrap();
    assert_eq!(
        ids(imap_row),
        vec![sent.id.to_string()],
        "a row action must not reach the inbox reply"
    );
    let gmail_row = rows
        .iter()
        .find(|row| row["account_id"] == world.gmail.to_string())
        .unwrap();
    assert_eq!(ids(gmail_row), vec![gmail_sent.id.to_string()]);
}

#[tokio::test]
async fn a_role_view_never_falls_back_to_the_whole_thread() {
    let temp = TempDir::new().unwrap();
    let world = World::new();
    // Listed by the daemon, but no message carries a Sent label of its own
    // account: the row is dropped instead of targeting everything.
    let thread = ThreadId::new();
    let mut unlabelled = envelope(&world.imap, &thread, "Not sent", 5);
    unlabelled.label_provider_ids = vec!["INBOX".into()];
    let (_ipc, addr) = serve_world(
        temp.path().join("mxr.sock"),
        &world,
        vec![unlabelled.clone()],
        vec![thread_of(&world.imap, &[&unlabelled])],
        Vec::new(),
    )
    .await;
    let sent_label = world.gmail_label("SENT").id.clone();
    let json = get_json(
        addr,
        &format!("lens_kind=label&label_id={sent_label}&view=threads"),
    )
    .await;
    assert!(rows(&json).is_empty());
}

#[tokio::test]
async fn an_imap_star_is_a_flag_so_the_starred_row_targets_the_flagged_message() {
    let temp = TempDir::new().unwrap();
    let world = World::new();
    let thread = ThreadId::new();
    let mut starred = envelope(&world.imap, &thread, "Flagged", 30);
    starred.flags = MessageFlags::STARRED;
    starred.label_provider_ids = vec!["INBOX".into()];
    let mut plain = envelope(&world.imap, &thread, "Re: Flagged", 5);
    plain.label_provider_ids = vec!["INBOX".into()];
    let (_ipc, addr) = serve_world(
        temp.path().join("mxr.sock"),
        &world,
        vec![starred.clone(), plain.clone()],
        vec![thread_of(&world.imap, &[&starred, &plain])],
        Vec::new(),
    )
    .await;
    let starred_label = world.gmail_label("STARRED").id.clone();
    let json = get_json(
        addr,
        &format!("lens_kind=label&label_id={starred_label}&view=threads"),
    )
    .await;
    let rows = rows(&json);
    assert_eq!(rows.len(), 1);
    assert_eq!(
        rows[0]["message_ids"],
        serde_json::json!([starred.id.to_string()])
    );
}

#[tokio::test]
async fn navigation_comes_from_roles_and_counts_from_the_daemon() {
    let temp = TempDir::new().unwrap();
    let world = World::new();
    let counts = [
        ("inbox", 5, 50),
        ("sent", 2, 20),
        ("drafts", 0, 3),
        ("spam", 1, 4),
        ("all_mail", 9, 9124),
    ]
    .map(|(role, unread, total)| mxr_protocol::SystemMailboxCount {
        role: role.into(),
        unread,
        total,
    })
    .to_vec();
    let (_ipc, addr) = serve_world(
        temp.path().join("mxr.sock"),
        &world,
        Vec::new(),
        Vec::new(),
        counts,
    )
    .await;
    let json = get_json(addr, "view=messages").await;
    let items = json["sidebar"]["sections"][0]["items"].as_array().unwrap();
    let item = |label: &str| items.iter().find(|item| item["label"] == label);
    // Gmail's `DRAFT` label is reachable as Drafts.
    assert_eq!(item("Drafts").unwrap()["total"], 3);
    // Spam exists only on the IMAP account, not the default one.
    let spam = item("Spam").expect("a mailbox only another account has");
    assert_eq!(spam["total"], 4);
    let spam_label = &world.imap_labels[2];
    assert_eq!(spam["lens"]["labelId"], spam_label.id.to_string());
    assert_eq!(item("Inbox").unwrap()["total"], 50);
    // All Mail counts messages, not labels: no account here has an All Mail
    // label at all.
    assert_eq!(item("All Mail").unwrap()["total"], 9124);
    assert_eq!(item("All Mail").unwrap()["unread"], 9);
    let all_mail = get_json(addr, "lens_kind=all_mail&view=messages").await;
    assert_eq!(all_mail["mailbox"]["counts"]["total"], 9124);
    let _ = Arc::new(());
}
