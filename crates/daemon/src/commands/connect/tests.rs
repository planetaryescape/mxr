use super::*;

fn pending(marker: &str) -> PendingConnection {
    PendingConnection {
        capabilities: vec![RemoteCapability::MailRead],
        origin: "https://fixture.test".into(),
        name: "pending".into(),
        mail_secret: RemoteSecret::new("a".repeat(64)),
        cleanup_secret: RemoteSecret::new(marker.repeat(64)),
        request_id: Some("expired".into()),
    }
}
fn registration(id: &str) -> ConnectionRegistration {
    ConnectionRegistration {
        capabilities: vec![RemoteCapability::MailRead],
        origin: "https://old.test".into(),
        client_id: id.into(),
        host_id: "host".into(),
        workspace_id: "workspace".into(),
        recovery_generation: "epoch".into(),
        mail_secret: RemoteSecret::new("c".repeat(64)),
        cleanup_secret: RemoteSecret::new("d".repeat(64)),
    }
}

#[test]
fn cancelling_a_pending_pairing_preserves_selected_mail_and_retires_only_the_matching_proof() {
    let expected = pending("b");
    let mut state = ConnectionState {
        selected: Some(registration("selected")),
        generation: 7,
        pending_pairing: Some(expected.clone()),
        ..Default::default()
    };
    cancel_pending(&mut state, Some(&expected)).unwrap();
    assert!(state.pending_pairing.is_none());
    assert_eq!(state.selected.as_ref().unwrap().client_id, "selected");
    assert_eq!(state.generation, 7);
    assert_eq!(state.retired_pairings.len(), 1);
    assert_eq!(
        state.retired_pairings[0].cleanup_secret.expose(),
        expected.cleanup_secret.expose()
    );
    state.pending_pairing = Some(pending("e"));
    assert!(cancel_pending(&mut state, Some(&expected)).is_err());
    assert_eq!(
        state.pending_pairing.unwrap().cleanup_secret.expose(),
        "e".repeat(64)
    );
}

#[test]
fn forgetting_cleanup_removes_the_previewed_intents_without_losing_new_or_selected_authority() {
    let mut state = ConnectionState {
        selected: Some(registration("selected")),
        generation: 7,
        cleanup: vec![registration("retired").into()],
        retired_pairings: vec![pending("b").into()],
        ..Default::default()
    };
    state.retired_pairings[0].origin = "legacy-invalid-origin".into();
    state.retired_pairings[0].cleanup_secret = RemoteSecret::new("legacy-invalid-proof".into());
    let snapshot = state.clone();
    state.cleanup.push(registration("new-retired").into());
    state.retired_pairings.push(pending("e").into());
    forget_previewed(&mut state, &snapshot);
    assert_eq!(state.selected.unwrap().client_id, "selected");
    assert_eq!(state.generation, 7);
    assert_eq!(state.cleanup.len(), 1);
    assert_eq!(state.cleanup[0].client_id, "new-retired");
    assert_eq!(state.retired_pairings.len(), 1);
    assert_eq!(
        state.retired_pairings[0].cleanup_secret.expose(),
        "e".repeat(64)
    );
}

#[test]
fn a_late_host_response_cannot_replace_a_restarted_pending_pairing() {
    let old = pending("b");
    let state = ConnectionState {
        selected: Some(registration("selected")),
        pending_pairing: Some(pending("e")),
        generation: 7,
        ..Default::default()
    };
    assert!(ensure_pending_matches(&state, &old.cleanup_secret).is_err());
    assert_eq!(state.selected.unwrap().client_id, "selected");
    assert_eq!(
        state.pending_pairing.unwrap().cleanup_secret.expose(),
        "e".repeat(64)
    );
}
