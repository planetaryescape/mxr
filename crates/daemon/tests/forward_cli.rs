//! Forwarding regressions against an isolated fake account. No real mail or SMTP.
use assert_cmd::prelude::*;
use mxr_core::{AttachmentDisposition, AttachmentId, AttachmentMeta};
use mxr_test_support::daemon::{
    daemon_lock, run_json, run_json_with_stdin, run_status_only, spawn_fake_daemon,
};
use serde_json::Value;
use std::process::Command;

#[test]
fn forward_cli_preview_editor_draft_inline_body_and_fake_send_include_original_and_five_pdfs() {
    let _guard = daemon_lock();
    let temp = tempfile::tempdir().expect("isolated forwarding profile");
    let (_daemon, instance, data_dir, config_dir) = spawn_fake_daemon(&temp, "forward-cli");
    run_status_only(&instance, &data_dir, &config_dir, &["sync", "--wait"]);
    let results = run_json(
        &instance,
        &data_dir,
        &config_dir,
        &["search", "deployment", "--format", "json"],
    );
    let rows = results
        .as_array()
        .or_else(|| results["results"].as_array())
        .expect("search results array");
    let id: mxr_core::MessageId = rows[0]["message_id"]
        .as_str()
        .expect("synthetic message id")
        .parse()
        .expect("valid synthetic message id");
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("fixture runtime");
    runtime.block_on(async {
        let store = mxr_store::Store::new(&data_dir.join("mxr.db"))
            .await
            .expect("isolated profile store");
        let mut body = store
            .get_body(&id)
            .await
            .expect("load synthetic body")
            .expect("synthetic body exists");
        body.text_plain = Some("Original reports.\nLiteral **asterisks** and {today}.\n".into());
        body.text_html = None;
        body.attachments = (0..5)
            .map(|index| AttachmentMeta {
                id: AttachmentId::new(),
                message_id: id.clone(),
                filename: "report.pdf".into(),
                mime_type: "application/pdf".into(),
                disposition: AttachmentDisposition::Attachment,
                content_id: None,
                content_location: None,
                size_bytes: 23,
                local_path: None,
                provider_id: format!("pdf-{index}"),
            })
            .collect();
        store
            .insert_body(&body)
            .await
            .expect("seed synthetic attachments");
    });
    let id = id.to_string();
    let cat = run_json(
        &instance,
        &data_dir,
        &config_dir,
        &["cat", &id, "--format", "json"],
    );
    assert_eq!(
        cat["attachments"]
            .as_array()
            .expect("original attachments")
            .len(),
        5
    );

    let preview = run_json(
        &instance,
        &data_dir,
        &config_dir,
        &[
            "forward",
            &id,
            "--to",
            "synthetic@example.com",
            "--yes",
            "--dry-run",
            "--no-signature",
            "--format",
            "json",
        ],
    );
    let preview = &preview["draft"];
    assert!(preview["body_text"]
        .as_str()
        .expect("preview text alternative")
        .contains("Original reports."));
    assert!(preview["attachments"]
        .as_array()
        .expect("preview path attachments")
        .is_empty());
    assert_eq!(
        preview["managed_attachments"]
            .as_array()
            .expect("preview managed attachments")
            .len(),
        5
    );
    assert!(run_json(
        &instance,
        &data_dir,
        &config_dir,
        &["drafts", "--format", "json"]
    )
    .as_array()
    .expect("draft listing")
    .is_empty());

    let extra = temp.path().join("extra.txt");
    std::fs::write(&extra, "Additional file").expect("additional attachment fixture");
    let with_extra = run_json(
        &instance,
        &data_dir,
        &config_dir,
        &[
            "forward",
            &id,
            "--to",
            "synthetic@example.com",
            "--attach",
            extra.to_str().expect("UTF-8 fixture path"),
            "--yes",
            "--dry-run",
            "--no-signature",
            "--format",
            "json",
        ],
    );
    assert_eq!(with_extra["draft"]["body_text"], preview["body_text"]);
    assert_eq!(
        with_extra["draft"]["managed_attachments"]
            .as_array()
            .expect("original managed attachments with extra file")
            .len(),
        5
    );
    assert!(
        with_extra["pending_attachments"].is_array(),
        "Missing pending attachment declarations: {with_extra}"
    );
    assert_eq!(
        with_extra["pending_attachments"]
            .as_array()
            .expect("pending attachment declarations")
            .len(),
        1
    );

    let output = Command::cargo_bin("mxr")
        .expect("test mxr binary")
        .env("MXR_INSTANCE", &instance)
        .env("MXR_DATA_DIR", &data_dir)
        .env("MXR_CONFIG_DIR", &config_dir)
        .env("EDITOR", "true")
        .env_remove("VISUAL")
        .env_remove("MXR_DAEMON_ADDR")
        .args([
            "forward",
            &id,
            "--to",
            "synthetic@example.com",
            "--draft",
            "--no-signature",
            "--format",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let saved: Value = serde_json::from_slice(&output).expect("saved draft JSON");
    assert_eq!(saved["attachment_count"], 5);
    let drafts = run_json(
        &instance,
        &data_dir,
        &config_dir,
        &["drafts", "--format", "json"],
    );
    let stored = drafts
        .as_array()
        .expect("saved draft listing")
        .iter()
        .find(|draft| draft["id"] == saved["draft_id"])
        .expect("saved forward remains listed");
    assert_eq!(stored["body_text"], preview["body_text"]);
    assert_eq!(stored["body_html"], preview["body_html"]);
    assert_eq!(
        stored["managed_attachments"]
            .as_array()
            .expect("stored managed attachments")
            .len(),
        5
    );

    let intro = run_json(
        &instance,
        &data_dir,
        &config_dir,
        &[
            "forward",
            &id,
            "--to",
            "synthetic@example.com",
            "--body",
            "Please review.",
            "--yes",
            "--dry-run",
            "--no-signature",
            "--format",
            "json",
        ],
    );
    let text = intro["draft"]["body_text"]
        .as_str()
        .expect("introduced text alternative");
    assert!(text.starts_with("Please review.\n\n---------- Forwarded message ----------"));
    assert!(text.contains("Original reports.") && text.contains("{today}"));
    for header in ["From:", "Date:", "Subject:", "To:"] {
        assert!(text.contains(header));
    }
    let stdin_preview = run_json_with_stdin(
        &instance,
        &data_dir,
        &config_dir,
        &[
            "forward",
            &id,
            "--to",
            "synthetic@example.com",
            "--body-stdin",
            "--yes",
            "--dry-run",
            "--no-signature",
            "--format",
            "json",
        ],
        "From stdin.",
    );
    assert!(stdin_preview["draft"]["body_text"]
        .as_str()
        .expect("stdin text alternative")
        .starts_with("From stdin.\n\n---------- Forwarded message ----------"));
    assert_eq!(
        stdin_preview["draft"]["managed_attachments"]
            .as_array()
            .expect("stdin managed attachments")
            .len(),
        5
    );

    // Actual submission goes only to the fake provider, using the reviewed stored draft.
    let draft_id = saved["draft_id"].as_str().expect("saved draft id");
    let review = run_json(
        &instance,
        &data_dir,
        &config_dir,
        &["send", draft_id, "--dry-run", "--format", "json"],
    );
    assert_eq!(review["draft"]["body_text"], stored["body_text"]);
    assert_eq!(
        review["draft"]["managed_attachments"],
        stored["managed_attachments"]
    );
    let sent = run_json(
        &instance,
        &data_dir,
        &config_dir,
        &[
            "send",
            draft_id,
            "--expected-revision",
            &review["send_identity"]["expected_revision"].to_string(),
            "--operation-id",
            review["send_identity"]["operation_id"]
                .as_str()
                .expect("reviewed operation id"),
            "--expected-workspace",
            &review["expected_workspace"].to_string(),
            "--format",
            "json",
        ],
    );
    assert_eq!(sent["state"], "accepted");
    let sent_id = sent["local_message_id"]
        .as_str()
        .expect("local sent message id");
    let sent_copy = run_json(
        &instance,
        &data_dir,
        &config_dir,
        &["cat", sent_id, "--format", "json"],
    );
    assert_eq!(
        sent_copy["attachments"]
            .as_array()
            .expect("sent attachments")
            .len(),
        5
    );
}
