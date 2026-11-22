use super::*;
use crate::test_fixtures::{test_account, TestEnvelopeBuilder};
use crate::{HighlightPrior, Store};
use chrono::{Duration, TimeZone};
use mxr_core::id::ThreadId;
use mxr_core::types::{Address, MessageDirection};

fn at(hours: i64) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 10, 1, 9, 0, 0)
        .single()
        .expect("valid time")
        + Duration::hours(hours)
}

struct Fx {
    store: Store,
    account: AccountId,
}

impl Fx {
    async fn new() -> Self {
        let store = Store::in_memory().await.expect("store");
        let account = test_account();
        store.insert_account(&account).await.expect("account");
        Self {
            store,
            account: account.id,
        }
    }

    async fn issue(&self, provider_id: &str, from: &str, date: DateTime<Utc>) -> MessageId {
        let mut envelope = TestEnvelopeBuilder::new()
            .account_id(self.account.clone())
            .build();
        envelope.provider_id = provider_id.to_string();
        envelope.message_id_header = Some(format!("<{provider_id}@test>"));
        envelope.thread_id = ThreadId::new();
        envelope.date = date;
        envelope.from = Address {
            name: Some("Platform Weekly".to_string()),
            email: from.to_string(),
        };
        self.store
            .upsert_envelope_with_direction(&envelope, MessageDirection::Inbound)
            .await
            .expect("envelope");
        envelope.id
    }

    fn item(&self, message_id: &MessageId, idx: i64, title: &str) -> ReadingItemRow {
        ReadingItemRow {
            message_id: message_id.clone(),
            idx,
            position: idx,
            account_id: self.account.clone(),
            kind: if idx == 0 { "issue" } else { "link" }.to_string(),
            shape: "digest".to_string(),
            title: title.to_string(),
            standfirst: None,
            url: (idx > 0).then(|| format!("https://example.com/{idx}")),
            domain: (idx > 0).then(|| "example.com".to_string()),
            tracked: false,
            words: 400,
            extractor_version: 1,
        }
    }
}

#[tokio::test]
async fn a_read_back_never_resurrects_or_loses_what_you_removed() {
    let fx = Fx::new().await;
    let issue = fx.issue("p1", "news@platform.example", at(0)).await;
    fx.store
        .replace_reading_items(&issue, &[fx.item(&issue, 0, "Issue")])
        .await
        .expect("items");
    let own = highlight(&fx, &issue, "own", "mxr");
    fx.store.insert_reading_highlight(&own).await.expect("own");
    let sent = ReadingMarginaliaRow {
        message_id: issue.clone(),
        idx: 0,
        account_id: fx.account.clone(),
        article_id: "k57".into(),
        read_url: "https://reader.example.com/read/k57".into(),
        view: "issue".into(),
        sent_at: at(2),
        highlights_synced_at: None,
    };
    fx.store.upsert_reading_marginalia(&sent).await.unwrap();
    let remote = MarginaliaHighlightIn {
        origin_id: "a".into(),
        quote: "one".into(),
        paragraph: None,
        created_at: at(3),
    };
    fx.store
        .mirror_marginalia_highlights(&sent, std::slice::from_ref(&remote), at(4))
        .await
        .unwrap();

    fx.store.remove_reading_highlight("own").await.unwrap();
    fx.store
        .mirror_marginalia_highlights(&sent, std::slice::from_ref(&remote), at(5))
        .await
        .unwrap();
    let rows = fx
        .store
        .reading_highlights_for_message(&issue)
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].origin, "marginalia");
    assert_eq!(
        fx.store
            .remove_reading_highlight(&rows[0].id)
            .await
            .unwrap(),
        HighlightEdit::ReadOnly("marginalia".into())
    );
}

#[tokio::test]
async fn items_are_replaced_per_message_and_go_with_it() {
    let fx = Fx::new().await;
    let issue = fx.issue("p1", "news@platform.example", at(0)).await;
    let rows = vec![fx.item(&issue, 0, "Issue"), fx.item(&issue, 1, "Link one")];
    fx.store
        .replace_reading_items(&issue, &rows)
        .await
        .expect("write");
    fx.store
        .replace_reading_items(&issue, &rows[..1])
        .await
        .expect("rewrite");
    let got = fx
        .store
        .reading_items_for_messages(std::slice::from_ref(&issue))
        .await
        .expect("read");
    assert_eq!(got, rows[..1]);

    fx.store
        .set_reading_later(&fx.account, &issue, 0, true, at(1))
        .await
        .expect("later");
    fx.store
        .delete_messages_and_derived(&fx.account, &["p1".to_string()])
        .await
        .expect("delete");
    assert!(fx
        .store
        .reading_items_for_messages(std::slice::from_ref(&issue))
        .await
        .expect("read")
        .is_empty());
    assert!(fx
        .store
        .reading_later(std::slice::from_ref(&fx.account))
        .await
        .expect("later")
        .is_empty());
}

#[tokio::test]
async fn engagement_adds_time_keeps_the_furthest_point_and_finishes_at_ninety_percent() {
    let fx = Fx::new().await;
    let issue = fx.issue("p1", "news@platform.example", at(0)).await;
    let report = |opened, dwell_ms, progress| ReadingEngagementReport {
        opened,
        dwell_ms,
        progress,
    };
    let finished = fx
        .store
        .record_reading_engagement(&fx.account, &issue, 0, report(true, 10_000, 0.5), at(1))
        .await
        .expect("first");
    assert!(!finished);
    fx.store
        .record_reading_engagement(&fx.account, &issue, 0, report(false, 5_000, 0.2), at(2))
        .await
        .expect("scroll back");
    let finished = fx
        .store
        .record_reading_engagement(&fx.account, &issue, 0, report(false, 20_000, 0.95), at(3))
        .await
        .expect("end");
    assert!(finished);
    let state = fx
        .store
        .reading_states_for_messages(std::slice::from_ref(&issue))
        .await
        .expect("state");
    assert_eq!(state[0].opened_at, Some(at(1)));
    assert_eq!(state[0].dwell_ms, 35_000);
    assert!((state[0].progress - 0.95).abs() < 1e-9);
    assert_eq!(state[0].finished_at, Some(at(3)));
    fx.store
        .replace_reading_items(&issue, &[fx.item(&issue, 0, "Issue")])
        .await
        .expect("items");
    assert_eq!(
        fx.store
            .reading_finished_samples(10)
            .await
            .expect("samples"),
        vec![(400, 35_000)]
    );
}

fn highlight(fx: &Fx, issue: &MessageId, id: &str, origin: &str) -> ReadingHighlightRow {
    ReadingHighlightRow {
        id: id.into(),
        account_id: fx.account.clone(),
        message_id: issue.clone(),
        idx: 0,
        view: "issue".into(),
        quote: format!("quote {id}"),
        note: Some("first thought".into()),
        created_at: at(1),
        origin: origin.into(),
        origin_id: (origin != "mxr").then(|| format!("remote-{id}")),
        paragraph: None,
    }
}

#[tokio::test]
async fn only_your_own_highlights_can_be_edited_or_removed() {
    let fx = Fx::new().await;
    let issue = fx.issue("p1", "news@platform.example", at(0)).await;
    fx.store
        .replace_reading_items(&issue, &[fx.item(&issue, 0, "Issue")])
        .await
        .expect("items");
    let own = highlight(&fx, &issue, "own", "mxr");
    let theirs = highlight(&fx, &issue, "theirs", "marginalia");
    fx.store.insert_reading_highlight(&own).await.expect("own");
    fx.store
        .insert_reading_highlight(&theirs)
        .await
        .expect("theirs");

    for id in ["theirs"] {
        assert_eq!(
            fx.store.set_reading_highlight_note(id, None).await.unwrap(),
            HighlightEdit::ReadOnly("marginalia".into())
        );
        assert_eq!(
            fx.store.remove_reading_highlight(id).await.unwrap(),
            HighlightEdit::ReadOnly("marginalia".into())
        );
    }
    assert_eq!(
        fx.store.reading_highlight("theirs").await.unwrap(),
        Some(theirs)
    );

    assert_eq!(
        fx.store
            .set_reading_highlight_note("own", Some("second thought"))
            .await
            .unwrap(),
        HighlightEdit::Done(own.clone())
    );
    assert_eq!(
        fx.store
            .reading_highlight("own")
            .await
            .unwrap()
            .and_then(|row| row.note),
        Some("second thought".into())
    );
    fx.store
        .set_reading_highlight_note("own", None)
        .await
        .unwrap();
    assert_eq!(
        fx.store
            .reading_highlight("own")
            .await
            .unwrap()
            .unwrap()
            .note,
        None,
        "no note clears it"
    );
    assert_eq!(
        fx.store.remove_reading_highlight("own").await.unwrap(),
        HighlightEdit::Done(ReadingHighlightRow {
            note: None,
            ..own.clone()
        })
    );
    assert_eq!(
        fx.store.remove_reading_highlight("own").await.unwrap(),
        HighlightEdit::Missing,
        "removing twice finds nothing the second time"
    );
}

#[tokio::test]
async fn undo_puts_a_removed_highlight_and_an_old_note_back() {
    let fx = Fx::new().await;
    let issue = fx.issue("p1", "news@platform.example", at(0)).await;
    fx.store
        .replace_reading_items(&issue, &[fx.item(&issue, 0, "Issue")])
        .await
        .expect("items");
    let own = highlight(&fx, &issue, "own", "mxr");
    fx.store.insert_reading_highlight(&own).await.unwrap();

    let HighlightEdit::Done(before_edit) = fx
        .store
        .set_reading_highlight_note("own", Some("changed"))
        .await
        .unwrap()
    else {
        panic!("edit should land");
    };
    let HighlightEdit::Done(before_remove) =
        fx.store.remove_reading_highlight("own").await.unwrap()
    else {
        panic!("remove should land");
    };
    let remove = HighlightPrior {
        row: before_remove,
        removed: true,
    };
    let edit = HighlightPrior {
        row: before_edit.clone(),
        removed: false,
    };
    assert_eq!(
        fx.store
            .put_back_reading_highlights(std::slice::from_ref(&edit))
            .await
            .unwrap(),
        0
    );
    assert_eq!(fx.store.reading_highlight("own").await.unwrap(), None);
    assert_eq!(
        fx.store
            .put_back_reading_highlights(std::slice::from_ref(&remove))
            .await
            .unwrap(),
        1
    );
    assert_eq!(
        fx.store
            .reading_highlight("own")
            .await
            .unwrap()
            .and_then(|row| row.note),
        Some("changed".into())
    );
    assert_eq!(
        fx.store.put_back_reading_highlights(&[edit]).await.unwrap(),
        1
    );
    assert_eq!(
        fx.store.reading_highlight("own").await.unwrap(),
        Some(before_edit)
    );
    assert_eq!(
        fx.store
            .put_back_reading_highlights(std::slice::from_ref(&remove))
            .await
            .unwrap(),
        0
    );

    fx.store.remove_reading_highlight("own").await.unwrap();
    fx.store
        .delete_messages_and_derived(&fx.account, &["p1".to_string()])
        .await
        .unwrap();
    assert_eq!(
        fx.store
            .put_back_reading_highlights(&[remove])
            .await
            .unwrap(),
        0
    );
}
