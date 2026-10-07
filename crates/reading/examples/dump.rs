//! Prints what extraction finds in an issue file: `cargo run -p
//! mxr-reading --example dump -- SUBJECT FILE...`.
fn main() {
    let mut args = std::env::args().skip(1);
    let subject = args.next().unwrap_or_default();
    for path in args {
        let body = std::fs::read_to_string(&path).unwrap_or_default();
        let is_html = path.ends_with(".html");
        let got = mxr_reading::extract(&mxr_reading::IssueInput {
            subject: &subject,
            source: None,
            html: is_html.then_some(body.as_str()),
            text: (!is_html).then_some(body.as_str()),
            snippet: "",
        });
        println!(
            "== {path}: {:?} words={} headline={:?}",
            got.shape, got.words, got.headline
        );
        println!("standfirst: {:?}", got.standfirst);
        for p in &got.paragraphs {
            println!(
                "  [{:?}] {}",
                p.kind,
                p.text.chars().take(90).collect::<String>()
            );
        }
        for l in &got.links {
            println!("  -> {} | {} | {:?}", l.title, l.domain, l.blurb);
        }
        println!("main: {:?}", got.main_link);
    }
}
