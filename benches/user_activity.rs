//! Micro-benchmarks for the activity log. Targets the hard gates in
//! `docs/activity-log.md`. Run with:
//!
//!     cargo bench --bench user_activity
//!
//! These benches use an in-memory SQLite store and a warm WAL, so the
//! numbers are best-case. Real-disk numbers should still meet the same
//! gates because the indexes carry the heavy lifting.

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use mxr_store::{ActivityFilter, ActivityInsert, Store, Tier};

fn seed_rows(store: &Store, n: usize) {
    let rt = tokio::runtime::Handle::current();
    rt.block_on(async {
        let batch: Vec<ActivityInsert> = (0..n)
            .map(|i| ActivityInsert {
                ts: (i as i64) * 100,
                account_id: None,
                source: if i % 3 == 0 { "tui" } else { "cli" },
                action: if i % 5 == 0 {
                    "mail.archive"
                } else if i % 3 == 0 {
                    "search.run"
                } else {
                    "view.open_screen"
                },
                target_kind: Some("thread"),
                target_id: Some(if i % 5 == 0 { "thr_archive" } else { "thr_other" }),
                tier: if i % 5 == 0 {
                    Tier::Important
                } else if i % 3 == 0 {
                    Tier::Standard
                } else {
                    Tier::Ephemeral
                },
                context: None,
            })
            .collect();
        store
            .record_activity_batch(&batch)
            .await
            .expect("seed batch");
    });
}

fn bench_insert_serial(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().expect("tokio");
    let store = rt.block_on(async { Store::in_memory().await.expect("store") });
    let _guard = rt.enter();

    let mut group = c.benchmark_group("user_activity_insert");
    group.sample_size(50);
    let mut counter: i64 = 0;
    group.bench_function("serial", |b| {
        b.iter(|| {
            counter += 1;
            rt.block_on(async {
                store
                    .record_activity(ActivityInsert {
                        ts: counter,
                        account_id: None,
                        source: "tui",
                        action: "mail.archive",
                        target_kind: Some("thread"),
                        target_id: Some("thr_1"),
                        tier: Tier::Important,
                        context: None,
                    })
                    .await
                    .expect("record");
            });
        })
    });
    group.finish();
}

fn bench_list_unfiltered(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().expect("tokio");
    let store = rt.block_on(async { Store::in_memory().await.expect("store") });
    let _guard = rt.enter();
    seed_rows(&store, 10_000);

    let mut group = c.benchmark_group("user_activity_list");
    group.sample_size(30);
    group.bench_function("first_50_unfiltered_over_10k", |b| {
        b.iter(|| {
            rt.block_on(async {
                let page = store
                    .list_activity(&ActivityFilter::default(), 50, None)
                    .await
                    .expect("list");
                black_box(page.rows.len());
            });
        })
    });
    group.finish();
}

fn bench_list_by_action_prefix(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().expect("tokio");
    let store = rt.block_on(async { Store::in_memory().await.expect("store") });
    let _guard = rt.enter();
    seed_rows(&store, 10_000);

    let mut filter = ActivityFilter::default();
    filter.action_prefix = Some("mail.".into());

    let mut group = c.benchmark_group("user_activity_list");
    group.sample_size(30);
    group.bench_function("by_action_prefix_mail_over_10k", |b| {
        b.iter(|| {
            rt.block_on(async {
                let page = store.list_activity(&filter, 50, None).await.expect("list");
                black_box(page.rows.len());
            });
        })
    });
    group.finish();
}

fn bench_stats_by_action(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().expect("tokio");
    let store = rt.block_on(async { Store::in_memory().await.expect("store") });
    let _guard = rt.enter();
    seed_rows(&store, 10_000);

    let mut group = c.benchmark_group("user_activity_stats");
    group.sample_size(30);
    group.bench_function("by_action_over_10k", |b| {
        b.iter(|| {
            rt.block_on(async {
                let buckets = store
                    .activity_stats_by_action(0, i64::MAX / 2)
                    .await
                    .expect("stats");
                black_box(buckets.len());
            });
        })
    });
    group.finish();
}

criterion_group!(
    benches,
    bench_insert_serial,
    bench_list_unfiltered,
    bench_list_by_action_prefix,
    bench_stats_by_action,
);
criterion_main!(benches);
