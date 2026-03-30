use criterion::{black_box, criterion_group, criterion_main, Criterion};
use mxr_core::id::{AccountId, MessageId, ThreadId};
use mxr_core::types::{Address, Envelope, MessageFlags, SortOrder, UnsubscribeMethod};
use mxr_search::{parse_query, MxrSchema, QueryBuilder, SearchIndex};

fn bench_search_query(c: &mut Criterion) {
    let mut index = SearchIndex::in_memory().expect("search index");
    for i in 0..32 {
        let envelope = Envelope {
            id: MessageId::new(),
            account_id: AccountId::new(),
            provider_id: format!("bench-{i}"),
            thread_id: ThreadId::new(),
            message_id_header: None,
            in_reply_to: None,
            references: Vec::new(),
            from: Address {
                name: Some("Alice".to_string()),
                email: "alice@example.com".to_string(),
            },
            to: vec![Address {
                name: None,
                email: "team@example.com".to_string(),
            }],
            cc: Vec::new(),
            bcc: Vec::new(),
            subject: format!("Deployment update {i}"),
            date: chrono::Utc::now() - chrono::Duration::minutes(i),
            flags: if i % 2 == 0 {
                MessageFlags::READ
            } else {
                MessageFlags::empty()
            },
            snippet: "rollout canary metrics".to_string(),
            has_attachments: i % 3 == 0,
            size_bytes: 1024,
            unsubscribe: UnsubscribeMethod::None,
            label_provider_ids: vec!["INBOX".to_string()],
        };
        index.index_envelope(&envelope).expect("index envelope");
    }
    index.commit().expect("commit index");

    let schema = MxrSchema::build();
    let builder = QueryBuilder::new(&schema);
    let query_text = "from:alice@example.com is:unread deployment";

    c.bench_function("search_parse_build_execute", |b| {
        b.iter(|| {
            let ast = parse_query(black_box(query_text)).expect("parse query");
            let query = builder.build(&ast);
            let _ = index
                .search_ast(query, 10, 0, SortOrder::Relevance)
                .expect("execute query");
        });
    });
}

criterion_group!(benches, bench_search_query);
criterion_main!(benches);
