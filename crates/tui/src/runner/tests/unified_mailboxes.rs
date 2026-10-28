//! "All accounts" in the TUI: the inbox, label views and All Mail ask the
//! daemon for a system mailbox by role, so mail from every account lists.

use super::*;
use crate::client::{unified_role_for, with_unified_system_counts, Client};
use futures::{SinkExt, StreamExt};
use mxr_protocol::{ClientKind, IpcCodec, IpcMessage, IpcPayload, Response, ResponseData};
use tokio::net::TcpListener;
use tokio_util::codec::Framed;

fn status() -> ResponseData {
    serde_json::from_value(serde_json::json!({"kind":"Status","uptime_secs":0,"accounts":[],"total_messages":1,"workspace_owner_supported":true,"archive_recovery_supported":true,"protocol_version":mxr_protocol::IPC_PROTOCOL_VERSION,"daemon_version":"test","daemon_build_id":"test"})).unwrap()
}

fn system_label(account_id: &AccountId, name: &str, unread: u32, total: u32) -> Label {
    Label {
        id: LabelId::from_scoped_provider_id(account_id, "test", name),
        account_id: account_id.clone(),
        name: name.into(),
        kind: LabelKind::System,
        color: None,
        provider_id: name.into(),
        unread_count: unread,
        total_count: total,
        role: None,
    }
}

/// A daemon with two accounts. The default account holds one inbox message;
/// a request that names the inbox role sees both accounts' messages.
async fn serve_two_accounts(
    first: Label,
    second: Label,
    envelopes: [Envelope; 2],
    seen_roles: Arc<Mutex<Vec<Option<String>>>>,
) -> mxr_client::SelectedDaemonTarget {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let target = mxr_client::SelectedDaemonTarget::new(
        mxr_client::TransportAddr::Tcp(listener.local_addr().unwrap()),
        None,
    );
    tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut framed = Framed::new(stream, IpcCodec::new());
        while let Some(Ok(message)) = framed.next().await {
            let IpcPayload::Request(request) = message.payload else {
                panic!("request")
            };
            let response = match request {
                Request::GetStatus => Response::Ok { data: status() },
                Request::GetArchiveWorkspace => Response::Ok {
                    data: ResponseData::ArchiveWorkspace {
                        workspace: mxr_protocol::ArchiveWorkspaceData {
                            workspace_id: "w".into(),
                            recovery_generation: "g".into(),
                        },
                    },
                },
                Request::ListLabels { all_accounts, .. } => Response::Ok {
                    data: ResponseData::Labels {
                        labels: if all_accounts {
                            vec![first.clone(), second.clone()]
                        } else {
                            vec![first.clone()]
                        },
                    },
                },
                Request::ListEnvelopes {
                    label_role,
                    label_id,
                    ..
                } => {
                    seen_roles.lock().unwrap().push(label_role.clone());
                    let listed = match label_role.as_deref() {
                        Some("inbox") | Some("all_mail") => envelopes.to_vec(),
                        // An older daemon: the default account's label only.
                        _ if label_id.as_ref() == Some(&first.id) => vec![envelopes[0].clone()],
                        _ => vec![envelopes[0].clone()],
                    };
                    Response::Ok {
                        data: ResponseData::Envelopes { envelopes: listed },
                    }
                }
                _ => Response::error("unexpected request"),
            };
            framed
                .send(IpcMessage {
                    id: message.id,
                    expected_workspace: None,
                    source: ClientKind::Daemon,
                    payload: IpcPayload::Response(response),
                })
                .await
                .unwrap();
        }
    });
    target
}

#[tokio::test]
async fn app_load_lists_every_accounts_inbox_and_sums_the_counts() {
    let (first_account, second_account) = (AccountId::new(), AccountId::new());
    let first = system_label(&first_account, "INBOX", 3, 10);
    let second = system_label(&second_account, "INBOX", 4, 20);
    let envelopes = [
        TestEnvelopeBuilder::new()
            .account_id(first_account)
            .subject("first account".to_string())
            .build(),
        TestEnvelopeBuilder::new()
            .account_id(second_account)
            .subject("second account".to_string())
            .build(),
    ];
    let seen_roles = Arc::new(Mutex::new(Vec::new()));
    let target = serve_two_accounts(first.clone(), second, envelopes, seen_roles.clone()).await;
    let mut client = Client::connect_target(&target).await.unwrap();

    let mut app = App::new();
    app.load(&mut client).await.unwrap();

    let subjects = app
        .mailbox
        .envelopes
        .iter()
        .map(|e| e.subject.as_str())
        .collect::<Vec<_>>();
    assert_eq!(subjects, vec!["first account", "second account"]);
    assert_eq!(app.mailbox.active_label.as_ref(), Some(&first.id));
    // The sidebar shows the default account's INBOX with both accounts' counts.
    let inbox = app
        .mailbox
        .labels
        .iter()
        .find(|label| label.name == "INBOX")
        .unwrap();
    assert_eq!((inbox.unread_count, inbox.total_count), (7, 30));
    assert!(seen_roles
        .lock()
        .unwrap()
        .contains(&Some("inbox".to_string())));
    // The all-envelopes list is every account's mail too.
    assert_eq!(app.mailbox.all_envelopes.len(), 2);
}

#[test]
fn system_labels_resolve_to_a_role_and_user_labels_do_not() {
    let account = AccountId::new();
    let labels = vec![
        system_label(&account, "SENT", 0, 0),
        Label {
            kind: LabelKind::User,
            ..system_label(&account, "Work", 0, 0)
        },
    ];
    assert_eq!(
        unified_role_for(&labels, &labels[0].id).as_deref(),
        Some("sent")
    );
    assert_eq!(unified_role_for(&labels, &labels[1].id), None);
    assert_eq!(unified_role_for(&labels, &LabelId::new()), None);
}

#[test]
fn unified_counts_replace_system_counts_only() {
    let (a, b) = (AccountId::new(), AccountId::new());
    let mut work = system_label(&a, "Work", 1, 2);
    work.kind = LabelKind::User;
    let mine = vec![system_label(&a, "INBOX", 3, 10), work.clone()];
    let mut other_work = system_label(&b, "Work", 100, 200);
    other_work.kind = LabelKind::User;
    let all = vec![
        system_label(&a, "INBOX", 3, 10),
        system_label(&b, "INBOX", 4, 20),
        work,
        other_work,
    ];
    let merged = with_unified_system_counts(mine, &all, &[]);
    assert_eq!((merged[0].unread_count, merged[0].total_count), (7, 30));
    assert_eq!((merged[1].unread_count, merged[1].total_count), (1, 2));
}

#[test]
fn a_mailbox_only_another_account_has_gets_a_sidebar_entry_with_daemon_counts() {
    // Default account is Gmail-shaped (INBOX, DRAFT); the second account is
    // IMAP-shaped and alone has SENT, with the folder's own provider id.
    let (gmail, imap) = (AccountId::new(), AccountId::new());
    let mine = vec![
        system_label(&gmail, "INBOX", 3, 10),
        system_label(&gmail, "DRAFT", 0, 2),
    ];
    let mut sent = system_label(&imap, "SENT", 1, 8);
    sent.provider_id = "Sent Items".into();
    let all = vec![
        mine[0].clone(),
        mine[1].clone(),
        system_label(&imap, "INBOX", 4, 20),
        sent.clone(),
    ];
    let counts = vec![
        mxr_protocol::SystemMailboxCount {
            role: "inbox".into(),
            unread: 9,
            total: 99,
        },
        mxr_protocol::SystemMailboxCount {
            role: "sent".into(),
            unread: 2,
            total: 30,
        },
    ];
    let merged = with_unified_system_counts(mine, &all, &counts);
    let named = |name: &str| merged.iter().find(|l| l.name == name).unwrap();
    assert_eq!(merged.len(), 3, "INBOX, DRAFT and the IMAP SENT");
    // Daemon counts win over the label sums (7, 30).
    assert_eq!(
        (named("INBOX").unread_count, named("INBOX").total_count),
        (9, 99)
    );
    assert_eq!(named("SENT").id, sent.id);
    assert_eq!(
        (named("SENT").unread_count, named("SENT").total_count),
        (2, 30)
    );
    // DRAFT has no daemon count here, so it is left as the account had it.
    assert_eq!(named("DRAFT").total_count, 2);
    // Selecting the IMAP entry lists Sent across accounts by role.
    assert_eq!(unified_role_for(&merged, &sent.id).as_deref(), Some("sent"));
}
