use criterion::{black_box, criterion_group, criterion_main, Criterion};
use mxr::handler::handle_request;
use mxr::state::AppState;
use mxr_core::id::{AccountId, MessageId, ThreadId};
use mxr_core::types::{
    Account, Address, BackendRef, Envelope, MessageBody, MessageFlags, MessageMetadata,
    ProviderKind, SearchMode, SortOrder, UnsubscribeMethod,
};
use mxr_protocol::{IpcMessage, IpcPayload, Request};
use mxr_search::{SearchIndexEntry, SearchUpdateBatch};
use std::sync::Arc;
use tempfile::TempDir;

fn bench_daemon_burst(c: &mut Criterion) {
    let runtime = tokio::runtime::Runtime::new().expect("tokio runtime");

    c.bench_function("daemon_burst_status_search_during_semantic_ingest", |b| {
        let temp = TempDir::new().expect("temp dir");
        std::env::set_var("MXR_DATA_DIR", temp.path().join("data"));
        std::env::set_var("MXR_CONFIG_DIR", temp.path().join("config"));

        let (state, message_ids) = runtime.block_on(async { daemon_bench_fixture().await });

        b.iter(|| {
            runtime.block_on(async {
                state
                    .semantic
                    .enqueue_ingest_messages(black_box(&message_ids))
                    .await
                    .expect("enqueue semantic ingest");

                let responses = futures::future::join_all((0..16u64).map(|index| {
                    let state = state.clone();
                    async move {
                        let message = if index % 2 == 0 {
                            search_request(index)
                        } else {
                            status_request(index)
                        };
                        handle_request(&state, &message).await
                    }
                }))
                .await;

                black_box(responses);
            });
        });
    });
}

async fn daemon_bench_fixture() -> (Arc<AppState>, Vec<MessageId>) {
    let state = Arc::new(AppState::new().await.expect("app state"));
    let account_id = AccountId::new();
    state
        .store
        .insert_account(&Account {
            id: account_id.clone(),
            name: "Bench".into(),
            email: "bench@example.com".into(),
            sync_backend: Some(BackendRef {
                provider_kind: ProviderKind::Fake,
                config_key: "bench".into(),
            }),
            send_backend: None,
            enabled: true,
        })
        .await
        .expect("insert account");

    let mut batch = SearchUpdateBatch::default();
    let mut message_ids = Vec::new();
    for index in 0..128 {
        let envelope = Envelope {
            id: MessageId::new(),
            account_id: account_id.clone(),
            provider_id: format!("daemon-bench-{index}"),
            thread_id: ThreadId::new(),
            message_id_header: Some(format!("<daemon-bench-{index}@example.com>")),
            in_reply_to: None,
            references: Vec::new(),
            from: Address {
                name: Some("Bench".into()),
                email: "bench@example.com".into(),
            },
            to: vec![Address {
                name: None,
                email: "team@example.com".into(),
            }],
            cc: Vec::new(),
            bcc: Vec::new(),
            subject: format!("Daemon benchmark {index}"),
            date: chrono::Utc::now(),
            flags: MessageFlags::READ,
            snippet: "background ingest active".into(),
            has_attachments: false,
            size_bytes: 1024,
            unsubscribe: UnsubscribeMethod::None,
            label_provider_ids: vec!["INBOX".into()],
        };
        let body = MessageBody {
            message_id: envelope.id.clone(),
            text_plain: Some(format!(
                "Search benchmark body {index} with deployment notes and launch status."
            )),
            text_html: None,
            attachments: Vec::new(),
            fetched_at: chrono::Utc::now(),
            metadata: MessageMetadata::default(),
        };

        state
            .store
            .upsert_envelope(&envelope)
            .await
            .expect("upsert envelope");
        state.store.insert_body(&body).await.expect("insert body");
        batch.entries.push(SearchIndexEntry {
            envelope: envelope.clone(),
            body: Some(body),
        });
        message_ids.push(envelope.id);
    }

    state.search.apply_batch(batch).await.expect("index batch");
    (state, message_ids)
}

fn status_request(id: u64) -> IpcMessage {
    IpcMessage {
        id,
        payload: IpcPayload::Request(Request::GetStatus),
    }
}

fn search_request(id: u64) -> IpcMessage {
    IpcMessage {
        id,
        payload: IpcPayload::Request(Request::Search {
            query: "deployment".into(),
            limit: 20,
            offset: 0,
            mode: Some(SearchMode::Lexical),
            sort: Some(SortOrder::DateDesc),
            explain: false,
        }),
    }
}

criterion_group!(benches, bench_daemon_burst);
criterion_main!(benches);
