use super::*;
use crate::app::{AssistantCall, AssistantFocus};
use mxr_protocol::{
    AssistantConversationData, AssistantDecisionData, AssistantDecisionKindData,
    AssistantDeleteData, AssistantItemData, AssistantPendingData, AssistantQuestionData,
    AssistantQuestionOptionData, AssistantStatusData, ResponseData,
};
use serde_json::json;

fn press(app: &mut App, code: KeyCode, modifiers: KeyModifiers) {
    if let Some(action) = app.handle_key(KeyEvent::new(code, modifiers)) {
        app.apply(action);
    }
}

fn type_text(app: &mut App, text: &str) {
    for c in text.chars() {
        let modifiers = if c.is_ascii_uppercase() {
            KeyModifiers::SHIFT
        } else {
            KeyModifiers::NONE
        };
        press(app, KeyCode::Char(c), modifiers);
    }
}

fn at(app: &mut App) {
    press(app, KeyCode::Char('@'), KeyModifiers::SHIFT);
}

fn requests(app: &mut App) -> Vec<Request> {
    app.assistant
        .take_outbound()
        .into_iter()
        .map(|outbound| outbound.request)
        .collect()
}

fn conversation(
    status: AssistantStatusData,
    pending: Vec<AssistantPendingData>,
) -> AssistantConversationData {
    AssistantConversationData {
        last_error_detail: None,
        id: "c1".into(),
        title: "Receipts".into(),
        status,
        last_outcome: None,
        last_error: None,
        created_at: 0,
        updated_at: 0,
        auto_approve: false,
        items: vec![AssistantItemData::User {
            seq: 1,
            text: "Archive the receipts".into(),
            created_at: 0,
        }],
        pending,
    }
}

fn approval() -> AssistantPendingData {
    AssistantPendingData::Approval {
        title: Some("Archive 2 emails".into()),
        warning: None,
        call_id: "call1".into(),
        tool: "mxr_mutate".into(),
        arguments: json!({"action": "archive", "message_ids": ["m1", "m2"]}),
        reason: "Receipts you already filed".into(),
        preview: Some(json!({"messages": [
            {"subject": "Receipt one", "from": {"name": "Acme"}},
            {"subject": "Receipt two", "from": {"name": "Acme"}}
        ]})),
        preview_error: None,
    }
}

fn question() -> AssistantPendingData {
    AssistantPendingData::Question {
        call_id: "q1".into(),
        reason: "Which ones".into(),
        questions: vec![AssistantQuestionData {
            question: "Which receipts?".into(),
            header: None,
            options: vec![
                AssistantQuestionOptionData {
                    label: "This year".into(),
                    description: None,
                },
                AssistantQuestionOptionData {
                    label: "All".into(),
                    description: None,
                },
            ],
            multi_select: false,
            allow_other: true,
        }],
    }
}

fn showing(conversation: AssistantConversationData) -> App {
    let mut app = App::new();
    at(&mut app);
    app.assistant.conversation_id = Some(conversation.id.clone());
    app.apply_assistant_result(
        AssistantCall::Conversation,
        Ok(ResponseData::AssistantConversation { conversation }),
    );
    requests(&mut app);
    app
}

fn decision(app: &mut App) -> AssistantDecisionData {
    match requests(app).as_slice() {
        [Request::AssistantDecide {
            conversation_id,
            decisions,
        }] => {
            assert_eq!(conversation_id, "c1");
            assert_eq!(decisions.len(), 1);
            decisions[0].clone()
        }
        other => panic!("expected one AssistantDecide, got {other:?}"),
    }
}

#[test]
fn at_opens_the_pane_with_the_keys_and_asks_if_it_can_run() {
    let mut app = App::new();
    at(&mut app);
    assert!(app.assistant.open);
    assert_eq!(app.assistant.focus, AssistantFocus::Input);
    assert!(matches!(
        requests(&mut app).as_slice(),
        [Request::GetAssistantAvailability]
    ));
    let wide = render_to_string(140, 30, |frame| app.draw(frame));
    assert!(wide.contains("Talk to your mail"), "{wide}");
    let narrow = render_to_string(90, 30, |frame| app.draw(frame));
    assert!(narrow.contains("Talk to your mail"), "{narrow}");

    press(&mut app, KeyCode::Esc, KeyModifiers::NONE);
    assert_eq!(app.assistant.focus, AssistantFocus::Pane);
    press(&mut app, KeyCode::Esc, KeyModifiers::NONE);
    assert_eq!(app.assistant.focus, AssistantFocus::Away);
    assert!(app.assistant.open);
    at(&mut app);
    assert_eq!(app.assistant.focus, AssistantFocus::Input);
    at(&mut app);
    assert_eq!(app.assistant.input, "@", "@ types in the input");
    press(&mut app, KeyCode::Esc, KeyModifiers::NONE);
    at(&mut app);
    assert!(!app.assistant.open);

    press(&mut app, KeyCode::Char('a'), KeyModifiers::CONTROL);
    assert!(!app.assistant.open);
}

#[test]
fn typing_in_the_pane_never_reaches_the_screen_behind_it() {
    let mut app = App::new();
    app.mailbox.envelopes = make_test_envelopes(3);
    at(&mut app);
    requests(&mut app);
    type_text(&mut app, "e#!q archive these");
    assert_eq!(app.assistant.input, "e#!q archive these");
    assert!(app.pending_mutation_queue.is_empty());
    assert!(!app.should_quit);
    assert!(app.assistant.take_outbound().is_empty());
}

#[test]
fn enter_sends_the_message_with_what_is_open() {
    let mut app = App::new();
    let envelopes = make_test_envelopes(2);
    app.mailbox.selected_set.insert(envelopes[0].id.clone());
    app.mailbox.envelopes = envelopes.clone();
    at(&mut app);
    requests(&mut app);
    type_text(&mut app, "Summarise these");
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    match requests(&mut app).as_slice() {
        [Request::AssistantSend {
            conversation_id,
            message,
            context,
        }] => {
            assert_eq!(conversation_id, &None);
            assert_eq!(message, "Summarise these");
            assert!(context.view.is_some());
            assert_eq!(
                context.selected_message_ids,
                vec![envelopes[0].id.to_string()]
            );
        }
        other => panic!("expected AssistantSend, got {other:?}"),
    }
    assert!(app.assistant.input.is_empty());
    assert!(app.assistant.running());
    type_text(&mut app, "more");
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    assert!(requests(&mut app).is_empty());
}

#[test]
fn y_approves_the_waiting_change() {
    let mut app = showing(conversation(
        AssistantStatusData::AwaitingUser,
        vec![approval()],
    ));
    assert_eq!(app.assistant.focus, AssistantFocus::Pane);
    let screen = render_to_string(140, 40, |frame| app.draw(frame));
    assert!(screen.contains("Archive 2 emails"), "{screen}");
    assert!(screen.contains("Receipt one"), "{screen}");
    press(&mut app, KeyCode::Char('y'), KeyModifiers::NONE);
    assert_eq!(
        decision(&mut app),
        AssistantDecisionData {
            call_id: "call1".into(),
            decision: AssistantDecisionKindData::Approve,
        }
    );
    press(&mut app, KeyCode::Char('y'), KeyModifiers::NONE);
    assert!(requests(&mut app).is_empty());
}

#[test]
fn esc_and_q_leave_the_question_waiting() {
    for key in [KeyCode::Esc, KeyCode::Char('q')] {
        let mut app = showing(conversation(
            AssistantStatusData::AwaitingUser,
            vec![question()],
        ));
        let _ = requests(&mut app);
        press(&mut app, key, KeyModifiers::NONE);
        assert!(
            requests(&mut app).is_empty(),
            "{key:?} decided the question"
        );
        assert_eq!(app.assistant.focus, AssistantFocus::Away, "{key:?}");
        assert_eq!(app.assistant.pending().len(), 1, "{key:?}");
    }
}

#[test]
fn delete_asks_the_daemon_first_and_confirms() {
    let mut app = showing(conversation(AssistantStatusData::Idle, Vec::new()));
    press(&mut app, KeyCode::Esc, KeyModifiers::NONE);
    press(&mut app, KeyCode::Char('#'), KeyModifiers::SHIFT);
    let preview = app.assistant.take_outbound();
    assert!(matches!(
        preview.as_slice(),
        [outbound] if matches!(&outbound.request, Request::DeleteAssistantConversation { dry_run: true, .. })
    ));
    app.apply_assistant_result(
        preview[0].call.clone(),
        Ok(ResponseData::AssistantDelete {
            delete: AssistantDeleteData {
                conversation_id: "c1".into(),
                title: "Receipts".into(),
                message_count: 4,
                dry_run: true,
                deleted: false,
            },
        }),
    );
    let screen = render_to_string(140, 40, |frame| app.draw(frame));
    assert!(screen.contains("its 4 messages"), "{screen}");
    press(&mut app, KeyCode::Char('y'), KeyModifiers::NONE);
    assert!(matches!(
        requests(&mut app).as_slice(),
        [Request::DeleteAssistantConversation { conversation_id, dry_run: false }] if conversation_id == "c1"
    ));
}

const LINKED_THREAD: &str = "019e6f2a-1234-7000-8000-abcdef000001";

fn linked_reply() -> App {
    let mut conversation = conversation(AssistantStatusData::Idle, vec![]);
    conversation.items.push(AssistantItemData::Assistant {
        rows: Vec::new(),
        seq: 2,
        text: format!(
            "Reply to [Q1 report](mxr:thread/{LINKED_THREAD}) from \
             [Jon Bell](mxr:person/person:jon@example.com), not \
             [Ann](mxr:person/person:ann@elsewhere.test). See [Q1 again](mxr:thread/{LINKED_THREAD})."
        ),
        model: None,
        leaves_machine: None,
        failed_call_ids: Vec::new(),
        created_at: 0,
    });
    let mut app = showing(conversation);
    app.assistant.focus = AssistantFocus::Pane;
    app
}

#[test]
fn a_number_opens_the_linked_email_in_the_reader() {
    let mut app = linked_reply();
    press(&mut app, KeyCode::Char('1'), KeyModifiers::NONE);
    let thread_id: mxr_core::ThreadId = LINKED_THREAD.parse().unwrap();
    assert_eq!(app.pending_assistant_thread_open.as_ref(), Some(&thread_id));
    assert_eq!(app.assistant.focus, AssistantFocus::Away);
    assert!(app.assistant.open, "the pane stays open beside the email");

    let envelope = crate::test_fixtures::TestEnvelopeBuilder::new()
        .thread_id(thread_id.clone())
        .subject("Q1 report")
        .build();
    app.apply(Action::OpenNow);
    app.open_assistant_thread(vec![envelope.clone()]);
    assert_eq!(app.screen, Screen::Mailbox);
    assert_eq!(
        app.mailbox.mailbox_view,
        MailboxView::Messages,
        "a mode lens has no reader beside it, so the email opens in All Mail"
    );
    assert_eq!(app.mailbox.active_pane, ActivePane::MessageView);
    assert_eq!(
        app.mailbox.viewing_envelope.as_ref().map(|e| &e.id),
        Some(&envelope.id)
    );
}

fn model_choice(
    id: &str,
    label: &str,
    cloud: bool,
    reason: Option<&str>,
) -> mxr_protocol::AssistantModelChoiceData {
    mxr_protocol::AssistantModelChoiceData {
        id: id.into(),
        label: label.into(),
        cloud,
        sees: "this conversation only".into(),
        available: reason.is_none(),
        unavailable_reason: reason.map(str::to_string),
    }
}

fn models() -> mxr_protocol::AssistantModelsData {
    mxr_protocol::AssistantModelsData {
        current: Some("gemma4:latest".into()),
        choices: vec![
            model_choice("gemma4:latest", "Gemma 4", false, None),
            model_choice(
                "gpt-6-luna",
                "GPT-6 Luna",
                true,
                Some("Add an OpenAI key: mxr llm key set openai"),
            ),
            model_choice("claude-haiku-5-5", "Claude Haiku 5.5", true, None),
        ],
    }
}

fn picking() -> App {
    let mut app = showing(conversation(AssistantStatusData::Idle, Vec::new()));
    press(&mut app, KeyCode::Esc, KeyModifiers::NONE);
    press(&mut app, KeyCode::Char('M'), KeyModifiers::SHIFT);
    assert!(matches!(
        requests(&mut app).as_slice(),
        [Request::GetAssistantModels]
    ));
    app.apply_assistant_result(
        AssistantCall::Models,
        Ok(ResponseData::AssistantModels { models: models() }),
    );
    app
}

#[test]
fn enter_on_an_available_model_asks_the_daemon_to_set_it_and_a_disabled_one_says_why() {
    let mut app = picking();
    press(&mut app, KeyCode::Char('j'), KeyModifiers::NONE);
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    assert!(requests(&mut app).is_empty());
    assert_eq!(
        app.assistant.error.as_deref(),
        Some("Add an OpenAI key: mxr llm key set openai")
    );

    press(&mut app, KeyCode::Char('j'), KeyModifiers::NONE);
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    let sent = app.assistant.take_outbound();
    assert!(matches!(
        sent.as_slice(),
        [outbound] if matches!(&outbound.request, Request::SetAssistantModel { model, dry_run: false } if model == "claude-haiku-5-5")
    ));
    app.apply_assistant_result(
        sent[0].call.clone(),
        Ok(ResponseData::AssistantModelChange {
            change: mxr_protocol::AssistantModelChangeData {
                dry_run: false,
                changed: true,
                previous: Some("gemma4:latest".into()),
                models: mxr_protocol::AssistantModelsData {
                    current: Some("claude-haiku-5-5".into()),
                    ..models()
                },
            },
        }),
    );
    assert!(app.assistant.models.is_none());
    assert!(matches!(
        requests(&mut app).as_slice(),
        [Request::GetAssistantAvailability]
    ));
}
