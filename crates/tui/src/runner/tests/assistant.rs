//! The assistant pane: hidden until Ctrl-a, takes every key while it has
//! focus, and each key sends the daemon request it names (send, approve,
//! reject, answer, skip, stop, trust, delete after a dry run).

use super::*;
use crate::app::{AssistantCall, AssistantFocus, AssistantPrompt};
use mxr_protocol::{
    AssistantAnswerData, AssistantCallStateData, AssistantConversationData, AssistantDecisionData,
    AssistantDecisionKindData, AssistantDeleteData, AssistantItemData, AssistantPendingData,
    AssistantProgressData, AssistantQuestionData, AssistantQuestionOptionData, AssistantStatusData,
    ResponseData,
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

fn ctrl_a(app: &mut App) {
    press(app, KeyCode::Char('a'), KeyModifiers::CONTROL);
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

/// An open pane showing `conversation`, with the requests it took to get
/// there drained.
fn showing(conversation: AssistantConversationData) -> App {
    let mut app = App::new();
    ctrl_a(&mut app);
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
fn the_pane_is_hidden_by_default() {
    let mut app = App::new();
    assert!(!app.assistant.open);
    let screen = render_to_string(140, 30, |frame| app.draw(frame));
    assert!(!screen.contains("Talk to your mail"), "{screen}");
}

#[test]
fn ctrl_a_opens_the_pane_with_the_keys_and_asks_if_it_can_run() {
    let mut app = App::new();
    ctrl_a(&mut app);
    assert!(app.assistant.open);
    assert_eq!(app.assistant.focus, AssistantFocus::Input);
    assert!(matches!(
        requests(&mut app).as_slice(),
        [Request::GetAssistantAvailability]
    ));
    let wide = render_to_string(140, 30, |frame| app.draw(frame));
    assert!(wide.contains("Talk to your mail"), "{wide}");
    // Narrow terminals get the pane over the right of the screen.
    let narrow = render_to_string(90, 30, |frame| app.draw(frame));
    assert!(narrow.contains("Talk to your mail"), "{narrow}");

    // Esc hands the keys back in two steps; Ctrl-a then focuses and hides.
    press(&mut app, KeyCode::Esc, KeyModifiers::NONE);
    assert_eq!(app.assistant.focus, AssistantFocus::Pane);
    press(&mut app, KeyCode::Esc, KeyModifiers::NONE);
    assert_eq!(app.assistant.focus, AssistantFocus::Away);
    assert!(app.assistant.open);
    ctrl_a(&mut app);
    assert_eq!(app.assistant.focus, AssistantFocus::Input);
    ctrl_a(&mut app);
    assert!(!app.assistant.open);
}

#[test]
fn typing_in_the_pane_never_reaches_the_screen_behind_it() {
    let mut app = App::new();
    app.mailbox.envelopes = make_test_envelopes(3);
    ctrl_a(&mut app);
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
    ctrl_a(&mut app);
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
    // A second Enter while the turn runs sends nothing.
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
    // Something waits, so the keys went to it rather than the message box.
    assert_eq!(app.assistant.focus, AssistantFocus::Pane);
    let screen = render_to_string(140, 40, |frame| app.draw(frame));
    assert!(screen.contains("Archive 2 messages"), "{screen}");
    assert!(screen.contains("Receipt one"), "{screen}");
    press(&mut app, KeyCode::Char('y'), KeyModifiers::NONE);
    assert_eq!(
        decision(&mut app),
        AssistantDecisionData {
            call_id: "call1".into(),
            decision: AssistantDecisionKindData::Approve,
        }
    );
    // A second y before the daemon answers decides nothing more.
    press(&mut app, KeyCode::Char('y'), KeyModifiers::NONE);
    assert!(requests(&mut app).is_empty());
}

#[test]
fn n_asks_why_then_rejects_with_the_reason() {
    let mut app = showing(conversation(
        AssistantStatusData::AwaitingUser,
        vec![approval()],
    ));
    press(&mut app, KeyCode::Char('n'), KeyModifiers::NONE);
    assert!(matches!(
        app.assistant.prompt,
        Some(AssistantPrompt::RejectReason { .. })
    ));
    type_text(&mut app, "keep them");
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    assert_eq!(
        decision(&mut app),
        AssistantDecisionData {
            call_id: "call1".into(),
            decision: AssistantDecisionKindData::Reject {
                reason: Some("keep them".into())
            },
        }
    );
}

#[test]
fn number_keys_choose_and_enter_answers() {
    let mut app = showing(conversation(
        AssistantStatusData::AwaitingUser,
        vec![question()],
    ));
    // Enter with nothing chosen sends nothing.
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    assert!(requests(&mut app).is_empty());
    press(&mut app, KeyCode::Char('2'), KeyModifiers::NONE);
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    assert_eq!(
        decision(&mut app),
        AssistantDecisionData {
            call_id: "q1".into(),
            decision: AssistantDecisionKindData::Answer {
                answers: vec![AssistantAnswerData {
                    selected: vec!["All".into()],
                    other: None,
                }],
            },
        }
    );
}

#[test]
fn an_own_answer_goes_as_other() {
    let mut app = showing(conversation(
        AssistantStatusData::AwaitingUser,
        vec![question()],
    ));
    press(&mut app, KeyCode::Char('o'), KeyModifiers::NONE);
    type_text(&mut app, "only Acme");
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    assert_eq!(
        decision(&mut app),
        AssistantDecisionData {
            call_id: "q1".into(),
            decision: AssistantDecisionKindData::Answer {
                answers: vec![AssistantAnswerData {
                    selected: Vec::new(),
                    other: Some("only Acme".into()),
                }],
            },
        }
    );
}

#[test]
fn s_skips_the_question() {
    let mut app = showing(conversation(
        AssistantStatusData::AwaitingUser,
        vec![question()],
    ));
    press(&mut app, KeyCode::Char('s'), KeyModifiers::NONE);
    assert_eq!(
        decision(&mut app),
        AssistantDecisionData {
            call_id: "q1".into(),
            decision: AssistantDecisionKindData::Reject { reason: None },
        }
    );
}

#[test]
fn ctrl_c_stops_a_running_turn() {
    let mut app = showing(conversation(AssistantStatusData::Running, Vec::new()));
    press(&mut app, KeyCode::Char('c'), KeyModifiers::CONTROL);
    assert!(matches!(
        requests(&mut app).as_slice(),
        [Request::AssistantCancel { conversation_id }] if conversation_id == "c1"
    ));
}

#[test]
fn updates_refetch_the_open_conversation_one_at_a_time() {
    let mut app = showing(conversation(AssistantStatusData::Running, Vec::new()));
    for step in 1..=3 {
        handle_daemon_event(
            &mut app,
            DaemonEvent::AssistantUpdated {
                conversation_id: "c1".into(),
                status: AssistantStatusData::Running,
                progress: Some(AssistantProgressData::Thinking { step }),
            },
        );
    }
    handle_daemon_event(
        &mut app,
        DaemonEvent::AssistantUpdated {
            conversation_id: "other".into(),
            status: AssistantStatusData::Running,
            progress: None,
        },
    );
    let refetches = app.assistant.take_outbound();
    assert_eq!(refetches.len(), 1, "one refetch in flight at a time");
    assert!(matches!(
        &refetches[0].request,
        Request::GetAssistantConversation { conversation_id } if conversation_id == "c1"
    ));
    assert_eq!(
        app.assistant.progress,
        Some(AssistantProgressData::Thinking { step: 3 })
    );
    // The answer lands, and the events that came meanwhile ask once more.
    let mut done = conversation(AssistantStatusData::Idle, Vec::new());
    done.items.push(AssistantItemData::ToolCall {
        seq: 2,
        call_id: "s1".into(),
        tool: "mxr_search".into(),
        reason: "Find the receipts".into(),
        arguments: json!({}),
        state: AssistantCallStateData::Done,
        result: None,
        readout: None,
        created_at: 0,
    });
    app.apply_assistant_result(
        refetches[0].call.clone(),
        Ok(ResponseData::AssistantConversation { conversation: done }),
    );
    assert_eq!(app.assistant.take_outbound().len(), 1);
    assert_eq!(
        app.assistant.conversation.as_ref().map(|c| c.items.len()),
        Some(2)
    );
}

#[test]
fn delete_asks_the_daemon_first_and_confirms() {
    let mut app = showing(conversation(AssistantStatusData::Idle, Vec::new()));
    press(&mut app, KeyCode::Esc, KeyModifiers::NONE);
    press(&mut app, KeyCode::Char('D'), KeyModifiers::SHIFT);
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

#[test]
fn t_toggles_auto_approve_for_the_conversation() {
    let mut app = showing(conversation(AssistantStatusData::Idle, Vec::new()));
    press(&mut app, KeyCode::Esc, KeyModifiers::NONE);
    press(&mut app, KeyCode::Char('T'), KeyModifiers::SHIFT);
    assert!(matches!(
        requests(&mut app).as_slice(),
        [Request::SetAssistantAutoApprove { conversation_id, enabled: true }] if conversation_id == "c1"
    ));
}

fn summary(id: &str, title: &str) -> mxr_protocol::AssistantConversationSummaryData {
    mxr_protocol::AssistantConversationSummaryData {
        id: id.into(),
        title: title.into(),
        status: AssistantStatusData::Idle,
        last_outcome: None,
        created_at: 0,
        updated_at: 0,
        message_count: 2,
        pending_count: 0,
    }
}

#[test]
fn shift_d_in_the_history_deletes_the_highlighted_conversation_after_a_dry_run() {
    let mut app = showing(conversation(AssistantStatusData::Idle, Vec::new()));
    press(&mut app, KeyCode::Esc, KeyModifiers::NONE);
    press(&mut app, KeyCode::Char('L'), KeyModifiers::SHIFT);
    requests(&mut app);
    app.apply_assistant_result(
        AssistantCall::List,
        Ok(ResponseData::AssistantConversations {
            conversations: vec![summary("c1", "Receipts"), summary("c2", "Old trip")],
        }),
    );
    press(&mut app, KeyCode::Char('j'), KeyModifiers::NONE);
    press(&mut app, KeyCode::Char('D'), KeyModifiers::SHIFT);
    let preview = app.assistant.take_outbound();
    assert!(matches!(
        preview.as_slice(),
        [outbound] if matches!(&outbound.request, Request::DeleteAssistantConversation { conversation_id, dry_run: true } if conversation_id == "c2")
    ));
    app.apply_assistant_result(
        preview[0].call.clone(),
        Ok(ResponseData::AssistantDelete {
            delete: AssistantDeleteData {
                conversation_id: "c2".into(),
                title: "Old trip".into(),
                message_count: 2,
                dry_run: true,
                deleted: false,
            },
        }),
    );
    let screen = render_to_string(140, 40, |frame| app.draw(frame));
    assert!(screen.contains("Delete \"Old trip\""), "{screen}");
    press(&mut app, KeyCode::Char('y'), KeyModifiers::NONE);
    let delete = app.assistant.take_outbound();
    assert!(matches!(
        delete.as_slice(),
        [outbound] if matches!(&outbound.request, Request::DeleteAssistantConversation { conversation_id, dry_run: false } if conversation_id == "c2")
    ));
    app.apply_assistant_result(
        delete[0].call.clone(),
        Ok(ResponseData::AssistantDelete {
            delete: AssistantDeleteData {
                conversation_id: "c2".into(),
                title: "Old trip".into(),
                message_count: 2,
                dry_run: false,
                deleted: true,
            },
        }),
    );
    // The list stays open without it; the open conversation is untouched.
    let list = app.assistant.list.as_ref().expect("the list stays open");
    assert_eq!(list.conversations.len(), 1);
    assert_eq!(list.selected, 0);
    assert_eq!(app.assistant.conversation_id.as_deref(), Some("c1"));
}

#[test]
fn deleting_the_open_conversation_from_the_history_starts_a_fresh_one() {
    let mut app = showing(conversation(AssistantStatusData::Idle, Vec::new()));
    press(&mut app, KeyCode::Esc, KeyModifiers::NONE);
    press(&mut app, KeyCode::Char('L'), KeyModifiers::SHIFT);
    requests(&mut app);
    app.apply_assistant_result(
        AssistantCall::List,
        Ok(ResponseData::AssistantConversations {
            conversations: vec![summary("c1", "Receipts")],
        }),
    );
    app.apply_assistant_result(
        AssistantCall::Delete,
        Ok(ResponseData::AssistantDelete {
            delete: AssistantDeleteData {
                conversation_id: "c1".into(),
                title: "Receipts".into(),
                message_count: 2,
                dry_run: false,
                deleted: true,
            },
        }),
    );
    assert_eq!(app.assistant.conversation_id, None);
    assert!(app.assistant.conversation.is_none());
    assert!(app
        .assistant
        .list
        .as_ref()
        .is_some_and(|list| list.conversations.is_empty()));
}

#[test]
fn x_shows_and_hides_the_exact_call_of_the_waiting_change() {
    let mut app = showing(conversation(
        AssistantStatusData::AwaitingUser,
        vec![approval()],
    ));
    let folded = render_to_string(140, 40, |frame| app.draw(frame));
    assert!(!folded.contains("EXACT CALL"), "{folded}");
    press(&mut app, KeyCode::Char('x'), KeyModifiers::NONE);
    let open = render_to_string(140, 40, |frame| app.draw(frame));
    assert!(open.contains("EXACT CALL"), "{open}");
    assert!(open.contains("mxr_mutate {"), "{open}");
    assert!(open.contains("\"action\": \"archive\""), "{open}");
    press(&mut app, KeyCode::Char('x'), KeyModifiers::NONE);
    assert_eq!(app.assistant.exact_call_for, None);
    assert!(requests(&mut app).is_empty(), "x decides nothing");
}

#[test]
fn toasts_stack_beside_the_pane_and_leave_its_hint_line_visible() {
    for width in [140, 90] {
        let mut app = showing(conversation(AssistantStatusData::Idle, Vec::new()));
        press(&mut app, KeyCode::Esc, KeyModifiers::NONE);
        app.push_toast(crate::app::Toast::success("Archived 3 messages"));
        let screen = render_to_string(width, 30, |frame| app.draw(frame));
        assert!(screen.contains("Archived 3 messages"), "{screen}");
        assert!(screen.contains("L history · T trust · D"), "{screen}");
    }
}
