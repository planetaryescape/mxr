use super::*;
use chrono::{Duration, TimeZone, Utc};
use mxr_core::id::{AccountId, MessageId, ThreadId};
use mxr_protocol::{
    ReadingBandGroupData, ReadingEditionData, ReadingItemKindData, ReadingLinkData,
    ReadingShapeData, ReadingSourceData, READING_GUIDE,
};
use mxr_test_support::render_to_string;

fn fixture_now() -> chrono::DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 11, 4, 12, 0, 0).unwrap()
}

fn at() -> chrono::DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 10, 6, 7, 0, 0)
        .single()
        .expect("valid time")
}

pub(crate) fn item(key: &str, title: &str, source: &str) -> ReadingItemData {
    ReadingItemData {
        item_key: format!("{key}:0"),
        account_id: AccountId::from_provider_id("fake", "alex@demo.mxr.local"),
        message_id: MessageId::from_provider_id("fake", key),
        thread_id: ThreadId::from_provider_id("fake", key),
        kind: ReadingItemKindData::Issue,
        shape: ReadingShapeData::Single,
        title: title.into(),
        standfirst: None,
        source: source.into(),
        sender_email: format!("{}@news.example", source.to_lowercase().replace(' ', "")),
        words: 690,
        minutes: 3,
        url: None,
        domain: None,
        tracked: false,
        arrived_at: at(),
        source_filed_at: None,
        expires_at: Some(at() + Duration::days(14)),
        why: "Here because: you subscribed, and it has an unsubscribe link (rule). Fades Monday unless you keep it.".into(),
        fades: Some("Fades Monday unless you keep it.".into()),
        lead: false,
        engagement: None,
        links: Vec::new(),
        on_later: false,
        later_at: None,
        still_want_it: false,
        progress: 0.0,
        opened: false,
        finished: false,
        article_cached: false,
        unsubscribe_offer: None,
    }
}

fn link(key: &str, idx: u8, title: &str, domain: &str, tracked: bool) -> ReadingLinkData {
    ReadingLinkData {
        item_key: format!("{key}:{idx}"),
        title: title.into(),
        blurb: None,
        url: format!("https://{domain}/{idx}"),
        domain: domain.into(),
        tracked,
        on_later: false,
        article_cached: false,
    }
}

fn source(name: &str, evidence: &str) -> ReadingSourceData {
    ReadingSourceData {
        account_id: AccountId::from_provider_id("fake", "alex@demo.mxr.local"),
        sender_email: format!("{}@news.example", name.to_lowercase().replace(' ', "")),
        name: name.into(),
        issues: 11,
        opened: 0,
        finished: 0,
        evidence: evidence.into(),
        affinity: 0.0,
        window_days: 14.0,
        median_gap_days: Some(7.0),
        new_source: false,
        original_layout: false,
        unsubscribe: ReadingUnsubscribeData::OneClick,
        suggest_unsubscribe: true,
    }
}

pub(crate) fn edition() -> ReadingEditionData {
    let mut lead = item(
        "essay",
        "The quiet death of the three-pane layout",
        "Long Reads Weekly",
    );
    lead.lead = true;
    lead.engagement = Some("you read 8 of 10".into());
    let mut digest = item("links", "Six things worth your time", "Local-first Links");
    digest.shape = ReadingShapeData::Digest;
    digest.minutes = 1;
    digest.links = vec![
        link(
            "links",
            1,
            "Local-first mail is having a moment",
            "links.demo.mxr.local",
            false,
        ),
        link(
            "links",
            2,
            "WAL checkpoints, explained with pictures",
            "links.demo.mxr.local",
            false,
        ),
        link(
            "links",
            3,
            "Terminal workflows that stuck",
            "substack.com",
            true,
        ),
        link(
            "links",
            4,
            "Why sync engines need tombstones",
            "example.org",
            false,
        ),
        link(
            "links",
            5,
            "A sourdough schedule for people with jobs",
            "example.org",
            false,
        ),
    ];
    let mut teaser = item(
        "teaser",
        "Shipping a sync engine in 2026",
        "Platform Weekly",
    );
    teaser.shape = ReadingShapeData::Teaser;
    teaser.url = Some("https://platform.demo.mxr.local/sync".into());
    teaser.domain = Some("platform.demo.mxr.local".into());
    let mut growth = item("growth", "5 ways to double your open rate", "Growth Digest");
    growth.unsubscribe_offer = Some("You opened 0 of the last 11 issues".into());
    ReadingEditionData {
        generated_at: at(),
        header: READING_GUIDE.header.into(),
        bands: vec![
            ReadingBandGroupData {
                band: ReadingBandData::SinceLastVisit,
                label: "Since you were last here".into(),
                note: None,
                items: vec![lead, digest],
            },
            ReadingBandGroupData {
                band: ReadingBandData::Earlier,
                label: "Earlier this week".into(),
                note: None,
                items: vec![teaser],
            },
            ReadingBandGroupData {
                band: ReadingBandData::Fading,
                label: "Fading".into(),
                note: Some("Goes within a day, unless you keep it with b.".into()),
                items: vec![growth],
            },
        ],
        last_visit_at: Some(at() - Duration::days(1)),
        left_off_here: true,
        later: Vec::new(),
        later_count: 2,
        sources: vec![source(
            "Growth Digest",
            "You opened 0 of the last 11 issues",
        )],
        pace_wpm: 230,
        pace_measured: false,
        expired_now: 0,
        empty: None,
    }
}

fn page() -> ReadingPageState {
    ReadingPageState {
        edition: Some(edition()),
        guide: Some(READING_GUIDE.to_data(|_| None, &mxr_keymap::guide_key)),
        ..ReadingPageState::default()
    }
}

fn render(page: &ReadingPageState, selected: usize, width: u16) -> String {
    render_to_string(width, 36, |frame| {
        draw(
            frame,
            Rect::new(0, 0, width, 36),
            &ReadingLensView {
                zone: crate::ui::received_label::DisplayZone::Fixed(
                    chrono::FixedOffset::east_opt(3600).unwrap(),
                ),
                now: fixture_now(),
                page,
                selected_index: selected,
                active_pane: &ActivePane::MailList,
            },
            &crate::theme::Theme::default(),
        );
    })
}

#[test]
fn the_edition_renders_its_bands_the_left_off_line_and_a_digests_links() {
    let page = page();
    for width in [80u16] {
        let rendered = render(&page, 0, width);
        insta::assert_snapshot!(format!("reading_lens_edition_{width}"), rendered);
        assert!(rendered.contains("SINCE YOU WERE LAST HERE"), "{width}");
        assert!(rendered.contains("EARLIER THIS WEEK"), "{width}");
        assert!(rendered.contains("FADING"), "{width}");
        assert!(rendered.contains("You left off here"), "{width}");
        assert!(rendered.contains("Later 2"), "{width}");
        assert!(rendered.contains("Local-first mail"), "{width}");
        assert!(rendered.contains("+ 1 more"), "{width}");
        assert!(
            !rendered.contains("Esc close"),
            "no card at the top at {width}"
        );
        assert!(
            !rendered.to_lowercase().contains("unread"),
            "no unread counts"
        );
    }
}

#[test]
fn the_let_go_preview_lists_exactly_the_previewed_issues() {
    let mut confirm = page();
    let thread_ids = confirm.thread_ids();
    confirm.confirm = Some(ReadingConfirm::LetGoAll {
        thread_ids: thread_ids[..2].to_vec(),
        items: Vec::new(),
    });
    let rendered = render(&confirm, 0, 120);
    assert!(rendered.contains("Let go of 2 in Reading?"));
    assert!(rendered.contains("The quiet death"));
    assert!(rendered.contains("Six things worth"));
}

#[test]
fn mail_text_cannot_reach_the_terminal_as_control_sequences() {
    let mut hostile = page();
    hostile.edition.as_mut().expect("edition").bands[0].items[0].title = "Read\u{1b}[2J me".into();
    let rendered = render(&hostile, 0, 80);
    assert!(
        !rendered
            .chars()
            .any(|c| matches!(c as u32, 0x00..=0x09 | 0x0B..=0x1F | 0x7F..=0x9F)),
        "{rendered:?}"
    );
}
