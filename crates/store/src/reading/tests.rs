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
async fn later_is_set_once_kept_on_a_second_press_and_cleared() {
    let fx = Fx::new().await;
    let issue = fx.issue("p1", "news@platform.example", at(0)).await;
    assert!(fx
        .store
        .set_reading_later(&fx.account, &issue, 1, true, at(1))
        .await
        .expect("on"));
    fx.store
        .set_reading_later(&fx.account, &issue, 1, true, at(5))
        .await
        .expect("keep");
    let later = fx
        .store
        .reading_later(std::slice::from_ref(&fx.account))
        .await
        .expect("list");
    assert_eq!(later.len(), 1);
    assert_eq!(later[0].later_at, Some(at(1)), "the first save stays");
    assert_eq!(later[0].kept_at, Some(at(5)));
    assert!(fx
        .store
        .set_reading_later(&fx.account, &issue, 1, false, at(6))
        .await
        .expect("off"));
    assert!(!fx
        .store
        .set_reading_later(&fx.account, &issue, 1, false, at(6))
        .await
        .expect("noop"));
    assert!(fx
        .store
        .reading_later(std::slice::from_ref(&fx.account))
        .await
        .expect("list")
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

#[tokio::test]
async fn source_issues_are_each_senders_latest_with_what_you_opened() {
    let fx = Fx::new().await;
    let mut ids = Vec::new();
    for day in 0..5 {
        ids.push(
            fx.issue(&format!("w{day}"), "news@platform.example", at(day * 24))
                .await,
        );
    }
    fx.issue("other", "digest@growth.example", at(3)).await;
    fx.store
        .record_reading_engagement(
            &fx.account,
            &ids[4],
            0,
            ReadingEngagementReport {
                opened: true,
                ..ReadingEngagementReport::default()
            },
            at(100),
        )
        .await
        .expect("open");
    let senders = vec![
        "news@platform.example".to_string(),
        "digest@growth.example".to_string(),
    ];
    let issues = fx
        .store
        .reading_source_issues(&fx.account, &senders, 3)
        .await
        .expect("issues");
    let platform: Vec<_> = issues
        .iter()
        .filter(|i| i.from_email == "news@platform.example")
        .collect();
    assert_eq!(platform.len(), 3);
    assert_eq!(platform[0].date, at(96));
    assert!(platform[0].opened);
    assert!(!platform[1].opened);
    let totals = fx
        .store
        .reading_source_totals(&fx.account, &senders)
        .await
        .expect("totals");
    assert_eq!(totals.get("news@platform.example"), Some(&5));
    assert_eq!(totals.get("digest@growth.example"), Some(&1));
}

#[tokio::test]
async fn articles_highlights_prefs_and_the_visit_round_trip() {
    let fx = Fx::new().await;
    let issue = fx.issue("p1", "news@platform.example", at(0)).await;
    let article = ReadingArticleRow {
        message_id: issue.clone(),
        idx: 2,
        url: "https://example.com/2".into(),
        final_url: Some("https://example.com/2/final".into()),
        status: "ok".into(),
        title: Some("Title".into()),
        byline: None,
        site_name: Some("Example".into()),
        html: Some("<p>Body</p>".into()),
        paragraphs: Some("[]".into()),
        words: 1200,
        contacted: r#"["example.com"]"#.into(),
        error: None,
        fetched_at: at(2),
    };
    fx.store.save_reading_article(&article).await.expect("save");
    assert_eq!(
        fx.store.reading_article(&issue, 2).await.expect("read"),
        Some(article)
    );
    assert_eq!(
        fx.store
            .reading_articles_saved(std::slice::from_ref(&issue))
            .await
            .expect("saved"),
        vec![(issue.clone(), 2)]
    );

    let highlight = ReadingHighlightRow {
        id: "h1".into(),
        account_id: fx.account.clone(),
        message_id: issue.clone(),
        idx: 0,
        view: "issue".into(),
        quote: "Start with the tablet.".into(),
        note: Some("good".into()),
        created_at: at(3),
        origin: "mxr".into(),
        origin_id: None,
        paragraph: None,
    };
    fx.store
        .insert_reading_highlight(&highlight)
        .await
        .expect("insert");
    assert_eq!(
        fx.store.reading_highlights(None).await.expect("all"),
        vec![highlight.clone()]
    );
    assert_eq!(
        fx.store
            .reading_highlights_for_message(&issue)
            .await
            .expect("one"),
        vec![highlight]
    );

    fx.store
        .set_reading_source_prefs(
            &fx.account,
            "news@platform.example",
            Some(true),
            false,
            at(4),
        )
        .await
        .expect("prefs");
    fx.store
        .set_reading_source_prefs(&fx.account, "news@platform.example", None, true, at(5))
        .await
        .expect("dismiss");
    let prefs = fx
        .store
        .reading_source_prefs(&fx.account)
        .await
        .expect("prefs");
    assert_eq!(
        prefs.get("news@platform.example"),
        Some(&ReadingSourcePrefs {
            original_layout: true,
            unsubscribe_offer_dismissed: true
        })
    );

    assert_eq!(
        fx.store.reading_visit(&fx.account).await.expect("none"),
        ReadingVisitRow::default()
    );
    let visit = ReadingVisitRow {
        first_seen: Some(at(1)),
        boundary: None,
        last_seen: Some(at(1)),
    };
    fx.store
        .set_reading_visit(&fx.account, &visit)
        .await
        .expect("visit");
    fx.store
        .set_reading_visit(
            &fx.account,
            &ReadingVisitRow {
                first_seen: Some(at(9)),
                boundary: Some(at(1)),
                last_seen: Some(at(9)),
            },
        )
        .await
        .expect("next");
    let got = fx.store.reading_visit(&fx.account).await.expect("read");
    assert_eq!(got.first_seen, Some(at(1)), "the first visit is kept");
    assert_eq!(got.boundary, Some(at(1)));
}

#[tokio::test]
async fn marginalia_highlights_mirror_without_touching_your_own() {
    let fx = Fx::new().await;
    let issue = fx.issue("p1", "news@platform.example", at(0)).await;
    fx.store
        .replace_reading_items(&issue, &[fx.item(&issue, 0, "Issue")])
        .await
        .expect("items");
    let own = ReadingHighlightRow {
        id: "own".into(),
        account_id: fx.account.clone(),
        message_id: issue.clone(),
        idx: 0,
        view: "issue".into(),
        quote: "made in mxr".into(),
        note: None,
        created_at: at(1),
        origin: "mxr".into(),
        origin_id: None,
        paragraph: None,
    };
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
    fx.store
        .upsert_reading_marginalia(&sent)
        .await
        .expect("sent");
    assert_eq!(
        fx.store.reading_marginalia(&issue, 0).await.expect("read"),
        Some(sent.clone())
    );
    assert_eq!(
        fx.store
            .reading_marginalia_due(None, at(3), at(0), 10)
            .await
            .expect("due")
            .len(),
        1
    );

    let marked = |id: &str, quote: &str| MarginaliaHighlightIn {
        origin_id: id.into(),
        quote: quote.into(),
        paragraph: (id == "b").then_some(4),
        created_at: at(3),
    };
    let first = fx
        .store
        .mirror_marginalia_highlights(&sent, &[marked("a", "one"), marked("b", "two")], at(4))
        .await
        .expect("mirror");
    assert_eq!(
        first,
        MarginaliaMirror {
            added: 2,
            removed: 0
        }
    );
    let second = fx
        .store
        .mirror_marginalia_highlights(&sent, &[marked("b", "two, edited")], at(5))
        .await
        .expect("mirror again");
    assert_eq!(
        second,
        MarginaliaMirror {
            added: 0,
            removed: 1
        }
    );

    let rows = fx
        .store
        .reading_highlights_for_message(&issue)
        .await
        .expect("highlights");
    let quotes: Vec<(&str, &str)> = rows
        .iter()
        .map(|row| (row.origin.as_str(), row.quote.as_str()))
        .collect();
    assert_eq!(
        quotes,
        vec![("mxr", "made in mxr"), ("marginalia", "two, edited")]
    );
    assert_eq!(
        rows.iter().map(|row| row.paragraph).collect::<Vec<_>>(),
        vec![None, Some(4)],
        "the mirrored row keeps Marginalia's paragraph"
    );
    let moved = MarginaliaHighlightIn {
        paragraph: Some(7),
        ..marked("b", "two, edited")
    };
    fx.store
        .mirror_marginalia_highlights(&sent, &[moved], at(5))
        .await
        .expect("mirror a moved paragraph");
    let rows = fx
        .store
        .reading_highlights_for_message(&issue)
        .await
        .expect("highlights");
    assert_eq!(rows[1].paragraph, Some(7), "a changed paragraph is updated");
    assert_eq!(
        fx.store
            .reading_marginalia(&issue, 0)
            .await
            .expect("read")
            .and_then(|row| row.highlights_synced_at),
        Some(at(5))
    );
    assert!(fx
        .store
        .reading_marginalia_due(None, at(5), at(0), 10)
        .await
        .expect("due")
        .is_empty());

    fx.store
        .delete_messages_and_derived(&fx.account, &["p1".to_string()])
        .await
        .expect("delete");
    assert_eq!(
        fx.store.reading_marginalia(&issue, 0).await.expect("gone"),
        None
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

    // Anything that isn't mxr's own is refused, whatever its name.
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

    // The mirror leaves your removal alone, and the removal leaves the
    // mirrored row to the mirror.
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
    // Undoing the edit after the removal must not bring the highlight back.
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
    // A second undo of the removal is a no-op, not a duplicate.
    assert_eq!(
        fx.store
            .put_back_reading_highlights(std::slice::from_ref(&remove))
            .await
            .unwrap(),
        0
    );

    // Its message gone: nothing to put it back on.
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

#[tokio::test]
async fn a_highlight_keeps_the_paragraph_it_was_made_in() {
    let fx = Fx::new().await;
    let issue = fx.issue("p-anchor", "news@platform.example", at(0)).await;
    fx.store
        .replace_reading_items(&issue, &[fx.item(&issue, 0, "Issue")])
        .await
        .expect("items");
    let mut anchored = highlight(&fx, &issue, "anchored", "mxr");
    anchored.paragraph = Some(3);
    let loose = highlight(&fx, &issue, "loose", "mxr");
    fx.store
        .insert_reading_highlight(&anchored)
        .await
        .expect("anchored");
    fx.store
        .insert_reading_highlight(&loose)
        .await
        .expect("loose");

    let rows = fx
        .store
        .reading_highlights_for_message(&issue)
        .await
        .expect("highlights");
    let paragraphs: Vec<(&str, Option<u32>)> =
        rows.iter().map(|r| (r.id.as_str(), r.paragraph)).collect();
    assert_eq!(paragraphs, vec![("anchored", Some(3)), ("loose", None)]);
}
