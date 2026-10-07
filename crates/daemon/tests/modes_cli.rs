//! `mxr modes` against a real daemon: the copy each mode teaches itself
//! with, as JSON, and the first-encounter card's seen state surviving
//! between commands.

#![expect(
    clippy::unwrap_used,
    reason = "integration tests unwrap to keep fixture failures direct"
)]

use mxr_test_support::daemon::{daemon_lock, run_json, spawn_fake_daemon};
use tempfile::TempDir;

#[test]
fn modes_explain_prints_the_to_do_guide_and_card_state_holds() {
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

    let closed = run_json(
        &instance,
        &data_dir,
        &config_dir,
        &["modes", "card", "todo", "--format", "json"],
    );
    assert_eq!(closed[0]["card_seen"], true, "{closed}");
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
        ["now", "messages", "todo"],
        "every shipped mode, Now first"
    );
    assert_eq!(again[2]["card_seen"], true, "a new command sees it closed");
    assert_eq!(
        again[0]["card_seen"], false,
        "closing one card leaves the others"
    );

    let shown = run_json(
        &instance,
        &data_dir,
        &config_dir,
        &["modes", "card", "todo", "--show", "--format", "json"],
    );
    assert_eq!(shown[0]["card_seen"], false, "{shown}");
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
        &["messages", "person", row["id"].as_str().unwrap(), "--format", "json"],
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
