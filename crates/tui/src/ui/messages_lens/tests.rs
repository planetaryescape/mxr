use super::*;
use chrono::{Duration, TimeZone};
use mxr_core::id::{AccountId, MessageId, ThreadId};
use mxr_core::types::Address;
use mxr_protocol::{
    ClosenessData, ComposerData, MessagesData, MessagesLapsedData, MessagesPreviewData,
    MessagesRowKindData, MessagesTopicData, PersonRefData, ThreadShapeData, TrimmedData,
    MESSAGES_GUIDE,
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
        row_count: 16,
        thread_count: 20,
        empty_state: None,
        lapsed: vec![],
        merge_suggestion_count: 0,
    }
}

pub(crate) fn clear() -> MessagesData {
    let mut data = populated();
    data.your_turn.clear();
    data.empty_state = Some(mxr_protocol::messages_copy::CLEAR.into());
    data.lapsed = vec![MessagesLapsedData {
        account_id: AccountId::from_provider_id("fake", "alex"),
        person: person("ari@example.com", "Ari Stone"),
        line: "Ari usually writes every week. Last: 19 days ago.".into(),
    }];
    data
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
        guide: Some(MESSAGES_GUIDE.to_data(|_| hints_seen.then(Utc::now))),
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
            },
            &crate::theme::Theme::default(),
        );
    })
}

#[test]
fn people_are_rows_in_bands_beside_the_person_page() {
    let page = page(populated(), true);
    for width in [60u16, 80, 120] {
        let rendered = render_at(&page, width, 0);
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
fn an_expanded_letter_shows_every_paragraph_and_highlights_the_ask() {
    let mut state = page(populated(), true);
    let letter = state
        .page
        .as_ref()
        .unwrap()
        .conversation
        .as_ref()
        .unwrap()
        .messages[1]
        .message_id
        .clone();
    state.expanded.insert(letter);
    let rendered = render_at(&state, 120, 0);
    assert!(rendered.contains("\u{bb}Can you take a look"), "{rendered}");
    assert!(!rendered.contains("paragraph]"), "{rendered}");
    assert!(rendered.contains("Two things changed"), "{rendered}");
}

#[test]
fn the_person_page_has_the_whole_screen_when_narrow_and_focused() {
    let mut state = page(populated(), true);
    state.focus = MessagesFocus::Person;
    let rendered = render_at(&state, 60, 0);
    assert!(rendered.contains("Topics:"), "{rendered}");
    assert!(!rendered.contains("YOUR TURN"), "{rendered}");
    insta::assert_snapshot!("messages_lens_person_60", rendered);
}

#[test]
fn a_clear_list_says_nobody_is_waiting_with_lapsed_people_as_facts() {
    let page = page(clear(), true);
    for width in [60u16, 80, 120] {
        let rendered = render_at(&page, width, 0);
        assert!(
            rendered.contains("Nobody is waiting on you."),
            "{width}\n{rendered}"
        );
        assert!(
            rendered.contains("Ari usually writes every week"),
            "{width}"
        );
        assert!(!rendered.contains("YOUR TURN"), "{width}");
        insta::assert_snapshot!(format!("messages_lens_clear_{width}"), rendered);
    }
}

#[test]
fn no_card_teaches_at_the_top_even_before_any_hint_is_seen() {
    let unseen = page(populated(), false);
    for width in [60u16, 80, 120] {
        let rendered = render_at(&unseen, width, 0);
        assert!(
            !rendered.contains("Each row is a person"),
            "{width}\n{rendered}"
        );
        assert!(!rendered.contains("Esc close"), "{width}");
    }
}

#[test]
fn got_it_shows_its_exact_text_and_countdown() {
    let mut state = page(populated(), true);
    state.ack = Some(crate::app::AckCountdown {
        plan: mxr_protocol::AckPlanData {
            account_id: AccountId::from_provider_id("fake", "alex"),
            thread_id: ThreadId::new(),
            reply_to_message_id: MessageId::new(),
            to: vec![],
            subject: "Re: Contract renewal".into(),
            text: "Hi Samir,\n\nGot it, thanks.\n\nAlex".into(),
            html: "<p>Hi Samir,</p>\n<p>Got it, thanks.</p>\n<p>Alex</p>".into(),
            built_from: String::new(),
            countdown_seconds: 5,
            dry_run: true,
            from: "alex@example.com".into(),
            preview_token: Some("1.test".into()),
            preview_expires_at: None,
            sent_message_id: None,
        },
        to: "Samir".into(),
        send_at: std::time::Instant::now(),
    });
    let rendered = render_to_string(120, 34, |frame| {
        draw(
            frame,
            Rect::new(0, 0, 120, 34),
            &MessagesView {
                page: &state,
                selected_index: 0,
                active_pane: &ActivePane::MailList,
                now: now(),
                offset: FixedOffset::east_opt(0).unwrap(),
                ack_seconds_left: Some(4),
            },
            &crate::theme::Theme::default(),
        );
    });
    assert!(rendered.contains("Got it to Samir in 4s"), "{rendered}");
    assert!(rendered.contains("Got it, thanks."), "{rendered}");
}

#[test]
fn mail_text_cannot_reach_the_terminal_as_control_sequences() {
    let mut data = populated();
    data.your_turn[0].title = "Sam\u{1b}[31m\nir\u{202e}".into();
    let rendered = render_at(&page(data, true), 120, 0);
    assert!(!rendered.contains('\u{1b}'));
    assert!(!rendered.contains('\u{202e}'));
}

#[test]
fn paragraph_blocks_finds_a_crlf_break_a_naive_split_misses() {
    // A message `new_text` returned with its own line endings kept (see
    // `crates/reader/src/new_text.rs`): a naive `split("\n\n")` never
    // finds "\n\n" inside "\r\n\r\n" and folds this into one paragraph.
    assert_eq!(
        paragraph_blocks("Senior Software Engineer at throxy.\r\n\r\nJack"),
        vec!["Senior Software Engineer at throxy.", "Jack"]
    );
}

#[test]
fn paragraph_blocks_treats_a_run_of_blank_lines_as_one_break() {
    assert_eq!(paragraph_blocks("a\n\n\n\nb"), vec!["a", "b"]);
}

#[test]
fn paragraph_blocks_keeps_single_newlines_in_one_paragraph() {
    assert_eq!(paragraph_blocks("a\nb\nc"), vec!["a\nb\nc"]);
}
