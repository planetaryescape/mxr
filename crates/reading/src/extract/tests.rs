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

fn text(subject: &str, source: &str, text: &str) -> Extraction {
    extract(&IssueInput {
        subject,
        source: Some(source),
        html: None,
        text: Some(text),
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
    // The hidden preheader, the Like/Comment buttons and the footer are gone.
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
    // The greeting goes; the rest of the first paragraph is the standfirst
    // only when it is long enough.
    assert!(got
        .standfirst
        .as_deref()
        .is_some_and(|s| s.starts_with("a short one this week")));
}

#[test]
fn a_mailchimp_table_layout_digest_keeps_its_headings_and_blurbs() {
    let got = html(
        "SQLite Notes #112: WAL tricks and a release",
        "SQLite Notes",
        include_str!("../../tests/fixtures/mailchimp_digest.html"),
    );
    assert_eq!(got.shape, Shape::Digest);
    assert_eq!(got.headline, "WAL tricks and a release");
    assert_eq!(
        titles(&got),
        [
            "SQLite 3.51 release notes",
            "Local-first mail is having a moment",
            "Debugging SQLITE_BUSY without guessing",
            "WAL checkpoints, explained with pictures",
            "Shipping a sync engine in 2026",
        ]
    );
    assert!(got.links[0].tracked);
    assert_eq!(
        got.links[0].blurb.as_deref(),
        Some("The JSON functions got faster again, and there is a new pragma for reading the WAL from a second process.")
    );
    // A Google redirect names its target: unwrapped, tracking stripped.
    let wal = &got.links[3];
    assert_eq!(wal.url, "https://blog.example.org/wal-checkpoints");
    assert_eq!(wal.domain, "blog.example.org");
    assert!(!wal.tracked);
    assert!(got
        .standfirst
        .as_deref()
        .is_some_and(|s| s.starts_with("Welcome to issue 112")));
    let body = body_text(&got);
    assert!(!body.contains("View this email"));
    assert!(!body.contains("mailing address"));
    assert!(!body.contains("Twitter"));
}

#[test]
fn a_beehiiv_digest_skips_the_sponsor_and_read_online() {
    let got = html(
        "5 ways to double your open rate",
        "Growth Digest",
        include_str!("../../tests/fixtures/beehiiv_digest.html"),
    );
    assert_eq!(got.shape, Shape::Digest);
    let titles = titles(&got);
    assert_eq!(titles.len(), 5, "{titles:?}");
    assert_eq!(titles[0], "Five subject lines that doubled opens");
    assert!(!titles.iter().any(|t| t.contains("Acme Analytics")));
    assert!(!titles.iter().any(|t| t.eq_ignore_ascii_case("Read Online")));
    assert_eq!(
        got.links[1].blurb.as_deref(),
        Some("The first email sets the habit, and most of them ask for too much too soon.")
    );
    assert!(got.links.iter().all(|l| l.tracked && l.domain == "link.mail.beehiiv.com"));
    assert!(got
        .standfirst
        .as_deref()
        .is_some_and(|s| s.starts_with("This week is all about the inbox")));
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
fn a_plain_text_digest_finds_titles_above_and_beside_urls() {
    let got = text(
        "This week's links",
        "Plain News",
        include_str!("../../tests/fixtures/plain_digest.txt"),
    );
    assert_eq!(got.shape, Shape::Digest);
    assert_eq!(
        titles(&got),
        [
            "Local-first mail is having a moment",
            "SQLite 3.51 release notes",
            "Terminal workflows that stuck",
            "Why sync engines need tombstones",
            "A keyboard I can type on for a whole day",
        ]
    );
    assert_eq!(
        got.links[0].url,
        "https://demo.example.com/articles/local-first"
    );
    assert_eq!(got.links[3].url, "https://example.org/tombstones");
    assert!(!got.links.iter().any(|l| l.url.contains("unsubscribe")));
}

#[test]
fn a_plain_text_essay_is_single_with_the_greeting_skipped() {
    let got = text(
        "One SQLite file per device",
        "Sam's Essays",
        include_str!("../../tests/fixtures/plain_essay.txt"),
    );
    assert_eq!(got.shape, Shape::Single);
    assert!(got
        .standfirst
        .as_deref()
        .is_some_and(|s| s.starts_with("I spent the last month")));
    assert!(!body_text(&got).contains("Unsubscribe"));
}

#[test]
fn no_body_falls_back_to_the_snippet() {
    let got = extract(&IssueInput {
        subject: "[Notes] A short one",
        source: None,
        html: None,
        text: None,
        snippet: "Just a quick note to say the next issue is late.",
    });
    assert_eq!(got.headline, "A short one");
    assert_eq!(got.shape, Shape::Notice);
}

#[test]
fn clip_cuts_at_a_word_with_an_ellipsis() {
    assert_eq!(clip("short", 10), "short");
    assert_eq!(clip("one two three four five", 12), "one two…");
}
