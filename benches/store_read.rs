use criterion::{black_box, criterion_group, criterion_main, Criterion};
use mxr_core::id::{AccountId, MessageId, ThreadId};
use mxr_core::types::{
    Account, Address, BackendRef, Envelope, MessageFlags, ProviderKind, UnsubscribeMethod,
};
use mxr_store::Store;

fn bench_store_read(c: &mut Criterion) {
    let runtime = tokio::runtime::Runtime::new().expect("tokio runtime");
    let (store, account_id, thread_id) = runtime.block_on(async {
        let store = Store::in_memory().await.expect("store");
        let account_id = AccountId::new();
        store
            .insert_account(&Account {
                id: account_id.clone(),
                name: "Bench".to_string(),
                email: "bench@example.com".to_string(),
                sync_backend: Some(BackendRef {
                    provider_kind: ProviderKind::Fake,
                    config_key: "bench".to_string(),
                }),
                send_backend: None,
                enabled: true,
            })
            .await
            .expect("insert account");

        let thread_id = ThreadId::new();
        for i in 0..64 {
            store
                .upsert_envelope(&Envelope {
                    id: MessageId::new(),
                    account_id: account_id.clone(),
                    provider_id: format!("bench-{i}"),
                    thread_id: thread_id.clone(),
                    message_id_header: None,
                    in_reply_to: None,
                    references: Vec::new(),
                    from: Address {
                        name: Some("Bench".to_string()),
                        email: "bench@example.com".to_string(),
                    },
                    to: vec![Address {
                        name: None,
                        email: "team@example.com".to_string(),
                    }],
                    cc: Vec::new(),
                    bcc: Vec::new(),
                    subject: format!("Store benchmark {i}"),
                    date: chrono::Utc::now() - chrono::Duration::minutes(i),
                    flags: MessageFlags::READ,
                    snippet: "seed".to_string(),
                    has_attachments: false,
                    size_bytes: 512,
                    unsubscribe: UnsubscribeMethod::None,
                    label_provider_ids: Vec::new(),
                })
                .await
                .expect("insert envelope");
        }

        (store, account_id, thread_id)
    });

    c.bench_function("store_list_envelopes_by_account", |b| {
        b.iter(|| {
            runtime
                .block_on(store.list_envelopes_by_account(black_box(&account_id), 50, 0))
                .expect("list envelopes");
        });
    });

    c.bench_function("store_get_thread", |b| {
        b.iter(|| {
            runtime
                .block_on(store.get_thread(black_box(&thread_id)))
                .expect("get thread");
        });
    });
}

criterion_group!(benches, bench_store_read);
criterion_main!(benches);
