//! Editing and removing highlights: the preview names what the run
//! changes, undo puts it back, Marginalia's stay read-only, and a scoped
//! agent can't reach another account's.

use super::*;
use crate::handler::handle_request;
use mxr_protocol::ReadingHighlightChangeData;
use mxr_store::ReadingHighlightRow;

const QUOTE: &str = "A delete is the absence of a row";

async fn save(fx: &Fixture, item_key: &str, note: Option<&str>) -> String {
    let ResponseData::ReadingHighlight { highlight } = request(
        fx,
        Request::SaveHighlight {
            item_key: item_key.to_string(),
            quote: QUOTE.to_string(),
            note: note.map(str::to_string),
            view: None,
            paragraph: None,
        },
    )
    .await
    else {
        panic!("highlight");
    };
    highlight.id
}

async fn changed(response: ResponseData) -> ReadingHighlightChangeData {
    let ResponseData::ReadingHighlightChanged { change } = response else {
        panic!("expected a highlight change, got {response:?}");
    };
    change
}

async fn failure(fx: &Fixture, req: Request) -> String {
    let msg = IpcMessage {
        expected_workspace: None,
        id: 3,
        source: ClientKind::default(),
        payload: IpcPayload::Request(req),
    };
    match handle_request(&fx.state, &msg).await.payload {
        IpcPayload::Response(Response::Error { message, .. }) => message,
        other => panic!("expected an error, got {other:?}"),
    }
}

async fn stored(fx: &Fixture, id: &str) -> Option<ReadingHighlightRow> {
    fx.state.store.reading_highlight(id).await.expect("read")
}

async fn undo(fx: &Fixture, mutation_id: &str) -> Result<(), String> {
    let msg = IpcMessage {
        expected_workspace: None,
        id: 4,
        source: ClientKind::default(),
        payload: IpcPayload::Request(Request::UndoMutation {
            mutation_id: mutation_id.to_string(),
        }),
    };
    match handle_request(&fx.state, &msg).await.payload {
        IpcPayload::Response(Response::Ok { .. }) => Ok(()),
        IpcPayload::Response(Response::Error { message, .. }) => Err(message),
        other => panic!("unexpected {other:?}"),
    }
}

/// A highlight as Marginalia's read-back leaves it.
async fn marginalia_highlight(fx: &Fixture, message: &MessageId) -> String {
    let row = ReadingHighlightRow {
        id: "from-marginalia".into(),
        account_id: fx.account.clone(),
        message_id: message.clone(),
        idx: 0,
        view: "issue".into(),
        quote: "an absence does not travel".into(),
        note: None,
        created_at: Utc::now(),
        origin: "marginalia".into(),
        origin_id: Some("h1".into()),
        paragraph: None,
    };
    fx.state
        .store
        .insert_reading_highlight(&row)
        .await
        .expect("insert");
    row.id
}

#[tokio::test]
async fn a_removal_previews_what_it_removes_and_undo_brings_it_back() {
    let fx = Fixture::new().await;
    let (message, _) = issue(&fx, WEEKLY, "Tombstones", &essay(200), Utc::now()).await;
    let key = format!("{message}:0");
    let id = save(&fx, &key, Some("for the talk")).await;
    let before = stored(&fx, &id).await.expect("saved");
    let remove = |dry_run| Request::RemoveHighlight {
        highlight_id: id.clone(),
        dry_run,
    };

    let preview = changed(request(&fx, remove(true)).await).await;
    assert!(preview.dry_run && preview.mutation_id.is_none());
    assert_eq!(preview.highlight.id, id);
    assert_eq!(
        stored(&fx, &id).await,
        Some(before.clone()),
        "a preview changes nothing"
    );

    let done = changed(request(&fx, remove(false)).await).await;
    assert!(!done.dry_run);
    assert_eq!(
        (
            &done.highlight.id,
            &done.highlight.quote,
            &done.highlight.note
        ),
        (
            &preview.highlight.id,
            &preview.highlight.quote,
            &preview.highlight.note
        ),
        "the run removed exactly what the preview named"
    );
    assert_eq!(stored(&fx, &id).await, None);

    // Gone is gone: removing again says so instead of quietly succeeding.
    assert!(failure(&fx, remove(false)).await.contains("No highlight"));

    // Letting go of the item doesn't stop the undo.
    let thread = fx
        .state
        .store
        .get_envelope(&message)
        .await
        .expect("envelope")
        .expect("exists")
        .thread_id;
    request(
        &fx,
        Request::SetModeDone {
            thread_accounts: Vec::new(),
            thread_ids: vec![thread],
            mode: ModeKindData::Reading,
            dry_run: false,
            todo_ids: vec![],
            sender: None,
        },
    )
    .await;
    let mutation_id = done.mutation_id.expect("undoable");
    undo(&fx, &mutation_id).await.expect("undo");
    assert_eq!(
        stored(&fx, &id).await,
        Some(before),
        "id, note and time as they were"
    );
    assert!(
        undo(&fx, &mutation_id).await.is_err(),
        "an undo is spent once"
    );
}

#[tokio::test]
async fn a_note_edit_previews_clears_on_empty_and_undoes() {
    let fx = Fixture::new().await;
    let (message, _) = issue(&fx, WEEKLY, "Tombstones", &essay(200), Utc::now()).await;
    let id = save(&fx, &format!("{message}:0"), Some("first")).await;
    let edit = |note: &str, dry_run| Request::UpdateHighlight {
        highlight_id: id.clone(),
        note: note.to_string(),
        dry_run,
    };

    let preview = changed(request(&fx, edit("second", true)).await).await;
    assert_eq!(preview.highlight.note.as_deref(), Some("second"));
    assert_eq!(
        preview.previous_note, None,
        "a preview records no previous note"
    );
    assert_eq!(
        stored(&fx, &id).await.and_then(|row| row.note).as_deref(),
        Some("first")
    );

    let done = changed(request(&fx, edit("  second  ", false)).await).await;
    assert_eq!(done.highlight.note, preview.highlight.note);
    assert_eq!(done.previous_note.as_deref(), Some("first"));
    assert_eq!(
        stored(&fx, &id).await.and_then(|row| row.note).as_deref(),
        Some("second")
    );
    undo(&fx, &done.mutation_id.expect("undoable"))
        .await
        .expect("undo");
    assert_eq!(
        stored(&fx, &id).await.and_then(|row| row.note).as_deref(),
        Some("first")
    );

    // An empty note clears; it is never stored as "".
    let cleared = changed(request(&fx, edit("   ", false)).await).await;
    assert_eq!(cleared.highlight.note, None);
    assert_eq!(stored(&fx, &id).await.expect("kept").note, None);

    let long = "x".repeat(2001);
    assert!(failure(&fx, edit(&long, false))
        .await
        .contains("note keeps at most"));
}

#[tokio::test]
async fn a_highlight_made_in_marginalia_is_read_only_here() {
    let fx = Fixture::new().await;
    let (message, _) = issue(&fx, WEEKLY, "Tombstones", &essay(200), Utc::now()).await;
    let id = marginalia_highlight(&fx, &message).await;
    let before = stored(&fx, &id).await;
    for req in [
        Request::RemoveHighlight {
            highlight_id: id.clone(),
            dry_run: false,
        },
        Request::RemoveHighlight {
            highlight_id: id.clone(),
            dry_run: true,
        },
        Request::UpdateHighlight {
            highlight_id: id.clone(),
            note: "mine now".into(),
            dry_run: false,
        },
        Request::UpdateHighlight {
            highlight_id: id.clone(),
            note: "mine now".into(),
            dry_run: true,
        },
    ] {
        let message = failure(&fx, req).await;
        assert!(
            message.contains("made in Marginalia; change it there"),
            "{message}"
        );
    }
    assert_eq!(stored(&fx, &id).await, before);
    assert!(failure(
        &fx,
        Request::RemoveHighlight {
            highlight_id: "nope".into(),
            dry_run: false
        }
    )
    .await
    .contains("No highlight nope"));
}

#[tokio::test]
async fn a_scoped_agent_cannot_change_or_undo_another_accounts_highlight() {
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
    let (theirs, _) = issue_in(
        &fx,
        &other.id,
        ("Their Digest", "digest@theirs.example"),
        "Theirs",
        &essay(200),
        Utc::now(),
    )
    .await;
    let (mine, _) = issue(&fx, WEEKLY, "Mine", &essay(200), Utc::now()).await;
    let their_id = save(&fx, &format!("{theirs}:0"), None).await;
    let my_id = save(&fx, &format!("{mine}:0"), None).await;
    // Someone else removed theirs, leaving an undo the agent mustn't use.
    let their_undo = changed(
        request(
            &fx,
            Request::RemoveHighlight {
                highlight_id: their_id.clone(),
                dry_run: false,
            },
        )
        .await,
    )
    .await
    .mutation_id
    .expect("undoable");
    let their_id = save(&fx, &format!("{theirs}:0"), None).await;

    let mut config = fx.state.config_snapshot();
    config.agent_surfaces.profiles.insert(
        "mcp".into(),
        mxr_config::AgentProfileConfig {
            safety_policy: mxr_config::SafetyPolicy::Full,
            allowed_accounts: vec![fx.account.as_str()],
            allow_send: true,
            allow_destructive: true,
            allowed_destructive_actions: vec![],
        },
    );
    fx.state.set_config_for_test(config).await;

    for req in [
        Request::RemoveHighlight {
            highlight_id: their_id.clone(),
            dry_run: true,
        },
        Request::RemoveHighlight {
            highlight_id: their_id.clone(),
            dry_run: false,
        },
        Request::UpdateHighlight {
            highlight_id: their_id.clone(),
            note: "x".into(),
            dry_run: false,
        },
        Request::UndoMutation {
            mutation_id: their_undo,
        },
    ] {
        denied(as_scoped_agent(&fx, req).await);
    }
    assert!(stored(&fx, &their_id).await.is_some());

    let Response::Ok {
        data: ResponseData::ReadingHighlightChanged { change },
    } = as_scoped_agent(
        &fx,
        Request::RemoveHighlight {
            highlight_id: my_id.clone(),
            dry_run: false,
        },
    )
    .await
    else {
        panic!("the agent's own highlight is removable");
    };
    assert_eq!(stored(&fx, &my_id).await, None);
    assert!(change.mutation_id.is_some());
}

#[tokio::test]
async fn a_highlight_saved_with_a_paragraph_returns_and_keeps_it() {
    let fx = Fixture::new().await;
    let (message, _) = issue(&fx, WEEKLY, "Tombstones", &essay(200), Utc::now()).await;
    let key = format!("{message}:0");
    let save_in = |paragraph| Request::SaveHighlight {
        item_key: key.clone(),
        quote: QUOTE.to_string(),
        note: None,
        view: None,
        paragraph,
    };

    let ResponseData::ReadingHighlight { highlight } = request(&fx, save_in(Some(2))).await else {
        panic!("highlight");
    };
    assert_eq!(highlight.paragraph, Some(2));
    assert_eq!(
        stored(&fx, &highlight.id).await.expect("saved").paragraph,
        Some(2)
    );

    let ResponseData::ReadingHighlight { highlight } = request(&fx, save_in(None)).await else {
        panic!("highlight");
    };
    assert_eq!(
        highlight.paragraph, None,
        "no anchor keeps the first-match behaviour"
    );
    let wire = serde_json::to_value(&highlight).expect("json");
    assert!(
        wire.get("paragraph").is_none(),
        "an unanchored highlight omits the field"
    );
}
