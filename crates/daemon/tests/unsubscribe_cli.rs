//! `mxr unsubscribe <address>...` end to end: the real binary, a fake-provider
//! daemon and the store. Receipts show which messages got a request.
use mxr_core::id::MessageId;
use mxr_core::types::Envelope;
use mxr_core::types::UnsubscribeMethod;
use mxr_core::AccountId;
use mxr_test_support::daemon::{daemon_lock, run_json, run_status_only, spawn_fake_daemon};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

/// Stamps `senders` so every message of a sender carries its own mailto target,
/// with dates rising through the sender's messages. Returns each sender's
/// newest message id, in `senders` order.
fn stamp_senders(
    runtime: &tokio::runtime::Runtime,
    store: &mxr_store::Store,
    senders: &[(String, Vec<Envelope>)],
) -> Vec<MessageId> {
    let base = 1_700_000_000i64;
    let mut newest = Vec::new();
    runtime.block_on(async {
        for (index, (_, messages)) in senders.iter().enumerate() {
            let mut messages = messages.clone();
            messages.sort_by_key(|envelope| envelope.date);
            for (position, envelope) in messages.iter_mut().enumerate() {
                envelope.unsubscribe = UnsubscribeMethod::Mailto {
                    address: format!("unsub-{index}@list.example"),
                    subject: None,
                };
                envelope.date = chrono::DateTime::from_timestamp(
                    base + index as i64 * 86_400 + position as i64 * 60,
                    0,
                )
                .expect("valid timestamp");
                store
                    .upsert_envelope(envelope)
                    .await
                    .expect("stamp the unsubscribe method");
            }
            newest.push(messages.last().expect("sender has messages").id.clone());
        }
    });
    newest
}

fn receipt_ids(
    runtime: &tokio::runtime::Runtime,
    store: &mxr_store::Store,
    ids: &[MessageId],
) -> BTreeSet<String> {
    runtime.block_on(async {
        let mut receipted = BTreeSet::new();
        for id in ids {
            if store
                .unsubscribe_receipt_method(id)
                .await
                .expect("receipt lookup")
                .is_some()
            {
                receipted.insert(id.as_str());
            }
        }
        receipted
    })
}

fn json_ids(value: &serde_json::Value) -> BTreeSet<String> {
    value["requests"]
        .as_array()
        .expect("requests array")
        .iter()
        .map(|request| {
            request["message_id"]
                .as_str()
                .expect("message id")
                .to_owned()
        })
        .collect()
}

fn run_unsubscribe(
    instance: &str,
    data_dir: &Path,
    config_dir: &Path,
    arguments: &[&str],
) -> serde_json::Value {
    let mut args = vec!["unsubscribe"];
    args.extend_from_slice(arguments);
    args.extend_from_slice(&["--format", "json"]);
    run_json(instance, data_dir, config_dir, &args)
}

#[test]
fn addresses_send_one_request_per_sender_and_the_dry_run_plans_exactly_what_is_sent() {
    let _guard = daemon_lock();
    let temp = tempfile::tempdir().expect("isolated unsubscribe profile");
    let (_daemon, instance, data_dir, config_dir) = spawn_fake_daemon(&temp, "unsubscribe-cli");
    run_status_only(&instance, &data_dir, &config_dir, &["sync", "--wait"]);

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("fixture runtime");
    let store = runtime
        .block_on(mxr_store::Store::new(&data_dir.join("mxr.db")))
        .expect("isolated profile store");
    let account: AccountId = runtime
        .block_on(store.list_accounts())
        .expect("accounts")
        .first()
        .expect("fake account")
        .id
        .clone();
    let envelopes = runtime
        .block_on(store.list_envelopes_by_account(&account, 500, 0))
        .expect("synced envelopes");

    let mut by_sender: BTreeMap<String, Vec<Envelope>> = BTreeMap::new();
    for envelope in envelopes {
        by_sender
            .entry(envelope.from.email.clone())
            .or_default()
            .push(envelope);
    }
    let senders: Vec<(String, Vec<Envelope>)> = by_sender
        .into_iter()
        .filter(|(_, messages)| messages.len() >= 3)
        .take(3)
        .collect();
    assert_eq!(
        senders.len(),
        3,
        "the fake mailbox must hold three senders with three messages each"
    );
    let (a, b, c) = (
        senders[0].0.clone(),
        senders[1].0.clone(),
        senders[2].0.clone(),
    );
    let all_ids: Vec<MessageId> = senders
        .iter()
        .flat_map(|(_, messages)| messages.iter().map(|envelope| envelope.id.clone()))
        .collect();
    let newest = stamp_senders(&runtime, &store, &senders);
    let (newest_a, newest_b, newest_c) = (newest[0].clone(), newest[1].clone(), newest[2].clone());

    // 1. A dry run for two senders names one request per sender, from each newest message.
    let dry = run_unsubscribe(
        &instance,
        &data_dir,
        &config_dir,
        &[a.as_str(), b.as_str(), "--dry-run"],
    );
    assert_eq!(dry["dry_run"], true);
    assert_eq!(dry["request_count"], 2, "{dry:#}");
    assert_eq!(dry["addresses"].as_array().expect("addresses").len(), 2);
    let planned = json_ids(&dry);
    assert_eq!(
        planned,
        BTreeSet::from([newest_a.as_str(), newest_b.as_str()]),
        "one request per sender, from its newest message"
    );
    assert!(
        receipt_ids(&runtime, &store, &all_ids).is_empty(),
        "a dry run sends nothing and writes no receipt"
    );

    // 2. A repeated address is planned once.
    let repeated = run_unsubscribe(
        &instance,
        &data_dir,
        &config_dir,
        &[a.as_str(), a.to_ascii_uppercase().as_str(), "--dry-run"],
    );
    assert_eq!(
        repeated["addresses"].as_array().expect("addresses").len(),
        1
    );
    assert_eq!(repeated["request_count"], 1);

    // 3. The real run sends exactly the planned requests.
    let real = run_unsubscribe(
        &instance,
        &data_dir,
        &config_dir,
        &[a.as_str(), b.as_str(), "--yes"],
    );
    assert_eq!(real["dry_run"], false);
    assert_eq!(real["succeeded"], 2, "{real:#}");
    assert_eq!(real["failed"], 0);
    assert_eq!(
        json_ids(&real),
        planned,
        "the sent requests match the dry run"
    );
    assert_eq!(
        receipt_ids(&runtime, &store, &all_ids),
        BTreeSet::from([newest_a.as_str(), newest_b.as_str()]),
        "exactly one message per sender was sent a request"
    );

    // 4. An already-unsubscribed address plus a new one sends only the new one.
    let mixed_dry = run_unsubscribe(
        &instance,
        &data_dir,
        &config_dir,
        &[a.as_str(), c.as_str(), "--dry-run"],
    );
    assert_eq!(mixed_dry["request_count"], 1, "{mixed_dry:#}");
    assert_eq!(mixed_dry["already_unsubscribed"], 1);
    assert_eq!(json_ids(&mixed_dry), BTreeSet::from([newest_c.as_str()]));
    let mixed_real = run_unsubscribe(
        &instance,
        &data_dir,
        &config_dir,
        &[a.as_str(), c.as_str(), "--yes"],
    );
    assert_eq!(mixed_real["succeeded"], 1, "{mixed_real:#}");
    assert_eq!(json_ids(&mixed_real), json_ids(&mixed_dry));
    assert_eq!(
        receipt_ids(&runtime, &store, &all_ids),
        BTreeSet::from([newest_a.as_str(), newest_b.as_str(), newest_c.as_str()]),
        "only the new sender gained a receipt"
    );

    // 5. An address mixed with a message id fails before anything is sent.
    let id_text = all_ids[0].as_str();
    let not_sent = assert_cmd::Command::cargo_bin("mxr")
        .expect("mxr binary")
        .env("MXR_INSTANCE", &instance)
        .env("MXR_DATA_DIR", &data_dir)
        .env("MXR_CONFIG_DIR", &config_dir)
        .env_remove("EDITOR")
        .env_remove("VISUAL")
        .args(["unsubscribe", a.as_str(), id_text.as_str(), "--yes"])
        .output()
        .expect("run mixed command");
    assert!(!not_sent.status.success());
    assert!(
        String::from_utf8_lossy(&not_sent.stderr)
            .contains("Pass message ids or email addresses, not both"),
        "{}",
        String::from_utf8_lossy(&not_sent.stderr)
    );
    assert_eq!(
        receipt_ids(&runtime, &store, &all_ids),
        BTreeSet::from([newest_a.as_str(), newest_b.as_str(), newest_c.as_str()]),
        "the mixed command wrote no receipt"
    );
}
