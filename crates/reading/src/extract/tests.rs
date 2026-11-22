use super::*;

fn html(subject: &str, source: &str, html: &str) -> Extraction {
    extract(&IssueInput {
        subject,
        source: Some(source),
        html: Some(html),
        text: None,
        snippet: "",
    })
}

fn titles(extraction: &Extraction) -> Vec<&str> {
    extraction.links.iter().map(|l| l.title.as_str()).collect()
}

fn body_text(extraction: &Extraction) -> String {
    extraction
        .paragraphs
        .iter()
        .map(|p| p.text.as_str())
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn a_substack_essay_is_one_item_and_its_inline_links_are_not_a_digest() {
    let got = html(
        "Why sync engines need tombstones",
        "Platform Notes",
        include_str!("../../tests/fixtures/substack_essay.html"),
    );
    assert_eq!(got.shape, Shape::Single);
    assert_eq!(got.headline, "Why sync engines need tombstones");
    assert!(got.links.is_empty());
    assert!(got.main_link.is_none());
    let standfirst = got.standfirst.as_deref().expect("a standfirst");
    assert!(
        standfirst.starts_with("Every sync engine I have built"),
        "{standfirst}"
    );
    let body = body_text(&got);
    assert!(!body.contains("fix is older than you think"));
    assert!(!body.contains("Restack"));
    assert!(!body.contains("Unsubscribe"));
    assert!(!body.contains("548 Market Street"));
    assert!((250..=330).contains(&got.words), "{} words", got.words);
}

#[test]
fn a_substack_link_roundup_is_a_digest_of_tracked_links() {
    let got = html(
        "Links I liked, week 41",
        "Weekly Links",
        include_str!("../../tests/fixtures/substack_links_digest.html"),
    );
    assert_eq!(got.shape, Shape::Digest);
    assert_eq!(
        titles(&got),
        [
            "Local-first mail is having a moment",
            "SQLite 3.51 release notes",
            "Terminal workflows that stuck",
            "Why sync engines need tombstones",
            "The quiet death of the three-pane layout",
            "A sourdough schedule for people with jobs",
        ]
    );
    let first = &got.links[0];
    assert!(first.tracked, "Substack redirects hide the site");
    assert_eq!(first.domain, "substack.com");
    assert_eq!(
        first.blurb.as_deref(),
        Some("Why three new mail clients all keep your mailbox on your own disk, and what that buys you offline.")
    );
    assert!(got
        .standfirst
        .as_deref()
        .is_some_and(|s| s.starts_with("A short one this week")));
}

#[test]
fn a_teaser_is_the_article_it_points_at() {
    let got = html(
        "New post: Shipping a sync engine in 2026",
        "Platform Weekly",
        include_str!("../../tests/fixtures/teaser.html"),
    );
    assert_eq!(got.shape, Shape::Teaser);
    let main = got.main_link.expect("a main link");
    assert_eq!(
        main.url,
        "https://blog.platformweekly.example.com/2026/10/shipping-a-sync-engine"
    );
    assert_eq!(main.domain, "blog.platformweekly.example.com");
    assert_eq!(main.title, got.headline);
    assert!(got
        .standfirst
        .as_deref()
        .is_some_and(|s| s.starts_with("We rebuilt replication")));
}

#[test]
fn a_short_announcement_is_a_notice() {
    let got = html(
        "Starting in one hour",
        "Example Events",
        include_str!("../../tests/fixtures/notice.html"),
    );
    assert_eq!(got.shape, Shape::Notice);
    assert!(got.links.is_empty());
    assert!(got.words < 20);
}

#[test]
fn a_lost_helper_reads_as_a_page_with_no_paragraphs_or_links() {
    let lost = std::sync::Arc::new(mxr_html_worker::Supervisor::new(
        mxr_html_worker::Config::new("/nonexistent/mxr-helper", Vec::new()),
    ));
    let got = mxr_html_worker::with_supervisor(lost, || {
        html(
            "Weekly",
            "Weekly",
            "<p>Some real words in an issue that the extractor would normally find.</p>",
        )
    });
    assert!(got.links.is_empty());
    assert!(got
        .paragraphs
        .iter()
        .all(|paragraph| !paragraph.text.contains("real words")));
}
