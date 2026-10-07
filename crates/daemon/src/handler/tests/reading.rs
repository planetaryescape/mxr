//! Reading on a moved clock: the bands and each source's fade, expiry as
//! done in Reading without touching the provider, ranking by what you
//! finish, Later outliving the issue, let go of all previewing exactly
//! what it does, the unsubscribe evidence, highlights and the article
//! fetch's refusal of private links.

use super::desk::{request, Fixture};
use crate::handler::reading;
use chrono::{DateTime, Duration, Utc};
use mxr_core::id::{MessageId, ThreadId};
use mxr_core::types::{
    Address, Envelope, MessageBody, MessageDirection, MessageFlags, MessageMetadata,
    UnsubscribeMethod,
};
use mxr_protocol::{
    ModeKindData, ReadingBandData, ReadingEditionData, ReadingItemData, ReadingShapeData, Request,
    ResponseData,
};
use mxr_store::{ReadingEngagementReport, ReadingVisitRow};

const DIGEST_HTML: &str = r#"<html><body><p>Hi friends, six links this week about local-first software and storage engines, all short.</p><ul>
<li><p><a href="https://demo.example.com/local-first?utm_source=x">Local-first mail is having a moment</a> — Three clients that keep your mailbox on your disk.</p></li>
<li><p><a href="https://sqlite.org/releaselog/3_51.html">SQLite 3.51 release notes</a> — Faster JSON and a new WAL pragma.</p></li>
<li><p><a href="https://example.com/terminal">Terminal workflows that stuck</a> — A year in the terminal.</p></li>
<li><p><a href="http://127.0.0.1:9/admin">An internal dashboard link</a> — This one points at this machine.</p></li>
</ul><p><a href="https://links.example/unsubscribe">Unsubscribe</a></p></body></html>"#;

fn essay(words: usize) -> String {
    let sentence = "A delete is the absence of a row and an absence does not travel on its own. ";
    let mut text = String::new();
    while text.split_whitespace().count() < words {
        text.push_str(sentence);
    }
    format!("<html><body><p>{text}</p><p><a href=\"https://x.example/unsubscribe\">Unsubscribe</a></p></body></html>")
}

/// One newsletter issue in the inbox, `age` old at `now`.
async fn issue(
    fx: &Fixture,
    from: (&str, &str),
    subject: &str,
    html: &str,
    sent: DateTime<Utc>,
) -> (MessageId, ThreadId) {
    let id = MessageId::new();
    let thread = ThreadId::new();
    let envelope = Envelope {
        id: id.clone(),
        account_id: fx.account.clone(),
        provider_id: format!("reading-{id}"),
        thread_id: thread.clone(),
        message_id_header: Some(format!("<{id}@example.com>")),
        in_reply_to: None,
        references: vec![],
        from: Address {
            name: Some(from.0.to_string()),
            email: from.1.to_string(),
        },
        to: vec![Address {
            name: None,
            email: super::desk::ME.to_string(),
        }],
        cc: vec![],
        bcc: vec![],
        subject: subject.to_string(),
        date: sent,
        flags: MessageFlags::empty(),
        snippet: String::new(),
        has_attachments: false,
        size_bytes: 10,
        unsubscribe: UnsubscribeMethod::OneClick {
            url: format!(
                "https://{}/unsubscribe",
                from.1.split('@').nth(1).unwrap_or("x")
            ),
        },
        link_count: 0,
        body_word_count: 0,
        label_provider_ids: vec![],
        keywords: std::collections::BTreeSet::new(),
    };
    fx.store_envelope(&envelope, MessageDirection::Inbound)
        .await;
    fx.state
        .store
        .insert_body(&MessageBody {
            message_id: id.clone(),
            text_plain: None,
            text_html: Some(html.to_string()),
            attachments: vec![],
            fetched_at: sent,
            metadata: MessageMetadata::default(),
        })
        .await
        .expect("body");
    (id, thread)
}

async fn plan(fx: &Fixture, visit: &ReadingVisitRow, now: DateTime<Utc>) -> reading::Plan {
    let visits = std::iter::once((fx.account.clone(), *visit)).collect();
    reading::plan(&fx.state, std::slice::from_ref(&fx.account), &visits, now)
        .await
        .expect("edition")
}

fn band(edition: &ReadingEditionData, band: ReadingBandData) -> Vec<&ReadingItemData> {
    edition
        .bands
        .iter()
        .filter(|group| group.band == band)
        .flat_map(|group| group.items.iter())
        .collect()
}

fn titles(items: &[&ReadingItemData]) -> Vec<String> {
    items.iter().map(|item| item.title.clone()).collect()
}

const WEEKLY: (&str, &str) = ("Long Reads Weekly", "essays@longreads.example");

/// A weekly source: issues 0.5, 7.5, 13.5 and 21 days old, plus older
/// ones that set its fourteen-day window.
async fn weekly(fx: &Fixture, now: DateTime<Utc>) -> Vec<(MessageId, ThreadId)> {
    let mut out = Vec::new();
    for (i, age_hours) in [12, 180, 324, 504, 672, 840].into_iter().enumerate() {
        out.push(
            issue(
                fx,
                WEEKLY,
                &format!("Essay {i}"),
                &essay(400),
                now - Duration::hours(age_hours),
            )
            .await,
        );
    }
    out
}

#[tokio::test]
async fn bands_follow_the_visit_and_each_sources_window() {
    let fx = Fixture::new().await;
    let now = Utc::now();
    weekly(&fx, now).await;
    let visit = ReadingVisitRow {
        first_seen: Some(now - Duration::days(60)),
        boundary: Some(now - Duration::days(1)),
        last_seen: Some(now - Duration::days(1)),
    };
    let planned = plan(&fx, &visit, now).await;
    let edition = &planned.edition;
    assert_eq!(
        titles(&band(edition, ReadingBandData::SinceLastVisit)),
        ["Essay 0"]
    );
    assert_eq!(
        titles(&band(edition, ReadingBandData::Earlier)),
        ["Essay 1"]
    );
    assert_eq!(titles(&band(edition, ReadingBandData::Fading)), ["Essay 2"]);
    assert!(edition.left_off_here);
    let fading = band(edition, ReadingBandData::Fading)[0];
    assert!(fading.why.contains("unless you keep it"), "{}", fading.why);
    assert_eq!(fading.minutes, 2, "400 words at 230 a minute");
    // Three issues past fourteen days are expired: done in Reading.
    assert_eq!(planned.expired.len(), 3);
    let source = &edition.sources[0];
    assert!(
        (source.window_days - 14.0).abs() < 0.1,
        "{}",
        source.window_days
    );
}

#[tokio::test]
async fn expiry_is_done_in_reading_and_never_archives_at_the_provider() {
    let fx = Fixture::new().await;
    let now = Utc::now();
    let issues = weekly(&fx, now).await;
    let visit = ReadingVisitRow::default();
    let planned = plan(&fx, &visit, now).await;
    let marked = reading::expire(&fx.state, &planned.expired)
        .await
        .expect("expire");
    assert_eq!(marked, 3);
    // The expired issue is out of Reading but still in the inbox.
    let old = &issues[3];
    let membership = request(
        &fx,
        Request::GetModeMembership {
            message_id: None,
            thread_id: Some(old.1.clone()),
            thread_ids: vec![],
        },
    )
    .await;
    let ResponseData::ModeMembership { threads } = membership else {
        panic!("membership");
    };
    assert!(threads[0].done_in.contains(&ModeKindData::Reading));
    assert!(threads[0].in_inbox, "expiry is local");
    // A second edition finds nothing more to expire.
    assert!(plan(&fx, &visit, now).await.expired.is_empty());
}

#[tokio::test]
async fn a_digest_is_its_links_and_a_link_on_later_outlives_its_issue() {
    let fx = Fixture::new().await;
    let now = Utc::now();
    let (digest, thread) = issue(
        &fx,
        ("Local-first Links", "links@links.example"),
        "Local-first Links #41",
        DIGEST_HTML,
        now - Duration::hours(3),
    )
    .await;
    let edition = plan(&fx, &ReadingVisitRow::default(), now).await.edition;
    let item = band(&edition, ReadingBandData::SinceLastVisit)[0];
    assert_eq!(item.shape, ReadingShapeData::Digest);
    let links: Vec<&str> = item.links.iter().map(|l| l.title.as_str()).collect();
    assert_eq!(
        links,
        [
            "Local-first mail is having a moment",
            "SQLite 3.51 release notes",
            "Terminal workflows that stuck",
            "An internal dashboard link"
        ]
    );
    assert_eq!(item.links[0].url, "https://demo.example.com/local-first");

    // Later on the second link, previewed first.
    let link = item.links[1].item_key.clone();
    let preview = request(
        &fx,
        Request::SetReadingLater {
            item_keys: vec![link.clone()],
            later: true,
            dry_run: true,
        },
    )
    .await;
    let ResponseData::ReadingLater {
        later_count, items, ..
    } = preview
    else {
        panic!("later");
    };
    assert_eq!(later_count, 0, "a dry run changes nothing");
    assert!(items[0].changed);
    let saved = request(
        &fx,
        Request::SetReadingLater {
            item_keys: vec![link.clone()],
            later: true,
            dry_run: false,
        },
    )
    .await;
    let ResponseData::ReadingLater {
        later_count, copy, ..
    } = saved
    else {
        panic!("later");
    };
    assert_eq!(later_count, 1);
    assert_eq!(copy, "Saved to Later. 1 thing saved.");

    // Let go of the issue: the link stays on Later.
    request(
        &fx,
        Request::SetModeDone {
            thread_ids: vec![thread],
            mode: ModeKindData::Reading,
            dry_run: false,
            todo_ids: vec![],
            sender: None,
        },
    )
    .await;
    let edition = plan(&fx, &ReadingVisitRow::default(), now).await.edition;
    assert!(edition.bands.is_empty());
    assert_eq!(edition.later_count, 1);
    assert_eq!(edition.later[0].item_key, link);
    assert_eq!(edition.later[0].message_id, digest);
    assert!(edition.later[0].expires_at.is_none(), "Later never fades");
    // Much later, it is still there and asks once.
    let edition = plan(&fx, &ReadingVisitRow::default(), now + Duration::days(40))
        .await
        .edition;
    assert_eq!(edition.later_count, 1);
    assert!(edition.later[0].still_want_it);
}

#[tokio::test]
async fn let_go_of_everything_shown_does_exactly_what_its_preview_said() {
    let fx = Fixture::new().await;
    let now = Utc::now();
    weekly(&fx, now).await;
    issue(
        &fx,
        ("Local-first Links", "links@links.example"),
        "Links",
        DIGEST_HTML,
        now - Duration::hours(2),
    )
    .await;
    let edition = plan(&fx, &ReadingVisitRow::default(), now).await.edition;
    let threads: Vec<ThreadId> = edition
        .bands
        .iter()
        .flat_map(|group| group.items.iter().map(|item| item.thread_id.clone()))
        .collect();
    assert_eq!(threads.len(), 4);
    let done = |dry_run| Request::SetModeDone {
        thread_ids: threads.clone(),
        mode: ModeKindData::Reading,
        dry_run,
        todo_ids: vec![],
        sender: None,
    };
    let ResponseData::ModeDone { items: preview, .. } = request(&fx, done(true)).await else {
        panic!("preview");
    };
    let ResponseData::ModeDone { items: commit, .. } = request(&fx, done(false)).await else {
        panic!("commit");
    };
    let ids = |items: &[mxr_protocol::ModeDoneOutcomeData]| {
        items
            .iter()
            .map(|item| (item.thread_id.clone(), item.marked, item.archived))
            .collect::<Vec<_>>()
    };
    assert_eq!(ids(&preview), ids(&commit));
    assert!(commit.iter().all(|item| item.error.is_none()));
    let edition = plan(&fx, &ReadingVisitRow::default(), now).await.edition;
    assert!(edition.bands.is_empty());
}

#[tokio::test]
async fn the_sources_you_finish_lead_and_a_new_source_keeps_a_lead_slot() {
    let fx = Fixture::new().await;
    let now = Utc::now();
    let first_seen = now - Duration::days(80);
    let loved = ("Loved Weekly", "essays@loved.example");
    let ignored = ("Ignored Daily", "daily@ignored.example");
    let mut loved_ids = Vec::new();
    for week in 0..6 {
        loved_ids.push(
            issue(
                &fx,
                loved,
                &format!("Loved {week}"),
                &essay(300),
                now - Duration::hours(6 + 24 * 7 * week),
            )
            .await
            .0,
        );
    }
    for day in 0..10 {
        issue(
            &fx,
            ignored,
            &format!("Ignored {day}"),
            &essay(300),
            now - Duration::hours(5 + 24 * day),
        )
        .await;
    }
    issue(
        &fx,
        ("Brand New", "hello@brandnew.example"),
        "Brand new 1",
        &essay(300),
        now - Duration::hours(8),
    )
    .await;
    for id in &loved_ids[1..] {
        fx.state
            .store
            .record_reading_engagement(
                &fx.account,
                id,
                0,
                ReadingEngagementReport {
                    opened: true,
                    dwell_ms: 90_000,
                    progress: 1.0,
                },
                now - Duration::days(1),
            )
            .await
            .expect("finish");
    }
    let visit = ReadingVisitRow {
        first_seen: Some(first_seen),
        boundary: None,
        last_seen: None,
    };
    let edition = plan(&fx, &visit, now).await.edition;
    let since = band(&edition, ReadingBandData::SinceLastVisit);
    assert_eq!(since[0].title, "Loved 0");
    assert!(since[0].lead);
    assert_eq!(since[0].engagement.as_deref(), Some("you read 5 of 6"));
    let leads: Vec<&str> = since
        .iter()
        .filter(|i| i.lead)
        .map(|i| i.title.as_str())
        .collect();
    assert_eq!(leads.len(), 3);
    assert!(leads.contains(&"Brand new 1"), "{leads:?}");
    // Ten unopened dailies since you started: the offer, with evidence.
    let offer = since
        .iter()
        .find(|item| item.source == "Ignored Daily")
        .and_then(|item| item.unsubscribe_offer.clone());
    assert_eq!(offer.as_deref(), Some("You opened 0 of the last 10 issues"));
    // Five finished essays measure the pace: about 300 words in 90 s.
    assert!(edition.pace_measured);
    assert!(
        (190..=215).contains(&edition.pace_wpm),
        "{}",
        edition.pace_wpm
    );
}

#[tokio::test]
async fn nothing_is_claimed_about_issues_from_before_your_first_visit() {
    let fx = Fixture::new().await;
    let now = Utc::now();
    for day in 1..=10 {
        issue(
            &fx,
            ("Old Daily", "daily@old.example"),
            &format!("Day {day}"),
            &essay(200),
            now - Duration::days(day),
        )
        .await;
    }
    let visit = ReadingVisitRow {
        first_seen: Some(now - Duration::hours(1)),
        boundary: None,
        last_seen: None,
    };
    let edition = plan(&fx, &visit, now).await.edition;
    assert!(edition
        .sources
        .iter()
        .all(|s| !s.suggest_unsubscribe && s.issues == 0));
}

#[tokio::test]
async fn highlights_export_as_markdown_with_their_source() {
    let fx = Fixture::new().await;
    let now = Utc::now();
    let (id, _) = issue(
        &fx,
        WEEKLY,
        "Why sync engines need tombstones",
        &essay(200),
        now,
    )
    .await;
    let key = format!("{id}:0");
    let ResponseData::ReadingHighlight { highlight } = request(
        &fx,
        Request::SaveHighlight {
            item_key: key.clone(),
            quote: "A delete is the absence of a row".to_string(),
            note: Some("for the talk".to_string()),
            view: None,
        },
    )
    .await
    else {
        panic!("highlight");
    };
    assert_eq!(highlight.source, "Long Reads Weekly");
    let ResponseData::ReadingHighlights {
        highlights,
        markdown,
    } = request(&fx, Request::ExportReadingHighlights { account_id: None }).await
    else {
        panic!("export");
    };
    assert_eq!(highlights.len(), 1);
    assert!(markdown.contains("## Why sync engines need tombstones"));
    assert!(markdown.contains("> A delete is the absence of a row"));
    assert!(markdown.contains("Note: for the talk"));
    let ResponseData::ReadingItem { item } =
        request(&fx, Request::GetReadingItem { item_key: key }).await
    else {
        panic!("item");
    };
    assert_eq!(item.highlights.len(), 1);
    assert!(item.paragraphs[0]
        .text
        .starts_with("A delete is the absence"));
}

#[tokio::test]
async fn an_article_on_this_machine_is_refused_and_the_reason_is_kept() {
    let fx = Fixture::new().await;
    let now = Utc::now();
    issue(
        &fx,
        ("Local-first Links", "links@links.example"),
        "Links",
        DIGEST_HTML,
        now,
    )
    .await;
    let edition = plan(&fx, &ReadingVisitRow::default(), now).await.edition;
    let local = edition.bands[0].items[0].links[3].item_key.clone();
    let ResponseData::ReadingArticle { fetch } = request(
        &fx,
        Request::FetchArticle {
            item_key: local.clone(),
            refresh: false,
        },
    )
    .await
    else {
        panic!("fetch");
    };
    assert!(fetch.article.is_none());
    assert!(
        fetch
            .error
            .as_deref()
            .is_some_and(|e| e.contains("private network")),
        "{:?}",
        fetch.error
    );
    let ResponseData::ReadingItem { item } =
        request(&fx, Request::GetReadingItem { item_key: local }).await
    else {
        panic!("item");
    };
    assert!(item.article_error.is_some());
}

#[tokio::test]
async fn opening_an_item_records_engagement_and_retires_the_card() {
    let fx = Fixture::new().await;
    let (id, _) = issue(&fx, WEEKLY, "Essay", &essay(300), Utc::now()).await;
    let ResponseData::ReadingEngagement {
        recorded, finished, ..
    } = request(
        &fx,
        Request::RecordReadingEngagement {
            item_key: format!("{id}:0"),
            opened: true,
            dwell_ms: 30_000,
            progress: 0.95,
        },
    )
    .await
    else {
        panic!("engagement");
    };
    assert!(recorded);
    assert!(finished);
    let ResponseData::ModeGuides { guides } = request(
        &fx,
        Request::GetModeGuide {
            mode: Some("reading".to_string()),
        },
    )
    .await
    else {
        panic!("guide");
    };
    assert!(guides[0].card_seen);
}

#[tokio::test]
async fn the_later_count_covers_only_the_requests_accounts() {
    let fx = Fixture::new().await;
    let now = Utc::now();
    let (mine, _) = issue(&fx, WEEKLY, "Mine", &essay(200), now).await;
    // Another account with its own item on Later.
    let other = mxr_core::Account {
        id: mxr_core::AccountId::new(),
        name: "Other".into(),
        email: "other@example.com".into(),
        sync_backend: None,
        send_backend: None,
        enabled: true,
    };
    fx.state
        .store
        .insert_account(&other)
        .await
        .expect("account");
    let mut theirs = fx
        .state
        .store
        .get_envelope(&mine)
        .await
        .expect("read")
        .expect("envelope");
    theirs.id = MessageId::new();
    theirs.account_id = other.id.clone();
    theirs.provider_id = format!("other-{}", theirs.id);
    theirs.message_id_header = Some(format!("<{}@other>", theirs.id));
    fx.state
        .store
        .upsert_envelope(&theirs)
        .await
        .expect("envelope");
    fx.state
        .store
        .set_reading_later(&other.id, &theirs.id, 0, true, now)
        .await
        .expect("later");

    let ResponseData::ReadingLater {
        later_count, copy, ..
    } = request(
        &fx,
        Request::SetReadingLater {
            item_keys: vec![format!("{mine}:0")],
            later: true,
            dry_run: false,
        },
    )
    .await
    else {
        panic!("later");
    };
    assert_eq!(later_count, 1, "the other account's shelf isn't counted");
    assert_eq!(copy, "Saved to Later. 1 thing saved.");
}

#[tokio::test]
async fn opening_reading_records_a_visit_per_account_and_not_while_activity_is_paused() {
    let fx = Fixture::new().await;
    let other = mxr_core::Account {
        id: mxr_core::AccountId::new(),
        name: "Other".into(),
        email: "other@example.com".into(),
        sync_backend: None,
        send_backend: None,
        enabled: true,
    };
    fx.state
        .store
        .insert_account(&other)
        .await
        .expect("account");
    let open = |account: &mxr_core::AccountId| Request::GetReadingEdition {
        account_id: Some(account.clone()),
        mark_visit: true,
    };
    request(&fx, open(&fx.account)).await;
    let mine = fx
        .state
        .store
        .reading_visit(&fx.account)
        .await
        .expect("visit");
    assert!(mine.last_seen.is_some(), "opening Reading is a visit");
    let theirs = fx
        .state
        .store
        .reading_visit(&other.id)
        .await
        .expect("visit");
    assert_eq!(
        theirs,
        ReadingVisitRow::default(),
        "another account's visit is its own"
    );

    fx.state.activity.pause(None);
    for _ in 0..200 {
        if fx.state.activity.pause_status().0 {
            break;
        }
        tokio::task::yield_now().await;
    }
    assert!(fx.state.activity.pause_status().0);
    request(&fx, open(&other.id)).await;
    let theirs = fx
        .state
        .store
        .reading_visit(&other.id)
        .await
        .expect("visit");
    assert_eq!(
        theirs,
        ReadingVisitRow::default(),
        "no visit while activity is paused"
    );
}

#[tokio::test]
async fn a_digest_link_keeps_its_later_state_when_re_extraction_reorders_the_links() {
    let fx = Fixture::new().await;
    let now = Utc::now();
    let (id, _) = issue(
        &fx,
        ("Local-first Links", "links@links.example"),
        "Links",
        DIGEST_HTML,
        now,
    )
    .await;
    let edition = plan(&fx, &ReadingVisitRow::default(), now).await.edition;
    let sqlite = edition.bands[0].items[0]
        .links
        .iter()
        .find(|link| link.title == "SQLite 3.51 release notes")
        .expect("the SQLite link")
        .item_key
        .clone();
    request(
        &fx,
        Request::SetReadingLater {
            item_keys: vec![sqlite.clone()],
            later: true,
            dry_run: false,
        },
    )
    .await;

    // The same issue, its links in another order, extracted again.
    let reordered = DIGEST_HTML.replace(
        "<li><p><a href=\"https://sqlite.org/releaselog/3_51.html\">SQLite 3.51 release notes</a> — Faster JSON and a new WAL pragma.</p></li>\n",
        "",
    );
    let reordered = reordered.replace(
        "<ul>\n",
        "<ul>\n<li><p><a href=\"https://sqlite.org/releaselog/3_51.html#top\">SQLite 3.51 release notes</a> — Faster JSON and a new WAL pragma.</p></li>\n",
    );
    assert_ne!(reordered, DIGEST_HTML);
    fx.state
        .store
        .insert_body(&MessageBody {
            message_id: id.clone(),
            text_plain: None,
            text_html: Some(reordered),
            attachments: vec![],
            fetched_at: now,
            metadata: MessageMetadata::default(),
        })
        .await
        .expect("body");
    fx.state
        .store
        .replace_reading_items(&id, &[])
        .await
        .expect("clear the cache");

    let edition = plan(&fx, &ReadingVisitRow::default(), now).await.edition;
    let links = &edition.bands[0].items[0].links;
    assert_eq!(
        links[0].title, "SQLite 3.51 release notes",
        "the new order shows"
    );
    assert_eq!(links[0].item_key, sqlite, "the link keeps its key");
    assert!(links[0].on_later);
    assert!(links[1..].iter().all(|link| !link.on_later));
    assert_eq!(edition.later[0].title, "SQLite 3.51 release notes");
}
