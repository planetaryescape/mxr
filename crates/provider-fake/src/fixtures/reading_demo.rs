//! Newsletters that give the demo's Reading an edition to show.
//!
//! * Long Reads Weekly: an essay a week; the newest arrived this morning,
//!   one last week, one about to fade, and older issues the demo command
//!   marks as read to the end, so the source leads the edition.
//! * Local-first Links: a weekly digest of six links, two of whose
//!   articles the demo can "fetch" from `demo_article_html`.
//! * Platform Weekly: a teaser pointing at one article.
//! * Growth Digest: eleven weekly issues nobody opened, the newest in its
//!   last day: the Fading band and the unsubscribe offer.
//!
//! Every link points at `*.demo.mxr.local`, which the demo serves from
//! here and the real fetch refuses, so the demo never touches the network.

use super::{build_demo_msg, DemoMessage};
use chrono::{DateTime, Duration, Utc};
use mxr_core::id::{AccountId, ThreadId};
use mxr_core::types::{Address, Envelope, MessageBody, MessageFlags, UnsubscribeMethod};

const LONG_READS: usize = 10;
const LINKS: usize = 2;
const PLATFORM: usize = 3;
const GROWTH: usize = 11;

/// Messages `reading_demo_messages` returns.
pub(super) const READING_DEMO_MESSAGE_COUNT: usize = LONG_READS + LINKS + PLATFORM + GROWTH;

/// Long Reads issues the demo command marks as read to the end, by their
/// position among `reading_demo_messages`: every one but the newest two.
pub(super) const READING_DEMO_FINISHED: std::ops::Range<usize> = 2..LONG_READS;

/// The newest Local-first Links digest, whose WAL link the demo command
/// puts on Later.
pub(super) const READING_DEMO_LATER_DIGEST: usize = LONG_READS;

pub const DEMO_ARTICLE_SYNC: &str = "https://platform.demo.mxr.local/2026/10/shipping-a-sync-engine";
pub const DEMO_ARTICLE_LOCAL_FIRST: &str = "https://links.demo.mxr.local/articles/local-first-mail";
pub const DEMO_ARTICLE_WAL: &str = "https://links.demo.mxr.local/articles/wal-checkpoints";

/// Hours old: 0.3, 6.5 and 13.1 days, then weekly from 21 days.
const LONG_READS_AGE_HOURS: [i64; LONG_READS] = [7, 156, 314, 504, 672, 840, 1008, 1176, 1344, 1512];

const LONG_READS_TITLES: [&str; LONG_READS] = [
    "The quiet death of the three-pane layout",
    "Why every inbox is a to-do list nobody asked for",
    "Notes on reading slowly in a fast feed",
    "The case for software that forgets",
    "What a paper newspaper knew about the end",
    "Local files, remote friends",
    "Against the unread count",
    "The half-life of a link",
    "Keyboards, habits and the second week",
    "How a mailbox became an archive",
];

fn thread(account_id: &AccountId, name: &str) -> ThreadId {
    ThreadId::from_scoped_provider_id(account_id, "fake", &format!("demo-reading-{name}"))
}

fn issue(from: &Address, to: &Address, subject: &str, text: String, date: DateTime<Utc>) -> DemoMessage {
    DemoMessage {
        from: from.clone(),
        to: vec![to.clone()],
        cc: Vec::new(),
        snippet: text
            .split_whitespace()
            .take(24)
            .collect::<Vec<_>>()
            .join(" "),
        subject: subject.to_string(),
        body_text: text,
        date,
        flags: MessageFlags::empty(),
        has_attachments: false,
        category: 9,
        unsubscribe: UnsubscribeMethod::OneClick {
            url: format!(
                "https://{}/unsubscribe",
                from.email.split('@').nth(1).unwrap_or("demo.mxr.local")
            ),
        },
        label_provider_ids: vec!["INBOX".to_string()],
        in_reply_to: None,
        references: Vec::new(),
    }
}

fn footer(name: &str, domain: &str) -> String {
    format!(
        r#"<table class="footer"><tr><td><p>You're receiving this because you subscribed to {name}.</p><p><a href="https://{domain}/unsubscribe">Unsubscribe</a> · <a href="https://{domain}/preferences">Manage preferences</a></p><p>© 2026 {name}</p></td></tr></table>"#
    )
}

fn essay_paragraphs(title: &str) -> Vec<String> {
    vec![
        format!("{title}. Every reader since 2002 shipped the same window: a list of sources on the left, a list of items in the middle and the item itself on the right. It was a good answer to a narrow question, which was how to show a lot of feeds on a small screen, and it hardened into the only answer."),
        "The trouble is the middle column. It turns every item into a row, and every row into a small debt. A row has a bold weight until you open it and a count beside its source until you clear it, so the shape of the window tells you that reading is a job with a backlog.".to_string(),
        "Paper never did this. A newspaper arrives, you read the front page and the two pieces that caught you, and the rest goes out with the recycling. Nobody keeps a count of the articles they skipped, and nobody feels behind on yesterday's paper.".to_string(),
        "A better reader starts from that. It shows what arrived since you last looked, puts the writers you actually finish at the top, and lets the rest fade on a schedule that matches how often each source writes. A daily fades in two days; a weekly lasts a fortnight.".to_string(),
        "Keeping something should be a choice you make, not a default you fight. One key puts a piece on a short shelf for later, and that shelf is the only place a count belongs, because everything on it is something you asked for.".to_string(),
        "When you do sit down to read, the page should look like a page: a column about sixty-six characters wide, generous line height, no sidebar, no share buttons, and a quiet line that tells you how much is left. The original layout is one key away for the newsletters whose design is the point.".to_string(),
        "None of this needs a model or a server. It needs the reader to remember a little about you, locally, and to admit that most of what arrives is not owed anything. That admission is the whole design.".to_string(),
    ]
}

fn essay_html(title: &str, paragraphs: &[String]) -> String {
    let body: String = paragraphs.iter().map(|p| format!("<p>{p}</p>")).collect();
    format!(
        r#"<!doctype html><html><head><meta charset="utf-8"></head><body><div style="display:none;max-height:0;overflow:hidden">A short essay from Long Reads Weekly</div><table width="100%"><tr><td><p><a href="https://longreads.demo.mxr.local/view">View in browser</a></p><h1>{title}</h1>{body}{}</td></tr></table></body></html>"#,
        footer("Long Reads Weekly", "longreads.demo.mxr.local")
    )
}

const LINK_ITEMS: [(&str, &str, &str); 6] = [
    (
        "Local-first mail is having a moment",
        DEMO_ARTICLE_LOCAL_FIRST,
        "Three clients that keep your whole mailbox on your own disk, and what that buys you offline.",
    ),
    (
        "WAL checkpoints, explained with pictures",
        DEMO_ARTICLE_WAL,
        "Passive, full, restart and truncate, and when each one blocks your writers.",
    ),
    (
        "Terminal workflows that stuck",
        "https://links.demo.mxr.local/articles/terminal-workflows",
        "A year of living in the terminal, and the three tools that survived it.",
    ),
    (
        "Why sync engines need tombstones",
        "https://links.demo.mxr.local/articles/tombstones",
        "Deletes are the hard part, and the fix is from the eighties.",
    ),
    (
        "A sourdough schedule for people with jobs",
        "https://links.demo.mxr.local/articles/sourdough",
        "Mix at night, shape in the morning, bake after work.",
    ),
    (
        "SQLite 3.51 release notes, annotated",
        "https://links.demo.mxr.local/articles/sqlite-3-51",
        "Faster JSON functions and a new way to read the WAL from a second process.",
    ),
];

fn digest(issue_number: usize) -> (String, String) {
    let items: String = LINK_ITEMS
        .iter()
        .map(|(title, url, blurb)| {
            format!(r#"<li><p><a href="{url}?utm_source=links&amp;utm_medium=email">{title}</a> — {blurb}</p></li>"#)
        })
        .collect();
    let html = format!(
        r#"<!doctype html><html><body><div class="post"><p>Hi friends, issue {issue_number} is six links about local-first software, storage engines and one about bread, about four minutes of skimming in all.</p><ul>{items}</ul><p>That's it for this week.</p></div>{}</body></html>"#,
        footer("Local-first Links", "links.demo.mxr.local")
    );
    let text = LINK_ITEMS
        .iter()
        .map(|(title, url, blurb)| format!("{title}\n{url}\n{blurb}"))
        .collect::<Vec<_>>()
        .join("\n\n");
    (html, text)
}

fn teaser(title: &str, url: &str) -> (String, String) {
    let lede = "We rebuilt replication on top of one SQLite file per device and stopped losing writes. Here is what changed, what broke on the way, and the one test that caught it.";
    let html = format!(
        r#"<!doctype html><html><body><table width="600"><tr><td><h1>{title}</h1><p>{lede}</p><p><a href="{url}?utm_source=newsletter">Read more →</a></p>{}</td></tr></table></body></html>"#,
        footer("Platform Weekly", "platform.demo.mxr.local")
    );
    (html, format!("{title}\n\n{lede}\n\nRead more: {url}"))
}

fn growth(issue_number: usize) -> (String, String) {
    let html = format!(
        r#"<!doctype html><html><body><table><tr><td><p><a href="https://growth.demo.mxr.local/p/{issue_number}">Read Online</a></p><p>Good morning, growth team. Issue {issue_number} is about subject lines, send times and the metrics that make a list look healthier than it is.</p><p><b><a href="https://growth.demo.mxr.local/r/{issue_number}/1">Five subject lines that doubled opens</a></b>: plain questions beat clever puns.</p><p><b><a href="https://growth.demo.mxr.local/r/{issue_number}/2">The best hour to send</a></b>: it depends, and here is how to find out.</p><p><b><a href="https://growth.demo.mxr.local/r/{issue_number}/3">Vanity metrics to stop reporting</a></b>: open rate is the first to go.</p></td></tr></table>{}</body></html>"#,
        footer("Growth Digest", "growth.demo.mxr.local")
    );
    (html, format!("Growth Digest, issue {issue_number}"))
}

/// Numbered from `first_num` so provider ids follow the seeded messages
/// before them.
pub(super) fn reading_demo_messages(
    account_id: &AccountId,
    self_addr: &Address,
    now: DateTime<Utc>,
    first_num: usize,
) -> Vec<(Envelope, MessageBody)> {
    let address = |name: &str, email: &str| Address {
        name: Some(name.to_string()),
        email: email.to_string(),
    };
    let long_reads = address("Long Reads Weekly", "essays@longreads.demo.mxr.local");
    let links = address("Local-first Links", "links@links.demo.mxr.local");
    let platform = address("Platform Weekly", "weekly@platform.demo.mxr.local");
    let growth_from = address("Growth Digest", "digest@growth.demo.mxr.local");

    let mut built = Vec::with_capacity(READING_DEMO_MESSAGE_COUNT);
    let mut push = |message: DemoMessage, name: String, html: String| {
        let (envelope, mut body) = build_demo_msg(
            first_num + built.len(),
            account_id,
            &thread(account_id, &name),
            message,
        );
        body.text_html = Some(html);
        built.push((envelope, body));
    };

    // Newest this morning, then about weekly (a median gap of seven days,
    // so a fourteen-day window): last week's is in the edition and the
    // third is in its last day.
    for (i, title) in LONG_READS_TITLES.iter().enumerate() {
        let age = Duration::hours(LONG_READS_AGE_HOURS[i]);
        let paragraphs = essay_paragraphs(title);
        let html = essay_html(title, &paragraphs);
        push(
            issue(&long_reads, self_addr, title, paragraphs.join("\n\n"), now - age),
            format!("longreads-{i}"),
            html,
        );
    }
    for i in 0..LINKS {
        let (html, text) = digest(41 - i);
        push(
            issue(
                &links,
                self_addr,
                &format!("Local-first Links #{}: six things worth your time", 41 - i),
                text,
                now - Duration::hours(10) - Duration::days(7 * i as i64),
            ),
            format!("links-{i}"),
            html,
        );
    }
    let platform_titles = [
        "New post: Shipping a sync engine in 2026",
        "New post: The test that caught our replication bug",
        "New post: One SQLite file per device",
    ];
    for (i, title) in platform_titles.iter().enumerate() {
        let url = if i == 0 {
            DEMO_ARTICLE_SYNC.to_string()
        } else {
            format!("https://platform.demo.mxr.local/2026/{i}")
        };
        let (html, text) = teaser(title.trim_start_matches("New post: "), &url);
        push(
            issue(&platform, self_addr, title, text, now - Duration::days(3 + 7 * i as i64)),
            format!("platform-{i}"),
            html,
        );
    }
    // Weekly and never opened; the newest is thirteen days and a bit old.
    for i in 0..GROWTH {
        let (html, text) = growth(88 - i);
        push(
            issue(
                &growth_from,
                self_addr,
                &format!("Growth Digest #{}: 5 ways to double your open rate", 88 - i),
                text,
                now - Duration::days(13) - Duration::hours(8) - Duration::days(7 * i as i64),
            ),
            format!("growth-{i}"),
            html,
        );
    }
    built
}

/// The article behind a demo link, as its site would serve it.
pub fn demo_article_html(url: &str) -> Option<&'static str> {
    let path = url.split(['?', '#']).next().unwrap_or(url);
    match path {
        DEMO_ARTICLE_SYNC => Some(SYNC_ARTICLE),
        DEMO_ARTICLE_LOCAL_FIRST => Some(LOCAL_FIRST_ARTICLE),
        DEMO_ARTICLE_WAL => Some(WAL_ARTICLE),
        _ => None,
    }
}

const SYNC_ARTICLE: &str = r#"<!doctype html><html><head><title>Shipping a sync engine in 2026 | Platform Weekly</title><meta property="og:site_name" content="Platform Weekly"><meta name="author" content="Sam Okafor"></head><body><nav><a href="/">Home</a> <a href="/archive">Archive</a></nav><article><h1>Shipping a sync engine in 2026</h1>
<p>We rebuilt replication on top of one SQLite file per device and stopped losing writes. This is the long version of what changed, what broke on the way, and the one test that caught it before our users did.</p>
<p>The first decision was the easiest to make and the hardest to live with: every device keeps its own file and its own clock, and nothing is ever merged in place. A change log records every write with the device that made it and the clock reading at the time, and a merge is just reading two logs in order.</p>
<h2>Deletes</h2>
<p>A delete is the absence of a row, and an absence does not travel on its own. We keep tombstones for three weeks, which is longer than our slowest device stays offline, and a device that has been away longer than that resyncs from scratch instead of guessing.</p>
<p>The cost is real. Every query filters out the dead rows and every index carries them, so we compact once a day and only past the window, which keeps the resurrection bug from coming back.</p>
<h2>The test that caught it</h2>
<p>The bug that would have shipped was a write made offline on a tablet, deleted on a phone, and then edited again on the tablet before it came back online. Our merge kept the edit and dropped the delete. A property test that shuffles three devices' logs found it in under a minute, and it has found two more since.</p>
<p>If you are about to build something like this, start with the slowest device you own and the shuffle test. Everything else follows from those two.</p>
</article><footer>© 2026 Platform Weekly · <a href="/privacy">Privacy</a></footer></body></html>"#;

const LOCAL_FIRST_ARTICLE: &str = r#"<!doctype html><html><head><title>Local-first mail is having a moment</title><meta property="og:site_name" content="Local-first Links"></head><body><header><a href="/">Local-first Links</a></header><main><article><h1>Local-first mail is having a moment</h1>
<p>Three new mail clients shipped this year with the same idea at their core: your whole mailbox lives in a database on your own disk, and the server is a place mail arrives, not the place it lives.</p>
<p>The benefits are the ones local-first software always promised. Search is instant because it never leaves the machine. Nothing breaks on a plane. And the client can remember things about you, like which newsletters you actually finish, without sending that memory anywhere.</p>
<p>The costs are real too. A first sync of a large mailbox takes a while, every device holds its own copy, and deleting mail now means deleting it in two places. The clients that handle this well are honest about it: they show progress, they sort the newest mail first, and they say plainly what stays on the machine.</p>
<p>What changed is not the technology, which has been ready for years, but the expectations. People who grew up on cloud mail now want a client that works offline and keeps their history private, and a database file turns out to be a good way to give them both.</p>
</article></main><footer><a href="/about">About</a></footer></body></html>"#;

const WAL_ARTICLE: &str = r#"<!doctype html><html><head><title>WAL checkpoints, explained with pictures</title></head><body><article><h1>WAL checkpoints, explained with pictures</h1>
<p>SQLite in write-ahead logging mode appends every change to a separate file and copies it back into the database later. That copy is a checkpoint, and there are four kinds, each with a different idea of how patient it should be with your readers and writers.</p>
<p>A passive checkpoint copies whatever it can without waiting for anyone, and stops when it reaches a page a reader still needs. It never blocks, which makes it the right default, but on a busy database it may never finish.</p>
<p>A full checkpoint waits for writers to finish and then copies everything, while readers carry on. A restart checkpoint does the same and then waits for readers too, so the next writer can start the log from the beginning. A truncate checkpoint goes one step further and shrinks the log file to nothing.</p>
<p>Most applications never need more than the automatic passive checkpoint. If your log keeps growing, the cause is almost always a reader that never finishes, and no checkpoint mode will fix that for you.</p>
</article></body></html>"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_demo_article_is_served_and_nothing_else_is() {
        for url in [DEMO_ARTICLE_SYNC, DEMO_ARTICLE_LOCAL_FIRST, DEMO_ARTICLE_WAL] {
            assert!(demo_article_html(url).is_some(), "{url}");
            assert!(demo_article_html(&format!("{url}?utm_source=x")).is_some());
        }
        assert!(demo_article_html("https://example.com/").is_none());
    }

    #[test]
    fn the_reading_demo_has_every_message_it_counts() {
        let account = AccountId::from_provider_id("fake", "alex@demo.mxr.local");
        let me = Address {
            name: None,
            email: "alex@demo.mxr.local".to_string(),
        };
        let messages = reading_demo_messages(&account, &me, Utc::now(), 100);
        assert_eq!(messages.len(), READING_DEMO_MESSAGE_COUNT);
        assert!(messages.iter().all(|(_, body)| body.text_html.is_some()));
    }
}
