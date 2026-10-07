use super::*;
use crate::test_fixtures::{test_account, TestEnvelopeBuilder};
use crate::Store;
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
