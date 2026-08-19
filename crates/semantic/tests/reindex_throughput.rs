//! Manual throughput harness for the semantic reindex path.
//!
//! Ignored by default: it needs the real fastembed model (~128 MB on disk) and
//! runs for minutes at realistic message counts.
//!
//! ```sh
//! MXR_THROUGHPUT_MESSAGES=2000 \
//!   MXR_THROUGHPUT_MODEL_CACHE="$HOME/Library/Application Support/mxr-demo/models" \
//!   cargo test -p mxr-semantic --features local --test reindex_throughput \
//!   -- --ignored --nocapture
//! ```
#![cfg(feature = "local")]
#![expect(clippy::unwrap_used, reason = "manual harness fails loudly")]

use mxr_config::SemanticConfig;
use mxr_core::id::{AccountId, MessageId, ThreadId};
use mxr_core::types::{
    Account, Address, BackendRef, Envelope, MessageBody, MessageFlags, MessageMetadata,
    ProviderKind, UnsubscribeMethod,
};
use mxr_semantic::SemanticEngine;
use mxr_store::Store;
use std::collections::BTreeSet;
use std::path::Path;
use std::sync::Arc;

fn word_body(seed: usize, words: usize) -> String {
    const VOCAB: [&str; 16] = [
        "deployment",
        "invoice",
        "roadmap",
        "incident",
        "quarterly",
        "renewal",
        "onboarding",
        "latency",
        "contract",
        "postmortem",
        "budget",
        "migration",
        "retention",
        "handoff",
        "release",
        "escalation",
    ];
    (0..words)
        .map(|index| VOCAB[(seed + index) % VOCAB.len()])
        .collect::<Vec<_>>()
        .join(" ")
}

async fn seed(store: &Store, count: usize) {
    let account_id = AccountId::new();
    store
        .insert_account(&Account {
            id: account_id.clone(),
            name: "Throughput".into(),
            email: "throughput@example.com".into(),
            sync_backend: Some(BackendRef {
                provider_kind: ProviderKind::Fake,
                config_key: "throughput".into(),
            }),
            send_backend: None,
            enabled: true,
        })
        .await
        .unwrap();

    for index in 0..count {
        let envelope = Envelope {
            id: MessageId::new(),
            account_id: account_id.clone(),
            provider_id: format!("throughput-{index}"),
            thread_id: ThreadId::new(),
            message_id_header: Some(format!("<throughput-{index}@example.com>")),
            in_reply_to: None,
            references: Vec::new(),
            from: Address {
                name: Some("Sender".into()),
                email: format!("sender{}@example.com", index % 50),
            },
            to: vec![Address {
                name: None,
                email: "throughput@example.com".into(),
            }],
            cc: Vec::new(),
            bcc: Vec::new(),
            subject: format!("Throughput message {index}"),
            date: chrono::Utc::now(),
            flags: MessageFlags::READ,
            snippet: word_body(index, 12),
            has_attachments: false,
            size_bytes: 2048,
            unsubscribe: UnsubscribeMethod::None,
            link_count: 0,
            body_word_count: 160,
            label_provider_ids: vec!["INBOX".into()],
            keywords: BTreeSet::new(),
        };
        let body = MessageBody {
            message_id: envelope.id.clone(),
            text_plain: Some(word_body(index, 160)),
            text_html: None,
            attachments: Vec::new(),
            fetched_at: chrono::Utc::now(),
            metadata: MessageMetadata::default(),
        };
        store.upsert_envelope(&envelope).await.unwrap();
        store.insert_body(&body).await.unwrap();
    }
}

/// Resident set size of this process, via `ps`. Good enough to see which
/// phase of a reindex owns the peak.
fn rss_mb(label: &str) {
    let output = std::process::Command::new("ps")
        .args(["-o", "rss=", "-p", &std::process::id().to_string()])
        .output()
        .expect("ps");
    let kb = String::from_utf8_lossy(&output.stdout)
        .trim()
        .parse::<f64>()
        .unwrap_or_default();
    println!("rss after {label}: {:.0} MB", kb / 1024.0);
}

fn link_model_cache(data_dir: &Path) {
    let Some(source) = std::env::var_os("MXR_THROUGHPUT_MODEL_CACHE") else {
        return;
    };
    std::os::unix::fs::symlink(source, data_dir.join("models")).unwrap();
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "manual throughput harness; needs the real embedding model"]
async fn reindex_throughput() {
    let count = std::env::var("MXR_THROUGHPUT_MESSAGES")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(500);

    let temp = tempfile::tempdir().unwrap();
    link_model_cache(temp.path());
    let store = Arc::new(Store::new(&temp.path().join("mxr.db")).await.unwrap());

    let seed_started = std::time::Instant::now();
    seed(&store, count).await;
    println!("seeded {count} messages in {:?}", seed_started.elapsed());
    rss_mb("seed");

    let config = SemanticConfig::default();
    let profile = config.active_profile;
    let mut engine = SemanticEngine::new(store.clone(), temp.path(), config);
    let install_started = std::time::Instant::now();
    engine.install_profile(profile).await.unwrap();
    println!("model load + warmup: {:?}", install_started.elapsed());

    let reindex_started = std::time::Instant::now();
    let record = engine.reindex_active().await.unwrap();
    let elapsed = reindex_started.elapsed();

    rss_mb("first reindex");
    let chunks = store.collect_record_counts().await.unwrap().semantic_chunks;
    if std::env::var_os("MXR_THROUGHPUT_HISTOGRAM").is_some() {
        // Materialises every row, so it is off by default: it would dominate
        // the peak RSS this harness is used to measure.
        print_chunk_word_histogram(&store, &record.id).await;
    }
    println!(
        "reindex {count} messages / {chunks} chunks in {:.2}s => {:.1} msg/s, {:.1} chunks/s",
        elapsed.as_secs_f64(),
        count as f64 / elapsed.as_secs_f64(),
        f64::from(chunks) / elapsed.as_secs_f64(),
    );
    assert_eq!(record.progress_completed, count as u32);

    let rerun_started = std::time::Instant::now();
    engine.reindex_active().await.unwrap();
    println!("second reindex (no changes): {:?}", rerun_started.elapsed());
    rss_mb("second reindex");

    // Separates the index's live set from allocator high-water: whatever is
    // still resident here was retained by the allocator, not held by us.
    drop(engine);
    rss_mb("dropping the engine");
}

/// Chunk length distribution in words. The tokenizer pads each ONNX batch to
/// the longest row in it (fastembed uses `PaddingStrategy::BatchLongest`), so
/// this is what decides how much padding the batches carry.
async fn print_chunk_word_histogram(store: &Store, profile_id: &mxr_core::id::SemanticProfileId) {
    let rows = store.list_semantic_embeddings(profile_id).await.unwrap();
    let mut words = rows
        .iter()
        .map(|(chunk, _)| chunk.normalized.split_whitespace().count())
        .collect::<Vec<_>>();
    if words.is_empty() {
        return;
    }
    words.sort_unstable();
    let percentile = |p: usize| words[(words.len() - 1) * p / 100];
    println!(
        "chunk words: n={} min={} p50={} p90={} p99={} max={} mean={:.1}",
        words.len(),
        words[0],
        percentile(50),
        percentile(90),
        percentile(99),
        words[words.len() - 1],
        words.iter().sum::<usize>() as f64 / words.len() as f64,
    );
}
