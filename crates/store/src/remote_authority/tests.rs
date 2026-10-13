use super::*;
fn secrets() -> (RemoteSecret, RemoteSecret, RemoteSecret, RemoteSecret) {
    let mail = RemoteSecret::new("a".repeat(64));
    let cleanup = RemoteSecret::new("b".repeat(64));
    let mv = RemoteSecret::new(verifier(&mail).unwrap());
    let cv = RemoteSecret::new(verifier(&cleanup).unwrap());
    (mail, cleanup, mv, cv)
}
async fn start(store: &crate::Store, now: i64) -> PendingPairing {
    let (_, _, mv, cv) = secrets();
    store
        .start_remote_pairing("fixture", &[RemoteCapability::MailRead], &mv, &cv, now)
        .await
        .unwrap()
}
#[tokio::test]
async fn preview_and_lists_do_not_initialize_or_approve() {
    let store = crate::Store::in_memory().await.unwrap();
    let before = store.remote_authority_status().await.unwrap();
    assert!(before.host_id.is_none());
    assert!(before.workspace_id.is_none());
    assert!(before.hosting_available);
    assert!(store.remote_pairings(100).await.unwrap().is_empty());
    assert!(store.remote_clients().await.unwrap().is_empty());
    assert!(store
        .approve_remote_pairing("missing", true, 100)
        .await
        .is_err());
    assert!(store
        .revoke_remote_client("missing", true, 100)
        .await
        .is_err());
    assert_eq!(before, store.remote_authority_status().await.unwrap());
    let p = start(&store, 100).await;
    assert!(
        !store
            .approve_remote_pairing(&p.request_id, true, 101)
            .await
            .unwrap()
            .approved
    );
    let (mail, cleanup, _, _) = secrets();
    assert!(store
        .claim_remote_pairing(&p.request_id, &mail, &cleanup, 101)
        .await
        .is_err());
}
#[tokio::test]
async fn claims_serialize_retry_and_cleanup_never_authorizes_mail() {
    let store = crate::Store::in_memory().await.unwrap();
    let p = start(&store, 100).await;
    let (mail, cleanup, _, _) = secrets();
    store
        .approve_remote_pairing(&p.request_id, false, 101)
        .await
        .unwrap();
    let (a, b) = tokio::join!(
        store.claim_remote_pairing(&p.request_id, &mail, &cleanup, 102),
        store.claim_remote_pairing(&p.request_id, &mail, &cleanup, 102)
    );
    let a = a.unwrap();
    assert_eq!(a, b.unwrap());
    assert_eq!(store.remote_clients().await.unwrap().len(), 1);
    assert!(store
        .authenticate_remote_client(&a.client_id, &cleanup, 103)
        .await
        .is_err());
    assert!(store
        .cleanup_remote_client(&a.client_id, &mail, 103)
        .await
        .is_err());
    assert!(store
        .revoke_remote_client(&a.client_id, true, 103)
        .await
        .unwrap()
        .revoked_at
        .is_none());
    assert!(store
        .authenticate_remote_client(&a.client_id, &mail, 103)
        .await
        .is_ok());
    store
        .cleanup_remote_client(&a.client_id, &cleanup, 104)
        .await
        .unwrap();
    store
        .cleanup_remote_client(&a.client_id, &cleanup, 105)
        .await
        .unwrap();
    assert!(store
        .authenticate_remote_client(&a.client_id, &mail, 105)
        .await
        .is_err());
    assert!(store
        .claim_remote_pairing(&p.request_id, &mail, &cleanup, 105)
        .await
        .is_err());
    assert!(store
        .cleanup_remote_client("another-client", &cleanup, 105)
        .await
        .is_err());
}
#[tokio::test]
async fn limits_expiry_and_invalid_proof_are_enforced() {
    let store = crate::Store::in_memory().await.unwrap();
    let p = start(&store, 100).await;
    let (mail, cleanup, mv, cv) = secrets();
    for name in ["", "bad\nname"] {
        assert!(store
            .start_remote_pairing(name, &[RemoteCapability::MailRead], &mv, &cv, 100)
            .await
            .is_err());
    }
    assert!(store
        .start_remote_pairing(
            &"x".repeat(81),
            &[RemoteCapability::MailRead],
            &mv,
            &cv,
            100
        )
        .await
        .is_err());
    assert!(store
        .start_remote_pairing(
            "fixture",
            &[RemoteCapability::MailRead, RemoteCapability::MailRead],
            &mv,
            &cv,
            100
        )
        .await
        .is_err());
    assert!(store
        .start_remote_pairing("fixture", &[], &mv, &cv, 100)
        .await
        .is_err());
    assert!(store
        .start_remote_pairing("fixture", &[RemoteCapability::MailRead], &mv, &mv, 100)
        .await
        .is_err());
    store
        .approve_remote_pairing(&p.request_id, false, 100)
        .await
        .unwrap();
    let wrong = RemoteSecret::new("c".repeat(64));
    assert!(store
        .claim_remote_pairing(
            &p.request_id,
            &RemoteSecret::new("malformed".into()),
            &cleanup,
            101
        )
        .await
        .is_err());
    for _ in 1..MAX_CLAIM_FAILURES {
        assert!(store
            .claim_remote_pairing(&p.request_id, &wrong, &cleanup, 101)
            .await
            .is_err());
    }
    assert!(store
        .claim_remote_pairing(&p.request_id, &mail, &cleanup, 101)
        .await
        .is_err());
    assert!(store.remote_pairings(101).await.unwrap().is_empty());
    for index in 0..MAX_PENDING_PAIRINGS {
        let name = format!("fixture-{index}");
        let mail = RemoteSecret::new(format!("{index:064x}"));
        let cleanup = RemoteSecret::new(format!("{:064x}", index + MAX_PENDING_PAIRINGS));
        store
            .start_remote_pairing(
                &name,
                &[RemoteCapability::MailRead],
                &RemoteSecret::new(verifier(&mail).unwrap()),
                &RemoteSecret::new(verifier(&cleanup).unwrap()),
                101,
            )
            .await
            .unwrap();
    }
    assert!(store
        .start_remote_pairing("overflow", &[RemoteCapability::MailRead], &mv, &cv, 101)
        .await
        .is_err());
    assert!(store
        .approve_remote_pairing(&p.request_id, false, 100 + PAIRING_TTL_SECONDS)
        .await
        .is_err());
    assert!(store
        .remote_pairings(102 + PAIRING_TTL_SECONDS)
        .await
        .unwrap()
        .is_empty());
    start(&store, 102 + PAIRING_TTL_SECONDS).await;
}
#[tokio::test]
async fn concurrent_approval_claim_and_revoke_have_one_durable_order() {
    let store = crate::Store::in_memory().await.unwrap();
    let p = start(&store, 100).await;
    let (mail, cleanup, _, _) = secrets();
    let (approved, claimed) = tokio::join!(
        store.approve_remote_pairing(&p.request_id, false, 101),
        store.claim_remote_pairing(&p.request_id, &mail, &cleanup, 101)
    );
    assert!(approved.is_ok());
    let c = match claimed {
        Ok(c) => c,
        Err(_) => store
            .claim_remote_pairing(&p.request_id, &mail, &cleanup, 102)
            .await
            .unwrap(),
    };
    let (_, revoked) = tokio::join!(
        store.claim_remote_pairing(&p.request_id, &mail, &cleanup, 103),
        store.revoke_remote_client(&c.client_id, false, 103)
    );
    assert!(revoked.unwrap().revoked_at.is_some());
    assert!(store
        .claim_remote_pairing(&p.request_id, &mail, &cleanup, 104)
        .await
        .is_err());
    assert!(store
        .authenticate_remote_client(&c.client_id, &mail, 104)
        .await
        .is_err());
}
#[tokio::test]
async fn restart_preserves_authority_but_recovery_generation_invalidates_it() {
    let home = tempfile::tempdir().unwrap();
    let path = home.path().join("mxr.db");
    let store = crate::Store::new(&path).await.unwrap();
    let p = start(&store, 100).await;
    let (mail, cleanup, _, _) = secrets();
    store
        .approve_remote_pairing(&p.request_id, false, 101)
        .await
        .unwrap();
    let c = store
        .claim_remote_pairing(&p.request_id, &mail, &cleanup, 102)
        .await
        .unwrap();
    let identity = store.remote_authority_status().await.unwrap();
    store.reader().close().await;
    store.writer().close().await;
    drop(store);
    let store = crate::Store::new(&path).await.unwrap();
    assert_eq!(identity, store.remote_authority_status().await.unwrap());
    assert!(store
        .authenticate_remote_client(&c.client_id, &mail, 103)
        .await
        .is_ok());
    assert!(store
        .authenticate_remote_client(&c.client_id, &mail, c.expires_at)
        .await
        .is_err());
    // Cleanup retries retain authority after ordinary credentials expire.
    store
        .cleanup_remote_client(&c.client_id, &cleanup, c.expires_at + 1)
        .await
        .unwrap();
    store.rotate_archive_generation().await.unwrap();
    assert!(store
        .remote_authority_status()
        .await
        .unwrap()
        .host_id
        .is_none());
    assert!(store
        .cleanup_remote_client(&c.client_id, &cleanup, c.expires_at + 2)
        .await
        .is_err());
    let p2 = start(&store, 200).await;
    assert_ne!(p.request_id, p2.request_id);
    assert_ne!(
        identity.host_id,
        store.remote_authority_status().await.unwrap().host_id
    );
}

#[tokio::test]
async fn migration_replay_preserves_grant_when_a_table_and_stamp_are_missing() {
    let home = tempfile::tempdir().unwrap();
    let path = home.path().join("mxr.db");
    let store = crate::Store::new(&path).await.unwrap();
    let p = start(&store, 100).await;
    let (mail, cleanup, _, _) = secrets();
    store
        .approve_remote_pairing(&p.request_id, false, 101)
        .await
        .unwrap();
    let c = store
        .claim_remote_pairing(&p.request_id, &mail, &cleanup, 102)
        .await
        .unwrap();
    let identity = store.remote_authority_status().await.unwrap();
    // Stamp-last adoption must reconcile partial DDL without deleting durable authority.
    sqlx::raw_sql("DROP TABLE remote_pairings; DELETE FROM schema_migrations WHERE version=83;")
        .execute(store.writer())
        .await
        .unwrap();
    store.reader().close().await;
    store.writer().close().await;
    drop(store);
    let reopened = crate::Store::new(&path).await.unwrap();
    assert_eq!(identity, reopened.remote_authority_status().await.unwrap());
    assert_eq!(
        c,
        reopened
            .authenticate_remote_client(&c.client_id, &mail, 103)
            .await
            .unwrap()
    );
    assert!(reopened.remote_pairings(103).await.unwrap().is_empty());
    let stamped: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM schema_migrations WHERE version=83)")
            .fetch_one(reopened.reader())
            .await
            .unwrap();
    assert!(stamped);
    reopened
        .cleanup_remote_client(&c.client_id, &cleanup, 104)
        .await
        .unwrap();
}

#[tokio::test]
async fn lost_start_ack_reuses_pairing_and_retirement_prevents_stale_start() {
    let store = crate::Store::in_memory().await.unwrap();
    let pending = start(&store, 100).await;
    assert_eq!(pending, start(&store, 101).await);
    let (mail, cleanup, mv, cv) = secrets();
    assert!(store.abandon_remote_pairing(&mail, 101).await.is_err());
    store.abandon_remote_pairing(&cleanup, 102).await.unwrap();
    store.abandon_remote_pairing(&cleanup, 103).await.unwrap();
    assert!(store
        .approve_remote_pairing(&pending.request_id, false, 103)
        .await
        .is_err());
    assert!(store
        .start_remote_pairing("fixture", &[RemoteCapability::MailRead], &mv, &cv, 104)
        .await
        .is_err());
    assert!(store.remote_clients().await.unwrap().is_empty());
}

#[tokio::test]
async fn unknown_cleanup_cannot_ack_a_start_that_has_not_arrived() {
    let store = crate::Store::in_memory().await.unwrap();
    start(&store, 100).await;
    assert!(store
        .abandon_remote_pairing(&RemoteSecret::new("c".repeat(64)), 102)
        .await
        .is_err());
    assert_eq!(store.remote_pairings(102).await.unwrap().len(), 1);
}

#[tokio::test]
async fn expired_start_can_be_reestablished_unapproved_then_retired() {
    let store = crate::Store::in_memory().await.unwrap();
    let original = start(&store, 100).await;
    let (_, cleanup, mail_verifier, cleanup_verifier) = secrets();
    let other_mail = RemoteSecret::new("c".repeat(64));
    let other_cleanup = RemoteSecret::new("d".repeat(64));
    store
        .start_remote_pairing(
            "other-client",
            &[RemoteCapability::MailRead],
            &RemoteSecret::new(verifier(&other_mail).unwrap()),
            &RemoteSecret::new(verifier(&other_cleanup).unwrap()),
            original.expires_at + 1,
        )
        .await
        .unwrap();

    // Expiry removes the pending proof row, so cleanup must not acknowledge yet.
    assert!(store
        .abandon_remote_pairing(&cleanup, original.expires_at + 1)
        .await
        .is_err());
    assert!(!store
        .remote_pairings(original.expires_at + 1)
        .await
        .unwrap()
        .iter()
        .any(|pairing| pairing.request_id == original.request_id));

    // The client can recreate its pending proof from retained cleanup intent.
    let recreated = store
        .start_remote_pairing(
            "fixture",
            &[RemoteCapability::MailRead],
            &mail_verifier,
            &cleanup_verifier,
            original.expires_at + 1,
        )
        .await
        .unwrap();
    assert!(!recreated.approved);
    store
        .abandon_remote_pairing(&cleanup, original.expires_at + 2)
        .await
        .unwrap();
    assert!(store
        .approve_remote_pairing(&recreated.request_id, false, original.expires_at + 3)
        .await
        .is_err());
    assert!(store
        .start_remote_pairing(
            "fixture",
            &[RemoteCapability::MailRead],
            &mail_verifier,
            &cleanup_verifier,
            original.expires_at + 4,
        )
        .await
        .is_err());
}

#[tokio::test]
async fn claimed_pairing_cleanup_is_exact_and_revocation_blocks_later_admission() {
    let store = crate::Store::in_memory().await.unwrap();
    let pending = start(&store, 100).await;
    let (mail, cleanup, _, _) = secrets();
    store
        .approve_remote_pairing(&pending.request_id, false, 101)
        .await
        .unwrap();
    let client = store
        .claim_remote_pairing(&pending.request_id, &mail, &cleanup, 102)
        .await
        .unwrap();
    store
        .authenticate_remote_client(&client.client_id, &mail, 103)
        .await
        .unwrap();
    store.abandon_remote_pairing(&cleanup, 104).await.unwrap();
    assert!(store
        .authenticate_remote_client(&client.client_id, &mail, 105)
        .await
        .is_err());
    assert!(store
        .claim_remote_pairing(&pending.request_id, &mail, &cleanup, 106)
        .await
        .is_err());
}

#[tokio::test]
async fn authority_writer_has_effective_full_durability_and_rerunnable_retirement_schema() {
    let directory = tempfile::tempdir().unwrap();
    let store = crate::Store::new(&directory.path().join("mxr.db"))
        .await
        .unwrap();
    let synchronous: i64 = sqlx::query_scalar("PRAGMA synchronous")
        .fetch_one(store.writer())
        .await
        .unwrap();
    let fullfsync: i64 = sqlx::query_scalar("PRAGMA fullfsync")
        .fetch_one(store.writer())
        .await
        .unwrap();
    assert_eq!(synchronous, 2);
    assert_eq!(fullfsync, 1);
    let started = std::time::Instant::now();
    let pending = start(&store, 100).await;
    let (_, cleanup, _, _) = secrets();
    store.abandon_remote_pairing(&cleanup, 101).await.unwrap();
    sqlx::query("DELETE FROM schema_migrations WHERE version=84")
        .execute(store.writer())
        .await
        .unwrap();
    store.run_migrations().await.unwrap();
    assert!(store
        .approve_remote_pairing(&pending.request_id, false, 102)
        .await
        .is_err());
    assert!(start_result(&store, 103).await.is_err());
    eprintln!(
        "FULL writer pairing/retirement/adoption elapsed {:?}",
        started.elapsed()
    );
}
async fn start_result(store: &crate::Store, now: i64) -> Result<PendingPairing, sqlx::Error> {
    let (_, _, mv, cv) = secrets();
    store
        .start_remote_pairing("fixture", &[RemoteCapability::MailRead], &mv, &cv, now)
        .await
}
