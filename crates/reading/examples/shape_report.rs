//! Shape counts and extraction time over issues read from stdin, one JSON
//! object per line: `{"subject", "source", "html", "text", "snippet"}`.
//! Prints counts only, so it can run over a real mailbox without echoing
//! any of it: `... | cargo run -p mxr-reading --example shape_report`.
use std::collections::BTreeMap;
use std::io::BufRead;

#[derive(serde::Deserialize)]
struct Line {
    subject: String,
    source: Option<String>,
    html: Option<String>,
    text: Option<String>,
    #[serde(default)]
    snippet: String,
}

fn main() {
    let mut shapes: BTreeMap<&str, usize> = BTreeMap::new();
    let mut links = 0usize;
    let mut with_standfirst = 0usize;
    let mut headline_changed = 0usize;
    let mut total = std::time::Duration::ZERO;
    let mut slowest = std::time::Duration::ZERO;
    let mut n = 0usize;
    for line in std::io::stdin().lock().lines().map_while(Result::ok) {
        let Ok(issue) = serde_json::from_str::<Line>(&line) else {
            continue;
        };
        let started = std::time::Instant::now();
        let got = mxr_reading::extract(&mxr_reading::IssueInput {
            subject: &issue.subject,
            source: issue.source.as_deref(),
            html: issue.html.as_deref(),
            text: issue.text.as_deref(),
            snippet: &issue.snippet,
        });
        let took = started.elapsed();
        total += took;
        slowest = slowest.max(took);
        n += 1;
        *shapes.entry(got.shape.id()).or_default() += 1;
        links += got.links.len();
        with_standfirst += usize::from(got.standfirst.is_some());
        headline_changed += usize::from(got.headline != issue.subject.trim());
    }
    println!("issues {n}");
    for (shape, count) in &shapes {
        println!("shape {shape} {count}");
    }
    println!("digest_links {links}");
    println!("with_standfirst {with_standfirst}");
    println!("headline_cleaned {headline_changed}");
    if n > 0 {
        println!("mean_ms {:.2}", total.as_secs_f64() * 1000.0 / n as f64);
        println!("max_ms {:.2}", slowest.as_secs_f64() * 1000.0);
    }
}
