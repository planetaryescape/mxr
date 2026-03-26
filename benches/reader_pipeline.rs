use criterion::{black_box, criterion_group, criterion_main, Criterion};
use mxr::mxr_reader::{clean, ReaderConfig};

fn bench_reader_pipeline(c: &mut Criterion) {
    let html = include_str!("../crates/reader/tests/fixtures/newsletter.html");
    let config = ReaderConfig::default();

    c.bench_function("reader_clean_html_newsletter", |b| {
        b.iter(|| {
            let _ = clean(None, Some(black_box(html)), &config);
        });
    });
}

criterion_group!(benches, bench_reader_pipeline);
criterion_main!(benches);
