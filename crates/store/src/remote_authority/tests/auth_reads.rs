use super::{secrets, start};
use std::time::Duration;

#[tokio::test]
async fn authentication_avoids_the_writer_and_observes_committed_revocation() {
    let directory = tempfile::tempdir().unwrap();
    let store = crate::Store::new(&directory.path().join("mxr.db"))
        .await
        .unwrap();
    let pairing = start(&store, 100).await;
    let (mail, cleanup, _, _) = secrets();
    store
        .approve_remote_pairing(&pairing.request_id, false, 101)
        .await
        .unwrap();
    let client = store
        .claim_remote_pairing(&pairing.request_id, &mail, &cleanup, 102)
        .await
        .unwrap();
    let (code, _) = store
        .issue_browser_code(&client.client_id, &mail, 103)
        .await
        .unwrap();
    let (browser, proof, _) = store.claim_browser_code(&code, 104).await.unwrap();

    // An older snapshot must not become the authority for a later admission.
    let mut old_reader = store.reader().begin().await.unwrap();
    let _: i64 = sqlx::query_scalar("SELECT count(*) FROM remote_clients")
        .fetch_one(&mut *old_reader)
        .await
        .unwrap();
    let mut writer = store.writer().begin().await.unwrap();
    sqlx::query("UPDATE remote_clients SET revoked_at=106 WHERE client_id=?")
        .bind(&client.client_id)
        .execute(&mut *writer)
        .await
        .unwrap();
    let (native, web) = tokio::join!(
        tokio::time::timeout(
            Duration::from_millis(500),
            store.authenticate_remote_client(&client.client_id, &mail, 105)
        ),
        tokio::time::timeout(
            Duration::from_millis(500),
            store.authenticate_remote_client(&browser, &proof, 105)
        ),
    );
    writer.commit().await.unwrap();
    assert!(
        native.is_ok(),
        "native authentication waited for the writer"
    );
    assert!(web.is_ok(), "browser authentication waited for the writer");
    assert_eq!(native.unwrap().unwrap().client_id, client.client_id);
    assert_eq!(web.unwrap().unwrap().client_id, client.client_id);
    let stale: Option<i64> =
        sqlx::query_scalar("SELECT revoked_at FROM remote_clients WHERE client_id=?")
            .bind(&client.client_id)
            .fetch_one(&mut *old_reader)
            .await
            .unwrap();
    assert!(stale.is_none(), "the held snapshot predates revocation");
    assert!(store
        .authenticate_remote_client(&client.client_id, &mail, 107)
        .await
        .is_err());
    assert!(store
        .authenticate_remote_client(&browser, &proof, 107)
        .await
        .is_err());
}

#[tokio::test]
async fn authentication_requires_current_owner_and_unpaused_recovery() {
    let store = crate::Store::in_memory().await.unwrap();
    let pairing = start(&store, 100).await;
    let (mail, cleanup, _, _) = secrets();
    store
        .approve_remote_pairing(&pairing.request_id, false, 101)
        .await
        .unwrap();
    let client = store
        .claim_remote_pairing(&pairing.request_id, &mail, &cleanup, 102)
        .await
        .unwrap();
    let (code, _) = store
        .issue_browser_code(&client.client_id, &mail, 103)
        .await
        .unwrap();
    let (browser, proof, _) = store.claim_browser_code(&code, 104).await.unwrap();
    let credentials = [(&client.client_id, &mail), (&browser, &proof)];
    sqlx::query("INSERT INTO restore_recovery(singleton,pending) VALUES (1,1)")
        .execute(store.writer())
        .await
        .unwrap();
    for (id, secret) in credentials {
        assert!(store
            .authenticate_remote_client(id, secret, 105)
            .await
            .is_err());
    }
    store.activate_recovery().await.unwrap();
    for (id, secret) in credentials {
        assert!(store
            .authenticate_remote_client(id, secret, 106)
            .await
            .is_ok());
    }
    store.rotate_archive_generation().await.unwrap();
    for (id, secret) in credentials {
        assert!(store
            .authenticate_remote_client(id, secret, 107)
            .await
            .is_err());
    }
}
