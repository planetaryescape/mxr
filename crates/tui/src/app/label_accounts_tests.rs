//! By-name label actions on a mailbox that spans accounts. The same name is a
//! different label on every account, so each message gets its own account's
//! label, and a selection with an account that lacks the label is refused.

use super::*;
use crate::test_fixtures::TestEnvelopeBuilder;
use mxr_core::id::{AccountId, LabelId};

fn label(account_id: &AccountId, name: &str, provider_id: &str) -> Label {
    Label {
        id: LabelId::new(),
        account_id: account_id.clone(),
        name: name.into(),
        kind: LabelKind::User,
        color: None,
        provider_id: provider_id.into(),
        unread_count: 0,
        total_count: 0,
        role: None,
    }
}

fn envelope(account_id: &AccountId) -> Envelope {
    TestEnvelopeBuilder::new()
        .account_id(account_id.clone())
        .build()
}

/// Two accounts, A (default) and B. `b_has_work` decides whether B has a
/// "Work" label; A always has one, with provider id "a-work".
fn two_accounts(b_has_work: bool) -> (App, AccountId, AccountId, Vec<Envelope>) {
    let account_a = AccountId::new();
    let account_b = AccountId::new();
    let mut app = App::new();
    app.mailbox.mail_list_mode = MailListMode::Messages;
    app.mailbox.labels = vec![label(&account_a, "Work", "a-work")];
    if b_has_work {
        app.mailbox.labels.push(label(&account_b, "Work", "b-work"));
    }
    let envelopes = vec![envelope(&account_a), envelope(&account_b)];
    app.mailbox.envelopes = envelopes.clone();
    app.mailbox.all_envelopes = envelopes.clone();
    (app, account_a, account_b, envelopes)
}

#[test]
fn apply_label_to_a_message_from_b_uses_bs_label() {
    let (mut app, _a, _b, envelopes) = two_accounts(true);
    app.mailbox.selected_index = 1;
    app.modals.pending_label_action = Some((LabelPickerMode::Apply, "Work".into()));

    app.apply(Action::ApplyLabel);

    let message_b = &app.mailbox.envelopes[1];
    assert_eq!(message_b.id, envelopes[1].id);
    assert_eq!(message_b.label_provider_ids, vec!["b-work".to_string()]);
    assert_eq!(app.pending_mutation_queue.len(), 1);
}

#[test]
fn mixed_selection_projects_each_accounts_own_label() {
    let (mut app, _a, _b, envelopes) = two_accounts(true);
    app.mailbox.selected_set = envelopes.iter().map(|e| e.id.clone()).collect();
    let ids: Vec<MessageId> = envelopes.iter().map(|e| e.id.clone()).collect();

    app.apply_local_label_refs(&ids, &["Work".into()], &[]);

    assert_eq!(
        app.mailbox.envelopes[0].label_provider_ids,
        vec!["a-work".to_string()]
    );
    assert_eq!(
        app.mailbox.envelopes[1].label_provider_ids,
        vec!["b-work".to_string()]
    );
}

#[test]
fn mixed_selection_is_previewed_for_confirmation_with_every_message() {
    let (mut app, _a, _b, envelopes) = two_accounts(true);
    app.mailbox.selected_set = envelopes.iter().map(|e| e.id.clone()).collect();
    app.modals.pending_label_action = Some((LabelPickerMode::Apply, "Work".into()));

    app.apply(Action::ApplyLabel);

    let confirm = app
        .modals
        .pending_bulk_confirm
        .as_ref()
        .expect("a two-message apply asks for confirmation");
    assert_eq!(confirm.preview.rows.len(), 2);
    assert!(app.pending_mutation_queue.is_empty());
}

#[test]
fn apply_label_refuses_when_the_message_account_lacks_it() {
    let (mut app, _a, b, _envelopes) = two_accounts(false);
    app.mailbox.selected_index = 1;
    app.modals.pending_label_action = Some((LabelPickerMode::Apply, "Work".into()));

    app.apply(Action::ApplyLabel);

    let status = app.status_message.clone().unwrap_or_default();
    assert!(
        status.contains("'Work'"),
        "status names the label: {status}"
    );
    assert!(
        status.contains(&app.account_display_name(&b)),
        "status names the account: {status}"
    );
    assert!(app.pending_mutation_queue.is_empty());
    assert!(app.modals.pending_bulk_confirm.is_none());
    assert!(app.mailbox.envelopes[1].label_provider_ids.is_empty());
}

#[test]
fn move_refuses_a_mixed_selection_when_one_account_lacks_the_label() {
    let (mut app, _a, b, envelopes) = two_accounts(false);
    app.mailbox.selected_set = envelopes.iter().map(|e| e.id.clone()).collect();
    app.modals.pending_label_action = Some((LabelPickerMode::Move, "Work".into()));

    app.apply(Action::MoveToLabel);

    let status = app.status_message.clone().unwrap_or_default();
    assert!(status.contains(&app.account_display_name(&b)), "{status}");
    assert!(app.modals.pending_bulk_confirm.is_none());
    assert!(app.pending_mutation_queue.is_empty());
}

#[test]
fn picker_offers_the_target_accounts_labels_once_per_name() {
    let (mut app, a, b, envelopes) = two_accounts(true);
    app.mailbox.labels.push(label(&a, "Errands", "a-errands"));
    app.mailbox.labels.push(label(&b, "Personal", "b-personal"));

    app.mailbox.selected_index = 1;
    let names: Vec<String> = app
        .label_picker_labels()
        .into_iter()
        .map(|label| label.name)
        .collect();
    assert_eq!(names, vec!["Work".to_string(), "Personal".to_string()]);

    app.mailbox.selected_set = envelopes.iter().map(|e| e.id.clone()).collect();
    let names: Vec<String> = app
        .label_picker_labels()
        .into_iter()
        .map(|label| label.name)
        .collect();
    assert_eq!(
        names,
        vec![
            "Work".to_string(),
            "Errands".to_string(),
            "Personal".to_string()
        ]
    );
}

#[test]
fn moving_a_second_account_message_to_a_system_mailbox_is_not_refused() {
    // The label list keeps one merged entry per system mailbox, the default
    // account's, so B has no TRASH of its own in it.
    let (mut app, a, _b, _envelopes) = two_accounts(false);
    let mut trash = label(&a, "TRASH", "TRASH");
    trash.kind = LabelKind::System;
    app.mailbox.labels.push(trash);
    app.mailbox.selected_index = 1;
    app.modals.pending_label_action = Some((LabelPickerMode::Move, "TRASH".into()));

    app.apply(Action::MoveToLabel);

    let status = app.status_message.clone().unwrap_or_default();
    assert!(!status.starts_with("No label"), "{status}");
    assert_eq!(app.pending_mutation_queue.len(), 1);
}

#[test]
fn a_name_no_loaded_label_has_is_left_to_the_daemon() {
    // The label list may not have loaded yet, or may predate a label made
    // in another client, so only a name another account has is refused.
    let (mut app, _a, _b, _envelopes) = two_accounts(false);
    app.mailbox.selected_index = 1;
    app.modals.pending_label_action = Some((LabelPickerMode::Apply, "Made elsewhere".into()));

    app.apply(Action::ApplyLabel);

    let status = app.status_message.clone().unwrap_or_default();
    assert!(!status.starts_with("No label"), "{status}");
    assert_eq!(app.pending_mutation_queue.len(), 1);
}
