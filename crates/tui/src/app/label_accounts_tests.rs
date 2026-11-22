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
