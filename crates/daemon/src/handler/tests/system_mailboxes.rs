//! "All accounts" system mailboxes: a `label_role` request lists the mailbox
//! across every enabled account, and everything else keeps its meaning.

use super::desk::{request, Fixture};
use super::*;
use chrono::{Duration, Utc};
use mxr_core::id::{AccountId, LabelId, ThreadId};
use mxr_core::types::{EventSource, Label, LabelKind, MessageDirection};

struct Accounts {
    fx: Fixture,
    first: AccountId,
    second: AccountId,
    disabled: AccountId,
    first_inbox: LabelId,
    second_inbox: LabelId,
    first_sent: LabelId,
}

async fn label(fx: &Fixture, account: &AccountId, name: &str, kind: LabelKind) -> LabelId {
    let id = LabelId::from_scoped_provider_id(account, "test", name);
    fx.state
        .store
        .upsert_label(&Label {
            id: id.clone(),
            account_id: account.clone(),
            name: name.into(),
            kind,
            color: None,
            provider_id: name.into(),
            unread_count: 0,
            total_count: 0,
            role: None,
        })
        .await
        .unwrap();
    id
}

async fn mail(
    fx: &Fixture,
    account: &AccountId,
    label: &LabelId,
    minutes_ago: i64,
    tag: &str,
) -> mxr_core::id::MessageId {
    let envelope = crate::test_fixtures::TestEnvelopeBuilder::new()
        .account_id(account.clone())
        .thread_id(ThreadId::new())
        .provider_id(format!("sysbox-{tag}"))
        .subject(tag.to_string())
        .date(Utc::now() - Duration::minutes(minutes_ago))
        .build();
    let store = &fx.state.store;
    store
        .upsert_envelope_with_direction(&envelope, MessageDirection::Inbound)
        .await
        .unwrap();
    store
        .set_message_labels(&envelope.id, std::slice::from_ref(label), EventSource::Sync)
        .await
        .unwrap();
    envelope.id
}

async fn accounts() -> Accounts {
    let fx = Fixture::new().await;
    let first = fx.account.clone();
    let first_inbox = fx.inbox.clone();
    let mut extra = Vec::new();
    for enabled in [true, false] {
        let account = AccountId::new();
        let mut record = crate::test_fixtures::test_account_with_id(account.clone());
        record.enabled = enabled;
        fx.state.store.insert_account(&record).await.unwrap();
        extra.push(account);
    }
    let (second, disabled) = (extra[0].clone(), extra[1].clone());
    let second_inbox = label(&fx, &second, "INBOX", LabelKind::System).await;
    let disabled_inbox = label(&fx, &disabled, "INBOX", LabelKind::System).await;
    let first_sent = label(&fx, &first, "Sent", LabelKind::System).await;

    mail(&fx, &first, &first_inbox, 10, "first-newest").await;
    mail(&fx, &second, &second_inbox, 20, "second-middle").await;
    mail(&fx, &first, &first_inbox, 30, "first-oldest").await;
    mail(&fx, &disabled, &disabled_inbox, 5, "disabled-newest").await;
    mail(&fx, &first, &first_sent, 1, "first-sent").await;
    Accounts {
        fx,
        first,
        second,
        disabled,
        first_inbox,
        second_inbox,
        first_sent,
    }
}

fn list_envelopes(
    account_id: Option<AccountId>,
    label_id: Option<LabelId>,
    label_role: Option<&str>,
) -> Request {
    Request::ListEnvelopes {
        label_id,
        account_id,
        limit: 50,
        offset: 0,
        label_role: label_role.map(str::to_string),
    }
}

fn list_threads(
    account_id: Option<AccountId>,
    label_id: Option<LabelId>,
    label_role: Option<&str>,
) -> Request {
    Request::ListThreads {
        account_id,
        label_id,
        limit: 50,
        offset: 0,
        sort: None,
        label_role: label_role.map(str::to_string),
    }
}

async fn envelope_subjects(a: &Accounts, req: Request) -> Vec<String> {
    let ResponseData::Envelopes { envelopes } = request(&a.fx, req).await else {
        panic!("expected envelopes")
    };
    envelopes.into_iter().map(|e| e.subject).collect()
}

async fn thread_subjects(a: &Accounts, req: Request) -> Vec<String> {
    let ResponseData::Threads { threads } = request(&a.fx, req).await else {
        panic!("expected threads")
    };
    threads.into_iter().map(|t| t.subject).collect()
}

#[tokio::test]
async fn role_without_account_lists_every_enabled_account_newest_first() {
    let a = accounts().await;
    let expected = vec!["first-newest", "second-middle", "first-oldest"];
    // The label id is the default account's, as an updated client still sends it.
    let req = list_envelopes(None, Some(a.first_inbox.clone()), Some("inbox"));
    assert_eq!(envelope_subjects(&a, req).await, expected);
    let req = list_threads(None, Some(a.first_inbox.clone()), Some("inbox"));
    assert_eq!(thread_subjects(&a, req).await, expected);
}

#[tokio::test]
async fn role_with_an_account_lists_only_that_account() {
    let a = accounts().await;
    let req = list_envelopes(Some(a.second.clone()), None, Some("inbox"));
    assert_eq!(envelope_subjects(&a, req).await, vec!["second-middle"]);
    let req = list_threads(Some(a.first.clone()), None, Some("inbox"));
    assert_eq!(
        thread_subjects(&a, req).await,
        vec!["first-newest", "first-oldest"]
    );
}

#[tokio::test]
async fn request_without_a_role_behaves_as_before() {
    let a = accounts().await;
    // An old client: only the default account's label id.
    let req = list_envelopes(None, Some(a.first_inbox.clone()), None);
    assert_eq!(
        envelope_subjects(&a, req).await,
        vec!["first-newest", "first-oldest"]
    );
    let req = list_threads(None, Some(a.second_inbox.clone()), None);
    assert_eq!(thread_subjects(&a, req).await, vec!["second-middle"]);
    // No label and no account still means the default account.
    let req = list_envelopes(None, None, None);
    assert_eq!(
        envelope_subjects(&a, req).await,
        vec!["first-sent", "first-newest", "first-oldest"]
    );
}

#[tokio::test]
async fn each_system_mailbox_has_its_own_role() {
    let a = accounts().await;
    let req = list_envelopes(None, Some(a.first_sent.clone()), Some("sent"));
    assert_eq!(envelope_subjects(&a, req).await, vec!["first-sent"]);
    let req = list_envelopes(None, None, Some("all_mail"));
    assert_eq!(
        envelope_subjects(&a, req).await,
        vec![
            "first-sent",
            "first-newest",
            "second-middle",
            "first-oldest"
        ],
        "All Mail spans enabled accounts and skips the disabled one"
    );
    let req = list_threads(None, None, Some("all_mail"));
    assert_eq!(thread_subjects(&a, req).await.len(), 4);
}

#[tokio::test]
async fn disabled_accounts_stay_out_of_the_merged_mailbox() {
    let a = accounts().await;
    let req = list_envelopes(None, None, Some("inbox"));
    let subjects = envelope_subjects(&a, req).await;
    assert!(!subjects.iter().any(|s| s.starts_with("disabled")));
    assert_ne!(a.disabled, a.first);
}

#[tokio::test]
async fn unknown_role_is_an_error_not_the_default_account() {
    let a = accounts().await;
    let msg = IpcMessage {
        expected_workspace: None,
        id: 3,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(list_envelopes(
            None,
            Some(a.first_inbox.clone()),
            Some("nope"),
        )),
    };
    let IpcPayload::Response(Response::Error { message, .. }) =
        handle_request(&a.fx.state, &msg).await.payload
    else {
        panic!("expected an error")
    };
    assert!(message.contains("unknown mailbox role"), "{message}");
}

#[tokio::test]
async fn list_labels_all_accounts_returns_each_enabled_accounts_labels() {
    let a = accounts().await;
    let ResponseData::Labels { labels } = request(
        &a.fx,
        Request::ListLabels {
            account_id: None,
            all_accounts: true,
        },
    )
    .await
    else {
        panic!("expected labels")
    };
    let accounts_seen: std::collections::HashSet<_> =
        labels.iter().map(|l| l.account_id.clone()).collect();
    assert!(accounts_seen.contains(&a.first) && accounts_seen.contains(&a.second));
    assert!(!accounts_seen.contains(&a.disabled));

    let ResponseData::Labels { labels } = request(
        &a.fx,
        Request::ListLabels {
            account_id: None,
            all_accounts: false,
        },
    )
    .await
    else {
        panic!("expected labels")
    };
    assert!(labels.iter().all(|l| l.account_id == a.first));
}

/// Profile limited to the first account, every other gate open.
async fn scope_to_first(a: &Accounts) {
    let mut config = a.fx.state.config_snapshot();
    config.agent_surfaces.profiles.insert(
        "mcp".into(),
        mxr_config::AgentProfileConfig {
            safety_policy: mxr_config::SafetyPolicy::Full,
            allowed_accounts: vec![a.first.as_str()],
            allow_send: true,
            allow_destructive: true,
            allowed_destructive_actions: vec![],
        },
    );
    a.fx.state.set_config_for_test(config).await;
}

async fn scoped_check(a: &Accounts, req: &Request) -> Result<(), String> {
    let config = a.fx.state.config_snapshot();
    enforce_client_profile(&a.fx.state, &config, ::mxr_protocol::ClientKind::Mcp, req).await
}

#[tokio::test]
async fn scoped_profiles_cannot_use_the_role_path_to_reach_other_accounts() {
    let a = accounts().await;
    scope_to_first(&a).await;
    // No account named: that is the all-accounts path, which a scoped
    // profile may not use.
    for req in [
        list_envelopes(None, None, Some("inbox")),
        list_threads(None, None, Some("all_mail")),
        Request::ListLabels {
            account_id: None,
            all_accounts: true,
        },
    ] {
        assert!(scoped_check(&a, &req).await.is_err(), "{req:?}");
    }
    // Naming the other account is refused, naming its own is fine.
    assert!(scoped_check(
        &a,
        &list_envelopes(Some(a.second.clone()), None, Some("inbox"))
    )
    .await
    .is_err());
    assert!(scoped_check(
        &a,
        &list_threads(Some(a.first.clone()), None, Some("inbox"))
    )
    .await
    .is_ok());
}

#[tokio::test]
async fn system_mailbox_counts_add_up_enabled_accounts_only() {
    let a = accounts().await;
    let counts = |scope: Option<AccountId>| {
        let fx = &a.fx;
        async move {
            let ResponseData::SystemMailboxCounts { mailboxes } =
                request(fx, Request::SystemMailboxCounts { account_id: scope }).await
            else {
                panic!("expected counts")
            };
            mailboxes
        }
    };
    let total = |mailboxes: &[mxr_protocol::SystemMailboxCount], role: &str| {
        mailboxes.iter().find(|m| m.role == role).map(|m| m.total)
    };
    // All Mail is counted from messages: first (3) + second (1); the disabled
    // account's message is not in scope.
    let all = counts(None).await;
    assert_eq!(total(&all, "all_mail"), Some(4));
    assert_eq!(
        total(&counts(Some(a.second.clone())).await, "all_mail"),
        Some(1)
    );
    // A disabled account named directly has nothing in scope.
    assert!(counts(Some(a.disabled.clone())).await.is_empty());
}

#[tokio::test]
async fn starred_lists_imap_flags_and_gmail_labels_together_through_the_daemon() {
    let a = accounts().await;
    let starred_label = label(&a.fx, &a.first, "STARRED", LabelKind::System).await;
    mail(&a.fx, &a.first, &starred_label, 2, "gmail-starred").await;
    // IMAP-shaped second account: the flag only, no STARRED label.
    let envelope = crate::test_fixtures::TestEnvelopeBuilder::new()
        .account_id(a.second.clone())
        .provider_id("sysbox-imap-flag")
        .subject("imap-flagged".to_string())
        .flags(mxr_core::types::MessageFlags::STARRED)
        .date(Utc::now() - Duration::minutes(3))
        .build();
    a.fx.state
        .store
        .upsert_envelope_with_direction(&envelope, MessageDirection::Inbound)
        .await
        .unwrap();
    let req = list_threads(None, Some(starred_label), Some("starred"));
    assert_eq!(
        thread_subjects(&a, req).await,
        vec!["gmail-starred", "imap-flagged"]
    );
}

#[tokio::test]
async fn a_disabled_accounts_message_in_a_shared_thread_never_reaches_a_merged_row() {
    let a = accounts().await;
    let shared = ThreadId::new();
    let visible = crate::test_fixtures::TestEnvelopeBuilder::new()
        .account_id(a.second.clone())
        .thread_id(shared.clone())
        .provider_id("shared-visible")
        .subject("Visible".to_string())
        .snippet("visible snippet".to_string())
        .date(Utc::now() - Duration::minutes(40))
        .build();
    let hidden = crate::test_fixtures::TestEnvelopeBuilder::new()
        .account_id(a.disabled.clone())
        .thread_id(shared.clone())
        .provider_id("shared-hidden")
        .subject("Aaa hidden".to_string())
        .snippet("hidden snippet".to_string())
        .date(Utc::now() - Duration::minutes(1))
        .build();
    let store = &a.fx.state.store;
    for env in [&visible, &hidden] {
        store
            .upsert_envelope_with_direction(env, MessageDirection::Inbound)
            .await
            .unwrap();
    }
    store
        .set_message_labels(
            &visible.id,
            std::slice::from_ref(&a.second_inbox),
            EventSource::Sync,
        )
        .await
        .unwrap();
    let ResponseData::Threads { threads } = request(
        &a.fx,
        list_threads(None, Some(a.first_inbox.clone()), Some("inbox")),
    )
    .await
    else {
        panic!("expected threads")
    };
    let thread = threads.iter().find(|t| t.id == shared).unwrap();
    assert_eq!(thread.subject, "Visible");
    assert_eq!(thread.snippet, "visible snippet");
    assert_eq!(thread.account_id, a.second);
    assert_eq!(thread.message_count, 1);
    assert_eq!(thread.message_ids, vec![visible.id.clone()]);
}
