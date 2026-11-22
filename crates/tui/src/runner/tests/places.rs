use super::*;
use mxr_protocol::{
    KindRuleData, MailKindData, MailPlaceData, PlaceBundleData, PlaceMessageData, SenderKindData,
    SweepPreviewData,
};

fn message(subject: &str, pinned: bool) -> PlaceMessageData {
    PlaceMessageData {
        message_id: mxr_core::MessageId::new(),
        thread_id: mxr_core::ThreadId::new(),
        subject: subject.into(),
        snippet: String::new(),
        date: chrono::Utc::now(),
        unread: true,
        pinned,
        starred: false,
    }
}

fn bundle(sender: &str, messages: Vec<PlaceMessageData>) -> PlaceBundleData {
    PlaceBundleData {
        account_id: mxr_core::AccountId::new(),
        sender_email: sender.into(),
        sender_name: None,
        kind: MailKindData {
            kind: SenderKindData::PaperTrail,
            rule: KindRuleData::AutomatedAddress,
            reason: "automated sender".into(),
            corrected: false,
        },
        message_count: messages.len() as u32,
        unread_count: 0,
        pinned_count: messages.iter().filter(|m| m.pinned).count() as u32,
        newest_at: chrono::Utc::now(),
        newest_subject: messages[0].subject.clone(),
        messages,
    }
}

fn paper_trail(bundles: Vec<PlaceBundleData>) -> App {
    let mut app = App::new();
    app.apply(Action::OpenPlace(MailPlaceData::PaperTrail));
    assert_eq!(
        app.mailbox.pending_place_refresh,
        Some(MailPlaceData::PaperTrail),
        "opening a place fetches it"
    );
    app.set_place(
        &refresh(MailPlaceData::PaperTrail),
        crate::app::PlacePageState {
            place: Some(MailPlaceData::PaperTrail),
            total_bundles: bundles.len() as u32,
            total_messages: bundles.iter().map(|b| b.message_count).sum(),
            bundles,
            loaded: true,
        },
    );
    app
}

fn refresh(place: MailPlaceData) -> crate::app::PlaceFetch {
    crate::app::PlacePageState::default().refresh_fetch(place)
}

fn press(app: &mut App, code: KeyCode) {
    if let Some(action) = app.handle_key(KeyEvent::new(code, KeyModifiers::NONE)) {
        app.apply(action);
    }
}

fn press_shifted(app: &mut App, c: char) {
    if let Some(action) = app.handle_key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::SHIFT)) {
        app.apply(action);
    }
}

fn queued(app: &App) -> Vec<Request> {
    app.pending_mutation_queue
        .iter()
        .map(|queued| queued.request.clone())
        .collect()
}

#[test]
fn s_pins_the_message_under_the_cursor_and_shows_it_at_once() {
    let receipt = message("Receipt 1", false);
    let mut app = paper_trail(vec![bundle(
        "receipts@shop.example",
        vec![receipt.clone(), message("Receipt 0", false)],
    )]);
    press(&mut app, KeyCode::Char('s'));
    assert!(matches!(
        queued(&app).as_slice(),
        [Request::PinMessages { message_ids, pinned: true }] if message_ids == &vec![receipt.message_id.clone()]
    ));
    assert!(app.selected_place_row().unwrap().1.pinned, "optimistic");
    assert_eq!(app.mailbox.place_page.bundles[0].pinned_count, 1);
}

#[test]
fn capital_p_moves_the_sender_and_the_menu_says_where_it_goes() {
    let shop = bundle("receipts@shop.example", vec![message("Receipt", false)]);
    let mut app = paper_trail(vec![shop.clone()]);
    press_shifted(&mut app, 'K');
    assert!(app.mailbox.sender_kind_menu.is_none(), "K moved to P");
    press_shifted(&mut app, 'P');
    assert_eq!(
        app.mailbox
            .sender_kind_menu
            .as_ref()
            .map(|menu| menu.sender_email.as_str()),
        Some("receipts@shop.example")
    );
    press(&mut app, KeyCode::Char('m'));
    assert!(app.mailbox.sender_kind_menu.is_none());
    assert!(matches!(
        queued(&app).as_slice(),
        [Request::SetSenderKind { account_id, sender_email, kind: Some(SenderKindData::People) }]
            if account_id == &shop.account_id && sender_email == "receipts@shop.example"
    ));

    app.pending_mutation_queue.clear();
    press_shifted(&mut app, 'P');
    press(&mut app, KeyCode::Char('a'));
    assert!(matches!(
        queued(&app).as_slice(),
        [Request::SetSenderKind { kind: None, .. }]
    ));
}

#[test]
fn the_bundle_sweep_previews_and_enter_runs_exactly_that_preview() {
    let shop = bundle(
        "receipts@shop.example",
        vec![message("Receipt 1", true), message("Receipt 0", false)],
    );
    let mut app = paper_trail(vec![shop.clone()]);
    press_shifted(&mut app, 'S');
    assert!(app.mailbox.pending_sweep_preview.is_none());
    app.apply(Action::SweepBundle);
    let target = app
        .mailbox
        .pending_sweep_preview
        .clone()
        .expect("a dry run is requested first");
    assert_eq!(
        target.sender_email.as_deref(),
        Some("receipts@shop.example")
    );
    assert_eq!(target.account_id.as_ref(), Some(&shop.account_id));
    assert!(matches!(
        crate::runner::sweep_request(&target, true, None),
        Request::SweepPlace {
            dry_run: true,
            preview_token: None,
            ..
        }
    ));

    let preview = SweepPreviewData {
        place: MailPlaceData::PaperTrail,
        sender_email: target.sender_email.clone(),
        count: 1,
        pinned_excluded: 1,
        senders: vec![],
        sample_subjects: vec!["Receipt 0".into()],
        preview_token: Some("tok-42".into()),
    };
    app.show_sweep_preview(crate::app::PendingSweepConfirm {
        target: target.clone(),
        preview,
        shown: 0,
        sweep_focused: false,
        from_updates: false,
    });
    assert_eq!(
        app.mailbox.sweep_confirm.as_ref().map(|c| c.shown),
        Some(1),
        "the unpinned message the lens shows for that sender"
    );
    press(&mut app, KeyCode::Char('p'));
    assert!(queued(&app).is_empty());
    press(&mut app, KeyCode::Enter);
    let confirmed = app.mailbox.pending_sweep.clone().expect("sweep queued");
    assert!(matches!(
        crate::runner::sweep_request(
            &confirmed.target,
            false,
            confirmed.preview.preview_token.clone()
        ),
        Request::SweepPlace {
            dry_run: false,
            preview_token: Some(ref token),
            sender_email: Some(_),
            ..
        } if token == "tok-42"
    ));
}

#[test]
fn a_whole_place_sweep_undoes_every_chunk_with_one_u() {
    let mut app = paper_trail(vec![bundle(
        "receipts@shop.example",
        vec![message("Receipt", false)],
    )]);
    press_shifted(&mut app, 'E');
    let target = app.mailbox.pending_sweep_preview.clone().unwrap();
    assert_eq!(target.sender_email, None);
    assert_eq!(target.account_id, None, "every account's Paper trail");

    app.finish_sweep(150, vec!["chunk-1".into(), "chunk-2".into()]);
    assert!(app.mailbox.pending_place_refresh.is_some());
    press(&mut app, KeyCode::Char('u'));
    let mut undone: Vec<String> = queued(&app)
        .into_iter()
        .filter_map(|request| match request {
            Request::UndoMutation { mutation_id } => Some(mutation_id),
            _ => None,
        })
        .collect();
    undone.sort();
    assert_eq!(undone, vec!["chunk-1".to_string(), "chunk-2".to_string()]);
}

#[test]
fn verbs_in_a_place_act_on_the_message_under_the_cursor_only() {
    let mut app = App::new();
    let unrelated = make_test_envelopes(1).remove(0);
    app.mailbox.viewing_envelope = Some(unrelated.clone());
    let receipt = message("Receipt", false);
    app.apply(Action::OpenPlace(MailPlaceData::PaperTrail));
    app.set_place(
        &refresh(MailPlaceData::PaperTrail),
        crate::app::PlacePageState {
            place: Some(MailPlaceData::PaperTrail),
            bundles: vec![bundle("receipts@shop.example", vec![receipt.clone()])],
            total_bundles: 1,
            total_messages: 1,
            loaded: true,
        },
    );
    app.apply(Action::Archive);
    assert!(matches!(
        queued(&app).as_slice(),
        [Request::SetModeDone { thread_ids, mode: mxr_protocol::ModeKindData::Updates, .. }]
            if thread_ids == &vec![receipt.thread_id.clone()]
    ));
}

pub(super) fn whole_place_preview() -> App {
    let mut app = paper_trail(vec![bundle(
        "receipts@shop.example",
        vec![message("Receipt", false)],
    )]);
    press_shifted(&mut app, 'E');
    let target = app
        .mailbox
        .pending_sweep_preview
        .take()
        .expect("E previews");
    let sender = |email: &str, count| mxr_protocol::SweepSenderData {
        account_id: mxr_core::AccountId::new(),
        sender_email: email.into(),
        sender_name: None,
        count,
    };
    app.show_sweep_preview(crate::app::PendingSweepConfirm {
        target,
        preview: SweepPreviewData {
            place: MailPlaceData::PaperTrail,
            sender_email: None,
            count: 143,
            pinned_excluded: 0,
            senders: vec![sender("a@shop.example", 100), sender("b@bank.example", 43)],
            sample_subjects: vec!["Receipt".into()],
            preview_token: Some("tok-all".into()),
        },
        shown: 0,
        sweep_focused: true,
        from_updates: false,
    });
    app
}

#[test]
fn e_then_enter_never_sweeps_the_whole_place() {
    let mut app = whole_place_preview();
    assert_eq!(
        app.mailbox.sweep_confirm.as_ref().map(|c| c.sweep_focused),
        Some(false),
        "the whole place opens on Cancel"
    );
    press(&mut app, KeyCode::Char('y'));
    assert!(
        app.mailbox.pending_sweep.is_none(),
        "y is not enough either"
    );
    press(&mut app, KeyCode::Enter);
    assert!(app.mailbox.pending_sweep.is_none());
    assert!(
        app.mailbox.sweep_confirm.is_none(),
        "Enter on Cancel closes it"
    );
}
