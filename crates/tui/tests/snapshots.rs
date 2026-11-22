use mxr_compose::frontmatter::ComposeFrontmatter;
use mxr_core::id::{AccountId, MessageId};
use mxr_protocol::{MutationCommand, Request};
use mxr_test_support::render_to_string;
use mxr_tui::action::UiContext;
use mxr_tui::app::{MutationEffect, PendingBulkConfirm, PendingSend, PendingSendMode};
use mxr_tui::ui::bulk_confirm_modal::draw as draw_bulk_confirm_modal;
use mxr_tui::ui::command_palette::{draw as draw_command_palette, CommandPalette};
use mxr_tui::ui::send_confirm_modal::draw as draw_send_confirm;
use mxr_tui::ui::unsubscribe_modal::draw as draw_unsubscribe_modal;
use ratatui::layout::Rect;

#[test]
fn command_palette_prompt_row_stays_fixed() {
    let mut broad = CommandPalette::default();
    broad.toggle(UiContext::MailboxList);
    broad.on_char('a');

    let mut narrow = CommandPalette::default();
    narrow.toggle(UiContext::MailboxList);
    for ch in "edit config".chars() {
        narrow.on_char(ch);
    }

    let broad_snapshot = render_to_string(70, 20, |frame| {
        draw_command_palette(
            frame,
            Rect::new(0, 0, 70, 20),
            &broad,
            &mxr_tui::theme::Theme::default(),
        );
    });
    let narrow_snapshot = render_to_string(70, 20, |frame| {
        draw_command_palette(
            frame,
            Rect::new(0, 0, 70, 20),
            &narrow,
            &mxr_tui::theme::Theme::default(),
        );
    });

    let broad_line = broad_snapshot
        .lines()
        .position(|line| line.contains("> a"))
        .expect("broad snapshot should contain selected shortcut");
    let narrow_line = narrow_snapshot
        .lines()
        .position(|line| line.contains("> edit config"))
        .expect("narrow snapshot should contain selected command");
    assert_eq!(broad_line, narrow_line);
}

#[test]
fn send_confirm_snapshot() {
    let pending = PendingSend {
        draft_revision: None,
        account_id: AccountId::new(),
        fm: ComposeFrontmatter {
            to: "bob@example.com".into(),
            cc: String::new(),
            bcc: String::new(),
            subject: "Snapshot draft".into(),
            from: "me@example.com".into(),
            in_reply_to: None,
            intent: mxr_core::DraftIntent::New,
            references: vec![],
            thread_id: None,
            attach: vec![],
            signature: None,
            forward_source: None,
        },
        body: "Hello".into(),
        draft_path: "/tmp/draft.md".into(),
        intent: mxr_core::DraftIntent::New,
        mode: PendingSendMode::SendOrSave,
        safety_report: None,
        safety_check_failed: None,
        override_token: None,
        suggested_collaborators: vec![],
        invite_reply: None,
        resolved_from: None,
        stored_draft: None,
    };

    let snapshot = render_to_string(70, 20, |frame| {
        draw_send_confirm(
            frame,
            Rect::new(0, 0, 70, 20),
            Some(&pending),
            None,
            None,
            &[],
            &mxr_tui::theme::Theme::default(),
            &mxr_tui::ui::account_mark::AccountMarks::default(),
        );
    });
    assert!(snapshot.contains("Send this draft?"));
    assert!(snapshot.contains("Subject: Snapshot draft"));
    assert!(snapshot.contains("[r] draft for me"));
    insta::assert_snapshot!("send_confirm_snapshot", snapshot);
}

#[test]
fn bulk_confirm_snapshot() {
    let pending = PendingBulkConfirm {
        title: "Archive messages".into(),
        detail: "Archive 3 selected messages?".into(),
        request: Request::mutation(MutationCommand::Archive {
            message_ids: vec![MessageId::new()],
        }),
        effect: MutationEffect::RefreshList,
        optimistic_effect: None,
        status_message: "Archiving...".into(),
        preview: Default::default(),
        preview_scroll: 0,
    };

    let snapshot = render_to_string(100, 20, |frame| {
        draw_bulk_confirm_modal(
            frame,
            Rect::new(0, 0, 100, 20),
            Some(&pending),
            &mxr_tui::theme::Theme::default(),
        );
    });
    insta::assert_snapshot!("bulk_confirm_snapshot", snapshot);
}

#[test]
fn unsubscribe_modal_snapshot() {
    let pending = mxr_tui::app::PendingUnsubscribeConfirm {
        message_id: MessageId::new(),
        account_id: AccountId::new(),
        sender_email: "news@example.com".into(),
        method_label: "one-click".into(),
        archive_message_ids: vec![MessageId::new(), MessageId::new()],
    };

    let snapshot = render_to_string(100, 20, |frame| {
        draw_unsubscribe_modal(
            frame,
            Rect::new(0, 0, 100, 20),
            Some(&pending),
            &mxr_tui::theme::Theme::default(),
        );
    });
    insta::assert_snapshot!("unsubscribe_modal_snapshot", snapshot);
}
