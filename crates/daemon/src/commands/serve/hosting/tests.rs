use super::*;
fn fixture(data: &std::path::Path) -> Hosting {
    serde_json::from_value(serde_json::json!({"version":1,"executable":"/bin/mxr","config_dir":"/cfg","data_dir":data,"socket":"/sock","instance":"f","public_origin":"https://host.ts.net:8443","listen":"127.0.0.1:18741","workspace":{"workspace_id":"w","recovery_generation":"g"},"enabled":true,"daemon_enabled":true,"mapping_installed":false,"service_installed":false})).unwrap()
}
#[tokio::test]
async fn restored_or_changed_offline_owners_cannot_start_provider_work() {
    use sqlx::Connection;
    let dir = tempfile::tempdir().unwrap();
    let mut db = sqlx::SqliteConnection::connect_with(
        &sqlx::sqlite::SqliteConnectOptions::new()
            .filename(dir.path().join("mxr.db"))
            .create_if_missing(true),
    )
    .await
    .unwrap();
    sqlx::raw_sql("CREATE TABLE archive_workspace(singleton INTEGER,workspace_id TEXT,recovery_generation TEXT);INSERT INTO archive_workspace VALUES(1,'w','g');CREATE TABLE restore_recovery(pending INTEGER);").execute(&mut db).await.unwrap();
    let config = fixture(dir.path());
    runner::verify_offline_owner(&config).await.unwrap();
    sqlx::query("INSERT INTO restore_recovery VALUES(1)")
        .execute(&mut db)
        .await
        .unwrap();
    assert!(runner::verify_offline_owner(&config).await.is_err());
    sqlx::raw_sql(
        "DELETE FROM restore_recovery;UPDATE archive_workspace SET recovery_generation='restored';",
    )
    .execute(&mut db)
    .await
    .unwrap();
    assert!(runner::verify_offline_owner(&config).await.is_err());
    db.close().await.unwrap();
}
#[test]
fn user_services_use_exact_profile_paths_and_no_privileged_daemon() {
    let mut config = fixture(std::path::Path::new("/tmp/mail & drafts"));
    let mac = supervisor::render(&config, true).unwrap();
    assert!(mac.contains("/tmp/mail &amp; drafts"));
    assert!(!mac.contains("<key>UserName</key>"));
    let linux = supervisor::render(&config, false).unwrap();
    assert!(linux.contains("Environment=\"MXR_DATA_DIR=/tmp/mail & drafts\""));
    assert!(linux.contains("KillMode=process"));
    config.instance = "bad\nExecStart=/evil".into();
    assert!(supervisor::render(&config, false).is_err());
}
#[test]
fn a_parallel_drain_keeps_the_original_bounded_notice_window() {
    assert_eq!(
        drain_notice_remaining(Some(105), 100_250),
        Duration::from_millis(4750)
    );
    assert_eq!(
        drain_notice_remaining(Some(105), 104_750),
        Duration::from_millis(250)
    );
    assert_eq!(drain_notice_remaining(Some(105), 106_000), Duration::ZERO);
    assert_eq!(
        drain_notice_remaining(Some(999), 100_000),
        Duration::from_secs(5)
    );
}
#[test]
fn healthy_other_profile_cannot_satisfy_gateway_setup() {
    let config = fixture(std::path::Path::new("/data"));
    let mut health = serde_json::json!({"service":"mxr-remote-gateway","protocol_version":mxr_protocol::IPC_PROTOCOL_VERSION,"profile_id":mxr_web::remote_gateway::profile_id(std::path::Path::new("/another.sock"))});
    assert!(!health_matches(&health, &config));
    health["profile_id"] = serde_json::json!(mxr_web::remote_gateway::profile_id(&config.socket));
    assert!(health_matches(&health, &config));
}
#[test]
fn public_origin_cannot_carry_credentials_or_launch_parameters() {
    assert_eq!(
        normalize_origin("https://host.ts.net:8443/").unwrap(),
        "https://host.ts.net:8443"
    );
    for invalid in [
        "http://host.ts.net",
        "https://u:p@host.ts.net",
        "https://host.ts.net/path",
        "https://host.ts.net?code=x",
        "https://host.ts.net#secret",
    ] {
        assert!(normalize_origin(invalid).is_err(), "{invalid}");
    }
}
