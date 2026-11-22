use super::*;
use chrono::{Duration, TimeZone};
use mxr_core::id::{AccountId, MessageId, ThreadId};
use mxr_core::types::Address;
use mxr_protocol::{
    ClosenessData, ComposerData, MessagesData, MessagesPreviewData, MessagesRowKindData,
    MessagesTopicData, PersonRefData, ThreadShapeData, TrimmedData, MESSAGES_GUIDE,
};
use mxr_test_support::render_to_string;

pub(crate) fn now() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 10, 2, 15, 0, 0)
        .single()
        .unwrap()
}

fn topic(subject: &str, state: TopicStateData, hours: i64) -> MessagesTopicData {
    MessagesTopicData {
        account_id: AccountId::from_provider_id("fake", "alex"),
        thread_id: ThreadId::from_provider_id("fake", subject),
        subject: subject.into(),
        shape: ThreadShapeData::OneToOne,
        state,
        last_at: now() - Duration::hours(hours),
        message_count: 2,
        with: vec![],
        message_ids: vec![MessageId::from_provider_id("fake", &format!("{subject}-1"))],
        reply_to_message_id: MessageId::from_provider_id("fake", &format!("{subject}-1")),
    }
}

fn person(email: &str, name: &str) -> PersonRefData {
    PersonRefData {
        id: email.into(),
        name: Some(name.into()),
        addresses: vec![email.into()],
    }
}

pub(crate) fn row(
    name: &str,
    band: MessagesBandData,
    preview: (MessagesPreviewKindData, &str),
    topics: Vec<MessagesTopicData>,
) -> MessagesRowData {
    let email = format!(
        "{}@example.com",
        name.split(' ').next().unwrap().to_lowercase()
    );
    let your_turn = band == MessagesBandData::YourTurn;
    MessagesRowData {
        id: format!("person:{email}"),
        kind: MessagesRowKindData::Person,
        account_id: AccountId::from_provider_id("fake", "alex"),
        band,
        title: name.into(),
        person: Some(person(&email, name)),
        members: vec![],
        closeness: ClosenessData::Close,
        your_turn,
        last_at: topics.first().map_or_else(now, |t| t.last_at),
        turn_since: your_turn.then(|| topics[0].last_at),
        preview: Some(MessagesPreviewData {
            kind: preview.0,
            text: preview.1.into(),
            message_id: None,
            model: None,
        }),
        draft: None,
        topics,
        usual_reply_seconds: Some(47 * 60),
        pace_label: Some("usually 47m".into()),
        overdue: your_turn,
        pinned: false,
        unread: your_turn,
        why: format!("Here because: {name} replied to your message, and you write to each other often (rule)."),
    }
}

pub(crate) fn populated() -> MessagesData {
    let samir = row(
        "Samir Patel",
        MessagesBandData::YourTurn,
        (
            MessagesPreviewKindData::Ask,
            "Can you take a look and reply with the next concrete step?",
        ),
        vec![
            topic("Contract renewal", TopicStateData::YourTurn, 16),
            topic("Launch checklist", TopicStateData::Waiting, 24),
        ],
    );
    let jon = row(
        "Jon Bell",
        MessagesBandData::YourTurn,
        (
            MessagesPreviewKindData::Latest,
            "Does the pricing copy read right to you?",
        ),
        vec![topic(
            "Pricing copy for the docs",
            TopicStateData::YourTurn,
            8,
        )],
    );
    let mut maya = row(
        "Maya Ortiz",
        MessagesBandData::Pinned,
        (MessagesPreviewKindData::Latest, ""),
        vec![],
    );
    maya.pinned = true;
    maya.your_turn = false;
    let iris = row(
        "Iris Chen",
        MessagesBandData::Recent,
        (MessagesPreviewKindData::You, "You: Thanks, on it."),
        vec![topic("Incident note", TopicStateData::Waiting, 28)],
    );
    MessagesData {
        generated_at: now(),
        header: MESSAGES_GUIDE.header.into(),
        your_turn: vec![samir, jon],
        your_turn_split: Default::default(),
        pinned: vec![maya],
        recent: vec![iris],
        quiet: vec![row(
            "Leo Park",
            MessagesBandData::Quiet,
            (MessagesPreviewKindData::Latest, "Sounds good"),
            vec![topic("Desk lamp", TopicStateData::Quiet, 24 * 12)],
        )],
        recent_total: 1,
        quiet_total: 12,
        filter: Default::default(),
        matched: 16,
        row_count: 16,
        thread_count: 20,
        empty_state: None,
        lapsed: vec![],
        merge_suggestion_count: 0,
    }
}

fn message(
    from_me: bool,
    text: &str,
    hours: i64,
    layout: MessageLayoutData,
) -> ConversationMessageData {
    ConversationMessageData {
        message_id: MessageId::from_provider_id("fake", &format!("{hours}-{from_me}")),
        from: Address {
            name: (!from_me).then(|| "Samir Patel".to_string()),
            email: if from_me {
                "alex@example.com"
            } else {
                "samir@example.com"
            }
            .into(),
        },
        from_me,
        date: now() - Duration::hours(hours),
        text: text.into(),
        has_preserved_context: false,
        source: Some(mxr_protocol::MessageTextSourceData::StoredBody),
        trimmed: TrimmedData {
            quote: !from_me,
            signature: !from_me,
            footer: false,
        },
        only_quoted: false,
        trimmed_label: (!from_me).then(|| "trimmed: quote, sig".to_string()),
        layout,
        paragraphs: u32::try_from(text.split("\n\n").count()).unwrap(),
        attachments: vec![],
        ask_quote: (!from_me)
            .then(|| "Can you take a look and reply with the next concrete step?".to_string()),
    }
}

pub(crate) fn samir_page() -> PersonPageData {
    let row = populated().your_turn[0].clone();
    let mut group = topic("Pricing copy", TopicStateData::Quiet, 72);
    group.with = vec!["Ruth".into()];
    group.shape = ThreadShapeData::Group;
    let mut topics = row.topics.clone();
    topics.push(group);
    PersonPageData {
        relationship_line: "You've written 48 times since 2023. Last: Tuesday.".into(),
        header_line: "close \u{b7} usually 47m".into(),
        conversation: Some(ConversationData {
            account_id: row.account_id.clone(),
            thread_id: topics[0].thread_id.clone(),
            subject: "Contract renewal".into(),
            shape: ThreadShapeData::OneToOne,
            state: TopicStateData::YourTurn,
            messages: vec![
                message(true, "Here's the renewal draft.", 40, MessageLayoutData::Compact),
                message(
                    false,
                    "Thanks for sending the renewal draft over. Most of it is fine as written.\n\nTwo things changed on our side: the support tier and the notice period.\n\nCan you take a look and reply with the next concrete step?",
                    16,
                    MessageLayoutData::Letter,
                ),
            ],
            earlier_count: 0,
            composer: ComposerData {
                label: "Reply to Samir \u{b7} Contract renewal".into(),
                reply_to_message_id: MessageId::from_provider_id("fake", "16-false"),
                reply_all: false,
            },
        }),
        topics,
        merge_suggestions: vec![],
        row,
    }
}

pub(crate) fn page(data: MessagesData, hints_seen: bool) -> MessagesPageState {
    let mut state = MessagesPageState {
        messages: Some(data),
        guide: Some(MESSAGES_GUIDE.to_data(|_| hints_seen.then(Utc::now), &mxr_keymap::guide_key)),
        ..MessagesPageState::default()
    };
    let samir = samir_page();
    state.page_for = Some(samir.row.id.clone());
    state.page = Some(samir);
    state
}

fn render_at(page: &MessagesPageState, width: u16, selected_index: usize) -> String {
    render_to_string(width, 34, |frame| {
        draw(
            frame,
            Rect::new(0, 0, width, 34),
            &MessagesView {
                page,
                selected_index,
                active_pane: &ActivePane::MailList,
                now: now(),
                offset: FixedOffset::east_opt(0).unwrap(),
                ack_seconds_left: None,
                account_marks: crate::ui::account_mark::AccountMarks::default(),
            },
            &crate::theme::Theme::default(),
        );
    })
}

#[test]
fn people_are_rows_in_bands_beside_the_person_page() {
    let mut data = populated();
    data.header = mxr_protocol::messages_copy::HISTORY_HEADER.into();
    data.filter.turn = Some(mxr_protocol::MessagesTurnData::All);
    data.row_count = 2;
    data.thread_count = 2;
    let filter = data.filter;
    let mut page = page(data, true);
    page.filter = filter;
    for width in [80u16] {
        let rendered = render_at(&page, width, 0);
        assert!(
            rendered.contains("All conversations"),
            "{width}\n{rendered}"
        );
        let your_turn = rendered.find("YOUR TURN").expect("Your turn band");
        let pinned = rendered.find("PINNED").expect("Pinned band");
        let recent = rendered.find("RECENT").expect("Recent band");
        assert!(your_turn < pinned && pinned < recent, "{width}\n{rendered}");
        assert!(rendered.contains("Samir Patel"), "{width}\n{rendered}");
        assert!(rendered.contains("QUIET (12)"), "{width}");
        assert!(rendered.contains("You: Thanks, on it."), "{width}");
        insta::assert_snapshot!(format!("messages_lens_populated_{width}"), rendered);
    }
    let wide = render_at(&page, 120, 0);
    assert!(wide.contains("Topics: [Contract renewal *]"), "{wide}");
    assert!(wide.contains("with Ruth: Pricing"), "{wide}");
    assert!(wide.contains("trimmed: quote, sig"), "{wide}");
    assert!(wide.contains("[+1 paragraph]"), "{wide}");
    assert!(
        wide.contains("\u{bb}Can you take a look"),
        "the ask shows on a closed letter\n{wide}"
    );
    assert!(wide.contains("> Reply to Samir"), "{wide}");
    assert!(wide.contains("close \u{b7} usually 47m"), "{wide}");
    assert!(wide.contains(". got it"), "{wide}");
}

#[test]
fn a_long_inline_exchange_is_never_reduced_to_the_cached_ask_paragraph() {
    let mut state = page(populated(), true);
    let message = &mut state
        .page
        .as_mut()
        .expect("person page is seeded")
        .conversation
        .as_mut()
        .expect("conversation is seeded")
        .messages[1];
    message.text = "> Can you ship Thursday?\n\nYes, but only if QA passes.\n\n> Can we skip the migration?\n\nNo, do not skip it; the revised date is Friday.".into();
    message.has_preserved_context = true;
    message.ask_quote = Some("No, do not skip it".into());
    message.paragraphs = 4;
    let rendered = render_at(&state, 120, 0);
    for context in [
        "Can you ship Thursday?",
        "only if QA passes",
        "Can we skip the migration?",
        "revised date is Friday",
    ] {
        assert!(
            rendered.contains(context),
            "missing {context:?}\n{rendered}"
        );
    }
    assert!(!rendered.contains("paragraph]"), "{rendered}");
}

#[test]
fn mail_text_cannot_reach_the_terminal_as_control_sequences() {
    let mut data = populated();
    data.your_turn[0].title = "Sam\u{1b}[31m\nir\u{202e}".into();
    let rendered = render_at(&page(data, true), 120, 0);
    assert!(!rendered.contains('\u{1b}'));
    assert!(!rendered.contains('\u{202e}'));
}
