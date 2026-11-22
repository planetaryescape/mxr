//! Hostile HTML against the real helper: the `mxr` binary in `__html-worker`
//! mode, driven through the production call sites with the same timeout and
//! memory limits the daemon uses.
//!
//! The documents are the ones the F1 review rounds found, plus the cases the
//! reader's budget does not catch at all. A document may be rendered, or the
//! helper may be stopped and the caller gets its fallback; what must hold is
//! that the call returns promptly, the parent's memory stays small, the
//! visible text survives, and the next ordinary message renders.

use std::fmt::Write as _;
use std::sync::Arc;
use std::time::{Duration, Instant};

use mxr_html_worker::{with_supervisor, Config, Supervisor};

/// Shorter than the daemon's 5 s so the whole table of cases stays quick; the
/// mechanism is the same.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(3);

/// The visible words each hostile document carries, so a fallback can be
/// checked for keeping the message's text.
const MARKER: &str = "keepmewords";

fn helper() -> Arc<Supervisor> {
    let mut config = Config::new(
        env!("CARGO_BIN_EXE_mxr"),
        vec![mxr::html_helper::HELPER_ARG.into()],
    );
    config.request_timeout = REQUEST_TIMEOUT;
    config.startup_timeout = Duration::from_secs(30);
    Arc::new(Supervisor::new(config))
}

fn resident_bytes() -> u64 {
    let output = std::process::Command::new("ps")
        .args(["-o", "rss=", "-p", &std::process::id().to_string()])
        .output()
        .expect("ps runs");
    String::from_utf8_lossy(&output.stdout)
        .trim()
        .parse::<u64>()
        .expect("ps prints a number")
        * 1024
}

fn formatting_element_reconstruction() -> String {
    formatting_element_reconstruction_of(6_000)
}

/// `elements` distinct formatting elements, each rebuilt after each of
/// `elements` paragraph ends: about `elements` squared DOM nodes.
fn formatting_element_reconstruction_of(elements: usize) -> String {
    let mut html = format!("<p>{MARKER}");
    for n in 0..elements {
        write!(html, "<b id=\"f{n}\">").expect("write");
    }
    for _ in 0..elements {
        html.push_str("</p><p>x");
    }
    html
}

/// The scan ends the tag at the quoted `>`, so it sees one attribute while the
/// tokenizer sees a hundred thousand.
fn quoted_gt_attribute_bypass() -> String {
    let mut html = format!("<p>{MARKER}</p><div a=\">\"");
    for n in 0..100_000 {
        write!(html, " b{n}").expect("write");
    }
    html.push_str(">tail</div>");
    html
}

fn fifty_thousand_rowspans() -> String {
    format!(
        "<p>{MARKER}</p><table><tr>{}</tr></table>",
        "<td rowspan=100>x</td>".repeat(50_000)
    )
}

fn hundred_thousand_deep_divs() -> String {
    format!(
        "<p>{MARKER}</p>{}deep{}",
        "<div>".repeat(100_000),
        "</div>".repeat(100_000)
    )
}

/// What the helper should have had to do for a document.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Outcome {
    /// Past the byte scan and too expensive to parse: the helper must be
    /// stopped for it, by the memory ceiling or the timeout depending on the
    /// build profile. Only a document that needs several times the ceiling
    /// qualifies; one whose cost is mostly time (a hundred thousand attributes
    /// takes 1.7 s in a release build and minutes in a debug one) is `Either`.
    Stopped,
    /// The reader's budget refuses it inside the helper, so nothing is lost.
    Budgeted,
    /// Either: it may finish inside the timeout or be stopped, depending on
    /// how fast the machine is.
    Either,
}

fn assert_survives(name: &str, html: &str, expected: Outcome) {
    let helper = helper();
    let before = resident_bytes();
    let started = Instant::now();

    let text = with_supervisor(Arc::clone(&helper), || {
        mxr_reader::html_to_plain_text(html, "test")
    });
    let elapsed = started.elapsed();

    assert!(
        elapsed < REQUEST_TIMEOUT * 2 + Duration::from_secs(30),
        "{name}: took {elapsed:?}"
    );
    assert!(
        text.contains(MARKER),
        "{name}: the message's own text was lost: {:?}",
        text.chars().take(120).collect::<String>()
    );
    assert!(
        resident_bytes() < before + 200 * 1024 * 1024,
        "{name}: this process grew from {before} to {} bytes",
        resident_bytes()
    );

    let next = with_supervisor(Arc::clone(&helper), || {
        mxr_reader::html_to_plain_text("<p>second message renders</p>", "test")
    });
    assert_eq!(next.trim(), "second message renders", "{name}");
    let stats = helper.stats();
    assert_eq!(stats.job_errors, 0, "{name}: {stats:?}");
    let lost = stats.timeouts + stats.deaths;
    match expected {
        Outcome::Stopped => {
            assert!(
                lost >= 1,
                "{name}: expected the helper to be stopped: {stats:?}"
            );
            assert!(stats.fallbacks >= 1, "{name}: {stats:?}");
            assert!(
                stats.restarts >= 1,
                "{name}: the next request started a new helper: {stats:?}"
            );
        }
        Outcome::Budgeted => {
            assert_eq!(
                lost, 0,
                "{name}: the budget should refuse it cheaply: {stats:?}"
            );
            assert_eq!(stats.fallbacks, 0, "{name}: {stats:?}");
        }
        Outcome::Either => {}
    }
}

#[test]
fn formatting_element_reconstruction_is_contained() {
    assert_survives(
        "formatting reconstruction",
        &formatting_element_reconstruction(),
        Outcome::Stopped,
    );
}

#[test]
fn a_quoted_gt_attribute_bypass_is_contained() {
    assert_survives(
        "quoted > bypass",
        &quoted_gt_attribute_bypass(),
        Outcome::Either,
    );
}

#[test]
fn fifty_thousand_rowspans_are_contained() {
    assert_survives(
        "50k rowspans",
        &fifty_thousand_rowspans(),
        Outcome::Budgeted,
    );
}

#[test]
fn a_hundred_thousand_deep_div_nest_is_contained() {
    assert_survives(
        "100k deep divs",
        &hundred_thousand_deep_divs(),
        Outcome::Budgeted,
    );
}

#[test]
fn the_reader_pipeline_gives_the_same_content_through_the_helper() {
    let html = "<div>Reply text</div><div class=\"gmail_quote\">On Mon, Sam wrote:\
        <blockquote>old words</blockquote></div><div class=\"gmail_signature\">Sam</div>";
    let config = mxr_reader::ReaderConfig::default();
    let in_process = mxr_reader::clean(None, Some(html), &config);
    let through_helper = with_supervisor(helper(), || mxr_reader::clean(None, Some(html), &config));
    assert_eq!(through_helper.content, in_process.content);
    assert_eq!(
        through_helper.quoted_messages.len(),
        in_process.quoted_messages.len()
    );
    assert_eq!(through_helper.signature, in_process.signature);
    assert!(in_process.content.contains("Reply text"));
    assert!(!in_process.content.contains("old words"));
}

#[test]
fn a_poison_message_is_answered_from_memory_the_second_time() {
    let helper = helper();
    let html = formatting_element_reconstruction();
    let first = Instant::now();
    let one = with_supervisor(Arc::clone(&helper), || {
        mxr_reader::html_to_plain_text(&html, "test")
    });
    let first = first.elapsed();
    let before = helper.stats();
    let second = Instant::now();
    let two = with_supervisor(Arc::clone(&helper), || {
        mxr_reader::html_to_plain_text(&html, "test")
    });
    let second = second.elapsed();
    let after = helper.stats();
    assert_eq!(one, two);
    assert!(before.timeouts + before.deaths >= 1, "{before:?}");
    assert_eq!(after.timeouts, before.timeouts);
    assert_eq!(after.deaths, before.deaths);
    assert_eq!(after.fallbacks, before.fallbacks + 1, "{after:?}");
    assert!(
        second < Duration::from_secs(2),
        "the remembered input skips the helper: second call took {second:?} after {first:?}"
    );
}

#[test]
fn parallel_reads_complete_while_a_hostile_message_is_stopped_in_the_batch() {
    let helper = helper();
    let hostile = Arc::new(formatting_element_reconstruction());
    let threads: Vec<_> = (0..6)
        .map(|thread| {
            let helper = Arc::clone(&helper);
            let hostile = Arc::clone(&hostile);
            std::thread::spawn(move || {
                with_supervisor(helper, || {
                    let mut rendered = 0;
                    for message in 0..10 {
                        let html = format!("<p>thread {thread} message {message}</p>");
                        if thread == 0 && message == 3 {
                            let text = mxr_reader::html_to_plain_text(&hostile, "test");
                            assert!(text.contains(MARKER));
                            continue;
                        }
                        let text = mxr_reader::html_to_plain_text(&html, "test");
                        assert_eq!(text.trim(), format!("thread {thread} message {message}"));
                        rendered += 1;
                    }
                    rendered
                })
            })
        })
        .collect();
    let rendered: usize = threads
        .into_iter()
        .map(|thread| thread.join().expect("reader thread"))
        .sum();
    assert_eq!(rendered, 59, "every ordinary message rendered");
    let stats = helper.stats();
    assert!(stats.timeouts + stats.deaths >= 1, "{stats:?}");
}

#[test]
fn every_isolated_parse_runs_in_the_helper_binary() {
    let helper = helper();
    let ok = |kind: &str, html: &str, params: &str| {
        let outcome = helper.run(kind, html.as_bytes(), params.as_bytes(), 2 * 1024 * 1024);
        assert!(
            matches!(outcome, mxr_html_worker::Outcome::Done(_)),
            "{kind}: {outcome:?}"
        );
    };
    ok("reader.render", "<p>x</p>", "null");
    ok(
        "reader.split_quote",
        "<div class=\"gmail_quote\">q</div>",
        "false",
    );
    ok("reading.walk_html", "<p>x</p>", "null");
    ok("reading.article", "<p>x</p>", "null");
    ok("reading.issue_reader_html", "<p>x</p>", "10");
    ok("safety.html_views", "<p title=a>x</p>", "null");
    ok("outbound.validate_html", "<p>x</p>", "null");
    ok("outbound.text_alternative", "<p>x</p>", "null");
    ok("tui.rich_lines", "<p>x</p>", "60");
    ok("daemon.cid_references", "<img src=\"cid:a\">", "null");
    ok("daemon.image_sources", "<img src=\"a.png\">", "null");
    assert_eq!(helper.stats().fallbacks, 0);
}
