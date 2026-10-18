//! `mxr modes` against a real daemon: the copy each mode teaches itself
//! with, as JSON, and a hint's seen state surviving between commands.

#![expect(
    clippy::unwrap_used,
    reason = "integration tests unwrap to keep fixture failures direct"
)]

use mxr_test_support::daemon::{daemon_lock, run_json, spawn_fake_daemon};
use tempfile::TempDir;

#[test]
fn cached_headerless_document_has_one_purpose_across_cli_json_surfaces() {
    let _guard = daemon_lock();
    let temp = TempDir::new().expect("isolated classification profile");
    let (_daemon, instance, data_dir, config_dir) = spawn_fake_daemon(&temp, "classification-json");
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let (message_id, thread_id) = runtime.block_on(async {
        let store = mxr_store::Store::new(&data_dir.join("mxr.db")).await.unwrap();
        let account = store.list_accounts().await.unwrap().into_iter()
            .find(|account| account.email == "fake@example.com").unwrap();
        let inbox = mxr_core::LabelId::from_scoped_provider_id(&account.id, "fake", "INBOX");
        store.upsert_label(&mxr_core::Label {
            id: inbox.clone(), account_id: account.id.clone(), name: "Inbox".into(),
            kind: mxr_core::LabelKind::System, color: None, provider_id: "INBOX".into(),
            unread_count: 0, total_count: 0, role: None,
        }).await.unwrap();
        let id = mxr_core::MessageId::new();
        let thread = mxr_core::ThreadId::new();
        store.upsert_envelope_with_direction(&mxr_core::Envelope {
            id: id.clone(), account_id: account.id, provider_id: format!("classification-{id}"),
            thread_id: thread.clone(), message_id_header: Some(format!("<{id}@example.org>")),
            in_reply_to: None, references: vec![],
            from: mxr_core::Address { name: Some("Vendor Accounts".into()), email: "emea_invoicing@email.vendor.example".into() },
            to: vec![mxr_core::Address { name: Some("Test User".into()), email: "fake@example.com".into() }],
            cc: vec![], bcc: vec![], subject: "Your invoice Number AB12345".into(),
            date: chrono::Utc::now() - chrono::Duration::minutes(1), flags: mxr_core::MessageFlags::empty(),
            snippet: "A billing document is available.".into(), has_attachments: false, size_bytes: 256,
            unsubscribe: mxr_core::UnsubscribeMethod::None, link_count: 0, body_word_count: 30,
            label_provider_ids: vec![], keywords: Default::default(),
        }, mxr_core::MessageDirection::Inbound).await.unwrap();
        store.set_message_labels(&id, &[inbox], mxr_core::EventSource::User).await.unwrap();
        store.insert_body(&mxr_core::MessageBody {
            message_id: id.clone(), text_plain: Some("Invoice number: AB12345\nInvoice date: 2026-10-18\nOrder number: PO23456\nThank you for your purchase. Your billing document is attached.".into()),
            text_html: None, attachments: vec![], fetched_at: chrono::Utc::now(), metadata: Default::default(),
        }).await.unwrap();
        (id.to_string(), thread.to_string())
    });
    let why = run_json(
        &instance,
        &data_dir,
        &config_dir,
        &["why", &message_id, "--format", "json"],
    );
    assert_eq!(why["mail_kind"]["kind"], "paper_trail", "{why}");
    assert!(why["mail_kind"]["reason"]
        .as_str()
        .unwrap()
        .contains("structured billing document"));
    let modes = run_json(
        &instance,
        &data_dir,
        &config_dir,
        &["modes", "why", &message_id, "--format", "json"],
    );
    assert!(modes[0]["modes"]
        .as_array()
        .unwrap()
        .iter()
        .any(|mode| mode["mode"] == "updates"));
    let arrivals = run_json(
        &instance,
        &data_dir,
        &config_dir,
        &[
            "arrivals", "list", "--mode", "updates", "--limit", "1000", "--format", "json",
        ],
    );
    let arrived = arrivals["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["message_id"] == message_id)
        .expect("the document is listed in Updates arrivals");
    assert_eq!(arrived["bucket"], "updates");
    assert!(arrived["reason"]
        .as_str()
        .unwrap()
        .contains("structured billing document"));
    let messages = run_json(
        &instance,
        &data_dir,
        &config_dir,
        &["messages", "--format", "json"],
    );
    for band in ["your_turn", "pinned", "recent", "quiet"] {
        assert!(
            !messages[band]
                .as_array()
                .unwrap()
                .iter()
                .flat_map(|row| row["topics"].as_array().unwrap())
                .any(|topic| topic["thread_id"] == thread_id),
            "the document must not merely move to Quiet"
        );
    }
    let now = run_json(
        &instance,
        &data_dir,
        &config_dir,
        &["now", "--format", "json"],
    );
    assert!(now["people"]["rows"].is_array());
    assert!(!serde_json::to_string(&now["people"])
        .unwrap()
        .contains(&thread_id));
}

#[test]
fn modes_explain_prints_the_to_do_guide_and_hint_state_holds() {
    let _guard = daemon_lock();
    let temp = TempDir::new().expect("temp dir");
    let (_daemon, instance, data_dir, config_dir) = spawn_fake_daemon(&temp, "modes-explain");

    let guides = run_json(
        &instance,
        &data_dir,
        &config_dir,
        &["modes", "explain", "todo", "--format", "json"],
    );
    insta::assert_snapshot!(
        "modes_explain_todo",
        serde_json::to_string_pretty(&guides).unwrap()
    );
    let updates = run_json(
        &instance,
        &data_dir,
        &config_dir,
        &["modes", "explain", "updates", "--format", "json"],
    );
    insta::assert_snapshot!(
        "modes_explain_updates",
        serde_json::to_string_pretty(&updates).unwrap()
    );

    let closed = run_json(
        &instance,
        &data_dir,
        &config_dir,
        &["modes", "hint", "todo.runway", "--format", "json"],
    );
    assert_eq!(closed[0]["hints"][0]["id"], "todo.runway", "{closed}");
    assert_eq!(closed[0]["hints"][0]["seen"], true, "{closed}");
    let again = run_json(
        &instance,
        &data_dir,
        &config_dir,
        &["modes", "explain", "--format", "json"],
    );
    let modes: Vec<&str> = again
        .as_array()
        .unwrap()
        .iter()
        .map(|guide| guide["mode"].as_str().unwrap())
        .collect();
    assert_eq!(
        modes,
        ["now", "messages", "todo", "updates", "reading", "archive"],
        "every shipped mode, Now first"
    );
    assert_eq!(
        again[2]["hints"][0]["seen"], true,
        "a new command sees it dismissed"
    );
    assert_eq!(
        again[2]["hints"][1]["seen"], false,
        "dismissing one hint leaves the others"
    );

    let shown = run_json(
        &instance,
        &data_dir,
        &config_dir,
        &["modes", "hint", "todo.runway", "--show", "--format", "json"],
    );
    assert_eq!(shown[0]["hints"][0]["seen"], false, "{shown}");
}

#[test]
fn now_and_the_rail_print_as_json_within_the_caps() {
    let _guard = daemon_lock();
    let temp = TempDir::new().expect("temp dir");
    let (_daemon, instance, data_dir, config_dir) = spawn_fake_daemon(&temp, "modes-now");

    let now = run_json(
        &instance,
        &data_dir,
        &config_dir,
        &["now", "--format", "json"],
    );
    assert_eq!(
        now["header"],
        "The few things that need you now, from every mode."
    );
    assert!(now["item_count"].as_u64().unwrap() <= 10, "{now}");
    assert!(now["people"]["rows"].as_array().unwrap().len() <= 3);
    assert!(now["due_soon"]["todos"].as_array().unwrap().len() <= 3);

    let rail = run_json(
        &instance,
        &data_dir,
        &config_dir,
        &["modes", "rail", "--format", "json"],
    );
    let keys: Vec<&str> = rail["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| entry["key"].as_str().unwrap())
        .collect();
    assert_eq!(keys, ["g h", "g m", "g x", "g u", "g r", "g e", "g i"]);
}

#[test]
fn messages_print_as_json_and_got_it_previews_without_sending() {
    let _guard = daemon_lock();
    let temp = TempDir::new().expect("temp dir");
    let (_daemon, instance, data_dir, config_dir) = spawn_fake_daemon(&temp, "modes-messages");

    let guide = run_json(
        &instance,
        &data_dir,
        &config_dir,
        &["modes", "explain", "messages", "--format", "json"],
    );
    insta::assert_snapshot!(
        "modes_explain_messages",
        serde_json::to_string_pretty(&guide).unwrap()
    );

    let messages = run_json(
        &instance,
        &data_dir,
        &config_dir,
        &["messages", "--format", "json"],
    );
    assert_eq!(
        messages["header"],
        "People you talk with, one row each. Reply or mark done."
    );
    for band in ["your_turn", "pinned", "recent", "quiet"] {
        assert!(messages[band].is_array(), "{band}: {messages}");
    }
    let Some(row) = messages["your_turn"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["kind"] == "person")
    else {
        return;
    };
    let page = run_json(
        &instance,
        &data_dir,
        &config_dir,
        &[
            "messages",
            "person",
            row["id"].as_str().unwrap(),
            "--format",
            "json",
        ],
    );
    let conversation = &page["conversation"];
    assert!(conversation["messages"].as_array().is_some(), "{page}");
    let thread = conversation["thread_id"].as_str().unwrap();
    let ack = run_json(
        &instance,
        &data_dir,
        &config_dir,
        &["messages", "ack", thread, "--dry-run", "--format", "json"],
    );
    assert_eq!(ack["dry_run"], true, "{ack}");
    assert!(ack["sent_message_id"].is_null(), "a dry run sends nothing");
    assert!(!ack["text"].as_str().unwrap().is_empty());
}

#[test]
fn reading_prints_its_guide_its_edition_and_its_highlights_as_json() {
    let _guard = daemon_lock();
    let temp = TempDir::new().expect("temp dir");
    let (_daemon, instance, data_dir, config_dir) = spawn_fake_daemon(&temp, "modes-reading");

    let guides = run_json(
        &instance,
        &data_dir,
        &config_dir,
        &["modes", "explain", "reading", "--format", "json"],
    );
    insta::assert_snapshot!(
        "modes_explain_reading",
        serde_json::to_string_pretty(&guides).unwrap()
    );

    let edition = run_json(
        &instance,
        &data_dir,
        &config_dir,
        &["reading", "edition", "--peek", "--format", "json"],
    );
    assert_eq!(
        edition["header"],
        "Newsletters you chose, as an edition. Read when you like."
    );
    assert!(edition["later_count"].is_u64(), "{edition}");
    assert!(edition["bands"].is_array());
    for band in edition["bands"].as_array().unwrap() {
        for item in band["items"].as_array().unwrap() {
            assert!(item["shape"].is_string(), "{item}");
            assert!(item["minutes"].as_u64().unwrap() >= 1, "{item}");
            assert!(item.get("unread").is_none(), "no unread counts in Reading");
        }
    }

    let highlights = run_json(
        &instance,
        &data_dir,
        &config_dir,
        &["reading", "export", "--format", "json"],
    );
    assert!(highlights.as_array().unwrap().is_empty());
}
